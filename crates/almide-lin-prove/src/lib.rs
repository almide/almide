//! The linear certificates' PROVER (#4005 A4) — untrusted.
//!
//! almide-anchor-check translates each self-host body into a RoutineLin
//! program with holes (`lin::template`). This crate fills them:
//!
//! - preconditions, by projecting each call site's facts onto the callee's
//!   parameters and the regions its pointer parameters point into (Fourier–
//!   Motzkin), kept when every call site entails them, to a fixpoint;
//! - loop invariants, Houdini-style: the facts at the loop's entry, minus
//!   those some back edge does not re-establish, to a fixpoint;
//! - every proof obligation, by an exact rational simplex (lp.rs), scaled to
//!   the integer Farkas certificate RoutineLin's `entails` checks.
//!
//! It replays the checker's fact lists EXACTLY (terms in Coq's order), since
//! a proof names facts by index and joins compare them syntactically. A
//! procedure whose obligations it cannot meet is left out of the table, its
//! callers' calls of it become opaque, and the rest is proven again. Nothing
//! here is believed: the proven checker and almide-anchor-check judge every
//! certificate it writes.

pub mod lp;

use almide_anchor_check::lin::{self, It, Skel, Tok, V};
use std::collections::{BTreeMap, BTreeSet};

type Slot = u32;

/// A fact `Σ a·v + c <= 0`, terms in the order the checker builds them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct L {
    t: Vec<(V, i128)>,
    c: i128,
}

impl L {
    fn text(&self) -> String {
        lin::Lin { terms: self.t.clone(), cst: self.c }.text()
    }
}

fn from_lin(l: &lin::Lin) -> L {
    L { t: l.terms.clone(), c: l.cst }
}

fn v1(v: V) -> L {
    L { t: vec![(v, 1)], c: 0 }
}
fn kz(c: i128) -> L {
    L { t: Vec::new(), c }
}
fn scale(k: i128, ts: &[(V, i128)]) -> Vec<(V, i128)> {
    ts.iter().map(|(v, a)| (v.clone(), k * a)).collect()
}
fn ladd(a: &L, b: &L) -> L {
    let mut t = a.t.clone();
    t.extend(b.t.iter().cloned());
    L { t, c: a.c + b.c }
}
fn lneg(a: &L) -> L {
    L { t: scale(-1, &a.t), c: -a.c }
}
fn lsub(a: &L, b: &L) -> L {
    ladd(a, &lneg(b))
}
fn lmul(k: i128, a: &L) -> L {
    L { t: scale(k, &a.t), c: k * a.c }
}
fn coef(ts: &[(V, i128)], v: &V) -> i128 {
    ts.iter().filter(|(u, _)| u == v).map(|(_, a)| *a).sum()
}
fn mentions(v: &V, l: &L) -> bool {
    l.t.iter().any(|(u, _)| u == v)
}
fn kill(f: &[L], v: &V) -> Vec<L> {
    f.iter().filter(|l| !mentions(v, l)).cloned().collect()
}
fn kill_region(f: &[L], r: Slot) -> Vec<L> {
    kill(&kill(f, &V::B(r)), &V::C(r))
}
fn eqs(v: &V, l: &L) -> Vec<L> {
    vec![lsub(&v1(v.clone()), l), lsub(l, &v1(v.clone()))]
}
fn dropv(v: &V, ts: &[(V, i128)]) -> Vec<(V, i128)> {
    ts.iter().filter(|(u, _)| u != v).cloned().collect()
}
fn subst(v: &V, d: &L, f: &L) -> L {
    let mut t = Vec::new();
    for (u, a) in &f.t {
        if u == v {
            t.extend(scale(*a, &d.t));
        } else {
            t.push((u.clone(), *a));
        }
    }
    L { t, c: f.c + coef(&f.t, v) * d.c }
}

const I64MIN: i128 = -(1 << 63);
const I64MAX: i128 = (1 << 63) - 1;
const ADDR: i128 = 1 << 32;
const RPARAM: Slot = 2000;

type St = Option<Vec<L>>;

fn fjoin(a: &St, b: &St) -> St {
    match (a, b) {
        (None, x) => x.clone(),
        (x, None) => x.clone(),
        (Some(fa), Some(fb)) => Some(fa.iter().filter(|l| fb.contains(l)).cloned().collect()),
    }
}

/// A Farkas proof of `t` from `f`, as certificate text.
/// A linear form's coefficients, summed per variable.
fn coefs(l: &L) -> BTreeMap<V, i128> {
    let mut m = BTreeMap::new();
    for (v, a) in &l.t {
        *m.entry(v.clone()).or_insert(0) += a;
    }
    m.retain(|_, a| *a != 0);
    m
}

thread_local! {
    static DEADLINE: std::cell::Cell<Option<std::time::Instant>> = const { std::cell::Cell::new(None) };
}

fn past_deadline() -> bool {
    DEADLINE.with(|d| d.get().is_some_and(|t| std::time::Instant::now() > t))
}

fn prove(f: &[L], t: &L) -> Option<String> {
    if let Some(j) = f.iter().position(|g| g == t) {
        return Some(format!("({j}:1)1"));
    }
    // One fact, scaled: λ·g = μ·t coefficientwise, λ·c_g >= μ·c_t.
    let tc = coefs(t);
    if let Some((v0, a0)) = tc.iter().next() {
        for (j, g) in f.iter().enumerate() {
            let gc = coefs(g);
            let Some(&b0) = gc.get(v0) else { continue };
            if (b0 > 0) != (*a0 > 0) || gc.len() != tc.len() {
                continue;
            }
            // λ = |a0|, μ = |b0| (so λ·b0 = μ·a0).
            let (lam, mu) = (a0.abs(), b0.abs());
            if gc.iter().all(|(v, b)| tc.get(v).is_some_and(|a| lam * b == mu * a)) && lam * g.c >= mu * t.c {
                let gg = gcd(lam, mu).max(1);
                return Some(format!("({j}:{}){}", lam / gg, mu / gg));
            }
        }
    }
    if past_deadline() {
        return None;
    }
    let mut idx: BTreeMap<V, usize> = BTreeMap::new();
    let row = |l: &L, idx: &mut BTreeMap<V, usize>| -> (Vec<(usize, i128)>, i128) {
        let mut m: BTreeMap<usize, i128> = BTreeMap::new();
        for (v, a) in &l.t {
            let n = idx.len();
            let i = *idx.entry(v.clone()).or_insert(n);
            *m.entry(i).or_insert(0) += a;
        }
        (m.into_iter().filter(|(_, a)| *a != 0).collect(), l.c)
    };
    let rows: Vec<_> = f.iter().map(|g| row(g, &mut idx)).collect();
    let tr = row(t, &mut idx);
    let (ps, mu) = lp::farkas(&rows, &tr, idx.len())?;
    let body: Vec<String> = ps.iter().map(|(j, k)| format!("{j}:{k}")).collect();
    Some(format!("({}){mu}", body.join(" ")))
}

fn entailed(f: &[L], t: &L) -> bool {
    f.contains(t) || prove(f, t).is_some()
}

fn trivially_wraps_not(l: &L) -> bool {
    matches!(l.t.as_slice(), [(V::S(_), 1)]) && l.c == 0
}

/// No-wrap proofs (low, high) for `l`, when they exist.
fn no_wrap(f: &[L], l: &L) -> Option<(String, String)> {
    if trivially_wraps_not(l) {
        return Some(("()1".into(), "()1".into()));
    }
    Some((prove(f, &lsub(&kz(I64MIN), l))?, prove(f, &lsub(l, &kz(I64MAX)))?))
}

/// One call site, as the inference reads it.
#[derive(Clone, Debug)]
struct Site {
    callee: usize,
    facts: Vec<L>,
    /// (parameter, argument, its no-wrap proven).
    args: Vec<(Slot, L, bool)>,
}

/// A procedure's contract.
#[derive(Clone, Debug, Default, PartialEq)]
struct Contract {
    pre: Vec<L>,
    post: Vec<L>,
}

/// The slots a body writes and the regions it rebinds (RoutineLin's
/// `writes` / `rebinds`).
fn writes_rebinds(items: &[It], w: &mut BTreeSet<Slot>, r: &mut BTreeSet<Slot>) {
    for it in items {
        match it {
            It::Handle { h, .. } | It::Len { n: h, .. } | It::Set(h) | It::SetR { x: h, .. } | It::Asn { x: h, .. } => {
                w.insert(*h);
            }
            It::Alloc { h, r: g, .. } => {
                w.insert(*h);
                r.insert(*g);
            }
            It::Static { r: g, .. } => {
                r.insert(*g);
            }
            It::Call { res, .. } => {
                w.insert(*res);
            }
            It::Test { thn, els, .. } | It::Branch { thn, els } => {
                writes_rebinds(thn, w, r);
                writes_rebinds(els, w, r);
            }
            It::Loop { body } => writes_rebinds(body, w, r),
            _ => {}
        }
    }
}

struct Sim<'a> {
    skels: &'a BTreeMap<usize, Skel>,
    contracts: &'a BTreeMap<usize, Contract>,
    fills: Vec<String>,
    sites: Vec<Site>,
    rets: Vec<Vec<L>>,
    conts: Vec<Vec<Vec<L>>>,
    probe: usize,
    err: Option<String>,
}

fn region_vars(f: &[L]) -> BTreeSet<Slot> {
    let mut out = BTreeSet::new();
    for l in f {
        for (v, _) in &l.t {
            if let V::B(r) | V::C(r) = v {
                out.insert(*r);
            }
        }
    }
    out
}

/// The caller region each pointer argument points into (`VB r <= arg`), as
/// the callee's region parameters `RPARAM + i`: the first parameter into a
/// region names it.
fn region_map(f: &[L], args: &[(Slot, L, bool)]) -> Vec<(Slot, Slot)> {
    let regions = region_vars(f);
    let mut used = BTreeSet::new();
    let mut out = Vec::new();
    for (p, a, ok) in args {
        if !*ok {
            continue;
        }
        for &r in &regions {
            if !used.contains(&r) && entailed(f, &lsub(&v1(V::B(r)), a)) {
                used.insert(r);
                out.push((RPARAM + p, r));
                break;
            }
        }
    }
    out
}

impl Sim<'_> {
    fn fill(&mut self, s: String) {
        self.fills.push(s);
    }

    fn fail<T>(&mut self, why: String) -> Option<T> {
        if self.err.is_none() {
            self.err = Some(why);
        }
        None
    }

    fn proofs(&mut self, f: &[L], g: &[L]) -> Option<String> {
        let mut out = Vec::new();
        for l in g {
            if f.contains(l) {
                out.push("()1".to_string());
            } else {
                match prove(f, l) {
                    Some(p) => out.push(p),
                    None => return self.fail(format!("cannot prove {}", l.text())),
                }
            }
        }
        Some(out.join(" "))
    }

    /// Default fills for the holes of items the checker never reads.
    fn dead(&mut self, items: &[It]) {
        let mut t = Vec::new();
        let sk = Skel { rslot: 0, body: items.to_vec(), sites: Vec::new() };
        for tok in lin::template(0, &sk).into_iter().skip(0) {
            t.push(tok);
        }
        // The template starts with the procedure's two fact holes.
        for tok in t.into_iter().filter_map(|x| match x {
            Tok::Hole(h) => Some(h),
            Tok::Lit(_) => None,
        }).skip(2) {
            self.fills.push(match tok {
                lin::Hole::Pf => "()1".into(),
                lin::Hole::Reg => "0".into(),
                _ => String::new(),
            });
        }
    }

    fn lcheck(&mut self, inv: Option<&Vec<L>>, post: &[L], mut f: Vec<L>, items: &[It]) -> Option<(St, St)> {
        let mut joins: Vec<St> = Vec::new();
        let finish = |joins: Vec<St>, s: St, b: St| -> Option<(St, St)> {
            Some((s, joins.iter().rev().fold(b, |acc, j| fjoin(j, &acc))))
        };
        for (i, it) in items.iter().enumerate() {
            let rest = &items[i + 1..];
            match it {
                It::Acc { w, off, addr, store, .. } => {
                    let addr = from_lin(addr);
                    let off = i128::from(*off);
                    let w = i128::from(*w);
                    let mut done = false;
                    for r in region_vars(&f) {
                        let (vb, vc) = (v1(V::B(r)), v1(V::C(r)));
                        let p0 = prove(&f, &lneg(&addr));
                        let p1 = prove(&f, &lsub(&vb, &ladd(&addr, &kz(off))));
                        let p2 = prove(&f, &lsub(&ladd(&addr, &kz(off + w - 12)), &ladd(&vb, &vc)));
                        if let (Some(a), Some(b), Some(c)) = (p0, p1, p2) {
                            self.fill(r.to_string());
                            self.fill(a);
                            self.fill(b);
                            self.fill(c);
                            done = true;
                            break;
                        }
                    }
                    if !done {
                        return self.fail(format!("{} of {w} at {}+{off} not in any region", if *store { "store" } else { "load" }, addr.text()));
                    }
                }
                It::Handle { h, r } => {
                    let mut g = eqs(&V::S(*h), &v1(V::B(*r)));
                    g.extend(kill(&f, &V::S(*h)));
                    f = g;
                }
                It::Len { n, r, s } => {
                    let vn = v1(V::S(*n));
                    let mut g = vec![lneg(&vn), lsub(&lmul(*s, &vn), &v1(V::C(*r)))];
                    g.extend(kill(&f, &V::S(*n)));
                    f = g;
                }
                It::Alloc { h, r, sz } => {
                    let sz = from_lin(sz);
                    let lo = prove(&f, &lneg(&sz));
                    let hi = prove(&f, &lsub(&sz, &kz(ADDR - 1)));
                    let (Some(lo), Some(hi)) = (lo, hi) else {
                        return self.fail(format!("allocation size {} not in [0, 2^32)", sz.text()));
                    };
                    self.fill(lo);
                    self.fill(hi);
                    if mentions(&V::S(*h), &sz) || mentions(&V::B(*r), &sz) || mentions(&V::C(*r), &sz) {
                        return self.fail("allocation size names its own result".into());
                    }
                    let mut g = eqs(&V::S(*h), &v1(V::B(*r)));
                    g.push(lsub(&sz, &v1(V::C(*r))));
                    g.extend(kill_region(&kill(&f, &V::S(*h)), *r));
                    f = g;
                }
                It::Wf(r) => {
                    let (vb, vc) = (v1(V::B(*r)), v1(V::C(*r)));
                    let mut g = vec![lneg(&vb), lneg(&vc), ladd(&ladd(&vb, &vc), &kz(12 - ADDR + 1))];
                    g.extend(f);
                    f = g;
                }
                It::Static { r, b0, c0 } => {
                    let mut g = eqs(&V::B(*r), &kz(*b0));
                    g.extend(eqs(&V::C(*r), &kz(*c0)));
                    g.extend(kill_region(&f, *r));
                    f = g;
                }
                It::Set(x) => f = kill(&f, &V::S(*x)),
                It::SetR { x, lo, hi } => {
                    let vx = v1(V::S(*x));
                    let mut g = vec![lsub(&kz(*lo), &vx), lsub(&vx, &kz(*hi))];
                    g.extend(kill(&f, &V::S(*x)));
                    f = g;
                }
                It::Asn { x, l } => {
                    let l = from_lin(l);
                    let vx = V::S(*x);
                    match no_wrap(&f, &l) {
                        Some((lo, hi)) => {
                            self.fill(lo);
                            self.fill(hi);
                            let c = coef(&l.t, &vx);
                            f = if c == 0 {
                                let mut g = eqs(&vx, &l);
                                g.extend(kill(&f, &vx));
                                g
                            } else if c == 1 {
                                let d = lsub(&v1(vx.clone()), &L { t: dropv(&vx, &l.t), c: l.c });
                                f.iter().map(|g| subst(&vx, &d, g)).collect()
                            } else {
                                kill(&f, &vx)
                            };
                        }
                        None => {
                            self.fill("()1".into());
                            self.fill("()1".into());
                            f = kill(&f, &vx);
                        }
                    }
                }
                It::Test { thn, els, .. } | It::Branch { thn, els } => {
                    let (ft, fe) = match it {
                        It::Test { a, b, sgn, .. } => {
                            let (a, b) = (from_lin(a), from_lin(b));
                            let pa = no_wrap(&f, &a);
                            let pb = no_wrap(&f, &b);
                            let nn = if *sgn {
                                Some(("()1".to_string(), "()1".to_string()))
                            } else {
                                match (prove(&f, &lneg(&a)), prove(&f, &lneg(&b))) {
                                    (Some(x), Some(y)) => Some((x, y)),
                                    _ => None,
                                }
                            };
                            let ok = pa.is_some() && pb.is_some() && nn.is_some();
                            let (pa, pb, nn) = (
                                pa.unwrap_or(("()1".into(), "()1".into())),
                                pb.unwrap_or(("()1".into(), "()1".into())),
                                nn.unwrap_or(("()1".into(), "()1".into())),
                            );
                            for p in [pa.0, pa.1, pb.0, pb.1, nn.0, nn.1] {
                                self.fill(p);
                            }
                            if ok {
                                (vec![lsub(&b, &a)], vec![ladd(&lsub(&a, &b), &kz(1))])
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
                    let (st, bt) = self.lcheck(inv, post, f1, thn)?;
                    let (se, be) = self.lcheck(inv, post, f2, els)?;
                    let j = fjoin(&bt, &be);
                    match fjoin(&st, &se) {
                        None => {
                            self.dead(rest);
                            return finish(joins, None, j);
                        }
                        Some(g) => {
                            joins.push(j);
                            f = g;
                        }
                    }
                }
                It::Loop { body } => {
                    // Houdini: the entry facts, minus what a back edge loses.
                    let mut cand = f.clone();
                    let (fm, sm) = (self.fills.len(), self.sites.len());
                    loop {
                        self.probe += 1;
                        self.conts.push(Vec::new());
                        let r = self.lcheck(Some(&cand), post, cand.clone(), body);
                        let states = self.conts.pop().unwrap_or_default();
                        self.probe -= 1;
                        self.fills.truncate(fm);
                        self.sites.truncate(sm);
                        r?;
                        let next: Vec<L> = cand.iter().filter(|g| states.iter().all(|s| entailed(s, g))).cloned().collect();
                        if next == cand {
                            break;
                        }
                        cand = next;
                    }
                    self.fill(cand.iter().map(L::text).collect::<Vec<_>>().join(" "));
                    let pin = vec!["()1".to_string(); cand.len()].join(" ");
                    self.fill(pin);
                    self.conts.push(Vec::new());
                    let r = self.lcheck(Some(&cand), post, cand.clone(), body);
                    self.conts.pop();
                    let (sb, bb) = r?;
                    match fjoin(&sb, &bb) {
                        None => {
                            self.dead(rest);
                            return finish(joins, None, None);
                        }
                        Some(g) => f = g,
                    }
                }
                It::Cont => {
                    let Some(iv) = inv else { return self.fail("a back edge outside a loop".into()) };
                    if self.probe > 0 {
                        if let Some(top) = self.conts.last_mut() {
                            top.push(f.clone());
                        }
                        self.fill(String::new());
                    } else {
                        let p = self.proofs(&f, iv)?;
                        self.fill(p);
                    }
                    return finish(joins, None, None);
                }
                It::Brk => return finish(joins, None, Some(f)),
                It::Trap => return finish(joins, None, None),
                It::Ret => {
                    if self.probe == 0 {
                        self.rets.push(f.clone());
                    }
                    let p = self.proofs(&f, post)?;
                    self.fill(p);
                    return finish(joins, None, None);
                }
                It::Call { f: g, args, res } => {
                    let pre = self.contracts.get(g).map(|c| c.pre.clone()).unwrap_or_default();
                    let mut sargs = Vec::new();
                    let mut valid: Vec<(Slot, L)> = Vec::new();
                    for (p, l) in args {
                        let l = from_lin(l);
                        match no_wrap(&f, &l) {
                            Some((lo, hi)) => {
                                self.fill(format!(" ! {lo} {hi}"));
                                valid.push((*p, l.clone()));
                                sargs.push((*p, l, true));
                            }
                            None => {
                                self.fill(String::new());
                                sargs.push((*p, l, false));
                            }
                        }
                    }
                    let reps = region_map(&f, &sargs);
                    self.fill(reps.iter().map(|(a, b)| format!("{a}:{b}")).collect::<Vec<_>>().join(" "));
                    self.fill(String::new());
                    // The precondition, translated into the caller's terms.
                    let tr = |v: &V| -> Option<L> {
                        match v {
                            V::S(q) => valid.iter().find(|(p, _)| p == q).map(|(_, l)| l.clone()),
                            V::B(r2) => reps.iter().find(|(a, _)| a == r2).map(|(_, r)| v1(V::B(*r))),
                            V::C(r2) => reps.iter().find(|(a, _)| a == r2).map(|(_, r)| v1(V::C(*r))),
                        }
                    };
                    let mut pre2 = Vec::new();
                    for c in &pre {
                        match tr_lin(&tr, c) {
                            Some(c2) => pre2.push(c2),
                            None => return self.fail(format!("call of {g}: precondition {} not expressible here", c.text())),
                        }
                    }
                    let p = self.proofs(&f, &pre2)?;
                    self.fill(p);
                    if self.probe == 0 {
                        self.sites.push(Site { callee: *g, facts: f.clone(), args: sargs.clone() });
                    }
                    // The postcondition, translated back (RoutineLin's tr_out).
                    let post_c = self.contracts.get(g).map(|c| c.post.clone()).unwrap_or_default();
                    let (mut wr, mut rb) = (BTreeSet::new(), BTreeSet::new());
                    if let Some(sk) = self.skels.get(g) {
                        writes_rebinds(&sk.body, &mut wr, &mut rb);
                    }
                    let rslot = self.skels.get(g).map_or(u32::MAX, |k| k.rslot);
                    let tro = |v: &V| -> Option<L> {
                        match v {
                            V::S(q) if *q == rslot => Some(v1(V::S(*res))),
                            V::S(q) if wr.contains(q) => None,
                            V::S(q) => valid.iter().find(|(p, _)| p == q).map(|(_, l)| l.clone()),
                            V::B(r2) | V::C(r2) if rb.contains(r2) => None,
                            V::B(r2) => reps.iter().find(|(a, _)| a == r2).map(|(_, r)| v1(V::B(*r))),
                            V::C(r2) => reps.iter().find(|(a, _)| a == r2).map(|(_, r)| v1(V::C(*r))),
                        }
                    };
                    let mut g2: Vec<L> = post_c.iter().filter_map(|c| tr_lin(&tro, c)).collect();
                    g2.extend(kill(&f, &V::S(*res)));
                    f = g2;
                }
            }
        }
        finish(joins, Some(f), None)
    }
}

/// RoutineLin's `tr_lin`: a right fold of `ladd (lmul a d) rest`, then the
/// constant.
fn tr_lin(tr: &dyn Fn(&V) -> Option<L>, l: &L) -> Option<L> {
    let mut ds = Vec::new();
    for (v, a) in &l.t {
        ds.push((*a, tr(v)?));
    }
    let mut m = kz(0);
    for (a, d) in ds.iter().rev() {
        m = ladd(&lmul(*a, d), &m);
    }
    Some(ladd(&m, &kz(l.c)))
}

// ── Projection (Fourier–Motzkin) ─────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Pv {
    Old(V),
    New(V),
}

type Row = (BTreeMap<Pv, i128>, i128);

fn row_of(l: &L, f: impl Fn(&V) -> Pv) -> Row {
    let mut m = BTreeMap::new();
    for (v, a) in &l.t {
        *m.entry(f(v)).or_insert(0) += a;
    }
    m.retain(|_, a| *a != 0);
    (m, l.c)
}

fn gcd(a: i128, b: i128) -> i128 {
    if b == 0 { a.abs() } else { gcd(b, a % b) }
}

fn norm(mut r: Row) -> Row {
    r.0.retain(|_, a| *a != 0);
    let g = r.0.values().fold(0, |g, a| gcd(g, *a));
    if g > 1 && r.1 % g == 0 {
        for a in r.0.values_mut() {
            *a /= g;
        }
        r.1 /= g;
    }
    r
}

/// The facts over `New` variables that `rows` imply (exact rational
/// elimination of every `Old` variable), or `None` when they contradict.
fn project(mut rows: Vec<Row>) -> Option<Vec<Row>> {
    const CAP: usize = 400;
    loop {
        rows = rows.into_iter().map(norm).collect();
        rows.sort();
        rows.dedup();
        for r in &rows {
            if r.0.is_empty() && r.1 > 0 {
                return None;
            }
        }
        rows.retain(|r| !r.0.is_empty());
        // The Old variable with the fewest pairings.
        let olds: BTreeSet<Pv> = rows.iter().flat_map(|r| r.0.keys().filter(|k| matches!(k, Pv::Old(_))).cloned()).collect();
        let Some(x) = olds.into_iter().min_by_key(|x| {
            let p = rows.iter().filter(|r| r.0.get(x).is_some_and(|a| *a > 0)).count();
            let n = rows.iter().filter(|r| r.0.get(x).is_some_and(|a| *a < 0)).count();
            p * n
        }) else {
            return Some(rows);
        };
        let (with, without): (Vec<Row>, Vec<Row>) = rows.into_iter().partition(|r| r.0.contains_key(&x));
        let pos: Vec<&Row> = with.iter().filter(|r| r.0[&x] > 0).collect();
        let neg: Vec<&Row> = with.iter().filter(|r| r.0[&x] < 0).collect();
        let mut next = without;
        for p in &pos {
            for n in &neg {
                let (a, b) = (p.0[&x], -n.0[&x]);
                let mut m: BTreeMap<Pv, i128> = BTreeMap::new();
                for (k, v) in &p.0 {
                    *m.entry(k.clone()).or_insert(0) += b * v;
                }
                for (k, v) in &n.0 {
                    *m.entry(k.clone()).or_insert(0) += a * v;
                }
                m.remove(&x);
                next.push(norm((m, b * p.1 + a * n.1)));
                if next.len() > CAP {
                    break;
                }
            }
        }
        if next.len() > CAP {
            next.sort_by_key(|r| r.0.len());
            next.truncate(CAP);
        }
        rows = next;
    }
}

/// Per coefficient direction, only the tightest row (`a·x + c <= 0` with
/// the largest `c`): the others follow from it.
fn strongest(rows: Vec<Row>) -> Vec<Row> {
    let mut best: BTreeMap<BTreeMap<Pv, i128>, i128> = BTreeMap::new();
    for (m, c) in rows {
        let e = best.entry(m).or_insert(c);
        if c > *e {
            *e = c;
        }
    }
    best.into_iter().collect()
}

/// The candidate precondition one call site gives its callee.
fn site_candidates(s: &Site) -> Vec<L> {
    let mut rows: Vec<Row> = s.facts.iter().map(|l| row_of(l, |v| Pv::Old(v.clone()))).collect();
    for (p, a, ok) in &s.args {
        if !*ok {
            continue;
        }
        // New(s_p) - arg = 0.
        let mut r = row_of(a, |v| Pv::Old(v.clone()));
        for v in r.0.values_mut() {
            *v = -*v;
        }
        r.1 = -r.1;
        r.0.insert(Pv::New(V::S(*p)), 1);
        let neg = (r.0.iter().map(|(k, v)| (k.clone(), -v)).collect(), -r.1);
        rows.push(r);
        rows.push(neg);
    }
    for (rp, r) in region_map(&s.facts, &s.args) {
        for (n, o) in [(V::B(rp), V::B(r)), (V::C(rp), V::C(r))] {
            let m: BTreeMap<Pv, i128> = [(Pv::New(n.clone()), 1), (Pv::Old(o.clone()), -1)].into_iter().collect();
            let neg = m.iter().map(|(k, v)| (k.clone(), -v)).collect();
            rows.push((m, 0));
            rows.push((neg, 0));
        }
    }
    let Some(out) = project(rows) else { return Vec::new() };
    strongest(out)
        .into_iter()
        .filter(|(m, _)| m.len() <= 4)
        .filter_map(|(m, c)| {
            let t: Vec<(V, i128)> = m
                .into_iter()
                .map(|(k, a)| match k {
                    Pv::New(v) => Some((v, a)),
                    Pv::Old(_) => None,
                })
                .collect::<Option<_>>()?;
            Some(L { t, c })
        })
        .take(80)
        .collect()
}

/// The postcondition every return entails, over the result slot, the
/// slots the body never writes and its region parameters.
fn infer_post(sk: &Skel, rets: &[Vec<L>]) -> Vec<L> {
    let Some(first) = rets.first() else { return Vec::new() };
    let (mut w, mut rb) = (BTreeSet::new(), BTreeSet::new());
    writes_rebinds(&sk.body, &mut w, &mut rb);
    let keep = |v: &V| match v {
        V::S(x) => *x == sk.rslot || !w.contains(x),
        V::B(r) | V::C(r) => *r >= RPARAM && !rb.contains(r),
    };
    let rows: Vec<Row> = first.iter().map(|l| row_of(l, |v| if keep(v) { Pv::New(v.clone()) } else { Pv::Old(v.clone()) })).collect();
    let Some(out) = project(rows) else { return Vec::new() };
    let mut post: Vec<L> = out
        .into_iter()
        .filter(|(m, _)| m.keys().any(|k| matches!(k, Pv::New(V::S(x)) if *x == sk.rslot)))
        .filter_map(|(m, c)| {
            let t = m.into_iter().map(|(k, a)| match k { Pv::New(v) => Some((v, a)), Pv::Old(_) => None }).collect::<Option<Vec<_>>>()?;
            Some(L { t, c })
        })
        .filter(|c| rets.iter().all(|f| entailed(f, c)))
        .take(40)
        .collect();
    let mut i = 0;
    while i < post.len() {
        let rest: Vec<L> = post.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, l)| l.clone()).collect();
        if entailed(&rest, &post[i]) {
            post.remove(i);
        } else {
            i += 1;
        }
    }
    post
}

/// Does the site entail candidate `c` (callee terms)?
fn site_entails(s: &Site, c: &L) -> bool {
    let reps = region_map(&s.facts, &s.args);
    let tr = |v: &V| -> Option<L> {
        match v {
            V::S(q) => s.args.iter().find(|(p, _, ok)| p == q && *ok).map(|(_, l, _)| l.clone()),
            V::B(r2) => reps.iter().find(|(a, _)| a == r2).map(|(_, r)| v1(V::B(*r))),
            V::C(r2) => reps.iter().find(|(a, _)| a == r2).map(|(_, r)| v1(V::C(*r))),
        }
    };
    tr_lin(&tr, c).is_some_and(|c2| entailed(&s.facts, &c2))
}

/// One procedure proven: its certificate stretch, or why not.
#[allow(clippy::type_complexity)]
fn prove_proc(skels: &BTreeMap<usize, Skel>, contracts: &BTreeMap<usize, Contract>, pos: usize) -> (Result<String, String>, Vec<Site>, Vec<Vec<L>>) {
    let sk = &skels[&pos];
    let pre = contracts.get(&pos).map(|c| c.pre.clone()).unwrap_or_default();
    let post = contracts.get(&pos).map(|c| c.post.clone()).unwrap_or_default();
    let mut sim = Sim { skels, contracts, fills: Vec::new(), sites: Vec::new(), rets: Vec::new(), conts: Vec::new(), probe: 0, err: None };
    sim.fills.push(pre.iter().map(L::text).collect::<Vec<_>>().join(" "));
    sim.fills.push(post.iter().map(L::text).collect::<Vec<_>>().join(" "));
    let r = sim.lcheck(None, &post, pre, &sk.body);
    let _ = sim.skels;
    match r {
        Some((None, None)) => {
            let tpl = lin::template(pos, sk);
            let mut out = String::new();
            let mut it = sim.fills.into_iter();
            for t in &tpl {
                match t {
                    Tok::Lit(l) => out.push_str(l),
                    Tok::Hole(_) => out.push_str(&it.next().unwrap_or_default()),
                }
            }
            (Ok(out), sim.sites, sim.rets)
        }
        Some(_) => (Err("a path does not end in a return".into()), sim.sites, sim.rets),
        None => (Err(sim.err.unwrap_or_else(|| "refused".into())), sim.sites, sim.rets),
    }
}

/// The certificate for a module: `;`-separated `Q` stretches, and per
/// function position the reason it is not in the table.
pub fn certify(wasm: &[u8], anchors: &str) -> Result<(String, BTreeMap<usize, String>), String> {
    certify_table(wasm, anchors).map(|(c, r, _)| (c, r))
}

/// [`certify`], and the positions certified.
pub fn certify_table(wasm: &[u8], anchors: &str) -> Result<(String, BTreeMap<usize, String>, BTreeSet<usize>), String> {
    let budget = std::env::var("ALMIDE_LIN_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(60u64);
    DEADLINE.with(|d| d.set(Some(std::time::Instant::now() + std::time::Duration::from_secs(budget))));
    let m = lin::Module::read(wasm)?;
    let exported = lin::exported_or_tabled(wasm, m.imports)?;
    let mut refused: BTreeMap<usize, String> = BTreeMap::new();
    let sites0 = lin::prim_sites(anchors)?;
    let mut table: BTreeSet<usize> = sites0.keys().copied().collect();
    loop {
        let sk = lin::skeletons(wasm, anchors, Some(&table))?;
        let mut skels = BTreeMap::new();
        let mut changed = false;
        for (pos, r) in sk {
            match r {
                Ok((s, _)) => {
                    skels.insert(pos, s);
                }
                Err(e) => {
                    refused.insert(pos, format!("translation: {e}"));
                    table.remove(&pos);
                    changed = true;
                }
            }
        }
        if changed {
            continue;
        }
        // Entry points: exported, tabled, or called from outside the table.
        let mut entry: BTreeSet<usize> = exported.iter().copied().filter(|p| table.contains(p)).collect();
        for (pos, b) in m.bodies.iter().enumerate() {
            if table.contains(&pos) {
                continue;
            }
            for op in &b.ops {
                if let wasmparser_op::Call { function_index } = op {
                    if let Some(p) = function_index.checked_sub(m.imports) {
                        entry.insert(p as usize);
                    }
                }
            }
        }
        // Contracts to a fixpoint.
        let mut contracts: BTreeMap<usize, Contract> = BTreeMap::new();
        let mut results = BTreeMap::new();
        for _round in 0..8 {
            if _round > 0 && past_deadline() {
                break;
            }
            let mut sites: Vec<Site> = Vec::new();
            let mut rets: BTreeMap<usize, Vec<Vec<L>>> = BTreeMap::new();
            results.clear();
            for &pos in &table {
                let (r, s, rt) = prove_proc(&skels, &contracts, pos);
                sites.extend(s);
                rets.insert(pos, rt);
                results.insert(pos, r);
            }
            let mut next: BTreeMap<usize, Contract> = BTreeMap::new();
            for &pos in &table {
                let post = infer_post(&skels[&pos], rets.get(&pos).map_or(&[][..], Vec::as_slice));
                if entry.contains(&pos) {
                    next.insert(pos, Contract { pre: Vec::new(), post });
                    continue;
                }
                let mine: Vec<&Site> = sites.iter().filter(|s| s.callee == pos).collect();
                let Some(first) = mine.first() else {
                    next.insert(pos, Contract { pre: Vec::new(), post });
                    continue;
                };
                let cands = site_candidates(first);
                let mut pre: Vec<L> = cands.into_iter().filter(|c| mine.iter().all(|s| site_entails(s, c))).collect();
                // Drop what the rest implies (a bounded effort).
                let mut i = 0;
                while i < pre.len() && pre.len() <= 48 && !past_deadline() {
                    let rest: Vec<L> = pre.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, l)| l.clone()).collect();
                    if entailed(&rest, &pre[i]) {
                        pre.remove(i);
                    } else {
                        i += 1;
                    }
                }
                next.insert(pos, Contract { pre, post });
            }
            if std::env::var_os("ALMIDE_LIN_DEBUG").is_some() {
                for (p, r) in &results {
                    eprintln!("round {_round}: {p}: {}", match r { Ok(_) => "ok".to_string(), Err(e) => e.clone() });
                }
                for s in &sites {
                    eprintln!("  site -> {}: args {:?}", s.callee, s.args.iter().map(|(p, l, ok)| format!("{p}:{}{}", l.text(), if *ok { "!" } else { "" })).collect::<Vec<_>>());
                    eprintln!("    facts {}", s.facts.iter().map(L::text).collect::<Vec<_>>().join(" "));
                }
                for (p, c) in &next {
                    eprintln!("  pre {p}: {}", c.pre.iter().map(L::text).collect::<Vec<_>>().join(" "));
                }
            }
            if next == contracts {
                break;
            }
            contracts = next;
        }
        // Drop what is not proven; prove the rest again without it.
        let failed: Vec<usize> = results.iter().filter(|(_, r)| r.is_err()).map(|(p, _)| *p).collect();
        if failed.is_empty() {
            let cert: Vec<String> = table.iter().map(|p| results[p].clone().unwrap_or_default()).collect();
            return Ok((cert.join(" ; "), refused, table));
        }
        for p in failed {
            refused.insert(p, results[&p].clone().err().unwrap_or_default());
            table.remove(&p);
        }
    }
}

/// wasmparser's operator, through anchor-check's re-export.
mod wasmparser_op {
    pub use almide_anchor_check::lin::WasmOp::*;
}
