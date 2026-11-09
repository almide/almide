//! The byte format of a linear certificate (proofs/RoutineLinCert.v),
//! parsed into lin.rs's items — split out of lin.rs to hold its file cap.

use super::{Arg, Item, Lin, Pf, Proc, Var};
use crate::nat::Nat;
use crate::zint::Z;

fn sk(s: &[u8], mut i: usize) -> usize {
    while s.get(i).is_some_and(|&b| matches!(b, b' ' | b'\n' | b'\r' | b'\t' | b',')) {
        i += 1;
    }
    i
}

fn tok(s: &[u8], i: usize, c: u8) -> Option<usize> {
    let j = sk(s, i);
    (s.get(j) == Some(&c)).then_some(j + 1)
}

fn at_stop(s: &[u8], i: usize) -> bool {
    matches!(s.get(sk(s, i)), None | Some(b'|' | b'}' | b']' | b')'))
}

fn digits_end(s: &[u8], i: usize) -> usize {
    i + s[i.min(s.len())..].iter().take_while(|b| b.is_ascii_digit()).count()
}

fn unum(s: &[u8], i: usize) -> Option<(Z, usize)> {
    let e = digits_end(s, i);
    (e > i).then(|| (Z::from_digits(&s[i..e]), e))
}

fn nat_at(s: &[u8], i: usize) -> Option<(Nat, usize)> {
    let j = sk(s, i);
    let e = digits_end(s, j);
    (e > j).then(|| (Nat::from_digits(&s[j..e]), e))
}

fn z_at(s: &[u8], i: usize) -> Option<(Z, usize)> {
    let j = sk(s, i);
    if s.get(j) == Some(&b'-') {
        let (z, k) = unum(s, j + 1)?;
        Some((z.neg(), k))
    } else {
        unum(s, j)
    }
}

fn many<T>(s: &[u8], mut i: usize, item: impl Fn(&[u8], usize) -> Option<(T, usize)>) -> Option<(Vec<T>, usize)> {
    let mut out = Vec::new();
    loop {
        if at_stop(s, i) {
            return Some((out, sk(s, i)));
        }
        let (x, j) = item(s, i)?;
        out.push(x);
        i = j;
    }
}

fn term_at(s: &[u8], i: usize) -> Option<((Var, Z), usize)> {
    let j = sk(s, i);
    let d = *s.get(j)?;
    let (n, k) = nat_at(s, j + 1)?;
    let k = tok(s, k, b':')?;
    let (a, k) = z_at(s, k)?;
    let v = match d {
        b's' => Var::S(n),
        b'b' => Var::B(n),
        b'c' => Var::C(n),
        _ => return None,
    };
    Some(((v, a), k))
}

fn lin_at(s: &[u8], i: usize) -> Option<(Lin, usize)> {
    let i = tok(s, i, b'[')?;
    let (terms, i) = many(s, i, term_at)?;
    let i = tok(s, i, b']')?;
    let (cst, i) = z_at(s, i)?;
    Some((Lin { terms, cst }, i))
}

fn mult_at(s: &[u8], i: usize) -> Option<((Nat, Z), usize)> {
    let (j, i) = nat_at(s, i)?;
    let i = tok(s, i, b':')?;
    let (k, i) = z_at(s, i)?;
    Some(((j, k), i))
}

fn pf_at(s: &[u8], i: usize) -> Option<(Pf, usize)> {
    let i = tok(s, i, b'(')?;
    let (ms, i) = many(s, i, mult_at)?;
    let i = tok(s, i, b')')?;
    let (mu, i) = z_at(s, i)?;
    Some(((ms, mu), i))
}

fn pf2_at(s: &[u8], i: usize) -> Option<((Pf, Pf), usize)> {
    let (a, i) = pf_at(s, i)?;
    let (b, i) = pf_at(s, i)?;
    Some(((a, b), i))
}

fn map_at(s: &[u8], i: usize) -> Option<((Nat, Nat), usize)> {
    let (a, i) = nat_at(s, i)?;
    let i = tok(s, i, b':')?;
    let (b, i) = nat_at(s, i)?;
    Some(((a, b), i))
}

fn arg_at(s: &[u8], i: usize) -> Option<(Arg, usize)> {
    let (p, i) = nat_at(s, i)?;
    let i = tok(s, i, b':')?;
    let (l, i) = lin_at(s, i)?;
    match tok(s, i, b'!') {
        Some(j) => {
            let (pp, j) = pf2_at(s, j)?;
            Some(((p, l, Some(pp)), j))
        }
        None => Some(((p, l, None), i)),
    }
}

fn pf3(s: &[u8], i: usize) -> Option<([Pf; 3], usize)> {
    let (a, i) = pf_at(s, i)?;
    let (b, i) = pf_at(s, i)?;
    let (c, i) = pf_at(s, i)?;
    Some(([a, b, c], i))
}

/// One straight-line item (no nested program) after its tag `c`, read
/// from `r`; `None` for a tag that is not one.
fn leaf_at(s: &[u8], c: u8, r: usize) -> Option<(Item, usize)> {
    Some(match c {
        b'A' | b'W' => {
            let (rg, k) = nat_at(s, r)?;
            let (w, k) = z_at(s, k)?;
            let (off, k) = z_at(s, k)?;
            let (addr, k) = lin_at(s, k)?;
            let (p, k) = pf3(s, k)?;
            (if c == b'A' { Item::Acc { r: rg, w, off, addr, p } } else { Item::Sto { r: rg, w, off, addr, p } }, k)
        }
        b'h' => {
            let (h, k) = nat_at(s, r)?;
            let (rg, k) = nat_at(s, k)?;
            (Item::Handle(h, rg), k)
        }
        b'n' => {
            let (n, k) = nat_at(s, r)?;
            let (rg, k) = nat_at(s, k)?;
            let (st, k) = z_at(s, k)?;
            (Item::Len(n, rg, st), k)
        }
        b'K' => {
            let (n, k) = nat_at(s, r)?;
            let (rg, k) = nat_at(s, k)?;
            (Item::Cap(n, rg), k)
        }
        b'a' => {
            let (h, k) = nat_at(s, r)?;
            let (rg, k) = nat_at(s, k)?;
            let (sz, k) = lin_at(s, k)?;
            let (lo, k) = pf_at(s, k)?;
            let (hi, k) = pf_at(s, k)?;
            (Item::Alloc { h, r: rg, sz, lo, hi }, k)
        }
        b'f' => {
            let (rg, k) = nat_at(s, r)?;
            (Item::Wf(rg), k)
        }
        b'g' => {
            let (rg, k) = nat_at(s, r)?;
            let (b0, k) = z_at(s, k)?;
            let (c0, k) = z_at(s, k)?;
            (Item::Static(rg, b0, c0), k)
        }
        b's' => {
            let (x, k) = nat_at(s, r)?;
            (Item::Set(x), k)
        }
        b'r' => {
            let (x, k) = nat_at(s, r)?;
            let (lo, k) = z_at(s, k)?;
            let (hi, k) = z_at(s, k)?;
            (Item::SetR(x, lo, hi), k)
        }
        b'=' | b'U' => {
            let (x, k) = nat_at(s, r)?;
            let (l, k) = lin_at(s, k)?;
            let (lo, k) = pf_at(s, k)?;
            let (hi, k) = pf_at(s, k)?;
            (if c == b'=' { Item::Asn { x, l, lo, hi } } else { Item::AsnU { x, l, lo, hi } }, k)
        }
        _ => return None,
    })
}

/// `lprog_at`: the items from `i` to the first stop (not consumed past the
/// skipped separators); a terminal item must end its sequence.
fn lprog_at(s: &[u8], mut i: usize) -> Option<(Vec<Item>, usize)> {
    let mut items = Vec::new();
    loop {
        if at_stop(s, i) {
            return Some((items, sk(s, i)));
        }
        let j = sk(s, i);
        let c = s[j];
        let r = j + 1;
        let (it, k) = match c {
            b'T' => {
                let g = sk(s, r);
                let sg = *s.get(g)?;
                if sg != b'+' && sg != b'u' {
                    return None;
                }
                let (a, k) = lin_at(s, g + 1)?;
                let (b, k) = lin_at(s, k)?;
                let (pa, k) = pf2_at(s, k)?;
                let (pb, k) = pf2_at(s, k)?;
                let (nn, k) = pf2_at(s, k)?;
                let k = tok(s, k, b'{')?;
                let (thn, k) = lprog_at(s, k)?;
                let k = tok(s, k, b'|')?;
                let (els, k) = lprog_at(s, k)?;
                let k = tok(s, k, b'}')?;
                (Item::Test { a, b, sgn: sg == b'+', pa, pb, nn, thn, els }, k)
            }
            b'B' => {
                let k = tok(s, r, b'{')?;
                let (thn, k) = lprog_at(s, k)?;
                let k = tok(s, k, b'|')?;
                let (els, k) = lprog_at(s, k)?;
                let k = tok(s, k, b'}')?;
                (Item::Branch(thn, els), k)
            }
            b'L' => {
                let k = tok(s, r, b'{')?;
                let (inv, k) = many(s, k, lin_at)?;
                let k = tok(s, k, b'|')?;
                let (pin, k) = many(s, k, pf_at)?;
                let k = tok(s, k, b'|')?;
                let (body, k) = lprog_at(s, k)?;
                let k = tok(s, k, b'}')?;
                (Item::Loop { inv, pin, body }, k)
            }
            b'c' | b'R' => {
                let k = tok(s, r, b'{')?;
                let (pfs, k) = many(s, k, pf_at)?;
                let k = tok(s, k, b'}')?;
                (if c == b'c' { Item::Cont(pfs) } else { Item::Ret(pfs) }, k)
            }
            b'k' => (Item::Brk, r),
            b'x' => (Item::Trap, r),
            b'C' => {
                let (f, k) = nat_at(s, r)?;
                let k = tok(s, k, b'{')?;
                let (args, k) = many(s, k, arg_at)?;
                let k = tok(s, k, b'|')?;
                let (reps, k) = many(s, k, map_at)?;
                let k = tok(s, k, b'|')?;
                let (outs, k) = many(s, k, map_at)?;
                let k = tok(s, k, b'|')?;
                let (res, k) = nat_at(s, k)?;
                let k = tok(s, k, b'|')?;
                let (pfs, k) = many(s, k, pf_at)?;
                let k = tok(s, k, b'}')?;
                (Item::Call { f, args, reps, outs, res, pfs }, k)
            }
            _ => leaf_at(s, c, r)?,
        };
        let terminal = matches!(it, Item::Cont(_) | Item::Ret(_) | Item::Brk | Item::Trap);
        items.push(it);
        i = k;
        if terminal {
            return at_stop(s, i).then(|| (items, sk(s, i)));
        }
    }
}

/// One procedure after `Q`: its id and its table entry; the index after the
/// closing `}` and any separators.
pub(crate) fn parse_lproc(s: &[u8], i: usize) -> Option<((Nat, Proc), usize)> {
    let (fid, i) = nat_at(s, i)?;
    let i = tok(s, i, b'{')?;
    let (pre, i) = many(s, i, lin_at)?;
    let i = tok(s, i, b'|')?;
    let (post, i) = many(s, i, lin_at)?;
    let i = tok(s, i, b'|')?;
    let (rslot, i) = nat_at(s, i)?;
    let i = tok(s, i, b'|')?;
    let (body, i) = lprog_at(s, i)?;
    let i = tok(s, i, b'}')?;
    Some(((fid, Proc { pre, post, rslot, body }), sk(s, i)))
}

/// Where a stretch starts, for the `Q` test (RoutineLinCert.v's
/// `lstretch_at` skips spaces and commas before it).
pub(crate) fn stretch_start(s: &[u8], i: usize) -> usize {
    sk(s, i)
}
