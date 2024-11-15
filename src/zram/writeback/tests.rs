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

const DEFAULT_TOTAL_ZRAM_SIZE: u64 = 4 << 30;
const DEFAULT_ZRAM_WRITEBACK_SIZE: u64 = 1 << 30;
const DEFAULT_PAGE_SIZE: u64 = 4096;

fn setup_default_meminfo(rmock: &crate::os::__mock_MockOps_Ops::__read_to_string::Context) {
    let meminfo = "MemTotal: 8144296 kB
        MemAvailable: 346452 kB";
    rmock.expect().withf(path_params(MEMINFO_PATH)).returning(|_| Ok(meminfo.to_string()));
}

fn setup_default_writeback_limit_read(
    rmock: &crate::os::__mock_MockOps_Ops::__read_to_string::Context,
) {
    rmock
        .expect()
        .withf(path_params(ZRAM_WRITEBACK_LIMIT_PATH))
        .returning(|_| Ok("100\n".to_string()));
}

#[test]
fn mark_and_flush_pages() {
    let _m = OPS_MTX.lock();
    let mut seq = Sequence::new();
    let wmock = MockOps::write_context();
    let rmock = MockOps::read_to_string_context();
    setup_default_meminfo(&rmock);
    setup_default_writeback_limit_read(&rmock);
    let params = Params::default();
    let stats = Stats { orig_data_size: params.max_bytes, ..Default::default() };
    let mut zram_writeback =
        ZramWriteback::new(DEFAULT_TOTAL_ZRAM_SIZE, DEFAULT_ZRAM_WRITEBACK_SIZE);

    wmock
        .expect()
        .withf(write_path_params(ZRAM_WRITEBACK_LIMIT_PATH))
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
        .withf(write_params(ZRAM_WRITEBACK_PATH, "huge_idle"))
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
        .withf(write_params(ZRAM_WRITEBACK_PATH, "idle"))
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_, _| Ok(()));
    wmock
        .expect()
        .withf(write_params(ZRAM_WRITEBACK_PATH, "huge"))
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_, _| Ok(()));

    assert!(zram_writeback
        .mark_and_flush_pages::<MockOps>(&params, &stats, Instant::now())
        .is_ok());
}

#[test]
fn mark_and_flush_pages_before_backoff() {
    let _m = OPS_MTX.lock();
    let wmock = MockOps::write_context();
    wmock.expect().returning(|_, _| Ok(()));
    let rmock = MockOps::read_to_string_context();
    setup_default_meminfo(&rmock);
    setup_default_writeback_limit_read(&rmock);
    let params = Params { backoff_duration: Duration::from_secs(100), ..Default::default() };
    let stats = Stats { orig_data_size: params.max_bytes, ..Default::default() };
    let base_time = Instant::now();
    let mut zram_writeback =
        ZramWriteback::new(DEFAULT_TOTAL_ZRAM_SIZE, DEFAULT_ZRAM_WRITEBACK_SIZE);
    assert!(zram_writeback.mark_and_flush_pages::<MockOps>(&params, &stats, base_time).is_ok());
    wmock.checkpoint();

    wmock.expect().times(0);

    assert!(matches!(
        zram_writeback.mark_and_flush_pages::<MockOps>(
            &params,
            &stats,
            base_time + Duration::from_secs(99)
        ),
        Err(Error::BackoffTime)
    ));
}

#[test]
fn mark_and_flush_pages_after_backoff() {
    let _m = OPS_MTX.lock();
    let wmock = MockOps::write_context();
    wmock.expect().returning(|_, _| Ok(()));
    let rmock = MockOps::read_to_string_context();
    setup_default_meminfo(&rmock);
    setup_default_writeback_limit_read(&rmock);
    let params = Params { backoff_duration: Duration::from_secs(100), ..Default::default() };
    let stats = Stats { orig_data_size: params.max_bytes, ..Default::default() };
    let base_time = Instant::now();
    let mut zram_writeback =
        ZramWriteback::new(DEFAULT_TOTAL_ZRAM_SIZE, DEFAULT_ZRAM_WRITEBACK_SIZE);
    assert!(zram_writeback.mark_and_flush_pages::<MockOps>(&params, &stats, base_time).is_ok());
    wmock.checkpoint();
    wmock.expect().withf(write_path_params(ZRAM_WRITEBACK_LIMIT_PATH)).returning(|_, _| Ok(()));
    wmock.expect().withf(write_path_params(ZRAM_IDLE_PATH)).returning(|_, _| Ok(()));

    wmock.expect().withf(write_path_params(ZRAM_WRITEBACK_PATH)).times(3).returning(|_, _| Ok(()));

    assert!(zram_writeback
        .mark_and_flush_pages::<MockOps>(&params, &stats, base_time + Duration::from_secs(100))
        .is_ok());
}

#[test]
fn mark_and_flush_pages_idle_time() {
    let _m = OPS_MTX.lock();
    let wmock = MockOps::write_context();
    wmock.expect().withf(write_path_params(ZRAM_WRITEBACK_LIMIT_PATH)).returning(|_, _| Ok(()));
    wmock.expect().withf(write_path_params(ZRAM_WRITEBACK_PATH)).returning(|_, _| Ok(()));
    let rmock = MockOps::read_to_string_context();
    let meminfo = "MemTotal: 10000 kB
        MemAvailable: 8000 kB";
    rmock.expect().withf(path_params(MEMINFO_PATH)).returning(|_| Ok(meminfo.to_string()));
    setup_default_writeback_limit_read(&rmock);
    let params = Params {
        min_idle: Duration::from_secs(3600),
        max_idle: Duration::from_secs(4000),
        ..Default::default()
    };
    let stats = Stats { orig_data_size: params.max_bytes, ..Default::default() };
    let mut zram_writeback =
        ZramWriteback::new(DEFAULT_TOTAL_ZRAM_SIZE, DEFAULT_ZRAM_WRITEBACK_SIZE);

    wmock.expect().withf(write_params(ZRAM_IDLE_PATH, "3747")).times(2).returning(|_, _| Ok(()));

    assert!(zram_writeback
        .mark_and_flush_pages::<MockOps>(&params, &stats, Instant::now())
        .is_ok());
}

#[test]
fn mark_and_flush_pages_calculate_idle_failure() {
    let _m = OPS_MTX.lock();
    let wmock = MockOps::write_context();
    wmock.expect().withf(write_path_params(ZRAM_WRITEBACK_LIMIT_PATH)).returning(|_, _| Ok(()));
    wmock.expect().withf(write_path_params(ZRAM_WRITEBACK_PATH)).returning(|_, _| Ok(()));
    let rmock = MockOps::read_to_string_context();
    setup_default_writeback_limit_read(&rmock);
    let params = Params {
        min_idle: Duration::from_secs(4000),
        max_idle: Duration::from_secs(3600),
        ..Default::default()
    };
    let stats = Stats { orig_data_size: params.max_bytes, ..Default::default() };
    let mut zram_writeback =
        ZramWriteback::new(DEFAULT_TOTAL_ZRAM_SIZE, DEFAULT_ZRAM_WRITEBACK_SIZE);

    assert!(matches!(
        zram_writeback.mark_and_flush_pages::<MockOps>(&params, &stats, Instant::now()),
        Err(Error::CalculateIdle(_))
    ));
}

#[test]
fn mark_and_flush_pages_mark_idle_failure() {
    let _m = OPS_MTX.lock();
    let wmock = MockOps::write_context();
    wmock.expect().withf(write_path_params(ZRAM_WRITEBACK_LIMIT_PATH)).returning(|_, _| Ok(()));
    wmock.expect().withf(write_path_params(ZRAM_WRITEBACK_PATH)).returning(|_, _| Ok(()));
    let rmock = MockOps::read_to_string_context();
    setup_default_meminfo(&rmock);
    setup_default_writeback_limit_read(&rmock);
    let params = Params::default();
    let stats = Stats { orig_data_size: params.max_bytes, ..Default::default() };
    let mut zram_writeback =
        ZramWriteback::new(DEFAULT_TOTAL_ZRAM_SIZE, DEFAULT_ZRAM_WRITEBACK_SIZE);

    wmock
        .expect()
        .withf(write_path_params(ZRAM_IDLE_PATH))
        .returning(|_, _| Err(std::io::Error::other("error")));

    assert!(matches!(
        zram_writeback.mark_and_flush_pages::<MockOps>(&params, &stats, Instant::now()),
        Err(Error::MarkIdle(_))
    ));
}

#[test]
fn mark_and_flush_pages_skip_huge_idle() {
    let _m = OPS_MTX.lock();
    let wmock = MockOps::write_context();
    wmock.expect().withf(write_path_params(ZRAM_WRITEBACK_LIMIT_PATH)).returning(|_, _| Ok(()));
    wmock.expect().withf(write_path_params(ZRAM_IDLE_PATH)).returning(|_, _| Ok(()));
    let rmock = MockOps::read_to_string_context();
    setup_default_meminfo(&rmock);
    setup_default_writeback_limit_read(&rmock);
    let params = Params { huge_idle: false, ..Default::default() };
    let stats = Stats { orig_data_size: params.max_bytes, ..Default::default() };
    let mut zram_writeback =
        ZramWriteback::new(DEFAULT_TOTAL_ZRAM_SIZE, DEFAULT_ZRAM_WRITEBACK_SIZE);

    wmock
        .expect()
        .withf(write_params(ZRAM_WRITEBACK_PATH, "huge_idle"))
        .times(0)
        .returning(|_, _| Ok(()));
    wmock
        .expect()
        .withf(write_params(ZRAM_WRITEBACK_PATH, "idle"))
        .times(1)
        .returning(|_, _| Ok(()));
    wmock
        .expect()
        .withf(write_params(ZRAM_WRITEBACK_PATH, "huge"))
        .times(1)
        .returning(|_, _| Ok(()));

    assert!(zram_writeback
        .mark_and_flush_pages::<MockOps>(&params, &stats, Instant::now())
        .is_ok());
}

#[test]
fn mark_and_flush_pages_skip_idle() {
    let _m = OPS_MTX.lock();
    let wmock = MockOps::write_context();
    wmock.expect().withf(write_path_params(ZRAM_WRITEBACK_LIMIT_PATH)).returning(|_, _| Ok(()));
    wmock.expect().withf(write_path_params(ZRAM_IDLE_PATH)).returning(|_, _| Ok(()));
    let rmock = MockOps::read_to_string_context();
    setup_default_meminfo(&rmock);
    setup_default_writeback_limit_read(&rmock);
    let params = Params { idle: false, ..Default::default() };
    let stats = Stats { orig_data_size: params.max_bytes, ..Default::default() };
    let mut zram_writeback =
        ZramWriteback::new(DEFAULT_TOTAL_ZRAM_SIZE, DEFAULT_ZRAM_WRITEBACK_SIZE);

    wmock
        .expect()
        .withf(write_params(ZRAM_WRITEBACK_PATH, "huge_idle"))
        .times(1)
        .returning(|_, _| Ok(()));
    wmock
        .expect()
        .withf(write_params(ZRAM_WRITEBACK_PATH, "idle"))
        .times(0)
        .returning(|_, _| Ok(()));
    wmock
        .expect()
        .withf(write_params(ZRAM_WRITEBACK_PATH, "huge"))
        .times(1)
        .returning(|_, _| Ok(()));

    assert!(zram_writeback
        .mark_and_flush_pages::<MockOps>(&params, &stats, Instant::now())
        .is_ok());
}

#[test]
fn mark_and_flush_pages_skip_huge() {
    let _m = OPS_MTX.lock();
    let wmock = MockOps::write_context();
    wmock.expect().withf(write_path_params(ZRAM_WRITEBACK_LIMIT_PATH)).returning(|_, _| Ok(()));
    wmock.expect().withf(write_path_params(ZRAM_IDLE_PATH)).returning(|_, _| Ok(()));
    let rmock = MockOps::read_to_string_context();
    setup_default_meminfo(&rmock);
    setup_default_writeback_limit_read(&rmock);
    let params = Params { huge: false, ..Default::default() };
    let stats = Stats { orig_data_size: params.max_bytes, ..Default::default() };
    let mut zram_writeback =
        ZramWriteback::new(DEFAULT_TOTAL_ZRAM_SIZE, DEFAULT_ZRAM_WRITEBACK_SIZE);

    wmock
        .expect()
        .withf(write_params(ZRAM_WRITEBACK_PATH, "huge_idle"))
        .times(1)
        .returning(|_, _| Ok(()));
    wmock
        .expect()
        .withf(write_params(ZRAM_WRITEBACK_PATH, "idle"))
        .times(1)
        .returning(|_, _| Ok(()));
    wmock
        .expect()
        .withf(write_params(ZRAM_WRITEBACK_PATH, "huge"))
        .times(0)
        .returning(|_, _| Ok(()));

    assert!(zram_writeback
        .mark_and_flush_pages::<MockOps>(&params, &stats, Instant::now())
        .is_ok());
}

#[test]
fn mark_and_flush_pages_write_limit_from_orig_data_size() {
    let _m = OPS_MTX.lock();
    let wmock = MockOps::write_context();
    wmock.expect().withf(write_path_params(ZRAM_IDLE_PATH)).returning(|_, _| Ok(()));
    wmock.expect().withf(write_path_params(ZRAM_WRITEBACK_PATH)).returning(|_, _| Ok(()));
    let rmock = MockOps::read_to_string_context();
    setup_default_meminfo(&rmock);
    setup_default_writeback_limit_read(&rmock);
    let params = Params {
        max_bytes: 600 * DEFAULT_PAGE_SIZE,
        min_bytes: 10 * DEFAULT_PAGE_SIZE,
        ..Default::default()
    };
    // zram utilization is 25%
    let stats = Stats { orig_data_size: 500 * DEFAULT_PAGE_SIZE, ..Default::default() };
    let mut zram_writeback = ZramWriteback::new_with_page_size(
        2000 * DEFAULT_PAGE_SIZE,
        1000 * DEFAULT_PAGE_SIZE,
        DEFAULT_PAGE_SIZE,
    );

    // Writeback limit is 25% of max_bytes.
    wmock
        .expect()
        .withf(write_params(ZRAM_WRITEBACK_LIMIT_PATH, "150"))
        .times(1)
        .returning(|_, _| Ok(()));

    assert!(zram_writeback
        .mark_and_flush_pages::<MockOps>(&params, &stats, Instant::now())
        .is_ok());
}

#[test]
fn mark_and_flush_pages_write_limit_from_orig_data_size_with_big_page_size() {
    let _m = OPS_MTX.lock();
    let wmock = MockOps::write_context();
    wmock.expect().withf(write_path_params(ZRAM_IDLE_PATH)).returning(|_, _| Ok(()));
    wmock.expect().withf(write_path_params(ZRAM_WRITEBACK_PATH)).returning(|_, _| Ok(()));
    let rmock = MockOps::read_to_string_context();
    setup_default_meminfo(&rmock);
    setup_default_writeback_limit_read(&rmock);
    let page_size = 2 * DEFAULT_PAGE_SIZE;
    let params =
        Params { max_bytes: 600 * page_size, min_bytes: 10 * page_size, ..Default::default() };
    // zram utilization is 25%
    let stats = Stats { orig_data_size: 500 * page_size, ..Default::default() };
    let mut zram_writeback =
        ZramWriteback::new_with_page_size(2000 * page_size, 1000 * page_size, page_size);

    // Writeback limit is 25% of maxPages.
    wmock
        .expect()
        .withf(write_params(ZRAM_WRITEBACK_LIMIT_PATH, "150"))
        .times(1)
        .returning(|_, _| Ok(()));

    assert!(zram_writeback
        .mark_and_flush_pages::<MockOps>(&params, &stats, Instant::now())
        .is_ok());
}

#[test]
fn mark_and_flush_pages_write_limit_capped_by_current_writeback_size() {
    let _m = OPS_MTX.lock();
    let wmock = MockOps::write_context();
    wmock.expect().withf(write_path_params(ZRAM_IDLE_PATH)).returning(|_, _| Ok(()));
    wmock.expect().withf(write_path_params(ZRAM_WRITEBACK_PATH)).returning(|_, _| Ok(()));
    let rmock = MockOps::read_to_string_context();
    setup_default_meminfo(&rmock);
    setup_default_writeback_limit_read(&rmock);
    let params = Params {
        max_bytes: 600 * DEFAULT_PAGE_SIZE,
        min_bytes: 10 * DEFAULT_PAGE_SIZE,
        ..Default::default()
    };
    // zram utilization is 25%
    let stats =
        Stats { orig_data_size: 500 * DEFAULT_PAGE_SIZE, current_writeback_pages: 1000 - 50 };
    let mut zram_writeback = ZramWriteback::new_with_page_size(
        2000 * DEFAULT_PAGE_SIZE,
        1000 * DEFAULT_PAGE_SIZE,
        DEFAULT_PAGE_SIZE,
    );

    // Writeback disk only has 50 pages left.
    wmock
        .expect()
        .withf(write_params(ZRAM_WRITEBACK_LIMIT_PATH, "50"))
        .times(1)
        .returning(|_, _| Ok(()));

    assert!(zram_writeback
        .mark_and_flush_pages::<MockOps>(&params, &stats, Instant::now())
        .is_ok());
}

#[test]
fn mark_and_flush_pages_write_limit_capped_by_min_pages() {
    let _m = OPS_MTX.lock();
    let wmock = MockOps::write_context();
    wmock.expect().withf(write_path_params(ZRAM_IDLE_PATH)).returning(|_, _| Ok(()));
    wmock.expect().withf(write_path_params(ZRAM_WRITEBACK_PATH)).returning(|_, _| Ok(()));
    let rmock = MockOps::read_to_string_context();
    setup_default_meminfo(&rmock);
    setup_default_writeback_limit_read(&rmock);
    let params = Params {
        max_bytes: 500 * DEFAULT_PAGE_SIZE,
        min_bytes: 6 * DEFAULT_PAGE_SIZE,
        ..Default::default()
    };
    // zram utilization is 1%
    let stats = Stats { orig_data_size: 20 * DEFAULT_PAGE_SIZE, ..Default::default() };
    let mut zram_writeback = ZramWriteback::new_with_page_size(
        2000 * DEFAULT_PAGE_SIZE,
        1000 * DEFAULT_PAGE_SIZE,
        DEFAULT_PAGE_SIZE,
    );

    // Writeback limit is 5 pages (= 1% of 500 pages). But it is lower than min pages.
    wmock
        .expect()
        .withf(write_path_params(ZRAM_WRITEBACK_LIMIT_PATH))
        .times(0)
        .returning(|_, _| Ok(()));

    assert!(matches!(
        zram_writeback.mark_and_flush_pages::<MockOps>(&params, &stats, Instant::now()),
        Err(Error::Limit)
    ));
}

#[test]
fn mark_and_flush_pages_write_limit_capped_by_daily_limit_with_no_log() {
    let _m = OPS_MTX.lock();
    let wmock = MockOps::write_context();
    wmock.expect().withf(write_path_params(ZRAM_IDLE_PATH)).returning(|_, _| Ok(()));
    wmock.expect().withf(write_path_params(ZRAM_WRITEBACK_PATH)).returning(|_, _| Ok(()));
    let rmock = MockOps::read_to_string_context();
    setup_default_meminfo(&rmock);
    setup_default_writeback_limit_read(&rmock);
    let params = Params {
        max_bytes: 600 * DEFAULT_PAGE_SIZE,
        min_bytes: 10 * DEFAULT_PAGE_SIZE,
        max_bytes_per_day: 100 * DEFAULT_PAGE_SIZE,
        ..Default::default()
    };
    // zram utilization is 25%
    let stats = Stats { orig_data_size: 500 * DEFAULT_PAGE_SIZE, current_writeback_pages: 0 };
    let mut zram_writeback = ZramWriteback::new_with_page_size(
        2000 * DEFAULT_PAGE_SIZE,
        1000 * DEFAULT_PAGE_SIZE,
        DEFAULT_PAGE_SIZE,
    );

    // Writeback limit is 25% of max_bytes. But use smaller maxPagesPerDay as the limit.
    wmock
        .expect()
        .withf(write_params(ZRAM_WRITEBACK_LIMIT_PATH, "100"))
        .times(1)
        .returning(|_, _| Ok(()));

    assert!(zram_writeback
        .mark_and_flush_pages::<MockOps>(&params, &stats, Instant::now())
        .is_ok());
}

#[test]
fn mark_and_flush_pages_write_limit_capped_by_daily_limit() {
    let _m = OPS_MTX.lock();
    let wmock = MockOps::write_context();
    wmock.expect().withf(write_path_params(ZRAM_IDLE_PATH)).returning(|_, _| Ok(()));
    wmock.expect().withf(write_path_params(ZRAM_WRITEBACK_PATH)).returning(|_, _| Ok(()));
    let rmock = MockOps::read_to_string_context();
    setup_default_meminfo(&rmock);
    let params = Params {
        max_bytes: 600 * DEFAULT_PAGE_SIZE,
        min_bytes: 10 * DEFAULT_PAGE_SIZE,
        max_bytes_per_day: 100 * DEFAULT_PAGE_SIZE,
        ..Default::default()
    };
    let stats = Stats { orig_data_size: 500 * DEFAULT_PAGE_SIZE, current_writeback_pages: 0 };
    let mut zram_writeback = ZramWriteback::new_with_page_size(
        2000 * DEFAULT_PAGE_SIZE,
        1000 * DEFAULT_PAGE_SIZE,
        DEFAULT_PAGE_SIZE,
    );
    let base_point = Instant::now();

    // Sets 100 as the daily limit for the first time.
    wmock
        .expect()
        .withf(write_params(ZRAM_WRITEBACK_LIMIT_PATH, "100"))
        .times(1)
        .returning(|_, _| Ok(()));
    // Load updated writeback_limit as the initial value.
    rmock
        .expect()
        .withf(path_params(ZRAM_WRITEBACK_LIMIT_PATH))
        .times(1)
        .returning(|_| Ok("100".to_string()));
    // On first writeback, 40 pages are written back.
    rmock
        .expect()
        .withf(path_params(ZRAM_WRITEBACK_LIMIT_PATH))
        .returning(|_| Ok("60".to_string()));

    assert!(zram_writeback.mark_and_flush_pages::<MockOps>(&params, &stats, base_point).is_ok());

    // Daily limit with the history is applied on second markAndFlushPages.
    wmock
        .expect()
        .withf(write_params(ZRAM_WRITEBACK_LIMIT_PATH, "60"))
        .times(1)
        .returning(|_, _| Ok(()));

    assert!(zram_writeback
        .mark_and_flush_pages::<MockOps>(&params, &stats, base_point + Duration::from_secs(3600))
        .is_ok());
}

#[test]
fn mark_and_flush_pages_write_limit_capped_by_daily_limit_expired() {
    let _m = OPS_MTX.lock();
    let wmock = MockOps::write_context();
    wmock.expect().withf(write_path_params(ZRAM_IDLE_PATH)).returning(|_, _| Ok(()));
    wmock.expect().withf(write_path_params(ZRAM_WRITEBACK_PATH)).returning(|_, _| Ok(()));
    let rmock = MockOps::read_to_string_context();
    setup_default_meminfo(&rmock);
    let params = Params {
        max_bytes: 600 * DEFAULT_PAGE_SIZE,
        min_bytes: 10 * DEFAULT_PAGE_SIZE,
        max_bytes_per_day: 100 * DEFAULT_PAGE_SIZE,
        ..Default::default()
    };
    let stats = Stats { orig_data_size: 500 * DEFAULT_PAGE_SIZE, current_writeback_pages: 0 };
    let mut zram_writeback = ZramWriteback::new_with_page_size(
        2000 * DEFAULT_PAGE_SIZE,
        1000 * DEFAULT_PAGE_SIZE,
        DEFAULT_PAGE_SIZE,
    );
    let base_point = Instant::now();

    // Sets 100 as the daily limit for the first time.
    wmock
        .expect()
        .withf(write_params(ZRAM_WRITEBACK_LIMIT_PATH, "100"))
        .times(1)
        .returning(|_, _| Ok(()));
    // Load updated writeback_limit as the initial value.
    rmock
        .expect()
        .withf(path_params(ZRAM_WRITEBACK_LIMIT_PATH))
        .times(1)
        .returning(|_| Ok("100\n".to_string()));
    // On first writeback, 40 pages are written back.
    rmock
        .expect()
        .withf(path_params(ZRAM_WRITEBACK_LIMIT_PATH))
        .returning(|_| Ok("60\n".to_string()));

    assert!(zram_writeback.mark_and_flush_pages::<MockOps>(&params, &stats, base_point).is_ok());

    // On second time, the history is expired after 24 hours.
    wmock
        .expect()
        .withf(write_params(ZRAM_WRITEBACK_LIMIT_PATH, "100"))
        .times(1)
        .returning(|_, _| Ok(()));
    assert!(zram_writeback
        .mark_and_flush_pages::<MockOps>(
            &params,
            &stats,
            base_point + Duration::from_secs(24 * 3600)
        )
        .is_ok());
}

#[test]
fn mark_and_flush_pages_skip_on_write_limit() {
    let _m = OPS_MTX.lock();
    let wmock = MockOps::write_context();
    wmock.expect().withf(write_path_params(ZRAM_IDLE_PATH)).returning(|_, _| Ok(()));
    wmock.expect().withf(write_path_params(ZRAM_WRITEBACK_LIMIT_PATH)).returning(|_, _| Ok(()));
    let rmock = MockOps::read_to_string_context();
    setup_default_meminfo(&rmock);
    let params = Params::default();
    let stats = Stats { orig_data_size: params.max_bytes, ..Default::default() };
    let mut zram_writeback = ZramWriteback::new_with_page_size(
        DEFAULT_TOTAL_ZRAM_SIZE,
        DEFAULT_ZRAM_WRITEBACK_SIZE,
        DEFAULT_PAGE_SIZE,
    );

    // Load updated writeback_limit as the initial value.
    rmock
        .expect()
        .withf(path_params(ZRAM_WRITEBACK_LIMIT_PATH))
        .times(1)
        .returning(|_| Ok("100\n".to_string()));
    // On first writeback, writeback limit becomes zero and skip following writeback.
    rmock
        .expect()
        .withf(path_params(ZRAM_WRITEBACK_LIMIT_PATH))
        .returning(|_| Ok("0\n".to_string()));
    wmock
        .expect()
        .withf(write_params(ZRAM_WRITEBACK_PATH, "huge_idle"))
        .times(1)
        .returning(|_, _| Ok(()));
    wmock.expect().withf(write_params(ZRAM_WRITEBACK_PATH, "idle")).times(0);
    wmock.expect().withf(write_params(ZRAM_WRITEBACK_PATH, "huge")).times(0);

    assert!(zram_writeback
        .mark_and_flush_pages::<MockOps>(&params, &stats, Instant::now())
        .is_ok());
}

#[test]
fn mark_and_flush_pages_skip_next_by_daily_limit() {
    let _m = OPS_MTX.lock();
    let wmock = MockOps::write_context();
    wmock.expect().withf(write_path_params(ZRAM_IDLE_PATH)).returning(|_, _| Ok(()));
    wmock.expect().withf(write_path_params(ZRAM_WRITEBACK_PATH)).returning(|_, _| Ok(()));
    wmock.expect().withf(write_path_params(ZRAM_WRITEBACK_LIMIT_PATH)).returning(|_, _| Ok(()));
    let rmock = MockOps::read_to_string_context();
    setup_default_meminfo(&rmock);
    let params = Params { max_bytes_per_day: 100 * DEFAULT_PAGE_SIZE, ..Default::default() };
    let stats = Stats { orig_data_size: params.max_bytes, ..Default::default() };
    let mut zram_writeback = ZramWriteback::new_with_page_size(
        DEFAULT_TOTAL_ZRAM_SIZE,
        DEFAULT_ZRAM_WRITEBACK_SIZE,
        DEFAULT_PAGE_SIZE,
    );
    let base_point = Instant::now();

    // Load updated writeback_limit as the initial value.
    rmock
        .expect()
        .withf(path_params(ZRAM_WRITEBACK_LIMIT_PATH))
        .times(1)
        .returning(|_| Ok("100\n".to_string()));
    // On first writeback, writeback limit becomes zero and skip following writeback.
    rmock
        .expect()
        .withf(path_params(ZRAM_WRITEBACK_LIMIT_PATH))
        .returning(|_| Ok("0\n".to_string()));
    assert!(zram_writeback.mark_and_flush_pages::<MockOps>(&params, &stats, base_point).is_ok());

    wmock.checkpoint();
    wmock.expect().withf(write_path_params(ZRAM_WRITEBACK_LIMIT_PATH)).times(0);
    assert!(matches!(
        zram_writeback.mark_and_flush_pages::<MockOps>(
            &params,
            &stats,
            base_point + params.backoff_duration
        ),
        Err(Error::Limit)
    ));

    wmock.checkpoint();
    wmock.expect().withf(write_path_params(ZRAM_IDLE_PATH)).returning(|_, _| Ok(()));
    wmock.expect().withf(write_path_params(ZRAM_WRITEBACK_PATH)).returning(|_, _| Ok(()));
    wmock.expect().withf(write_path_params(ZRAM_WRITEBACK_LIMIT_PATH)).returning(|_, _| Ok(()));
    // writeback succeeds 24 hours after.
    assert!(zram_writeback
        .mark_and_flush_pages::<MockOps>(
            &params,
            &stats,
            base_point + Duration::from_secs(24 * 3600)
        )
        .is_ok());
}

#[test]
fn mark_and_flush_pages_fails_to_record_history_by_writeback_error() {
    let _m = OPS_MTX.lock();
    let wmock = MockOps::write_context();
    wmock.expect().withf(write_path_params(ZRAM_IDLE_PATH)).returning(|_, _| Ok(()));
    wmock.expect().withf(write_path_params(ZRAM_WRITEBACK_LIMIT_PATH)).returning(|_, _| Ok(()));
    let rmock = MockOps::read_to_string_context();
    setup_default_meminfo(&rmock);
    let params = Params { max_bytes_per_day: 100 * DEFAULT_PAGE_SIZE, ..Default::default() };
    let stats = Stats { orig_data_size: params.max_bytes, ..Default::default() };
    let mut zram_writeback = ZramWriteback::new_with_page_size(
        DEFAULT_TOTAL_ZRAM_SIZE,
        DEFAULT_ZRAM_WRITEBACK_SIZE,
        DEFAULT_PAGE_SIZE,
    );
    let base_point = Instant::now();

    // Load updated writeback_limit as the initial value.
    rmock
        .expect()
        .withf(path_params(ZRAM_WRITEBACK_LIMIT_PATH))
        .times(1)
        .returning(|_| Ok("100\n".to_string()));
    wmock
        .expect()
        .withf(write_path_params(ZRAM_WRITEBACK_PATH))
        .returning(|_, _| Err(std::io::Error::other("error")));
    assert!(zram_writeback.mark_and_flush_pages::<MockOps>(&params, &stats, base_point).is_err());

    wmock.checkpoint();
    wmock.expect().withf(write_path_params(ZRAM_WRITEBACK_LIMIT_PATH)).times(0);
    assert!(matches!(
        zram_writeback.mark_and_flush_pages::<MockOps>(
            &params,
            &stats,
            base_point + params.backoff_duration
        ),
        Err(Error::Limit)
    ));
}

#[test]
fn mark_and_flush_pages_fails_to_record_history_by_limit_load_error() {
    let _m = OPS_MTX.lock();
    let wmock = MockOps::write_context();
    wmock.expect().withf(write_path_params(ZRAM_IDLE_PATH)).returning(|_, _| Ok(()));
    wmock.expect().withf(write_path_params(ZRAM_WRITEBACK_PATH)).returning(|_, _| Ok(()));
    wmock.expect().withf(write_path_params(ZRAM_WRITEBACK_LIMIT_PATH)).returning(|_, _| Ok(()));
    let rmock = MockOps::read_to_string_context();
    setup_default_meminfo(&rmock);
    let params = Params { max_bytes_per_day: 100 * DEFAULT_PAGE_SIZE, ..Default::default() };
    let stats = Stats { orig_data_size: params.max_bytes, ..Default::default() };
    let mut zram_writeback = ZramWriteback::new_with_page_size(
        DEFAULT_TOTAL_ZRAM_SIZE,
        DEFAULT_ZRAM_WRITEBACK_SIZE,
        DEFAULT_PAGE_SIZE,
    );
    let base_point = Instant::now();

    rmock
        .expect()
        .withf(path_params(ZRAM_WRITEBACK_LIMIT_PATH))
        .times(1)
        .returning(|_| Ok("100\n".to_string()));
    // read writeback_limit fails just after writeback.
    rmock
        .expect()
        .withf(path_params(ZRAM_WRITEBACK_LIMIT_PATH))
        .returning(|_| Err(std::io::Error::other("error")));
    assert!(zram_writeback.mark_and_flush_pages::<MockOps>(&params, &stats, base_point).is_ok());

    wmock.checkpoint();
    wmock.expect().withf(write_path_params(ZRAM_WRITEBACK_LIMIT_PATH)).times(0);
    assert!(matches!(
        zram_writeback.mark_and_flush_pages::<MockOps>(
            &params,
            &stats,
            base_point + params.backoff_duration
        ),
        Err(Error::Limit)
    ));
}
