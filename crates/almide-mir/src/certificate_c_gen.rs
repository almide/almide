// ── faithfulness over the read shapes (#3259) ──
// `gen_wellformed` draws no prims and no calls, so it never compared the
// certificate and `verify_ownership` on a dereference or a call handle
// argument — the shapes #3233 added to the certificate. This generator draws
// them: addresses built from a handle (`prim.handle` + offset, `ElemAddr`),
// `LoadHandle` / `Load` / `Store` through an address made EARLIER (so the
// object may have been freed in between), direct loads through a handle, and
// `Call` handle args — over owned objects and a borrowed heap param. #3261:
// the loaded CHILD handles are reused — read, dereferenced, loaded from, and
// `Dup`'d (a reference of the frame's own on the child).

/// The generator state: the op list, the live owned handles, which object each
/// handle denotes, the address pool, the raw loaded children, and the
/// handles `Dup`'d from a child (directly or from another such handle).
struct ReadGen {
    st: u64,
    next: u32,
    ops: Vec<Op>,
    live: Vec<ValueId>,
    obj: std::collections::BTreeMap<ValueId, ValueId>,
    param: Option<ValueId>,
    addrs: Vec<ValueId>,
    children: Vec<ValueId>,
    child_dups: std::collections::BTreeSet<ValueId>,
    elem_addrs: std::collections::BTreeSet<ValueId>,
}

impl ReadGen {
    fn fresh(&mut self) -> ValueId {
        let v = ValueId(self.next);
        self.next += 1;
        v
    }

    fn pick(&mut self, pool: &[ValueId]) -> Option<ValueId> {
        if pool.is_empty() {
            return None;
        }
        Some(pool[(next_rand(&mut self.st) as usize) % pool.len()])
    }

    /// Every handle that denotes an object (live or not), plus the param.
    fn handles(&self) -> Vec<ValueId> {
        self.obj.keys().copied().collect()
    }

    /// Is `o` still held: the param (the caller holds it), or some live handle on it.
    fn object_live(&self, o: ValueId) -> bool {
        Some(o) == self.param || self.live.iter().any(|h| self.obj.get(h) == Some(&o))
    }

    /// The call-arg pool. A handle that was dropped while a sibling handle
    /// keeps its object alive is left out: `verify_ownership` checks a call
    /// arg per HANDLE and the certificate per OBJECT, a known difference that
    /// `gen_wellformed` also steers around (it only `Dup`s / `Borrow`s live
    /// handles). Live handles and handles whose whole object is gone are in.
    /// A released child `Dup` is out for the same per-handle reason; the raw
    /// children are in.
    fn call_arg_pool(&self) -> Vec<ValueId> {
        let mut pool: Vec<ValueId> = self
            .handles()
            .into_iter()
            .filter(|h| self.live.contains(h) || Some(*h) == self.param || !self.object_live(self.obj[h]))
            .filter(|h| self.live.contains(h) || !self.child_dups.contains(h))
            .collect();
        pool.extend(self.children.iter().copied());
        pool
    }

    fn alloc(&mut self) {
        let v = self.fresh();
        self.ops.push(Op::Alloc { dst: v, repr: heap(), init: Init::Opaque });
        self.obj.insert(v, v);
        self.live.push(v);
    }

    fn dup(&mut self) {
        let mut pool = self.live.clone();
        pool.extend(self.param);
        pool.extend(self.children.iter().copied());
        let Some(src) = self.pick(&pool) else { return };
        let v = self.fresh();
        self.ops.push(Op::Dup { dst: v, src });
        let o = self.obj.get(&src).copied().unwrap_or(src);
        self.obj.insert(v, o);
        if self.children.contains(&src) || self.child_dups.contains(&src) {
            self.child_dups.insert(v);
        }
        self.live.push(v);
    }

    fn drop_one(&mut self) {
        if self.live.is_empty() {
            return;
        }
        let i = (next_rand(&mut self.st) as usize) % self.live.len();
        let v = self.live.remove(i);
        self.ops.push(Op::Drop { v });
    }

    /// `prim.handle(h) + off`, or `ElemAddr(h, idx)` (which reads the list's
    /// bounds, so it is itself a dereference), kept for a later load.
    fn address(&mut self) {
        let mut pool = self.handles();
        pool.extend(self.children.iter().copied());
        let Some(src) = self.pick(&pool) else { return };
        let (h, k, a) = (self.fresh(), self.fresh(), self.fresh());
        self.ops.push(Op::ConstInt { dst: k, value: 8 });
        if next_rand(&mut self.st) % 3 == 0 {
            self.ops.push(Op::Prim { kind: PrimKind::ElemAddr, dst: Some(a), args: vec![src, k] });
            self.elem_addrs.insert(a);
        } else {
            self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![src] });
            self.ops.push(Op::IntBinOp { dst: a, op: crate::IntOp::Add, a: h, b: k });
        }
        self.addrs.push(a);
    }

    /// A load or store through an address from the pool, or directly through a
    /// handle or a raw child. A `LoadHandle` result joins the child pool.
    fn deref(&mut self) {
        let mut pool = self.addrs.clone();
        pool.extend(self.handles());
        pool.extend(self.children.iter().copied());
        let Some(a) = self.pick(&pool) else { return };
        let d = self.fresh();
        match next_rand(&mut self.st) % 3 {
            0 => {
                self.ops.push(Op::Prim { kind: PrimKind::LoadHandle, dst: Some(d), args: vec![a] });
                // `verify_ownership` keeps a child loaded through an `ElemAddr`
                // address off its model (an unknown handle, dead to every
                // use), where the certificate tracks it: not reused.
                if !self.elem_addrs.contains(&a) {
                    self.children.push(d);
                }
            }
            1 => self.ops.push(Op::Prim { kind: PrimKind::Load { width: 8 }, dst: Some(d), args: vec![a] }),
            _ => {
                self.ops.push(Op::ConstInt { dst: d, value: 1 });
                self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![a, d] });
            }
        }
    }

    /// A runtime call borrowing one handle.
    fn call(&mut self) {
        let Some(v) = self.pick(&self.call_arg_pool()) else { return };
        self.ops.push(Op::Call { dst: None, func: RtFn::PrintStr, args: vec![CallArg::Handle(v)], result: None });
    }

    /// A `Borrow` of a live handle or a raw child.
    fn borrow(&mut self) {
        let mut pool = self.live.clone();
        pool.extend(self.children.iter().copied());
        let Some(v) = self.pick(&pool) else { return };
        self.ops.push(Op::Borrow { v });
    }

    fn step(&mut self) {
        match next_rand(&mut self.st) % 7 {
            0 => self.alloc(),
            1 => self.dup(),
            2 => self.drop_one(),
            3 => self.address(),
            4 => self.deref(),
            5 => self.borrow(),
            _ => self.call(),
        }
    }
}

fn gen_reads(seed: u64) -> MirFunction {
    let mut st = seed.wrapping_add(7);
    let with_param = next_rand(&mut st) % 4 == 0;
    let mut g = ReadGen {
        st,
        next: 0,
        ops: Vec::new(),
        live: Vec::new(),
        obj: std::collections::BTreeMap::new(),
        param: None,
        addrs: Vec::new(),
        children: Vec::new(),
        child_dups: std::collections::BTreeSet::new(),
        elem_addrs: std::collections::BTreeSet::new(),
    };
    if with_param {
        let p = g.fresh();
        g.param = Some(p);
        g.obj.insert(p, p);
    }
    let steps = 3 + (next_rand(&mut g.st) % 12) as usize;
    for _ in 0..steps {
        g.step();
    }
    // Usually release what is left, so most draws are leak-free and the read
    // checks decide the verdict.
    if next_rand(&mut g.st) % 4 != 0 {
        for v in std::mem::take(&mut g.live) {
            g.ops.push(Op::Drop { v });
        }
    }
    let params = g.param.map(|p| vec![MirParam { value: p, repr: heap() }]).unwrap_or_default();
    MirFunction { name: "reads".into(), params, ops: g.ops, ..Default::default() }
}

#[test]
fn certificate_verdict_matches_verify_ownership_on_reads() {
    let (mut accepted, mut rejected) = (0, 0);
    for seed in 0u64..4000 {
        let f = gen_reads(seed);
        let cert = ownership_certificate(&f);
        let cert_ok = cert_all_balanced(&cert);
        let verify_ok = verify_ownership(&f).is_ok();
        assert_eq!(
            cert_ok, verify_ok,
            "seed {seed}: certificate says {cert_ok}, verify_ownership says {verify_ok}\ncert: {cert:?}\nops: {:?}",
            f.ops
        );
        if verify_ok {
            accepted += 1;
        } else {
            rejected += 1;
        }
    }
    // The corpus spans both verdicts.
    assert!(accepted > 400 && rejected > 400, "accepted {accepted}, rejected {rejected}");
}

/// #3261: a `LoadHandle` CHILD (a payload loaded through an address into its
/// parent) is live while its parent is, or while a reference the frame took
/// on it (`Dup`) is held. A raw read probes the child's own line when that is
/// positive, else its parent's — so both sides reject a raw read after the
/// parent's last release, and both accept it while a `Dup` keeps the child.
#[test]
fn loaded_child_reads_agree_with_verify_ownership() {
    let (o, h, off, addr, c, d) = (ValueId(0), ValueId(1), ValueId(2), ValueId(3), ValueId(4), ValueId(5));
    let head = || vec![
        Op::Alloc { dst: o, repr: heap(), init: Init::Opaque },
        Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![o] },
        Op::ConstInt { dst: off, value: 12 },
        Op::IntBinOp { dst: addr, op: crate::IntOp::Add, a: h, b: off },
        Op::Prim { kind: PrimKind::LoadHandle, dst: Some(c), args: vec![addr] },
    ];
    let read = || Op::Call { dst: None, func: RtFn::PrintStr, args: vec![CallArg::Handle(c)], result: None };
    let case = |tail: Vec<Op>| {
        let mut ops = head();
        ops.extend(tail);
        func(ops)
    };
    // A raw read after the parent's last release: rejected by both.
    let raw = case(vec![Op::Drop { v: o }, read()]);
    assert_eq!(ownership_certificate(&raw), "ibdb\n");
    assert!(!cert_all_balanced(&ownership_certificate(&raw)));
    assert!(verify_ownership(&raw).is_err());
    // A Dup keeps the child: the raw read probes the child's own line.
    let kept = case(vec![Op::Dup { dst: d, src: c }, Op::Drop { v: o }, read(), Op::Drop { v: d }]);
    assert_eq!(ownership_certificate(&kept), "ibbd\nabd\n");
    assert!(cert_all_balanced(&ownership_certificate(&kept)));
    assert_eq!(verify_ownership(&kept), Ok(()));
    // The Dup released too: nothing keeps the child, rejected by both.
    let gone = case(vec![Op::Dup { dst: d, src: c }, Op::Drop { v: d }, Op::Drop { v: o }, read()]);
    assert_eq!(ownership_certificate(&gone), "ibbdb\nad\n");
    assert!(!cert_all_balanced(&ownership_certificate(&gone)));
    assert!(verify_ownership(&gone).is_err());
    // A Dup of a child whose parent is gone reads a freed block.
    let late = case(vec![Op::Drop { v: o }, Op::Dup { dst: d, src: c }, Op::Drop { v: d }]);
    assert_eq!(ownership_certificate(&late), "ibdb\nad\n");
    assert!(!cert_all_balanced(&ownership_certificate(&late)));
    assert!(verify_ownership(&late).is_err());
    // The poisoned certificate is `gone`'s witness.
    assert_eq!(
        ownership_certificate(&gone),
        include_str!("../../../proofs/poisoned-certs/3261-loaded-child-after-parent-free.cert")
    );
}

/// #3261: a `Dup` of a GRANDCHILD (loaded through the raw child) keeps the
/// grandchild, not the child. `verify_ownership` counted every child `Dup` on
/// the root parent, so it read the raw child as live after the parent was
/// freed; both sides now reject the read.
#[test]
fn a_grandchild_dup_does_not_keep_the_child() {
    let v = ValueId;
    let f = func(vec![
        Op::Alloc { dst: v(0), repr: heap(), init: Init::Opaque },
        Op::Prim { kind: PrimKind::Handle, dst: Some(v(1)), args: vec![v(0)] },
        Op::ConstInt { dst: v(2), value: 12 },
        Op::IntBinOp { dst: v(3), op: crate::IntOp::Add, a: v(1), b: v(2) },
        Op::Prim { kind: PrimKind::LoadHandle, dst: Some(v(4)), args: vec![v(3)] },
        Op::Prim { kind: PrimKind::Handle, dst: Some(v(5)), args: vec![v(4)] },
        Op::IntBinOp { dst: v(6), op: crate::IntOp::Add, a: v(5), b: v(2) },
        Op::Prim { kind: PrimKind::LoadHandle, dst: Some(v(7)), args: vec![v(6)] },
        Op::Dup { dst: v(8), src: v(7) },
        Op::Drop { v: v(0) },
        Op::Call { dst: None, func: RtFn::PrintStr, args: vec![CallArg::Handle(v(4))], result: None },
        Op::Drop { v: v(8) },
    ]);
    assert_eq!(ownership_certificate(&f), "ibbbdb\nad\n");
    assert!(!cert_all_balanced(&ownership_certificate(&f)));
    assert!(verify_ownership(&f).is_err());
    // Reading the GRANDCHILD there is fine: its own Dup holds it.
    let mut ok = f.clone();
    ok.ops[10] = Op::Call { dst: None, func: RtFn::PrintStr, args: vec![CallArg::Handle(v(7))], result: None };
    assert!(cert_all_balanced(&ownership_certificate(&ok)));
    assert_eq!(verify_ownership(&ok), Ok(()));
}
