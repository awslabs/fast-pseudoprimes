// main.rs Copyright 2018 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(feature = "unstable", feature(asm))]
#![cfg_attr(feature = "unstable", feature(duration_as_u128))]

extern crate pseudoprimes;
extern crate rug;
extern crate threadpool;
extern crate itertools;

use pseudoprimes::*;

use magic_numbers::*;
use bloomfilter::*;

use std::time::Instant;
use pseudoprimes::time::get_elapsed_time;


fn main() {
    let start = Instant::now();

    // fp p<=0.001, 32GiB, k=2
    let filter = bloom_t1(&T1_INVERSE);

    let t2_map = build_t2(filter, &T2);

    println!("T2 matches: {}", t2_map.len());

    let results = final_sieve(&T1_INVERSE, t2_map, &T1, &T2);

    for result in results.iter() {
        println!("Found passing prime {}, vector {:?}", result.pseudoprime, result.factors);
    }

    println!("Completed in: {}, primes found: {}", get_elapsed_time(start), results.len());
}
