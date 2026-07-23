
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
}

    struct BranchFrame {
        entry_rc: BTreeMap<ValueId, i64>,
        entry_dead: BTreeMap<ValueId, bool>,
        then_exit: Option<(BTreeMap<ValueId, i64>, BTreeMap<ValueId, bool>)>,
    }

impl OwnershipScan {
    /// One op's ownership transition. Verbatim text move of the scan loop body
    /// (locals renamed to fields).
    fn step(&mut self, i: usize, op: &Op) {
        match op {
            Op::Alloc { dst, repr, .. } => {
                debug_assert!(repr.is_heap(), "Alloc of a non-heap repr is malformed MIR");
                self.object_of.insert(*dst, *dst);
                self.rc.insert(*dst, 1);
                self.dead.insert(*dst, false);
            }
            // A rung-4 scalar-list LITERAL is alloc-class: one fresh owned object
            // (the identical accounting the replaced `Alloc{DynList}` had). Its
            // element values are raw i64 slot scalars — no ownership to check.
            Op::ListLit { dst, .. } => {
                self.object_of.insert(*dst, *dst);
                self.rc.insert(*dst, 1);
                self.dead.insert(*dst, false);
            }
            // The rung-4 element load/store BORROW the list handle (live-check,
            // no refcount change — exactly the `Borrow`/`MakeUnique` discipline);
            // the scalar element/index/value carry no ownership.
            Op::ListGetScalar { list, .. } | Op::ListSetScalar { list, .. } => {
                if live_object(&self.object_of, &self.rc, &self.dead, &self.borrowed, *list).is_none() {
                    self.violations.push(violation(i, *list, ViolationKind::UseAfterFree));
                }
            }
            Op::Const { dst: _ } | Op::ConstInt { .. } => {
                // A scalar — no ownership accounting.
            }
            Op::FuncRef { .. } => {
                // A function-table slot index — a scalar constant, no ownership.
            }
            Op::Dup { dst, src } => {
                if let Some(o) = live_object(&self.object_of, &self.rc, &self.dead, &self.borrowed, *src) {
                    // Acquire OUR own reference. A `Dup` of a self.borrowed param has no
                    // prior self.rc entry (we owned none) — start it at 0, then +1.
                    *self.rc.entry(o).or_insert(0) += 1;
                    self.object_of.insert(*dst, o);
                    self.dead.insert(*dst, false);
                } else {
                    self.violations.push(violation(i, *src, ViolationKind::UseAfterFree));
                }
            }
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
                match release(&self.object_of, &mut self.rc, &mut self.dead, &self.borrowed, *v) {
                    Ok(()) => {}
                    Err(()) => self.violations.push(violation(i, *v, ViolationKind::DoubleFree)),
                }
            }
            Op::Consume { v } => match release(&self.object_of, &mut self.rc, &mut self.dead, &self.borrowed, *v) {
                Ok(()) => {}
                Err(()) => self.violations.push(violation(i, *v, ViolationKind::UseAfterMove)),
            },
            Op::Borrow { v } | Op::MakeUnique { v } => {
                if live_object(&self.object_of, &self.rc, &self.dead, &self.borrowed, *v).is_none() {
                    self.violations.push(violation(i, *v, ViolationKind::UseAfterFree));
                }
            }
            Op::Pure { dst: _, uses } => {
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
                for a in args {
                    if let CallArg::Handle(v) = a {
                        if live_object(&self.object_of, &self.rc, &self.dead, &self.borrowed, *v).is_none() {
                            self.violations.push(violation(i, *v, ViolationKind::UseAfterFree));
                        }
                    }
                }
                if let (Some(d), Some(r)) = (dst, result) {
                    if r.is_heap() {
                        self.object_of.insert(*d, *d);
                        self.rc.insert(*d, 1);
                        self.dead.insert(*d, false);
                    }
                }
            }
            // The if-markers carry no ownership of their own, but they scope the
            // BRANCH JOIN: both arms run from the entry state and must agree.
            Op::IfThen { .. } => {
                self.branches.push(BranchFrame {
                    entry_rc: self.rc.clone(),
                    entry_dead: self.dead.clone(),
                    then_exit: None,
                });
            }
            Op::Else { .. } => {
                if let Some(fr) = self.branches.last_mut() {
                    fr.then_exit = Some((self.rc.clone(), self.dead.clone()));
                    self.rc = fr.entry_rc.clone();
                    self.dead = fr.entry_dead.clone();
                }
            }
            Op::EndIf { .. } => {
                if let Some(fr) = self.branches.pop() {
                    let (then_rc, then_dead) = match fr.then_exit {
                        Some(t) => t,
                        // No Else marker: everything since IfThen was the then arm;
                        // the else arm is empty (= the entry state).
                        None => {
                            let cur = (self.rc.clone(), self.dead.clone());
                            self.rc = fr.entry_rc.clone();
                            self.dead = fr.entry_dead.clone();
                            cur
                        }
                    };
                    // Agreement per object (absent = 0 owned refs).
                    let keys: BTreeSet<ValueId> =
                        then_rc.keys().chain(self.rc.keys()).copied().collect();
                    for k in keys {
                        let a = then_rc.get(&k).copied().unwrap_or(0);
                        let b = self.rc.get(&k).copied().unwrap_or(0);
                        if a != b {
                            self.violations.push(violation(i, k, ViolationKind::BranchDisagreement));
                        }
                    }
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
            }
            // Scalar arithmetic — no ownership.
            // A scalar arithmetic op and a primitive-floor op carry no ownership: a
            // scalar result is Copy and a `Prim` handle arg is BORROWED (read only).
            Op::IntBinOp { .. }
            // Loop markers carry no ownership; the body ops between them are
            // per-iteration-balanced (verified flat, one iteration).
            | Op::LoopStart
            | Op::LoopBreakUnless { .. }
            | Op::LoopEnd => {}
            // VALUE-RC modeling (柱C extension) — bring the Value refcount ops out of the prim blind
            // spot for the NAMEABLE case: prim.handle(v) carries its source object in args[0], so the
            // self.rc events on it verify against the same self.rc machine. load64-fed handles have no carrier
            // and stay unmodeled (the differential-test floor). MIRRORED in ownership_certificate.
            Op::Prim { kind, dst, args } => match kind {
                PrimKind::Handle => {
                    if let (Some(d), Some(&o)) =
                        (dst.as_ref(), args.first().and_then(|a| self.object_of.get(a)))
                    {
                        self.object_of.insert(*d, o);
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
            },
            // `SetLocal` into a HEAP slot is a loop-carried REBIND (`acc = acc + [x]`):
            // the slot now aliases the source's object. The slot's OLD object was
            // released by a preceding `Drop` in the loop body, so rebinding makes the
            // slot LIVE again (= the new object), preserving the per-iteration invariant
            // (slot owns exactly one ref at the body's start and end) — exactly the
            // soundness condition proved in OwnershipChecker.v's `check_line_unroll_sound`
            // (a self.rc-preserving loop body is leak/double-free-free for any iteration
            // count). For a SCALAR src (the scalar-TCO loop var) `self.object_of` has no
            // entry, so this is a no-op, as before.
            Op::SetLocal { local, src } => {
                if let Some(o) = self.object_of.get(src).copied() {
                    self.object_of.insert(*local, o);
                    self.dead.insert(*local, false);
                }
            }
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
    };
    for (i, op) in func.ops.iter().enumerate() {
        scan.step(i, op);
    }
    let OwnershipScan { object_of, mut rc, mut dead, borrowed, mut violations, .. } = scan;

    check_return_release(func, &object_of, &mut rc, &mut dead, &borrowed, &mut violations);
    check_leaks(func, &rc, &mut violations);

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
