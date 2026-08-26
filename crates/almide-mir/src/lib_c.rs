
/// A function parameter: a value the caller supplies, with its [`Repr`]. A heap
/// param is BORROWED (the v1 calling convention): the CALLER retains ownership
/// and releases it; the callee gets a live handle but no owned reference. So a
/// param contributes NO `+1` to the ownership certificate — an owned-param `+1`
/// would be synthetic (no runtime `Alloc`/`rc_inc` backs it), the gate-blind
/// use-after-free class. A body that needs to consume or return a param must
/// first `Dup` it (acquire its own reference). A scalar param carries no
/// ownership. (Per-param move-mode signatures are a later refinement.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MirParam {
    pub value: ValueId,
    pub repr: Repr,
}

/// A MIR function: params, a flat ownership-explicit op sequence, and an
/// optional returned value (moved out — a [`Op::Consume`] of `ret` is implied at
/// the boundary).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct MirFunction {
    pub name: String,
    pub params: Vec<MirParam>,
    pub ops: Vec<Op>,
    pub ret: Option<ValueId>,
    /// The host [`Capability`]s this function is PERMITTED to reach (its effect
    /// signature, lowered). The capability witness checks the capabilities the
    /// body actually uses against this declared bound — accept ⟹ no undeclared
    /// host effect (proofs/CapabilityBound.v). Empty = a pure/sandboxed function.
    pub declared_caps: Vec<Capability>,
    /// RENDER-ONLY side table: a value → the i64-SLOT INDICES that hold an OWNED heap
    /// handle, for a MIXED scalar+heap record/tuple block (e.g. `R { name: String, n: Int }`
    /// = `[0]`). It refines the recursive free of an [`Op::DropListStr`] on such a value:
    /// instead of the uniform "free EVERY slot" loop (correct only for a homogeneous
    /// `List[String]`), the render frees exactly these slots, then the block. A value
    /// ABSENT from this table keeps the uniform-loop behavior (`List[String]` / all-heap
    /// aggregate). This carries NO ownership semantics — the certificate sees a `DropListStr`
    /// as the SAME single `d` regardless (each heap field was already accounted `m`/consumed
    /// at its move-in store), exactly as for `List[String]`. So it is a pure rendering
    /// refinement (like the `DropValue` tag dispatch) — NOT a new op or certificate event.
    pub heap_slot_masks: BTreeMap<ValueId, Vec<usize>>,
}

/// A whole MIR program.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct MirProgram {
    pub functions: Vec<MirFunction>,
    /// `pub fn` export roots (#457 — the fns the v0 emitter also exports). Each entry:
    /// (export_name — the `@export(wasm, "sym")` override or the fn name, internal fn
    /// name, per-param is_float, ret: None = void / Some(is_float)). A Float-bearing
    /// signature renders through a thin `f64.reinterpret_i64` wrapper so the export
    /// presents REAL f64s (the v0 ABI) while the internal fn keeps the i64-bits
    /// convention. Populated by the pipeline from the MAIN program's Public non-test
    /// non-generic functions; empty everywhere else.
    pub exports: Vec<(String, String, Vec<bool>, Option<bool>)>,
    /// The number of MUTABLE module-level `var` storage slots. Slot `i` lives at linear
    /// address [`mg_slot_addr`]`(i)` — the 8-byte region `[MG_SLOT_BASE, MG_SLOT_BASE +
    /// 8*count)` carved between the print line buffer (which ends at `MG_SLOT_BASE`) and
    /// the bump allocator (whose base the renderer shifts to `MG_SLOT_BASE + 8*count`).
    /// A count of 0 renders byte-identically to a program with no mutable globals.
    pub mutable_global_count: u32,
}

/// The base linear-memory address of the mutable-global slot region (== the renderer's
/// `HEAP_BASE`; with no mutable globals the bump allocator starts exactly here).
pub const MG_SLOT_BASE: u32 = 8192;

/// The linear-memory address of mutable-global slot `index` (one uniform 8-byte slot per
/// module-level `var`: a scalar holds its i64 value, a heap global its block handle).
pub const fn mg_slot_addr(index: u32) -> u32 {
    MG_SLOT_BASE + 8 * index
}

// ─────────────────────────── Ownership verifier ───────────────────────────
//
// The executable ownership invariant (#575/#576). A symbolic refcount
// interpretation over the ops: every heap value's owner count must return to 0
// (every reference dropped or moved out), never go negative (double-free), and
// never be used after it reaches 0 / is moved (use-after-free / -move).

/// What an ownership violation is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ViolationKind {
    /// A `drop` of a value whose owner count is already 0.
    DoubleFree,
    /// A `dup`/`borrow`/`make_unique`/`pure`-use of a freed value.
    UseAfterFree,
    /// A `consume` of a value already moved out (count 0).
    UseAfterMove,
    /// A heap value still owned (count > 0) at function end.
    Leak,
    /// The two arms of an `IfThen`/`Else`/`EndIf` branch leave an object at
    /// DIFFERENT owner counts — whichever way the branch goes at runtime, the
    /// later accounting is wrong for the other path (a path-dependent leak or
    /// double-free). Mirrors the proven checker's `CBranch` agreement rule.
    BranchDisagreement,
}

/// A located ownership violation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Violation {
    /// Index into `func.ops`; equals `ops.len()` for an end-of-function leak.
    pub op_index: usize,
    pub value: ValueId,
    pub kind: ViolationKind,
}

/// Verify the ownership invariant for one function.
///
/// Returns `Ok(())` if the MIR is balanced (the by-construction guarantee the
/// renderers rely on), or every violation found (deterministic order). This is
/// the MIR-level analogue of the Perceus belt's IR check, but it is the SINGLE
/// source — there is no second hand-written copy in a renderer to drift from.
/// The mutable scan state of [`verify_ownership`] — one step per op (#781:
/// the cog-140 loop body became [`OwnershipScan::step`]).
struct OwnershipScan {
    object_of: BTreeMap<ValueId, ValueId>,
    rc: BTreeMap<ValueId, i64>,
    dead: BTreeMap<ValueId, bool>,
    borrowed: BTreeSet<ValueId>,
    branches: Vec<BranchFrame>,
    violations: Vec<Violation>,
    /// A pending [`Op::Return`] divergence (law 6): the current arm ended in a
    /// frame-targeted early exit whose whole function-exit obligation was
    /// checked AT the `Return` op. Consumed by the next `Else`/`EndIf` (the
    /// arm's closing marker — mir_wellformed guarantees nothing sits between);
    /// still set at scan end = a TOP-LEVEL return, so the phase-3/4 boundary
    /// checks are skipped (they already ran at the op).
    diverged: bool,
}

    struct BranchFrame {
        entry_rc: BTreeMap<ValueId, i64>,
        entry_dead: BTreeMap<ValueId, bool>,
        then_exit: Option<(BTreeMap<ValueId, i64>, BTreeMap<ValueId, bool>)>,
        /// The `IfThen`'s result slot, and whether any arm MOVED a heap value
        /// into it — the branch-result modeling of #1037's second gap. An arm's
        /// merge value arrives either already `Consume`d (the Alloc+Consume
        /// pattern) or as a LIVE owned value (a nested branch result), which
        /// the merge itself moves ([`OwnershipScan::merge_val_move`] releases
        /// it). Either way the arm nets 0 and the join re-materializes the
        /// moved reference as a fresh owned object on `dst`, which the
        /// scope-end `Drop`/`Consume` then releases. A live-but-UNOWNED value
        /// (a borrowed param) is NOT a move — `dst` stays unmodeled and a
        /// later release of it walls conservatively, as before.
        dst: Option<ValueId>,
        moved_in: bool,
        /// The then arm ended in [`Op::Return`] (its exit state is DIVERGED —
        /// it took its whole exit obligation at the op and takes no part in
        /// the agreement; the join continues from the surviving arm alone).
        then_diverged: bool,
    }

impl OwnershipScan {
    /// One op's ownership transition. Verbatim text move of the scan loop body
    /// (locals renamed to fields).
    ///
    /// ROUTER (codopsy r2, #852 — this was the crate's worst fn at cyclomatic 79 /
    /// cog 92): each arm's BODY moved into the helper below it that names the
    /// decision it makes (open a fresh owned object, live-check a borrowed use,
    /// acquire a reference, release one, borrow a call's args and own its result,
    /// enter/leave a branch frame, apply a Value-rc prim event, rebind a slot). The
    /// `match` itself is untouched and still EXHAUSTIVE — no catch-all arm — so a new
    /// [`Op`] variant remains a COMPILE error here, which is the property that makes
    /// this function the ownership source of truth rather than a best-effort scan. No
    /// arm was reordered, no condition rewritten; the arms that merged into one
    /// (`Const`/`ConstInt`/`FuncRef`/`IntBinOp`/the loop markers) all had the SAME
    /// empty body, and each kept its own comment above its own pattern.
    /// The `Add`-with-one-handle-operand address-alias rule (#1037) — split
    /// from [`Self::step`] for the complexity budget: the result denotes an
    /// address INTO the handle operand's object, so it inherits that object.
    fn step_add_address_alias(&mut self, dst: ValueId, a: ValueId, b: ValueId) {
        match (self.object_of.get(&a).copied(), self.object_of.get(&b).copied()) {
            (Some(o), None) | (None, Some(o)) => {
                self.object_of.insert(dst, o);
            }
            _ => {}
        }
    }

    fn step(&mut self, i: usize, op: &Op) {
        match op {
            // Probe charge: no ownership event (no alloc, no dup, no drop).
            // The dyn charge READS its src (a borrow-class use, like a Prim
            // handle arg) and changes no refcount.
            Op::Charge { .. } => {}
            Op::ChargeDyn { src, .. } => self.check_borrowed_use(i, *src),
            Op::Alloc { dst, repr, .. } => {
                debug_assert!(repr.is_heap(), "Alloc of a non-heap repr is malformed MIR");
                self.own_fresh_object(*dst);
            }
            // A rung-4 scalar-list LITERAL is alloc-class: one fresh owned object
            // (the identical accounting the replaced `Alloc{DynList}` had). Its
            // element values are raw i64 slot scalars — no ownership to check.
            Op::ListLit { dst, .. } => self.own_fresh_object(*dst),
            // The rung-4 element load/store BORROW the list handle (live-check,
            // no refcount change — exactly the `Borrow`/`MakeUnique` discipline);
            // the scalar element/index/value carry no ownership.
            Op::ListGetScalar { list, .. } | Op::ListSetScalar { list, .. } => {
                self.check_borrowed_use(i, *list)
            }
            // Scalar arithmetic carries no ownership — EXCEPT the ADDRESS form
            // the payload-borrow idiom lowers to (`prim.handle(x) + offset`,
            // then a `LoadHandle` at that address — `load_at_offset`): an `Add`
            // with exactly one handle-derived operand denotes an address INTO
            // that operand's object, and the alias must survive to the
            // `LoadHandle` so the loaded child handle can be accounted rather
            // than fall off the model (#1037 — every `option.unwrap_or` tuple/
            // heap payload walled the native verifier on exactly this chain).
            // The `PrimKind::Handle` rule, extended one hop; no `dead` entry is
            // created — an address is never itself live-checked, only traversed.
            Op::IntBinOp { dst, op: crate::IntOp::Add, a, b } => {
                self.step_add_address_alias(*dst, *a, *b)
            }
            // A scalar — no ownership accounting.
            Op::Const { dst: _ }
            | Op::ConstInt { .. }
            // A function-table slot index — a scalar constant, no ownership.
            | Op::FuncRef { .. }
            // Scalar arithmetic — no ownership.
            // A scalar arithmetic op and a primitive-floor op carry no ownership: a
            // scalar result is Copy and a `Prim` handle arg is BORROWED (read only).
            | Op::IntBinOp { .. }
            // Loop markers carry no ownership; the body ops between them are
            // per-iteration-balanced (verified flat, one iteration).
            | Op::LoopStart
            | Op::LoopBreakUnless { .. }
            | Op::LoopEnd => {}
            Op::Dup { dst, src } => self.acquire_reference(i, *dst, *src),
            // A `DropListStr`/`DropListValue` releases the LIST object exactly like a `Drop` (the
            // recursive element free is a RENDER concern, gated on self.rc==1; the cert sees one −1 on the
            // list — its elements were `Consume`d into it when stored).
            Op::Drop { v }
            | Op::DropListStr { v }
            | Op::DropValue { v }
            | Op::DropListValue { v }
            | Op::DropListStrValue { v }
            | Op::DropListStrStr { v }
            | Op::DropListIntStr { v }
            | Op::DropListStrInt { v }
            | Op::DropResultListValue { v }
            | Op::DropResultValue { v }
            | Op::DropResultStrInt { v }
            | Op::DropResultValueInt { v }
            | Op::DropResultListValueInt { v }
            | Op::DropResultListStrInt { v }
            | Op::DropResultListStr { v }
            | Op::DropListListStr { v }
            | Op::DropVariant { v, .. }
            | Op::DropWrapperRec { v, .. } => {
                self.release_or_report(i, *v, ViolationKind::DoubleFree)
            }
            Op::Consume { v } => self.release_or_report(i, *v, ViolationKind::UseAfterMove),
            Op::Borrow { v } | Op::MakeUnique { v } => self.check_borrowed_use(i, *v),
            Op::Pure { dst: _, uses } => self.check_pure_uses(i, uses),
            // A runtime/user call BORROWS its heap-handle args (live-checked, no
            // refcount change). Immediate/label args carry no ownership. A call
            // whose `result` is a heap repr returns a FRESH OWNED value (the
            // callee allocated and moved it out — the return-mode signature): the
            // `dst` becomes a new owned object, like Alloc.
            Op::Call { args, dst, result, .. }
            | Op::CallFn { args, dst, result, .. }
            // A CallImport (a host wasm import) has the SAME ownership shape: heap-handle
            // args are BORROWED, a heap result is a FRESH OWNED value (the host returns a
            // pointer the caller now owns). Its scalar args carry no ownership.
            | Op::CallImport { args, dst, result, .. }
            // A CallIndirect has the same ownership shape as a CallFn: its heap-arg handles
            // must be live, a heap result is a FRESH OWNED value. The `table_idx` is a
            // scalar closure value (no ownership).
            | Op::CallIndirect { args, dst, result, .. } => {
                self.borrow_call_args_and_own_result(i, args, dst, result)
            }
            // The if-markers carry no ownership of their own, but they scope the
            // BRANCH JOIN: both arms run from the entry state and must agree.
            Op::IfThen { dst, .. } => self.enter_branch_frame(*dst),
            Op::Else { val } => self.switch_to_else_arm(*val),
            Op::EndIf { val } => self.join_branch_arms(i, *val),
            // VALUE-RC modeling (柱C extension) — bring the Value refcount ops out of the prim blind
            // spot for the NAMEABLE case: prim.handle(v) carries its source object in args[0], so the
            // self.rc events on it verify against the same self.rc machine. load64-fed handles have no carrier
            // and stay unmodeled (the differential-test floor). MIRRORED in ownership_certificate.
            Op::Prim { kind, dst, args } => self.apply_prim_rc_event(kind, dst, args),
            // `SetLocal` into a HEAP slot is a loop-carried REBIND (`acc = acc + [x]`):
            // the slot now aliases the source's object. The slot's OLD object was
            // released by a preceding `Drop` in the loop body, so rebinding makes the
            // slot LIVE again (= the new object), preserving the per-iteration invariant
            // (slot owns exactly one ref at the body's start and end) — exactly the
            // soundness condition proved in OwnershipChecker.v's `check_line_unroll_sound`
            // (a self.rc-preserving loop body is leak/double-free-free for any iteration
            // count). For a SCALAR src (the scalar-TCO loop var) `self.object_of` has no
            // entry, so this is a no-op, as before.
            Op::SetLocal { local, src } => self.rebind_local_slot(*local, *src),
            // Frame-targeted early exit (law 6): the arm ends HERE, so this op
            // index carries the whole function-exit obligation the tail carries
            // at ops.len() — then the arm is marked DIVERGED for its closing
            // marker to consume.
            Op::Return { val } => self.diverge_return(i, *val),
        }
    }

    /// The `Return` arm: the boundary release of the returned value (the
    /// [`check_return_release`] rule, applied at THIS index — returning a
    /// borrowed param we never acquired is the same double-owner fault) and the
    /// leak obligation over everything else (the [`check_leaks`] rule, at THIS
    /// index — the lowering must have emitted this exit path's drops before the
    /// op). Sets [`Self::diverged`]; the enclosing `Else`/`EndIf` consumes it,
    /// or scan end reads it as a top-level return (boundary phases skip).
    fn diverge_return(&mut self, i: usize, val: Option<ValueId>) {
        if let Some(v) = val {
            if self.object_of.contains_key(&v)
                && release(&self.object_of, &mut self.rc, &mut self.dead, &self.borrowed, v)
                    .is_err()
            {
                self.violations.push(violation(i, v, ViolationKind::UseAfterMove));
            }
        }
        for (o, c) in &self.rc {
            if *c > 0 {
                self.violations.push(violation(i, *o, ViolationKind::Leak));
            }
        }
        self.diverged = true;
    }

    /// Extracted from [`Self::step`] (codopsy r2, #852): a FRESH owned object —
    /// `dst` is its own object representative, WE hold its single reference, and the
    /// handle is live. The shared body of the `Alloc` and `ListLit` arms and of a
    /// heap call `result` (all three are the same alloc-class +1), verbatim.
    fn own_fresh_object(&mut self, dst: ValueId) {
        self.object_of.insert(dst, dst);
        self.rc.insert(dst, 1);
        self.dead.insert(dst, false);
    }

    /// Extracted from [`Self::step`] (codopsy r2, #852): a BORROWING use — the handle
    /// must be live, and nothing about the refcount changes. Records a
    /// [`ViolationKind::UseAfterFree`] when it is not. The shared body of the
    /// `ListGetScalar`/`ListSetScalar` and `Borrow`/`MakeUnique` arms, verbatim.
    fn check_borrowed_use(&mut self, i: usize, v: ValueId) {
        if live_object(&self.object_of, &self.rc, &self.dead, &self.borrowed, v).is_none() {
            self.violations.push(violation(i, v, ViolationKind::UseAfterFree));
        }
    }

    /// Extracted from [`Self::step`] (codopsy r2, #852): the `Dup` arm — `dst` becomes
    /// a second handle on `src`'s object and we acquire one more reference to it.
    /// Verbatim.
    fn acquire_reference(&mut self, i: usize, dst: ValueId, src: ValueId) {
        if let Some(o) = live_object(&self.object_of, &self.rc, &self.dead, &self.borrowed, src) {
            // Acquire OUR own reference. A `Dup` of a self.borrowed param has no
            // prior self.rc entry (we owned none) — start it at 0, then +1.
            *self.rc.entry(o).or_insert(0) += 1;
            self.object_of.insert(dst, o);
            self.dead.insert(dst, false);
        } else {
            self.violations.push(violation(i, src, ViolationKind::UseAfterFree));
        }
    }

    /// Extracted from [`Self::step`] (codopsy r2, #852): release ONE reference held
    /// by handle `v`, recording `on_failure` when we hold none to release. The two
    /// callers differ only in that kind — the whole DROP family reports
    /// [`ViolationKind::DoubleFree`], `Consume` reports
    /// [`ViolationKind::UseAfterMove`] — so the kind is the parameter and the
    /// [`release`] call is verbatim.
    fn release_or_report(&mut self, i: usize, v: ValueId, on_failure: ViolationKind) {
        match release(&self.object_of, &mut self.rc, &mut self.dead, &self.borrowed, v) {
            Ok(()) => {}
            Err(()) => self.violations.push(violation(i, v, on_failure)),
        }
    }

    /// Extracted from [`Self::step`] (codopsy r2, #852): the `Pure` arm — a
    /// computation that BORROWS every one of its uses, so each accountable one must
    /// be live. Verbatim.
    fn check_pure_uses(&mut self, i: usize, uses: &[ValueId]) {
        for v in uses {
            // Only heap handles are accountable; scalar uses are absent
            // from `self.object_of` and correctly skipped.
            if self.object_of.contains_key(v)
                && live_object(&self.object_of, &self.rc, &self.dead, &self.borrowed, *v).is_none()
            {
                self.violations.push(violation(i, *v, ViolationKind::UseAfterFree));
            }
        }
    }

    /// Extracted from [`Self::step`] (codopsy r2, #852): the shared body of the
    /// `Call`/`CallFn`/`CallImport`/`CallIndirect` arms — every heap-handle arg is
    /// live-checked (borrowed, refcount unchanged) and a heap `result` opens a fresh
    /// owned object on `dst`. Verbatim, with the three result-insert lines reading
    /// through [`Self::own_fresh_object`] (the same three inserts, same order).
    fn borrow_call_args_and_own_result(
        &mut self,
        i: usize,
        args: &[CallArg],
        dst: &Option<ValueId>,
        result: &Option<Repr>,
    ) {
        for a in args {
            if let CallArg::Handle(v) = a {
                if live_object(&self.object_of, &self.rc, &self.dead, &self.borrowed, *v).is_none() {
                    self.violations.push(violation(i, *v, ViolationKind::UseAfterFree));
                }
            }
        }
        if let (Some(d), Some(r)) = (dst, result) {
            if r.is_heap() {
                self.own_fresh_object(*d);
            }
        }
    }

    /// Extracted from [`Self::step`] (codopsy r2, #852): the `IfThen` arm — open a
    /// branch frame remembering the ENTRY state both arms run from. Verbatim.
    fn enter_branch_frame(&mut self, dst: Option<ValueId>) {
        self.branches.push(BranchFrame {
            entry_rc: self.rc.clone(),
            entry_dead: self.dead.clone(),
            then_exit: None,
            dst,
            moved_in: false,
            then_diverged: false,
        });
    }

    /// Extracted from [`Self::step`] (codopsy r2, #852): the `Else` arm — park the
    /// then arm's EXIT state in the frame and rewind the scan to the entry state, so
    /// the else arm runs from the same place the then arm did. Verbatim.
    fn switch_to_else_arm(&mut self, val: Option<ValueId>) {
        // The move into the merge happens BEFORE the arm's exit state is
        // snapshotted — the release is part of the then arm's accounting.
        // A DIVERGED then arm (it ended in `Return`) moved nothing into the
        // merge and its exit state is not a join input: park the divergence on
        // the frame instead of a snapshot (`then_exit` stays the "Else was
        // seen" witness either way).
        let diverged = std::mem::take(&mut self.diverged);
        let moved = if diverged { false } else { self.merge_val_move(val) };
        if let Some(fr) = self.branches.last_mut() {
            fr.moved_in |= moved;
            fr.then_diverged = diverged;
            fr.then_exit = Some((self.rc.clone(), self.dead.clone()));
            self.rc = fr.entry_rc.clone();
            self.dead = fr.entry_dead.clone();
        }
    }

    /// Extracted from [`Self::step`] (codopsy r2, #852): the `EndIf` arm — close the
    /// frame, then run the two phases the arm always ran in this order, the
    /// agreement CHECK first (it only reads) and the JOIN second. Verbatim.
    fn join_branch_arms(&mut self, i: usize, val: Option<ValueId>) {
        // A pending divergence belongs to the arm that just ENDED at this
        // `EndIf`: the else arm when an `Else` marker was seen (`then_exit` is
        // its witness), otherwise the then arm (the else arm is empty). A
        // diverged arm moved nothing into the merge and is not a join input.
        let pending = std::mem::take(&mut self.diverged);
        let moved = if pending { false } else { self.merge_val_move(val) };
        if let Some(mut fr) = self.branches.pop() {
            let else_seen = fr.then_exit.is_some();
            let (then_diverged, else_diverged) = if else_seen {
                (fr.then_diverged, pending)
            } else {
                (fr.then_diverged || pending, false)
            };
            fr.moved_in |= moved;
            let dst = fr.dst.filter(|_| fr.moved_in);
            // Law 6 — a diverged arm drops the merge continuation: the join
            // continues from the SURVIVING arm's state alone, and agreement is
            // only between join inputs (none needed with ≤1 survivor). Each
            // diverged arm already took its whole exit obligation at its
            // `Return`. Both arms diverged ⇒ the `if` itself diverges — the
            // divergence propagates to the enclosing arm's closing marker
            // (state parks at the frame entry; every path out was checked).
            match (then_diverged, else_diverged) {
                (false, false) => {}
                (true, false) => {
                    // Continue from the surviving (else) state: the current
                    // state when an `Else` ran, the untouched entry state when
                    // the else arm is empty (the current state is then the
                    // diverged then-arm's residue).
                    if !else_seen {
                        self.rc = fr.entry_rc;
                        self.dead = fr.entry_dead;
                    }
                    if let Some(d) = dst {
                        self.own_fresh_object(d);
                    }
                    return;
                }
                (false, true) => {
                    // Continue from the surviving (then) state.
                    let (then_rc, then_dead) = self.take_then_arm_exit(fr);
                    self.rc = then_rc;
                    self.dead = then_dead;
                    if let Some(d) = dst {
                        self.own_fresh_object(d);
                    }
                    return;
                }
                (true, true) => {
                    self.rc = fr.entry_rc;
                    self.dead = fr.entry_dead;
                    self.diverged = true;
                    return;
                }
            }
            let (then_rc, then_dead) = self.take_then_arm_exit(fr);
            self.check_branch_agreement(i, &then_rc);
            self.merge_branch_exits(then_rc, then_dead);
            // A HEAP branch result: each arm moved its value into the merge
            // (explicitly `Consume`d, or released by `merge_val_move`), so the
            // join owns the moved reference — a fresh object on the `IfThen`
            // dst, released by the scope-end Drop/Consume exactly like a call
            // result (#1037, second gap: the unmodeled dst made every later
            // Drop of it a phantom DoubleFree).
            if let Some(d) = dst {
                self.own_fresh_object(d);
            }
        }
    }

    /// Does this arm's merge value MOVE a heap reference into the branch
    /// result? Three cases (#1037, refined by the nested-branch regression the
    /// charge-probe `branch` fixture caught):
    ///   - already dead: the arm `Consume`d it into the merge (Alloc+Consume);
    ///   - live and OWNED (rc >= 1 — a nested branch result): the merge itself
    ///     is the move, so release the reference here, inside the arm;
    ///   - live but unowned (a borrowed param), or untracked (scalar): no move.
    fn merge_val_move(&mut self, val: Option<ValueId>) -> bool {
        let Some(v) = val else { return false };
        let Some(&o) = self.object_of.get(&v) else { return false };
        if self.dead.get(&v).copied().unwrap_or(true) {
            return true;
        }
        if self.rc.get(&o).copied().unwrap_or(0) >= 1 {
            let _ = release(&self.object_of, &mut self.rc, &mut self.dead, &self.borrowed, v);
            return true;
        }
        false
    }

    /// Extracted from [`Self::step`]'s `EndIf` arm (codopsy r2, #852, phase 1 of 3):
    /// the then arm's exit state, rewinding the scan to the frame's entry state when
    /// there was no `Else` marker. Verbatim.
    fn take_then_arm_exit(
        &mut self,
        fr: BranchFrame,
    ) -> (BTreeMap<ValueId, i64>, BTreeMap<ValueId, bool>) {
        match fr.then_exit {
            Some(t) => t,
            // No Else marker: everything since IfThen was the then arm;
            // the else arm is empty (= the entry state).
            None => {
                let cur = (self.rc.clone(), self.dead.clone());
                self.rc = fr.entry_rc.clone();
                self.dead = fr.entry_dead.clone();
                cur
            }
        }
    }

    /// Extracted from [`Self::step`]'s `EndIf` arm (codopsy r2, #852, phase 2 of 3):
    /// the arms must AGREE on every object's leaving count — a disagreement is a
    /// path-dependent leak or double-free. Verbatim.
    fn check_branch_agreement(&mut self, i: usize, then_rc: &BTreeMap<ValueId, i64>) {
        // Agreement per object (absent = 0 owned refs).
        let keys: BTreeSet<ValueId> = then_rc.keys().chain(self.rc.keys()).copied().collect();
        for k in keys {
            let a = then_rc.get(&k).copied().unwrap_or(0);
            let b = self.rc.get(&k).copied().unwrap_or(0);
            if a != b {
                self.violations.push(violation(i, k, ViolationKind::BranchDisagreement));
            }
        }
    }

    /// Extracted from [`Self::step`]'s `EndIf` arm (codopsy r2, #852, phase 3 of 3):
    /// the state the scan continues from after the branch. Verbatim.
    fn merge_branch_exits(
        &mut self,
        then_rc: BTreeMap<ValueId, i64>,
        then_dead: BTreeMap<ValueId, bool>,
    ) {
        // Continue with the JOIN: pointwise max keeps the run stable
        // after a reported disagreement (no cascading underflows); on
        // agreement it is the common value. A handle self.dead on EITHER
        // path is unusable after the merge.
        for (k, v) in then_rc {
            let e = self.rc.entry(k).or_insert(0);
            if v > *e {
                *e = v;
            }
        }
        for (k, d) in then_dead {
            let e = self.dead.entry(k).or_insert(d);
            *e = *e || d;
        }
    }

    /// Extracted from [`Self::step`] (codopsy r2, #852): the `Prim` arm — the three
    /// NAMEABLE Value-rc events (`handle` carries its source object into a second
    /// handle, `rc_inc`/`rc_dec` move that object's count); every other prim kind is
    /// ownership-neutral. Verbatim.
    fn apply_prim_rc_event(&mut self, kind: &PrimKind, dst: &Option<ValueId>, args: &[ValueId]) {
        match kind {
            PrimKind::Handle => {
                if let (Some(d), Some(&o)) =
                    (dst.as_ref(), args.first().and_then(|a| self.object_of.get(a)))
                {
                    self.object_of.insert(*d, o);
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
            // record field — ALIASES the parent object for accounting, the same
            // conflation `ResErrStr` already makes for the Err String. A `Dup`
            // of it acquires a reference counted on the parent; the matching
            // `Drop` releases it — per-object balance is preserved, and the
            // live-check grounds out on the parent the frame still owns
            // (#1037). An address with NO tracked root stays off the model
            // (the pre-existing load64 floor) — unknown, never guessed.
            PrimKind::LoadHandle => {
                if let (Some(d), Some(&o)) =
                    (dst.as_ref(), args.first().and_then(|a| self.object_of.get(a)))
                {
                    self.object_of.insert(*d, o);
                    self.dead.insert(*d, false);
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

    /// Extracted from [`Self::step`] (codopsy r2, #852): the `SetLocal` arm — a
    /// loop-carried rebind aliases the slot onto the source's object and makes it
    /// live again. Verbatim.
    fn rebind_local_slot(&mut self, local: ValueId, src: ValueId) {
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

pub fn verify_ownership(func: &MirFunction) -> Result<(), Vec<Violation>> {
    // Handle ≠ object. Each known heap HANDLE (ValueId) maps to its OBJECT (the
    // `Alloc`'d representative ValueId); the refcount is per OBJECT. A handle is
    // also tracked LIVE/dead, so a use of a handle after its own drop/consume is
    // caught even when the object lives on through a sibling handle.
    let mut object_of: BTreeMap<ValueId, ValueId> = BTreeMap::new();
    let rc: BTreeMap<ValueId, i64> = BTreeMap::new(); // keyed by object — OUR (callee's) owned refs
    let mut dead: BTreeMap<ValueId, bool> = BTreeMap::new(); // keyed by handle
    let violations: Vec<Violation> = Vec::new();

    let mut borrowed: BTreeSet<ValueId> = BTreeSet::new();
    init_borrowed_params(func, &mut object_of, &mut dead, &mut borrowed);

    // BRANCH JOIN (mirrors the proven checker's `CBranch` rule): each arm of an
    // `IfThen`/`Else`/`EndIf` runs from the SAME entry state, and the arms must
    // AGREE on every object's leaving count (the net may be nonzero — a
    // heap-result branch nets +1 through either arm). Folding the arms FLAT
    // (the old model) counted BOTH arms' events, silently accepting cross-arm
    // compensation — a `Consume` in one arm "balancing" the other arm's missing
    // release, i.e. a path-dependent leak/double-free.

    // Decomposed (#781, cog 140): the per-op transition lives in
    // `OwnershipScan::step`; the maps moved into the scan struct verbatim.
    let mut scan = OwnershipScan {
        object_of,
        rc,
        dead,
        borrowed,
        branches: Vec::new(),
        violations,
        diverged: false,
    };
    for (i, op) in func.ops.iter().enumerate() {
        scan.step(i, op);
    }
    let OwnershipScan { object_of, mut rc, mut dead, borrowed, mut violations, diverged, .. } =
        scan;

    // A TOP-LEVEL `Op::Return` already took the whole boundary obligation at
    // its own op index (and mir_wellformed forbids ops after it), so the tail
    // is unreachable — running the boundary phases again would double-release
    // the returned value and re-report the same leaks.
    if !diverged {
        check_return_release(func, &object_of, &mut rc, &mut dead, &borrowed, &mut violations);
        check_leaks(func, &rc, &mut violations);
    }

    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
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

include!("lib_p2.rs");
