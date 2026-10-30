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
//! has no object, so its `Consume` and its release go and its `Dup` becomes
//! `ResCopy`; a call result keeps its heap birth and its release.
//!
//! The wasm leg never sees any of this.

use crate::{Init, MirFunction, Op, PrimKind, ValueId};
use std::collections::{BTreeMap, BTreeSet};

/// The one-slot blocks of `ops` whose window is a String-Ok Result's — a
/// `Store8` of a handle at +12 and a `Store4` at +16 (a one-element list has
/// no word at +16: its element fills +12..+20) — and the piece handles those
/// stores take.
fn recognize(ops: &[Op]) -> (BTreeSet<ValueId>, BTreeSet<ValueId>) {
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
            Op::Alloc { dst, init: Init::DynListStr { len }, .. } if consts.get(len) == Some(&1) => {
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
pub fn rewrite_result_str_ops(f: &mut MirFunction, res_str_fns: &BTreeSet<String>, res_str_params: &[ValueId]) {
    let (recognized, piece_handle_ids) = recognize(&f.ops);
    let ops = std::mem::take(&mut f.ops);
    let mut out: Vec<Op> = Vec::with_capacity(ops.len());
    let mut t = Tracker::default();
    t.vals.extend(res_str_params.iter().copied());
    for op in ops {
        match &op {
            Op::ConstInt { dst, value } => {
                t.consts.insert(*dst, *value);
                out.push(op);
            }
            // A recognized block becomes its carrier at its payload store.
            Op::Alloc { dst, .. } if recognized.contains(dst) => {}
            Op::Prim { kind: PrimKind::Handle, dst: Some(d), args } if args.len() == 1 => {
                let a = args[0];
                if t.vals.contains(&a) || recognized.contains(&a) {
                    t.handles.insert(*d, a);
                } else if piece_handle_ids.contains(d) {
                    t.piece_handles.insert(*d, a);
                } else {
                    out.push(op);
                }
            }
            Op::IntBinOp { dst, op: crate::IntOp::Add, a, b } if t.handles.contains_key(a) && t.consts.contains_key(b) => {
                t.addrs.insert(*dst, (t.handles[a], t.consts[b]));
            }
            Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args }
                if args.len() == 2
                    && t.addrs.get(&args[0]).is_some_and(|(r, off)| *off == 12 && recognized.contains(r))
                    && t.piece_handles.contains_key(&args[1]) =>
            {
                let (r, _) = t.addrs[&args[0]];
                let piece = t.piece_handles[&args[1]];
                out.push(Op::Prim { kind: PrimKind::ResStrMake, dst: Some(r), args: vec![piece] });
                t.vals.insert(r);
                t.minted.insert(r);
            }
            Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args }
                if args.len() == 2
                    && t.addrs.get(&args[0]).is_some_and(|(r, off)| *off == 16 && t.minted.contains(r)) =>
            {
                let (r, _) = t.addrs[&args[0]];
                out.push(Op::Prim { kind: PrimKind::ResStrSetTag, dst: None, args: vec![r, args[1]] });
            }
            Op::Prim { kind, dst: Some(d), args } if args.len() == 1 && t.addrs.contains_key(&args[0]) => {
                let (r, off) = t.addrs[&args[0]];
                match (kind, off) {
                    (PrimKind::Load { width: 4 }, 16) if t.vals.contains(&r) => {
                        out.push(Op::Prim { kind: PrimKind::ResTag, dst: Some(*d), args: vec![r] })
                    }
                    (PrimKind::LoadHandle, 12) if t.vals.contains(&r) => {
                        out.push(Op::Prim { kind: PrimKind::ResErrStr, dst: Some(*d), args: vec![r] })
                    }
                    _ => out.push(op),
                }
            }
            Op::Dup { dst, src } if t.minted.contains(src) => {
                out.push(Op::Prim { kind: PrimKind::ResCopy, dst: Some(*dst), args: vec![*src] });
                t.vals.insert(*dst);
                t.minted.insert(*dst);
            }
            Op::Drop { v } | Op::DropListStr { v } if t.minted.contains(v) => {}
            Op::Consume { v } if t.vals.contains(v) => {}
            Op::CallFn { dst, name, args, result } => {
                if let Some(d) = dst {
                    if res_str_fns.contains(name) {
                        t.vals.insert(*d);
                    }
                }
                // A minted carrier has no object: as a call arg it is a value
                // the callee copies, not a heap handle it borrows.
                let args = args
                    .iter()
                    .map(|a| match a {
                        crate::CallArg::Handle(v) if t.minted.contains(v) => crate::CallArg::Scalar(*v),
                        other => other.clone(),
                    })
                    .collect();
                out.push(Op::CallFn { dst: *dst, name: name.clone(), args, result: result.clone() });
            }
            Op::IfThen { dst, .. } => {
                t.if_dsts.push(*dst);
                out.push(op);
            }
            Op::Else { val } | Op::EndIf { val } => {
                if let (Some(v), Some(Some(d))) = (val, t.if_dsts.last()) {
                    if t.vals.contains(v) {
                        t.vals.insert(*d);
                    }
                }
                if matches!(op, Op::EndIf { .. }) {
                    t.if_dsts.pop();
                }
                out.push(op);
            }
            _ => out.push(op),
        }
    }
    f.ops = out;
}

#[derive(Default)]
struct Tracker {
    consts: BTreeMap<ValueId, i64>,
    /// Values known to be String-Ok Results.
    vals: BTreeSet<ValueId>,
    /// Carriers this pass minted (no object of their own).
    minted: BTreeSet<ValueId>,
    /// A swallowed `Handle` over a tracked or recognized block → the block.
    handles: BTreeMap<ValueId, ValueId>,
    /// A swallowed `Handle + const` → (the block, the offset).
    addrs: BTreeMap<ValueId, (ValueId, i64)>,
    /// A recognized store's piece `Handle` → the piece.
    piece_handles: BTreeMap<ValueId, ValueId>,
    if_dsts: Vec<Option<ValueId>>,
}
