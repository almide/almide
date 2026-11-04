//! #4006: a map's positional reads through v0's runtime `AlmideMap`.
//!
//! A map value is the generated `AlmideMap<K, V>` alias (its ops are declared
//! floor calls). What the lowering still emits as block reads is the
//! `for (k, v) in m` walk, which reads the map's window by position:
//!
//!   the length word            `Load4(H + 4)`                → `m.len()`
//!   entry `i`'s key            `H + 12 + i * 8`              → `m.entry_at(i).0`
//!   entry `i`'s value          `H + 12 + (len + i) * 8`      → `m.entry_at(i).1`
//!
//! where `len` is that map's own length read and `i` a plain counter (no
//! arithmetic of its own). Any other window over a map walls, so a layout
//! this does not name (an interleaved one's `2 * i`) never renders as one.

use super::{AddrState, Idx};
use crate::lower::layout::{BLOCK_HEADER, BLOCK_LEN_OFFSET, SLOT_SIZE};
use crate::lower::LowerError;
use crate::render_native::{var, wall, NTy};
use crate::render_native_rec::{map_kinds, owned_ty};
use crate::{IntOp, Op, PrimKind, ValueId};
use std::collections::BTreeMap;
use std::fmt::Write as _;

fn map_of(t: Option<NTy>) -> Option<(NTy, NTy)> {
    match t {
        Some(NTy::Rec(i) | NTy::RecRef(i)) => map_kinds(i),
        _ => None,
    }
}

fn emit(out: &mut String, indent: usize, s: &str) {
    for _ in 0..indent {
        out.push_str("    ");
    }
    writeln!(out, "{s}").expect("writing to a String cannot fail");
}

/// An index `Add`: recorded as plain arithmetic, and as `len + i` when one
/// side is a map's own length read.
fn note_add(st: &mut AddrState, dst: ValueId, a: ValueId, b: ValueId) {
    st.arith.insert(dst);
    if let Some(m) = st.map_lens.get(&a).or_else(|| st.map_lens.get(&b)).copied() {
        let i = if st.map_lens.contains_key(&a) { b } else { a };
        st.len_plus.insert(dst, (m, i));
    }
}

/// Any other index arithmetic: recorded, and a `* SLOT_SIZE` scaling noted.
fn note_arith(st: &mut AddrState, dst: ValueId, op: IntOp, a: ValueId, b: ValueId) {
    st.arith.insert(dst);
    if op == IntOp::Mul {
        let w = i64::from(SLOT_SIZE);
        if st.consts.get(&b) == Some(&w) {
            st.scaled_src.insert(dst, a);
        } else if st.consts.get(&a) == Some(&w) {
            st.scaled_src.insert(dst, b);
        }
    }
}

/// Render `op` if it reads a map's window. `None` = not a map op.
pub(super) fn render_map_op(
    op: &Op,
    st: &mut AddrState,
    tys: &mut BTreeMap<ValueId, NTy>,
    out: &mut String,
    indent: usize,
) -> Option<Result<bool, LowerError>> {
    match op {
        // Index arithmetic: recorded, rendered as the plain i64 it is.
        Op::IntBinOp { dst, op: IntOp::Add, a, b } if !st.addrs.contains_key(a) && !st.addrs.contains_key(b) => {
            note_add(st, *dst, *a, *b);
            None
        }
        Op::IntBinOp { dst, op, a, b } if !st.addrs.contains_key(a) && !st.addrs.contains_key(b) => {
            note_arith(st, *dst, *op, *a, *b);
            None
        }
        Op::Prim { kind: kind @ (PrimKind::Load { .. } | PrimKind::LoadHandle), dst: Some(d), args } if args.len() == 1 => {
            let ad = st.addrs.get(&args[0]).copied()?;
            let (k, v) = map_of(tys.get(&ad.base).copied())?;
            let m = var(ad.base);
            if matches!(kind, PrimKind::Load { width: 4 }) && ad.off == i64::from(BLOCK_LEN_OFFSET) && ad.idx.is_none() {
                tys.insert(*d, NTy::I64);
                st.map_lens.insert(*d, ad.base);
                emit(out, indent, &format!("let mut {}: i64 = {m}.len() as i64;", var(*d)));
                return Some(Ok(true));
            }
            let (Some(Idx::Bytes(x)), true) = (ad.idx, ad.off == i64::from(BLOCK_HEADER)) else {
                return Some(Err(wall(format!("native: a map read at {ad:?}"))));
            };
            let Some(&s) = st.scaled_src.get(&x) else {
                return Some(Err(wall("native: a map read at an unscaled position")));
            };
            let (part, kind_of, i) = match st.len_plus.get(&s).copied() {
                Some((owner, i)) if owner == ad.base && !st.arith.contains(&i) => ("1", v, i),
                None if !st.arith.contains(&s) => ("0", k, s),
                _ => return Some(Err(wall("native: a map read at a computed position"))),
            };
            let scalar = matches!(kind, PrimKind::Load { width: 8 });
            if scalar != (kind_of == NTy::I64) {
                return Some(Err(wall("native: a map read of the wrong width for its kind")));
            }
            tys.insert(*d, kind_of);
            let read = format!("{m}.entry_at({} as usize).{part}", var(i));
            let rhs = if scalar { format!("*{read}") } else { format!("{read}.clone()") };
            emit(out, indent, &format!("let mut {}: {} = {rhs};", var(*d), owned_ty(kind_of)));
            Some(Ok(true))
        }
        _ => None,
    }
}
