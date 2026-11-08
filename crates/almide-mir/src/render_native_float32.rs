//! Float32 uses the MIR's zero-extended binary32 bits in an i64 local. Keep
//! that representation at calls, joins and aggregate slots; round each
//! arithmetic operation in f32 before storing its bits. No MIR ownership,
//! call-mode or bounds event is rewritten by this renderer.

use crate::lower::LowerError;
use crate::render_native::{as_f64_arg, var, wall, NTy, OpSink};
use crate::{FBinOp, FCmpOp, FUnOp, Op, PrimKind, ValueId};
use std::collections::BTreeMap;
use std::fmt::Write as _;

fn lane(v: ValueId, tys: &BTreeMap<ValueId, NTy>) -> Result<String, LowerError> {
    match tys.get(&v) {
        Some(NTy::I64) => Ok(var(v)),
        other => Err(wall(format!("native: Float32 lane requires i64 bits, got {other:?}"))),
    }
}

fn f32_arg(v: ValueId, tys: &BTreeMap<ValueId, NTy>) -> Result<String, LowerError> {
    lane_arg(&lane(v, tys)?, NTy::I64)
}

/// The same representation boundary for primitive operations and declared
/// runtime calls: an f64 local cannot stand in for low binary32 bits.
pub(crate) fn lane_arg(code: &str, t: NTy) -> Result<String, LowerError> {
    match t {
        NTy::I64 => Ok(format!("f32::from_bits({code} as u32)")),
        _ => Err(wall("native: Float32 runtime argument is not an i64 bit lane")),
    }
}

pub(crate) fn render(op: &Op, s: OpSink<'_>) -> Result<bool, LowerError> {
    let Op::Prim { kind, dst: Some(d), args } = op else { return Ok(false) };
    let arity = match kind {
        PrimKind::F32Demote | PrimKind::F32Promote | PrimKind::IntToF32
        | PrimKind::F32Bits | PrimKind::F32Un(_) => 1,
        PrimKind::F32Bin(_) | PrimKind::F32Cmp(_) => 2,
        _ => return Ok(false),
    };
    if args.len() != arity {
        return Err(wall(format!("native: {kind:?} requires {arity} operands")));
    }
    let (ty, expr) = match kind {
        PrimKind::F32Demote => {
            let t = *s.tys.get(&args[0]).ok_or_else(|| wall("native: Float32 demotion of an untyped value"))?;
            let a = as_f64_arg(&var(args[0]), t)?;
            (NTy::I64, format!("(({a}) as f32).to_bits() as i64"))
        }
        PrimKind::F32Promote => (NTy::F64, format!("{} as f64", f32_arg(args[0], s.tys)?)),
        // Convert the integer directly: an intermediate f64 would round twice.
        PrimKind::IntToF32 => (NTy::I64, format!("({} as f32).to_bits() as i64", lane(args[0], s.tys)?)),
        PrimKind::F32Bits => (NTy::I64, lane(args[0], s.tys)?),
        PrimKind::F32Un(op) => {
            let a = f32_arg(args[0], s.tys)?;
            let expr = match op {
                FUnOp::Neg => format!("-({a})"),
                FUnOp::Abs => format!("({a}).abs()"),
                FUnOp::Sqrt => format!("({a}).sqrt()"),
                FUnOp::Floor => format!("({a}).floor()"),
                FUnOp::Ceil => format!("({a}).ceil()"),
                FUnOp::Nearest => format!("({a}).round_ties_even()"),
            };
            (NTy::I64, format!("({expr}).to_bits() as i64"))
        }
        PrimKind::F32Bin(op) => {
            let a = f32_arg(args[0], s.tys)?;
            let b = f32_arg(args[1], s.tys)?;
            let expr = match op {
                FBinOp::Add => format!("{a} + {b}"),
                FBinOp::Sub => format!("{a} - {b}"),
                FBinOp::Mul => format!("{a} * {b}"),
                FBinOp::Div => format!("{a} / {b}"),
                FBinOp::CopySign => format!("({a}).copysign({b})"),
                // Rust min/max select the non-NaN operand; wasm propagates
                // NaN. Do not silently substitute the host's semantics.
                FBinOp::Min | FBinOp::Max => return Err(wall("native: Float32 min/max NaN semantics are not rendered")),
            };
            (NTy::I64, format!("({expr}).to_bits() as i64"))
        }
        PrimKind::F32Cmp(op) => {
            let a = f32_arg(args[0], s.tys)?;
            let b = f32_arg(args[1], s.tys)?;
            let sym = match op {
                FCmpOp::Lt => "<", FCmpOp::Le => "<=", FCmpOp::Gt => ">",
                FCmpOp::Ge => ">=", FCmpOp::Eq => "==", FCmpOp::Ne => "!=",
            };
            (NTy::I64, format!("({a} {sym} {b}) as i64"))
        }
        _ => unreachable!("the arity dispatch selects only Float32 primitives"),
    };
    s.tys.insert(*d, ty);
    let rust_ty = if ty == NTy::F64 { "f64" } else { "i64" };
    for _ in 0..s.indent { s.out.push_str("    "); }
    writeln!(s.out, "let mut {}: {rust_ty} = {expr};", var(*d)).unwrap();
    Ok(true)
}
