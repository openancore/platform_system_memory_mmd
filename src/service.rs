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

use std::sync::Mutex;
use std::time::Instant;

use anyhow::Context;
use binder::Interface;
use binder::Result as BinderResult;
use log::error;
use mmd_aidl_interface::aidl::android::os::IMmd::IMmd;

use mmd::os::OpsImpl;
use mmd::zram::writeback::Error as ZramWritebackError;
use mmd::zram::writeback::ZramWriteback;

pub struct MmdService {
    zram_writeback: Mutex<Option<ZramWriteback>>,
}

impl MmdService {
    pub fn new(zram_writeback: Option<ZramWriteback>) -> Self {
        Self { zram_writeback: Mutex::new(zram_writeback) }
    }
}

impl Interface for MmdService {}

impl IMmd for MmdService {
    fn doZramMaintenance(&self) -> BinderResult<()> {
        let mut zram_writeback = self.zram_writeback.lock().expect("mmd aborts on panics");
        if let Some(zram_writeback) = zram_writeback.as_mut() {
            let params = load_zram_writeback_params();
            let stats = match load_zram_writeback_stats() {
                Ok(v) => v,
                Err(e) => {
                    error!("failed to load zram writeback stats: {e:?}");
                    return Ok(());
                }
            };
            match zram_writeback.mark_and_flush_pages::<OpsImpl>(&params, &stats, Instant::now()) {
                Ok(_) | Err(ZramWritebackError::BackoffTime) | Err(ZramWritebackError::Limit) => {}
                Err(e) => error!("failed to zram writeback: {e:?}"),
            }
        }
        Ok(())
    }
}

fn load_zram_writeback_params() -> mmd::zram::writeback::Params {
    // TODO: load params from system properties.
    mmd::zram::writeback::Params::default()
}

fn load_zram_writeback_stats() -> anyhow::Result<mmd::zram::writeback::Stats> {
    let mm_stat = mmd::zram::stats::ZramMmStat::load::<OpsImpl>().context("load mm_stat")?;
    let bd_stat = mmd::zram::stats::ZramBdStat::load::<OpsImpl>().context("load bd_stat")?;
    Ok(mmd::zram::writeback::Stats {
        orig_data_size: mm_stat.orig_data_size,
        current_writeback_pages: bd_stat.bd_count_pages,
    })
}
