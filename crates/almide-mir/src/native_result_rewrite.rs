//! NATIVE-ONLY Result carrier rewrite (T1-3).
//!
//! PRODUCERS are semantic since the result-family-from-type "desugar once"
//! slice: the materializers emit `Alloc { init: ResOkScalar }` /
//! `Alloc { init: ResErrStr }` (one op each), and this pass maps them 1:1 onto
//! `PrimKind::ResMakeOk` / `ResMakeErrStr` — a TOTAL single-op match. The
//! producer-window recognizer this replaced (a 5–6-op speculative pattern
//! match over ConstInt/Alloc/Handle/Store sequences, with #1100 buffering and
//! rollback) is GONE: it broke whenever a materializer's op sequence changed
//! shape (the 2026-08-14 fuel-lane wall was exactly that), which was the
//! definition of the disease. A materializer that has not been converted to a
//! semantic init keeps emitting raw block ops; they pass through untouched and
//! WALL at the native render exactly as an abandoned window always did —
//! unrecognized never means wrong.
//!
//! CONSUMERS are still recognized syntactically (small, stable shapes the
//! match lowering emits over an already-tracked res value):
//!
//!   consumer tag:         Handle H(r) → Load4(H+4)
//!   consumer ok payload:  Load8(H+12)
//!   consumer err payload: LoadHandle(H+12)
//!
//! rewritten to `ResTag` / `ResOkScalar` / `ResErrStr`, which the native
//! renderer maps onto a Rust `Result<i64, String>` local (`NTy::Res`).
//!
//! This pass runs ONLY on the native leg, after lowering and before
//! verification/render. Ownership accounting is preserved: the Err STRING's
//! `Alloc`/`Consume` pair stays (the carrier clones; `Init::ResErrStr`'s move
//! contract keeps the same `i…m` shape on both legs); the semantic Alloc
//! itself becomes the prim (the carrier value is scalar-like to the verifier);
//! a `CallFn` heap result keeps its `Some(Ptr)` birth and its `DropListStr`
//! release.
//!
//! The wasm leg never sees any of this — zero risk to the proven leg.

use crate::{Init, MirFunction, Op, PrimKind, ValueId};
use std::collections::{BTreeMap, BTreeSet};

/// Rewrite every recognized Result window in `f`. `result_fns` names the
/// functions whose (declared) return is a native-carrier Result — their
/// `CallFn` dsts seed the consumer-side value tracking.
pub fn rewrite_result_ops(f: &mut MirFunction, result_fns: &BTreeSet<String>) {
    let ops = std::mem::take(&mut f.ops);
    let mut out: Vec<Op> = Vec::with_capacity(ops.len());
    let mut t = ResultWindowTracker::default();

    for op in ops {
        match &op {
            Op::ConstInt { dst, value } => {
                t.const_vals.insert(*dst, *value);
                out.push(op);
            }
            // The SEMANTIC Result inits (result-family-from-type, "desugar once"):
            // a total single-op match — no window, no speculation, nothing to
            // abandon. `Alloc{ResOkScalar}` IS `ResMakeOk`, `Alloc{ResErrStr}` IS
            // `ResMakeErrStr` (whose native semantics were always borrow-contract:
            // the piece's own accounting untouched — the carrier clones). This is
            // what retires the producer-window recognition below for every
            // materializer that emits the semantic form.
            Op::Alloc { dst, init: Init::ResOkScalar { payload }, .. } => {
                let (dst, payload) = (*dst, *payload);
                out.push(Op::Prim {
                    kind: PrimKind::ResMakeOk,
                    dst: Some(dst),
                    args: vec![payload],
                });
                t.res_vals.insert(dst);
            }
            Op::Alloc { dst, init: Init::ResErrStr { piece }, .. } => {
                let (dst, piece) = (*dst, *piece);
                out.push(Op::Prim {
                    kind: PrimKind::ResMakeErrStr,
                    dst: Some(dst),
                    args: vec![piece],
                });
                t.res_vals.insert(dst);
            }
            Op::Prim {
                kind: PrimKind::Handle,
                dst: Some(_),
                args,
            } if args.len() == 1 => {
                t.track_handle(op, &mut out);
            }
            Op::IntBinOp {
                op: crate::IntOp::Add,
                ..
            } => t.track_addr_add(op, &mut out),
            Op::Prim {
                kind: PrimKind::Load { .. } | PrimKind::LoadHandle,
                dst: Some(_),
                args,
            } if args.len() == 1 && t.res_addrs.contains_key(&args[0]) => {
                t.track_res_load(op, &mut out);
            }
            // A Consume of a rewritten result value: the carrier is
            // scalar-like to the verifier (no object) — drop the op.
            Op::Consume { v } if t.res_vals.contains(v) => {}
            Op::CallFn {
                dst: Some(d), name, ..
            } if result_fns.contains(name) => {
                t.res_vals.insert(*d);
                out.push(op);
            }
            // Res-ness propagates through if-value joins.
            Op::IfThen { dst, .. } => {
                t.if_dsts.push(*dst);
                out.push(op);
            }
            Op::Else { .. } | Op::EndIf { .. } => t.track_if_join(op, &mut out),
            _ => out.push(op),
        }
    }

    sweep_dead_window_material(&mut out, f.ret);
    f.ops = out;
}

/// Running knowledge of the window recognizer, all keyed by ValueId.
#[derive(Default)]
struct ResultWindowTracker {
    const_vals: BTreeMap<ValueId, i64>,
    res_vals: BTreeSet<ValueId>,
    /// Handle over a res value → the res value.
    res_handles: BTreeMap<ValueId, ValueId>,
    /// Address value → (res value, byte offset).
    res_addrs: BTreeMap<ValueId, (ValueId, i64)>,
    /// if-value stack: dst of each open IfThen (for res propagation to joins).
    if_dsts: Vec<Option<ValueId>>,
}

impl ResultWindowTracker {
    /// The `Handle` arm: classify what the handle points at. Pending/res
    /// handles are NOT emitted (pure address material). A str handle IS
    /// emitted — if it feeds a recognized store it dies with the window;
    /// otherwise the sweep keeps it and the render walls honestly as before.
    fn track_handle(&mut self, op: Op, out: &mut Vec<Op>) {
        let Op::Prim {
            kind: PrimKind::Handle,
            dst: Some(d),
            args,
        } = &op
        else {
            unreachable!("caller matched the handle prim")
        };
        let (d, a) = (*d, args[0]);
        if self.res_vals.contains(&a) {
            self.res_handles.insert(d, a);
        } else {
            out.push(op);
        }
    }

    /// The `Add` arm: an offset add over a window/res handle becomes address
    /// knowledge (not emitted); anything else passes through.
    fn track_addr_add(&mut self, op: Op, out: &mut Vec<Op>) {
        let Op::IntBinOp {
            dst,
            op: crate::IntOp::Add,
            a,
            b,
        } = &op
        else {
            unreachable!("caller matched the add")
        };
        let (dst, a) = (*dst, *a);
        let off = self.const_vals.get(b).copied();
        if let (Some(r), Some(o)) = (self.res_handles.get(&a).copied(), off) {
            self.res_addrs.insert(dst, (r, o));
        } else {
            out.push(op);
        }
    }

    /// A load through a rewritten-result address: the tag (Load4 @+4), the ok
    /// payload (Load8 @+12), or the err payload (LoadHandle @+12). Any other
    /// width/offset combination passes through untouched.
    fn track_res_load(&mut self, op: Op, out: &mut Vec<Op>) {
        let Op::Prim {
            kind,
            dst: Some(d),
            args,
        } = &op
        else {
            unreachable!("caller matched the load prims")
        };
        let (r, off) = self.res_addrs[&args[0]];
        let res_kind = match (kind, off) {
            (PrimKind::Load { width: 4 }, 4) => Some(PrimKind::ResTag),
            (PrimKind::Load { width: 8 }, 12) => Some(PrimKind::ResOkScalar),
            (PrimKind::LoadHandle, 12) => Some(PrimKind::ResErrStr),
            _ => None,
        };
        let d = *d;
        match res_kind {
            Some(kind) => out.push(Op::Prim {
                kind,
                dst: Some(d),
                args: vec![r],
            }),
            None => out.push(op),
        }
    }

    /// The `Else`/`EndIf` arm: propagate res-ness from an arm value to the
    /// join dst, and close the if-value scope on `EndIf`.
    fn track_if_join(&mut self, op: Op, out: &mut Vec<Op>) {
        let (Op::Else { val } | Op::EndIf { val }) = &op else {
            unreachable!("caller matched the join ops")
        };
        if let (Some(v), Some(Some(d))) = (val, self.if_dsts.last()) {
            if self.res_vals.contains(v) {
                self.res_vals.insert(*d);
            }
        }
        if matches!(op, Op::EndIf { .. }) {
            self.if_dsts.pop();
        }
        out.push(op);
    }

}

/// Dead-op sweep: the recognized windows orphaned their ConstInt feeders
/// (and any Handle whose only use was a rewritten window). Iterate to a
/// fixpoint so chains (ConstInt → Add → Handle) fully disappear — a live
/// program value is never touched (only PURE ops are candidates, and the
/// read set of [`collect_reads`] is COMPLETE over the Op grammar).
fn sweep_dead_window_material(out: &mut Vec<Op>, ret: Option<ValueId>) {
    loop {
        let mut used: BTreeSet<ValueId> = BTreeSet::new();
        for op in out.iter() {
            collect_reads(op, &mut used);
        }
        if let Some(r) = ret {
            used.insert(r);
        }
        let before = out.len();
        out.retain(|op| match op {
            Op::ConstInt { dst, .. } => used.contains(dst),
            Op::IntBinOp { dst, .. } => used.contains(dst),
            Op::Prim {
                kind: PrimKind::Handle,
                dst: Some(d),
                ..
            } => used.contains(d),
            _ => true,
        });
        if out.len() == before {
            break;
        }
    }
}

/// The Alloc-init reads, exhaustive over Init for the same no-wildcard
/// reason — split from `collect_reads` for the complexity budget.
fn alloc_reads(init: &Init, used: &mut BTreeSet<ValueId>) {
    match init {
        Init::DynStr { len } | Init::DynList { len } | Init::DynListStr { len } => {
            used.insert(*len);
        }
        Init::OptSome { payload } | Init::ResOkScalar { payload } => {
            used.insert(*payload);
        }
        Init::ResErrStr { piece } => {
            used.insert(*piece);
        }
        Init::Opaque
        | Init::Empty
        | Init::IntList(_)
        | Init::Bytes(_)
        | Init::Str(_)
        | Init::OptNone => {}
    }
}

/// EVERY ValueId an op reads — exhaustive over the Op grammar (a miss here
/// could sweep a live ConstInt, so no wildcard arm for value-carrying ops).
fn collect_reads(op: &Op, used: &mut BTreeSet<ValueId>) {
    use crate::CallArg;
    fn call_args(args: &[CallArg], used: &mut BTreeSet<ValueId>) {
        for a in args {
            match a {
                CallArg::Handle(v) | CallArg::Scalar(v) => {
                    used.insert(*v);
                }
                _ => {}
            }
        }
    }
    match op {
        Op::Alloc { init, .. } => alloc_reads(init, used),
        Op::Const { .. } | Op::ConstInt { .. } | Op::FuncRef { .. } => {}
        Op::Dup { src, .. } | Op::SetLocal { src, .. } => {
            used.insert(*src);
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
        | Op::MakeUnique { v } => {
            used.insert(*v);
        }
        Op::Pure { uses, .. } => used.extend(uses.iter().copied()),
        Op::Call { args, .. } | Op::CallFn { args, .. } | Op::CallImport { args, .. } => {
            call_args(args, used)
        }
        Op::CallIndirect {
            table_idx, args, ..
        } => {
            used.insert(*table_idx);
            call_args(args, used);
        }
        Op::ListLit { elems, .. } => used.extend(elems.iter().copied()),
        Op::ListGetScalar { list, idx, .. } => {
            used.insert(*list);
            used.insert(*idx);
        }
        Op::ListSetScalar { list, idx, val } => {
            used.insert(*list);
            used.insert(*idx);
            used.insert(*val);
        }
        Op::IntBinOp { a, b, .. } => {
            used.insert(*a);
            used.insert(*b);
        }
        Op::Prim { args, .. } => used.extend(args.iter().copied()),
        Op::IfThen { cond, .. } | Op::LoopBreakUnless { cond } => {
            used.insert(*cond);
        }
        Op::Else { val } | Op::EndIf { val } | Op::Return { val } => {
            used.extend(val.iter().copied())
        }
        Op::LoopStart | Op::LoopEnd | Op::Charge { .. } => {}
        Op::ChargeDyn { src, .. } => {
            used.insert(*src);
        }
    }
}
