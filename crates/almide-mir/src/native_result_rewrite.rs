//! NATIVE-ONLY Result carrier rewrite (T1-3).
//!
//! The shared MIR lowering materializes `Result[T, String]` as a raw memory
//! block (`Alloc DynListStr` + `Handle` + address arithmetic + `Store`/`Load`
//! prims at fixed offsets — see `result_ctors.rs` / the `materialize_result_*`
//! family). The wasm renderer executes that block model over linear memory;
//! the native renderer HAS NO memory model, so every Result op walled.
//!
//! This pass runs ONLY on the native leg, after lowering and before
//! verification/render, and recognizes the stereotyped windows:
//!
//!   producer Ok(scalar):  ConstInt 1 → Alloc R DynListStr → Handle H →
//!                         Store8(H+12, p) → Store4(H+4, 0)
//!   producer Err(str):    Alloc S Str(..) → ConstInt 1 → Alloc R DynListStr →
//!                         Handle H → Handle SH(S) → Store8(H+12, SH)
//!   consumer tag:         Handle H(r) → Load4(H+4)
//!   consumer ok payload:  Load8(H+12)
//!   consumer err payload: LoadHandle(H+12)
//!
//! and rewrites them to the five `PrimKind::Res*` carrier prims the native
//! renderer maps onto a Rust `Result<i64, String>` local. Everything it does
//! is per-function and syntactic; a window it does not recognize is simply
//! left alone, and the untouched block ops then WALL exactly as before —
//! unrecognized never means wrong.
//!
//! Ownership accounting is preserved: the `Alloc`/`Consume` pair of the Err
//! STRING stays; the result block's own `Alloc`+`Consume` disappear together
//! (the carrier value is scalar-like to the verifier); a `CallFn` heap result
//! keeps its `Some(Ptr)` birth and its `DropListStr` release (the drop renders
//! as scope-end for `NTy::Res` like every native drop).
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

    // Running knowledge, all keyed by ValueId.
    let mut const_vals: BTreeMap<ValueId, i64> = BTreeMap::new();
    let mut str_allocs: BTreeSet<ValueId> = BTreeSet::new();
    let mut res_vals: BTreeSet<ValueId> = BTreeSet::new();
    // Handle over a res value → the res value.
    let mut res_handles: BTreeMap<ValueId, ValueId> = BTreeMap::new();
    // Handle over an owned Str alloc → the Str value.
    let mut str_handles: BTreeMap<ValueId, ValueId> = BTreeMap::new();
    // Address value → (res value, byte offset).
    let mut res_addrs: BTreeMap<ValueId, (ValueId, i64)> = BTreeMap::new();
    // In-flight producers: R → (its Handle, stored payload, payload-was-str-handle).
    struct Pending {
        handle: Option<ValueId>,
        payload: Option<(ValueId, bool)>,
    }
    let mut pending: BTreeMap<ValueId, Pending> = BTreeMap::new();
    // Producer-window handles → their R.
    let mut pending_handles: BTreeMap<ValueId, ValueId> = BTreeMap::new();
    // Addresses inside producer windows → (R, offset).
    let mut pending_addrs: BTreeMap<ValueId, (ValueId, i64)> = BTreeMap::new();
    // if-value stack: dst of each open IfThen (for res propagation to joins).
    let mut if_dsts: Vec<Option<ValueId>> = Vec::new();

    for op in ops {
        match &op {
            Op::ConstInt { dst, value } => {
                const_vals.insert(*dst, *value);
                out.push(op);
            }
            Op::Alloc { dst, init: Init::Str(_), .. } => {
                str_allocs.insert(*dst);
                out.push(op);
            }
            // Producer window opens: a len-1 DynListStr alloc.
            Op::Alloc { dst, init: Init::DynListStr { len }, .. }
                if const_vals.get(len) == Some(&1) =>
            {
                pending.insert(*dst, Pending { handle: None, payload: None });
                // NOT emitted — replaced by the ResMake* at window completion.
            }
            Op::Prim { kind: PrimKind::Handle, dst: Some(d), args } if args.len() == 1 => {
                let a = args[0];
                if let Some(p) = pending.get_mut(&a) {
                    p.handle = Some(*d);
                    pending_handles.insert(*d, a);
                } else if res_vals.contains(&a) {
                    res_handles.insert(*d, a);
                } else if str_allocs.contains(&a) {
                    str_handles.insert(*d, a);
                    // Emitted nothing yet — if this handle feeds a recognized
                    // store it dies with the window; otherwise the sweep keeps
                    // it and the render walls honestly as before.
                    out.push(op);
                    continue;
                } else {
                    out.push(op);
                }
                // pending/res handles are NOT emitted (pure address material).
            }
            Op::IntBinOp { dst, op: crate::IntOp::Add, a, b } => {
                let off = const_vals.get(b).copied();
                if let (Some(r), Some(o)) = (pending_handles.get(a).copied(), off) {
                    pending_addrs.insert(*dst, (r, o));
                    // not emitted
                } else if let (Some(r), Some(o)) = (res_handles.get(a).copied(), off) {
                    res_addrs.insert(*dst, (r, o));
                    // not emitted
                } else {
                    out.push(op);
                }
            }
            Op::Prim { kind: PrimKind::Store { width }, dst: None, args } if args.len() == 2 => {
                match pending_addrs.get(&args[0]).copied() {
                    Some((r, 12)) if *width == 8 => {
                        let (payload, is_str) = match str_handles.get(&args[1]) {
                            Some(s) => (*s, true),
                            None => (args[1], false),
                        };
                        if let Some(p) = pending.get_mut(&r) {
                            p.payload = Some((payload, is_str));
                        }
                        // Err(str) completes HERE (len stays 1 = Err tag).
                        if is_str {
                            if let Some(p) = pending.remove(&r) {
                                if let Some(h) = p.handle {
                                    pending_handles.remove(&h);
                                }
                                out.push(Op::Prim {
                                    kind: PrimKind::ResMakeErrStr,
                                    dst: Some(r),
                                    args: vec![payload],
                                });
                                res_vals.insert(r);
                            }
                        }
                        // Ok(scalar) completes at the len:=0 store below.
                    }
                    Some((r, 4)) if *width == 4 && const_vals.get(&args[1]) == Some(&0) => {
                        // Ok tag store: complete the Ok(scalar) producer.
                        if let Some(p) = pending.remove(&r) {
                            if let Some(h) = p.handle {
                                pending_handles.remove(&h);
                            }
                            if let Some((payload, false)) = p.payload {
                                out.push(Op::Prim {
                                    kind: PrimKind::ResMakeOk,
                                    dst: Some(r),
                                    args: vec![payload],
                                });
                                res_vals.insert(r);
                            }
                            // A str payload with an Ok tag is outside the
                            // recognized set — the window ops were already
                            // dropped, so re-emitting is impossible; leave R
                            // undefined and let the render wall on its use.
                        }
                    }
                    _ => out.push(op),
                }
            }
            Op::Prim { kind: PrimKind::Load { width: 4 }, dst: Some(d), args }
                if args.len() == 1 && matches!(res_addrs.get(&args[0]), Some((_, 4))) =>
            {
                let (r, _) = res_addrs[&args[0]];
                out.push(Op::Prim { kind: PrimKind::ResTag, dst: Some(*d), args: vec![r] });
            }
            Op::Prim { kind: PrimKind::Load { width: 8 }, dst: Some(d), args }
                if args.len() == 1 && matches!(res_addrs.get(&args[0]), Some((_, 12))) =>
            {
                let (r, _) = res_addrs[&args[0]];
                out.push(Op::Prim { kind: PrimKind::ResOkScalar, dst: Some(*d), args: vec![r] });
            }
            Op::Prim { kind: PrimKind::LoadHandle, dst: Some(d), args }
                if args.len() == 1 && matches!(res_addrs.get(&args[0]), Some((_, 12))) =>
            {
                let (r, _) = res_addrs[&args[0]];
                out.push(Op::Prim { kind: PrimKind::ResErrStr, dst: Some(*d), args: vec![r] });
            }
            // A Consume of a rewritten result value: the carrier is
            // scalar-like to the verifier (no object) — drop the op.
            Op::Consume { v } if res_vals.contains(v) => {}
            Op::CallFn { dst: Some(d), name, .. } if result_fns.contains(name) => {
                res_vals.insert(*d);
                out.push(op);
            }
            // Res-ness propagates through if-value joins.
            Op::IfThen { dst, .. } => {
                if_dsts.push(*dst);
                out.push(op);
            }
            Op::Else { val } | Op::EndIf { val } => {
                if let (Some(v), Some(Some(d))) = (val, if_dsts.last()) {
                    if res_vals.contains(v) {
                        res_vals.insert(*d);
                    }
                }
                if matches!(op, Op::EndIf { .. }) {
                    if_dsts.pop();
                }
                out.push(op);
            }
            _ => out.push(op),
        }
    }

    // Dead-op sweep: the recognized windows orphaned their ConstInt feeders
    // (and any Handle whose only use was a rewritten window). Iterate to a
    // fixpoint so chains (ConstInt → Add → Handle) fully disappear — a live
    // program value is never touched (only PURE ops are candidates, and the
    // read set below is COMPLETE over the Op grammar).
    loop {
        let mut used: BTreeSet<ValueId> = BTreeSet::new();
        for op in &out {
            collect_reads(op, &mut used);
        }
        if let Some(r) = f.ret {
            used.insert(r);
        }
        let before = out.len();
        out.retain(|op| match op {
            Op::ConstInt { dst, .. } => used.contains(dst),
            Op::IntBinOp { dst, .. } => used.contains(dst),
            Op::Prim { kind: PrimKind::Handle, dst: Some(d), .. } => used.contains(d),
            _ => true,
        });
        if out.len() == before {
            break;
        }
    }

    f.ops = out;
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
        Op::Alloc { init, .. } => match init {
            Init::DynStr { len }
            | Init::DynList { len }
            | Init::DynListStr { len } => {
                used.insert(*len);
            }
            Init::OptSome { payload } => {
                used.insert(*payload);
            }
            Init::Opaque
            | Init::Empty
            | Init::IntList(_)
            | Init::Bytes(_)
            | Init::Str(_)
            | Init::OptNone => {}
        },
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
        Op::CallIndirect { table_idx, args, .. } => {
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
        Op::Else { val } | Op::EndIf { val } => used.extend(val.iter().copied()),
        Op::LoopStart | Op::LoopEnd | Op::Charge { .. } => {}
        Op::ChargeDyn { src, .. } => {
            used.insert(*src);
        }
    }
}
