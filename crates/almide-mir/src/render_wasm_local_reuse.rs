// Render-level LOCAL-SLOT REUSE for oversized functions (#1554 second half).
//
// The renderer allocates wasm locals SSA-style — one `(local $vN)` per
// defined value, never reused — so a function's local count is proportional
// to its OP count, and a large-but-ordinary function (teastia's Markdown
// `render` measured ~47.5k) walks into the validator's 50,000-local ceiling.
// This pass assigns values to SHARED slots by liveness (a classic linear
// scan over the flat op list), so the count is bounded by peak simultaneous
// liveness instead.
//
// DISCIPLINE (the #806 break-fusion precedent): render-level only. The MIR
// and its certificate are untouched — the plan rewrites a CLONE of the
// function immediately before text emission, and only for functions over
// [`LOCAL_REUSE_THRESHOLD`] distinct locals, so every module below the
// threshold renders byte-identically to before (zero churn on the proven
// corpus, prelude digests included).
//
// SOUNDNESS ARGUMENT, piece by piece:
// - The op list is textual-order = execution-order except LOOP back-edges
//   (`LoopStart`/`LoopEnd` markers). A value's naive live range is
//   [first def index, last touch index].
// - LOOP EXTENSION: a value defined BEFORE a loop and touched inside it is
//   read again on every iteration, so its range extends to the loop's END —
//   otherwise a later-in-the-body value could take the slot after the last
//   TEXTUAL touch and clobber the next iteration's read. Applied to
//   fixpoint, so nested loops chain outward.
// - Values defined INSIDE a loop are re-defined every iteration before
//   their uses (the lowering's defs dominate uses; loop-carried state goes
//   through pre-loop values via `SetLocal`), so their naive ranges stand.
//   A use-before-def anywhere BAILS the whole plan (belt and suspenders).
// - IF ARMS (`IfThen`/`Else`/`EndIf`) do not repeat, and arm-local values'
//   ranges cannot overlap the other arm's defs incorrectly: sharing a slot
//   across arms is exactly wasm-legal local reuse (each path defines before
//   its own uses; cross-arm reads go through the `IfThen` merge dst, whose
//   range spans both arms and so never shares).
// - TYPES: slots are pooled by the SAME (f64-class, wasm type) pair the
//   local declaration uses, so a merged local's declared type fits every
//   occupant.
// - EXCLUSIONS: params (their slots are the signature), the return value,
//   `heap_slot_masks` keys (the mask side-table is keyed by ValueId — two
//   masked values sharing an id would collide their masks), and
//   `Op::Const`-defined values may REUSE nothing (their render is "leave the
//   local at wasm's zero default", which a previous occupant's bytes would
//   break) though others may reuse THEIR slot after last touch.
// - RENDER SHAPES: every op renders as "compute full expression, then ONE
//   `local.set`", so an op whose dst shares a slot with one of its own
//   operands still reads the operand first (`local.set $x (… (local.get
//   $x) …)`), and the expiry sweep is STRICT (`end < def`), so an operand
//   whose last touch IS this op never shares with its dst.
// - Downstream render analyses: break fusion and BCE are DISABLED for a
//   reused function (`plain` render — they pattern-match on single-def
//   value identities the merge no longer guarantees); the const-wrap
//   peephole keys on text-level SINGLE-set locals and so excludes merged
//   ones by construction.

/// Activate reuse only above this many distinct locals. Well under the 50k
/// ceiling (headroom for growth within one fn between releases), well over
/// anything in the spec corpus (the largest measured render fns sit ~7k) —
/// so the pass is a no-op for every existing proven module.
pub(crate) const LOCAL_REUSE_THRESHOLD: usize = 8_000;

/// The active threshold: the compile-time default, or the
/// `ALMIDE_LOCAL_REUSE_THRESHOLD` env override — a TEST knob so the whole
/// spec corpus can run with the transform FORCED ON (threshold 0/low),
/// cross-validating it against shapes far beyond any hand-written probe.
/// Production builds never set it.
pub(crate) fn local_reuse_threshold() -> usize {
    std::env::var("ALMIDE_LOCAL_REUSE_THRESHOLD")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(LOCAL_REUSE_THRESHOLD)
}

/// The distinct-local count the SSA-style declaration would emit (params +
/// first-def values) — the same count `declare_fn_locals` produces, minus
/// the constant drop-scratch registers.
pub(crate) fn distinct_local_count(func: &MirFunction) -> usize {
    let mut seen: BTreeSet<ValueId> = func.params.iter().map(|p| p.value).collect();
    for op in &func.ops {
        if let Some(d) = defined_value(op) {
            seen.insert(d);
        }
    }
    seen.len()
}

pub(crate) struct LocalReusePlan {
    remap: BTreeMap<ValueId, ValueId>,
    /// Locals the rewritten function will declare (params included).
    pub(crate) slot_count: usize,
}

/// Compute a slot-sharing plan, or `None` when nothing merges (or a
/// soundness guard trips — the caller then renders SSA-style as today and
/// the 50k wall stays the honest surface).
pub(crate) fn plan_local_reuse(func: &MirFunction) -> Option<LocalReusePlan> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;

    let params: BTreeSet<ValueId> = func.params.iter().map(|p| p.value).collect();

    // First-def / first-touch / last-touch indexes over the flat op list.
    let mut def_at: BTreeMap<ValueId, usize> = BTreeMap::new();
    let mut first_touch: BTreeMap<ValueId, usize> = BTreeMap::new();
    let mut last_touch: BTreeMap<ValueId, usize> = BTreeMap::new();
    let mut const_defined: BTreeSet<ValueId> = BTreeSet::new();
    let mut vals_buf: Vec<ValueId> = Vec::new();
    for (i, op) in func.ops.iter().enumerate() {
        if let Some(d) = defined_value(op) {
            def_at.entry(d).or_insert(i);
            // `Op::Const` renders NOTHING (the local is wasm's zero default —
            // a previous occupant's bytes would break it), and `Op::ConstInt`
            // is the Fuser's fn-wide-constant invariant (`scan_consts` maps
            // dst → value with no position: a merged id carrying two consts,
            // or a const plus anything else, substitutes the WRONG number at
            // some read — the forced sweep caught it as hash corruption and a
            // softmax trap). Both are EXCLUDED outright: never remapped,
            // never a rep.
            if matches!(op, Op::Const { .. } | Op::ConstInt { .. }) {
                const_defined.insert(d);
            }
        }
        vals_buf.clear();
        op_values(op, &mut vals_buf);
        for v in &vals_buf {
            first_touch.entry(*v).or_insert(i);
            last_touch.insert(*v, i);
        }
    }

    // Guard: every non-param value must be defined at or before its first
    // touch — a violation means an op-list shape this pass's model does not
    // cover, so bail (no reuse, no risk).
    for (v, ft) in &first_touch {
        if params.contains(v) {
            continue;
        }
        match def_at.get(v) {
            Some(d) if d <= ft => {}
            _ => return None,
        }
    }

    // Loop regions (start, end) from the flat markers, plus a per-op nesting
    // depth (LoopStart/IfThen open at their own index's depth; body ops sit
    // one deeper) and each value's DEF depth.
    let mut regions: Vec<(usize, usize)> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    let mut depth_at: Vec<usize> = Vec::with_capacity(func.ops.len());
    let mut def_depth: BTreeMap<ValueId, usize> = BTreeMap::new();
    let mut depth = 0usize;
    for (i, op) in func.ops.iter().enumerate() {
        match op {
            Op::LoopStart => {
                depth_at.push(depth);
                stack.push(i);
                depth += 1;
            }
            Op::LoopEnd => {
                depth = depth.saturating_sub(1);
                depth_at.push(depth);
                let s = stack.pop()?;
                regions.push((s, i));
            }
            Op::IfThen { .. } => {
                depth_at.push(depth);
                depth += 1;
            }
            Op::Else { .. } => {
                depth_at.push(depth.saturating_sub(1));
            }
            Op::EndIf { .. } => {
                depth = depth.saturating_sub(1);
                depth_at.push(depth);
            }
            _ => depth_at.push(depth),
        }
        if let Some(d) = defined_value(op) {
            def_depth.entry(d).or_insert_with(|| depth_at[i].max(depth));
        }
    }
    if !stack.is_empty() {
        return None; // unbalanced markers — not a shape to touch
    }

    // Fixpoint loop extension, two rules per region:
    //  (a) live-into-a-loop ⇒ live to its end (the pre-loop def read each
    //      iteration);
    //  (b) CONDITIONALLY defined inside a loop ⇒ live to its end. A def at
    //      nesting depth deeper than the loop body's own (inside an if arm,
    //      or a nested loop that may run zero iterations) is NOT re-executed
    //      on every iteration — on the skipping iteration the local carries
    //      the PREVIOUS iteration's value across the back edge, a liveness
    //      the textual range cannot see. The forced-threshold sweep caught
    //      the miss as data corruption in the base64/hash/softmax loops.
    //      An UNCONDITIONAL body def (depth == body depth) redefines before
    //      every use and keeps its narrow range.
    loop {
        let mut changed = false;
        for (s, e) in &regions {
            let body_depth = depth_at[*s] + 1;
            for (v, d) in &def_at {
                let u = match last_touch.get_mut(v) {
                    Some(u) => u,
                    None => continue,
                };
                let live_in = d < s && *u >= *s && *u < *e;
                let cond_in = d > s
                    && *d < *e
                    && *u < *e
                    && def_depth.get(v).copied().unwrap_or(0) > body_depth;
                if live_in || cond_in {
                    *u = *e;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }

    // Slot type pools: the SAME classification the local declaration uses.
    let reprs = value_reprs_wasm(func);
    let floats = classify_f64_locals(func);
    let key_of = |v: &ValueId| -> (bool, &'static str) {
        if floats.contains(v) {
            (true, "f64")
        } else {
            (false, wasm_ty(reprs.get(v).copied().unwrap_or(SCALAR_REPR)))
        }
    };

    let excluded: BTreeSet<ValueId> = params
        .iter()
        .copied()
        .chain(func.ret)
        .chain(func.heap_slot_masks.keys().copied())
        .chain(const_defined.iter().copied())
        .collect();

    // Linear scan in def order.
    let mut defs: Vec<(usize, ValueId)> =
        def_at.iter().map(|(v, d)| (*d, *v)).collect();
    defs.sort_unstable();

    type Key = (bool, &'static str);
    let mut active: BTreeMap<Key, BinaryHeap<Reverse<(usize, ValueId)>>> = BTreeMap::new();
    let mut free: BTreeMap<Key, Vec<ValueId>> = BTreeMap::new();
    let mut remap: BTreeMap<ValueId, ValueId> = BTreeMap::new();

    for (d, v) in defs {
        if excluded.contains(&v) {
            continue;
        }
        let key = key_of(&v);
        let end = last_touch.get(&v).copied().unwrap_or(d);
        let act = active.entry(key).or_default();
        let fr = free.entry(key).or_default();
        while let Some(Reverse((e, rep))) = act.peek().copied() {
            if e < d {
                act.pop();
                fr.push(rep);
            } else {
                break;
            }
        }
        if let Some(rep) = fr.pop() {
            remap.insert(v, rep);
            act.push(Reverse((end, rep)));
            continue;
        }
        act.push(Reverse((end, v)));
    }

    if remap.is_empty() {
        return None;
    }
    let slot_count = distinct_local_count(func) - remap.len();
    Some(LocalReusePlan { remap, slot_count })
}

/// Apply the plan to a CLONE — every op operand/dst runs through the remap.
/// `params`, `ret` and `heap_slot_masks` keys were excluded from the plan, so
/// they need no rewriting (their ids never appear as remap keys).
pub(crate) fn apply_local_reuse(func: &MirFunction, plan: &LocalReusePlan) -> MirFunction {
    let mut f = func.clone();
    for op in &mut f.ops {
        op_values_mut(op, &mut |v| {
            if let Some(r) = plan.remap.get(v) {
                *v = *r;
            }
        });
    }
    f
}

/// The MUT mirror of [`op_values`]: visit every [`ValueId`] an op touches.
/// Exhaustive on purpose — a new Op variant breaks the build here until it
/// declares its values, the same registry discipline as `op_values` itself.
fn op_values_mut(op: &mut Op, f: &mut impl FnMut(&mut ValueId)) {
    let call_args = |args: &mut Vec<CallArg>, f: &mut dyn FnMut(&mut ValueId)| {
        for a in args {
            match a {
                CallArg::Handle(v) | CallArg::Scalar(v) => f(v),
                CallArg::Imm(_) | CallArg::Label(_) => {}
            }
        }
    };
    let opt = |v: &mut Option<ValueId>, f: &mut dyn FnMut(&mut ValueId)| {
        if let Some(v) = v {
            f(v);
        }
    };
    match op {
        Op::Charge { .. } | Op::LoopStart | Op::LoopEnd => {}
        Op::ChargeDyn { src, .. } => f(src),
        Op::Alloc { dst, init, .. } => {
            f(dst);
            match init {
                Init::DynStr { len } | Init::DynList { len } | Init::DynListStr { len } => f(len),
                Init::OptSome { payload } | Init::ResOkScalar { payload } => f(payload),
                Init::ResErrStr { piece } => f(piece),
                Init::Opaque
                | Init::Empty
                | Init::OptNone
                | Init::IntList(_)
                | Init::Bytes(_)
                | Init::Str(_) => {}
            }
        }
        Op::Const { dst } | Op::ConstInt { dst, .. } | Op::FuncRef { dst, .. } => f(dst),
        Op::Dup { dst, src } => {
            f(dst);
            f(src);
        }
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
        | Op::DropWrapperRec { v, .. }
        | Op::Consume { v }
        | Op::Borrow { v }
        | Op::MakeUnique { v } => f(v),
        Op::Pure { dst, uses } => {
            f(dst);
            for u in uses {
                f(u);
            }
        }
        Op::Call { dst, args, .. } | Op::CallFn { dst, args, .. } | Op::CallImport { dst, args, .. } => {
            opt(dst, f);
            call_args(args, f);
        }
        Op::CallIndirect { dst, table_idx, args, .. } => {
            opt(dst, f);
            f(table_idx);
            call_args(args, f);
        }
        Op::ListLit { dst, elems } => {
            f(dst);
            for e in elems {
                f(e);
            }
        }
        Op::Prim { dst, args, .. } => {
            opt(dst, f);
            for a in args {
                f(a);
            }
        }
        Op::ListGetScalar { dst, list, idx } => {
            f(dst);
            f(list);
            f(idx);
        }
        Op::ListSetScalar { list, idx, val } => {
            f(list);
            f(idx);
            f(val);
        }
        Op::IntBinOp { dst, a, b, .. } => {
            f(dst);
            f(a);
            f(b);
        }
        Op::SetLocal { local, src } => {
            f(local);
            f(src);
        }
        Op::IfThen { cond, dst } => {
            f(cond);
            opt(dst, f);
        }
        Op::Else { val } | Op::EndIf { val } | Op::Return { val } => opt(val, f),
        Op::LoopBreakUnless { cond } => f(cond),
    }
}

// ── Deferred terminal drops (#1554, the half that makes reuse BITE) ─────────
//
// The lowering's conservative scope-end convention parks one flat `Drop` per
// heap temp at the END of the function — so every such temp's TRUE liveness
// runs to fn-end and slot reuse alone compresses nothing (the 15k-local probe
// merged 8). This transform moves those handles OUT of locals: a fn-entry
// scratch block (a plain rc-headered DynList) receives each eligible handle
// right after its def (`i64.store` — the cell machinery's exact store shape),
// the terminal `Drop`s are deleted, and ONE loop at the end `rc_dec`s every
// slot, then the block. After it, an eligible value's last touch is the store
// beside its def — ranges collapse, and the linear scan does the rest.
//
// Every inserted op is EXISTING vocabulary (ConstInt/Alloc DynList/Prim
// Handle/Store/LoadHandle/IntBinOp/SetLocal/loop markers) — no new op, no new
// render arm, no certificate surface. rc_dec order changes (buffer order vs
// reverse-declaration order): decrements on independent owners commute, and
// the sentinel only concerns TOTAL counts, which are unchanged.
//
// ELIGIBILITY (all required — everything else keeps its local + terminal Drop):
//  - dropped by a FLAT `Op::Drop` inside the terminal drop cluster (the
//    maximal all-Drop-family op suffix), and that is its ONLY drop;
//  - defined exactly once, at CONTROL DEPTH 0 (not inside an if arm or loop —
//    a conditional def would leave its buffer slot as raw heap garbage for
//    the end loop to rc_dec);
//  - a heap ptr (i32) — the only kind with a terminal flat Drop;
//  - never a `SetLocal`/`MakeUnique` target (either can repoint the local
//    AFTER the spill store, leaving a stale handle in the buffer);
//  - not a param / the return value / a `heap_slot_masks` key.
pub(crate) fn spill_terminal_drops(func: &MirFunction) -> Option<MirFunction> {
    // A REGION-specialized function (region_alloc.rs rewrote its
    // consume(produce) windows to bump-region allocation) is off-limits
    // wholesale: a region handle is a bump pointer, not an rc block, and
    // spilling one into the buffer for the end loop to `rc_dec` corrupts the
    // free list — the forced-threshold sweep caught exactly that as string
    // corruption in the base64/hash/softmax fixtures. Region fns are compact
    // compute loops far below the production threshold anyway.
    let has_region_prims = func.ops.iter().any(|op| {
        matches!(
            op,
            Op::Prim {
                kind: PrimKind::RegionSave
                    | PrimKind::RegionRestore
                    | PrimKind::RegionAllocC { .. }
                    | PrimKind::RegionLoadH { .. }
                    | PrimKind::RegionLoadS { .. }
                    | PrimKind::RegionStoreH { .. }
                    | PrimKind::RegionStoreS { .. }
                    | PrimKind::RegionTagSel { .. },
                ..
            }
        )
    });
    if has_region_prims {
        return None;
    }
    // The terminal cluster: maximal suffix of Drop-family ops.
    let is_drop_family = |op: &Op| {
        matches!(
            op,
            Op::Drop { .. }
                | Op::DropListStr { .. }
                | Op::DropValue { .. }
                | Op::DropListValue { .. }
                | Op::DropListStrValue { .. }
                | Op::DropListStrStr { .. }
                | Op::DropListIntStr { .. }
                | Op::DropListStrInt { .. }
                | Op::DropResultListValue { .. }
                | Op::DropResultValue { .. }
                | Op::DropResultStrInt { .. }
                | Op::DropResultValueInt { .. }
                | Op::DropResultListValueInt { .. }
                | Op::DropResultListStrInt { .. }
                | Op::DropResultListStr { .. }
                | Op::DropListListStr { .. }
                | Op::DropVariant { .. }
                | Op::DropWrapperRec { .. }
        )
    };
    let mut cluster_start = func.ops.len();
    while cluster_start > 0 && is_drop_family(&func.ops[cluster_start - 1]) {
        cluster_start -= 1;
    }
    if cluster_start == func.ops.len() {
        return None;
    }

    // Per-value facts: def count/index/depth, drop count, mutation targets.
    let mut def_count: BTreeMap<ValueId, usize> = BTreeMap::new();
    let mut def_depth: BTreeMap<ValueId, usize> = BTreeMap::new();
    let mut def_index: BTreeMap<ValueId, usize> = BTreeMap::new();
    let mut drop_count: BTreeMap<ValueId, usize> = BTreeMap::new();
    let mut mutated: BTreeSet<ValueId> = BTreeSet::new();
    let mut depth = 0usize;
    let mut max_id = func.params.iter().map(|p| p.value.0).max().unwrap_or(0);
    for (i, op) in func.ops.iter().enumerate() {
        match op {
            Op::IfThen { .. } | Op::LoopStart => depth += 1,
            Op::EndIf { .. } | Op::LoopEnd => depth = depth.saturating_sub(1),
            Op::SetLocal { local, .. } => {
                mutated.insert(*local);
            }
            Op::MakeUnique { v } => {
                mutated.insert(*v);
            }
            Op::Drop { v } => {
                *drop_count.entry(*v).or_insert(0) += 1;
            }
            _ => {}
        }
        if let Some(d) = defined_value(op) {
            *def_count.entry(d).or_insert(0) += 1;
            def_depth.entry(d).or_insert(depth);
            def_index.entry(d).or_insert(i);
        }
        let mut vals = Vec::new();
        op_values(op, &mut vals);
        for v in vals {
            max_id = max_id.max(v.0);
        }
    }

    let reprs = value_reprs_wasm(func);
    let floats = classify_f64_locals(func);
    let excluded: BTreeSet<ValueId> = func
        .params
        .iter()
        .map(|p| p.value)
        .chain(func.ret)
        .chain(func.heap_slot_masks.keys().copied())
        .collect();

    // Eligible values, in cluster order (stable slot numbering).
    let mut eligible: Vec<ValueId> = Vec::new();
    let mut seen: BTreeSet<ValueId> = BTreeSet::new();
    for op in &func.ops[cluster_start..] {
        let Op::Drop { v } = op else { continue };
        if seen.contains(v)
            || excluded.contains(v)
            || mutated.contains(v)
            || floats.contains(v)
            || def_count.get(v).copied() != Some(1)
            || def_depth.get(v).copied() != Some(0)
            || drop_count.get(v).copied() != Some(1)
            || wasm_ty(reprs.get(v).copied().unwrap_or(SCALAR_REPR)) != "i32"
        {
            continue;
        }
        seen.insert(*v);
        eligible.push(*v);
    }
    // Below this, the buffer + loop overhead outweighs the ~n saved locals.
    if eligible.len() < 64 {
        return None;
    }
    let n = eligible.len();
    let slot_of: BTreeMap<ValueId, usize> =
        eligible.iter().enumerate().map(|(k, v)| (*v, k)).collect();

    let mut next = max_id + 1;
    let mut fresh = || {
        next += 1;
        ValueId(next - 1)
    };

    // Entry: cN; spill = alloc DynList(cN); h = handle(spill); cur = h + 12.
    // The store CURSOR advances by 8 after each spill store (SetLocal), so no
    // per-slot ConstInt offset value ever exists — ConstInt-defined values are
    // reuse-EXCLUDED (the Fuser's fn-wide-constant invariant), and a per-slot
    // constant would hand back one immortal local per spilled value, undoing
    // the whole compression (measured: 20 → 15,025 before this cursor).
    let c_n = fresh();
    let spill = fresh();
    let h = fresh();
    let c12 = fresh();
    let c8 = fresh();
    let cur = fresh();
    let entry = vec![
        Op::ConstInt { dst: c_n, value: n as i64 },
        Op::Alloc {
            dst: spill,
            repr: Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: Init::DynList { len: c_n },
        },
        Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![spill] },
        Op::ConstInt { dst: c12, value: LIST_HEADER as i64 },
        Op::ConstInt { dst: c8, value: ELEM_SIZE as i64 },
        Op::IntBinOp { dst: cur, op: IntOp::Add, a: h, b: c12 },
    ];

    // Rebuild the op list: entry ops, then each original op (spill store
    // appended right after an eligible def; terminal Drops of eligible values
    // deleted), then the end loop + buffer free.
    let mut ops: Vec<Op> = Vec::with_capacity(func.ops.len() + entry.len() + 6 * n + 16);
    ops.extend(entry);
    for (i, op) in func.ops.iter().enumerate() {
        if i >= cluster_start {
            if let Op::Drop { v } = op {
                if slot_of.contains_key(v) {
                    continue;
                }
            }
        }
        ops.push(op.clone());
        if let Some(d) = defined_value(op) {
            if slot_of.contains_key(&d) && def_index.get(&d).copied() == Some(i) {
                // Store at the cursor, then advance it — the end loop walks
                // slots in the SAME def order, so indexes agree implicitly.
                let hv = fresh();
                let inc = fresh();
                ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(hv), args: vec![d] });
                ops.push(Op::Prim {
                    kind: PrimKind::Store { width: 8 },
                    dst: None,
                    args: vec![cur, hv],
                });
                ops.push(Op::IntBinOp { dst: inc, op: IntOp::Add, a: cur, b: c8 });
                ops.push(Op::SetLocal { local: cur, src: inc });
            }
        }
    }
    // End loop: for i in 0..N { rc_dec(load(spill + 12 + 8i)) }; free buffer.
    let ctr = fresh();
    let one = fresh();
    let cnd = fresh();
    let m = fresh();
    let a1 = fresh();
    let a2 = fresh();
    let w = fresh();
    let inc = fresh();
    ops.push(Op::ConstInt { dst: ctr, value: 0 });
    ops.push(Op::ConstInt { dst: one, value: 1 });
    ops.push(Op::LoopStart);
    ops.push(Op::IntBinOp { dst: cnd, op: IntOp::Lt, a: ctr, b: c_n });
    ops.push(Op::LoopBreakUnless { cond: cnd });
    ops.push(Op::IntBinOp { dst: m, op: IntOp::Mul, a: ctr, b: c8 });
    ops.push(Op::IntBinOp { dst: a1, op: IntOp::Add, a: h, b: c12 });
    ops.push(Op::IntBinOp { dst: a2, op: IntOp::Add, a: a1, b: m });
    ops.push(Op::Prim { kind: PrimKind::LoadHandle, dst: Some(w), args: vec![a2] });
    ops.push(Op::Drop { v: w });
    ops.push(Op::IntBinOp { dst: inc, op: IntOp::Add, a: ctr, b: one });
    ops.push(Op::SetLocal { local: ctr, src: inc });
    ops.push(Op::LoopEnd);
    ops.push(Op::Drop { v: spill });

    let mut out = func.clone();
    out.ops = ops;
    Some(out)
}
