// gray_prod_iters.rs Copyright 2018 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0


use crate::modulus::*;

pub struct ProductSet<M: Modulus + 'static> {
    elems: Vec<u64>,
    inverse: Vec<u64>,
    modulus: M
}

impl<M: Modulus + 'static> ProductSet<M> {
    pub fn new(elems: &[u64], modulus: M) -> Self {
        let inverse = inverse(elems, modulus);
        ProductSet { elems: Vec::from(elems), inverse, modulus }
    }
}

pub struct ProductIter<'a, M: Modulus + 'static> {
    product_set: &'a ProductSet<M>,
    next: Option<(u64, u64)>,
    end: u64
}

/// convert index i to the i'th gray codeword
fn to_gray(v: u64) -> u64 {
    v ^ (v >> 1)
}

/// compute the subset product corresponding to the mask v
/// We include ps.elems[i] in the subset product if bit i of v is 1.
fn subsetprod<M: Modulus>(v: u64, ps: &ProductSet<M>) -> u64 {
    let mut accum = 1;

    for i in 0..ps.elems.len() {
        if (v & (1 << i)) != 0 {
            accum = ps.modulus.mulmod(accum, ps.elems[i]);
        }
    }

    accum
}

impl<'a, M: Modulus + 'static> ProductIter<'a, M> {
    /// takes the start and end codeword indices (not codewords themselves)
    pub fn new(product_set: &'a ProductSet<M>, start: u64, end: u64) -> Self {
        if start == end {
            return ProductIter { product_set, next: None, end };
        }
        assert!(start < end);
        // we can't iterate over all subset products for a set of size 64 (would require end=2^64)
        // so make sure the set is smaller than that
        assert!(product_set.elems.len() < 64);
        assert!(end <= (1 << product_set.elems.len()));

        let start_gray = to_gray(start);

        // compute the subset product corresponding to the 'start'th codeword
        let accum = subsetprod(start_gray, product_set);

        ProductIter { product_set, next: Some((start, accum)), end }
    }
}

impl<'a, M: Modulus + 'static> Iterator for ProductIter<'a, M> {
    type Item = (u64, u64);

    fn next(&mut self) -> Option<Self::Item> {
        let (cur_index, cur_val) = match self.next {
            Some(pair) => pair,
            None => return None
        };

        let cur_gray = to_gray(cur_index);

        let next_index = cur_index + 1;
        if next_index >= self.end {
            self.next = None;
            return Some((cur_gray, cur_val));
        }

        let next_gray = to_gray(next_index);

        let diff = cur_gray ^ next_gray;

        // index of the changed bit
        let bit  = 63 - diff.leading_zeros();

        let twiddle = if next_gray & diff != 0 {
            // we changed a bit from a 0 to a 1
            self.product_set.elems[bit as usize]
        } else {
            // we changed a bit from a 1 to a 0
            self.product_set.inverse[bit as usize]
        };

        let next_val = self.product_set.modulus.mulmod(cur_val, twiddle);

        self.next = Some((next_index, next_val));

        return Some((cur_gray, cur_val));
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use magic_numbers::R;

    /// computes all subset products in a range, stores a vector of pairs (i,ssp(i))
    fn reference_range<M: Modulus>(ps: &ProductSet<M>, start: u64, end: u64) -> Vec<(u64, u64)> {
        let mut out = Vec::new();

        for i in start..end {
            out.push((i, subsetprod(i,ps)));
        }

        return out;
    }

    /// sorts a list of pairs by the first element
    fn sort_range(r: &mut [(u64, u64)]) {
        r.sort_unstable_by(|&(ref ia, _), &(ref ib, _)| ia.cmp(ib));
    }

    #[test]
    pub fn test() {
        let ps = ProductSet::new(&R[0..63], MODULUS);

        // compute the subsect products using the gray code iterator
        // this range must be from 0 to a power of 2 for the test to work.
        let mut gray : Vec<(u64, u64)> = ProductIter::new(&ps, 0, 0x10).collect();

        // check that adjacent gray code words differ by one bit
        for i in 0..gray.len()-1 {
            let (gcw1,_) = gray[i];
            let (gcw2,_) = gray[i+1];
            assert_eq!((gcw1 ^ gcw2).count_ones(), 1);
        }

        sort_range(&mut gray);
        // compute the same subset products using the masking method
        let reference = reference_range(&ps, 0, 0x10);
        // make sure they are the same
        assert_eq!(gray, reference);        

        let gray : Vec<(u64, u64)> = ProductIter::new(&ps, 0x1000, 0x1200).collect();
        // check that the length of a custom range is correct
        assert_eq!(0x200, gray.len());
        // check that the subset products in this range are correct
        for (k, v) in gray {
            assert_eq!(subsetprod(k, &ps), v);
        }
    }
}