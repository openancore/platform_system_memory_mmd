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

#[test]
fn mark_and_flush_pages() {
    let _m = OPS_MTX.lock();
    let mut seq = Sequence::new();
    let wmock = MockOps::write_context();
    let rmock = MockOps::read_to_string_context();
    setup_default_meminfo(&rmock);
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
