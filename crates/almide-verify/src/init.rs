//! The init checker — the Rust mirror of `proofs/InitCert.v`'s
//! `check_init_cert` (#4013).
//!
//! The witness is `;`-separated construction sites: a fixed site
//! `h H w W n N r R` followed by `s I` stores, or a loop fill
//! `h H w W v K r R f L t S` (the count in local K, the countdown loop on
//! local L, the cursor step S). Whitespace between tokens is ignored, but a
//! tag touches its digits. A string that does not parse is rejected; the
//! empty string is vacuous. A fixed site is accepted iff H > 0, W > 0, every
//! slot below N is stored and no store names a slot at or past N
//! (`site_ok`); a loop fill iff H > 0, W > 0, K = L and S = W (`fill_ok`).

use crate::nat::Nat;
use std::collections::BTreeSet;

enum Site {
    Fixed { hdr: Nat, width: Nat, count: Nat, stores: Vec<Nat> },
    Fill { hdr: Nat, width: Nat, count_local: Nat, counter: Nat, step: Nat },
}

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\n' | b'\r' | b'\t')
}

fn skip_blank(s: &[u8], mut i: usize) -> usize {
    while s.get(i).is_some_and(|&b| is_space(b)) {
        i += 1;
    }
    i
}

/// `field tag`: after blanks, `tag` touching at least one digit.
fn field(s: &[u8], i: usize, tag: u8) -> Option<(Nat, usize)> {
    let i = skip_blank(s, i);
    if s.get(i) != Some(&tag) || !s.get(i + 1).is_some_and(u8::is_ascii_digit) {
        return None;
    }
    let start = i + 1;
    let end = start + s[start..].iter().take_while(|b| b.is_ascii_digit()).count();
    Some((Nat::from_digits(&s[start..end]), end))
}

/// `site_at`: one site, and where the rest begins (the end or a `;`).
fn site_at(s: &[u8], i: usize) -> Option<(Site, usize)> {
    let (hdr, i) = field(s, i, b'h')?;
    let (width, i) = field(s, i, b'w')?;
    if let Some((count, i)) = field(s, i, b'n') {
        let (_repr, mut i) = field(s, i, b'r')?;
        let mut stores = Vec::new();
        loop {
            let j = skip_blank(s, i);
            match s.get(j) {
                None | Some(b';') => return Some((Site::Fixed { hdr, width, count, stores }, j)),
                _ => {
                    let (n, k) = field(s, j, b's')?;
                    stores.push(n);
                    i = k;
                }
            }
        }
    }
    let (count_local, i) = field(s, i, b'v')?;
    let (_repr, i) = field(s, i, b'r')?;
    let (counter, i) = field(s, i, b'f')?;
    let (step, i) = field(s, i, b't')?;
    let j = skip_blank(s, i);
    match s.get(j) {
        None | Some(b';') => Some((Site::Fill { hdr, width, count_local, counter, step }, j)),
        _ => None,
    }
}

fn parse(s: &[u8]) -> Option<Vec<Site>> {
    let mut out = Vec::new();
    let mut i = 0;
    loop {
        if skip_blank(s, i) == s.len() {
            return Some(out);
        }
        let (site, j) = site_at(s, i)?;
        out.push(site);
        match s.get(j) {
            None => return Some(out),
            Some(b';') => i = j + 1,
            Some(_) => return None,
        }
    }
}

fn site_ok(site: &Site) -> bool {
    let zero = Nat::from_digits(b"");
    match site {
        Site::Fixed { hdr, width, count, stores } => {
            if *hdr == zero || *width == zero || stores.iter().any(|i| i >= count) {
                return false;
            }
            // Every store is below N; they cover 0..N exactly when N of them are distinct.
            let distinct: BTreeSet<&Nat> = stores.iter().collect();
            Nat::from_digits(distinct.len().to_string().as_bytes()) == *count
        }
        Site::Fill { hdr, width, count_local, counter, step } => {
            *hdr != zero && *width != zero && count_local == counter && step == width
        }
    }
}

/// `check_init_cert`: the bytes parse and every site is accepted.
pub(crate) fn check_init_cert(witness: &[u8]) -> bool {
    parse(witness).is_some_and(|sites| sites.iter().all(site_ok))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_coq_examples() {
        assert!(check_init_cert(b"h12 w8 n3 r0 s0 s1 s2"));
        assert!(check_init_cert(b"h12 w8 n3 r0 s2 s0 s2 s1"));
        assert!(check_init_cert(b"h12 w8 n2 r0 s0 s1 ; h12w4n1r3s0"));
        assert!(check_init_cert(b""));
        assert!(check_init_cert(b"h12 w8 n0 r0"));
        assert!(!check_init_cert(b"h12 w8 n3 r0 s0 s2"));
        assert!(!check_init_cert(b"h12 w8 n2 r0 s0 s1 s2"));
        assert!(!check_init_cert(b"h12 w0 n0 r0"));
        assert!(!check_init_cert(b"h12 w8 n1 r0 s0 x"));
        assert!(!check_init_cert(b"h12 n1 w8 r0 s0"));
        assert!(!check_init_cert(b"h12 w8 n1 r0 s"));
    }

    #[test]
    fn edges() {
        assert!(!check_init_cert(b"h0 w8 n0 r0"));
        assert!(!check_init_cert(b"h 12 w8 n0 r0"));
        assert!(check_init_cert(b"h12 w8 n1 r0 s0 ;"));
        assert!(check_init_cert(b"h012 w8 n01 r0 s000"));
        assert!(!check_init_cert(b";"));
    }

    #[test]
    fn loop_fills() {
        assert!(check_init_cert(b"h12 w8 v4 r0 f4 t8"));
        assert!(check_init_cert(b"h12 w8 v4 r0 f4 t8 ; h12 w8 n1 r0 s0"));
        assert!(!check_init_cert(b"h12 w8 v4 r0 f5 t8"));
        assert!(!check_init_cert(b"h12 w8 v4 r0 f4 t16"));
        assert!(!check_init_cert(b"h12 w8 v4 r0 f4 t8 s0"));
        assert!(!check_init_cert(b"h12 w8 v4 r0 f4"));
    }
}
