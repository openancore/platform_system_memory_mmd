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

//! This module provides policy to manage zram writeback feature.
//!
//! See "writeback" section in the kernel document for details.
//!
//! https://www.kernel.org/doc/Documentation/blockdev/zram.txt

#[cfg(test)]
mod tests;

use std::time::Duration;
use std::time::Instant;

use crate::os::get_page_size;
use crate::os::Ops;
use crate::zram::idle::calculate_idle_time;
use crate::zram::idle::set_zram_idle_time;

const ZRAM_WRITEBACK_PATH: &str = "/sys/block/zram0/writeback";
const ZRAM_WRITEBACK_LIMIT_PATH: &str = "/sys/block/zram0/writeback_limit";

/// Error from [ZramWriteback].
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// writeback too frequently
    #[error("writeback too frequently")]
    BackoffTime,
    /// no more space for zram writeback
    #[error("no pages in zram for zram writeback")]
    Limit,
    /// failure on setting zram idle
    #[error("calculate zram idle {0}")]
    CalculateIdle(#[from] crate::zram::idle::CalculateError),
    /// failure on setting zram idle
    #[error("set zram idle {0}")]
    MarkIdle(std::io::Error),
    /// failure on writing to /sys/block/zram0/writeback
    #[error("writeback: {0}")]
    Writeback(std::io::Error),
    /// failure on writing to /sys/block/zram0/writeback_limit
    #[error("writeback_limit: {0}")]
    WritebackLimit(std::io::Error),
}

type Result<T> = std::result::Result<T, Error>;

/// The parameters for zram writeback.
pub struct Params {
    /// The backoff time since last writeback.
    pub backoff_duration: Duration,
    /// The minimum idle duration to writeback. This is used for [calculate_idle_time].
    pub min_idle: Duration,
    /// The maximum idle duration to writeback. This is used for [calculate_idle_time].
    pub max_idle: Duration,
    /// Whether writeback huge and idle pages or not.
    pub huge_idle: bool,
    /// Whether writeback idle pages or not.
    pub idle: bool,
    /// Whether writeback huge pages or not.
    pub huge: bool,
    /// Minimum bytes to writeback in 1 round.
    pub min_bytes: u64,
    /// Maximum bytes to writeback in 1 round.
    pub max_bytes: u64,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            // 10 minutes
            backoff_duration: Duration::from_secs(600),
            // 20 hours
            min_idle: Duration::from_secs(20 * 3600),
            // 25 hours
            max_idle: Duration::from_secs(25 * 3600),
            huge_idle: true,
            idle: true,
            huge: true,
            // 5 MiB
            min_bytes: 5 << 20,
            // 300 MiB
            max_bytes: 300 << 20,
        }
    }
}

/// The stats for zram writeback.
#[derive(Debug, Default)]
pub struct Stats {
    /// orig_data_size of [crate::zram::stats::ZramMmStat].
    pub orig_data_size: u64,
    /// bd_count_pages of [crate::zram::stats::ZramBdStat].
    pub current_writeback_pages: u64,
}

enum Mode {
    HugeIdle,
    Idle,
    Huge,
}

/// ZramWriteback manages zram writeback policies.
pub struct ZramWriteback {
    last_writeback_at: Option<Instant>,
    total_zram_pages: u64,
    zram_writeback_pages: u64,
    page_size: u64,
}

impl ZramWriteback {
    /// Creates a new [ZramWriteback].
    pub fn new(total_zram_size: u64, zram_writeback_size: u64) -> Self {
        Self::new_with_page_size(total_zram_size, zram_writeback_size, get_page_size())
    }

    /// Creates a new [ZramWriteback] with a specified page size.
    pub fn new_with_page_size(
        total_zram_size: u64,
        zram_writeback_size: u64,
        page_size: u64,
    ) -> Self {
        assert!(page_size != 0);
        let total_zram_pages = total_zram_size / page_size;
        let zram_writeback_pages = zram_writeback_size / page_size;
        assert!(total_zram_pages != 0);
        Self { last_writeback_at: None, total_zram_pages, zram_writeback_pages, page_size }
    }

    /// Writes back idle or huge zram pages to disk.
    pub fn mark_and_flush_pages<O: Ops>(
        &mut self,
        params: &Params,
        stats: &Stats,
        now: Instant,
    ) -> Result<()> {
        if let Some(last_at) = self.last_writeback_at {
            if now - last_at < params.backoff_duration {
                return Err(Error::BackoffTime);
            }
        }

        let limit_pages = self.calculate_writeback_limit(params, stats);
        if limit_pages == 0 {
            return Err(Error::Limit);
        }
        O::write(ZRAM_WRITEBACK_LIMIT_PATH, limit_pages.to_string())
            .map_err(Error::WritebackLimit)?;

        if params.huge_idle {
            self.writeback::<O>(params, Mode::HugeIdle, now)?;
        }
        if params.idle {
            self.writeback::<O>(params, Mode::Idle, now)?;
        }
        if params.huge {
            self.writeback::<O>(params, Mode::Huge, now)?;
        }

        Ok(())
    }

    fn calculate_writeback_limit(&self, params: &Params, stats: &Stats) -> u64 {
        let min_pages = params.min_bytes / self.page_size;
        let max_pages = params.max_bytes / self.page_size;
        // All calculations are performed in basis points, 100 bps = 1.00%. The number of pages
        // allowed to be written back follows a simple linear relationship. The allowable range is
        // [min_pages, max_pages], and the writeback limit will be the (zram utilization) * the
        // range, that is, the more zram we're using the more we're going to allow to be written
        // back.
        const BPS: u64 = 100 * 100;
        let zram_utilization_bps =
            stats.orig_data_size / self.page_size * BPS / self.total_zram_pages;
        let limit_pages = zram_utilization_bps * max_pages / BPS;

        // And try to limit it to the approximate number of free backing device pages (if it's
        // less).
        let free_bd_pages = self.zram_writeback_pages - stats.current_writeback_pages;
        let limit_pages = std::cmp::min(limit_pages, free_bd_pages);

        if limit_pages < min_pages {
            // Configured to not writeback fewer than configured min_pages.
            return 0;
        }

        // Finally enforce the limits, we won't even attempt writeback if we cannot writeback at
        // least the min, and we will cap to the max if it's greater.
        std::cmp::min(limit_pages, max_pages)
    }

    fn writeback<O: Ops>(&mut self, params: &Params, mode: Mode, now: Instant) -> Result<()> {
        match mode {
            Mode::HugeIdle | Mode::Idle => {
                let idle_age = calculate_idle_time::<O>(params.min_idle, params.max_idle)?;
                // TODO: adjust the idle_age by suspend duration.
                set_zram_idle_time::<O>(idle_age).map_err(Error::MarkIdle)?;
            }
            Mode::Huge => {}
        }

        let mode = match mode {
            Mode::HugeIdle => "huge_idle",
            Mode::Idle => "idle",
            Mode::Huge => "huge",
        };

        O::write(ZRAM_WRITEBACK_PATH, mode).map_err(Error::Writeback)?;

        self.last_writeback_at = Some(now);

        Ok(())
    }
}
