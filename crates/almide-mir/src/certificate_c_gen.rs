// ── faithfulness over the read shapes (#3259) ──
// `gen_wellformed` draws no prims and no calls, so it never compared the
// certificate and `verify_ownership` on a dereference or a call handle
// argument — the shapes #3233 added to the certificate. This generator draws
// them: addresses built from a handle (`prim.handle` + offset, `ElemAddr`),
// `LoadHandle` / `Load` / `Store` through an address made EARLIER (so the
// object may have been freed in between), direct loads through a handle, and
// `Call` handle args — over owned objects and a borrowed heap param.

/// The generator state: the op list, the live owned handles, which object each
/// handle denotes, and the address pool.
struct ReadGen {
    st: u64,
    next: u32,
    ops: Vec<Op>,
    live: Vec<ValueId>,
    obj: std::collections::BTreeMap<ValueId, ValueId>,
    param: Option<ValueId>,
    addrs: Vec<ValueId>,
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
    fn call_arg_pool(&self) -> Vec<ValueId> {
        self.handles()
            .into_iter()
            .filter(|h| self.live.contains(h) || Some(*h) == self.param || !self.object_live(self.obj[h]))
            .collect()
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
        let Some(src) = self.pick(&pool) else { return };
        let v = self.fresh();
        self.ops.push(Op::Dup { dst: v, src });
        let o = self.obj[&src];
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
        let Some(src) = self.pick(&self.handles()) else { return };
        let (h, k, a) = (self.fresh(), self.fresh(), self.fresh());
        self.ops.push(Op::ConstInt { dst: k, value: 8 });
        if next_rand(&mut self.st) % 3 == 0 {
            self.ops.push(Op::Prim { kind: PrimKind::ElemAddr, dst: Some(a), args: vec![src, k] });
        } else {
            self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![src] });
            self.ops.push(Op::IntBinOp { dst: a, op: crate::IntOp::Add, a: h, b: k });
        }
        self.addrs.push(a);
    }

    /// A load or store through an address from the pool, or directly through a
    /// handle. The loaded child handle is not used again: the two sides model
    /// it differently (see `loaded_child_read_after_parent_free_diverges`).
    fn deref(&mut self) {
        let mut pool = self.addrs.clone();
        pool.extend(self.handles());
        let Some(a) = self.pick(&pool) else { return };
        let d = self.fresh();
        match next_rand(&mut self.st) % 3 {
            0 => self.ops.push(Op::Prim { kind: PrimKind::LoadHandle, dst: Some(d), args: vec![a] }),
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

    fn step(&mut self) {
        match next_rand(&mut self.st) % 6 {
            0 => self.alloc(),
            1 => self.dup(),
            2 => self.drop_one(),
            3 => self.address(),
            4 => self.deref(),
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

/// A known difference the generator steers around, pinned (#3259 finding):
/// a `LoadHandle` CHILD (a payload loaded through an address into its
/// parent) passed to a call after the parent's last release. The certificate
/// gives the child no line and so does not probe the call arg (`ibd`
/// accepts); `verify_ownership` aliases the child to its parent and rejects
/// it. Neither side can simply copy the other: a child the frame `Dup`'d
/// stays live on its own reference after the parent goes, which the
/// verifier counts on the parent and a per-object certificate line cannot
/// express as one probe. `verify_ownership` is the stricter side here.
#[test]
fn loaded_child_read_after_parent_free_diverges() {
    let (o, h, off, addr, c) = (ValueId(0), ValueId(1), ValueId(2), ValueId(3), ValueId(4));
    let f = func(vec![
        Op::Alloc { dst: o, repr: heap(), init: Init::Opaque },
        Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![o] },
        Op::ConstInt { dst: off, value: 12 },
        Op::IntBinOp { dst: addr, op: crate::IntOp::Add, a: h, b: off },
        Op::Prim { kind: PrimKind::LoadHandle, dst: Some(c), args: vec![addr] },
        Op::Drop { v: o },
        Op::Call { dst: None, func: RtFn::PrintStr, args: vec![CallArg::Handle(c)], result: None },
    ]);
    assert_eq!(ownership_certificate(&f), "ibd\n");
    assert!(cert_all_balanced(&ownership_certificate(&f)));
    assert!(verify_ownership(&f).is_err());
}
