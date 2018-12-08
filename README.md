## Fast Pseudoprimes
This package finds 55 fake primes that pass the Miller Rabin test for the fixed bases [2,3,5,7,11,13,17,19,23,29,31,37,41].

This attack comes from https://pdfs.semanticscholar.org/e9f1/f083adc1786466d344db5b3d85f4c268b429.pdf?_ga=2.5572531.1844877752.1534547402-251794521.1523992889.

The Bleichenbacher attack requires finding subsets of a set R of size 64 whose product is 1 mod M. We accomplish this using a meet-in-the-middle algorithm which divides the set into two halves of size 32 each. We look for collisions between subset proudcts of the first half and subset products of the inverse of the second half. The simplest method for achieving this goal is to create a hashmap for SSPs from the first half (from SSP values to corresponding mask). We would then compute SSPs for the second half of (the inverse of) R and check for membership in the map, making the process essentially two phases.

However, it is hard to parallelize hashmap insertion. If only a single thread is used, we waste processing power, and are constrained by memory latency to a large hashmap. In addition, insertions are not truly constant time. Even if we could parallelize insertions, we would then be constrained by memory bandwidth. A solution to this is to create multiple hashmaps, each local to a NUMA node. Unfortunately, merging the hashmaps is relatively slow.

The solution to all of these problems is to use a three-phase approach with a Bloom filter. Insertion in a Bloom filter is guaranteed constant time. It is trivial to have multiple threads operating at once, and merging Bloom filters is linear time. The Bloom filter only stores the SSP itself, not the corresponding mask, which makes this a three phase approach: we insert the SSPs for the first half of R into the filter, and then compute the SSPs for the second half of R, checking for membership in the Bloom filter and storing the SSP and the mask in a (small) hashmap. In the third phase, we *recompute* the SSPs for the first half of R, checking the hashmap for membership. In more detail, the three phases are:

## PHASE 1
In this stage, we need to record 2^32 64-bit subset products (SSPs) for the first half of R. We build a Bloom filter for this, since parallel insertion is easy, and it is easy to combine the results of two Bloom filters. Since we are inserting 2^32 items, a large Bloom filter is needed (approximately 2^39 bits to get a reasonable false-positive rate). Runtime of the algorithm is bound by memory, so we can increase perforamce by creating two Bloom filters, each of size 2^39 bits, for each of the NUMA nodes on a m5d.24xlarge EC2 instance, and inserting the SSPs into one of the two filters (depending on the NUMA node of the thread running computing the SSP). The resulting Bloom filters are then OR'd together to form two identical Bloom filters, both containing all 2^32 SSPs.

## PHASE 2
The next phase of the algorithm computes all 2^32 SSPs of the (inverse mod M of the) other half of the set. We check the closest Bloom filter to see if the SSP may be preset. If so, we record this in a map from SSP values to the SSP mask (from this phase) which created the SSP.

## PHASE 3
In the final phase, we recompute all of the subset products in phase 1 (recall the computation is memory bound) and check the phase 2 map for membership. If the SSP is a key in the map, we use the corresponding value (the SSP mask from phase 2) and the SSP mask from this phase to obtain the subset of the original set which meets the conditions from the paper. Each of these (very few) values is further checked for remaining conditions.

## Note on the code
* Must run on a computer with at least ~128 GB of memory
* This package makes use of "unsafe" assembly for fast 64 bit multiplication mod another 64 bit number
* Requires rust nightly
* We use Gray codes to avoid duplicate work when computing subset products.
* This is a memory-bound computation, and is highly optimized for this. It uses information about NUMA nodes to create multiple bloom filters for local access, and then shares the result across all nodes.

## How to run this code 
From a fresh Ubuntu EC2 instance (m5d.24xlarge):

```
sudo apt update --fix-missing
sudo apt install libnuma-dev build-essential m4
curl https://sh.rustup.rs -sSf | sh
source $HOME/.cargo/env
rustup install nightly
for i in /sys/devices/system/node/node*/hugepages/hugepages-1048576kB/nr_hugepages; do
    echo 64 | sudo tee $i
done
git clone https://github.com/awslabs/fast-pseudoprimes.git
cd fast-pseudoprimes
cargo +nightly run --features numa,unstable --release
```
The code takes about 21.9 seconds to run from start to finish.

## Status of this code
This code is released as-is, and we have no plans to maintain it. We are happy to accept pull requests.

## Contact Us
* Andrew Hopkins andhop@amazon.com
* Eric Crockett ericcro@amazon.com
