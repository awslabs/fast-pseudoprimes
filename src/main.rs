// main.rs Copyright 2018 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(feature = "unstable", feature(asm))]

extern crate pseudoprimes;
extern crate rug;
extern crate threadpool;
extern crate itertools;

use pseudoprimes::*;

use crate::magic_numbers::*;
use crate::bloomfilter::*;

use std::time::Instant;


fn main() {
    let total = Instant::now();

    // fp p<=0.001, 32GiB, k=2
    let filter = bloom_t1(&T1_INVERSE);

    let t2_map = build_t2(filter, &T2);

    println!("T2 matches: {}", t2_map.len());

    let results = final_sieve(&T1_INVERSE, t2_map, &T1, &T2);

    for result in results.iter() {
        println!("Found passing prime {}, vector {:?}", result.pseudoprime, result.factors);
    }

    println!("Total time: {} seconds, primes found: {}", total.elapsed().as_secs(), results.len());
}
