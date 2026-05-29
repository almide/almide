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

use super::ir::{Op, Const, WasmTy, LoadKind, StoreKind, BinOp as B, UnOp as U};
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

        // ── Tier 3: higher-order with an inline (non-capturing) lambda ──
        "almide_rt_list_map" if args.len() == 2 => list_map(&args[0], &args[1], ret_ty, ctx),
        "almide_rt_list_filter" if args.len() == 2 => list_filter(&args[0], &args[1], ret_ty, ctx),
        "almide_rt_list_fold" if args.len() == 3 => list_fold(&args[0], &args[1], &args[2], ret_ty, ctx),

        _ => None,
    }
}

/// Extract a single-`Op::Block(Loop)` over a list with the standard header:
/// sets up `xs`/`len`/`idx` locals, emits the bounds check, and runs `body`
/// (which sees the element address computation already done into `elem`).
///
/// Returns (xs_local, idx_local, len_local).
struct ListLoop { xs: u32, idx: u32, len: u32 }

fn list_loop_header(xs_expr: &IrExpr, ctx: &mut LowerCtx) -> (ListLoop, Vec<Op>) {
    let xs = ctx.alloc_local(WasmTy::I32);
    let idx = ctx.alloc_local(WasmTy::I32);
    let len = ctx.alloc_local(WasmTy::I32);
    let mut ops = lower_expr(xs_expr, ctx);
    ops.push(Op::LocalSet(xs));
    ops.push(Op::LocalGet(xs));
    ops.push(Op::FieldLoad { layout: layout::LIST, field: list::LEN, kind: LoadKind::I32 });
    ops.push(Op::LocalSet(len));
    ops.push(Op::Const(Const::I32(0)));
    ops.push(Op::LocalSet(idx));
    (ListLoop { xs, idx, len }, ops)
}

/// Load `xs[idx]` (element width `es`, kind `lk`) and push its address-free value.
fn load_elem(xs: u32, idx: u32, es: i32, lk: LoadKind) -> Vec<Op> {
    vec![
        Op::LocalGet(xs),
        Op::Const(Const::I32(8)),
        Op::BinOp(B::I32Add),
        Op::LocalGet(idx),
        Op::Const(Const::I32(es)),
        Op::BinOp(B::I32Mul),
        Op::BinOp(B::I32Add),
        Op::Load(lk),
    ]
}

/// `list.map(xs, f)` with an inline lambda `f = (p) => body`. Builds a new list
/// of the same length, applying the lowered body per element.
fn list_map(xs_expr: &IrExpr, f: &IrExpr, ret_ty: &Ty, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (pvar, pty, body) = inline_lambda(f, 1)?;
    let in_es = super::lower::wasm_byte_size(&pty);
    let in_lk = load_kind_of(ty_to_wasm(&pty));
    let out_ty = super::lower::list_element_ty(ret_ty).unwrap_or(Ty::Int);
    let out_es = super::lower::wasm_byte_size(&out_ty);
    let out_sk = store_kind_of(ty_to_wasm(&out_ty));

    let (lp, mut ops) = list_loop_header(xs_expr, ctx);
    let out = ctx.alloc_local(WasmTy::I32);
    let elem = ctx.alloc_local(ty_to_wasm(&pty));
    ctx.map_var(pvar, elem);

    // out = __alloc(8 + len*out_es); out.len = out.cap = len
    let alloc = (ctx.func_idx)("__alloc")?;
    ops.push(Op::Const(Const::I32(8)));
    ops.push(Op::LocalGet(lp.len));
    ops.push(Op::Const(Const::I32(out_es)));
    ops.push(Op::BinOp(B::I32Mul));
    ops.push(Op::BinOp(B::I32Add));
    ops.push(Op::Call { idx: alloc, pops: 1, pushes: 1 });
    ops.push(Op::LocalSet(out));
    ops.push(Op::LocalGet(out));
    ops.push(Op::LocalGet(lp.len));
    ops.push(Op::Store(StoreKind::I32));
    ops.push(Op::LocalGet(out));
    ops.push(Op::Const(Const::I32(4)));
    ops.push(Op::BinOp(B::I32Add));
    ops.push(Op::LocalGet(lp.len));
    ops.push(Op::Store(StoreKind::I32));

    let mut loop_body = Vec::new();
    loop_body.push(Op::LocalGet(lp.idx));
    loop_body.push(Op::LocalGet(lp.len));
    loop_body.push(Op::BinOp(B::I32GeU));
    loop_body.push(Op::BrIf(1));
    // elem = xs[idx]
    loop_body.extend(load_elem(lp.xs, lp.idx, in_es, in_lk));
    loop_body.push(Op::LocalSet(elem));
    // out_addr = out + 8 + idx*out_es ; then body value ; store
    loop_body.push(Op::LocalGet(out));
    loop_body.push(Op::Const(Const::I32(8)));
    loop_body.push(Op::BinOp(B::I32Add));
    loop_body.push(Op::LocalGet(lp.idx));
    loop_body.push(Op::Const(Const::I32(out_es)));
    loop_body.push(Op::BinOp(B::I32Mul));
    loop_body.push(Op::BinOp(B::I32Add));
    loop_body.extend(lower_expr(body, ctx));
    loop_body.push(Op::Store(out_sk));
    // idx++
    loop_body.push(Op::LocalGet(lp.idx));
    loop_body.push(Op::Const(Const::I32(1)));
    loop_body.push(Op::BinOp(B::I32Add));
    loop_body.push(Op::LocalSet(lp.idx));
    loop_body.push(Op::Br(0));

    ops.push(Op::Block(vec![Op::Loop(loop_body)]));
    ops.push(Op::LocalGet(out));
    Some(ops)
}

/// `list.filter(xs, pred)` with an inline lambda `pred = (p) => bool`.
/// Over-allocates to the input length, then fixes len/cap to the match count.
fn list_filter(xs_expr: &IrExpr, f: &IrExpr, ret_ty: &Ty, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (pvar, pty, body) = inline_lambda(f, 1)?;
    let es = super::lower::wasm_byte_size(&pty);
    let lk = load_kind_of(ty_to_wasm(&pty));
    let sk = store_kind_of(ty_to_wasm(&pty));
    let _ = ret_ty;

    let (lp, mut ops) = list_loop_header(xs_expr, ctx);
    let out = ctx.alloc_local(WasmTy::I32);
    let oc = ctx.alloc_local(WasmTy::I32); // matched count
    let elem = ctx.alloc_local(ty_to_wasm(&pty));
    ctx.map_var(pvar, elem);

    // out = __alloc(8 + len*es) (worst case); oc = 0
    let alloc = (ctx.func_idx)("__alloc")?;
    ops.push(Op::Const(Const::I32(8)));
    ops.push(Op::LocalGet(lp.len));
    ops.push(Op::Const(Const::I32(es)));
    ops.push(Op::BinOp(B::I32Mul));
    ops.push(Op::BinOp(B::I32Add));
    ops.push(Op::Call { idx: alloc, pops: 1, pushes: 1 });
    ops.push(Op::LocalSet(out));
    ops.push(Op::Const(Const::I32(0)));
    ops.push(Op::LocalSet(oc));

    // store-if-match body: out[oc*es] = elem; oc++
    let keep = vec![
        Op::LocalGet(out), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
        Op::LocalGet(oc), Op::Const(Const::I32(es)), Op::BinOp(B::I32Mul), Op::BinOp(B::I32Add),
        Op::LocalGet(elem),
        Op::Store(sk),
        Op::LocalGet(oc), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(oc),
    ];

    let mut loop_body = Vec::new();
    loop_body.push(Op::LocalGet(lp.idx));
    loop_body.push(Op::LocalGet(lp.len));
    loop_body.push(Op::BinOp(B::I32GeU));
    loop_body.push(Op::BrIf(1));
    loop_body.extend(load_elem(lp.xs, lp.idx, es, lk));
    loop_body.push(Op::LocalSet(elem));
    loop_body.extend(lower_expr(body, ctx)); // predicate → i32 bool
    loop_body.push(Op::IfVoid { then: keep, else_: vec![] });
    loop_body.push(Op::LocalGet(lp.idx));
    loop_body.push(Op::Const(Const::I32(1)));
    loop_body.push(Op::BinOp(B::I32Add));
    loop_body.push(Op::LocalSet(lp.idx));
    loop_body.push(Op::Br(0));
    ops.push(Op::Block(vec![Op::Loop(loop_body)]));

    // out.len = out.cap = oc
    ops.push(Op::LocalGet(out));
    ops.push(Op::LocalGet(oc));
    ops.push(Op::Store(StoreKind::I32));
    ops.push(Op::LocalGet(out));
    ops.push(Op::Const(Const::I32(4)));
    ops.push(Op::BinOp(B::I32Add));
    ops.push(Op::LocalGet(oc));
    ops.push(Op::Store(StoreKind::I32));
    ops.push(Op::LocalGet(out));
    Some(ops)
}

/// `list.fold(xs, init, f)` with an inline lambda `f = (acc, elem) => body`.
fn list_fold(xs_expr: &IrExpr, init: &IrExpr, f: &IrExpr, ret_ty: &Ty, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (params, body) = inline_lambda_n(f, 2)?;
    let (acc_var, acc_ty) = params[0].clone();
    let (elem_var, elem_ty) = params[1].clone();
    let acc_wasm = ty_to_wasm(ret_ty);
    let in_es = super::lower::wasm_byte_size(&elem_ty);
    let in_lk = load_kind_of(ty_to_wasm(&elem_ty));

    let acc = ctx.alloc_local(acc_wasm);
    let elem = ctx.alloc_local(ty_to_wasm(&elem_ty));
    let _ = acc_ty;
    let mut ops = lower_expr(init, ctx);
    ops.push(Op::LocalSet(acc));
    let (lp, header) = list_loop_header(xs_expr, ctx);
    ops.extend(header);
    ctx.map_var(acc_var, acc);
    ctx.map_var(elem_var, elem);

    let mut loop_body = Vec::new();
    loop_body.push(Op::LocalGet(lp.idx));
    loop_body.push(Op::LocalGet(lp.len));
    loop_body.push(Op::BinOp(B::I32GeU));
    loop_body.push(Op::BrIf(1));
    loop_body.extend(load_elem(lp.xs, lp.idx, in_es, in_lk));
    loop_body.push(Op::LocalSet(elem));
    // acc = body(acc, elem)
    loop_body.extend(lower_expr(body, ctx));
    loop_body.push(Op::LocalSet(acc));
    loop_body.push(Op::LocalGet(lp.idx));
    loop_body.push(Op::Const(Const::I32(1)));
    loop_body.push(Op::BinOp(B::I32Add));
    loop_body.push(Op::LocalSet(lp.idx));
    loop_body.push(Op::Br(0));

    ops.push(Op::Block(vec![Op::Loop(loop_body)]));
    ops.push(Op::LocalGet(acc));
    Some(ops)
}

/// Match an inline lambda with exactly `n` params; return (params, body).
fn inline_lambda_n(f: &IrExpr, n: usize) -> Option<(Vec<(almide_ir::VarId, Ty)>, &IrExpr)> {
    match &f.kind {
        almide_ir::IrExprKind::Lambda { params, body, .. } if params.len() == n => {
            Some((params.clone(), body))
        }
        _ => None, // ClosureCreate / fn-ref args are not inlined (yet)
    }
}

/// Single-param convenience wrapper around `inline_lambda_n`.
fn inline_lambda(f: &IrExpr, n: usize) -> Option<(almide_ir::VarId, Ty, &IrExpr)> {
    let (params, body) = inline_lambda_n(f, n)?;
    let (v, t) = params[0].clone();
    Some((v, t, body))
}

fn store_kind_of(wt: WasmTy) -> StoreKind {
    use StoreKind as SK;
    match wt {
        WasmTy::I64 => SK::I64,
        WasmTy::F64 => SK::F64,
        WasmTy::F32 => SK::F32,
        WasmTy::I32 => SK::I32,
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
