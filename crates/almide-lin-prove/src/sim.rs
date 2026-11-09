//! The prover's replay of RoutineLin's checker over one translated body
//! (lib.rs): the facts at each item, the holes it fills, the call sites and
//! returns the contract inference reads.

use super::*;

/// One call site, as the inference reads it.
#[derive(Clone, Debug)]
pub(crate) struct Site {
    pub(crate) callee: usize,
    pub(crate) facts: Vec<L>,
    /// (parameter, argument, its no-wrap proven).
    pub(crate) args: Vec<(Slot, L, bool)>,
}

/// A procedure's contract.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Contract {
    pub(crate) pre: Vec<L>,
    pub(crate) post: Vec<L>,
}

/// The slots a body writes and the regions it rebinds (RoutineLin's
/// `writes` / `rebinds`).
pub(crate) fn writes_rebinds(items: &[It], w: &mut BTreeSet<Slot>, r: &mut BTreeSet<Slot>) {
    for it in items {
        match it {
            It::Handle { h, .. }
            | It::Len { n: h, .. }
            | It::Cap { n: h, .. }
            | It::Set(h)
            | It::SetR { x: h, .. }
            | It::Asn { x: h, .. }
            | It::AsnU { x: h, .. } => {
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

pub(crate) struct Sim<'a> {
    pub(crate) skels: &'a BTreeMap<usize, Skel>,
    pub(crate) contracts: &'a BTreeMap<usize, Contract>,
    pub(crate) fills: Vec<String>,
    pub(crate) sites: Vec<Site>,
    pub(crate) rets: Vec<Vec<L>>,
    pub(crate) conts: Vec<Vec<Vec<L>>>,
    pub(crate) probe: usize,
    pub(crate) err: Option<String>,
    /// A runtime routine's selection pass (#4005 A4): an access no region
    /// holds is recorded here instead of failing the body — it leaves the
    /// selection, and the body is proven again without it (an access is
    /// no fact, so the rest proves the same).
    pub(crate) skip: bool,
    pub(crate) dropped: Vec<usize>,
}

pub(crate) fn region_vars(f: &[L]) -> BTreeSet<Slot> {
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
pub(crate) fn region_map(f: &[L], args: &[(Slot, L, bool)]) -> Vec<(Slot, Slot)> {
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
        for tok in lin::template(0, &sk) {
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

    /// The facts at an item's end, for the items that only rewrite them
    /// (`None` for the others).
    fn rewrite(f: &[L], it: &It) -> Option<Vec<L>> {
        let mut g = match it {
            It::Handle { h, r } => {
                let mut g = eqs(&V::S(*h), &v1(V::B(*r)));
                g.extend(kill(f, &V::S(*h)));
                return Some(g);
            }
            It::Len { n, r, s } => {
                let vn = v1(V::S(*n));
                let mut g = vec![lneg(&vn), lsub(&lmul(*s, &vn), &v1(V::C(*r)))];
                g.extend(kill(f, &V::S(*n)));
                return Some(g);
            }
            It::Cap { n, r } => {
                let mut g = eqs(&V::S(*n), &v1(V::C(*r)));
                g.extend(kill(f, &V::S(*n)));
                return Some(g);
            }
            It::Wf(r) => {
                let (vb, vc) = (v1(V::B(*r)), v1(V::C(*r)));
                vec![lneg(&vb), lneg(&vc), ladd(&ladd(&vb, &vc), &kz(12 - ADDR + 1))]
            }
            It::Static { r, b0, c0 } => {
                let mut g = eqs(&V::B(*r), &kz(*b0));
                g.extend(eqs(&V::C(*r), &kz(*c0)));
                g.extend(kill_region(f, *r));
                return Some(g);
            }
            It::Set(x) => return Some(kill(f, &V::S(*x))),
            It::SetR { x, lo, hi } => {
                let vx = v1(V::S(*x));
                let mut g = vec![lsub(&kz(*lo), &vx), lsub(&vx, &kz(*hi))];
                g.extend(kill(f, &V::S(*x)));
                return Some(g);
            }
            _ => return None,
        };
        g.extend(f.iter().cloned());
        Some(g)
    }

    /// An access: some region holds it, by three proofs.
    fn access(&mut self, f: &[L], addr: &lin::Lin, off: u64, w: u32, store: bool, site: usize) -> Option<()> {
        let addr = from_lin(addr);
        let (off, w) = (i128::from(off), i128::from(w));
        let held = region_vars(f).into_iter().find_map(|r| {
            let (vb, vc) = (v1(V::B(r)), v1(V::C(r)));
            let p0 = prove(f, &lneg(&addr));
            let p1 = prove(f, &lsub(&vb, &ladd(&addr, &kz(off))));
            let p2 = prove(f, &lsub(&ladd(&addr, &kz(off + w - 12)), &ladd(&vb, &vc)));
            Some([r.to_string(), p0?, p1?, p2?])
        });
        let Some(fills) = held else {
            if self.skip {
                if self.probe == 0 && !self.dropped.contains(&site) {
                    self.dropped.push(site);
                }
                self.fills.extend(["0".to_string(), "()1".to_string(), "()1".to_string(), "()1".to_string()]);
                return Some(());
            }
            return self.fail(format!("{} of {w} at {}+{off} not in any region", if store { "store" } else { "load" }, addr.text()));
        };
        self.fills.extend(fills);
        Some(())
    }

    fn alloc(&mut self, f: &[L], h: Slot, r: Slot, sz: &lin::Lin) -> Option<Vec<L>> {
        let sz = from_lin(sz);
        let (Some(lo), Some(hi)) = (prove(f, &lneg(&sz)), prove(f, &lsub(&sz, &kz(ADDR - 1)))) else {
            return self.fail(format!("allocation size {} not in [0, 2^32)", sz.text()));
        };
        self.fill(lo);
        self.fill(hi);
        if [V::S(h), V::B(r), V::C(r)].iter().any(|v| mentions(v, &sz)) {
            return self.fail("allocation size names its own result".into());
        }
        let mut g = eqs(&V::S(h), &v1(V::B(r)));
        g.push(lsub(&sz, &v1(V::C(r))));
        g.extend(kill_region(&kill(f, &V::S(h)), r));
        Some(g)
    }

    fn assign(&mut self, f: Vec<L>, x: Slot, l: &lin::Lin) -> Vec<L> {
        let l = from_lin(l);
        let vx = V::S(x);
        let Some((lo, hi)) = no_wrap(&f, &l) else {
            self.fill("()1".into());
            self.fill("()1".into());
            return kill(&f, &vx);
        };
        self.fill(lo);
        self.fill(hi);
        asn_core(&f, &vx, &l)
    }

    /// RoutineLin's `asnu_facts`: an i32 result proven in [0, 2^32) is
    /// LAsn's (the same two proofs show it does not wrap), else only its
    /// range.
    fn assign_u(&mut self, f: Vec<L>, x: Slot, l: &lin::Lin) -> Vec<L> {
        let l = from_lin(l);
        let vx = V::S(x);
        if let (Some(lo), Some(hi)) = (prove(&f, &lneg(&l)), prove(&f, &lsub(&l, &kz(ADDR - 1)))) {
            self.fill(lo);
            self.fill(hi);
            return asn_core(&f, &vx, &l);
        }
        self.fill("()1".into());
        self.fill("()1".into());
        let mut g = vec![lsub(&kz(0), &v1(vx.clone())), lsub(&v1(vx.clone()), &kz(ADDR - 1))];
        g.extend(kill(&f, &vx));
        g
    }

    /// A test's facts on its two sides, its six proof holes filled.
    fn test_facts(&mut self, f: &[L], a: &lin::Lin, b: &lin::Lin, sgn: bool) -> (Vec<L>, Vec<L>) {
        let (a, b) = (from_lin(a), from_lin(b));
        let unit = || ("()1".to_string(), "()1".to_string());
        let pa = no_wrap(f, &a);
        let pb = no_wrap(f, &b);
        let nn = if sgn { Some(unit()) } else { prove(f, &lneg(&a)).zip(prove(f, &lneg(&b))) };
        let ok = pa.is_some() && pb.is_some() && nn.is_some();
        for (x, y) in [pa.unwrap_or_else(unit), pb.unwrap_or_else(unit), nn.unwrap_or_else(unit)] {
            self.fill(x);
            self.fill(y);
        }
        if ok {
            (vec![lsub(&b, &a)], vec![ladd(&lsub(&a, &b), &kz(1))])
        } else {
            (Vec::new(), Vec::new())
        }
    }

    fn branch(&mut self, inv: Option<&Vec<L>>, post: &[L], f: Vec<L>, it: &It, rest: &[It]) -> Option<Flow> {
        let (ft, fe, thn, els) = match it {
            It::Test { a, b, sgn, thn, els } => {
                let (ft, fe) = self.test_facts(&f, a, b, *sgn);
                (ft, fe, thn, els)
            }
            It::Branch { thn, els } => (Vec::new(), Vec::new(), thn, els),
            _ => return None,
        };
        let with = |mut g: Vec<L>| {
            g.extend(f.iter().cloned());
            g
        };
        let (st, bt) = self.lcheck(inv, post, with(ft), thn)?;
        let (se, be) = self.lcheck(inv, post, with(fe), els)?;
        let j = fjoin(&bt, &be);
        let Some(g) = fjoin(&st, &se) else {
            self.dead(rest);
            return Some(Flow::End(None, j));
        };
        Some(Flow::Joined(j, g))
    }

    /// Houdini: the entry facts, minus what a back edge loses, to a fixpoint.
    fn invariant(&mut self, post: &[L], f: &[L], body: &[It]) -> Option<Vec<L>> {
        let mut cand = f.to_vec();
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
                return Some(cand);
            }
            cand = next;
        }
    }

    fn lloop(&mut self, post: &[L], f: &[L], body: &[It], rest: &[It]) -> Option<Flow> {
        let cand = self.invariant(post, f, body)?;
        self.fill(cand.iter().map(L::text).collect::<Vec<_>>().join(" "));
        self.fill(vec!["()1".to_string(); cand.len()].join(" "));
        self.conts.push(Vec::new());
        let r = self.lcheck(Some(&cand), post, cand.clone(), body);
        self.conts.pop();
        let (sb, bb) = r?;
        match fjoin(&sb, &bb) {
            None => {
                self.dead(rest);
                Some(Flow::End(None, None))
            }
            Some(g) => Some(Flow::Go(g)),
        }
    }

    fn back_edge(&mut self, inv: Option<&Vec<L>>, f: Vec<L>) -> Option<Flow> {
        let Some(iv) = inv else { return self.fail("a back edge outside a loop".into()) };
        if self.probe == 0 {
            let p = self.proofs(&f, iv)?;
            self.fill(p);
            return Some(Flow::End(None, None));
        }
        if let Some(top) = self.conts.last_mut() {
            top.push(f);
        }
        self.fill(String::new());
        Some(Flow::End(None, None))
    }

    fn ret(&mut self, post: &[L], f: Vec<L>) -> Option<Flow> {
        if self.probe == 0 {
            self.rets.push(f.clone());
        }
        let p = self.proofs(&f, post)?;
        self.fill(p);
        Some(Flow::End(None, None))
    }

    /// A call's arguments: each with its no-wrap proven or not.
    fn call_args(&mut self, f: &[L], args: &[(Slot, lin::Lin)]) -> Vec<(Slot, L, bool)> {
        let mut sargs = Vec::new();
        for (p, l) in args {
            let l = from_lin(l);
            let nw = no_wrap(f, &l);
            self.fill(nw.as_ref().map_or_else(String::new, |(lo, hi)| format!(" ! {lo} {hi}")));
            sargs.push((*p, l, nw.is_some()));
        }
        sargs
    }

    fn call(&mut self, f: Vec<L>, g: usize, args: &[(Slot, lin::Lin)], res: Slot) -> Option<Vec<L>> {
        let sargs = self.call_args(&f, args);
        let valid: Vec<(Slot, L)> = sargs.iter().filter(|a| a.2).map(|(p, l, _)| (*p, l.clone())).collect();
        let reps = region_map(&f, &sargs);
        self.fill(reps.iter().map(|(a, b)| format!("{a}:{b}")).collect::<Vec<_>>().join(" "));
        self.fill(String::new());
        // The precondition, translated into the caller's terms.
        let none = BTreeSet::new();
        let tr = |v: &V| translate_var(v, &valid, &reps, None, &none, &none);
        let pre = self.contracts.get(&g).map(|c| c.pre.clone()).unwrap_or_default();
        let mut pre2 = Vec::new();
        for c in &pre {
            let Some(c2) = tr_lin(&tr, c) else {
                return self.fail(format!("call of {g}: precondition {} not expressible here", c.text()));
            };
            pre2.push(c2);
        }
        let p = self.proofs(&f, &pre2)?;
        self.fill(p);
        if self.probe == 0 {
            self.sites.push(Site { callee: g, facts: f.clone(), args: sargs });
        }
        // The postcondition, translated back (RoutineLin's tr_out).
        let post_c = self.contracts.get(&g).map(|c| c.post.clone()).unwrap_or_default();
        let (mut wr, mut rb) = (BTreeSet::new(), BTreeSet::new());
        if let Some(sk) = self.skels.get(&g) {
            writes_rebinds(&sk.body, &mut wr, &mut rb);
        }
        let ret = self.skels.get(&g).map_or(u32::MAX, |k| k.rslot);
        let tro = |v: &V| translate_var(v, &valid, &reps, Some((ret, res)), &wr, &rb);
        let mut g2: Vec<L> = post_c.iter().filter_map(|c| tr_lin(&tro, c)).collect();
        g2.extend(kill(&f, &V::S(res)));
        Some(g2)
    }

    /// One item: the facts after it, a join of two arms, or the end of the
    /// stretch.
    fn step(&mut self, inv: Option<&Vec<L>>, post: &[L], f: Vec<L>, it: &It, rest: &[It]) -> Option<Flow> {
        if let Some(g) = Self::rewrite(&f, it) {
            return Some(Flow::Go(g));
        }
        match it {
            It::Acc { w, off, addr, store, site } => self.access(&f, addr, *off, *w, *store, *site).map(|()| Flow::Go(f)),
            It::Alloc { h, r, sz } => self.alloc(&f, *h, *r, sz).map(Flow::Go),
            It::Asn { x, l } => Some(Flow::Go(self.assign(f, *x, l))),
            It::AsnU { x, l } => Some(Flow::Go(self.assign_u(f, *x, l))),
            It::Test { .. } | It::Branch { .. } => self.branch(inv, post, f, it, rest),
            It::Loop { body } => self.lloop(post, &f, body, rest),
            It::Cont => self.back_edge(inv, f),
            It::Brk => Some(Flow::End(None, Some(f))),
            It::Ret => self.ret(post, f),
            It::Call { f: g, args, res } => self.call(f, *g, args, *res).map(Flow::Go),
            _ => Some(Flow::End(None, None)),
        }
    }

    pub(crate) fn lcheck(&mut self, inv: Option<&Vec<L>>, post: &[L], mut f: Vec<L>, items: &[It]) -> Option<(St, St)> {
        let mut joins: Vec<St> = Vec::new();
        let finish = |joins: Vec<St>, s: St, b: St| Some((s, joins.iter().rev().fold(b, |acc, j| fjoin(j, &acc))));
        for (i, it) in items.iter().enumerate() {
            match self.step(inv, post, f, it, &items[i + 1..])? {
                Flow::Go(g) => f = g,
                Flow::Joined(j, g) => {
                    joins.push(j);
                    f = g;
                }
                Flow::End(s, b) => return finish(joins, s, b),
            }
        }
        finish(joins, Some(f), None)
    }
}

/// LAsn's facts once `l` is shown not to wrap: `x = l`, or the invertible
/// step `x := x + d` substituted through, else nothing on x.
fn asn_core(f: &[L], vx: &V, l: &L) -> Vec<L> {
    match coef(&l.t, vx) {
        0 => {
            let mut g = eqs(vx, l);
            g.extend(kill(f, vx));
            g
        }
        1 => {
            let d = lsub(&v1(vx.clone()), &L { t: dropv(vx, &l.t), c: l.c });
            f.iter().map(|g| subst(vx, &d, g)).collect()
        }
        _ => kill(f, vx),
    }
}

/// What one item leaves: the facts to go on with, a join of two arms with
/// the facts after it, or the end of the stretch (its state and its break).
enum Flow {
    Go(Vec<L>),
    Joined(St, Vec<L>),
    End(St, St),
}

/// A callee's variable in the caller's terms: an argument, the region an
/// argument points into, and (for a postcondition) the result slot; a slot
/// the callee writes or a region it rebinds says nothing after the call.
fn translate_var(v: &V, valid: &[(Slot, L)], reps: &[(Slot, Slot)], ret: Option<(Slot, Slot)>, wr: &BTreeSet<Slot>, rb: &BTreeSet<Slot>) -> Option<L> {
    let region = |r2: &Slot, mk: fn(Slot) -> V| reps.iter().find(|(a, _)| a == r2).map(|(_, r)| v1(mk(*r)));
    match v {
        V::S(q) if ret.is_some_and(|(rs, _)| rs == *q) => ret.map(|(_, res)| v1(V::S(res))),
        V::S(q) if wr.contains(q) => None,
        V::S(q) => valid.iter().find(|(p, _)| p == q).map(|(_, l)| l.clone()),
        V::B(r2) | V::C(r2) if rb.contains(r2) => None,
        V::B(r2) => region(r2, V::B),
        V::C(r2) => region(r2, V::C),
    }
}

/// RoutineLin's `tr_lin`: a right fold of `ladd (lmul a d) rest`, then the
/// constant.
pub(crate) fn tr_lin(tr: &dyn Fn(&V) -> Option<L>, l: &L) -> Option<L> {
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
