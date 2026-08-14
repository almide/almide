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
//!                         (or, since result-family-from-type Phase 1, the
//!                         TAGGED form: … → TG = SH + (1 << 32) →
//!                         Store8(H+12, TG) — the @16 Err tag riding the
//!                         payload slot's high half; TG aliases SH here)
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
    let mut t = ResultWindowTracker::default();

    for op in ops {
        // ROLLBACK GUARD (#1100): if this op READS anything a pending window
        // swallowed and is not that window's own next recognized step, the
        // window's producer guess was wrong — restore its buffered ops before
        // processing, so nothing downstream ever references a dropped def.
        t.flush_windows_read_by(&op, &mut out);
        match &op {
            Op::ConstInt { dst, value } => {
                t.const_vals.insert(*dst, *value);
                out.push(op);
            }
            Op::Alloc {
                dst,
                init: Init::Str(_),
                ..
            } => {
                t.str_allocs.insert(*dst);
                out.push(op);
            }
            // Producer window opens: a len-1 DynListStr alloc.
            Op::Alloc {
                dst,
                init: Init::DynListStr { len },
                ..
            } if t.const_vals.get(len) == Some(&1) => {
                let dst = *dst;
                t.pending.insert(
                    dst,
                    Pending {
                        handle: None,
                        payload: None,
                        buffered: vec![op],
                        owned: BTreeSet::from([dst]),
                    },
                );
                // NOT emitted — replaced by the ResMake* at completion, or
                // flushed back verbatim if the window is abandoned (#1100).
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
                kind: PrimKind::Store { .. },
                dst: None,
                args,
            } if args.len() == 2 => {
                t.track_store(op, &mut out);
            }
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

    // A window still pending at function end never completed — restore it.
    t.flush_all(&mut out);
    sweep_dead_window_material(&mut out, f.ret);
    f.ops = out;
}

/// In-flight producers: R → (its Handle, stored payload, payload-was-str-handle).
///
/// A window is SPECULATIVE until it completes (#1100): every op it swallows is
/// buffered in `buffered` and every dst it swallows recorded in `owned`, so an
/// op that proves the guess wrong — a len-1 `DynListStr` that is NOT a Result
/// producer, e.g. the tuple-payload bind the matrix fixture lowers — flushes
/// the buffer back verbatim and the stream is exactly as if the window had
/// never opened. Completion discards the buffer. "Unrecognized never means
/// wrong" only holds with the rollback; eager dropping orphaned the window's
/// downstream loads.
struct Pending {
    handle: Option<ValueId>,
    payload: Option<(ValueId, bool)>,
    buffered: Vec<Op>,
    owned: BTreeSet<ValueId>,
}

/// Running knowledge of the window recognizer, all keyed by ValueId.
#[derive(Default)]
struct ResultWindowTracker {
    const_vals: BTreeMap<ValueId, i64>,
    str_allocs: BTreeSet<ValueId>,
    res_vals: BTreeSet<ValueId>,
    /// Handle over a res value → the res value.
    res_handles: BTreeMap<ValueId, ValueId>,
    /// Handle over an owned Str alloc → the Str value.
    str_handles: BTreeMap<ValueId, ValueId>,
    /// Address value → (res value, byte offset).
    res_addrs: BTreeMap<ValueId, (ValueId, i64)>,
    pending: BTreeMap<ValueId, Pending>,
    /// Producer-window handles → their R.
    pending_handles: BTreeMap<ValueId, ValueId>,
    /// Addresses inside producer windows → (R, offset).
    pending_addrs: BTreeMap<ValueId, (ValueId, i64)>,
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
        if let Some(p) = self.pending.get_mut(&a) {
            p.handle = Some(d);
            p.buffered.push(op);
            p.owned.insert(d);
            self.pending_handles.insert(d, a);
        } else if self.res_vals.contains(&a) {
            self.res_handles.insert(d, a);
        } else if self.str_allocs.contains(&a) {
            self.str_handles.insert(d, a);
            out.push(op);
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
        if let (Some(r), Some(o)) = (self.pending_handles.get(&a).copied(), off) {
            self.pending_addrs.insert(dst, (r, o));
            if let Some(p) = self.pending.get_mut(&r) {
                p.buffered.push(op);
                p.owned.insert(dst);
            }
        } else if let (Some(r), Some(o)) = (self.res_handles.get(&a).copied(), off) {
            self.res_addrs.insert(dst, (r, o));
        } else {
            // The TAGGED err payload (`materialize_result_err_str`,
            // result-family-from-type Phase 1): `SH + (1 << 32)` packs the @16
            // Err tag into the high half of the payload's own 8-byte slot
            // store. The tagged value is STILL a handle to the same owned
            // Str — alias it so the following Store8 completes the Err window
            // exactly as the untagged form always has (the native carrier
            // encodes err-ness structurally, so the tag bits are dead material
            // the completion sweep removes). Emitted like the SH it derives
            // from, so an abandoned window stays well-defined.
            if let (Some(s), Some(4294967296)) = (self.str_handles.get(&a).copied(), off) {
                self.str_handles.insert(dst, s);
            }
            out.push(op);
        }
    }

    /// The `Store` arm: a store through a producer-window address records the
    /// payload (offset 12, width 8) or completes the Ok producer (the len:=0
    /// tag store at offset 4). Any other store passes through.
    fn track_store(&mut self, op: Op, out: &mut Vec<Op>) {
        let Op::Prim {
            kind: PrimKind::Store { width },
            dst: None,
            args,
        } = &op
        else {
            unreachable!("caller matched the store prim")
        };
        let (width, addr, stored) = (*width, args[0], args[1]);
        match self.pending_addrs.get(&addr).copied() {
            Some((r, 12)) if width == 8 => self.store_producer_payload(r, stored, op, out),
            Some((r, 4)) if width == 4 && self.const_vals.get(&stored) == Some(&0) => {
                self.complete_ok_producer(r, out);
            }
            _ => out.push(op),
        }
    }

    /// The offset-12 payload store. An Err(str) completes HERE (len stays 1 =
    /// Err tag); Ok(scalar) completes at the len:=0 store.
    fn store_producer_payload(&mut self, r: ValueId, stored: ValueId, op: Op, out: &mut Vec<Op>) {
        let (payload, is_str) = match self.str_handles.get(&stored) {
            Some(s) => (*s, true),
            None => (stored, false),
        };
        if let Some(p) = self.pending.get_mut(&r) {
            p.payload = Some((payload, is_str));
            if !is_str {
                // Not a completion yet — the Ok tag store may still arrive, or
                // the window may be abandoned; either way the op must survive
                // in the buffer (#1100).
                p.buffered.push(op);
                return;
            }
        } else {
            return;
        }
        let Some(p) = self.remove_window(r) else {
            return;
        };
        let _ = p; // Err completes: the buffered window ops are REPLACED.
        out.push(Op::Prim {
            kind: PrimKind::ResMakeErrStr,
            dst: Some(r),
            args: vec![payload],
        });
        self.res_vals.insert(r);
    }

    /// The Ok tag store (len := 0): complete the Ok(scalar) producer. A shape
    /// outside the recognized set (a str payload with an Ok tag, no payload at
    /// all) FLUSHES the buffered window back instead — re-emitting is possible
    /// now, so R is never left undefined (#1100).
    fn complete_ok_producer(&mut self, r: ValueId, out: &mut Vec<Op>) {
        let Some(p) = self.remove_window(r) else {
            return;
        };
        match p.payload {
            Some((payload, false)) => {
                out.push(Op::Prim {
                    kind: PrimKind::ResMakeOk,
                    dst: Some(r),
                    args: vec![payload],
                });
                self.res_vals.insert(r);
            }
            _ => out.extend(p.buffered),
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

    // ── the #1100 rollback machinery ────────────────────────────────────

    /// Remove window `r` from every tracker map, returning its state.
    fn remove_window(&mut self, r: ValueId) -> Option<Pending> {
        let p = self.pending.remove(&r)?;
        self.pending_handles.retain(|_, pr| *pr != r);
        self.pending_addrs.retain(|_, (pr, _)| *pr != r);
        Some(p)
    }

    /// Restore window `r`'s buffered ops verbatim — the stream continues
    /// exactly as if the producer guess had never been made.
    fn flush_window(&mut self, r: ValueId, out: &mut Vec<Op>) {
        if let Some(p) = self.remove_window(r) {
            out.extend(p.buffered);
        }
    }

    /// Flush every window still pending (function end — no completion came).
    fn flush_all(&mut self, out: &mut Vec<Op>) {
        let rs: Vec<ValueId> = self.pending.keys().copied().collect();
        for r in rs {
            self.flush_window(r, out);
        }
    }

    /// The rollback guard: flush any pending window whose swallowed material
    /// this op READS, unless the op is that window's own next recognized step
    /// (which reads window material by design — the Handle over R, the offset
    /// Add over the window handle, and the two recognized Store forms).
    fn flush_windows_read_by(&mut self, op: &Op, out: &mut Vec<Op>) {
        if self.pending.is_empty() {
            return;
        }
        if self.is_window_continuation(op) {
            return;
        }
        let mut reads = BTreeSet::new();
        collect_reads(op, &mut reads);
        if reads.is_empty() {
            return;
        }
        let hit: Vec<ValueId> = self
            .pending
            .iter()
            .filter(|(_, p)| p.owned.iter().any(|v| reads.contains(v)))
            .map(|(r, _)| *r)
            .collect();
        for r in hit {
            self.flush_window(r, out);
        }
    }

    /// Is this op one of the shapes the window recognizer itself consumes?
    /// Only those may read window material without abandoning the window.
    fn is_window_continuation(&self, op: &Op) -> bool {
        match op {
            Op::Prim {
                kind: PrimKind::Handle,
                dst: Some(_),
                args,
            } if args.len() == 1 => self.pending.contains_key(&args[0]),
            Op::IntBinOp {
                op: crate::IntOp::Add,
                a,
                b,
                ..
            } => self.pending_handles.contains_key(a) && self.const_vals.contains_key(b),
            Op::Prim {
                kind: PrimKind::Store { width },
                dst: None,
                args,
            } if args.len() == 2 => match self.pending_addrs.get(&args[0]).copied() {
                Some((_, 12)) => *width == 8,
                Some((_, 4)) => *width == 4 && self.const_vals.get(&args[1]) == Some(&0),
                _ => false,
            },
            _ => false,
        }
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
            Init::DynStr { len } | Init::DynList { len } | Init::DynListStr { len } => {
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
        Op::Else { val } | Op::EndIf { val } => used.extend(val.iter().copied()),
        Op::LoopStart | Op::LoopEnd | Op::Charge { .. } => {}
        Op::ChargeDyn { src, .. } => {
            used.insert(*src);
        }
    }
}
