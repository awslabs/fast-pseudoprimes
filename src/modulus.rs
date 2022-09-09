// modulus.rs Copyright 2018 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::magic_numbers::M;
use modinverse::modinverse;

pub const MODULUS : OptiM = OptiM{};

/// invert an entire array
pub fn inverse<M: Modulus>(xs: &[u64], modulus: M) -> Vec<u64> {
    let mut ys = Vec::with_capacity(xs.len());

    for x in xs {
        let inv = modulus.inverse(*x).or_else(
            || {
                panic!("Can't invert {}", *x);
            }
        ).unwrap();

        debug_assert_eq!(1, modulus.mulmod(inv, *x), "Bad inverse for {}", x);

        ys.push(inv);
    }

    ys
}

pub trait Modulus : Copy + Clone {
    fn addmod(&self, a: u64, b: u64) -> u64;
    fn mulmod(&self, a: u64, b: u64) -> u64;
    fn inverse(&self, v: u64) -> Option<u64>;
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct BasicDivisor {
    modulus: u64
}

impl BasicDivisor {
    pub fn new(modulus: u64) -> Self {
        BasicDivisor { modulus }
    }
}

/// Modulus for an arbitrary modulus
impl Modulus for BasicDivisor {
    fn addmod(&self, a: u64, b: u64) -> u64 {
        return (((a as u128) + (b as u128)) % (self.modulus as u128)) as u64;
    }

    fn inverse(&self, v: u64) -> Option<u64> {
        modinverse(v as i128, self.modulus as i128).map(|result|
            ((result + (self.modulus as i128)) % (self.modulus as i128)) as u64
        )
    }

    #[cfg(not(all(feature = "unstable", target_arch = "x86_64")))]
    fn mulmod(&self, a: u64, b: u64) -> u64 {
        (((a as u128) * (b as u128)) % (self.modulus as u128)) as u64
    }


    #[cfg(all(feature = "unstable", target_arch = "x86_64"))]
    fn mulmod(&self, a: u64, b: u64) -> u64 {
        let _q: u64;
        let r: u64;
        let _tmp: u64;

        unsafe {
            asm!("mulq $4;
                  // If EDX:EAX > M << 64, then we'll raise #FP on divq, so we need to fix this up
                  mov %rdx, $2;
                  sub $5, %rdx;
                  cmovc $2, %rdx;
                  divq $5;"
                : "=&{ax}"(_q), "=&{dx}"(r), "=&r" (_tmp)
                : "{ax}"(a), "rm"(b), "r"(self.modulus)
                : "cc"
                );
        }
        r
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct OptiM {}

/// 2^124/M
const M_RECIP : u128 = 0x18c8acd8d948f58b6a0fb2d6e1aaecf8u128;
const LO_64   : u128 = (1u128 << 64) - 1;

#[cfg(all(feature = "unstable", target_arch = "x86_64"))]
fn reduce_m_asm(v: u128) -> u64 {
    let v_hi = (v >> 64) as u64;
    let v_lo = v as u64;
    let r_hi = (M_RECIP >> 64) as u64;
    let r_lo = M_RECIP as u64;
    let mulv_hi : u64;
    let mulv_lo : u64;

    let _tmp1 : u64;
    let _tmp2 : u64;

    // (mid, lo)  = v_hi * m_lo + v_lo * m_hi
    // (hi, mid) += v_hi * m_hi
    // quot = (hi,mid) >> 60
    // (mulv_hi, mulv_lo) = quot * M
    
    unsafe {
        asm!("
            mulq $7;                      // v_lo * m_hi
            mov %rax, $1; mov %rdx, $0;   // stash (mid, lo)
            mov $5, %rax; mulq $6;        // v_hi * m_lo
            add %rax, $1; adc %rdx, $0;   // update (mid, lo)
            mov $5, %rax; mulq $7;        // v_hi * m_hi => RDX:RAX
            add $0, %rax; adc $$0, %rdx;  // merge mid into hi:mid
            mov %rdx, $0; shr $$60, $0;   // save q_hi into tmp1
            shr $$60, %rax; shl $$4, %rdx; or %rdx, %rax;  // q_lo(RAX) = (RDX:RAX) >> 60
            mov %rax, $1; // save q_lo
            mulq $8;  // (RAX * M) => RDX:RAX (M * q_lo)
            test $0, $0;  // do we have a high quotient part?
            mov %rax, $3; // save mulv_lo
            mov %rdx, $2; // save mulv_hi (partial)
            jz .no_hi${:uid};
            mov $0, %rax; // prep for high multiply
            mulq $8; add %rax, $2;
            .align 16;
            .no_hi${:uid}:
            "
            /* 0           1                 2                 3              */
            : "=&r" (_tmp1), "=&r" (_tmp2), "=&r" (mulv_hi), "=&r" (mulv_lo)
            /* 4              5           6             7           8 */
            : "{ax}" (v_lo), "r" (v_hi), "rm" (r_lo), "r" (r_hi), "r" (M)
            : "cc", "ax", "dx"
        )
    }

    let mulv = ((mulv_hi as i128) << 64) | (mulv_lo as i128);
    let mut diff = (v as i128) - mulv;

    if diff > (M as i128) {
        diff -= M as i128;
    }

    #[cfg(test)]
    {
        let q = ((_tmp1 as u128) << 64) | (_tmp2 as u128);
        let true_q = v / (M as u128);
        let diff_q = (true_q as i128) - (q as i128);
        assert!(diff < (M as i128) && diff >= 0, "diff: {:016X} quot: {:016X} true quot: {:016X} error: {}", diff, q, (v / (M as u128)), diff_q);
    }

    return diff as u64;
}

#[allow(dead_code)]
fn reduce_m(v: u128) -> u64 {
    let v_lo = v & LO_64;
    let v_hi = v >> 64;
    let m_lo = M_RECIP & LO_64;
    let m_hi = M_RECIP >> 64;

    // We're doing a multiply followed by a shift-by-124 here.

    let (mid, overflow) = (v_hi * m_lo).overflowing_add(v_lo * m_hi);
    let mut hi_128 = v_hi * m_hi;

    if overflow {
        hi_128 += 1u128 << 64;
    }

    hi_128 += mid >> 64;

    let quot = hi_128 >> 60;

    #[cfg(test)]
    let mulv = quot.checked_mul(M as u128).unwrap();

    #[cfg(not(test))]
    let mulv = quot * (M as u128);

    let mut diff = (v as i128) - (mulv as i128);

    // We sacrifice some accuracy by skipping the low-order multiplies, correct for this with a branch
    if diff > (M as i128) {
        diff -= M as i128;
    }

    #[cfg(test)]
    {
        assert!(diff >= 0);
        assert!(diff < (M as i128), "diff: {:032X} m: {:032X}", diff, M);
    }

    return diff as u64;
}

/// highly tuned Modulus implementation for the specific modulus used in the paper
impl Modulus for OptiM {
    fn addmod(&self, a: u64, b: u64) -> u64 {
        let r = (a as u128) + (b as u128);
        if r > (M as u128) {
            (r - (M as u128)) as u64
        } else {
            r as u64
        }
    }

    fn inverse(&self, v: u64) -> Option<u64> {
        modinverse(v as i128, M as i128).map(|result|
            ((result + (M as i128)) % (M as i128)) as u64
        )
    }

    fn mulmod(&self, a: u64, b: u64) -> u64 {
        #[cfg(all(feature = "unstable", target_arch = "x86_64"))]
        return reduce_m_asm((a as u128) * (b as u128));

        #[cfg(not(all(feature = "unstable", target_arch = "x86_64")))]
        return reduce_m((a as u128) * (b as u128));
    }
}
 
pub mod test {
    extern crate rand;
    use self::rand::*;
    use super::*;

    #[cfg(all(feature = "unstable", target_arch = "x86_64"))]
    #[inline(never)]
    fn check_asm(v: u128) -> u64 {
        reduce_m_asm(v)
    }

    #[test]
    pub fn test_reduce0() {
        test_reduce();
    }

    #[inline(never)]
    pub fn check_bd(a : u64, b : u64) -> u64 {
        BasicDivisor::new(M).mulmod(a, b) 
    }

    pub fn test_reduce() {
        let mut rng = thread_rng();

        for _ in 0..1000000 {
            let a = rng.gen();
            let b = rng.gen();
            let v = (a as u128) * (b as u128);

            let refval = (v % (M as u128)) as u64;

            let opt1 = OptiM{}.mulmod(a, b);
            let basicdiv = BasicDivisor::new(M).mulmod(a, b);
            assert_eq!(opt1, refval);
            if basicdiv != refval {
                assert_eq!(check_bd(a,b), refval);
            }

            #[cfg(all(feature = "unstable", target_arch = "x86_64"))]
            {
                use std::panic;

                let v2 = v;
                let result = panic::catch_unwind(move|| reduce_m_asm(v2))
                    .unwrap_or_else(|_| check_asm(v));

                assert_eq!(opt1, result);
            }
        }
    }
}
