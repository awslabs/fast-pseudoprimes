// unstable.rs Copyright 2018 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use std::ptr;
use std::intrinsics::{atomic_load, atomic_or};
use std::marker::{Send,Sync};
use libc::{self, size_t, c_void};

/// see stable.rs for API documentation

#[cfg(feature = "numa")]
use libc::{c_ulong, c_long, c_int, ENOENT, EFAULT};

type Element = u32;
const BITS: usize = 32;

pub struct BitSet {
    arena: *mut Element,
    len: size_t
}

unsafe impl Send for BitSet {}
unsafe impl Sync for BitSet {}

impl Drop for BitSet {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(
                self.arena as *mut c_void,
                self.len
            );
        }
    }
}

#[cfg(feature = "numa")]
#[link(name="numa")]
extern {
    fn move_pages(pid: c_int, count: c_ulong, pages: *mut *mut c_void,
        nodes: *const c_int, status: *mut c_int, flags: c_int
    ) -> c_long;
}

#[cfg(all(feature = "numa"))]
const MPOL_MF_MOVE: c_int = (1 << 1);

#[cfg(all(feature = "numa"))]
const PAGE_SIZE_SHIFT: usize = 30;

impl BitSet {
    pub fn new(capacity: usize) -> Self {
        use libc::{MAP_ANONYMOUS, MAP_PRIVATE, MAP_HUGETLB};
        use libc::{PROT_READ, PROT_WRITE, MAP_FAILED};
        const MAP_HUGE_SHIFT: usize = 26;
        let capacity_blocks = (capacity + BITS - 1) / BITS;
        let capacity_bytes  = capacity_blocks * BITS / 8;

        let ptr = unsafe {
            libc::mmap(
                ptr::null_mut(),
                capacity_bytes,
                PROT_READ | PROT_WRITE,
                MAP_ANONYMOUS | MAP_PRIVATE | MAP_HUGETLB | (30 << MAP_HUGE_SHIFT),
                -1, 0
            )
        };

        if ptr == MAP_FAILED {
            panic!("Out of memory");
        }

        BitSet { arena: ptr as *mut Element, len: capacity_bytes }
    }

    #[cfg(feature = "numa")]
    pub fn on_node(self, node_id: u32) -> Self {
        let page_size = 1 << PAGE_SIZE_SHIFT;
        let n_pages = (self.len + page_size - 1) / page_size;

        let p_start = self.arena as *mut u8;
        for page in 0..n_pages {
            let mut pages = unsafe {p_start.offset((page * page_size) as isize)} as *mut c_void;
            let mut status : c_int = -1;
            let nodes  : c_int = node_id as c_int;

            unsafe {
                let p_pages = &mut pages;
                let p_pages = p_pages as *mut *mut c_void;
                let p_status = &mut status;
                let p_status = p_status as *mut c_int;
                let p_nodes  = &nodes as *const c_int;

                move_pages(0, 1, p_pages, p_nodes, p_status, MPOL_MF_MOVE);
            };

            if status < 0 && status != -ENOENT && status != -EFAULT {
                panic!("move_pages failed");
            }
        }

        self
    }

    pub fn max_index(&self) -> usize {
        return self.len * 8;
    }

    pub fn insert(&self, index: usize) {
        let block = index / BITS;
        let bit   = index % BITS;

        if block * (BITS / 8) >= self.max_index() {
            panic!("Element out of range: {}", index);
        }

        unsafe { atomic_or(self.arena.offset(block as isize), 1 << bit) };
    }

    pub fn contains(&self, index: usize) -> bool {
        let block = index / BITS;
        let bit   = index % BITS;

        if block * (BITS / 8) >= self.max_index() {
            panic!("Element out of range: {}", index);
        }

        return unsafe { atomic_load(self.arena.offset(block as isize)) } & (1 << bit) != 0;
    }

    pub fn cross_or(&mut self, other: &mut Self) {
        use std::time::Instant;
        use std::slice::from_raw_parts_mut;
        use threadpool::ThreadPool;

        assert_eq!(self.len, other.len);

        let now = Instant::now();

        let pool = ThreadPool::default();

        const JOB_SIZE : isize = 128isize << 20;

        let mut offset = 0;
        loop {
            // 128MiB jobs
            let mut end = offset + JOB_SIZE;
            if end > self.len as isize {
                end = self.len as isize;
            }

            let len     = (end - offset) as usize;
            let slice_a = unsafe {from_raw_parts_mut((self.arena as *mut u8).offset(offset), len)};
            let slice_b = unsafe {from_raw_parts_mut((other.arena as *mut u8).offset(offset), len)};

            pool.execute(move|| {
                cross_or_slice(slice_a, slice_b);
            });

            if end == self.len as isize {
                break;
            }

            offset += len as isize;
        }

        pool.join();
         
        let elapsed = now.elapsed();
        println!("Merge elapsed: {}s, {}ms", elapsed.as_secs(), elapsed.subsec_millis());
    }
}

fn cross_or_slice(a: &mut [u8], b: &mut [u8]) {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if is_x86_feature_detected!("avx512f") {
            unsafe {cross_or_avx512(a, b)};
            return;
        } else if is_x86_feature_detected!("avx2") {
            unsafe {cross_or_avx2(a,b)};
            return;
        }
    }

    cross_or_impl(a, b);
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
unsafe fn cross_or_avx2(a: &mut [u8], b: &mut [u8]) {
    cross_or_impl(a, b);
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx512f")]
unsafe fn cross_or_avx512(a: &mut [u8], b: &mut [u8]) {
    cross_or_impl(a, b);
}

fn cross_or_impl(slice_a: &mut [u8], slice_b: &mut [u8]) {
    // optimizer hint
    if slice_a.len() != slice_b.len() {
        unreachable!();
    }

    for (a, b) in slice_a.iter_mut().zip(slice_b.iter_mut()) {
        let val = *a | *b;
        *a = val;
        *b = val;
    }
}    


#[cfg(test)]
mod test {
    use super::*;

    #[cfg(all(feature = "numa"))]
    #[test]
    pub fn test_nodes() {
        let bitset = BitSet::new(8usize << 30);
        bitset.on_node(0);
    }

    #[test]
    pub fn test_cross_or() {
        let mut bitset1 = BitSet::new(128usize << 30);
        let mut bitset2 = BitSet::new(128usize << 30);

        for i in 0..16 {
            bitset1.insert(i << 33);
            bitset2.insert(i << 33);
        }

        bitset1.cross_or(&mut bitset2);
        panic!();
    }
}