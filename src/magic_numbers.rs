// magic_numbers.rs Copyright 2018 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0
use lazy_static::*;
use rug::Integer;
use rug::integer::IsPrime;
use itertools::iproduct;
use crate::modulus::*;


pub const M: u64 = 11908862398227544750;
pub const MAX_R: u64 = 1152921504606846976;
pub const MIN_R: u64 = 256;

pub struct MagicPairs {
    b: isize,
    c: i32,
}

/// legendre symbol table
pub const MAGIC_PAIRS: [MagicPairs; 13] = [
    MagicPairs { b: 2, c: -1 },
    MagicPairs { b: 3, c: 1 },
    MagicPairs { b: 5, c: 1 },
    MagicPairs { b: 7, c: -1 },
    MagicPairs { b: 11, c: -1 },
    MagicPairs { b: 13, c: 1 },
    MagicPairs { b: 17, c: 1 },
    MagicPairs { b: 19, c: -1 },
    MagicPairs { b: 23, c: -1 },
    MagicPairs { b: 29, c: 1 },
    MagicPairs { b: 31, c: -1 },
    MagicPairs { b: 37, c: 1 },
    MagicPairs { b: 41, c: 1 },
];


lazy_static! {
    pub static ref R: Vec<u64> = {
        // Find the set R
        let mut results = Vec::new();

        // we compute all subset products of the prime factorization of M,
        // subject to the conditions laid out in the paper. In particular,
        // the conditions require that the subset product is even, because
        // 1+ssp must be prime to be in the set R.

        // note that 2 is excluded here: all valid subset products must 
        // be even, so we always multiply by 2 below
        let m_prime: [u64; 9] = [13, 17, 19, 23, 29, 31, 37, 41, 61];
        let mut m_pps = Vec::new();
        for (i,j,k) in iproduct!(vec!(1,5,25,125),vec!(1,7,49),vec!(1,11,121)) {
            m_pps.push(i*j*k);
        }
        let len = m_prime.len();
        for counter in 1..2u32.pow(len as u32) {
            let mut mask = counter;
            // it would be nice to use gray code subsets here,
            // but the gray_prod_iters is only for modular arithemtic
            let mut prime_ssp: u64 = 1;
            for i in 0..len {
                if mask & 0b1 == 1 {
                    prime_ssp *= m_prime[i];
                }
                mask = mask >> 1;
            }
            for pp_ssp in &m_pps {
                let candidate = (2 as u64)*pp_ssp*prime_ssp + 1;

                if check_divisor(candidate) {
                    if !results.contains(&candidate) {
                        results.push(candidate);
                    }
                }
            }
        }
        // This checks that we got the expected size of this set,
        // which is given in the Bleichenbacher paper.
        assert_eq!(64,results.len());

        results
    };
}

/// Find set  R of all integers r satisfying
/// 256 < r < 2^60
/// r - 1 | M
/// r is prime
/// (bi/r) = ci for )<i<14
pub fn check_divisor(r: u64) -> bool {
    if r < MIN_R || r > MAX_R {
        return false;
    }

    let r_int = Integer::from(r);
    let result = r_int.is_probably_prime(15);
    if result == IsPrime::No {
        return false;
    }

    for pair in MAGIC_PAIRS.iter() {
        let b = Integer::from(pair.b);
        if b.jacobi(&r_int) != pair.c {
            return false;
        }
    }
    
    return true;
}

lazy_static! {
    pub static ref T1: Vec<u64> = Vec::from(&R[0..32]);
    pub static ref T2: Vec<u64> = Vec::from(&R[32..64]);

    pub static ref T1_INVERSE: Vec<u64> = inverse(&T1[..],MODULUS);
    pub static ref T2_INVERSE: Vec<u64> = inverse(&T2[..],MODULUS);

    pub static ref MIN_N: Integer = Integer::from(Integer::i_pow_u(2, 512));
}

/// collect the values indicated by the mask
fn mask_to_big_int(accumulator: &mut Vec<Integer>, mask: u32, array: &[u64]) {
    let mut mask: u32 = mask;
    for i in 0..32 {
        if mask & 0b1 == 1 {
            accumulator.push(Integer::from(array[i]));
        }
        mask = mask >> 1;
    }
}

/// given a t1 mask and a t2 mask, outputs the values to multiply for the subset product
pub fn get_vals_to_multiply(t1: &[u64], t2: &[u64], t1_mask: u32, t2_mask: u32) -> Vec<Integer> {
    let mut values_to_multiply = Vec::new();
    values_to_multiply.push(Integer::from(2));
    mask_to_big_int(&mut values_to_multiply, t1_mask, t1);
    mask_to_big_int(&mut values_to_multiply, t2_mask, t2);

    return values_to_multiply;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pseudoprime {
    pub pseudoprime: Integer,
    pub factors: Vec<Integer>
}

/// checks whether the composite number indicated by t1_mask and t2_mask is actually a pseudoprime 
/// according to the conditions in the paper
pub fn check_prime(min_n: &Integer, t1: &[u64], t2: &[u64], t1_mask: u32, t2_mask: u32) -> Option<Pseudoprime> {
    use std::cmp::Ordering;
    let values_to_multiply = get_vals_to_multiply(t1, t2, t1_mask, t2_mask);

    let product = Integer::from(Integer::product(values_to_multiply.iter()));
    let n_result = Integer::from(&product + &Integer::from(1));
    if n_result.cmp(&min_n) == Ordering::Greater {
        let result = n_result.is_probably_prime(15);
        if result == IsPrime::Probably || result == IsPrime::Yes {
            return Some(Pseudoprime { pseudoprime: n_result, factors: values_to_multiply });
        }
    }

    return None;
}
