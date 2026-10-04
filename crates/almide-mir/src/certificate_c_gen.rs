// ── faithfulness over the read shapes (#3259) ──
// `gen_wellformed` draws no prims and no calls, so it never compared the
// certificate and `verify_ownership` on a dereference or a call handle
// argument — the shapes #3233 added to the certificate. This generator draws
// them: addresses built from a handle (`prim.handle` + offset, `ElemAddr`),
// `LoadHandle` / `Load` / `Store` through an address made EARLIER (so the
// object may have been freed in between), direct loads through a handle, and
// `Call` handle args — over owned objects and a borrowed heap param. #3261:
// the loaded CHILD handles are reused — read, dereferenced, loaded from, and
// `Dup`'d (a reference of the frame's own on the child). #3263: a child
// loaded through an `ElemAddr` address is reused like any other, and a call
// arg may be any handle — one released while a sibling keeps its object, or a
// `prim.handle` carrier — since both sides now check a call arg per object.

/// The generator state: the op list, the live owned handles, which object each
/// handle denotes, the address pool, the raw loaded children, and the
/// `prim.handle` carriers.
struct ReadGen {
    st: u64,
    next: u32,
    ops: Vec<Op>,
    live: Vec<ValueId>,
    obj: std::collections::BTreeMap<ValueId, ValueId>,
    param: Option<ValueId>,
    addrs: Vec<ValueId>,
    children: Vec<ValueId>,
    carriers: Vec<ValueId>,
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

    /// The call-arg pool: every handle, live or released, every raw child and
    /// every `prim.handle` carrier.
    fn call_arg_pool(&self) -> Vec<ValueId> {
        let mut pool = self.handles();
        pool.extend(self.children.iter().copied());
        pool.extend(self.carriers.iter().copied());
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
        } else {
            self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![src] });
            self.ops.push(Op::IntBinOp { dst: a, op: crate::IntOp::Add, a: h, b: k });
            self.carriers.push(h);
        }
        self.addrs.push(a);
    }

    /// A load or store through an address from the pool, or directly through a
    /// handle, a raw child or a carrier. A `LoadHandle` result joins the child pool.
    fn deref(&mut self) {
        let mut pool = self.addrs.clone();
        pool.extend(self.handles());
        pool.extend(self.children.iter().copied());
        pool.extend(self.carriers.iter().copied());
        let Some(a) = self.pick(&pool) else { return };
        let d = self.fresh();
        match next_rand(&mut self.st) % 3 {
            0 => {
                self.ops.push(Op::Prim { kind: PrimKind::LoadHandle, dst: Some(d), args: vec![a] });
                self.children.push(d);
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

    /// #3269: rebind a slot (`xs = list.set(xs, i, v)`): a new block, the
    /// old block's `Drop`, the `SetLocal`. The slot keeps its handle, now on the
    /// new block; the old block's children are no longer kept by the slot.
    fn rebind(&mut self) {
        // A slot no other handle ever denoted: a `Dup` of it (live or
        // released) is a handle on the old block after the rebind, which the
        // certificate's per-object line cannot tell from the slot (the
        // existing per-handle difference); a view of it (child, address,
        // carrier) is what this step exercises.
        let slots: Vec<ValueId> = self
            .live
            .iter()
            .copied()
            .filter(|&l| self.obj.get(&l) == Some(&l) && self.obj.iter().all(|(&h, &o)| h == l || o != l))
            .collect();
        let Some(l) = self.pick(&slots) else { return };
        let new = self.fresh();
        self.ops.push(Op::Alloc { dst: new, repr: heap(), init: Init::Opaque });
        self.ops.push(Op::Drop { v: l });
        self.ops.push(Op::SetLocal { local: l, src: new });
    }

    fn step(&mut self) {
        // The high bits: the LCG's low three bits cycle with period 8, so a
        // `% 8` on them never draws some arms between the other draws.
        match (next_rand(&mut self.st) >> 33) % 8 {
            0 => self.alloc(),
            1 => self.dup(),
            2 => self.drop_one(),
            3 => self.address(),
            4 => self.deref(),
            5 => self.borrow(),
            6 => self.rebind(),
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
        carriers: Vec::new(),
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

/// #3267: the read shapes around one two-armed `if`. The generator's pools
/// are NOT scoped to the path, so an arm reads handles the other arm defined
/// and the code after the join reads handles one arm defined: both sides must
/// reject those, and agree everywhere else.
/// A cut never splits a rebind (`Alloc new; Drop l; SetLocal l = new`) across
/// a branch boundary: the lowering emits the three together.
fn whole_rebind_cut(ops: &[Op], mut k: usize) -> usize {
    let is_set = |i: usize| matches!(ops.get(i), Some(Op::SetLocal { .. }));
    if is_set(k + 1) {
        k += 2;
    } else if is_set(k) {
        k += 1;
    }
    k.min(ops.len())
}

fn gen_branch_reads(seed: u64) -> MirFunction {
    let mut f = gen_reads(seed);
    let mut st = seed.wrapping_add(11);
    let n = f.ops.len();
    // Cut the body into prefix / then / else / suffix at three points.
    let mut cuts: Vec<usize> = (0..3).map(|_| whole_rebind_cut(&f.ops, (next_rand(&mut st) as usize) % (n + 1))).collect();
    cuts.sort_unstable();
    let c = ValueId(10_000);
    let mut ops = vec![Op::ConstInt { dst: c, value: 1 }];
    ops.extend_from_slice(&f.ops[..cuts[0]]);
    ops.push(Op::IfThen { cond: c, dst: None });
    ops.extend_from_slice(&f.ops[cuts[0]..cuts[1]]);
    ops.push(Op::Else { val: None });
    ops.extend_from_slice(&f.ops[cuts[1]..cuts[2]]);
    ops.push(Op::EndIf { val: None });
    ops.extend_from_slice(&f.ops[cuts[2]..]);
    f.ops = ops;
    f
}

#[test]
fn certificate_verdict_matches_verify_ownership_around_a_branch() {
    let (mut accepted, mut rejected) = (0, 0);
    for seed in 0u64..4000 {
        let f = gen_branch_reads(seed);
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
    assert!(accepted > 200 && rejected > 400, "accepted {accepted}, rejected {rejected}");
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

/// #3263: a `prim.handle` carrier of a raw child, passed as a call arg. Both
/// sides accept it while the child's parent is live (`string.eq(prim.handle
/// (child))` in every derived `eq`), and both reject it after the parent's
/// last release. The certificate used to miss the second case: the carrier
/// sits only in `addr_of`, so its call-arg probe found no line.
#[test]
fn a_child_carrier_call_arg_is_probed_on_the_child() {
    let v = ValueId;
    let call = Op::Call { dst: None, func: RtFn::PrintStr, args: vec![CallArg::Handle(v(2))], result: None };
    let head = vec![
        Op::Alloc { dst: v(0), repr: heap(), init: Init::Opaque },
        Op::Prim { kind: PrimKind::LoadHandle, dst: Some(v(1)), args: vec![v(0)] },
        Op::Prim { kind: PrimKind::Handle, dst: Some(v(2)), args: vec![v(1)] },
    ];
    let live = func([head.clone(), vec![call.clone(), Op::Drop { v: v(0) }]].concat());
    assert_eq!(ownership_certificate(&live), "ibbd\n");
    assert_eq!(verify_ownership(&live), Ok(()));
    let freed = func([head, vec![Op::Drop { v: v(0) }, call]].concat());
    assert_eq!(
        ownership_certificate(&freed),
        include_str!("../../../proofs/poisoned-certs/3263-child-carrier-callarg-after-free.cert")
    );
    assert!(!cert_all_balanced(&ownership_certificate(&freed)));
    assert!(verify_ownership(&freed).is_err());
}

/// #3263: a child loaded through an `ElemAddr` address is tracked like one
/// loaded through `prim.handle + off`. Over a borrowed list param (every
/// `__list_dec_go` / `__list_enc_go`) a call on the element is fine; over an
/// owned list it is fine until the list's last release.
#[test]
fn an_elem_addr_child_is_tracked() {
    let v = ValueId;
    let call = Op::Call { dst: None, func: RtFn::PrintStr, args: vec![CallArg::Handle(v(3))], result: None };
    let load = |list: ValueId| {
        vec![
            Op::ConstInt { dst: v(1), value: 0 },
            Op::Prim { kind: PrimKind::ElemAddr, dst: Some(v(2)), args: vec![list, v(1)] },
            Op::Prim { kind: PrimKind::LoadHandle, dst: Some(v(3)), args: vec![v(2)] },
        ]
    };
    let mut borrowed = func([load(v(0)), vec![call.clone()]].concat());
    borrowed.params = vec![MirParam { value: v(0), repr: heap() }];
    assert!(cert_all_balanced(&ownership_certificate(&borrowed)));
    assert_eq!(verify_ownership(&borrowed), Ok(()));
    let alloc = Op::Alloc { dst: v(0), repr: heap(), init: Init::Opaque };
    let owned = func([vec![alloc.clone()], load(v(0)), vec![call.clone(), Op::Drop { v: v(0) }]].concat());
    assert!(cert_all_balanced(&ownership_certificate(&owned)));
    assert_eq!(verify_ownership(&owned), Ok(()));
    let freed = func([vec![alloc], load(v(0)), vec![Op::Drop { v: v(0) }, call]].concat());
    assert!(!cert_all_balanced(&ownership_certificate(&freed)));
    assert!(verify_ownership(&freed).is_err());
}

/// #3267: a handle the then arm defines, read in the else arm. The guard err
/// arm of a `mut`-param effect fn wrote back the then arm's copy-on-write
/// clone, a value the err path never computes. The certificate's handle map
/// was not scoped to the path, so the else arm's `Dup` counted on the param
/// (`adad`, accepted). The maps now restart at each arm: the read lands on the
/// out-of-path handle's own line at count 0 (`bad`), and both sides reject.
#[test]
fn a_handle_from_the_other_arm_is_not_defined() {
    let v = ValueId;
    let (p, c) = (v(0), v(9));
    let f = |else_src: ValueId| {
        let mut f = func(vec![
            Op::ConstInt { dst: c, value: 1 },
            Op::IfThen { cond: c, dst: None },
            Op::Dup { dst: v(1), src: p },
            Op::Drop { v: v(1) },
            Op::Else { val: None },
            Op::Dup { dst: v(2), src: else_src },
            Op::Drop { v: v(2) },
            Op::EndIf { val: None },
        ]);
        f.params = vec![MirParam { value: p, repr: heap() }];
        f
    };
    let cross = f(v(1));
    assert_eq!(
        ownership_certificate(&cross),
        include_str!("../../../proofs/poisoned-certs/3267-cross-arm-handle.cert")
    );
    assert!(!cert_all_balanced(&ownership_certificate(&cross)));
    assert!(verify_ownership(&cross).is_err());
    // The else arm reading the param itself is fine on both sides.
    let own = f(p);
    assert!(cert_all_balanced(&ownership_certificate(&own)));
    assert_eq!(verify_ownership(&own), Ok(()));
}

/// #3269: a child loaded from a slot's block, read after the slot is rebound
/// (`xs = list.set(xs, 2, v)` under its bounds check: the new block, the old
/// block's `Drop`, the `SetLocal`). The slot's line carries the new block's
/// `i`, so it never reaches 0 and the child's probe landed on a positive line
/// although the block the child lives in was freed (accepted). A rebind ends
/// the old block's children: the read lands on the child's own line at 0.
#[test]
fn a_child_of_a_rebound_slot_dies_with_the_old_block() {
    let v = ValueId;
    let (xs, child, new, kept) = (v(0), v(1), v(2), v(3));
    let read = Op::Call { dst: None, func: RtFn::PrintStr, args: vec![CallArg::Handle(child)], result: None };
    let f = |keep: bool, in_branch: bool| {
        let mut ops = vec![
            Op::Alloc { dst: xs, repr: heap(), init: Init::Opaque },
            Op::Prim { kind: PrimKind::LoadHandle, dst: Some(child), args: vec![xs] },
        ];
        if keep {
            ops.push(Op::Dup { dst: kept, src: child });
        }
        let rebind = vec![
            Op::Alloc { dst: new, repr: heap(), init: Init::Opaque },
            Op::Drop { v: xs },
            Op::SetLocal { local: xs, src: new },
        ];
        if in_branch {
            ops.push(Op::ConstInt { dst: v(9), value: 1 });
            ops.push(Op::IfThen { cond: v(9), dst: None });
            ops.extend(rebind);
            ops.push(Op::Else { val: None });
            ops.push(Op::EndIf { val: None });
        } else {
            ops.extend(rebind);
        }
        ops.push(read.clone());
        if keep {
            ops.push(Op::Drop { v: kept });
        }
        ops.push(Op::Drop { v: xs });
        func(ops)
    };
    for in_branch in [false, true] {
        let gone = f(false, in_branch);
        let cert = ownership_certificate(&gone);
        assert!(!cert_all_balanced(&cert), "in_branch {in_branch}: {cert:?}");
        assert!(verify_ownership(&gone).is_err(), "in_branch {in_branch}");
        // A `Dup` of the child keeps it across the rebind on both sides.
        let held = f(true, in_branch);
        assert!(cert_all_balanced(&ownership_certificate(&held)), "in_branch {in_branch}: {:?}", ownership_certificate(&held));
        assert_eq!(verify_ownership(&held), Ok(()), "in_branch {in_branch}");
    }
    assert_eq!(
        ownership_certificate(&f(false, false)),
        include_str!("../../../proofs/poisoned-certs/3269-child-after-slot-rebind.cert")
    );
}

/// #3279: a module-global slot root. `LoadHandle` of the slot's constant
/// address yields a handle the slot holds; the lowering `Dup`s it to read the
/// global, or `MakeUnique`s it and stores it back. The certificate counts it
/// as a line with no `i` (a root kept alive outside the frame), and
/// `verify_ownership` as a borrowed root, while the slot keeps it.
#[test]
fn a_global_slot_load_is_a_borrowed_root() {
    let v = ValueId;
    let slot = |dst: u32| Op::ConstInt { dst: v(dst), value: 8192 };
    let load = Op::Prim { kind: PrimKind::LoadHandle, dst: Some(v(1)), args: vec![v(0)] };
    let read = func(vec![slot(0), load.clone(), Op::Dup { dst: v(2), src: v(1) }, Op::Drop { v: v(2) }]);
    assert!(cert_all_balanced(&ownership_certificate(&read)));
    assert_eq!(verify_ownership(&read), Ok(()));
    // The in-place write: unique the slot's block, store it back, mutate it.
    let call = |h: u32| Op::Call { dst: None, func: RtFn::PrintStr, args: vec![CallArg::Handle(v(h))], result: None };
    let write = func(vec![
        slot(0),
        load.clone(),
        Op::MakeUnique { v: v(1) },
        slot(2),
        Op::Prim { kind: PrimKind::Handle, dst: Some(v(3)), args: vec![v(1)] },
        Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![v(2), v(3)] },
        call(1),
    ]);
    assert!(cert_all_balanced(&ownership_certificate(&write)));
    assert_eq!(verify_ownership(&write), Ok(()));
    // The slot no longer keeps the handle after a call (which may reassign
    // the global) or after another value is stored in it: a read then is
    // rejected (stricter than the certificate, which has no slot model).
    let after_call = func(vec![slot(0), load.clone(), call(9), Op::Dup { dst: v(2), src: v(1) }, Op::Drop { v: v(2) }]);
    assert!(verify_ownership(&after_call).is_err());
    let after_store = func(vec![
        slot(0),
        load,
        slot(2),
        Op::ConstInt { dst: v(3), value: 0 },
        Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![v(2), v(3)] },
        Op::Dup { dst: v(4), src: v(1) },
        Op::Drop { v: v(4) },
    ]);
    assert!(verify_ownership(&after_store).is_err());
}

/// #3279: a heap branch result stored into a container slot without a
/// `Consume` (a list element built from an `if`). The arms move their values
/// into the merge, and the merge's value moves on into the list, which
/// releases it: the certificate opens no line for the merge dst, and
/// `verify_ownership` no longer owns it (it reported a leak). A merge that is
/// released afterwards still owns its reference on both sides.
#[test]
fn a_merge_stored_into_a_container_holds_no_reference() {
    let v = ValueId;
    let arms = |merge: u32| {
        vec![
            Op::ConstInt { dst: v(9), value: 1 },
            Op::IfThen { cond: v(9), dst: Some(v(merge)) },
            Op::Alloc { dst: v(2), repr: heap(), init: Init::Opaque },
            Op::Consume { v: v(2) },
            Op::Else { val: Some(v(2)) },
            Op::Alloc { dst: v(3), repr: heap(), init: Init::Opaque },
            Op::Consume { v: v(3) },
            Op::EndIf { val: Some(v(3)) },
        ]
    };
    let list = Op::Alloc { dst: v(0), repr: heap(), init: Init::Opaque };
    let mut stored = vec![list.clone()];
    stored.extend(arms(4));
    stored.extend([
        Op::Prim { kind: PrimKind::Handle, dst: Some(v(5)), args: vec![v(4)] },
        Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![v(0), v(5)] },
        Op::DropListStr { v: v(0) },
    ]);
    let stored = func(stored);
    assert!(cert_all_balanced(&ownership_certificate(&stored)));
    assert_eq!(verify_ownership(&stored), Ok(()));
    let mut released = vec![list];
    released.extend(arms(4));
    released.extend([Op::Drop { v: v(4) }, Op::Drop { v: v(0) }]);
    let released = func(released);
    assert!(cert_all_balanced(&ownership_certificate(&released)));
    assert_eq!(verify_ownership(&released), Ok(()));
}

/// #3279: an arm rebinds a `var` slot onto a `Dup` of a payload loaded before
/// the branch (the C-132 write-back), the other arm keeps the slot's old
/// object. Both paths leave the slot holding one reference; per object the
/// arms disagree, and neither object is arm-fresh. The certificate's slot line
/// balances; `verify_ownership` moves the slot's reference onto a slot object
/// and agrees. Rebinding without releasing the old object is rejected by both.
#[test]
fn a_slot_rebound_onto_a_pre_branch_payload_balances() {
    let v = ValueId;
    let case = |release_old: bool| {
        let mut ops = vec![
            Op::Alloc { dst: v(0), repr: heap(), init: Init::Opaque },
            Op::Prim { kind: PrimKind::LoadHandle, dst: Some(v(1)), args: vec![v(0)] },
            Op::Alloc { dst: v(2), repr: heap(), init: Init::Opaque },
            Op::ConstInt { dst: v(9), value: 1 },
            Op::IfThen { cond: v(9), dst: None },
            Op::Else { val: None },
            Op::Dup { dst: v(3), src: v(1) },
        ];
        if release_old {
            ops.push(Op::DropListStr { v: v(2) });
        }
        ops.extend([
            Op::SetLocal { local: v(2), src: v(3) },
            Op::EndIf { val: None },
            Op::Drop { v: v(2) },
            Op::Drop { v: v(0) },
        ]);
        func(ops)
    };
    let ok = case(true);
    assert!(cert_all_balanced(&ownership_certificate(&ok)), "{}", ownership_certificate(&ok));
    assert_eq!(verify_ownership(&ok), Ok(()));
    let leak = case(false);
    assert!(!cert_all_balanced(&ownership_certificate(&leak)));
    assert!(verify_ownership(&leak).is_err());
}
