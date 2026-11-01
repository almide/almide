//! NATIVE-ONLY narrow integer arithmetic (#4006).
//!
//! The MIR carries every integer in one i64 lane, so the lowering re-wraps a
//! narrow result after each `+`/`-`/`*` at its declared width
//! (`wrap_to_declared_width`): a signed width `w` as `shr(shl(x, 64-w),
//! 64-w)`, an unsigned one as `and(x, 2^w-1)` (#889). Rendered as written,
//! an `Int32` recurrence sign-extends twice per step on its dependency chain,
//! where the standard codegen computes in real `i32` (0.26s against 0.19s).
//!
//! A ring op whose ONLY reader is such a re-wrap is fused with it into
//! `PrimKind::NativeNarrowBin`: the op computed in the narrow type itself
//! (`(a as i32).wrapping_mul(b as i32) as i64`), whose widening back to the
//! lane is exactly the re-wrap's value — the low `w` bits of a two's-complement
//! ring result depend only on the low `w` bits of its operands, which `as i32`
//! keeps, and `as i64` sign-extends (an unsigned type zero-extends) the way the
//! re-wrap does. The re-wrap itself becomes the identity `x | x`.
//!
//! The width and signedness are read off the re-wrap's own constants — the
//! lowering's encoding of the declared width — never off a type or a name.
//! Integer ops carry no ownership, so the certificate taken after this pass
//! is the one it would have been. The wasm leg never sees this pass.

use crate::{IntOp, MirFunction, Op, PrimKind, ValueId};
use std::collections::BTreeMap;

/// The narrow widths the native render spells as real Rust integer types.
fn native_width(bits: u32) -> Option<u8> {
    matches!(bits, 8 | 16 | 32).then_some(bits as u8)
}

/// Fuse every ring op that is re-wrapped to a narrow width in `f`.
///
/// WALLS a call into a legacy runtime shim whose result is re-wrapped to a
/// narrow width: the shim table's bodies compute in the i64 lane (their
/// declared params and return are `i64`), so the narrow value would pay the
/// lane arithmetic where the standard codegen computes in the narrow type —
/// a performance loss the certified path does not take. Read off the shim
/// table's declarations and the re-wrap after the call, not off a name.
pub fn rewrite_narrow_ops(f: &mut MirFunction) -> Result<(), crate::lower::LowerError> {
    let mut consts: BTreeMap<ValueId, i64> = BTreeMap::new();
    let mut uses: BTreeMap<ValueId, Vec<usize>> = BTreeMap::new();
    for (i, op) in f.ops.iter().enumerate() {
        if let Op::ConstInt { dst, value } = op {
            consts.insert(*dst, *value);
        }
        let mut reads = Vec::new();
        crate::mir_ops::op_reads(op, &mut reads);
        for v in reads {
            uses.entry(v).or_default().push(i);
        }
    }
    if let Some(r) = f.ret {
        uses.entry(r).or_default().push(usize::MAX);
    }
    let only_use = |v: ValueId| -> Option<usize> {
        match uses.get(&v).map(Vec::as_slice) {
            Some([i]) if *i != usize::MAX => Some(*i),
            _ => None,
        }
    };
    // The narrow re-wrap `t`'s only reader starts, if any: (bits, signed).
    let rewrap_of = |t: ValueId| -> Option<(u32, bool)> {
        let j = only_use(t)?;
        match &f.ops[j] {
            Op::IntBinOp { op: IntOp::And, a, b, .. } => {
                let m = *(if *a == t { consts.get(b) } else if *b == t { consts.get(a) } else { None })?;
                (m > 0 && (m & (m + 1)) == 0).then(|| (64 - m.leading_zeros(), false))
            }
            Op::IntBinOp { dst: s, op: IntOp::Shl, a, b: c1 } if *a == t => {
                let k = only_use(*s)?;
                let Op::IntBinOp { op: IntOp::Shr, a: s2, b: c2, .. } = &f.ops[k] else { return None };
                let (c, c_) = (*consts.get(c1)?, *consts.get(c2)?);
                (s2 == s && c == c_ && (1..64).contains(&c)).then(|| (64 - c as u32, true))
            }
            _ => None,
        }
    };
    for op in &f.ops {
        let Op::CallFn { dst: Some(t), name, .. } = op else { continue };
        let Some((params, ret, _)) = crate::render_native_shims::shim(name) else { continue };
        let lane = params.iter().chain(ret.iter()).all(|k| *k == crate::render_native::NTy::I64);
        if lane && rewrap_of(*t).is_some_and(|(bits, _)| native_width(bits).is_some()) {
            return Err(crate::render_native::wall(format!(
                "native: a narrow-width result of the i64-lane runtime shim `{name}` — it would compute \
                 in the lane where the standard codegen computes in the narrow type"
            )));
        }
    }
    // (ring op index, re-wrap op indices to make the identity, bits, signed)
    let mut fuse: Vec<(usize, Vec<usize>, u8, bool)> = Vec::new();
    for (i, op) in f.ops.iter().enumerate() {
        let Op::IntBinOp { dst: t, op: IntOp::Add | IntOp::Sub | IntOp::Mul, .. } = op else { continue };
        let Some(j) = only_use(*t) else { continue };
        match &f.ops[j] {
            // and(t, 2^w - 1): an unsigned re-wrap.
            Op::IntBinOp { op: IntOp::And, a, b, .. } => {
                let m = if a == t { consts.get(b) } else if b == t { consts.get(a) } else { None };
                let Some(&m) = m.filter(|m| **m > 0 && (**m & (**m + 1)) == 0) else { continue };
                if let Some(bits) = native_width(64 - m.leading_zeros()) {
                    fuse.push((i, vec![j], bits, false));
                }
            }
            // shr(shl(t, c), c): a signed re-wrap.
            Op::IntBinOp { dst: s, op: IntOp::Shl, a, b: c1 } if a == t => {
                let Some(k) = only_use(*s) else { continue };
                let Op::IntBinOp { op: IntOp::Shr, a: s2, b: c2, .. } = &f.ops[k] else { continue };
                let (Some(&c), Some(&c_)) = (consts.get(c1), consts.get(c2)) else { continue };
                if s2 != s || c != c_ || !(1..64).contains(&c) {
                    continue;
                }
                if let Some(bits) = native_width(64 - c as u32) {
                    fuse.push((i, vec![j, k], bits, true));
                }
            }
            _ => {}
        }
    }
    for (i, wraps, bits, signed) in fuse {
        let Op::IntBinOp { dst: t, op, a, b } = f.ops[i].clone() else { continue };
        f.ops[i] = Op::Prim { kind: PrimKind::NativeNarrowBin { op, bits, signed }, dst: Some(t), args: vec![a, b] };
        for w in wraps {
            if let Op::IntBinOp { dst, .. } = f.ops[w] {
                f.ops[w] = Op::IntBinOp { dst, op: IntOp::Or, a: t, b: t };
            }
        }
    }
    Ok(())
}
