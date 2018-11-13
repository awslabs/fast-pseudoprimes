// mod.rs Copyright 2018 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

#[cfg(all(feature = "unstable", target_arch = "x86_64"))]
mod mulmod_asm;

#[cfg(all(feature = "unstable", target_arch = "x86_64"))]
pub use self::mulmod_asm::mul_mod;

#[cfg(not(all(feature = "unstable", target_arch = "x86_64")))]
pub fn mul_mod(a: u64, b: u64, m: u64) -> u64 {
    (((a as u128) * (b as u128)) % (m as u128)) as u64
}
