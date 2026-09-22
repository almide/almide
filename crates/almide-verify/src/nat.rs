//! Decimal naturals and the shared witness splitters — the Rust mirror of
//! `proofs/Subset.v` (`pnats`, `split_bar`, `split_semi`, `subset_check`).
//!
//! Coq's `nat` is unbounded, so a witness id is kept as its normalized digit
//! string (leading zeros stripped, zero = no digits) and compared by length
//! then bytes — exact for every input, with no overflow to disagree on.

use std::cmp::Ordering;
use std::collections::BTreeSet;

/// A natural number as its normalized decimal digits (`007` and `7` are equal;
/// zero is the empty digit string).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Nat(Vec<u8>);

impl Nat {
    fn from_digits(digits: &[u8]) -> Nat {
        let first = digits.iter().position(|&d| d != b'0').unwrap_or(digits.len());
        Nat(digits[first..].to_vec())
    }

    /// `n <= 1`, the call-mode signature's well-formedness test (`sig_wf`).
    pub(crate) fn le_one(&self) -> bool {
        self.0.is_empty() || self.0 == b"1"
    }

    /// The value as an index, if it fits (a larger value is out of range of
    /// any list this verifier can hold).
    pub(crate) fn as_index(&self) -> Option<usize> {
        if self.0.len() > 18 {
            return None;
        }
        self.0.iter().try_fold(0usize, |acc, &d| {
            acc.checked_mul(10)?.checked_add(usize::from(d - b'0'))
        })
    }
}

impl Ord for Nat {
    fn cmp(&self, other: &Nat) -> Ordering {
        self.0.len().cmp(&other.0.len()).then_with(|| self.0.cmp(&other.0))
    }
}

impl PartialOrd for Nat {
    fn partial_cmp(&self, other: &Nat) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Every maximal run of ASCII digits, in order (`pnats`): any other byte —
/// space, comma, a second `|` — only separates.
pub(crate) fn pnats(s: &[u8]) -> Vec<Nat> {
    s.split(|b| !b.is_ascii_digit())
        .filter(|run| !run.is_empty())
        .map(Nat::from_digits)
        .collect()
}

/// Split at the FIRST `|` (`split_bar`); with none, everything is the left
/// side and the right side is empty.
pub(crate) fn split_bar(s: &[u8]) -> (&[u8], &[u8]) {
    match s.iter().position(|&b| b == b'|') {
        Some(i) => (&s[..i], &s[i + 1..]),
        None => (s, &[]),
    }
}

/// `;`-separated segments, empties kept (`split_semi`): `""` is one empty
/// segment and a trailing `;` adds an empty last one.
pub(crate) fn split_semi(s: &[u8]) -> Vec<&[u8]> {
    s.split(|&b| b == b';').collect()
}

/// Every element of `sub` occurs in `sup` (`subset_check`; the sorted fast
/// path `subset_check_fast` decides the same relation, so one test serves).
pub(crate) fn subset(sup: &[Nat], sub: &[Nat]) -> bool {
    let sup: BTreeSet<&Nat> = sup.iter().collect();
    sub.iter().all(|x| sup.contains(x))
}

/// `<superset ids>|<subset ids>` → the subset holds (`subset_cert`). Both the
/// name-totality witness (`check_names_cert`: used ⊆ defined) and the
/// capability witness (`check_caps_cert`: used ⊆ allowed) are this shape.
pub(crate) fn subset_cert(witness: &[u8]) -> bool {
    let (sup, sub) = split_bar(witness);
    subset(&pnats(sup), &pnats(sub))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(s: &str) -> Nat {
        Nat::from_digits(s.as_bytes())
    }

    #[test]
    fn leading_zeros_do_not_change_the_value() {
        assert_eq!(n("007"), n("7"));
        assert_eq!(n("000"), n("0"));
        assert!(n("10") > n("9"));
        assert!(n("100000000000000000000000") > n("99999999999999999999999"));
    }

    #[test]
    fn pnats_treats_every_non_digit_as_a_separator() {
        assert_eq!(pnats(b" 1 22,3|4"), vec![n("1"), n("22"), n("3"), n("4")]);
        assert!(pnats(b"").is_empty());
    }

    #[test]
    fn split_semi_keeps_empty_segments() {
        assert_eq!(split_semi(b""), vec![&b""[..]]);
        assert_eq!(split_semi(b"a;"), vec![&b"a"[..], &b""[..]]);
    }

    #[test]
    fn as_index_refuses_what_does_not_fit() {
        assert_eq!(n("12").as_index(), Some(12));
        assert_eq!(n("0").as_index(), Some(0));
        assert_eq!(n("1000000000000000000000").as_index(), None);
    }
}
