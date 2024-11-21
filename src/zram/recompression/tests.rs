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

use super::*;
use crate::os::MockOps;
use crate::test_helper::path_params;
use crate::test_helper::write_params;
use crate::test_helper::write_path_params;
use crate::test_helper::OPS_MTX;
use crate::zram::idle::MEMINFO_PATH;
use crate::zram::idle::ZRAM_IDLE_PATH;
use mockall::Sequence;

fn setup_default_meminfo(rmock: &crate::os::__mock_MockOps_Ops::__read_to_string::Context) {
    let meminfo = "MemTotal: 8144296 kB
        MemAvailable: 346452 kB";
    rmock.expect().withf(path_params(MEMINFO_PATH)).returning(|_| Ok(meminfo.to_string()));
}

#[test]
fn test_is_zram_recompression_activated() {
    let _m = OPS_MTX.lock();
    let rmock = MockOps::read_to_string_context();
    rmock
        .expect()
        .withf(path_params(ZRAM_RECOMP_ALGORITHM_PATH))
        .returning(|_| Ok("#1: lzo lzo-rle lz4 [zstd]".to_string()));

    assert!(is_zram_recompression_activated::<MockOps>().unwrap());
}

#[test]
fn test_is_zram_recompression_activated_not_activated() {
    let _m = OPS_MTX.lock();
    let rmock = MockOps::read_to_string_context();
    rmock.expect().withf(path_params(ZRAM_RECOMP_ALGORITHM_PATH)).returning(|_| Ok("".to_string()));

    assert!(!is_zram_recompression_activated::<MockOps>().unwrap());
}

#[test]
fn test_is_zram_recompression_activated_not_supported() {
    let _m = OPS_MTX.lock();
    let rmock = MockOps::read_to_string_context();
    rmock
        .expect()
        .withf(path_params(ZRAM_RECOMP_ALGORITHM_PATH))
        .returning(|_| Err(std::io::Error::new(std::io::ErrorKind::NotFound, "not found")));

    assert!(!is_zram_recompression_activated::<MockOps>().unwrap());
}

#[test]
fn test_is_zram_recompression_activated_failure() {
    let _m = OPS_MTX.lock();
    let rmock = MockOps::read_to_string_context();
    rmock
        .expect()
        .withf(path_params(ZRAM_RECOMP_ALGORITHM_PATH))
        .returning(|_| Err(std::io::Error::other("error")));

    assert!(is_zram_recompression_activated::<MockOps>().is_err());
}

#[test]
fn mark_and_recompress() {
    let _m = OPS_MTX.lock();
    let mut seq = Sequence::new();
    let wmock = MockOps::write_context();
    let rmock = MockOps::read_to_string_context();
    setup_default_meminfo(&rmock);
    let params = Params { max_mib: 0, ..Default::default() };
    let mut zram_recompression = ZramRecompression::new();

    wmock
        .expect()
        .withf(write_path_params(ZRAM_IDLE_PATH))
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_, _| Ok(()));
    wmock
        .expect()
        .withf(write_params(ZRAM_RECOMPRESS_PATH, "type=huge_idle"))
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_, _| Ok(()));
    wmock
        .expect()
        .withf(write_path_params(ZRAM_IDLE_PATH))
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_, _| Ok(()));
    wmock
        .expect()
        .withf(write_params(ZRAM_RECOMPRESS_PATH, "type=idle"))
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_, _| Ok(()));
    wmock
        .expect()
        .withf(write_params(ZRAM_RECOMPRESS_PATH, "type=huge"))
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_, _| Ok(()));

    assert!(zram_recompression.mark_and_recompress::<MockOps>(&params, Instant::now()).is_ok());
}

#[test]
fn mark_and_recompress_with_threshold() {
    let _m = OPS_MTX.lock();
    let wmock = MockOps::write_context();
    wmock.expect().withf(write_path_params(ZRAM_IDLE_PATH)).returning(|_, _| Ok(()));
    let rmock = MockOps::read_to_string_context();
    setup_default_meminfo(&rmock);
    let params = Params { max_mib: 12345, ..Default::default() };
    let mut zram_recompression = ZramRecompression::new();

    wmock
        .expect()
        .withf(write_params(ZRAM_RECOMPRESS_PATH, "type=huge_idle threshold=12345"))
        .times(1)
        .returning(|_, _| Ok(()));
    wmock
        .expect()
        .withf(write_params(ZRAM_RECOMPRESS_PATH, "type=idle threshold=12345"))
        .times(1)
        .returning(|_, _| Ok(()));
    wmock
        .expect()
        .withf(write_params(ZRAM_RECOMPRESS_PATH, "type=huge threshold=12345"))
        .times(1)
        .returning(|_, _| Ok(()));

    assert!(zram_recompression.mark_and_recompress::<MockOps>(&params, Instant::now()).is_ok());
}

#[test]
fn mark_and_recompress_before_backoff() {
    let _m = OPS_MTX.lock();
    let wmock = MockOps::write_context();
    wmock.expect().returning(|_, _| Ok(()));
    let rmock = MockOps::read_to_string_context();
    setup_default_meminfo(&rmock);
    let params =
        Params { backoff_duration: Duration::from_secs(100), max_mib: 0, ..Default::default() };
    let base_time = Instant::now();
    let mut zram_recompression = ZramRecompression::new();
    assert!(zram_recompression.mark_and_recompress::<MockOps>(&params, base_time).is_ok());
    wmock.checkpoint();

    wmock.expect().times(0);

    assert!(matches!(
        zram_recompression
            .mark_and_recompress::<MockOps>(&params, base_time + Duration::from_secs(99)),
        Err(Error::BackoffTime)
    ));
}

#[test]
fn mark_and_recompress_after_backoff() {
    let _m = OPS_MTX.lock();
    let wmock = MockOps::write_context();
    wmock.expect().returning(|_, _| Ok(()));
    let rmock = MockOps::read_to_string_context();
    setup_default_meminfo(&rmock);
    let params =
        Params { backoff_duration: Duration::from_secs(100), max_mib: 0, ..Default::default() };
    let base_time = Instant::now();
    let mut zram_recompression = ZramRecompression::new();
    assert!(zram_recompression.mark_and_recompress::<MockOps>(&params, base_time).is_ok());
    wmock.checkpoint();
    wmock.expect().withf(write_path_params(ZRAM_IDLE_PATH)).returning(|_, _| Ok(()));
    wmock.expect().withf(write_path_params(ZRAM_RECOMPRESS_PATH)).times(3).returning(|_, _| Ok(()));

    assert!(zram_recompression
        .mark_and_recompress::<MockOps>(&params, base_time + Duration::from_secs(100))
        .is_ok());
}

#[test]
fn mark_and_recompress_idle_time() {
    let _m = OPS_MTX.lock();
    let wmock = MockOps::write_context();
    wmock.expect().withf(write_path_params(ZRAM_RECOMPRESS_PATH)).returning(|_, _| Ok(()));
    let rmock = MockOps::read_to_string_context();
    let meminfo = "MemTotal: 10000 kB
        MemAvailable: 8000 kB";
    rmock.expect().withf(path_params(MEMINFO_PATH)).returning(|_| Ok(meminfo.to_string()));
    let params = Params {
        min_idle: Duration::from_secs(3600),
        max_idle: Duration::from_secs(4000),
        max_mib: 0,
        ..Default::default()
    };
    let mut zram_recompression = ZramRecompression::new();

    wmock.expect().withf(write_params(ZRAM_IDLE_PATH, "3747")).times(2).returning(|_, _| Ok(()));

    assert!(zram_recompression.mark_and_recompress::<MockOps>(&params, Instant::now()).is_ok());
}

#[test]
fn mark_and_recompress_calculate_idle_failure() {
    let _m = OPS_MTX.lock();
    let wmock = MockOps::write_context();
    wmock.expect().withf(write_path_params(ZRAM_RECOMPRESS_PATH)).returning(|_, _| Ok(()));
    let params = Params {
        min_idle: Duration::from_secs(4000),
        max_idle: Duration::from_secs(3600),
        max_mib: 0,
        ..Default::default()
    };
    let mut zram_recompression = ZramRecompression::new();

    assert!(matches!(
        zram_recompression.mark_and_recompress::<MockOps>(&params, Instant::now()),
        Err(Error::CalculateIdle(_))
    ));
}

#[test]
fn mark_and_recompress_mark_idle_failure() {
    let _m = OPS_MTX.lock();
    let wmock = MockOps::write_context();
    let rmock = MockOps::read_to_string_context();
    setup_default_meminfo(&rmock);
    let params = Params { max_mib: 0, ..Default::default() };
    let mut zram_recompression = ZramRecompression::new();

    wmock
        .expect()
        .withf(write_path_params(ZRAM_IDLE_PATH))
        .returning(|_, _| Err(std::io::Error::other("error")));

    assert!(matches!(
        zram_recompression.mark_and_recompress::<MockOps>(&params, Instant::now()),
        Err(Error::MarkIdle(_))
    ));
}

#[test]
fn mark_and_recompress_skip_huge_idle() {
    let _m = OPS_MTX.lock();
    let wmock = MockOps::write_context();
    wmock.expect().withf(write_path_params(ZRAM_IDLE_PATH)).returning(|_, _| Ok(()));
    let rmock = MockOps::read_to_string_context();
    setup_default_meminfo(&rmock);
    let params = Params { huge_idle: false, max_mib: 0, ..Default::default() };
    let mut zram_recompression = ZramRecompression::new();

    wmock
        .expect()
        .withf(write_params(ZRAM_RECOMPRESS_PATH, "type=huge_idle"))
        .times(0)
        .returning(|_, _| Ok(()));
    wmock
        .expect()
        .withf(write_params(ZRAM_RECOMPRESS_PATH, "type=idle"))
        .times(1)
        .returning(|_, _| Ok(()));
    wmock
        .expect()
        .withf(write_params(ZRAM_RECOMPRESS_PATH, "type=huge"))
        .times(1)
        .returning(|_, _| Ok(()));

    assert!(zram_recompression.mark_and_recompress::<MockOps>(&params, Instant::now()).is_ok());
}

#[test]
fn mark_and_recompress_skip_idle() {
    let _m = OPS_MTX.lock();
    let wmock = MockOps::write_context();
    wmock.expect().withf(write_path_params(ZRAM_IDLE_PATH)).returning(|_, _| Ok(()));
    let rmock = MockOps::read_to_string_context();
    setup_default_meminfo(&rmock);
    let params = Params { idle: false, max_mib: 0, ..Default::default() };
    let mut zram_recompression = ZramRecompression::new();

    wmock
        .expect()
        .withf(write_params(ZRAM_RECOMPRESS_PATH, "type=huge_idle"))
        .times(1)
        .returning(|_, _| Ok(()));
    wmock
        .expect()
        .withf(write_params(ZRAM_RECOMPRESS_PATH, "type=idle"))
        .times(0)
        .returning(|_, _| Ok(()));
    wmock
        .expect()
        .withf(write_params(ZRAM_RECOMPRESS_PATH, "type=huge"))
        .times(1)
        .returning(|_, _| Ok(()));

    assert!(zram_recompression.mark_and_recompress::<MockOps>(&params, Instant::now()).is_ok());
}

#[test]
fn mark_and_recompress_skip_huge() {
    let _m = OPS_MTX.lock();
    let wmock = MockOps::write_context();
    wmock.expect().withf(write_path_params(ZRAM_IDLE_PATH)).returning(|_, _| Ok(()));
    let rmock = MockOps::read_to_string_context();
    setup_default_meminfo(&rmock);
    let params = Params { huge: false, max_mib: 0, ..Default::default() };
    let mut zram_recompression = ZramRecompression::new();

    wmock
        .expect()
        .withf(write_params(ZRAM_RECOMPRESS_PATH, "type=huge_idle"))
        .times(1)
        .returning(|_, _| Ok(()));
    wmock
        .expect()
        .withf(write_params(ZRAM_RECOMPRESS_PATH, "type=idle"))
        .times(1)
        .returning(|_, _| Ok(()));
    wmock
        .expect()
        .withf(write_params(ZRAM_RECOMPRESS_PATH, "type=huge"))
        .times(0)
        .returning(|_, _| Ok(()));

    assert!(zram_recompression.mark_and_recompress::<MockOps>(&params, Instant::now()).is_ok());
}
