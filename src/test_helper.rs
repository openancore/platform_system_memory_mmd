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

//! This module provides helpers for tests.
//!
//! `*_params` are the helper functions to use with `.withf` of [crate::os::MockOps].

use std::path::Path;
use std::sync::Mutex;

/// Mutex to synchronize tests using [crate::os::MockOps].
///
/// mockall for static functions requires synchronization.
///
/// https://docs.rs/mockall/latest/mockall/#static-methods
pub static OPS_MTX: Mutex<()> = Mutex::new(());

/// This is helper function for [Ops::write].
pub fn write_params<P: AsRef<Path>, C: AsRef<[u8]>>(
    p: P,
    c: C,
) -> impl Fn(&dyn AsRef<Path>, &dyn AsRef<[u8]>) -> bool {
    move |path, contents| path.as_ref() == p.as_ref() && contents.as_ref() == c.as_ref()
}

/// This is helper function for [Ops::write]. This consider the path only.
pub fn write_path_params<P: AsRef<Path>>(
    p: P,
) -> impl Fn(&dyn AsRef<Path>, &dyn AsRef<[u8]>) -> bool {
    move |path, _| path.as_ref() == p.as_ref()
}

/// This is helper function for functions with single `AsRef<Path>` argument.
///
/// * [Ops::read_to_string]
pub fn path_params<P: AsRef<Path>>(p: P) -> impl Fn(&dyn AsRef<Path>) -> bool {
    move |path| path.as_ref() == p.as_ref()
}
