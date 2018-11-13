// mulmod_asm.rs Copyright 2018 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

pub fn mul_mod(a: u64, b: u64, m: u64) -> u64 {
    let _q: u64;
    let r: u64;
    unsafe {
        asm!("mulq $3; divq $4;"
             : "=&{ax}"(_q), "=&{dx}"(r)
             : "{ax}"(a), "r"(b), "r"(m)
             );
    }
    r
}
