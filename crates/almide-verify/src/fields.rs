//! The fields checker — the Rust mirror of `proofs/FieldCert.v`'s
//! `check_fields_cert` (#4013, #4279).
//!
//! The witness is `;`-separated items:
//!
//! - `L R c C n N z S (o O w W)*` — the layout of kind (R, C): N cases, a
//!   payload of S bytes, field i at offset O_i, W_i bytes wide;
//! - `k R c C (s I)*` — a construction site of kind (R, C) storing fields I;
//! - `a R c C f I` — a load or store of field I of kind (R, C);
//! - `t R` — a read of the tag word of a block of representation R.
//!
//! Whitespace between tokens is ignored, but a tag touches its digits. A
//! string that does not parse is rejected; the empty string is vacuous. It
//! is accepted iff no two layouts name one kind, every layout's fields fit
//! its payload (`0 < W` and `O + W <= S`) and its case is below N, every
//! site's kind has a layout and the site stores each of its fields and none
//! past them, every access names a declared field of a laid-out kind, and
//! every tag read's representation has a layout, each of whose cases starts
//! with the tag word `(0, 4)`.

use crate::nat::Nat;

struct Shape {
    sz: Nat,
    fld: Vec<(Nat, Nat)>,
}

enum Item {
    Lay { r: Nat, c: Nat, n: Nat, s: Shape },
    Site { r: Nat, c: Nat, st: Vec<Nat> },
    Acc { r: Nat, c: Nat, f: Nat },
    Tag { r: Nat },
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

/// After blanks: the end, or a `;` (`at_end`); the position there.
fn at_end(s: &[u8], i: usize) -> Option<usize> {
    let j = skip_blank(s, i);
    match s.get(j) {
        None | Some(b';') => Some(j),
        _ => None,
    }
}

/// `tag N` tokens up to the end or a `;` (`stores_at` / `pairs_at`).
fn repeated(s: &[u8], mut i: usize, tags: &[u8]) -> Option<(Vec<Vec<Nat>>, usize)> {
    let mut out = Vec::new();
    loop {
        if let Some(j) = at_end(s, i) {
            return Some((out, j));
        }
        let mut one = Vec::new();
        for &t in tags {
            let (v, k) = field(s, i, t)?;
            one.push(v);
            i = k;
        }
        out.push(one);
    }
}

fn item_at(s: &[u8], i: usize) -> Option<(Item, usize)> {
    if let Some((r, i)) = field(s, i, b'L') {
        let (c, i) = field(s, i, b'c')?;
        let (n, i) = field(s, i, b'n')?;
        let (sz, i) = field(s, i, b'z')?;
        let (ps, j) = repeated(s, i, b"ow")?;
        let fld = ps.into_iter().map(|mut p| (p.remove(0), p.remove(0))).collect();
        return Some((Item::Lay { r, c, n, s: Shape { sz, fld } }, j));
    }
    if let Some((r, i)) = field(s, i, b'k') {
        let (c, i) = field(s, i, b'c')?;
        let (st, j) = repeated(s, i, b"s")?;
        return Some((Item::Site { r, c, st: st.into_iter().flatten().collect() }, j));
    }
    if let Some((r, i)) = field(s, i, b'a') {
        let (c, i) = field(s, i, b'c')?;
        let (f, i) = field(s, i, b'f')?;
        return Some((Item::Acc { r, c, f }, at_end(s, i)?));
    }
    let (r, i) = field(s, i, b't')?;
    Some((Item::Tag { r }, at_end(s, i)?))
}

fn parse(s: &[u8]) -> Option<Vec<Item>> {
    let mut out = Vec::new();
    let mut i = 0;
    loop {
        if skip_blank(s, i) == s.len() {
            return Some(out);
        }
        let (it, j) = item_at(s, i)?;
        out.push(it);
        match s.get(j) {
            None => return Some(out),
            Some(b';') => i = j + 1,
            Some(_) => return None,
        }
    }
}

fn shape_ok(s: &Shape) -> bool {
    let zero = Nat::from_digits(b"");
    s.fld.iter().all(|(o, w)| *w > zero && o.add(w) <= s.sz)
}

fn below(i: &Nat, len: usize) -> bool {
    i.as_index().is_some_and(|x| x < len)
}

/// `check_fields_cert`: the bytes parse, no kind is laid out twice, and
/// every item is accepted against the layout table.
pub(crate) fn check_fields_cert(witness: &[u8]) -> bool {
    let Some(items) = parse(witness) else { return false };
    let table: Vec<(&Nat, &Nat, &Shape)> = items
        .iter()
        .filter_map(|it| match it {
            Item::Lay { r, c, s, .. } => Some((r, c, s)),
            _ => None,
        })
        .collect();
    for (k, a) in table.iter().enumerate() {
        if table[k + 1..].iter().any(|b| b.0 == a.0 && b.1 == a.1) {
            return false;
        }
    }
    let lookup = |r: &Nat, c: &Nat| table.iter().find(|e| e.0 == r && e.1 == c).map(|e| e.2);
    let four = Nat::from_digits(b"4");
    let zero = Nat::from_digits(b"");
    items.iter().all(|it| match it {
        Item::Lay { c, n, s, .. } => shape_ok(s) && c < n,
        Item::Site { r, c, st } => lookup(r, c).is_some_and(|s| {
            let len = s.fld.len();
            (0..len).all(|i| st.iter().any(|x| x.as_index() == Some(i))) && st.iter().all(|x| below(x, len))
        }),
        Item::Acc { r, c, f } => lookup(r, c).is_some_and(|s| below(f, s.fld.len())),
        Item::Tag { r } => {
            table.iter().any(|e| e.0 == r)
                && table
                    .iter()
                    .filter(|e| e.0 == r)
                    .all(|e| e.2.fld.first().is_some_and(|(o, w)| *o == zero && *w == four))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_coq_examples() {
        assert!(check_fields_cert(b"L4 c0 n1 z16 o0 w8 o8 w4 ; k4 c0 s0 s1 ; a4 c0 f1"));
        assert!(check_fields_cert(b"L6 c0 n2 z8 o0 w4 ; L6 c1 n2 z16 o0 w4 o8 w8 ; k6 c1 s0 s1 ; t6 ; a6 c1 f1"));
        assert!(check_fields_cert(b""));
        assert!(!check_fields_cert(b"L4 c0 n1 z16 o0 w8 o8 w4 ; k4 c0 s0"));
        assert!(!check_fields_cert(b"L4 c0 n1 z12 o0 w8 o8 w8"));
        assert!(!check_fields_cert(b"L4 c0 n1 z16 o0 w8 o8 w4 ; a4 c0 f2"));
        assert!(!check_fields_cert(b"L4 c0 n1 z16 o0 w8 o8 w4 ; a5 c0 f0"));
        assert!(!check_fields_cert(b"L6 c0 n1 z8 o0 w8 ; t6"));
        assert!(!check_fields_cert(b"L4 c1 n1 z8"));
        assert!(!check_fields_cert(b"L4 c0 n1 z8 o0 w8 ; L4 c0 n1 z16 o0 w8"));
        assert!(!check_fields_cert(b"L4 c0 n1 z8 o0"));
        assert!(!check_fields_cert(b"a4 c0 f0 x"));
        assert!(!check_fields_cert(b"k4 s0"));
        assert!(!check_fields_cert(b";"));
    }

    #[test]
    fn edges() {
        // A trailing `;`, leading zeros, any order of stores, a store twice.
        assert!(check_fields_cert(b"L4 c0 n1 z8 o0 w8 ; k4 c0 s0 ;"));
        assert!(check_fields_cert(b"L004 c0 n01 z0016 o0 w8 o08 w4 ; k4 c0 s1 s0 s1"));
        // A zero-width field, a field ending exactly at the payload's end.
        assert!(!check_fields_cert(b"L4 c0 n1 z8 o0 w0"));
        assert!(check_fields_cert(b"L4 c0 n1 z8 o4 w4"));
        // A site of a kind with no layout; an empty site of an empty layout.
        assert!(!check_fields_cert(b"k4 c0"));
        assert!(check_fields_cert(b"L4 c0 n1 z0 ; k4 c0"));
        // A tag read with no layout of the representation.
        assert!(!check_fields_cert(b"L4 c0 n1 z8 o0 w4 ; t6"));
        // Offsets past any machine word.
        assert!(!check_fields_cert(b"L4 c0 n1 z8 o99999999999999999999999 w8"));
        // A tag apart from its digits.
        assert!(!check_fields_cert(b"L 4 c0 n1 z8"));
    }
}
