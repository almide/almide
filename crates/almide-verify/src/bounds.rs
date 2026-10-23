//! The bounds checker — the Rust mirror of `proofs/BoundsCert.v`'s
//! `check_bounds_cert` over `proofs/SpatialCert.v`'s `accepts` (#4005).
//!
//! The witness is `;`-separated stretches, each a sequence of
//! `c N` (check slot N), `a N` / `w N` (read / write slot N), `s N` (slot N
//! changed), `L{ … }` (a loop body), `B{ … | … }` (two arms); whitespace
//! between tokens is ignored. A string that does not parse is rejected.
//! Accepted iff every access in every stretch is preceded, on every path, by
//! a check of its slot with no change of the slot since.
//!
//! Coq's program is a continuation chain (`PCheck n k`); here it is the flat
//! sequence of the same items, walked with a loop, so a long stretch costs
//! no recursion depth. Slot numbers are compared as normalized decimals
//! (`Nat`), so no overflow can make the two disagree.

use crate::nat::Nat;
use std::collections::BTreeSet;

#[derive(Debug, PartialEq)]
enum Item {
    Check(Nat),
    Read(Nat),
    Write(Nat),
    Set(Nat),
    Loop(Vec<Item>),
    Branch(Vec<Item>, Vec<Item>),
}

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\n' | b'\r' | b'\t')
}

/// `prog_at`: the items from `i` to the first `}`, `|` or `;` (not consumed)
/// or the end. `None` = the bytes are not a program.
fn prog_at(s: &[u8], mut i: usize) -> Option<(Vec<Item>, usize)> {
    let mut items = Vec::new();
    loop {
        let Some(&c) = s.get(i) else { return Some((items, i)) };
        match c {
            b'}' | b'|' | b';' => return Some((items, i)),
            _ if is_space(c) => i += 1,
            b'c' | b'a' | b'w' | b's' => {
                let start = i + 1;
                let end = start + s[start..].iter().take_while(|b| b.is_ascii_digit()).count();
                if end == start {
                    return None;
                }
                let n = Nat::from_digits(&s[start..end]);
                items.push(match c {
                    b'c' => Item::Check(n),
                    b'a' => Item::Read(n),
                    b'w' => Item::Write(n),
                    _ => Item::Set(n),
                });
                i = end;
            }
            b'L' => {
                if s.get(i + 1) != Some(&b'{') {
                    return None;
                }
                let (body, j) = prog_at(s, i + 2)?;
                if s.get(j) != Some(&b'}') {
                    return None;
                }
                items.push(Item::Loop(body));
                i = j + 1;
            }
            b'B' => {
                if s.get(i + 1) != Some(&b'{') {
                    return None;
                }
                let (thn, j) = prog_at(s, i + 2)?;
                if s.get(j) != Some(&b'|') {
                    return None;
                }
                let (els, k) = prog_at(s, j + 1)?;
                if s.get(k) != Some(&b'}') {
                    return None;
                }
                items.push(Item::Branch(thn, els));
                i = k + 1;
            }
            _ => return None,
        }
    }
}

/// `progs_at`: the stretches separated by `;`, to the end of the bytes.
fn parse(s: &[u8]) -> Option<Vec<Vec<Item>>> {
    let mut out = Vec::new();
    let mut i = 0;
    loop {
        let (p, j) = prog_at(s, i)?;
        out.push(p);
        match s.get(j) {
            None => return Some(out),
            Some(b';') => i = j + 1,
            Some(_) => return None,
        }
    }
}

type Known = BTreeSet<Nat>;

/// `kills`: every slot a `Set` anywhere in the items names.
fn kills(items: &[Item], out: &mut Known) {
    for it in items {
        match it {
            Item::Set(n) => {
                out.insert(n.clone());
            }
            Item::Loop(body) => kills(body, out),
            Item::Branch(t, e) => {
                kills(t, out);
                kills(e, out);
            }
            _ => {}
        }
    }
}

/// `check`: the known set after `items`, or `None` when an access is not
/// justified.
fn check(mut kn: Known, items: &[Item]) -> Option<Known> {
    for it in items {
        match it {
            Item::Check(n) => {
                kn.insert(n.clone());
            }
            Item::Read(n) | Item::Write(n) => {
                if !kn.contains(n) {
                    return None;
                }
            }
            Item::Set(n) => {
                kn.remove(n);
            }
            Item::Loop(body) => {
                let mut ks = Known::new();
                kills(body, &mut ks);
                let k0: Known = kn.difference(&ks).cloned().collect();
                let kb = check(k0.clone(), body)?;
                if !k0.is_subset(&kb) {
                    return None;
                }
                kn = k0;
            }
            Item::Branch(t, e) => {
                let kt = check(kn.clone(), t)?;
                let ke = check(kn, e)?;
                kn = kt.intersection(&ke).cloned().collect();
            }
        }
    }
    Some(kn)
}

/// `check_bounds_cert`: the bytes parse and every stretch is accepted from
/// an empty known set.
pub(crate) fn check_bounds_cert(witness: &[u8]) -> bool {
    match parse(witness) {
        Some(ps) => ps.iter().all(|p| check(Known::new(), p).is_some()),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The Examples of proofs/BoundsCert.v, verdict for verdict.
    #[test]
    fn the_coq_examples() {
        assert!(check_bounds_cert(b"c0 a0 c1 a1 w0 w1"));
        assert!(check_bounds_cert(b"L{ s0 c0 a0 } ; B{ c3 | c3 a3 } a3"));
        assert!(!check_bounds_cert(b"c0 s0 a0"));
        assert!(!check_bounds_cert(b"c0 ; a0"));
        assert!(!check_bounds_cert(b"B{ c0 | } a0"));
        assert!(!check_bounds_cert(b"c0 a0 x"));
        assert!(!check_bounds_cert(b"c a0"));
        assert!(!check_bounds_cert(b"L{ c0 a0"));
        assert!(!check_bounds_cert(b"c0 a0 }"));
    }

    // SpatialCert.v's controls, through the byte format.
    #[test]
    fn the_spatial_cert_controls() {
        assert!(check_bounds_cert(b"c0 a0 a0"));
        assert!(check_bounds_cert(b"L{ s0 c0 a0 }"));
        assert!(check_bounds_cert(b"c0 L{ a0 s1 }"));
        assert!(check_bounds_cert(b"c0 a0 w0"));
        assert!(!check_bounds_cert(b"a0"));
        assert!(!check_bounds_cert(b"w0"));
        assert!(!check_bounds_cert(b"c0 L{ a0 s0 }"));
        assert!(!check_bounds_cert(b"c1 a0"));
    }

    #[test]
    fn slots_compare_as_numbers_and_the_empty_witness_is_vacuous() {
        assert!(check_bounds_cert(b"c007 a7"));
        assert!(check_bounds_cert(b"c0a0"));
        assert!(check_bounds_cert(b""));
        assert!(check_bounds_cert(b";"));
        assert!(!check_bounds_cert(b"L {c0}"));
        assert!(!check_bounds_cert(b"c0 | a0"));
    }

    #[test]
    fn a_long_stretch_needs_no_recursion() {
        let mut w = String::from("c0");
        for _ in 0..200_000 {
            w.push_str(" a0");
        }
        assert!(check_bounds_cert(w.as_bytes()));
    }
}
