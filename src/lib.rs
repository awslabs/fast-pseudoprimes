// lib.rs Copyright 2018 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)]
#![cfg_attr(feature = "unstable", feature(duration_as_u128))]
#![cfg_attr(feature = "unstable", feature(asm))]
#![cfg_attr(feature = "unstable", feature(core_intrinsics))]
#![cfg_attr(feature = "unstable", feature(avx512_target_feature))]

#[macro_use]
extern crate lazy_static;
extern crate modinverse;
extern crate rug;
extern crate threadpool;
extern crate hashbrown;

#[cfg(feature="unstable")]
extern crate libc;
extern crate itertools;

pub mod mulmod;
pub mod bloomfilter;
pub mod progress;
pub mod gray_prod_iter;
pub mod magic_numbers;
pub mod bitset;
pub mod modulus;
pub mod numa_threadpool;