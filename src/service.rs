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

use binder::Interface;
use binder::Result as BinderResult;
use log::error;
use mmd_aidl_interface::aidl::android::os::IMmd::IMmd;

use mmd::os::OpsImpl;
use mmd::zram::writeback::Error as ZramWritebackError;
use mmd::zram::writeback::ZramWriteback;

pub struct MmdService {
    zram_writeback: Mutex<ZramWriteback>,
}

impl MmdService {
    pub fn new() -> Self {
        Self { zram_writeback: Mutex::new(ZramWriteback::new()) }
    }
}

impl Interface for MmdService {}

impl IMmd for MmdService {
    fn doZramMaintenance(&self) -> BinderResult<()> {
        let mut zram_writeback = self.zram_writeback.lock().expect("mmd aborts on panics");
        let params = load_zram_writeback_params();
        match zram_writeback.mark_and_flush_pages::<OpsImpl>(&params, Instant::now()) {
            Ok(_) | Err(ZramWritebackError::BackoffTime) => {}
            Err(e) => error!("failed to zram writeback: {e:?}"),
        }
        Ok(())
    }
}

fn load_zram_writeback_params() -> mmd::zram::writeback::Params {
    // TODO: load params from system properties.
    mmd::zram::writeback::Params::default()
}
