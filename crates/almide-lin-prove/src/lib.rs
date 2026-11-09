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
mod sim;

use almide_anchor_check::lin::{self, It, Skel, Tok, V};
use std::collections::{BTreeMap, BTreeSet};

use sim::{region_map, tr_lin, writes_rebinds, Contract, Sim, Site};

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

/// The search's effort per module, in simplex pivots ([`lp::pivots`]) — a
/// step count, not wall time, so what is certified is the same on every
/// machine and the counted class can be ratcheted. A body the search did not
/// reach within it stays counted. `ALMIDE_LIN_PIVOTS` overrides it for an
/// experiment; the wall runs the default.
pub const PIVOT_BUDGET: u64 = 2_000_000;

thread_local! {
    static BUDGET: std::cell::Cell<u64> = const { std::cell::Cell::new(PIVOT_BUDGET) };
}

fn past_deadline() -> bool {
    lp::pivots() > BUDGET.with(std::cell::Cell::get)
}

/// The budget the last [`certify_table`] ran under, and the pivots it took.
pub fn effort() -> (u64, u64) {
    (BUDGET.with(std::cell::Cell::get), lp::pivots())
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

/// One procedure's proof attempt: its certificate stretch (or why not),
/// its call sites, its returns' facts, and (a routine's selection pass) the
/// accesses no region held.
struct Proved {
    stretch: Result<String, String>,
    sites: Vec<Site>,
    rets: Vec<Vec<L>>,
    dropped: Vec<usize>,
}

/// One procedure proven: its certificate stretch, or why not.
fn prove_proc(skels: &BTreeMap<usize, Skel>, contracts: &BTreeMap<usize, Contract>, pos: usize, skip: bool) -> Proved {
    let sk = &skels[&pos];
    let pre = contracts.get(&pos).map(|c| c.pre.clone()).unwrap_or_default();
    let post = contracts.get(&pos).map(|c| c.post.clone()).unwrap_or_default();
    let mut sim = Sim {
        skels,
        contracts,
        fills: Vec::new(),
        sites: Vec::new(),
        rets: Vec::new(),
        conts: Vec::new(),
        probe: 0,
        err: None,
        skip,
        dropped: Vec::new(),
    };
    sim.fills.push(pre.iter().map(L::text).collect::<Vec<_>>().join(" "));
    sim.fills.push(post.iter().map(L::text).collect::<Vec<_>>().join(" "));
    let r = sim.lcheck(None, &post, pre, &sk.body);
    let _ = sim.skels;
    let stretch = match r {
        Some((None, None)) => {
            let tpl = lin::template(pos, sk);
            let mut out = String::new();
            let mut it = std::mem::take(&mut sim.fills).into_iter();
            for t in &tpl {
                match t {
                    Tok::Lit(l) => out.push_str(l),
                    Tok::Hole(_) => out.push_str(&it.next().unwrap_or_default()),
                }
            }
            Ok(out)
        }
        Some(_) => Err("a path does not end in a return".into()),
        None => Err(sim.err.take().unwrap_or_else(|| "refused".into())),
    };
    Proved { stretch, sites: sim.sites, rets: sim.rets, dropped: sim.dropped }
}

/// The certificate for a module: `;`-separated `Q` stretches, and per
/// function position the reason it is not in the table.
pub fn certify(wasm: &[u8], anchors: &str) -> Result<(String, BTreeMap<usize, String>), String> {
    certify_table(wasm, anchors).map(|(c, r, _)| (c, r))
}

/// [`certify_table`]'s answer: the certificate text, the reason each
/// uncertified function was left out, and the certified functions.
pub type Certified = (String, BTreeMap<usize, String>, BTreeSet<usize>);

/// [`certify`], and the positions certified.
pub fn certify_table(wasm: &[u8], anchors: &str) -> Result<Certified, String> {
    let budget = almide_base::env::var("ALMIDE_LIN_PIVOTS").and_then(|s| s.parse().ok()).unwrap_or(PIVOT_BUDGET);
    BUDGET.with(|b| b.set(budget));
    lp::reset_pivots();
    let m = lin::Module::read(wasm)?;
    let exported = lin::exported_or_tabled(wasm, m.imports)?;
    let mut refused: BTreeMap<usize, String> = BTreeMap::new();
    let sites0 = lin::prim_sites(anchors)?;
    let heap = lin::selfhost_params(anchors);
    let (alloc, ..) = almide_anchor_check::parse_anchors(anchors)?;
    let mut table: BTreeSet<usize> = sites0.keys().copied().collect();
    loop {
        let input = Input { m: &m, sites: &sites0, heap: &heap, alloc, routine: false };
        let Some(skels) = translate_table(&input, &mut table, &mut refused) else { continue };
        let entry = entry_points(&m, &exported, &table);
        let (results, _) = infer_contracts(&skels, &table, &entry, false);
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

/// What the translation of a table reads: the module, each function's prim
/// sites and heap parameters, the allocator's index, and whether the table
/// is the runtime routines' (#4005 A4) or the self-host bodies'.
struct Input<'a> {
    m: &'a lin::Module<'a>,
    sites: &'a BTreeMap<usize, BTreeSet<usize>>,
    heap: &'a BTreeMap<usize, BTreeMap<u32, Option<i128>>>,
    alloc: u32,
    routine: bool,
}

/// Every table function's translation; `None` when one is refused (it
/// leaves the table and the round starts again).
fn translate_table(input: &Input<'_>, table: &mut BTreeSet<usize>, refused: &mut BTreeMap<usize, String>) -> Option<BTreeMap<usize, Skel>> {
    let mut skels = BTreeMap::new();
    let mut changed = false;
    let none = BTreeSet::new();
    for &pos in &table.clone() {
        let hp = input.heap.get(&pos).cloned().unwrap_or_default();
        let s = input.sites.get(&pos).unwrap_or(&none);
        let tr = if input.routine {
            lin::translate_routine(input.m, pos, s, &hp, table, input.alloc)
        } else {
            lin::translate(input.m, pos, s, &hp, table, input.alloc)
        };
        match tr {
            Ok(s) => {
                skels.insert(pos, s);
            }
            Err(e) => {
                refused.insert(pos, format!("translation: {e}"));
                table.remove(&pos);
                changed = true;
            }
        }
    }
    (!changed).then_some(skels)
}

/// Entry points: exported, tabled, or called from outside the table.
fn entry_points(m: &lin::Module<'_>, exported: &BTreeSet<usize>, table: &BTreeSet<usize>) -> BTreeSet<usize> {
    let mut entry: BTreeSet<usize> = exported.iter().copied().filter(|p| table.contains(p)).collect();
    let outside = m.bodies.iter().enumerate().filter(|(pos, _)| !table.contains(pos));
    for op in outside.flat_map(|(_, b)| b.ops.iter()) {
        if let wasmparser_op::Call { function_index } = op {
            if let Some(p) = function_index.checked_sub(m.imports) {
                entry.insert(p as usize);
            }
        }
    }
    entry
}

/// A function's next contract: its inferred postcondition, and (unless it
/// is an entry point or never called) the facts every call site entails.
fn next_contract(skel: &Skel, rets: &[Vec<L>], sites: &[Site], pos: usize, entry: bool) -> Contract {
    let post = infer_post(skel, rets);
    let mine: Vec<&Site> = sites.iter().filter(|s| s.callee == pos).collect();
    let (false, Some(first)) = (entry, mine.first()) else {
        return Contract { pre: Vec::new(), post };
    };
    let pre: Vec<L> = site_candidates(first).into_iter().filter(|c| mine.iter().all(|s| site_entails(s, c))).collect();
    Contract { pre: minimize(pre), post }
}

/// Drop what the rest implies (a bounded effort).
fn minimize(mut pre: Vec<L>) -> Vec<L> {
    let mut i = 0;
    while i < pre.len() && pre.len() <= 48 && !past_deadline() {
        let rest: Vec<L> = pre.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, l)| l.clone()).collect();
        if entailed(&rest, &pre[i]) {
            pre.remove(i);
        } else {
            i += 1;
        }
    }
    pre
}

type Results = BTreeMap<usize, Result<String, String>>;

/// Per function, the accesses its selection pass dropped (#4005 A4).
type Dropped = BTreeMap<usize, Vec<usize>>;

/// Contracts to a fixpoint (at most 8 rounds); each function's result
/// under the last, and (`skip`, a routine table) the accesses it dropped.
fn infer_contracts(skels: &BTreeMap<usize, Skel>, table: &BTreeSet<usize>, entry: &BTreeSet<usize>, skip: bool) -> (Results, Dropped) {
    let mut contracts: BTreeMap<usize, Contract> = BTreeMap::new();
    let mut results = BTreeMap::new();
    let mut dropped = BTreeMap::new();
    for round in 0..8 {
        if round > 0 && past_deadline() {
            break;
        }
        let mut sites: Vec<Site> = Vec::new();
        let mut rets: BTreeMap<usize, Vec<Vec<L>>> = BTreeMap::new();
        results.clear();
        dropped.clear();
        for &pos in table {
            let p = prove_proc(skels, &contracts, pos, skip);
            sites.extend(p.sites);
            rets.insert(pos, p.rets);
            results.insert(pos, p.stretch);
            if !p.dropped.is_empty() {
                dropped.insert(pos, p.dropped);
            }
        }
        let next: BTreeMap<usize, Contract> = table
            .iter()
            .map(|&pos| (pos, next_contract(&skels[&pos], rets.get(&pos).map_or(&[][..], Vec::as_slice), &sites, pos, entry.contains(&pos))))
            .collect();
        debug_round(round, &results, &sites, &next);
        if next == contracts {
            break;
        }
        contracts = next;
    }
    (results, dropped)
}

/// [`certify_routines`]'s answer: the routine certificate, its site
/// selection (one `<position> <op>...` line per procedure), the reason
/// each routine left out was, and per certified routine the accesses it
/// left counted.
pub type RoutinesCertified = (String, String, BTreeMap<usize, String>, BTreeMap<usize, (usize, usize)>);

/// The runtime routines' certificate (#4005 A4): every routine the anchor
/// file's `routines` line names, each access of its body a candidate
/// event. An access no region holds leaves the routine's selection (it
/// stays counted) and the table is proven again; a routine whose body
/// cannot be translated or proven otherwise leaves the table.
pub fn certify_routines(wasm: &[u8], anchors: &str) -> Result<RoutinesCertified, String> {
    let budget = almide_base::env::var("ALMIDE_LIN_PIVOTS").and_then(|s| s.parse().ok()).unwrap_or(PIVOT_BUDGET);
    BUDGET.with(|b| b.set(budget));
    lp::reset_pivots();
    let m = lin::Module::read(wasm)?;
    let exported = lin::exported_or_tabled(wasm, m.imports)?;
    let heap = lin::routine_params(anchors);
    let (alloc, ..) = almide_anchor_check::parse_anchors(anchors)?;
    let all: BTreeMap<usize, BTreeSet<usize>> = heap.keys().map(|&p| (p, lin::access_sites(&m, p))).collect();
    let mut sel = all.clone();
    let mut refused: BTreeMap<usize, String> = BTreeMap::new();
    // A routine with no access is nothing to certify.
    let mut table: BTreeSet<usize> = all.iter().filter(|(_, s)| !s.is_empty()).map(|(p, _)| *p).collect();
    for _ in 0..64 {
        let input = Input { m: &m, sites: &sel, heap: &heap, alloc, routine: true };
        let Some(skels) = translate_table(&input, &mut table, &mut refused) else { continue };
        let entry = entry_points(&m, &exported, &table);
        let (results, dropped) = infer_contracts(&skels, &table, &entry, true);
        if !dropped.is_empty() {
            for (p, ks) in dropped {
                if let Some(s) = sel.get_mut(&p) {
                    for k in ks {
                        s.remove(&k);
                    }
                }
            }
            continue;
        }
        let failed: Vec<usize> = results.iter().filter(|(_, r)| r.is_err()).map(|(p, _)| *p).collect();
        if failed.is_empty() {
            let cert: Vec<String> = table.iter().map(|p| results[p].clone().unwrap_or_default()).collect();
            let lines: Vec<String> = table
                .iter()
                .map(|p| {
                    let ops: Vec<String> = sel[p].iter().map(usize::to_string).collect();
                    format!("{p} {}", ops.join(" ")).trim_end().to_string()
                })
                .collect();
            let counts = table.iter().map(|p| (*p, (sel[p].len(), all[p].len() - sel[p].len()))).collect();
            return Ok((cert.join(" ; "), lines.join("\n"), refused, counts));
        }
        for p in failed {
            refused.insert(p, results[&p].clone().err().unwrap_or_default());
            table.remove(&p);
        }
    }
    Err("the routine table did not settle in 64 rounds".into())
}

/// ALMIDE_LIN_DEBUG: each round's results, call sites and preconditions.
fn debug_round(round: usize, results: &Results, sites: &[Site], next: &BTreeMap<usize, Contract>) {
    if !almide_base::env::flag("ALMIDE_LIN_DEBUG") {
        return;
    }
    for (p, r) in results {
        eprintln!("round {round}: {p}: {}", r.as_ref().err().map_or("ok", String::as_str));
    }
    for s in sites {
        eprintln!("  site -> {}: args {:?}", s.callee, s.args.iter().map(|(p, l, ok)| format!("{p}:{}{}", l.text(), if *ok { "!" } else { "" })).collect::<Vec<_>>());
        eprintln!("    facts {}", s.facts.iter().map(L::text).collect::<Vec<_>>().join(" "));
    }
    for (p, c) in next {
        eprintln!("  pre {p}: {}", c.pre.iter().map(L::text).collect::<Vec<_>>().join(" "));
    }
}

/// wasmparser's operator, through anchor-check's re-export.
mod wasmparser_op {
    pub use almide_anchor_check::lin::WasmOp::*;
}
