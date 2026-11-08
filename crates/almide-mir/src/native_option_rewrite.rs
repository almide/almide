//! NATIVE-ONLY scalar-Option carrier rewrite (#4006, family B).
//!
//! The Result carrier's twin (`native_result_rewrite`). A scalar Option is a
//! 0-or-1 block: the materializers emit `Alloc { init: OptSome { payload } }`
//! and `Alloc { init: OptNone }`, and a consumer reads the block's length word
//! (`Handle H → Load4(H+4)`, 1 = Some) and its payload slot (`Load8(H+12)`).
//! This pass maps them onto `PrimKind::Opt*`, which the native renderer
//! renders as a Rust `Option<i64>` local (`NTy::Opt`).
//!
//! Which values are Options comes from TYPES, never names: a minted Alloc, a
//! call to a fn whose declared return is a scalar Option (`option_fns`), a
//! param whose declared type is one (`opt_params`), and an if-value join over
//! one of those. Producers are a total single-op match; consumers are the
//! stereotyped windows over a tracked value. Anything else touching a tracked
//! value (another offset, a `LoadHandle`, a heap payload) is left unrewritten
//! and WALLS at the render, as an unrecognized window always has.
//!
//! Ownership accounting follows the Result carrier's: a minted carrier has no
//! object (the Alloc became a prim, ownership-neutral to the verifier), so its
//! `Consume` goes, its `Dup` becomes `OptCopy` and its release goes; a call
//! result keeps its heap birth and its release.
//!
//! The wasm leg never sees any of this.

use crate::{Init, MirFunction, Op, PrimKind, ValueId};
use std::collections::{BTreeMap, BTreeSet};

/// Rewrite every recognized scalar-Option window in `f`.
///
/// `carriers` are the values whose declared type is a generated heap Option
/// carrier (`render_native_carrier`): their `OptNone` is that carrier's.
pub fn rewrite_option_ops(
    f: &mut MirFunction,
    option_fns: &BTreeSet<String>,
    opt_params: &[ValueId],
    carriers: &BTreeSet<ValueId>,
    filled: &BTreeSet<ValueId>,
) {
    let ops = std::mem::take(&mut f.ops);
    let mut out: Vec<Op> = Vec::with_capacity(ops.len());
    let mut t = OptionTracker { fresh: next_value(f, &ops), filled: filled.clone(), ..OptionTracker::default() };
    // A generic runtime declaration can serve both scalar and heap payloads.
    // A typed handle read is positive evidence of the latter: preserve its
    // entire address window, including the owner edge the checker follows.
    let heap_payloads = heap_payload_roots(&ops);
    t.opt_vals.extend(opt_params.iter().copied());
    for op in ops {
        let Some(op) = t.carrier_op(op, &mut out, carriers) else { continue };
        t.flow_op(op, &mut out, option_fns, &heap_payloads);
    }
    f.ops = out;
}

impl OptionTracker {
    /// A 0-or-1 block filled in place (`crate::option_blocks`): None until
    /// its hit stores the payload, then its length word 1. `true` when `op`
    /// is one of the block's ops and was rewritten; any other length word is
    /// left as it is (`false`), and walls.
    fn filled_block_op(&mut self, op: &Op, out: &mut Vec<Op>) -> bool {
        match op {
            Op::Alloc { dst, init: Init::DynList { len }, .. } if self.filled.contains(dst) && self.const_vals.get(len) == Some(&1) => {
                out.push(Op::Prim { kind: PrimKind::OptMakeNone, dst: Some(*dst), args: vec![] });
                self.opt_vals.insert(*dst);
                self.minted.insert(*dst);
                self.filling.insert(*dst);
            }
            Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args }
                if args.len() == 2 && self.addrs.get(&args[0]).is_some_and(|(o, off)| *off == 12 && self.filling.contains(o)) =>
            {
                self.pending.insert(self.addrs[&args[0]].0, args[1]);
            }
            Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args }
                if args.len() == 2 && self.addrs.get(&args[0]).is_some_and(|(o, off)| *off == 4 && self.filling.contains(o)) =>
            {
                let o = self.addrs[&args[0]].0;
                let made = match self.const_vals.get(&args[1]) {
                    Some(0) => {
                        self.pending.remove(&o);
                        Some(Op::Prim { kind: PrimKind::OptMakeNone, dst: Some(ValueId(self.fresh)), args: vec![] })
                    }
                    Some(1) => self.pending.remove(&o).map(|p| Op::Prim { kind: PrimKind::OptMakeSome, dst: Some(ValueId(self.fresh)), args: vec![p] }),
                    _ => None,
                };
                let Some(made) = made else { return false };
                out.push(made);
                out.push(Op::SetLocal { local: o, src: ValueId(self.fresh) });
                self.fresh += 1;
            }
            _ => return false,
        }
        true
    }

    /// The carrier-producing and carrier-reading arms: a const, a minted
    /// carrier, its ownership ops, and the swallowed `Handle`/address/load
    /// window. Hands `op` back when none of them applies.
    fn carrier_op(&mut self, op: Op, out: &mut Vec<Op>, carriers: &BTreeSet<ValueId>) -> Option<Op> {
        if self.filled_block_op(&op, out) {
            return None;
        }
        match &op {
            Op::ConstInt { dst, value } => {
                self.const_vals.insert(*dst, *value);
                out.push(op);
            }
            Op::Alloc { dst, init: Init::OptSome { payload }, .. } => {
                out.push(Op::Prim { kind: PrimKind::OptMakeSome, dst: Some(*dst), args: vec![*payload] });
                self.opt_vals.insert(*dst);
                self.minted.insert(*dst);
            }
            Op::Alloc { dst, init: Init::OptNone, .. } if !carriers.contains(dst) => {
                out.push(Op::Prim { kind: PrimKind::OptMakeNone, dst: Some(*dst), args: vec![] });
                self.opt_vals.insert(*dst);
                self.minted.insert(*dst);
            }
            Op::Dup { dst, src } if self.minted.contains(src) => {
                out.push(Op::Prim { kind: PrimKind::OptCopy, dst: Some(*dst), args: vec![*src] });
                self.opt_vals.insert(*dst);
                self.minted.insert(*dst);
            }
            Op::Drop { v } | Op::DropListStr { v } if self.minted.contains(v) => {}
            Op::Consume { v } if self.opt_vals.contains(v) => {}
            Op::Prim { kind: PrimKind::Handle, dst: Some(d), args } if args.len() == 1 && self.opt_vals.contains(&args[0]) => {
                self.handles.insert(*d, args[0]);
            }
            // An Option stored into a list's slot: the slot holds the Option
            // itself (the list's elements are `Option<i64>`), not a handle.
            Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args }
                if args.len() == 2 && self.handles.contains_key(&args[1]) && !self.addrs.contains_key(&args[0]) =>
            {
                out.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![args[0], self.handles[&args[1]]] });
            }
            Op::IntBinOp { dst, op: crate::IntOp::Add, a, b } if self.handles.contains_key(a) && self.const_vals.contains_key(b) => {
                self.addrs.insert(*dst, (self.handles[a], self.const_vals[b]));
            }
            Op::Prim { kind, dst: Some(d), args } if args.len() == 1 && self.addrs.contains_key(&args[0]) => {
                let (o, off) = self.addrs[&args[0]];
                let load = load_window(kind, off, *d, o);
                out.push(load.unwrap_or(op));
            }
            _ => return Some(op),
        }
        None
    }

    /// The call, if-join and pass-through arms.
    fn flow_op(&mut self, op: Op, out: &mut Vec<Op>, option_fns: &BTreeSet<String>, heap_payloads: &BTreeSet<ValueId>) {
        match &op {
            Op::CallFn { dst, name, args, result } => {
                if let Some(d) = dst {
                    if !heap_payloads.contains(d)
                        && (option_fns.contains(name) || crate::native_floor::declared_scalar_option(name)) {
                        self.opt_vals.insert(*d);
                    }
                }
                // A minted carrier has no object: as a call arg it is a value
                // the callee copies, not a heap handle it borrows.
                let args = args
                    .iter()
                    .map(|a| match a {
                        crate::CallArg::Handle(v) if self.minted.contains(v) => crate::CallArg::Scalar(*v),
                        other => other.clone(),
                    })
                    .collect();
                out.push(Op::CallFn { dst: *dst, name: name.clone(), args, result: *result });
            }
            Op::IfThen { dst, .. } => {
                self.if_dsts.push(*dst);
                out.push(op);
            }
            Op::Else { val } | Op::EndIf { val } => {
                if let (Some(v), Some(Some(d))) = (val, self.if_dsts.last()) {
                    if self.opt_vals.contains(v) {
                        self.opt_vals.insert(*d);
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

/// Roots whose slot windows contain a typed heap load. Unlike the generic
/// callee's name, that load describes this call's instantiated payload.
fn heap_payload_roots(ops: &[Op]) -> BTreeSet<ValueId> {
    let mut roots = BTreeMap::new();
    let mut heap = BTreeSet::new();
    for op in ops {
        match op {
            Op::Prim { kind: PrimKind::Handle, dst: Some(d), args } if args.len() == 1 => {
                roots.insert(*d, args[0]);
            }
            Op::IntBinOp { dst, op: crate::IntOp::Add, a, b } => {
                if let Some(root) = roots.get(a).or_else(|| roots.get(b)).copied() {
                    roots.insert(*dst, root);
                }
            }
            Op::Prim { kind: PrimKind::LoadHandle, args, .. } if args.len() == 1 => {
                if let Some(root) = roots.get(&args[0]) { heap.insert(*root); }
            }
            _ => {}
        }
    }
    heap
}

/// The Option read a swallowed `Load` at offset `off` of Option `o` is: the
/// tag (length word) or the payload slot; `None` for any other window.
pub(crate) fn load_window(kind: &PrimKind, off: i64, d: ValueId, o: ValueId) -> Option<Op> {
    match (kind, off) {
        (PrimKind::Load { width: 4 }, 4) => Some(Op::Prim { kind: PrimKind::OptTag, dst: Some(d), args: vec![o] }),
        (PrimKind::Load { width: 8 }, 12) => Some(Op::Prim { kind: PrimKind::OptPayload, dst: Some(d), args: vec![o] }),
        _ => None,
    }
}

#[derive(Default)]
struct OptionTracker {
    const_vals: BTreeMap<ValueId, i64>,
    /// Values known to be scalar Options.
    opt_vals: BTreeSet<ValueId>,
    /// Carriers this pass minted (no object of their own).
    minted: BTreeSet<ValueId>,
    /// A swallowed `Handle` over an Option → the Option.
    handles: BTreeMap<ValueId, ValueId>,
    /// A swallowed `Handle + const` → (the Option, the offset).
    addrs: BTreeMap<ValueId, (ValueId, i64)>,
    if_dsts: Vec<Option<ValueId>>,
    /// The blocks filled in place this function's lowering recorded, the ones
    /// this pass took over, and the payload each one's slot store holds
    /// until its length word makes it Some.
    filled: BTreeSet<ValueId>,
    filling: BTreeSet<ValueId>,
    pending: BTreeMap<ValueId, ValueId>,
    /// The next value id no op of the function uses.
    fresh: u32,
}

/// One past the highest value id `f`'s params and `ops` mention.
fn next_value(f: &MirFunction, ops: &[Op]) -> u32 {
    let mut hi = f.params.iter().map(|p| p.value.0 + 1).max().unwrap_or(0);
    for op in ops {
        let mut reads = Vec::new();
        crate::mir_ops::op_reads(op, &mut reads);
        let defined = crate::mir_ops::defined_value(op);
        let set = match op {
            Op::SetLocal { local, .. } => Some(*local),
            _ => None,
        };
        for v in reads.into_iter().chain(defined).chain(set) {
            hi = hi.max(v.0 + 1);
        }
    }
    hi
}
