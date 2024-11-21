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

//! This module provides mockable interface and its implementation of os
//! operations.

use std::io;
use std::path::Path;

/// [Ops] is the mockable interface for accesses to system global os properties
/// (e.g. read/write on procfs, sysfs).
///
/// generic functions with AsRef requires #[mockall::concretize] which has a
/// downside that we can't use `.with` but need to use `.withf` or `.withf_st`.
///
/// https://docs.rs/mockall/latest/mockall/attr.concretize.html
///
/// `mod test_helpers` defines helper macros to generate matcher for `.withf`.
#[cfg_attr(test, mockall::automock)]
pub trait Ops {
    /// mockable wrapper for [std::fs::read_to_string].
    #[mockall::concretize]
    fn read_to_string<P: AsRef<Path>>(path: P) -> io::Result<String>;
    /// mockable wrapper for [std::fs::write].
    #[mockall::concretize]
    fn write<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, contents: C) -> io::Result<()>;
}

/// Real implementation of [Ops].
pub struct OpsImpl;

impl Ops for OpsImpl {
    fn read_to_string<P: AsRef<Path>>(path: P) -> io::Result<String> {
        std::fs::read_to_string(path)
    }
    fn write<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, contents: C) -> io::Result<()> {
        std::fs::write(path, contents)
    }
}

/// Returns the page size of the system.
pub fn get_page_size() -> u64 {
    // SAFETY: `sysconf` simply returns an integer.
    unsafe { libc::sysconf(libc::_SC_PAGESIZE) as u64 }
}
