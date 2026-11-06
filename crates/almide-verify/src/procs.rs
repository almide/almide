//! The self-host procedure checker — the Rust mirror of `proofs/RoutineProcs.v`'s
//! `pcheck` / `procs_ok` through `proofs/RoutineProcsCert.v`'s byte format
//! (#4005 A4 items 4–5).
//!
//! A stretch of the program's `bounds` witness that begins with `P` is one
//! procedure: `P<id>{ groups | known | counts | nonneg | body }` (bounds.rs
//! hands it here). The witness's procedures form one table keyed by id; the
//! witness is accepted when no id repeats and every body is accepted under
//! its precondition, each call proving its callee's. Body items:
//!
//! - `hX,O,W` / `HX,O,W`, `aX,S,O,W` / `wX,S,O,W`, `nN,X,S` — as RoutineBounds;
//! - `cX,N,S` / `CX,N,S` — the unsigned / signed test; `eX,N,S` / `EX,N,S` — a
//!   loop's exit test, unsigned / signed;
//! - `sX`, `kX,C` (X := C), `tX,Y,C,S` (X := Y + C);
//! - `L{ … }`, `B{ … | … }`, `F<id>{ P:X … }` (a call, callee slot : caller slot).
//!
//! Coq's program is a continuation chain; here it is the flat item list
//! walked with a loop (an exit's state met with the sequence's final one, as
//! rbounds.rs does), every number a normalized decimal (`Nat`).

use crate::nat::Nat;
use std::collections::BTreeSet;

type Fact = (Nat, Nat); // (slot, stride)
type Count = (Nat, Nat, Nat); // (count, representative slot, stride)

#[derive(Debug, PartialEq)]
pub(crate) enum Item {
    HdrOk(Nat, Nat),
    Acc(Fact, Nat, Nat),
    Len(Nat, Nat, Nat),
    Check(Fact, Nat, bool),
    Exit(Fact, Nat, bool),
    Set(Nat),
    Const(Nat),
    Step(Nat, Fact, Nat),
    Loop(Vec<Item>),
    Branch(Vec<Item>, Vec<Item>),
    Call(Nat, Vec<(Nat, Nat)>),
}

/// One procedure: groups (slot, representative), known, counts (count,
/// parameter slot, stride), not-negative slots, body.
#[derive(Debug, PartialEq)]
pub(crate) struct Proc {
    groups: Vec<(Nat, Nat)>,
    known: Vec<Fact>,
    counts: Vec<Count>,
    nonneg: Vec<Nat>,
    body: Vec<Item>,
}

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\n' | b'\r' | b'\t')
}

fn skip(s: &[u8], mut i: usize) -> usize {
    while s.get(i).is_some_and(|&b| is_space(b)) {
        i += 1;
    }
    i
}

fn number(s: &[u8], i: usize) -> Option<(Nat, usize)> {
    let end = i + s.get(i..)?.iter().take_while(|b| b.is_ascii_digit()).count();
    (end > i).then(|| (Nat::from_digits(&s[i..end]), end))
}

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

/// `expect c`: whitespace, then `c`.
fn expect(s: &[u8], i: usize, c: u8) -> Option<usize> {
    let i = skip(s, i);
    (s.get(i) == Some(&c)).then_some(i + 1)
}

/// `seq_at item`: items up to the first `|` or `}` (not consumed).
fn seq_at<T>(s: &[u8], mut i: usize, item: impl Fn(&[u8], usize) -> Option<(T, usize)>) -> Option<(Vec<T>, usize)> {
    let mut out = Vec::new();
    loop {
        let &c = s.get(i)?;
        if c == b'|' || c == b'}' {
            return Some((out, i));
        }
        if is_space(c) {
            i += 1;
            continue;
        }
        let (x, e) = item(s, i)?;
        out.push(x);
        i = e;
    }
}

fn pair_item(s: &[u8], i: usize) -> Option<((Nat, Nat), usize)> {
    let (x, e) = number(s, i)?;
    if s.get(e) != Some(&b':') {
        return None;
    }
    let (y, e2) = number(s, e + 1)?;
    Some(((x, y), e2))
}

fn triple_item(s: &[u8], i: usize) -> Option<(Count, usize)> {
    let (v, e) = nums(s, i, 3)?;
    let [a, b, c]: [Nat; 3] = v.try_into().ok()?;
    Some(((a, b, c), e))
}

/// `pprog_at`: the items from `i` to the first `}`, `|` or `;` (not
/// consumed) or the end.
fn pprog_at(s: &[u8], mut i: usize) -> Option<(Vec<Item>, usize)> {
    let mut items = Vec::new();
    loop {
        let Some(&c) = s.get(i) else { return Some((items, i)) };
        match c {
            b'}' | b'|' | b';' => return Some((items, i)),
            _ if is_space(c) => i += 1,
            b'h' | b'H' | b'n' | b'c' | b'C' | b'e' | b'E' => {
                let (v, e) = nums(s, i + 1, 3)?;
                let [a, b, d]: [Nat; 3] = v.try_into().ok()?;
                items.push(match c {
                    b'h' | b'H' => Item::HdrOk(b, d),
                    b'n' => Item::Len(a, b, d),
                    b'c' => Item::Check((a, d), b, false),
                    b'C' => Item::Check((a, d), b, true),
                    b'e' => Item::Exit((a, d), b, false),
                    _ => Item::Exit((a, d), b, true),
                });
                i = e;
            }
            b'a' | b'w' | b't' => {
                let (v, e) = nums(s, i + 1, 4)?;
                let [x, y, o, w]: [Nat; 4] = v.try_into().ok()?;
                items.push(match c {
                    b't' => Item::Step(x, (y, w), o),
                    _ => Item::Acc((x, y), o, w),
                });
                i = e;
            }
            b'k' => {
                let (v, e) = nums(s, i + 1, 2)?;
                let [x, _]: [Nat; 2] = v.try_into().ok()?;
                items.push(Item::Const(x));
                i = e;
            }
            b's' => {
                let (x, e) = number(s, i + 1)?;
                items.push(Item::Set(x));
                i = e;
            }
            b'L' => {
                let j = expect(s, i + 1, b'{')?;
                let (body, k) = pprog_at(s, j)?;
                if s.get(k) != Some(&b'}') {
                    return None;
                }
                items.push(Item::Loop(body));
                i = k + 1;
            }
            b'B' => {
                let j = expect(s, i + 1, b'{')?;
                let (thn, k) = pprog_at(s, j)?;
                if s.get(k) != Some(&b'|') {
                    return None;
                }
                let (els, l) = pprog_at(s, k + 1)?;
                if s.get(l) != Some(&b'}') {
                    return None;
                }
                items.push(Item::Branch(thn, els));
                i = l + 1;
            }
            b'F' => {
                let (g, e) = number(s, i + 1)?;
                let j = expect(s, e, b'{')?;
                let (args, k) = seq_at(s, j, pair_item)?;
                if s.get(k) != Some(&b'}') {
                    return None;
                }
                items.push(Item::Call(g, args));
                i = k + 1;
            }
            _ => return None,
        }
    }
}

/// One procedure after its `P`: its id, its entry, and the index past it
/// (trailing whitespace skipped).
pub(crate) fn parse_proc(s: &[u8], i: usize) -> Option<((Nat, Proc), usize)> {
    let (id, e) = number(s, i)?;
    let i = expect(s, e, b'{')?;
    let (groups, i) = seq_at(s, i, pair_item)?;
    let i = (s.get(i) == Some(&b'|')).then_some(i + 1)?;
    let (known, i) = seq_at(s, i, pair_item)?;
    let i = (s.get(i) == Some(&b'|')).then_some(i + 1)?;
    let (counts, i) = seq_at(s, i, triple_item)?;
    let i = (s.get(i) == Some(&b'|')).then_some(i + 1)?;
    let (nonneg, i) = seq_at(s, i, number)?;
    let i = (s.get(i) == Some(&b'|')).then_some(i + 1)?;
    let (body, i) = pprog_at(s, i)?;
    if s.get(i) != Some(&b'}') {
        return None;
    }
    Some(((id, Proc { groups, known, counts, nonneg, body }), skip(s, i + 1)))
}

fn zero() -> Nat {
    Nat::from_digits(b"0")
}

fn positive(n: &Nat) -> bool {
    *n > zero()
}

fn in_slot(st: &Nat, o: &Nat, w: &Nat) -> bool {
    positive(st) && positive(w) && o.add(w) <= *st
}

fn in_hdr(o: &Nat, w: &Nat) -> bool {
    positive(w) && o.add(w) <= Nat::from_digits(b"12")
}

/// `gof`: the slot's representative (its first group pair), else itself.
fn gof(gs: &[(Nat, Nat)], x: &Nat) -> Nat {
    gs.iter().find(|(a, _)| a == x).map_or_else(|| x.clone(), |(_, r)| r.clone())
}

fn amap<'a>(args: &'a [(Nat, Nat)], p: &Nat) -> Option<&'a Nat> {
    args.iter().find(|(a, _)| a == p).map(|(_, x)| x)
}

fn kills(items: &[Item], out: &mut BTreeSet<Nat>) {
    for it in items {
        match it {
            Item::Set(x) | Item::Const(x) | Item::Step(x, _, _) | Item::Len(x, _, _) => {
                out.insert(x.clone());
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

#[derive(Clone, Default)]
struct St {
    kn: BTreeSet<Fact>,
    cn: BTreeSet<Count>,
    nn: BTreeSet<Nat>,
}

impl St {
    fn kill(&mut self, y: &Nat) {
        self.kn.retain(|f| f.0 != *y);
        self.cn.retain(|c| c.0 != *y);
        self.nn.remove(y);
    }

    fn minus(&self, ks: &BTreeSet<Nat>) -> St {
        St {
            kn: self.kn.iter().filter(|f| !ks.contains(&f.0)).cloned().collect(),
            cn: self.cn.iter().filter(|c| !ks.contains(&c.0)).cloned().collect(),
            nn: self.nn.iter().filter(|x| !ks.contains(x)).cloned().collect(),
        }
    }

    fn meet(&self, o: &St) -> St {
        St {
            kn: self.kn.intersection(&o.kn).cloned().collect(),
            cn: self.cn.intersection(&o.cn).cloned().collect(),
            nn: self.nn.intersection(&o.nn).cloned().collect(),
        }
    }

    fn within(&self, o: &St) -> bool {
        self.kn.is_subset(&o.kn) && self.cn.is_subset(&o.cn) && self.nn.is_subset(&o.nn)
    }

    fn nonneg(&self, x: &Nat) -> bool {
        self.nn.contains(x) || self.kn.iter().any(|f| f.0 == *x)
    }
}

type Table = [(Nat, Proc)];

fn plook<'a>(p: &'a Table, f: &Nat) -> Option<&'a Proc> {
    p.iter().find(|(id, _)| id == f).map(|(_, d)| d)
}

/// `pre_met`: the caller (groups `gs`, state `st`) proves the callee's
/// precondition of the slots it passes.
fn pre_met(gs: &[(Nat, Nat)], st: &St, args: &[(Nat, Nat)], callee: &Proc) -> bool {
    let agree = args.iter().all(|a1| {
        args.iter().all(|a2| gof(&callee.groups, &a1.0) != gof(&callee.groups, &a2.0) || gof(gs, &a1.1) == gof(gs, &a2.1))
    });
    agree
        && callee.known.iter().all(|(p, s)| amap(args, p).is_some_and(|x| st.kn.contains(&(x.clone(), s.clone()))))
        && callee.counts.iter().all(|(n, r, s)| match (amap(args, n), amap(args, r)) {
            (Some(n2), Some(x)) => st.cn.contains(&(n2.clone(), gof(gs, x), s.clone())),
            _ => false,
        })
        && callee.nonneg.iter().all(|y| amap(args, y).is_some_and(|x| st.nonneg(x)))
}

/// `pcheck`: the state after `items`, or `None` when an access is not
/// justified.
fn pcheck(p: &Table, gs: &[(Nat, Nat)], mut st: St, items: &[Item]) -> Option<St> {
    let mut left: Vec<St> = Vec::new();
    for it in items {
        match it {
            Item::HdrOk(o, w) => {
                if !in_hdr(o, w) {
                    return None;
                }
            }
            Item::Acc(f, o, w) => {
                if !st.kn.contains(f) || !in_slot(&f.1, o, w) {
                    return None;
                }
            }
            Item::Len(n, x, s) => {
                if !positive(s) {
                    return None;
                }
                st.kill(n);
                st.cn.insert((n.clone(), gof(gs, x), s.clone()));
                st.nn.insert(n.clone());
            }
            Item::Check(f, n, g) | Item::Exit(f, n, g) => {
                if !st.cn.contains(&(n.clone(), gof(gs, &f.0), f.1.clone())) || (*g && !st.nonneg(&f.0)) {
                    return None;
                }
                if matches!(it, Item::Exit(..)) {
                    left.push(st.clone());
                }
                st.kn.insert(f.clone());
            }
            Item::Set(x) => st.kill(x),
            Item::Const(x) => {
                st.kill(x);
                st.nn.insert(x.clone());
            }
            Item::Step(x, (y, s), c) => {
                let ok = st.kn.contains(&(y.clone(), s.clone())) && *c <= *s;
                st.kill(x);
                if ok {
                    st.nn.insert(x.clone());
                }
            }
            Item::Loop(body) => {
                let mut ks = BTreeSet::new();
                kills(body, &mut ks);
                let s0 = st.minus(&ks);
                let sb = pcheck(p, gs, s0.clone(), body)?;
                if !s0.within(&sb) {
                    return None;
                }
                st = s0;
            }
            Item::Branch(t, e) => {
                let a = pcheck(p, gs, st.clone(), t)?;
                let b = pcheck(p, gs, st, e)?;
                st = a.meet(&b);
            }
            Item::Call(f, args) => {
                let callee = plook(p, f)?;
                if !pre_met(gs, &st, args, callee) {
                    return None;
                }
            }
        }
    }
    for l in left {
        st = st.meet(&l);
    }
    Some(st)
}

/// The witness's procedures accepted: no id twice, every body under its
/// precondition (`nodup_ids` and `procs_ok`).
pub(crate) fn procs_ok(p: &Table) -> bool {
    let ids: BTreeSet<&Nat> = p.iter().map(|(id, _)| id).collect();
    ids.len() == p.len()
        && p.iter().all(|(_, d)| {
            let st = St {
                kn: d.known.iter().cloned().collect(),
                cn: d.counts.iter().cloned().collect(),
                nn: d.nonneg.iter().cloned().collect(),
            };
            pcheck(p, &d.groups, st, &d.body).is_some()
        })
}

#[cfg(test)]
mod tests {
    use crate::bounds::check_bounds_cert;

    const COPY: &str = "P7{ 3:2 | | 1,2,8 | 2 | B{ | C2,1,8 a2,8,0,8 t3,2,1,8 F7{1:1 2:3} } }";

    // The Examples of proofs/RoutineProcsCert.v, verdict for verdict.
    #[test]
    fn the_coq_examples() {
        assert!(check_bounds_cert(b"c0 a0 c1 a1 w0 w1"));
        assert!(!check_bounds_cert(b"c0 s0 a0"));
        assert!(check_bounds_cert(b"R n1,0,1 L{ e0,1,1 a0,1,0,1 s0 }"));
        assert!(!check_bounds_cert(b"R a0,1,0,1"));
        assert!(check_bounds_cert(COPY.as_bytes()));
        assert!(check_bounds_cert(format!("c0 a0 ; {COPY}").as_bytes()));
        assert!(!check_bounds_cert(b"P7{ 3:2 | | 1,2,8 | | B{ | C2,1,8 a2,8,0,8 t3,2,1,8 F7{1:1 2:3} } }"));
        assert!(!check_bounds_cert(b"P7{ | | | | F8{} }"));
        assert!(!check_bounds_cert(b"P7{ | | | | } ; P7{ | | | | }"));
        assert!(!check_bounds_cert(format!("R a0,1,0,1 ; {COPY}").as_bytes()));
        assert!(!check_bounds_cert(b"P7{ 3:2 | | 1,2 | | }"));
        assert!(!check_bounds_cert(b"P{ | | | | }"));
        assert!(!check_bounds_cert(b"P7{ | | | | B{ }"));
    }

    // RoutineProcs.v's controls, through the byte format.
    #[test]
    fn the_routine_procs_controls() {
        // the step past its stride: the recursive call's not-negative fact fails
        assert!(!check_bounds_cert(b"P0{ 3:2 | | 1,2,8 | 2 | B{ | C2,1,8 a2,8,0,8 t3,2,16,8 F0{1:1 2:3} } }"));
        // a count the caller does not have
        assert!(!check_bounds_cert(b"P0{ 3:2 | | 1,2,8 | 2 | B{ | C2,1,8 a2,8,0,8 t3,2,1,8 F0{1:4 2:3} } }"));
        // the step's slot outside the counter's group
        assert!(!check_bounds_cert(b"P0{ | | 1,2,8 | 2 | B{ | C2,1,8 a2,8,0,8 t3,2,1,8 F0{1:1 2:3} } }"));
        // a procedure reading with no test
        assert!(!check_bounds_cert(b"P0{ | | | | a0,8,0,8 }"));
        // a header read needs nothing
        assert!(check_bounds_cert(b"P0{ | | | | h0,4,4 }"));
        // groups agree / disagree (the caller is itself a procedure here)
        let callee = "P0{ 4:2 | | 1,2,8 | 4 | C4,1,8 a4,8,0,8 }";
        assert!(check_bounds_cert(format!("{callee} ; P1{{ 5:2 | | | | n1,2,8 k5,0 F0{{1:1 2:2 4:5}} }}").as_bytes()));
        assert!(!check_bounds_cert(format!("{callee} ; P1{{ | | | | n1,2,8 k5,0 F0{{1:1 2:2 4:5}} }}").as_bytes()));
    }
}
