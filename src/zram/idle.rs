// Copyright 2024, The Android Open Source Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::time::Duration;

use crate::os::Ops;

pub const ZRAM_IDLE_PATH: &str = "/sys/block/zram0/idle";
pub const MEMINFO_PATH: &str = "/proc/meminfo";

/// Sets idle duration in seconds to "/sys/block/zram0/idle".
///
/// Fractions of a second are truncated.
pub fn set_zram_idle_time<O: Ops>(idle_age: Duration) -> std::io::Result<()> {
    O::write(ZRAM_IDLE_PATH, idle_age.as_secs().to_string())
}

/// This parses the content of "/proc/meminfo" and returns the number of "MemTotal" and
/// "MemAvailable".
///
/// This does not care about the unit, because the user `calculate_idle_time()` use the values to
/// calculate memory utilization rate. The unit should be always "kB".
///
/// This returns `None` if this fails to parse the content.
fn parse_meminfo(content: &str) -> Option<(u64, u64)> {
    let mut total = None;
    let mut available = None;
    for line in content.split("\n") {
        let container = if line.contains("MemTotal:") {
            &mut total
        } else if line.contains("MemAvailable:") {
            &mut available
        } else {
            continue;
        };
        let Some(number_str) = line.split_whitespace().nth(1) else {
            continue;
        };
        let Ok(number) = number_str.parse::<u64>() else {
            continue;
        };
        *container = Some(number);
    }
    if let (Some(total), Some(available)) = (total, available) {
        Some((total, available))
    } else {
        None
    }
}

/// Error from [calculate_idle_time].
#[derive(Debug, thiserror::Error)]
pub enum CalculateError {
    #[error("min_idle is longer than max_idle")]
    InvalidMinAndMax,
    #[error("failed to parse meminfo")]
    InvalidMeminfo,
    #[error("failed to read meminfo: {0}")]
    ReadMeminfo(std::io::Error),
}

/// Calculates idle duration from min_idle and max_idle using meminfo.
pub fn calculate_idle_time<O: Ops>(
    min_idle: Duration,
    max_idle: Duration,
) -> std::result::Result<Duration, CalculateError> {
    if min_idle > max_idle {
        return Err(CalculateError::InvalidMinAndMax);
    }
    let content = match O::read_to_string(MEMINFO_PATH) {
        Ok(v) => v,
        Err(e) => return Err(CalculateError::ReadMeminfo(e)),
    };
    let (total, available) = match parse_meminfo(&content) {
        Some((total, available)) if total > 0 => (total, available),
        _ => {
            // Fallback to use the safest value.
            return Err(CalculateError::InvalidMeminfo);
        }
    };

    let mem_utilization = 1.0 - (available as f64) / (total as f64);

    // Exponentially decay the age vs. memory utilization. The reason we choose exponential decay is
    // because we want to do as little work as possible when the system is under very low memory
    // pressure. As pressure increases we want to start aggressively shrinking our idle age to force
    // newer pages to be written back/recompressed.
    const LAMBDA: f64 = 5.0;
    let seconds = ((max_idle - min_idle).as_secs() as f64)
        * std::f64::consts::E.powf(-LAMBDA * mem_utilization)
        + (min_idle.as_secs() as f64);

    Ok(Duration::from_secs(seconds as u64))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::os::MockOps;
    use crate::test_helper::path_params;
    use crate::test_helper::write_params;
    use crate::test_helper::OPS_MTX;

    #[test]
    fn test_set_zram_idle_time() {
        let _m = OPS_MTX.lock();
        let mock = MockOps::write_context();
        mock.expect().withf(write_params(ZRAM_IDLE_PATH, "3600")).returning(|_, _| Ok(()));

        assert!(set_zram_idle_time::<MockOps>(Duration::from_secs(3600)).is_ok());
    }

    #[test]
    fn test_set_zram_idle_time_in_seconds() {
        let _m = OPS_MTX.lock();
        let mock = MockOps::write_context();
        mock.expect().withf(write_params(ZRAM_IDLE_PATH, "3600")).returning(|_, _| Ok(()));

        assert!(set_zram_idle_time::<MockOps>(Duration::from_millis(3600567)).is_ok());
    }

    #[test]
    fn test_parse_meminfo() {
        let content = "MemTotal:       123456789 kB
MemFree:        12345 kB
MemAvailable:   67890 kB
    ";
        assert_eq!(parse_meminfo(content).unwrap(), (123456789, 67890));
    }

    #[test]
    fn test_parse_meminfo_invalid_format() {
        // empty
        assert!(parse_meminfo("").is_none());
        // no number
        let content = "MemTotal:
MemFree:        12345 kB
MemAvailable:   67890 kB
    ";
        assert!(parse_meminfo(content).is_none());
        // no number
        let content = "MemTotal:       kB
MemFree:        12345 kB
MemAvailable:   67890 kB
    ";
        assert!(parse_meminfo(content).is_none());
        // total memory missing
        let content = "MemFree:        12345 kB
MemAvailable:   67890 kB
    ";
        assert!(parse_meminfo(content).is_none());
        // available memory missing
        let content = "MemTotal:       123456789 kB
MemFree:        12345 kB
    ";
        assert!(parse_meminfo(content).is_none());
    }

    #[test]
    fn test_calculate_idle_time() {
        let _m = OPS_MTX.lock();
        let mock = MockOps::read_to_string_context();
        let meminfo = "MemTotal: 8144296 kB
    MemAvailable: 346452 kB";
        mock.expect().withf(path_params(MEMINFO_PATH)).returning(|_| Ok(meminfo.to_string()));

        assert_eq!(
            calculate_idle_time::<MockOps>(Duration::from_secs(72000), Duration::from_secs(90000))
                .unwrap(),
            Duration::from_secs(72150)
        );
    }

    #[test]
    fn test_calculate_idle_time_same_min_max() {
        let _m = OPS_MTX.lock();
        let mock = MockOps::read_to_string_context();
        let meminfo = "MemTotal: 8144296 kB
    MemAvailable: 346452 kB";
        mock.expect().withf(path_params(MEMINFO_PATH)).returning(|_| Ok(meminfo.to_string()));

        assert_eq!(
            calculate_idle_time::<MockOps>(Duration::from_secs(90000), Duration::from_secs(90000))
                .unwrap(),
            Duration::from_secs(90000)
        );
    }

    #[test]
    fn test_calculate_idle_time_min_is_bigger_than_max() {
        assert!(matches!(
            calculate_idle_time::<MockOps>(Duration::from_secs(90000), Duration::from_secs(72000)),
            Err(CalculateError::InvalidMinAndMax)
        ));
    }

    #[test]
    fn test_calculate_idle_time_no_available() {
        let _m = OPS_MTX.lock();
        let mock = MockOps::read_to_string_context();
        let meminfo = "MemTotal: 8144296 kB
    MemAvailable: 0 kB";
        mock.expect().withf(path_params(MEMINFO_PATH)).returning(|_| Ok(meminfo.to_string()));

        assert_eq!(
            calculate_idle_time::<MockOps>(Duration::from_secs(72000), Duration::from_secs(90000))
                .unwrap(),
            Duration::from_secs(72121)
        );
    }

    #[test]
    fn test_calculate_idle_time_meminfo_fail() {
        let _m = OPS_MTX.lock();
        let mock = MockOps::read_to_string_context();
        mock.expect()
            .withf(path_params(MEMINFO_PATH))
            .returning(|_| Err(std::io::Error::other("error")));

        assert!(matches!(
            calculate_idle_time::<MockOps>(Duration::from_secs(72000), Duration::from_secs(90000)),
            Err(CalculateError::ReadMeminfo(_))
        ));
    }

    #[test]
    fn test_calculate_idle_time_invalid_meminfo() {
        let _m = OPS_MTX.lock();
        let mock = MockOps::read_to_string_context();
        let meminfo = "";
        mock.expect().withf(path_params(MEMINFO_PATH)).returning(|_| Ok(meminfo.to_string()));

        assert!(matches!(
            calculate_idle_time::<MockOps>(Duration::from_secs(72000), Duration::from_secs(90000)),
            Err(CalculateError::InvalidMeminfo)
        ));
    }

    #[test]
    fn test_calculate_idle_time_zero_total_memory() {
        let _m = OPS_MTX.lock();
        let mock = MockOps::read_to_string_context();
        let meminfo = "MemTotal: 0 kB
    MemAvailable: 346452 kB";
        mock.expect().withf(path_params(MEMINFO_PATH)).returning(|_| Ok(meminfo.to_string()));

        assert!(matches!(
            calculate_idle_time::<MockOps>(Duration::from_secs(72000), Duration::from_secs(90000)),
            Err(CalculateError::InvalidMeminfo)
        ));
    }
}
