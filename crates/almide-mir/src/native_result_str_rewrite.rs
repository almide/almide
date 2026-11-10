//! NATIVE-ONLY String-Ok Result carrier rewrite (#4006, family C).
//!
//! `Result[String, String]` is a one-slot list block: the materializer
//! allocates `DynListStr { len: 1 }`, stores the payload String's handle at
//! +12 and the tag word at +16 (0 = ok, 1 = err); a consumer reads the tag
//! (`Load4(H+16)`) and the payload (`LoadHandle(H+12)`) — the same slot on
//! both sides. This pass maps those windows onto a Rust
//! `Result<AlmideString, AlmideString>` local (`NTy::ResStr`):
//!
//!   producer: `Store8(H+12, handle(piece))` → `ResStrMake [piece]`
//!             (`Ok(piece.clone())`), then `Store4(H+16, tag)` →
//!             `ResStrSetTag [r, tag]` (moves the payload to Err when tag ≠ 0);
//!   consumer: the tag read → `ResTag`, the payload read → `ResErrStr` (the
//!             borrow the verifier already aliases to the Result's object; the
//!             render reads either side's String for a `ResStr` operand).
//!
//! Which blocks are such Results comes from TYPES: a call to a fn declared
//! `-> Result[String, String]`, a param declared so, and a block whose window
//! stores a String payload — the render types the piece, and a non-String
//! piece walls there. Any other access to a tracked block is left unrewritten
//! and walls at the render. Ownership follows the Result carrier's: the
//! piece's own Alloc/Consume stays (the carrier clones it); a minted carrier
//! has its own alloc-class credit, keeps its transfer/release, and its `Dup`
//! becomes `ResCopy`. A call result keeps its heap birth and release too.
//!
//! The wasm leg never sees any of this.

use crate::{Init, MirFunction, Op, PrimKind, ValueId};
use std::collections::{BTreeMap, BTreeSet};

/// The one-slot blocks of `ops` whose window is a String-Ok Result's — a
/// `Store8` of a handle at +12 and a `Store4` at +16 (a one-element list has
/// no word at +16: its element fills +12..+20) — and the piece handles those
/// stores take.
fn recognize(ops: &[Op], skip: &BTreeSet<ValueId>) -> (BTreeSet<ValueId>, BTreeSet<ValueId>) {
    let mut consts: BTreeMap<ValueId, i64> = BTreeMap::new();
    let mut blocks: BTreeSet<ValueId> = BTreeSet::new();
    let mut handles: BTreeMap<ValueId, ValueId> = BTreeMap::new();
    let mut addrs: BTreeMap<ValueId, (ValueId, i64)> = BTreeMap::new();
    let mut payload: BTreeMap<ValueId, ValueId> = BTreeMap::new();
    let mut tagged: BTreeSet<ValueId> = BTreeSet::new();
    for op in ops {
        match op {
            Op::ConstInt { dst, value } => {
                consts.insert(*dst, *value);
            }
            Op::Alloc { dst, init: Init::DynListStr { len }, .. } if consts.get(len) == Some(&1) && !skip.contains(dst) => {
                blocks.insert(*dst);
            }
            Op::Prim { kind: PrimKind::Handle, dst: Some(d), args } if args.len() == 1 => {
                handles.insert(*d, args[0]);
            }
            Op::IntBinOp { dst, op: crate::IntOp::Add, a, b } if handles.contains_key(a) && consts.contains_key(b) => {
                addrs.insert(*dst, (handles[a], consts[b]));
            }
            Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args } if args.len() == 2 => {
                if let (Some((r, 12)), Some(_)) = (addrs.get(&args[0]).copied(), handles.get(&args[1])) {
                    if blocks.contains(&r) {
                        payload.insert(r, args[1]);
                    }
                }
            }
            Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args } if args.len() == 2 => {
                if let Some((r, 16)) = addrs.get(&args[0]).copied() {
                    if blocks.contains(&r) {
                        tagged.insert(r);
                    }
                }
            }
            _ => {}
        }
    }
    let recognized: BTreeSet<ValueId> = payload.keys().copied().filter(|r| tagged.contains(r)).collect();
    let pieces = recognized.iter().map(|r| payload[r]).collect();
    (recognized, pieces)
}

/// Rewrite every recognized String-Ok Result window in `f`.
///
/// `carriers` are the values whose declared type is a generated heap
/// Option/Result carrier (`render_native_carrier`): their one-slot blocks
/// share this window's shape and are left to that render.
pub fn rewrite_result_str_ops(
    f: &mut MirFunction,
    res_str_fns: &BTreeSet<String>,
    res_str_params: &[ValueId],
    carriers: &BTreeSet<ValueId>,
) {
    let (recognized, piece_handle_ids) = recognize(&f.ops, carriers);
    let ops = std::mem::take(&mut f.ops);
    let stored: BTreeSet<_> = ops.iter().filter_map(|op| match op {
        Op::Prim { kind: PrimKind::Store { width: 8 }, args, .. } if args.len() == 2 => Some(args[1]),
        _ => None,
    }).collect();
    let transferred: BTreeSet<_> = ops.iter().filter_map(|op| match op {
        Op::Prim { kind: PrimKind::Handle, dst: Some(d), args } if stored.contains(d) && args.len() == 1 => Some(args[0]),
        _ => None,
    }).collect();
    let mut out: Vec<Op> = Vec::with_capacity(ops.len());
    let mut t = Tracker::default();
    let str_result = crate::render_native_rec::block_key(&almide_lang::types::Ty::result(
        almide_lang::types::Ty::String, almide_lang::types::Ty::String));
    t.vals.extend(f.aggregate_blocks.iter().filter(|(_, ty)| **ty == str_result).map(|(v, _)| *v));
    t.vals.extend(res_str_params.iter().copied());
    for op in ops {
        if let Op::Prim { kind: PrimKind::Handle, dst: Some(d), args } = &op {
            if args.len() == 1 && stored.contains(d) && t.vals.contains(&args[0]) { out.push(op.clone()); }
        }
        let Some(op) = t.window_op(op, &mut out, &recognized, &piece_handle_ids) else { continue };
        let Some(op) = t.ownership_op(op, &mut out, &transferred) else { continue };
        t.flow_op(op, &mut out, res_str_fns);
    }
    f.ops = out;
}

impl Tracker {
    /// The block-window arms: a const, a recognized block's Alloc, the
    /// swallowed `Handle`/address, the payload and tag stores, and the tag and
    /// payload reads. Hands `op` back when none of them applies.
    fn window_op(
        &mut self,
        op: Op,
        out: &mut Vec<Op>,
        recognized: &BTreeSet<ValueId>,
        piece_handle_ids: &BTreeSet<ValueId>,
    ) -> Option<Op> {
        match &op {
            Op::ConstInt { dst, value } => {
                self.consts.insert(*dst, *value);
                out.push(op);
            }
            // A recognized block becomes its carrier at its payload store.
            Op::Alloc { dst, .. } if recognized.contains(dst) => {}
            Op::Prim { kind: PrimKind::Handle, dst: Some(d), args } if args.len() == 1 => {
                let da = (*d, args[0]);
                self.handle_op(op, da, out, recognized, piece_handle_ids);
            }
            Op::IntBinOp { dst, op: crate::IntOp::Add, a, b } if self.handles.contains_key(a) && self.consts.contains_key(b) => {
                self.addrs.insert(*dst, (self.handles[a], self.consts[b]));
            }
            Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args }
                if args.len() == 2
                    && self.addrs.get(&args[0]).is_some_and(|(r, off)| *off == 12 && recognized.contains(r))
                    && self.piece_handles.contains_key(&args[1]) =>
            {
                let (r, _) = self.addrs[&args[0]];
                let piece = self.piece_handles[&args[1]];
                out.push(Op::Prim { kind: PrimKind::ResStrMake, dst: Some(r), args: vec![piece] });
                self.vals.insert(r);
                self.minted.insert(r);
            }
            Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args }
                if args.len() == 2
                    && self.addrs.get(&args[0]).is_some_and(|(r, off)| *off == 16 && self.minted.contains(r)) =>
            {
                let (r, _) = self.addrs[&args[0]];
                out.push(Op::Prim { kind: PrimKind::ResStrSetTag, dst: None, args: vec![r, args[1]] });
            }
            Op::Prim { kind, dst: Some(d), args } if args.len() == 1 && self.addrs.contains_key(&args[0]) => {
                let (r, off) = self.addrs[&args[0]];
                let load = load_window(kind, off, *d, r, self.vals.contains(&r));
                out.push(load.unwrap_or(op));
            }
            _ => return Some(op),
        }
        None
    }

    /// A one-arg `Handle` op `d = handle(a)`: swallowed over a tracked or
    /// recognized block, swallowed as a recognized store's piece handle, or
    /// kept.
    fn handle_op(
        &mut self,
        op: Op,
        (d, a): (ValueId, ValueId),
        out: &mut Vec<Op>,
        recognized: &BTreeSet<ValueId>,
        piece_handle_ids: &BTreeSet<ValueId>,
    ) {
        if self.vals.contains(&a) || recognized.contains(&a) {
            self.handles.insert(d, a);
        } else if piece_handle_ids.contains(&d) {
            self.piece_handles.insert(d, a);
        } else {
            out.push(op);
        }
    }

    /// The ownership arms over a minted or tracked carrier. Hands `op` back
    /// when none of them applies.
    fn ownership_op(&mut self, op: Op, out: &mut Vec<Op>, transferred: &BTreeSet<ValueId>) -> Option<Op> {
        match &op {
            Op::Dup { dst, src } if self.minted.contains(src) => {
                out.push(Op::Prim { kind: PrimKind::ResCopy, dst: Some(*dst), args: vec![*src] });
                self.vals.insert(*dst);
                self.minted.insert(*dst);
            }
            Op::Consume { v } if self.vals.contains(v) && !self.minted.contains(v) && !transferred.contains(v) => {}
            _ => return Some(op),
        }
        None
    }

    /// The call, if-join and pass-through arms.
    fn flow_op(&mut self, op: Op, out: &mut Vec<Op>, res_str_fns: &BTreeSet<String>) {
        match &op {
            Op::CallFn { dst, name, args, result } => {
                if let Some(d) = dst {
                    if res_str_fns.contains(name) || crate::native_floor::declared_result_carrier(name) == Some(true) {
                        self.vals.insert(*d);
                    }
                }
                // Native String-Result construction retains its owned credit;
                // a call borrows the carrier exactly like any other Result.
                let args = args.clone();
                out.push(Op::CallFn { dst: *dst, name: name.clone(), args, result: *result });
            }
            Op::IfThen { dst, .. } => {
                self.if_dsts.push(*dst);
                out.push(op);
            }
            Op::Else { val } | Op::EndIf { val } => {
                if let (Some(v), Some(Some(d))) = (val, self.if_dsts.last()) {
                    if self.vals.contains(v) {
                        self.vals.insert(*d);
                    }
                }
                if matches!(op, Op::EndIf { .. }) {
                    self.if_dsts.pop();
                }
                out.push(op);
            }
            _ => out.push(op),
        }
    }
}

/// The Result read a swallowed load at offset `off` of block `r` is, when
/// `r` is a tracked Result (`tracked`): the tag word or the payload String;
/// `None` for any other window.
fn load_window(kind: &PrimKind, off: i64, d: ValueId, r: ValueId, tracked: bool) -> Option<Op> {
    match (kind, off) {
        (PrimKind::Load { width: 4 }, 16) if tracked => Some(Op::Prim { kind: PrimKind::ResTag, dst: Some(d), args: vec![r] }),
        (PrimKind::LoadHandle, 12) if tracked => Some(Op::Prim { kind: PrimKind::ResErrStr, dst: Some(d), args: vec![r] }),
        _ => None,
    }
}

#[derive(Default)]
struct Tracker {
    consts: BTreeMap<ValueId, i64>,
    /// Values known to be String-Ok Results.
    vals: BTreeSet<ValueId>,
    /// Carriers this pass minted, each owning its own alloc-class credit.
    minted: BTreeSet<ValueId>,
    /// A swallowed `Handle` over a tracked or recognized block → the block.
    handles: BTreeMap<ValueId, ValueId>,
    /// A swallowed `Handle + const` → (the block, the offset).
    addrs: BTreeMap<ValueId, (ValueId, i64)>,
    /// A recognized store's piece `Handle` → the piece.
    piece_handles: BTreeMap<ValueId, ValueId>,
    if_dsts: Vec<Option<ValueId>>,
}

#[cfg(test)]
#[path = "native_result_str_credit_test.rs"]
mod credit_tests;
