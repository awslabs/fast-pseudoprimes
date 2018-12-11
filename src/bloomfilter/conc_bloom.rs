// conc_bloom.rs Copyright 2018 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use std::hash::{Hasher, Hash, BuildHasher};
use std::collections::hash_map::RandomState;
use std::marker::PhantomData;

use crate::bitset::BitSet;

pub struct Builder<T: Hash> {
    hash_states: Vec<RandomState>,
    size: usize,
    mask: usize,
    phantom: PhantomData<T>
}

pub struct BloomFilter<T: Hash> {
    hash_states: Vec<RandomState>,
    bits: BitSet,
    mask: usize,
    phantom: PhantomData<T>
}

impl<T: Hash> Builder<T> {
    /// takes size (in bits) and number of hashes
    pub fn new(size: usize, hashes: usize) -> Self {
        let mut hash_states = Vec::with_capacity(hashes);

        for _i in 0..hashes {
            hash_states.push(RandomState::new());
        }

        // Round size up to the next power of two
        let size = size as u64;
        let mut size_bits = 64 - size.leading_zeros() - 1;
        // if size is greater than a power of 2, we need an extra bit
        if 1 << size_bits < size {
            size_bits += 1;
        }
        let size = 1 << size_bits as usize;

        let mask = size - 1;

        Builder { hash_states, size, mask, phantom: PhantomData }
    }

    pub fn build(&self) -> BloomFilter<T> {
        BloomFilter {
            hash_states: self.hash_states.clone(),
            bits: BitSet::new(self.size),
            mask: self.mask,
            phantom: PhantomData
        }
    }

    pub fn on_node(&self, node_id: u32) -> BloomFilter<T> {
        let filter = self.build();
        BloomFilter {
            hash_states: filter.hash_states,
            bits: filter.bits.on_node(node_id),
            mask: filter.mask,
            phantom: filter.phantom
        }
    }
}

struct BitSelector<'a, T: Hash, I: Iterator<Item=&'a RandomState>> {
    item: T,
    hash_iter: I,
    mask: usize,
    locality: Option<usize>,
    local_index: usize
}

impl<'a, T: Hash, I: Iterator<Item=&'a RandomState>> BitSelector<'a, T, I> {
    fn new(item: T, mask: usize, iter: I) -> Self {
        BitSelector { item, mask, hash_iter: iter, locality: None, local_index: 0 }
    }
}

const LOCAL_INDEXES: usize = 2;
const LOCAL_MASK: usize = (1 << 8) - 1;

impl<'a, T: Hash, I: Iterator<Item=&'a RandomState>> Iterator for BitSelector<'a, T, I> {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        self.hash_iter.next().map(|hasher| {
            let mut hasher = hasher.build_hasher();
            self.item.hash(&mut hasher);

            let index = hasher.finish() as usize;

            let (offset, mask) = match &self.locality {
                Some(x) => (*x, LOCAL_MASK),
                None => (0, self.mask)
            };

            let index = (index & mask) + offset;
            self.local_index += 1;
            if self.local_index >= LOCAL_INDEXES {
                self.locality = None;
            } else {
                self.locality = Some(index & !LOCAL_MASK);
            }

            return index;
        })
    }
}

impl<T: Hash> BloomFilter<T> {
    pub fn new(size: usize, hashes: usize) -> Self {
        Builder::new(size, hashes).build()
    }

    pub fn maybe_present(&self, val: &T) -> bool {
        for i in BitSelector::new(val, self.mask, self.hash_states.iter()) {
            if !self.bits.contains(i) {
                return false;
            }
        }

        return true;
    }

    pub fn put(&self, val: &T) {
        for i in BitSelector::new(val, self.mask, self.hash_states.iter()) {
            self.bits.insert(i);
        }
    }

    pub fn cross_or(&mut self, other: &mut Self) {
        // TODO: check: assert_eq!(self.hash_states, other.hash_states);
        assert_eq!(self.mask, other.mask);

        self.bits.cross_or(&mut other.bits);
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    pub fn test_no_false_negative() {
        let filter = BloomFilter::new(100, 2);
        for i in 0..16 {
            filter.put(&i);
        }

        for i in 0..16 {
            assert!(filter.maybe_present(&i));
        }
    }

    #[test]
    pub fn test_fp_rate() {
        let filter = BloomFilter::new(8192, 4);
        // FP rate should be about p = 0.024
        let rate_min = 0.02;
        let rate_max = 0.028;

        for i in 0..1024 {
            filter.put(&i);
        }

        for i in 0..1024 {
            assert!(filter.maybe_present(&i));
        }

        let mut false_positives = 0;
        let mut total_lookups = 0;
        for i in 10000..200000 {
            if filter.maybe_present(&i) {
                false_positives += 1;
            }
            total_lookups += 1;
        }

        let rate = (false_positives as f64) / (total_lookups as f64);

        println!("FP rate: {}", rate);

        assert!(rate >= rate_min);
        assert!(rate <= rate_max);
    }
}