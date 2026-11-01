//! Unbounded signed integers — the Rust mirror of Coq's `Z`, exact for every
//! input (the linear-facts checker, lin.rs, multiplies and adds witness
//! constants, so a fixed-width integer could overflow where Coq does not and
//! the two verdicts would disagree).

use std::cmp::Ordering;

/// Sign and magnitude, base 2^32, little-endian limbs, no trailing zero
/// limb; zero is non-negative with no limbs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Z {
    neg: bool,
    mag: Vec<u32>,
}

fn trim(v: &mut Vec<u32>) {
    while v.last() == Some(&0) {
        v.pop();
    }
}

fn cmp_mag(a: &[u32], b: &[u32]) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.iter().rev().cmp(b.iter().rev()))
}

fn add_mag(a: &[u32], b: &[u32]) -> Vec<u32> {
    let mut out = Vec::with_capacity(a.len().max(b.len()) + 1);
    let mut carry = 0u64;
    for i in 0..a.len().max(b.len()) {
        let s = u64::from(*a.get(i).unwrap_or(&0)) + u64::from(*b.get(i).unwrap_or(&0)) + carry;
        out.push(s as u32);
        carry = s >> 32;
    }
    if carry > 0 {
        out.push(carry as u32);
    }
    out
}

/// `a - b` for `|a| >= |b|`.
fn sub_mag(a: &[u32], b: &[u32]) -> Vec<u32> {
    let mut out = Vec::with_capacity(a.len());
    let mut borrow = 0i64;
    for i in 0..a.len() {
        let mut d = i64::from(a[i]) - i64::from(*b.get(i).unwrap_or(&0)) - borrow;
        borrow = 0;
        if d < 0 {
            d += 1 << 32;
            borrow = 1;
        }
        out.push(d as u32);
    }
    trim(&mut out);
    out
}

fn mul_mag(a: &[u32], b: &[u32]) -> Vec<u32> {
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }
    let mut out = vec![0u32; a.len() + b.len()];
    for (i, &x) in a.iter().enumerate() {
        let mut carry = 0u64;
        for (j, &y) in b.iter().enumerate() {
            let t = u64::from(out[i + j]) + u64::from(x) * u64::from(y) + carry;
            out[i + j] = t as u32;
            carry = t >> 32;
        }
        let mut k = i + b.len();
        while carry > 0 {
            let t = u64::from(out[k]) + carry;
            out[k] = t as u32;
            carry = t >> 32;
            k += 1;
        }
    }
    trim(&mut out);
    out
}

impl Z {
    pub(crate) fn zero() -> Z {
        Z { neg: false, mag: Vec::new() }
    }

    pub(crate) fn from_i64(v: i64) -> Z {
        let m = v.unsigned_abs();
        let mut mag = vec![m as u32, (m >> 32) as u32];
        trim(&mut mag);
        Z { neg: v < 0 && m != 0, mag }
    }

    /// The decimal digits `ds` (ASCII), non-negative.
    pub(crate) fn from_digits(ds: &[u8]) -> Z {
        let mut z = Z::zero();
        let ten = Z::from_i64(10);
        for &d in ds {
            z = z.mul(&ten).add(&Z::from_i64(i64::from(d - b'0')));
        }
        z
    }

    /// 2^k.
    pub(crate) fn pow2(k: u32) -> Z {
        let mut mag = vec![0u32; (k / 32) as usize];
        mag.push(1 << (k % 32));
        Z { neg: false, mag }
    }

    pub(crate) fn is_neg(&self) -> bool {
        self.neg
    }

    pub(crate) fn is_pos(&self) -> bool {
        !self.neg && !self.mag.is_empty()
    }

    pub(crate) fn neg(&self) -> Z {
        Z { neg: !self.neg && !self.mag.is_empty(), mag: self.mag.clone() }
    }

    pub(crate) fn add(&self, o: &Z) -> Z {
        if self.neg == o.neg {
            return Z { neg: self.neg, mag: add_mag(&self.mag, &o.mag) };
        }
        match cmp_mag(&self.mag, &o.mag) {
            Ordering::Equal => Z::zero(),
            Ordering::Greater => Z { neg: self.neg, mag: sub_mag(&self.mag, &o.mag) },
            Ordering::Less => Z { neg: o.neg, mag: sub_mag(&o.mag, &self.mag) },
        }
    }

    pub(crate) fn sub(&self, o: &Z) -> Z {
        self.add(&o.neg())
    }

    pub(crate) fn mul(&self, o: &Z) -> Z {
        let mag = mul_mag(&self.mag, &o.mag);
        Z { neg: !mag.is_empty() && self.neg != o.neg, mag }
    }

    /// Converts a small non-negative value to an index; `None` past usize.
    pub(crate) fn as_index(&self) -> Option<usize> {
        if self.neg || self.mag.len() > 2 {
            return None;
        }
        let v = u64::from(*self.mag.first().unwrap_or(&0)) | (u64::from(*self.mag.get(1).unwrap_or(&0)) << 32);
        usize::try_from(v).ok()
    }
}

impl PartialOrd for Z {
    fn partial_cmp(&self, o: &Z) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

impl Ord for Z {
    fn cmp(&self, o: &Z) -> Ordering {
        match (self.neg, o.neg) {
            (false, true) => Ordering::Greater,
            (true, false) => Ordering::Less,
            (false, false) => cmp_mag(&self.mag, &o.mag),
            (true, true) => cmp_mag(&o.mag, &self.mag),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arithmetic_agrees_with_i128() {
        let vals: [i64; 9] = [0, 1, -1, 7, -12, i64::MAX, i64::MIN, 1 << 40, -(1 << 33) + 5];
        for &a in &vals {
            for &b in &vals {
                let (x, y) = (Z::from_i64(a), Z::from_i64(b));
                let to = |v: i128| {
                    let s = v.unsigned_abs().to_string();
                    let z = Z::from_digits(s.as_bytes());
                    if v < 0 { z.neg() } else { z }
                };
                assert_eq!(x.add(&y), to(i128::from(a) + i128::from(b)));
                assert_eq!(x.sub(&y), to(i128::from(a) - i128::from(b)));
                assert_eq!(x.mul(&y), to(i128::from(a) * i128::from(b)));
                assert_eq!(x.cmp(&y), a.cmp(&b));
            }
        }
        assert_eq!(Z::pow2(63), Z::from_digits(b"9223372036854775808"));
        assert_eq!(Z::from_digits(b"000"), Z::zero());
        assert!(!Z::zero().neg().is_neg());
    }
}
