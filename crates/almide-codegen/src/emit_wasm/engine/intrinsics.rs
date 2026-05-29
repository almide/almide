//! Declarative stdlib dispatch for the v2 engine.
//!
//! Stdlib functions are Almide declarations carrying `@intrinsic("almide_rt_…")`;
//! calls lower to `IrExprKind::RuntimeCall { symbol, args }`. This module is the
//! single registry mapping each intrinsic symbol to an implementation expressed
//! in **verified WasmIR** (`Op`) over **`LayoutRegistry`** — no raw wasm-encoder,
//! no hardcoded offsets. See docs/roadmap/active/wasm-stdlib-dispatch-v2.md.
//!
//! `lower_intrinsic` lowers the argument expressions itself, so tiers that need
//! special argument handling (closure-bearing ops) stay in control. Returning
//! `None` signals "not implemented here" — the caller emits `Op::Unsupported`,
//! and codegen falls back to the legacy emitter.

use almide_ir::IrExpr;
use almide_lang::types::Ty;

use super::ir::{Op, Const, WasmTy, LoadKind, BinOp as B, UnOp as U};
use super::layout::{self, string, list};
use super::lower::{lower_expr, ty_to_wasm, wasm_byte_size, load_kind_of, LowerCtx};

/// Lower a stdlib intrinsic call, or `None` if the symbol is unknown here.
pub fn lower_intrinsic(
    symbol: &str, args: &[IrExpr], ret_ty: &Ty, ctx: &mut LowerCtx,
) -> Option<Vec<Op>> {
    match symbol {
        // ── Tier 1: pure-read primitives ──
        // String/List length: load the i32 LEN field, widen to the Int (i64).
        "almide_rt_string_len" => Some(len_field(&args[0], layout::STRING, string::LEN, ctx)),
        "almide_rt_list_len" => Some(len_field(&args[0], layout::LIST, list::LEN, ctx)),

        // is_empty: LEN == 0 (i32 bool).
        "almide_rt_string_is_empty" => Some(is_empty(&args[0], layout::STRING, string::LEN, ctx)),
        "almide_rt_list_is_empty" => Some(is_empty(&args[0], layout::LIST, list::LEN, ctx)),

        // ── Tier 1: integer min/max/abs (typed i64) ──
        "almide_rt_int_abs" => Some(int_abs(&args[0], ctx)),
        "almide_rt_int_min" if args.len() == 2 => Some(int_minmax(args, B::I64LtS, ctx)),
        "almide_rt_int_max" if args.len() == 2 => Some(int_minmax(args, B::I64GtS, ctx)),

        // ── Tier 1.5: indexed read with bounds fallback ──
        // list.get_or(xs, i, default): xs[i] if i < len else default.
        "almide_rt_list_get_or" if args.len() == 3 => Some(list_get_or(args, ret_ty, ctx)),

        // ── Tier 2: route to an existing runtime function ──
        "almide_rt_int_to_string" => call_runtime("__int_to_string", &args[0..1], 1, ctx),

        _ => None,
    }
}

/// `field` (i32) widened to i64 — for string/list length.
fn len_field(arg: &IrExpr, lay: layout::LayoutId, field: layout::FieldId, ctx: &mut LowerCtx) -> Vec<Op> {
    let mut ops = lower_expr(arg, ctx);
    ops.push(Op::FieldLoad { layout: lay, field, kind: LoadKind::I32 });
    ops.push(Op::UnOp(U::I64ExtendI32U));
    ops
}

/// `field == 0` → i32 bool.
fn is_empty(arg: &IrExpr, lay: layout::LayoutId, field: layout::FieldId, ctx: &mut LowerCtx) -> Vec<Op> {
    let mut ops = lower_expr(arg, ctx);
    ops.push(Op::FieldLoad { layout: lay, field, kind: LoadKind::I32 });
    ops.push(Op::UnOp(U::I32Eqz));
    ops
}

/// `int.abs(n)` = `n < 0 ? -n : n`.
fn int_abs(arg: &IrExpr, ctx: &mut LowerCtx) -> Vec<Op> {
    let n = ctx.alloc_local(WasmTy::I64);
    let mut ops = lower_expr(arg, ctx);
    ops.push(Op::LocalSet(n));
    ops.push(Op::LocalGet(n));
    ops.push(Op::Const(Const::I64(0)));
    ops.push(Op::BinOp(B::I64LtS));
    ops.push(Op::If {
        ty: WasmTy::I64,
        then: vec![Op::Const(Const::I64(0)), Op::LocalGet(n), Op::BinOp(B::I64Sub)],
        else_: vec![Op::LocalGet(n)],
    });
    ops
}

/// `int.min`/`int.max` — `cmp(a, b) ? a : b` for the given comparison.
fn int_minmax(args: &[IrExpr], cmp: B, ctx: &mut LowerCtx) -> Vec<Op> {
    let a = ctx.alloc_local(WasmTy::I64);
    let b = ctx.alloc_local(WasmTy::I64);
    let mut ops = lower_expr(&args[0], ctx);
    ops.push(Op::LocalSet(a));
    ops.extend(lower_expr(&args[1], ctx));
    ops.push(Op::LocalSet(b));
    ops.push(Op::LocalGet(a));
    ops.push(Op::LocalGet(b));
    ops.push(Op::BinOp(cmp));
    ops.push(Op::If {
        ty: WasmTy::I64,
        then: vec![Op::LocalGet(a)],
        else_: vec![Op::LocalGet(b)],
    });
    ops
}

/// `list.get_or(xs, i, default)` — bounds-checked element read.
fn list_get_or(args: &[IrExpr], ret_ty: &Ty, ctx: &mut LowerCtx) -> Vec<Op> {
    let elem_wasm = ty_to_wasm(ret_ty);
    let es = wasm_byte_size(ret_ty);
    let xs = ctx.alloc_local(WasmTy::I32);
    let i = ctx.alloc_local(WasmTy::I32);

    let mut ops = lower_expr(&args[0], ctx);
    ops.push(Op::LocalSet(xs));
    ops.extend(lower_expr(&args[1], ctx)); // index (i64)
    ops.push(Op::UnOp(U::I32WrapI64));
    ops.push(Op::LocalSet(i));

    // cond: i <u len(xs)
    ops.push(Op::LocalGet(i));
    ops.push(Op::LocalGet(xs));
    ops.push(Op::FieldLoad { layout: layout::LIST, field: list::LEN, kind: LoadKind::I32 });
    ops.push(Op::BinOp(B::I32LtU));

    // then: load xs data + i*es ; else: default
    let then_ops = {
        let data_off = ctx.reg.fixed_offset(layout::LIST, list::DATA) as i32;
        vec![
            Op::LocalGet(xs),
            Op::Const(Const::I32(data_off)),
            Op::BinOp(B::I32Add),
            Op::LocalGet(i),
            Op::Const(Const::I32(es)),
            Op::BinOp(B::I32Mul),
            Op::BinOp(B::I32Add),
            Op::Load(load_kind_of(elem_wasm)),
        ]
    };
    let else_ops = lower_expr(&args[2], ctx);
    ops.push(Op::If { ty: elem_wasm, then: then_ops, else_: else_ops });
    ops
}

/// Lower args and call a named engine runtime function.
fn call_runtime(name: &str, args: &[IrExpr], pushes: u8, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let idx = (ctx.func_idx)(name)?;
    let mut ops = Vec::new();
    for arg in args {
        ops.extend(lower_expr(arg, ctx));
    }
    ops.push(Op::Call { idx, pops: args.len() as u8, pushes });
    Some(ops)
}
