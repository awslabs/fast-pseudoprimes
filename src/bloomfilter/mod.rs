// mod.rs Copyright 2018 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::gray_prod_iter::*;
use crate::progress;
use crate::numa_threadpool::ThreadPool;

use std::sync::{Mutex, Arc, mpsc::channel};
use std::sync::atomic::{Ordering, AtomicUsize};
use std::collections::HashMap;
use std::time::Instant;

mod conc_bloom;
use crate::bloomfilter::conc_bloom::*;

use crate::magic_numbers::*;
use crate::modulus::*;

const FILTER_SIZE : usize = 1usize << 39;
const FILTER_HASHES : usize = 2;
const N_TASKS : u64 = 1u64 << 16;

/// computes gray code SSPs from the start'th gray code word to the end'th gray code word (not included),
/// inserting the SSP values into the Bloom filter
pub fn bloom_t1_kernel<M: Modulus>(
    product_set: &ProductSet<M>, 
    start: u64, 
    end: u64, 
    filter: &BloomFilter<u64>, 
    progress: &progress::ProgressReporter
) {
    let mut handle = progress.handle();

    let filter = &filter as &BloomFilter<u64>;

    for (_k, v) in ProductIter::new(&product_set, start, end) {
        filter.put(&v);
        handle.report(1);
    }
}

/// For all subsets of the input array t1 (which main.rs passes T1_INVERSE),
/// computes the corresponding subset product
/// and inserts the product into a Bloom filter.
/// Details: we crate a bloom filter for each NUMA node,
/// then divides the work up into lots of chunks. 
/// The subset products for each chunk are inserted into one of the two bloom filters,
/// and at the end of the comptutation, we "cross_or" the two filters together so that
/// each bloom filter contains *all* of the subset products.
/// We output a map from NUMA node ID to a bloom filter,
/// where each bloom filter contains all subset products in t1.
pub fn bloom_t1(t1: &[u64]) -> HashMap<u32, Arc<BloomFilter<u64>>> {
    // we will work on 2^t1.len() subsets; divide this into N tasks
    let total_work = 1u64 << t1.len();

    let progress = Arc::new(progress::ProgressReporter::new("bloom_t1", total_work as usize));
    // crate an empty bloom filter
    let builder = conc_bloom::Builder::new(FILTER_SIZE, FILTER_HASHES);

    let product_set = Arc::new(ProductSet::new(t1, MODULUS));

    let pool = ThreadPool::new(|node_id| builder.on_node(node_id));
    
    let per_task = total_work / N_TASKS;

    // evaluate the kernel for each task
    for i in 0..N_TASKS {
        let start_idx = per_task * i;
        let end_idx = if i == N_TASKS - 1 { total_work } else { start_idx + per_task };

        let product_set = product_set.clone();
        let progress = progress.clone();

        pool.execute(move|filter| {
            bloom_t1_kernel(&product_set, start_idx, end_idx, &filter, &progress);
        });
    }

    // wait for all tasks to complete
    let mut filters = pool.join();

    // this code ONLY works for at most two NUMA nodes; it would have to be generalized
    // to work for more.
    assert!(filters.len() <= 2);

    // if there are exactly two NUMA nodes, there are two bloom filters: each containing
    // half of the subset products. We need *both* filters to contain *all* subset products
    // so we just OR the two bitsets together.
    if filters.len() == 2 {
        let (node_id, mut f2) = filters.pop().unwrap();
        filters[0].1.cross_or(&mut f2);
        filters.push((node_id, f2));
    }

    // crate a map from NUMA node to corresponding bloom filter
    let mut filtermap = HashMap::new();
    for (node, filter) in filters.into_iter() {
        filtermap.insert(node, Arc::new(filter));
    }

    return filtermap;
}

/// outputs a vector of (t2-idx,SSP) pairs for SSPs found in the bloom filter
/// (using the bloom filter closest to the NUMA node running the kernel)
fn build_t2_kernel<M: Modulus>(
    filter: &BloomFilter<u64>,
    progress: &progress::ProgressReporter,
    product_set: &ProductSet<M>,
    start: u64,
    end: u64
) -> Vec<(u32, u64)> {
    let mut results = Vec::new();
    let mut handle = progress.handle();

    for (mask, ssp) in ProductIter::new(&product_set, start, end) {
        if filter.maybe_present(&ssp) {
            results.push((mask as u32, ssp));
        }

        handle.report(1);
    }

    return results;
}

/// The next step is to compute all subset products for the array t2, and record those
/// that were also SSPs for T1_INVERSE.
/// Again, this task is divided up into many chunks, which gets assigned to available
/// compute resources. For each subset proudct, we check the (closest copy of the) bloom filter.
/// If the product is in the bloom filter, we add the (product, SSP mask) to the map,
/// otherwise we discard it.
/// Outputs a hashmap from SSPs to t2-masks which crate them for SSPs found in the bloom filter
pub fn build_t2(
    filters: HashMap<u32, Arc<BloomFilter<u64>>>, 
    t2: &[u64]
) -> HashMap<u64, u32> {
    // we will work on 2^t2.len() subsets; divide this into N tasks
    let total_work = 1u64 << t2.len();
    let progress = Arc::new(progress::ProgressReporter::new("t2_map", total_work as usize));
    let product_set = Arc::new(ProductSet::new(t2, MODULUS));  

    let per_task = total_work / N_TASKS;

    let pool : ThreadPool<Arc<BloomFilter<u64>>> = ThreadPool::new(|node_id| 
        filters.get(&node_id).unwrap_or_else(|| {
            println!("Warning: Couldn't find a T1 for node {}, falling back to arbitrary node", node_id);
            filters.iter().next().unwrap().1
        }).clone()
    );

    let (tx, rx) = channel();
    let parallel_end = Arc::new(Mutex::new(Instant::now()));

    // evaluate the kernel for each task
    for task_idx in 0..N_TASKS {
        let start_idx = task_idx * per_task;
        let end_idx   = if task_idx == N_TASKS - 1 { total_work } else { start_idx + per_task };

        let progress = progress.clone();
        let product_set = product_set.clone();
        let parallel_end = parallel_end.clone();
        let tx = tx.clone();

        pool.execute(move |filter| {
            let result = build_t2_kernel(&filter, &progress, &product_set, start_idx, end_idx);
            let mut guard = parallel_end.lock().unwrap();
            *guard = Instant::now();

            tx.send(result).unwrap();
        });
    }

    // each kernel returns a vector of subset indicators and subset products, where
    // the subset product is in 
    let mut hashmap = HashMap::new();
    for _task_idx in 0..N_TASKS {
        let vals = rx.recv().unwrap();

        for (v, k) in vals {
            hashmap.insert(k, v);
        }
    }

    println!("[t2 serial] {} entries, {} seconds single-thread",
        hashmap.len(), parallel_end.lock().unwrap().elapsed().as_secs()
    );

    hashmap
}

/// Compute subset products for some range in t1_product_set.
/// If the SSP is in t2map, we have found a match! Check the candidate
/// for the remaining conditions, and save it if they are met (otherwise it is a `t3_miss`)
fn final_sieve_kernel<M:Modulus>(
    t1_product_set: &ProductSet<M>,
    t2map: &HashMap<u64, u32>,
    start_idx: u64,
    end_idx: u64,
    t1: &[u64],
    t2: &[u64],
    t3_misses: &AtomicUsize,
    results: &Mutex<Vec<Pseudoprime>>
) {
    for (t1_mask, v) in ProductIter::new(t1_product_set, start_idx, end_idx) {
        match t2map.get(&v) {
            Some(t2_mask) => {
                match check_prime(&MIN_N, t1, t2, t1_mask as u32, *t2_mask) {
                    Some(result) => {
                        let mut guard = results.lock().unwrap();
                        guard.push(result);
                    }
                    None => {
                        t3_misses.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            None => {}
        }
    }
}


/// The final step is to *recompute* the SSPs for T1_INVERSE (this is a memory-bound computation).
/// If the SSP is a key in the map from the previous step, we have found a candidate pseudoprime.
/// We check the remaining conditions, and if the candidate is satisfactory, add it to the output vector. 
pub fn final_sieve(
    t1_forward: &[u64],
    t2map: HashMap<u64, u32>,
    t1: &[u64],
    t2: &[u64]
) -> Vec<Pseudoprime> {
    let t2map = Arc::new(t2map);
    let pool = ThreadPool::new(|_| ());
    let t1_product_set = Arc::new(ProductSet::new(t1_forward, MODULUS));
    // a counter for candidates which have a matching subset in T2 and T1_INVERSE,
    // but which do not satisfy the remaining conditions imposed by Bleichenbacher.
    let t3_misses = Arc::new(AtomicUsize::new(0));
    let results = Arc::new(Mutex::new(Vec::new()));

    for task in 0..N_TASKS {
        let t2map = t2map.clone();
        let t1_product_set = t1_product_set.clone();

        let start_idx = task * N_TASKS;
        let end_idx = start_idx + N_TASKS;

        let t1 = Vec::from(t1);
        let t2 = Vec::from(t2);
        let t3_misses = t3_misses.clone();
        let results = results.clone();

        pool.execute(move |_| {
            final_sieve_kernel(&t1_product_set, &t2map, start_idx, end_idx, &t1, &t2,
                &t3_misses, &results);
        })
    }
    // wait for all tasks to complete
    pool.join();

    // accumulate results
    let results = Arc::try_unwrap(results).unwrap();
    let results = results.into_inner().unwrap();

    let t3_misses = t3_misses.load(Ordering::SeqCst);

    println!("Found {} pseudoprimes, with {} T3 misses, {} T2 false positives",
        results.len(), t3_misses, t2map.len() - t3_misses - results.len());

    return results;
}
