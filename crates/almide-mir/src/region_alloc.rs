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

/// Per-function single-def map (dst → op index) and SSA-const map (ConstInt
/// dsts never reassigned) — the SAME discipline as Fuser::scan_consts.
fn fn_tables(f: &MirFunction) -> (BTreeMap<ValueId, usize>, BTreeMap<ValueId, i64>) {
    let mut def: BTreeMap<ValueId, usize> = BTreeMap::new();
    let mut multi: BTreeSet<ValueId> = BTreeSet::new();
    let mut consts: BTreeMap<ValueId, i64> = BTreeMap::new();
    for (i, op) in f.ops.iter().enumerate() {
        if let Some(d) = crate::render_wasm::defined_value(op) {
            if def.insert(d, i).is_some() {
                multi.insert(d);
            }
        }
        if let Op::ConstInt { dst, value } = op {
            consts.insert(*dst, *value);
        }
        if let Op::SetLocal { local, .. } = op {
            multi.insert(*local);
            consts.remove(local);
        }
    }
    for m in &multi {
        def.remove(m);
        consts.remove(m);
    }
    (def, consts)
}

/// SINGLETON qualification for one function: every `Prim::Store` address
/// chain must root at a fresh non-`ListLit` alloc (so no store can ever hit a
/// `ListLit` block), and there must be no `ListSetScalar` (same reason). When
/// this holds for the whole closure, an all-const `ListLit` block is
/// immutable for its region lifetime and every instance can be ONE shared
/// block built once at region entry (binarytrees' `Leaf`: half of all nodes).
fn stores_root_at_fresh_allocs(f: &MirFunction) -> bool {
    let (def, consts) = fn_tables(f);
    for op in &f.ops {
        if matches!(op, Op::ListSetScalar { .. }) {
            return false;
        }
        let Op::Prim { kind: PrimKind::Store { .. }, args, .. } = op else { continue };
        let mut v = args[0];
        let mut ok = false;
        for _ in 0..8 {
            let Some(&d) = def.get(&v) else { break };
            match &f.ops[d] {
                Op::IntBinOp { op: crate::IntOp::Add, a, b, .. } => {
                    if consts.contains_key(b) {
                        v = *a;
                    } else if consts.contains_key(a) {
                        v = *b;
                    } else {
                        break;
                    }
                }
                Op::Prim { kind: PrimKind::Handle, args: hargs, .. } => {
                    let Some(&hd) = def.get(&hargs[0]) else { break };
                    ok = matches!(&f.ops[hd], Op::Alloc { .. });
                    break;
                }
                _ => break,
            }
        }
        if !ok {
            return false;
        }
    }
    true
}

/// The distinct all-const `ListLit` element vectors of a function.
fn const_listlit_shapes(f: &MirFunction) -> Vec<Vec<i64>> {
    let (_, consts) = fn_tables(f);
    let mut shapes: Vec<Vec<i64>> = Vec::new();
    for op in &f.ops {
        let Op::ListLit { elems, .. } = op else { continue };
        let vals: Option<Vec<i64>> = elems.iter().map(|e| consts.get(e).copied()).collect();
        if let Some(v) = vals {
            if !shapes.contains(&v) {
                shapes.push(v);
            }
        }
    }
    shapes
}

struct RegionWindow {
    fi: usize,
    start: usize,
    drop_at: usize,
    closure: BTreeSet<String>,
    shapes: Vec<Vec<i64>>,
}

/// Rewrite every qualifying window and append the `__rgn_` clones. Applied to
/// the wasm leg only, after pipeline verification, before the reachability
/// prune (the prune scans rendered text, so the clone calls keep them live).
pub fn apply_region_specialization(prog: &mut MirProgram) {
    let mg_lo = crate::MG_SLOT_BASE as i64;
    let mg_hi = mg_lo + 8 * prog.mutable_global_count as i64;

    // Pass A: collect qualifying windows (no mutation yet).
    let mut windows: Vec<RegionWindow> = Vec::new();
    let mut qualified_cache: BTreeMap<(String, String), Option<(BTreeSet<String>, Vec<Vec<i64>>)>> =
        BTreeMap::new();
    for (fi, func) in prog.functions.iter().enumerate() {
        if func.name.starts_with("__rgn_") {
            continue;
        }
        let mut occ: BTreeMap<ValueId, usize> = BTreeMap::new();
        let mut vals: Vec<ValueId> = Vec::new();
        for op in &func.ops {
            vals.clear();
            crate::render_wasm::op_values(op, &mut vals);
            for v in &vals {
                *occ.entry(*v).or_insert(0) += 1;
            }
        }
        let mut i = 0;
        while i + 3 <= func.ops.len() {
            let Some((_t, f, g, drop_at)) = match_region_window(&func.ops, i, &occ) else {
                i += 1;
                continue;
            };
            let key = (f.clone(), g.clone());
            let entry = qualified_cache.entry(key).or_insert_with(|| {
                let c = callee_closure(prog, [f.as_str(), g.as_str()])?;
                let members: Vec<&MirFunction> = c
                    .iter()
                    .filter_map(|n| prog.functions.iter().find(|h| &h.name == n))
                    .collect();
                if members.len() != c.len() {
                    return None;
                }
                if !members
                    .iter()
                    .all(|h| h.ops.iter().all(|op| region_safe_op(op, mg_lo, mg_hi, &c)))
                {
                    return None;
                }
                // Singleton shapes: only when every member's stores provably
                // avoid ListLit blocks; capped so the extra clone params stay
                // register-friendly. Failing the bar disables the singleton
                // (empty shapes), never the region itself.
                let mut shapes: Vec<Vec<i64>> = Vec::new();
                if members.iter().all(|h| stores_root_at_fresh_allocs(h)) {
                    for h in &members {
                        for s in const_listlit_shapes(h) {
                            if !shapes.contains(&s) {
                                shapes.push(s);
                            }
                        }
                    }
                    if shapes.len() > 2 {
                        shapes.clear();
                    }
                }
                Some((c, shapes))
            });
            if let Some((c, shapes)) = entry.clone() {
                windows.push(RegionWindow { fi, start: i, drop_at, closure: c, shapes });
                i = drop_at + 1;
            } else {
                i += 1;
            }
        }
    }
    if windows.is_empty() {
        return;
    }

    // Consolidate: a clone is generated ONCE per name, so every closure that
    // shares a member must agree on the singleton shape vector (it changes
    // the clone's arity). Disagreement disables singletons everywhere —
    // conservative and rare.
    {
        let mut by_name: BTreeMap<&str, &Vec<Vec<i64>>> = BTreeMap::new();
        let mut conflict = false;
        for w in &windows {
            for n in &w.closure {
                match by_name.get(n.as_str()) {
                    Some(existing) if *existing != &w.shapes => {
                        conflict = true;
                    }
                    _ => {
                        by_name.insert(n, &w.shapes);
                    }
                }
            }
        }
        if conflict {
            for w in &mut windows {
                w.shapes.clear();
            }
        }
    }

    // Pass B1: rewrite the windows, per function (descending op index so the
    // recorded positions stay valid).
    let mut clones_needed: BTreeMap<String, Vec<Vec<i64>>> = BTreeMap::new();
    for w in windows.iter().rev() {
        for n in &w.closure {
            clones_needed.entry(n.clone()).or_insert_with(|| w.shapes.clone());
        }
        let func = &mut prog.functions[w.fi];
        let mut max_id: u32 = 0;
        let mut vals: Vec<ValueId> = Vec::new();
        for op in &func.ops {
            vals.clear();
            crate::render_wasm::op_values(op, &mut vals);
            for v in &vals {
                max_id = max_id.max(v.0);
            }
        }
        for p in &func.params {
            max_id = max_id.max(p.value.0);
        }
        let mut seq: Vec<Op> = Vec::new();
        max_id += 1;
        let sp = ValueId(max_id);
        seq.push(Op::Prim { kind: PrimKind::RegionSave, dst: Some(sp), args: vec![] });
        // Build each singleton ONCE inside the region; the clones receive its
        // handle as a trailing param and alias it per instance.
        let mut singleton_ids: Vec<ValueId> = Vec::new();
        for shape in &w.shapes {
            let mut elem_ids = Vec::with_capacity(shape.len());
            for value in shape {
                max_id += 1;
                let c = ValueId(max_id);
                seq.push(Op::ConstInt { dst: c, value: *value });
                elem_ids.push(c);
            }
            max_id += 1;
            let s = ValueId(max_id);
            seq.push(Op::ListLit { dst: s, elems: elem_ids });
            singleton_ids.push(s);
        }
        let mut fc = func.ops[w.start].clone();
        let mut gc = func.ops[w.start + 1].clone();
        for call in [&mut fc, &mut gc] {
            if let Op::CallFn { name, args, .. } = call {
                *name = rgn_name(name);
                for s in &singleton_ids {
                    args.push(CallArg::Handle(*s));
                }
            }
        }
        seq.push(fc);
        seq.push(gc);
        seq.push(Op::Prim { kind: PrimKind::RegionRestore, dst: None, args: vec![sp] });
        // Keep the ops between the consumer and the (removed) drop — they
        // cannot reference `t` (occ == 3) and now run after the restore.
        for k in w.start + 2..w.drop_at {
            seq.push(func.ops[k].clone());
        }
        func.ops.splice(w.start..=w.drop_at, seq);
    }

    // Pass B2: append the clones — drops removed (the frontier reset IS the
    // teardown), callee names remapped into the clone set, singleton params
    // appended and threaded through every internal closure call. `Dup` keeps
    // its normal render (alias + rc_inc): the count is dead weight inside a
    // region (nothing frees), and a stray increment is a harmless store.
    for (name, shapes) in &clones_needed {
        let Some(orig) = prog.functions.iter().find(|f| &f.name == name) else { continue };
        let mut clone = orig.clone();
        clone.name = rgn_name(name);
        let mut max_id: u32 = 0;
        let mut vals: Vec<ValueId> = Vec::new();
        for op in &clone.ops {
            vals.clear();
            crate::render_wasm::op_values(op, &mut vals);
            for v in &vals {
                max_id = max_id.max(v.0);
            }
        }
        for p in &clone.params {
            max_id = max_id.max(p.value.0);
        }
        let mut singleton_params: Vec<ValueId> = Vec::new();
        for _ in shapes {
            max_id += 1;
            let p = ValueId(max_id);
            clone.params.push(crate::MirParam {
                value: p,
                repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            });
            singleton_params.push(p);
        }
        let (_, consts) = fn_tables(orig);
        clone.ops = clone
            .ops
            .into_iter()
            .filter(|op| !matches!(op, Op::Drop { .. } | Op::DropVariant { .. }))
            .map(|op| {
                if let Op::ListLit { dst, elems } = &op {
                    let vals: Option<Vec<i64>> =
                        elems.iter().map(|e| consts.get(e).copied()).collect();
                    if let Some(v) = vals {
                        if let Some(k) = shapes.iter().position(|s| s == &v) {
                            // Every instance of this immutable all-const
                            // block ALIASES the one built at region entry.
                            return Op::Dup { dst: *dst, src: singleton_params[k] };
                        }
                    }
                }
                let mut op = op;
                if let Op::CallFn { name, args, .. } = &mut op {
                    if clones_needed.contains_key(name) {
                        *name = rgn_name(name);
                        for p in &singleton_params {
                            args.push(CallArg::Handle(*p));
                        }
                    }
                }
                op
            })
            .collect();
        prog.functions.push(clone);
    }
}
