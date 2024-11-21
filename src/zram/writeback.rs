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

use crate::os::Ops;
use crate::zram::idle::calculate_idle_time;
use crate::zram::idle::set_zram_idle_time;

const ZRAM_WRITEBACK_PATH: &str = "/sys/block/zram0/writeback";

/// Error from [ZramWriteback].
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// writeback too frequently
    #[error("writeback too frequently")]
    BackoffTime,
    /// failure on setting zram idle
    #[error("calculate zram idle {0}")]
    CalculateIdle(#[from] crate::zram::idle::CalculateError),
    /// failure on setting zram idle
    #[error("set zram idle {0}")]
    MarkIdle(std::io::Error),
    /// failure on writing to /sys/block/zram0/writeback
    #[error("writeback: {0}")]
    Writeback(std::io::Error),
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
        }
    }
}

enum Mode {
    HugeIdle,
    Idle,
    Huge,
}

/// ZramWriteback manages zram writeback policies.
pub struct ZramWriteback {
    last_writeback_at: Option<Instant>,
}

impl ZramWriteback {
    /// Creates a new [ZramWriteback].
    pub fn new() -> Self {
        Self { last_writeback_at: None }
    }

    /// Writes back idle or huge zram pages to disk.
    pub fn mark_and_flush_pages<O: Ops>(&mut self, params: &Params, now: Instant) -> Result<()> {
        if let Some(last_at) = self.last_writeback_at {
            if now - last_at < params.backoff_duration {
                return Err(Error::BackoffTime);
            }
        }

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

// TODO: remove this when we add parameters to [ZramWriteback::new].
// This is just to suppress clippy::new_without_default.
impl Default for ZramWriteback {
    fn default() -> Self {
        Self::new()
    }
}
