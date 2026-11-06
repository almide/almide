//! The routine bounds checker — the Rust mirror of `proofs/RoutineBounds.v`'s
//! `accepts` through `proofs/RoutineBoundsCert.v`'s byte format (#4005 A4).
//!
//! A stretch of the `bounds` witness that begins with `R` is a routine
//! program (bounds.rs hands it here); every other stretch is the list-element
//! format bounds.rs judges itself. Items, whitespace between them ignored:
//!
//! - `hX,O,W` / `HX,O,W` — a header read / write of slot X's block;
//! - `aX,S,O,W` / `wX,S,O,W` — a payload slot read / write at stride S;
//! - `nN,X,S` — N := a count of X's block at stride S;
//! - `cX,N,S` / `eX,N,S` — the routine's test / a loop's exit test;
//! - `sX`, `L{ … }`, `B{ … | … }`;
//! - `C{ pairs | pairs | body }` — a helper call: preconditions,
//!   postconditions (`X:S` pairs), the helper's body.
//!
//! Coq's program is a continuation chain; here it is the flat item list,
//! walked with a loop, and every number is a normalized decimal (`Nat`), so
//! no overflow can make the two disagree.

use crate::nat::Nat;
use std::collections::BTreeSet;

type Fact = (Nat, Nat); // (slot, stride)
type Count = (Nat, Nat, Nat); // (count, slot, stride)

#[derive(Debug, PartialEq)]
pub(crate) enum Item {
    Hdr(Nat, Nat),
    Acc(Fact, Nat, Nat),
    Len(Count),
    Check(Fact, Nat),
    Exit(Fact, Nat),
    Set(Nat),
    Loop(Vec<Item>),
    Branch(Vec<Item>, Vec<Item>),
    Call(Vec<Fact>, Vec<Fact>, Vec<Item>),
}

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\n' | b'\r' | b'\t')
}

fn number(s: &[u8], i: usize) -> Option<(Nat, usize)> {
    let end = i + s.get(i..)?.iter().take_while(|b| b.is_ascii_digit()).count();
    (end > i).then(|| (Nat::from_digits(&s[i..end]), end))
}

/// `nums k`: k numbers joined by `,`.
fn nums(s: &[u8], mut i: usize, k: usize) -> Option<(Vec<Nat>, usize)> {
    let mut out = Vec::with_capacity(k);
    for j in 0..k {
        let (n, e) = number(s, i)?;
        out.push(n);
        i = e;
        if j + 1 < k {
            if s.get(i) != Some(&b',') {
                return None;
            }
            i += 1;
        }
    }
    Some((out, i))
}

/// `pairs_at`: `X:S` pairs up to the first `|` (not consumed).
fn pairs_at(s: &[u8], mut i: usize) -> Option<(Vec<Fact>, usize)> {
    let mut out = Vec::new();
    loop {
        let &c = s.get(i)?;
        if c == b'|' {
            return Some((out, i));
        }
        if is_space(c) {
            i += 1;
            continue;
        }
        let (x, e) = number(s, i)?;
        if s.get(e) != Some(&b':') {
            return None;
        }
        let (st, e2) = number(s, e + 1)?;
        out.push((x, st));
        i = e2;
    }
}

/// `rprog_at`: the items from `i` to the first `}`, `|` or `;` (not
/// consumed) or the end.
fn rprog_at(s: &[u8], mut i: usize) -> Option<(Vec<Item>, usize)> {
    let mut items = Vec::new();
    loop {
        let Some(&c) = s.get(i) else { return Some((items, i)) };
        match c {
            b'}' | b'|' | b';' => return Some((items, i)),
            _ if is_space(c) => i += 1,
            b'h' | b'H' | b'n' | b'c' | b'e' => {
                let (v, e) = nums(s, i + 1, 3)?;
                let [a, b, d]: [Nat; 3] = v.try_into().ok()?;
                items.push(match c {
                    b'h' | b'H' => Item::Hdr(b, d),
                    b'n' => Item::Len((a, b, d)),
                    b'c' => Item::Check((a, d), b),
                    _ => Item::Exit((a, d), b),
                });
                i = e;
            }
            b'a' | b'w' => {
                let (v, e) = nums(s, i + 1, 4)?;
                let [x, st, o, w]: [Nat; 4] = v.try_into().ok()?;
                items.push(Item::Acc((x, st), o, w));
                i = e;
            }
            b's' => {
                let (x, e) = number(s, i + 1)?;
                items.push(Item::Set(x));
                i = e;
            }
            b'L' => {
                if s.get(i + 1) != Some(&b'{') {
                    return None;
                }
                let (body, j) = rprog_at(s, i + 2)?;
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
                let (thn, j) = rprog_at(s, i + 2)?;
                if s.get(j) != Some(&b'|') {
                    return None;
                }
                let (els, k) = rprog_at(s, j + 1)?;
                if s.get(k) != Some(&b'}') {
                    return None;
                }
                items.push(Item::Branch(thn, els));
                i = k + 1;
            }
            b'C' => {
                if s.get(i + 1) != Some(&b'{') {
                    return None;
                }
                let (req, j) = pairs_at(s, i + 2)?;
                let (ens, k) = pairs_at(s, j + 1)?;
                let (body, l) = rprog_at(s, k + 1)?;
                if s.get(l) != Some(&b'}') {
                    return None;
                }
                items.push(Item::Call(req, ens, body));
                i = l + 1;
            }
            _ => return None,
        }
    }
}

/// A routine stretch's text after its `R`: its items and where it ends.
pub(crate) fn parse_routine(s: &[u8], i: usize) -> Option<(Vec<Item>, usize)> {
    rprog_at(s, i)
}

fn zero() -> Nat {
    Nat::from_digits(b"0")
}

fn positive(n: &Nat) -> bool {
    *n > zero()
}

/// `in_slot s o w`: 0 < s, 0 < w, o + w <= s.
fn in_slot(st: &Nat, o: &Nat, w: &Nat) -> bool {
    positive(st) && positive(w) && o.add(w) <= *st
}

/// `in_hdr o w`: 0 < w, o + w <= PAYLOAD (12).
fn in_hdr(o: &Nat, w: &Nat) -> bool {
    positive(w) && o.add(w) <= Nat::from_digits(b"12")
}

fn kills(items: &[Item], out: &mut BTreeSet<Nat>) {
    for it in items {
        match it {
            Item::Set(x) => {
                out.insert(x.clone());
            }
            Item::Len((n, _, _)) => {
                out.insert(n.clone());
            }
            Item::Loop(body) => kills(body, out),
            Item::Branch(t, e) => {
                kills(t, out);
                kills(e, out);
            }
            Item::Call(_, _, body) => kills(body, out),
            _ => {}
        }
    }
}

type Known = BTreeSet<Fact>;
type Counts = BTreeSet<Count>;

fn minus(kn: &Known, cn: &Counts, ks: &BTreeSet<Nat>) -> (Known, Counts) {
    (
        kn.iter().filter(|f| !ks.contains(&f.0)).cloned().collect(),
        cn.iter().filter(|c| !ks.contains(&c.0)).cloned().collect(),
    )
}

/// `check`: the known facts and counts after `items`, or `None` when an
/// access is not justified.
fn check(mut kn: Known, mut cn: Counts, items: &[Item]) -> Option<(Known, Counts)> {
    // Coq's `QExit` is `(kn, cn) ∩ check ((x, s) :: kn, cn) k`: unfolded over
    // the sequence, the final sets are met with every pair an exit left from.
    let mut left: Vec<(Known, Counts)> = Vec::new();
    for it in items {
        match it {
            Item::Hdr(o, w) => {
                if !in_hdr(o, w) {
                    return None;
                }
            }
            Item::Acc(f, o, w) => {
                if !kn.contains(f) || !in_slot(&f.1, o, w) {
                    return None;
                }
            }
            Item::Len((n, x, st)) => {
                if !positive(st) {
                    return None;
                }
                kn.retain(|f| f.0 != *n);
                cn.retain(|c| c.0 != *n);
                cn.insert((n.clone(), x.clone(), st.clone()));
            }
            Item::Check(f, n) => {
                if !cn.contains(&(n.clone(), f.0.clone(), f.1.clone())) {
                    return None;
                }
                kn.insert(f.clone());
            }
            Item::Exit(f, n) => {
                if !cn.contains(&(n.clone(), f.0.clone(), f.1.clone())) {
                    return None;
                }
                left.push((kn.clone(), cn.clone()));
                kn.insert(f.clone());
            }
            Item::Set(x) => {
                kn.retain(|f| f.0 != *x);
                cn.retain(|c| c.0 != *x);
            }
            Item::Loop(body) => {
                let mut ks = BTreeSet::new();
                kills(body, &mut ks);
                let (k0, c0) = minus(&kn, &cn, &ks);
                let (kb, cb) = check(k0.clone(), c0.clone(), body)?;
                if !k0.is_subset(&kb) || !c0.is_subset(&cb) {
                    return None;
                }
                kn = k0;
                cn = c0;
            }
            Item::Branch(t, e) => {
                let (kt, ct) = check(kn.clone(), cn.clone(), t)?;
                let (ke, ce) = check(kn, cn, e)?;
                kn = kt.intersection(&ke).cloned().collect();
                cn = ct.intersection(&ce).cloned().collect();
            }
            Item::Call(req, ens, body) => {
                if !req.iter().all(|f| kn.contains(f)) {
                    return None;
                }
                let (kb, _) = check(req.iter().cloned().collect(), Counts::new(), body)?;
                if !ens.iter().all(|f| kb.contains(f)) {
                    return None;
                }
                let mut ks = BTreeSet::new();
                kills(body, &mut ks);
                let (k1, c1) = minus(&kn, &cn, &ks);
                kn = k1;
                kn.extend(ens.iter().cloned());
                cn = c1;
            }
        }
    }
    for (lk, lc) in left {
        kn = kn.intersection(&lk).cloned().collect();
        cn = cn.intersection(&lc).cloned().collect();
    }
    Some((kn, cn))
}

/// `RoutineBounds.accepts`: the stretch is accepted from no facts.
pub(crate) fn accepts(items: &[Item]) -> bool {
    check(Known::new(), Counts::new(), items).is_some()
}

#[cfg(test)]
mod tests {
    use crate::bounds::check_bounds_cert;

    // The Examples of proofs/RoutineBoundsCert.v, verdict for verdict.
    #[test]
    fn the_coq_examples() {
        assert!(check_bounds_cert(b"c0 a0 c1 a1 w0 w1"));
        assert!(!check_bounds_cert(b"c0 s0 a0"));
        assert!(check_bounds_cert(b"L{ e0 a0 s0 }"));
        assert!(!check_bounds_cert(b"a0 e0"));
        assert!(check_bounds_cert(b"R n1,0,1 L{ e0,1,1 a0,1,0,1 s0 }"));
        assert!(check_bounds_cert(b"R h0,4,4 ; c0 a0"));
        assert!(check_bounds_cert(b"R C{ | 2:8 | n1,2,8 c2,1,8 } a2,8,4,4"));
        assert!(!check_bounds_cert(b"R a0,1,0,1"));
        assert!(!check_bounds_cert(b"R h0,8,8"));
        assert!(!check_bounds_cert(b"R C{ | 3:8 | n1,2,8 c2,1,8 }"));
        assert!(!check_bounds_cert(b"R a0,1,0"));
        assert!(!check_bounds_cert(b"R x"));
        assert!(!check_bounds_cert(b"R L{ s0"));
    }

    // RoutineBounds.v's controls, through the byte format.
    #[test]
    fn the_routine_bounds_controls() {
        assert!(check_bounds_cert(b"R h0,4,4"));
        assert!(!check_bounds_cert(b"R h0,8,8"));
        assert!(check_bounds_cert(b"R n1,0,1 L{ e0,1,1 a0,1,0,1 s0 }"));
        assert!(!check_bounds_cert(b"R n1,0,1 c0,1,1 a0,8,0,8"));
        assert!(!check_bounds_cert(b"R n1,0,8 c0,1,8 a0,8,4,8"));
        assert!(!check_bounds_cert(b"R n1,2,1 c0,1,1 a0,1,0,1"));
        assert!(!check_bounds_cert(b"R n1,0,1 s1 c0,1,1 a0,1,0,1"));
        assert!(check_bounds_cert(b"R C{ | 2:8 | n1,2,8 c2,1,8 } a2,8,4,4"));
        assert!(!check_bounds_cert(b"R C{ | | n1,2,8 c2,1,8 } a2,8,4,4"));
        assert!(!check_bounds_cert(b"R C{ | 3:8 | n1,2,8 c2,1,8 }"));
        assert!(!check_bounds_cert(b"R C{ 0:1 | | }"));
        assert!(check_bounds_cert(b"R n1,0,1 c0,1,1 C{ 0:1 | | a0,1,0,1 }"));
    }
}
