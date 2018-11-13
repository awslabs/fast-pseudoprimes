// stable.rs Copyright 2018 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use std::sync::atomic::{AtomicUsize, Ordering};

fn usize_bits() -> usize {
    usize::max_value().count_ones() as usize
}

pub struct BitSet {
    bits: Vec<AtomicUsize>
}

impl BitSet {
    /// creates a new bitset with the specified size (in bits)
    pub fn new(capacity: usize) -> Self {
        let capacity_blocks = (capacity + usize_bits() - 1) / usize_bits();
        let mut bits = Vec::with_capacity(capacity_blocks);

        // initially, the bitset is all 0s
        for _ in 0..capacity_blocks {
            bits.push(AtomicUsize::new(0));
        }

        return BitSet { bits };
    }

    /// sets the bit at index `index`
    pub fn insert(&self, index: usize) {
        let block = index / usize_bits();
        let bit   = index % usize_bits();

        self.bits[block].fetch_or(1usize << bit, Ordering::Relaxed);
    }

    /// checks if the bit at index `index` is set
    pub fn contains(&self, index: usize) -> bool {
        let block = index / usize_bits();
        let bit   = index % usize_bits();

        return self.bits[block].load(Ordering::Relaxed) & (1usize << bit) != 0;
    }

    /// given inputs a and b, results in a = a|b and b=a|b
    pub fn cross_or(&mut self, other: &mut Self) {
        for (a, b) in self.bits.iter().zip(other.bits.iter()) {
            let val_a = a.load(Ordering::Relaxed);
            let val_b = b.load(Ordering::Relaxed);

            let val = val_a | val_b;

            a.store(val, Ordering::Relaxed);
            b.store(val, Ordering::Relaxed);
        }
    }
}