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

//! mmd is a native daemon managing memory task:
//!
//! * zram

mod service;

use std::time::Duration;

use binder::BinderFeatures;
use log::error;
use log::info;
use log::LevelFilter;
use mmd::os::OpsImpl;
use mmd::zram::stats::load_total_zram_size;
use mmd::zram::writeback::is_zram_writeback_activated;
use mmd::zram::writeback::ZramWriteback;
use mmd_aidl_interface::aidl::android::os::IMmd::BnMmd;

// In Android zram writeback file is always "/data/per_boot/zram_swap".
const ZRAM_WRITEBACK_FILE_PATH: &str = "/data/per_boot/zram_swap";

fn main() {
    let _init_success = logger::init(
        logger::Config::default().with_tag_on_device("mmd").with_max_level(LevelFilter::Trace),
    );
    if !mmd_flags::mmd_enabled() {
        info!("mmd is disabled");
        loop {
            std::thread::sleep(Duration::MAX);
        }
    }

    // TODO: This is to wait until swapon_all command sets up zram. Remove this when mmd sets up
    // zram.
    std::thread::sleep(Duration::from_secs(60));

    let total_zram_size = match load_total_zram_size::<OpsImpl>() {
        Ok(v) => v,
        Err(e) => {
            error!("failed to load total zram size: {e:?}");
            std::process::exit(1);
        }
    };
    let zram_writeback = match load_zram_writeback_disk_size() {
        Ok(Some(zram_writeback_disk_size)) => {
            info!("zram writeback is enabled");
            Some(ZramWriteback::new(total_zram_size, zram_writeback_disk_size))
        }
        Ok(None) => {
            info!("zram writeback is disabled");
            None
        }
        Err(e) => {
            error!("failed to load zram writeback file size: {e:?}");
            None
        }
    };

    let mmd_service = service::MmdService::new(zram_writeback);
    let mmd_service_binder = BnMmd::new_binder(mmd_service, BinderFeatures::default());
    binder::add_service("mmd", mmd_service_binder.as_binder()).expect("register service");

    info!("mmd started");

    binder::ProcessState::join_thread_pool();
}

/// Loads the zram writeback disk size.
///
/// If zram writeback is not enabled, this returns `Ok(None)`.
pub fn load_zram_writeback_disk_size() -> std::io::Result<Option<u64>> {
    if is_zram_writeback_activated::<OpsImpl>()? {
        Ok(Some(std::fs::metadata(ZRAM_WRITEBACK_FILE_PATH)?.len()))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
