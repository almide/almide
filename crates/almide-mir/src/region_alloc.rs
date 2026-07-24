//! Region-specialized allocation (issue #838, stage 1): the
//! `consume(produce(scalars))` window — a heap value built by a PURE, CLOSED
//! call tree, read by a borrowing consumer with a SCALAR result, and dropped
//! immediately — becomes a bump REGION:
//!
//! ```text
//! t = CallFn make(scalars)      ; heap result        sp = RegionSave
//! s = CallFn check(t, …)   →    ; scalar result      t = CallFn __rgn_make(…)
//! Drop t                        ;                    s = CallFn __rgn_check(t, …)
//!                               ;                    RegionRestore(sp)
//! ```
//!
//! `RegionSave` packs the allocator state (`bump | freelist << 32`) and
//! EMPTIES the free-list, so every `$alloc` inside the region falls through
//! the (now empty) free-list scan and bump-allocates; `RegionRestore` puts
//! both back — the whole object graph born inside the region is reclaimed by
//! one frontier reset, and no free-list block can have been captured into it.
//! The `__rgn_` CLONES of the transitive callee set drop their `Drop*` ops
//! (per-node teardown is the frontier reset) and their `Dup` rc_inc (sharing
//! needs no count when nothing frees); binarytrees' per-node cost falls from
//! free-list-scan + header + recursive `__drop_Tree` walk to a bump.
//!
//! SOUNDNESS — the window is rewritten only when ALL of the following are
//! mechanically verified over the transitive callee closure C of {f, g}:
//!
//! 1. CLOSED: every `CallFn` target is itself in C (defined `MirFunction`s —
//!    no imports, no indirect/closure calls, no runtime `Op::Call`s).
//! 2. PURE & ESCAPE-FREE: no host-capability prims, and no mutable-global
//!    slot access (a `ConstInt` in the slot address range) — the only places
//!    a region pointer could be stored that outlive the window. `g` returns
//!    a SCALAR, and `t` is used exactly {def, one Handle arg, drop}, so the
//!    region's object graph is unreachable after the restore.
//! 3. NO COW / NO free-list interaction inside: `MakeUnique` is rejected
//!    (its clone path would allocate a survivor), as are the `Alloc` inits
//!    that route through `$list_new` and the multi-op recursive-drop
//!    families (their suppression is not a single-op removal).
//! 4. Traps inside a region (bounds, div) abort the process — identical
//!    observables, no cleanup obligations.
//!
//! This runs INSIDE the wasm renderer entry (after pipeline verification, the
//! same trust position as the BCE loop versioning): the VERIFIED program is
//! the input; the rewrite is a render-side refinement whose fidelity is
//! covered by the differential gates (spec corpus, diff-fuzz, output-parity).

use crate::{CallArg, MirFunction, MirProgram, Op, PrimKind, ValueId};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Ops allowed inside a region clone. Anything else disqualifies the window.
fn region_safe_op(op: &Op, mg_lo: i64, mg_hi: i64, in_closure: &BTreeSet<String>) -> bool {
    match op {
        Op::Const { .. }
        | Op::IntBinOp { .. }
        | Op::SetLocal { .. }
        | Op::IfThen { .. }
        | Op::Else { .. }
        | Op::EndIf { .. }
        | Op::LoopStart
        | Op::LoopBreakUnless { .. }
        | Op::LoopEnd
        | Op::ListGetScalar { .. }
        | Op::ListSetScalar { .. }
        | Op::ListLit { .. }
        | Op::Dup { .. }
        | Op::Drop { .. }
        | Op::DropVariant { .. }
        | Op::Consume { .. }
        | Op::Borrow { .. }
        | Op::Pure { .. } => true,
        // A mutable-global access materializes its slot address as a ConstInt
        // in [MG_SLOT_BASE, MG_SLOT_BASE + 8·count) — the one hole through
        // which a region pointer could escape (or an outside object leak in).
        Op::ConstInt { value, .. } => *value < mg_lo || *value >= mg_hi,
        // Inits that render through a direct `$alloc` (the bump-safe path).
        // IntList/OptSome/OptNone/Opaque route through `$list_new`, whose own
        // internal `$alloc` is fine, but whose PUSH_HEADROOM/list_set shape is
        // not audited here — reject until needed.
        Op::Alloc { init, .. } => matches!(
            init,
            crate::Init::Str(_)
                | crate::Init::Bytes(_)
                | crate::Init::DynStr { .. }
                | crate::Init::DynList { .. }
                | crate::Init::DynListStr { .. }
        ),
        Op::Prim { kind, .. } => matches!(
            kind,
            PrimKind::FloatUn(_)
                | PrimKind::FloatBin(_)
                | PrimKind::FloatCmp(_)
                | PrimKind::F64FromInt
                | PrimKind::IntToFloat
                | PrimKind::FloatToInt
                | PrimKind::FloatBits
                | PrimKind::F32Demote
                | PrimKind::F32Promote
                | PrimKind::IntToF32
                | PrimKind::F32Bits
                | PrimKind::F32Bin(_)
                | PrimKind::F32Cmp(_)
                | PrimKind::F32Un(_)
                | PrimKind::Handle
                | PrimKind::Load { .. }
                | PrimKind::LoadHandle
                | PrimKind::Store { .. }
                | PrimKind::ElemAddr
        ),
        Op::CallFn { name, .. } => in_closure.contains(name),
        _ => false,
    }
}

/// Transitive `CallFn` closure of `roots` over the program's own functions.
/// `None` when a callee is not a defined `MirFunction` (import/runtime name).
fn callee_closure(
    prog: &MirProgram,
    roots: [&str; 2],
) -> Option<BTreeSet<String>> {
    let by_name: BTreeMap<&str, &MirFunction> =
        prog.functions.iter().map(|f| (f.name.as_str(), f)).collect();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut work: VecDeque<String> = roots.iter().map(|s| s.to_string()).collect();
    while let Some(n) = work.pop_front() {
        if !seen.insert(n.clone()) {
            continue;
        }
        let f = by_name.get(n.as_str())?;
        for op in &f.ops {
            if let Op::CallFn { name, .. } = op {
                if !seen.contains(name) {
                    work.push_back(name.clone());
                }
            }
        }
    }
    Some(seen)
}

fn rgn_name(n: &str) -> String {
    format!("__rgn_{n}")
}

/// The window rooted at `ops[i]`: `(t, f, g, drop_idx)` — see module doc.
fn match_region_window(
    ops: &[Op],
    i: usize,
    occ: &BTreeMap<ValueId, usize>,
) -> Option<(ValueId, String, String, usize)> {
    let Op::CallFn { dst: Some(t), name: f, args: fargs, result: Some(fres) } = &ops[i] else {
        return None;
    };
    if !fres.is_heap() {
        return None;
    }
    if !fargs.iter().all(|a| matches!(a, CallArg::Scalar(_) | CallArg::Imm(_))) {
        return None;
    }
    let Op::CallFn { name: g, args: gargs, result: gres, .. } = &ops[i + 1] else {
        return None;
    };
    if matches!(gres, Some(r) if r.is_heap()) {
        return None;
    }
    let mut t_handles = 0;
    for a in gargs {
        match a {
            CallArg::Handle(v) if v == t => t_handles += 1,
            CallArg::Handle(_) => return None,
            CallArg::Scalar(v) if v == t => return None,
            CallArg::Scalar(_) | CallArg::Imm(_) => {}
            CallArg::Label(_) => return None,
        }
    }
    if t_handles != 1 {
        return None;
    }
    // The drop of `t` may trail the consumer by a few scalar ops (`total =
    // total + check(make(d))` puts the add/rebind between). Any intervening
    // op is safe: `occ == 3` below proves nothing in between can reference
    // `t`, and the rewrite places the restore immediately after `g` — the
    // intervening ops then run OUTSIDE the region, exactly as they would
    // have with the original per-object drop.
    const DROP_SCAN: usize = 16;
    let mut drop_at = None;
    for (j, op) in ops.iter().enumerate().skip(i + 2).take(DROP_SCAN) {
        match op {
            Op::Drop { v } | Op::DropVariant { v, .. } if v == t => {
                drop_at = Some(j);
                break;
            }
            _ => {}
        }
    }
    let drop_at = drop_at?;
    // t is window-local: its def + the Handle arg + the drop, nothing else.
    if occ.get(t).copied() != Some(3) {
        return None;
    }
    Some((*t, f.clone(), g.clone(), drop_at))
}

/// Rewrite every qualifying window and append the `__rgn_` clones. Applied to
/// the wasm leg only, after pipeline verification, before the reachability
/// prune (the prune scans rendered text, so the clone calls keep them live).
pub fn apply_region_specialization(prog: &mut MirProgram) {
    let mg_lo = crate::MG_SLOT_BASE as i64;
    let mg_hi = mg_lo + 8 * prog.mutable_global_count as i64;
    let mut clones_needed: BTreeSet<String> = BTreeSet::new();
    let mut qualified_cache: BTreeMap<(String, String), Option<BTreeSet<String>>> =
        BTreeMap::new();

    // Pass 1: find windows per function, qualify their closures, rewrite ops.
    let names: Vec<String> = prog.functions.iter().map(|f| f.name.clone()).collect();
    for fi in 0..prog.functions.len() {
        // Skip functions that are themselves clones (added in a prior call).
        if names.get(fi).is_some_and(|n| n.starts_with("__rgn_")) {
            continue;
        }
        let mut occ: BTreeMap<ValueId, usize> = BTreeMap::new();
        {
            let f = &prog.functions[fi];
            let mut vals: Vec<ValueId> = Vec::new();
            for op in &f.ops {
                vals.clear();
                crate::render_wasm::op_values(op, &mut vals);
                for v in &vals {
                    *occ.entry(*v).or_insert(0) += 1;
                }
            }
        }
        let mut max_id: u32 = 0;
        for v in occ.keys() {
            max_id = max_id.max(v.0);
        }
        for p in &prog.functions[fi].params {
            max_id = max_id.max(p.value.0);
        }
        let mut i = 0;
        let mut out: Vec<Op> = Vec::new();
        while i < prog.functions[fi].ops.len() {
            let window = if i + 3 <= prog.functions[fi].ops.len() {
                match_region_window(&prog.functions[fi].ops, i, &occ)
            } else {
                None
            };
            if let Some((_t, f, g, drop_at)) = window {
                let key = (f.clone(), g.clone());
                let closure = qualified_cache.entry(key).or_insert_with(|| {
                    let c = callee_closure(prog, [f.as_str(), g.as_str()])?;
                    let ok = c.iter().all(|n| {
                        prog.functions
                            .iter()
                            .find(|h| &h.name == n)
                            .is_some_and(|h| {
                                h.ops.iter().all(|op| region_safe_op(op, mg_lo, mg_hi, &c))
                            })
                    });
                    ok.then_some(c)
                });
                if let Some(c) = closure.clone() {
                    max_id += 1;
                    let sp = ValueId(max_id);
                    let (fc, gc) = {
                        let ops = &prog.functions[fi].ops;
                        let mut fc = ops[i].clone();
                        let mut gc = ops[i + 1].clone();
                        if let Op::CallFn { name, .. } = &mut fc {
                            *name = rgn_name(name);
                        }
                        if let Op::CallFn { name, .. } = &mut gc {
                            *name = rgn_name(name);
                        }
                        (fc, gc)
                    };
                    out.push(Op::Prim {
                        kind: PrimKind::RegionSave,
                        dst: Some(sp),
                        args: vec![],
                    });
                    out.push(fc);
                    out.push(gc);
                    out.push(Op::Prim {
                        kind: PrimKind::RegionRestore,
                        dst: None,
                        args: vec![sp],
                    });
                    // Keep the ops between the consumer and the (removed)
                    // drop — they cannot reference `t` (occ == 3) and now run
                    // after the restore, i.e. outside the region.
                    for k in i + 2..drop_at {
                        out.push(prog.functions[fi].ops[k].clone());
                    }
                    clones_needed.extend(c);
                    i = drop_at + 1;
                    continue;
                }
            }
            out.push(prog.functions[fi].ops[i].clone());
            i += 1;
        }
        prog.functions[fi].ops = out;
    }

    // Pass 2: append the clones — drops removed (the frontier reset IS the
    // teardown), callee names remapped into the clone set. `Dup` keeps its
    // normal render (alias + rc_inc): the count is dead weight inside a
    // region (nothing frees), but a stray increment on a region block is a
    // harmless store — correctness never depends on stripping it.
    for name in &clones_needed {
        let Some(orig) = prog.functions.iter().find(|f| &f.name == name) else { continue };
        let mut clone = orig.clone();
        clone.name = rgn_name(name);
        clone.ops = clone
            .ops
            .into_iter()
            .filter(|op| !matches!(op, Op::Drop { .. } | Op::DropVariant { .. }))
            .map(|mut op| {
                if let Op::CallFn { name, .. } = &mut op {
                    if clones_needed.contains(name) {
                        *name = rgn_name(name);
                    }
                }
                op
            })
            .collect();
        prog.functions.push(clone);
    }
}
