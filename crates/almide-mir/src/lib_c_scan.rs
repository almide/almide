// ── the ownership scan's rc events and liveness queries, and verify_ownership's
// phases, of lib_c.rs, include!-spliced back at module level (a code move under
// the 800-line ceiling the codopsy gate holds this crate to) ──

impl OwnershipScan {
    /// Extracted from [`Self::step`] (codopsy r2, #852): the `Prim` arm — the three
    /// NAMEABLE Value-rc events (`handle` carries its source object into a second
    /// handle, `rc_inc`/`rc_dec` move that object's count); every other prim kind is
    /// ownership-neutral. Verbatim.
    fn apply_prim_rc_event(&mut self, kind: &PrimKind, dst: &Option<ValueId>, args: &[ValueId]) {
        // A native Result carrier birth is alloc-class: a fresh owned object,
        // released by the carrier's kept `Drop`/`Consume` (#4349).
        if let (true, Some(d)) = (kind.births_native_carrier(), dst) {
            self.own_fresh_object(*d);
            return;
        }
        // The region window's arena mark (#4006 C2): a fresh owned object,
        // released once by the window's `RegionRestore`.
        if let (PrimKind::RegionSave, Some(d)) = (kind, dst) {
            self.own_fresh_object(*d);
            return;
        }
        match kind {
            // The carrier is a LIVE alias of its source's object (#3263): a
            // borrowing use of it (a call's handle arg — `string.eq(prim.handle
            // (child))`) live-checks that OBJECT, as the certificate's line does.
            // It acquires nothing, so a release through it still needs a held
            // reference.
            PrimKind::Handle => {
                self.record_carrier(*dst, args);
                if let (Some(d), Some(&o)) =
                    (dst.as_ref(), args.first().and_then(|a| self.object_of.get(a)))
                {
                    self.object_of.insert(*d, o);
                    self.dead.insert(*d, false);
                    self.carriers.insert(*d);
                }
            }
            // T1-3 native Result carrier: the borrowed Err-String read ALIASES
            // the Result's object (exactly the `Handle` rule) so a downstream
            // borrowing use (a `CallArg::Handle` into println) live-checks
            // against the Result value the caller still owns.
            PrimKind::ResErrStr => {
                if let (Some(d), Some(&o)) =
                    (dst.as_ref(), args.first().and_then(|a| self.object_of.get(a)))
                {
                    self.object_of.insert(*d, o);
                    self.dead.insert(*d, false);
                }
            }
            // A `LoadHandle` through a TRACKED address (the `IntBinOp Add` alias
            // above): the loaded CHILD handle — an Option/Result payload, a
            // record field — is its OWN object, live while the object it was
            // loaded from is (its slot holds the child) or while the frame
            // holds a `Dup` of it (#3261, [`Self::object_alive`]). A `Dup`
            // acquires a reference counted on the CHILD, the matching `Drop`
            // releases it. (#1037 counted it on the parent, which kept a raw
            // child "live" through a `Dup` of a grandchild after the parent
            // was freed.) An address with NO tracked root stays off the model
            // (the pre-existing load64 floor) — unknown, never guessed.
            // The address may come from `ElemAddr` (a list element — every
            // `__list_dec_go` / `__list_enc_go` over a borrowed list param), which
            // only `addr_of` records: the certificate's `load_child` reads the
            // same map (#3263).
            PrimKind::LoadHandle => {
                if let (Some(d), Some(o)) =
                    (dst.as_ref(), args.first().and_then(|a| self.address_object(*a)))
                {
                    self.object_of.insert(*d, *d);
                    self.child_parent.insert(*d, o);
                    self.dead.insert(*d, false);
                } else if let (Some(d), Some(a)) = (dst, args.first()) {
                    self.load_slot_root(*d, *a);
                }
            }
            PrimKind::RcInc => {
                if let Some(&o) = args.first().and_then(|a| self.object_of.get(a)) {
                    *self.rc.entry(o).or_insert(0) += 1;
                }
            }
            PrimKind::RcDec => {
                if let Some(&o) = args.first().and_then(|a| self.object_of.get(a)) {
                    if self.rc.get(&o).copied().unwrap_or(0) >= 1 {
                        *self.rc.entry(o).or_insert(0) -= 1;
                    }
                }
            }
            _ => {}
        }
    }

    /// A load or store DEREFERENCES the object its address points into, so that
    /// object must still be live (#3259, the certificate's `b` probe of #3233).
    /// `ElemAddr` reads its list's bounds, and its result is an address into the
    /// list. `prim.handle` is not a dereference: it only turns the pointer into
    /// an integer (the lowering's move into a container reads it after the
    /// `Consume` that transferred the reference).
    fn check_dereference(&mut self, i: usize, kind: &PrimKind, dst: Option<ValueId>, args: &[ValueId]) {
        let Some(&addr) = args.first() else { return };
        match kind {
            PrimKind::LoadHandle | PrimKind::Load { .. } | PrimKind::Store { .. } => {
                self.check_address_live(i, addr)
            }
            // A native Result carrier read dereferences the carrier (#4349).
            k if k.reads_native_carrier() => self.check_address_live(i, addr),
            PrimKind::ElemAddr => {
                self.check_address_live(i, addr);
                if let (Some(d), Some(o)) = (dst, self.address_object(addr)) {
                    self.addr_of.insert(d, o);
                }
            }
            _ => {}
        }
    }

    /// The object `v` denotes, iff the handle is live ([`live_object`], with
    /// the loaded-child rule of [`Self::object_alive`]).
    fn live(&self, v: ValueId) -> Option<ValueId> {
        if self.dead.get(&v).copied().unwrap_or(true) {
            return None;
        }
        let o = *self.object_of.get(&v)?;
        self.object_alive(o).then_some(o)
    }

    /// Object `o` is live: a borrowed param, an object we hold a reference
    /// to, or a loaded child whose parent is live (#3261).
    fn object_alive(&self, o: ValueId) -> bool {
        let mut o = o;
        loop {
            if self.borrowed.contains(&o) || self.rc.get(&o).copied().unwrap_or(0) >= 1 {
                return true;
            }
            if self.rebound.contains(&o) {
                return false;
            }
            match self.child_parent.get(&o) {
                Some(&p) => o = p,
                None => return false,
            }
        }
    }

    /// A call's handle argument is checked per OBJECT, the certificate's
    /// probe (#3263): a handle released while a sibling still holds its object
    /// passes the same live pointer. An untracked argument stays a violation.
    fn call_arg_live(&self, v: ValueId) -> bool {
        !self.ended_view(v) && self.address_object(v).is_some_and(|o| self.object_alive(o))
    }

    /// The object an address (or a handle used as one) points into.
    fn address_object(&self, v: ValueId) -> Option<ValueId> {
        self.addr_of.get(&v).or_else(|| self.object_of.get(&v)).copied()
    }

    /// The object behind `addr` is live: a borrowed param (the caller holds
    /// it), or an object we still hold a reference to. The check is per OBJECT,
    /// like the certificate's line: an address outlives the handle it came from.
    /// An address with no tracked object stays off the model, as before.
    fn check_address_live(&mut self, i: usize, addr: ValueId) {
        let Some(o) = self.address_object(addr) else { return };
        if self.ended_view(addr) || !self.object_alive(o) {
            self.violations.push(violation(i, addr, ViolationKind::UseAfterFree));
        }
    }

    /// Extracted from [`Self::step`] (codopsy r2, #852): the `SetLocal` arm — a
    /// loop-carried rebind aliases the slot onto the source's object and makes it
    /// live again. Verbatim.
    fn rebind_local_slot(&mut self, local: ValueId, src: ValueId) {
        self.end_rebound_children(local);
        if let Some(o) = self.object_of.get(&src).copied() {
            self.object_of.insert(local, o);
            self.dead.insert(local, false);
        }
    }
}

// Heap params are BORROWED by default (the v1 calling convention): the CALLER owns
// the reference and releases it at its own scope end; the callee gets a LIVE
// handle but holds NO owned reference of its own (its rc starts at 0). This is the
// exact dual of the certificate omitting the param's `i` event — an owned-param
// `+1` would be SYNTHETIC (no `Alloc`/`rc_inc` backs it), the gate-blind
// use-after-free class. A body that wants to consume or return a param must first
// `Dup` it (acquire its own ref); a release with rc 0 (the `borrowed` object, never
// `Dup`'d) fails — exactly the cert's `d`/`m` at rc 0, which the proven checker
// faults. Split out of `verify_ownership` (codopsy cc) as phase 1 of a sequential
// setup → scan → return-check → leak-check pipeline — each phase touches disjoint
// state (this one only populates, never reads a later phase's writes), the same
// fold-independent-writes shape used elsewhere in this crate.
fn init_borrowed_params(
    func: &MirFunction,
    object_of: &mut BTreeMap<ValueId, ValueId>,
    dead: &mut BTreeMap<ValueId, bool>,
    borrowed: &mut BTreeSet<ValueId>,
) {
    for p in &func.params {
        if p.repr.is_heap() {
            object_of.insert(p.value, p.value);
            dead.insert(p.value, false);
            borrowed.insert(p.value);
        }
    }
}

// A heap return value is MOVED OUT to the caller. It must be a reference WE own
// (an `Alloc`/call-result, or a `Dup` we acquired): releasing it transfers our
// reference out. Returning a BORROWED param we never acquired (rc 0) would give
// the caller a SECOND owner of the caller's own reference — a double-free.
// `release` fails there (rc 0) and we record it, the dual of the cert's `m` at rc
// 0 which the proven checker faults. Phase 3 of `verify_ownership`'s pipeline.
fn check_return_release(
    func: &MirFunction,
    object_of: &BTreeMap<ValueId, ValueId>,
    rc: &mut BTreeMap<ValueId, i64>,
    dead: &mut BTreeMap<ValueId, bool>,
    borrowed: &BTreeSet<ValueId>,
    violations: &mut Vec<Violation>,
) {
    if let Some(r) = func.ret {
        if object_of.contains_key(&r) && release(object_of, rc, dead, borrowed, r).is_err() {
            violations.push(violation(func.ops.len(), r, ViolationKind::UseAfterMove));
        }
    }
}

// Leak check: every object's references must have left (dropped or moved). Phase
// 4 (final) of `verify_ownership`'s pipeline.
fn check_leaks(func: &MirFunction, rc: &BTreeMap<ValueId, i64>, violations: &mut Vec<Violation>) {
    for (o, c) in rc {
        if *c > 0 {
            violations.push(violation(func.ops.len(), *o, ViolationKind::Leak));
        }
    }
}

include!("verify_ownership_scan.rs");

/// One arm's exit state at a branch join: per-object counts, per-handle
/// deadness, and which object each handle denotes.
type ArmExit = (BTreeMap<ValueId, i64>, BTreeMap<ValueId, bool>, BTreeMap<ValueId, ValueId>);

/// Rename object `from` to `to` in one arm's state: every handle that denotes
/// `from` now denotes `to`, and `from`'s count is added to `to`'s.
fn rename_object(
    object_of: &mut BTreeMap<ValueId, ValueId>,
    rc: &mut BTreeMap<ValueId, i64>,
    from: ValueId,
    to: ValueId,
) {
    for o in object_of.values_mut() {
        if *o == from {
            *o = to;
        }
    }
    if let Some(c) = rc.remove(&from) {
        *rc.entry(to).or_insert(0) += c;
    }
}

fn violation(op_index: usize, value: ValueId, kind: ViolationKind) -> Violation {
    Violation { op_index, value, kind }
}

/// The object a handle denotes, iff the handle is live. A handle is live when it
/// is not yet dropped AND either WE hold a reference to its object (rc ≥ 1) OR
/// the object is a `borrowed` param the CALLER keeps alive for the call's
/// duration (a borrow is always valid against the caller's reference, even when
/// our own count is 0). `None` = dead/unknown handle, or a non-borrowed object
/// whose references have all left.
fn live_object(
    object_of: &BTreeMap<ValueId, ValueId>,
    rc: &BTreeMap<ValueId, i64>,
    dead: &BTreeMap<ValueId, bool>,
    borrowed: &BTreeSet<ValueId>,
    v: ValueId,
) -> Option<ValueId> {
    if dead.get(&v).copied().unwrap_or(true) {
        return None; // unknown handle or already dropped/consumed
    }
    let o = *object_of.get(&v)?;
    if borrowed.contains(&o) || rc.get(&o).copied().unwrap_or(0) >= 1 {
        Some(o)
    } else {
        None
    }
}

/// Release one reference held by handle `v` (drop or consume): mark the handle
/// dead and decrement OUR object's refcount. `Err(())` if `v` is not live, OR if
/// we hold no reference of our own to release (rc 0 — e.g. a `borrowed` param we
/// never `Dup`'d): freeing a reference we do not own is a double-free against the
/// caller, so it is rejected rather than silently underflowed.
fn release(
    object_of: &BTreeMap<ValueId, ValueId>,
    rc: &mut BTreeMap<ValueId, i64>,
    dead: &mut BTreeMap<ValueId, bool>,
    borrowed: &BTreeSet<ValueId>,
    v: ValueId,
) -> Result<(), ()> {
    match live_object(object_of, rc, dead, borrowed, v) {
        Some(o) if rc.get(&o).copied().unwrap_or(0) >= 1 => {
            *rc.get_mut(&o).expect("a held reference has a refcount") -= 1;
            dead.insert(v, true);
            Ok(())
        }
        _ => Err(()),
    }
}
