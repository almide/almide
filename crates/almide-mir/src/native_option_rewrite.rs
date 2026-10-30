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
pub fn rewrite_option_ops(f: &mut MirFunction, option_fns: &BTreeSet<String>, opt_params: &[ValueId]) {
    let ops = std::mem::take(&mut f.ops);
    let mut out: Vec<Op> = Vec::with_capacity(ops.len());
    let mut t = OptionTracker::default();
    t.opt_vals.extend(opt_params.iter().copied());
    for op in ops {
        match &op {
            Op::ConstInt { dst, value } => {
                t.const_vals.insert(*dst, *value);
                out.push(op);
            }
            Op::Alloc { dst, init: Init::OptSome { payload }, .. } => {
                out.push(Op::Prim { kind: PrimKind::OptMakeSome, dst: Some(*dst), args: vec![*payload] });
                t.opt_vals.insert(*dst);
                t.minted.insert(*dst);
            }
            Op::Alloc { dst, init: Init::OptNone, .. } => {
                out.push(Op::Prim { kind: PrimKind::OptMakeNone, dst: Some(*dst), args: vec![] });
                t.opt_vals.insert(*dst);
                t.minted.insert(*dst);
            }
            Op::Dup { dst, src } if t.minted.contains(src) => {
                out.push(Op::Prim { kind: PrimKind::OptCopy, dst: Some(*dst), args: vec![*src] });
                t.opt_vals.insert(*dst);
                t.minted.insert(*dst);
            }
            Op::Drop { v } | Op::DropListStr { v } if t.minted.contains(v) => {}
            Op::Consume { v } if t.opt_vals.contains(v) => {}
            Op::Prim { kind: PrimKind::Handle, dst: Some(d), args } if args.len() == 1 && t.opt_vals.contains(&args[0]) => {
                t.handles.insert(*d, args[0]);
            }
            Op::IntBinOp { dst, op: crate::IntOp::Add, a, b } if t.handles.contains_key(a) && t.const_vals.contains_key(b) => {
                t.addrs.insert(*dst, (t.handles[a], t.const_vals[b]));
            }
            Op::Prim { kind, dst: Some(d), args } if args.len() == 1 && t.addrs.contains_key(&args[0]) => {
                let (o, off) = t.addrs[&args[0]];
                match (kind, off) {
                    (PrimKind::Load { width: 4 }, 4) => {
                        out.push(Op::Prim { kind: PrimKind::OptTag, dst: Some(*d), args: vec![o] })
                    }
                    (PrimKind::Load { width: 8 }, 12) => {
                        out.push(Op::Prim { kind: PrimKind::OptPayload, dst: Some(*d), args: vec![o] })
                    }
                    _ => out.push(op),
                }
            }
            Op::CallFn { dst: Some(d), name, .. } if option_fns.contains(name) => {
                t.opt_vals.insert(*d);
                out.push(op);
            }
            Op::IfThen { dst, .. } => {
                t.if_dsts.push(*dst);
                out.push(op);
            }
            Op::Else { val } | Op::EndIf { val } => {
                if let (Some(v), Some(Some(d))) = (val, t.if_dsts.last()) {
                    if t.opt_vals.contains(v) {
                        t.opt_vals.insert(*d);
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
}
