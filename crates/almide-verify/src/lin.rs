//! The linear-facts procedure checker — the Rust mirror of `proofs/RoutineLin.v`'s
//! `lcheck` / `procs_ok` through `proofs/RoutineLinCert.v`'s byte format
//! (#4005 A4).
//!
//! A stretch of the program's `bounds` witness that begins with `Q` is one
//! procedure: `Q<id>{ pre | post | result-slot | body }` (bounds.rs hands it
//! here; the grammar is RoutineLinCert.v's header). The witness's `Q`
//! procedures form one table keyed by id, accepted when no id repeats and
//! every body is accepted under its precondition.
//!
//! Facts are linear forms `Σ a·v + c` (meaning `<= 0`) kept EXACTLY as Coq
//! builds them — the same terms in the same order — because a certificate's
//! proofs name facts by their index in the list and joins compare facts
//! syntactically. Integers are unbounded (zint.rs), slot ids normalized
//! decimals (`Nat`), so no overflow can make the two verdicts disagree. Coq's
//! program is a continuation chain; here it is the flat item list walked
//! with a loop, nesting only into arms and loop bodies.

use crate::nat::Nat;
use crate::zint::Z;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Var {
    S(Nat),
    B(Nat),
    C(Nat),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Lin {
    terms: Vec<(Var, Z)>,
    cst: Z,
}

type Pf = (Vec<(Nat, Z)>, Z);
/// A call argument: parameter, value, no-wrap proofs when a contract names it.
type Arg = (Nat, Lin, Option<(Pf, Pf)>);
type Facts = Vec<Lin>;
type St = Option<Facts>;

#[derive(Debug)]
pub(crate) enum Item {
    Acc { r: Nat, w: Z, off: Z, addr: Lin, p: [Pf; 3] },
    Sto { r: Nat, w: Z, off: Z, addr: Lin, p: [Pf; 3] },
    Handle(Nat, Nat),
    Len(Nat, Nat, Z),
    /// `LCap n r`: n := r's capacity (the cap word).
    Cap(Nat, Nat),
    Alloc { h: Nat, r: Nat, sz: Lin, lo: Pf, hi: Pf },
    Wf(Nat),
    Static(Nat, Z, Z),
    Set(Nat),
    SetR(Nat, Z, Z),
    Asn { x: Nat, l: Lin, lo: Pf, hi: Pf },
    /// `LAsnU x l lo hi`: x := l mod 2^32 (an i32 result).
    AsnU { x: Nat, l: Lin, lo: Pf, hi: Pf },
    Test { a: Lin, b: Lin, sgn: bool, pa: (Pf, Pf), pb: (Pf, Pf), nn: (Pf, Pf), thn: Vec<Item>, els: Vec<Item> },
    Branch(Vec<Item>, Vec<Item>),
    Loop { inv: Facts, pin: Vec<Pf>, body: Vec<Item> },
    Cont(Vec<Pf>),
    Brk,
    Trap,
    Call { f: Nat, args: Vec<Arg>, reps: Vec<(Nat, Nat)>, outs: Vec<(Nat, Nat)>, res: Nat, pfs: Vec<Pf> },
    Ret(Vec<Pf>),
}

#[derive(Debug)]
pub(crate) struct Proc {
    pre: Facts,
    post: Facts,
    rslot: Nat,
    body: Vec<Item>,
}

// ── The byte format (RoutineLinCert.v): lin_parse.rs ───────────────────

#[path = "lin_parse.rs"]
mod parse;
pub(crate) use parse::{parse_lproc, stretch_start};

// ── Term algebra, exactly as RoutineLin.v builds it ─────────────────────

fn one() -> Z {
    Z::from_i64(1)
}

fn v1(v: Var) -> Lin {
    Lin { terms: vec![(v, one())], cst: Z::zero() }
}

fn kz(c: Z) -> Lin {
    Lin { terms: Vec::new(), cst: c }
}

fn scale(k: &Z, ts: &[(Var, Z)]) -> Vec<(Var, Z)> {
    ts.iter().map(|(v, a)| (v.clone(), k.mul(a))).collect()
}

fn ladd(a: &Lin, b: &Lin) -> Lin {
    let mut terms = a.terms.clone();
    terms.extend(b.terms.iter().cloned());
    Lin { terms, cst: a.cst.add(&b.cst) }
}

fn lneg(a: &Lin) -> Lin {
    Lin { terms: scale(&Z::from_i64(-1), &a.terms), cst: a.cst.neg() }
}

fn lsub(a: &Lin, b: &Lin) -> Lin {
    ladd(a, &lneg(b))
}

fn lmul(k: &Z, a: &Lin) -> Lin {
    Lin { terms: scale(k, &a.terms), cst: k.mul(&a.cst) }
}

fn coef(ts: &[(Var, Z)], v: &Var) -> Z {
    ts.iter().filter(|(u, _)| u == v).fold(Z::zero(), |acc, (_, a)| a.add(&acc))
}

fn mentions(v: &Var, l: &Lin) -> bool {
    l.terms.iter().any(|(u, _)| u == v)
}

fn kill(f: &[Lin], v: &Var) -> Facts {
    f.iter().filter(|l| !mentions(v, l)).cloned().collect()
}

fn kill_region(f: &[Lin], r: &Nat) -> Facts {
    kill(&kill(f, &Var::B(r.clone())), &Var::C(r.clone()))
}

fn eqs(v: &Var, l: &Lin) -> Facts {
    vec![lsub(&v1(v.clone()), l), lsub(l, &v1(v.clone()))]
}

fn comb(f: &[Lin], ps: &[(Nat, Z)]) -> Option<(Vec<(Var, Z)>, Z)> {
    // Right fold: the first pair's terms come first.
    let mut ts: Vec<(Var, Z)> = Vec::new();
    let mut c = Z::zero();
    for (j, k) in ps.iter().rev() {
        let fact = f.get(j.as_index()?)?;
        if k.is_neg() {
            return None;
        }
        let mut t = scale(k, &fact.terms);
        t.extend(ts);
        ts = t;
        c = k.mul(&fact.cst).add(&c);
    }
    Some((ts, c))
}

fn entails(f: &[Lin], pf: &Pf, t: &Lin) -> bool {
    let (ps, mu) = pf;
    if !mu.is_pos() {
        return false;
    }
    let Some((ts, c)) = comb(f, ps) else { return false };
    let all = ts.iter().chain(t.terms.iter()).all(|(v, _)| coef(&ts, v) == mu.mul(&coef(&t.terms, v)));
    all && mu.mul(&t.cst) <= c
}

fn i64min() -> Z {
    Z::pow2(63).neg()
}

fn i64max() -> Z {
    Z::pow2(63).sub(&one())
}

fn no_wrap(f: &[Lin], l: &Lin, pf: &(Pf, Pf)) -> bool {
    if l.terms.len() == 1 && matches!(l.terms[0].0, Var::S(_)) && l.terms[0].1 == one() && l.cst == Z::zero() {
        return true;
    }
    entails(f, &pf.0, &lsub(&kz(i64min()), l)) && entails(f, &pf.1, &lsub(l, &kz(i64max())))
}

fn fjoin(a: &St, b: &St) -> St {
    match (a, b) {
        (None, x) => x.clone(),
        (x, None) => x.clone(),
        (Some(fa), Some(fb)) => Some(fa.iter().filter(|l| fb.contains(l)).cloned().collect()),
    }
}

fn proves(f: &[Lin], g: &[Lin], pfs: &[Pf]) -> bool {
    g.len() == pfs.len() && g.iter().zip(pfs).all(|(l, pf)| f.contains(l) || entails(f, pf, l))
}

fn acc_ok(f: &[Lin], r: &Nat, w: &Z, off: &Z, addr: &Lin, p: &[Pf; 3]) -> bool {
    let vb = v1(Var::B(r.clone()));
    let vc = v1(Var::C(r.clone()));
    w.is_pos()
        && !off.is_neg()
        && entails(f, &p[0], &lneg(addr))
        && entails(f, &p[1], &lsub(&vb, &ladd(addr, &kz(off.clone()))))
        && entails(f, &p[2], &lsub(&ladd(addr, &kz(off.add(w).sub(&Z::from_i64(12)))), &ladd(&vb, &vc)))
}

fn dropv(v: &Var, ts: &[(Var, Z)]) -> Vec<(Var, Z)> {
    ts.iter().filter(|(u, _)| u != v).cloned().collect()
}

fn subst(v: &Var, d: &Lin, f: &Lin) -> Lin {
    let mut terms = Vec::new();
    for (u, a) in &f.terms {
        if u == v {
            terms.extend(scale(a, &d.terms));
        } else {
            terms.push((u.clone(), a.clone()));
        }
    }
    Lin { terms, cst: f.cst.add(&coef(&f.terms, v).mul(&d.cst)) }
}

fn asn_facts(f: &[Lin], x: &Nat, l: &Lin, lo: &Pf, hi: &Pf) -> Facts {
    let vx = Var::S(x.clone());
    if !no_wrap(f, l, &(lo.clone(), hi.clone())) {
        return kill(f, &vx);
    }
    let c = coef(&l.terms, &vx);
    if c == Z::zero() {
        let mut out = eqs(&vx, l);
        out.extend(kill(f, &vx));
        out
    } else if c == one() {
        let d = lsub(&v1(vx.clone()), &Lin { terms: dropv(&vx, &l.terms), cst: l.cst.clone() });
        f.iter().map(|g| subst(&vx, &d, g)).collect()
    } else {
        kill(f, &vx)
    }
}

// ── Calls ──────────────────────────────────────────────────────────────

fn assoc<'a, T>(k: &Nat, l: &'a [(Nat, T)]) -> Option<&'a T> {
    l.iter().find(|(k2, _)| k2 == k).map(|(_, a)| a)
}

fn tr_lin(tr: &dyn Fn(&Var) -> Option<Lin>, l: &Lin) -> Option<Lin> {
    // tr_terms is a right fold: Some (kz 0) at the end, each term
    // `ladd (lmul a d) rest`.
    let mut ds = Vec::with_capacity(l.terms.len());
    for (v, a) in &l.terms {
        ds.push((a.clone(), tr(v)?));
    }
    let mut m = kz(Z::zero());
    for (a, d) in ds.iter().rev() {
        m = ladd(&lmul(a, d), &m);
    }
    Some(ladd(&m, &kz(l.cst.clone())))
}

fn writes(items: &[Item], out: &mut Vec<Nat>) {
    for it in items {
        match it {
            Item::Handle(h, _) | Item::Len(h, _, _) | Item::Cap(h, _) | Item::Set(h) | Item::SetR(h, _, _) => out.push(h.clone()),
            Item::Alloc { h, .. } | Item::Asn { x: h, .. } | Item::AsnU { x: h, .. } => out.push(h.clone()),
            Item::Call { res, .. } => out.push(res.clone()),
            Item::Test { thn, els, .. } | Item::Branch(thn, els) => {
                writes(thn, out);
                writes(els, out);
            }
            Item::Loop { body, .. } => writes(body, out),
            _ => {}
        }
    }
}

fn rebinds(items: &[Item], out: &mut Vec<Nat>) {
    for it in items {
        match it {
            Item::Alloc { r, .. } | Item::Static(r, _, _) => out.push(r.clone()),
            Item::Call { outs, .. } => out.extend(outs.iter().map(|q| q.0.clone())),
            Item::Test { thn, els, .. } | Item::Branch(thn, els) => {
                rebinds(thn, out);
                rebinds(els, out);
            }
            Item::Loop { body, .. } => rebinds(body, out),
            _ => {}
        }
    }
}

type Table = [(Nat, Proc)];

fn plook<'a>(p: &'a Table, f: &Nat) -> Option<&'a Proc> {
    p.iter().find(|(id, _)| id == f).map(|(_, d)| d)
}

/// The facts after a call, or `None` when the call is refused.
fn call_facts(p: &Table, f: &[Lin], it: &Item) -> Option<Facts> {
    let Item::Call { f: g, args, reps, outs, res, pfs } = it else { return None };
    let d = plook(p, g)?;
    let a: Vec<(Nat, Lin)> = args
        .iter()
        .filter_map(|(q, l, pp)| match pp {
            Some(pp) if no_wrap(f, l, pp) => Some((q.clone(), l.clone())),
            _ => None,
        })
        .collect();
    let tr_in = |v: &Var| -> Option<Lin> {
        match v {
            Var::S(q) => assoc(q, &a).cloned(),
            Var::B(r2) => assoc(r2, reps).map(|r| v1(Var::B(r.clone()))),
            Var::C(r2) => assoc(r2, reps).map(|r| v1(Var::C(r.clone()))),
        }
    };
    let mut pre2 = Vec::with_capacity(d.pre.len());
    for g in &d.pre {
        pre2.push(tr_lin(&tr_in, g)?);
    }
    let call_ok = a.iter().all(|(_, l)| {
        !mentions(&Var::S(res.clone()), l)
            && outs.iter().all(|q| !mentions(&Var::B(q.0.clone()), l) && !mentions(&Var::C(q.0.clone()), l))
    }) && reps.iter().all(|q| !outs.iter().any(|q2| q2.0 == q.1));
    if !(proves(f, &pre2, pfs) && call_ok) {
        return None;
    }
    let (mut wr, mut rb) = (Vec::new(), Vec::new());
    writes(&d.body, &mut wr);
    rebinds(&d.body, &mut rb);
    let out_of = |r2: &Nat| -> Option<Nat> {
        outs.iter().find(|q| assoc(&q.0, outs) == Some(r2)).map(|q| q.0.clone())
    };
    let tr_out = |v: &Var| -> Option<Lin> {
        match v {
            Var::S(q) => {
                if *q == d.rslot {
                    Some(v1(Var::S(res.clone())))
                } else if wr.contains(q) {
                    None
                } else {
                    assoc(q, &a).cloned()
                }
            }
            Var::B(r2) | Var::C(r2) => {
                let mk = |r: Nat| if matches!(v, Var::B(_)) { v1(Var::B(r)) } else { v1(Var::C(r)) };
                match out_of(r2) {
                    Some(r) => Some(mk(r)),
                    None if rb.contains(r2) => None,
                    None => assoc(r2, reps).map(|r| mk(r.clone())),
                }
            }
        }
    };
    let mut out: Facts = d.post.iter().filter_map(|g| tr_lin(&tr_out, g)).collect();
    let mut g = f.to_vec();
    for q in outs {
        g = kill_region(&g, &q.0);
    }
    out.extend(kill(&g, &Var::S(res.clone())));
    Some(out)
}

/// `LCap n r`: n is r's capacity.
fn cap_facts(f: &[Lin], n: &Nat, r: &Nat) -> Facts {
    let mut g = eqs(&Var::S(n.clone()), &v1(Var::C(r.clone())));
    g.extend(kill(f, &Var::S(n.clone())));
    g
}

/// `asnu_facts`: an i32 result proven in [0, 2^32) is LAsn's, else only
/// its range.
fn asnu_facts(f: &[Lin], x: &Nat, l: &Lin, lo: &Pf, hi: &Pf) -> Facts {
    let top = Z::pow2(32).sub(&one());
    if entails(f, lo, &lneg(l)) && entails(f, hi, &lsub(l, &kz(top.clone()))) {
        return asn_facts(f, x, l, lo, hi);
    }
    let vx = v1(Var::S(x.clone()));
    let mut g = vec![lsub(&kz(Z::zero()), &vx), lsub(&vx, &kz(top))];
    g.extend(kill(f, &Var::S(x.clone())));
    g
}

// ── The checker ────────────────────────────────────────────────────────

/// (loop invariant inside a loop, postcondition)
type Ctx<'a> = (Option<&'a Facts>, &'a Facts);

/// A straight-line item's facts after it, from `f`; `None` when it does
/// not hold (and for an item with a nested program or a jump, which
/// `lcheck` handles itself).
fn step(p: &Table, f: Facts, it: &Item) -> Option<Facts> {
    Some(match it {
        Item::Acc { r, w, off, addr, p: ps } | Item::Sto { r, w, off, addr, p: ps } => {
            if !acc_ok(&f, r, w, off, addr, ps) {
                return None;
            }
            f
        }
        Item::Handle(h, r) => {
            let mut g = eqs(&Var::S(h.clone()), &v1(Var::B(r.clone())));
            g.extend(kill(&f, &Var::S(h.clone())));
            g
        }
        Item::Len(n, r, st) => {
            let vn = v1(Var::S(n.clone()));
            let mut g = vec![lneg(&vn), lsub(&lmul(st, &vn), &v1(Var::C(r.clone())))];
            g.extend(kill(&f, &Var::S(n.clone())));
            g
        }
        Item::Alloc { h, r, sz, lo, hi } => {
            if !(entails(&f, lo, &lneg(sz))
                && entails(&f, hi, &lsub(sz, &kz(Z::pow2(32).sub(&one()))))
                && !mentions(&Var::S(h.clone()), sz)
                && !mentions(&Var::B(r.clone()), sz)
                && !mentions(&Var::C(r.clone()), sz))
            {
                return None;
            }
            let mut g = eqs(&Var::S(h.clone()), &v1(Var::B(r.clone())));
            g.push(lsub(sz, &v1(Var::C(r.clone()))));
            g.extend(kill_region(&kill(&f, &Var::S(h.clone())), r));
            g
        }
        Item::Wf(r) => {
            let (vb, vc) = (v1(Var::B(r.clone())), v1(Var::C(r.clone())));
            let k = Z::from_i64(12).sub(&Z::pow2(32)).add(&one());
            let mut g = vec![lneg(&vb), lneg(&vc), ladd(&ladd(&vb, &vc), &kz(k))];
            g.extend(f);
            g
        }
        Item::Static(r, b0, c0) => {
            let mut g = eqs(&Var::B(r.clone()), &kz(b0.clone()));
            g.extend(eqs(&Var::C(r.clone()), &kz(c0.clone())));
            g.extend(kill_region(&f, r));
            g
        }
        Item::Set(x) => kill(&f, &Var::S(x.clone())),
        Item::SetR(x, lo, hi) => {
            let vx = v1(Var::S(x.clone()));
            let mut g = vec![lsub(&kz(lo.clone()), &vx), lsub(&vx, &kz(hi.clone()))];
            g.extend(kill(&f, &Var::S(x.clone())));
            g
        }
        Item::Asn { x, l, lo, hi } => asn_facts(&f, x, l, lo, hi),
        Item::Cap(n, r) => cap_facts(&f, n, r),
        Item::AsnU { x, l, lo, hi } => asnu_facts(&f, x, l, lo, hi),
        Item::Call { .. } => call_facts(p, &f, it)?,
        _ => return None,
    })
}

/// `lcheck`: (fall-through state, break state), or `None` when refused.
fn lcheck(p: &Table, c: Ctx, mut f: Facts, items: &[Item]) -> Option<(St, St)> {
    // The breaks of the arms already passed, outermost first: the result's
    // break state is `fjoin j1 (fjoin j2 (... b))`, as Coq's chain nests it.
    let mut joins: Vec<St> = Vec::new();
    let finish = |joins: Vec<St>, s: St, b: St| -> Option<(St, St)> {
        Some((s, joins.iter().rev().fold(b, |acc, j| fjoin(j, &acc))))
    };
    for it in items {
        match it {
            Item::Test { thn, els, .. } | Item::Branch(thn, els) => {
                let (ft, fe) = match it {
                    Item::Test { a, b, sgn, pa, pb, nn, .. } => {
                        let ok = no_wrap(&f, a, pa)
                            && no_wrap(&f, b, pb)
                            && (*sgn || (entails(&f, &nn.0, &lneg(a)) && entails(&f, &nn.1, &lneg(b))));
                        if ok {
                            (vec![lsub(b, a)], vec![ladd(&lsub(a, b), &kz(one()))])
                        } else {
                            (Vec::new(), Vec::new())
                        }
                    }
                    _ => (Vec::new(), Vec::new()),
                };
                let mut f1 = ft;
                f1.extend(f.iter().cloned());
                let mut f2 = fe;
                f2.extend(f.iter().cloned());
                let (st, bt) = lcheck(p, c, f1, thn)?;
                let (se, be) = lcheck(p, c, f2, els)?;
                let j = fjoin(&bt, &be);
                match fjoin(&st, &se) {
                    None => return finish(joins, None, j),
                    Some(g) => {
                        joins.push(j);
                        f = g;
                    }
                }
            }
            Item::Loop { inv, pin, body } => {
                if !proves(&f, inv, pin) {
                    return None;
                }
                let (sb, bb) = lcheck(p, (Some(inv), c.1), inv.clone(), body)?;
                match fjoin(&sb, &bb) {
                    None => return finish(joins, None, None),
                    Some(g) => f = g,
                }
            }
            Item::Cont(pfs) => {
                return match c.0 {
                    Some(inv) if proves(&f, inv, pfs) => finish(joins, None, None),
                    _ => None,
                };
            }
            Item::Brk => return finish(joins, None, Some(f)),
            Item::Trap => return finish(joins, None, None),
            Item::Ret(pfs) => {
                return if proves(&f, c.1, pfs) { finish(joins, None, None) } else { None };
            }
            _ => f = step(p, f, it)?,
        }
    }
    finish(joins, Some(f), None)
}

/// `lnodup_ids && procs_ok`: no id twice; every body accepted under its
/// precondition, ending every path in a return or a trap.
pub(crate) fn procs_ok(p: &Table) -> bool {
    let nodup = p.iter().enumerate().all(|(i, (id, _))| !p[i + 1..].iter().any(|(j, _)| j == id));
    nodup
        && p.iter().all(|(_, d)| matches!(lcheck(p, (None, &d.post), d.pre.clone(), &d.body), Some((None, None))))
}

#[cfg(test)]
#[path = "lin_tests.rs"]
mod tests;
