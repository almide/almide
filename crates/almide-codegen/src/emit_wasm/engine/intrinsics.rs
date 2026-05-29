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
        // string.len counts UTF-8 code points (the documented semantics), not
        // bytes. list.len is the element count (the LEN field directly).
        "almide_rt_string_len" => Some(string_char_len(&args[0], ctx)),
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
        "almide_rt_list_any" if args.len() == 2 => list_any_all(&args[0], &args[1], true, ctx),
        "almide_rt_list_all" if args.len() == 2 => list_any_all(&args[0], &args[1], false, ctx),
        "almide_rt_list_count" if args.len() == 2 => list_count(&args[0], &args[1], ctx),
        "almide_rt_list_find" if args.len() == 2 => list_find(&args[0], &args[1], ret_ty, ctx),
        "almide_rt_list_reverse" if args.len() == 1 => list_reverse(&args[0], ret_ty, ctx),
        "almide_rt_list_filter_map" if args.len() == 2 => list_filter_map(&args[0], &args[1], ret_ty, ctx),
        "almide_rt_list_flat_map" if args.len() == 2 => list_flat_map(&args[0], &args[1], ret_ty, ctx),

        // ── Option / Result tag tests (tag @0: Some=1/None=0, Ok=0/Err=1) ──
        "almide_rt_option_is_some" => Some(load_tag(&args[0], ctx)),     // tag (1=Some)
        "almide_rt_result_is_err" => Some(load_tag(&args[0], ctx)),     // tag (1=Err)
        "almide_rt_option_is_none" => Some(tag_eqz(&args[0], ctx)),     // tag==0
        "almide_rt_result_is_ok" => Some(tag_eqz(&args[0], ctx)),      // tag==0 (Ok)
        // unwrap_or: Option keeps payload when tag != 0; Result when tag == 0.
        "almide_rt_option_unwrap_or" if args.len() == 2 =>
            Some(tagged_unwrap_or(&args[0], &args[1], true, ret_ty, ctx)),
        "almide_rt_result_unwrap_or" if args.len() == 2 =>
            Some(tagged_unwrap_or(&args[0], &args[1], false, ret_ty, ctx)),
        "almide_rt_option_map" if args.len() == 2 => option_map(&args[0], &args[1], ret_ty, ctx),
        "almide_rt_result_map" if args.len() == 2 => result_map(&args[0], &args[1], ret_ty, ctx),
        "almide_rt_result_map_err" if args.len() == 2 => result_map_err(&args[0], &args[1], ret_ty, ctx),
        "almide_rt_string_slice" if args.len() == 3 =>
            call_runtime("__string_slice", args, 1, ctx),
        "almide_rt_string_char_at" if args.len() == 2 =>
            call_runtime("__string_get", args, 1, ctx),

        // ── Map: Int or String keys; Int or pointer/i32 values (not Float). ──
        "almide_rt_map_new" if map_supported(ret_ty) =>
            (ctx.func_idx)("__map_new").map(|idx| vec![Op::Call { idx, pops: 0, pushes: 1 }]),
        "almide_rt_map_get" if args.len() == 2 && map_supported(&args[0].ty) =>
            map_get(&args[0], &args[1], ctx),
        "almide_rt_map_get_or" if args.len() == 3 && map_supported(&args[0].ty) =>
            map_get_or(&args[0], &args[1], &args[2], ctx),
        "almide_rt_map_set" if args.len() == 3 && map_supported(&args[0].ty) =>
            map_set_op(&args[0], &args[1], &args[2], ctx),
        "almide_rt_map_contains" if args.len() == 2 && map_supported(&args[0].ty) =>
            map_contains_op(&args[0], &args[1], ctx),
        "almide_rt_map_len" if args.len() == 1 && map_supported(&args[0].ty) =>
            call_runtime("__map_len", args, 1, ctx),
        "almide_rt_map_keys" if args.len() == 1 && map_supported(&args[0].ty) =>
            map_collect(&args[0], 0, ctx),
        "almide_rt_map_values" if args.len() == 1 && map_supported(&args[0].ty) =>
            map_collect(&args[0], 8, ctx),
        "almide_rt_map_remove" if args.len() == 2 && map_supported(&args[0].ty) =>
            map_remove_op(&args[0], &args[1], ctx),
        "almide_rt_map_merge" if args.len() == 2 && map_supported(&args[0].ty) =>
            map_merge_op(&args[0], &args[1], ctx),
        "almide_rt_map_map_values" if args.len() == 2 && map_supported(&args[0].ty) && map_supported(ret_ty) =>
            map_map_values(&args[0], &args[1], ret_ty, ctx),
        "almide_rt_list_sum" if args.len() == 1 => Some(list_sum(&args[0], ctx)),
        // sort: Int lists via the runtime selection sort; other element types
        // (Float/String/composite) fall back until typed comparators land.
        "almide_rt_list_sort" if args.len() == 1
            && matches!(super::lower::list_element_ty(&args[0].ty), Some(Ty::Int)) =>
            call_runtime("__list_sort_int", args, 1, ctx),
        "almide_rt_list_contains" if args.len() == 2 => list_contains(&args[0], &args[1], ctx),
        "almide_rt_string_starts_with" if args.len() == 2 => call_runtime("__string_starts_with", args, 1, ctx),
        "almide_rt_string_ends_with" if args.len() == 2 => call_runtime("__string_ends_with", args, 1, ctx),

        _ => None,
    }
}

/// `list.sum(xs: List[Int])` — accumulate i64 elements.
fn list_sum(xs_expr: &IrExpr, ctx: &mut LowerCtx) -> Vec<Op> {
    let (lp, mut ops) = list_loop_header(xs_expr, ctx);
    let acc = ctx.alloc_local(WasmTy::I64);
    ops.push(Op::Const(Const::I64(0)));
    ops.push(Op::LocalSet(acc));
    let mut loop_body = Vec::new();
    loop_body.push(Op::LocalGet(lp.idx));
    loop_body.push(Op::LocalGet(lp.len));
    loop_body.push(Op::BinOp(B::I32GeU));
    loop_body.push(Op::BrIf(1));
    loop_body.push(Op::LocalGet(acc));
    loop_body.extend(load_elem(lp.xs, lp.idx, 8, LoadKind::I64));
    loop_body.push(Op::BinOp(B::I64Add));
    loop_body.push(Op::LocalSet(acc));
    loop_body.push(Op::LocalGet(lp.idx));
    loop_body.push(Op::Const(Const::I32(1)));
    loop_body.push(Op::BinOp(B::I32Add));
    loop_body.push(Op::LocalSet(lp.idx));
    loop_body.push(Op::Br(0));
    ops.push(Op::Block(vec![Op::Loop(loop_body)]));
    ops.push(Op::LocalGet(acc));
    ops
}

/// `list.contains(xs, x)` — scan for an element equal to `x`. Supports scalar
/// elements (Int/Float/Bool) and String (deep eq); other element types fall
/// back (None).
fn list_contains(xs_expr: &IrExpr, x_expr: &IrExpr, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let elem_ty = super::lower::list_element_ty(&xs_expr.ty).unwrap_or(Ty::Int);
    let wt = ty_to_wasm(&elem_ty);
    let es = wasm_byte_size(&elem_ty);
    let lk = load_kind_of(wt);
    // Equality ops consuming [elem, x] → i32 bool, per element type.
    let eq: Vec<Op> = match &elem_ty {
        Ty::Int => vec![Op::BinOp(B::I64Eq)],
        Ty::Float => vec![Op::BinOp(B::F64Eq)],
        Ty::Bool => vec![Op::BinOp(B::I32Eq)],
        Ty::String => vec![Op::Call { idx: (ctx.func_idx)("__string_eq")?, pops: 2, pushes: 1 }],
        _ => return None,
    };

    let result = ctx.alloc_local(WasmTy::I32);
    let xval = ctx.alloc_local(wt);
    let elem = ctx.alloc_local(wt);
    let mut ops = lower_expr(x_expr, ctx);
    ops.push(Op::LocalSet(xval));
    let (lp, header) = list_loop_header(xs_expr, ctx);
    ops.extend(header);
    ops.push(Op::Const(Const::I32(0)));
    ops.push(Op::LocalSet(result));

    let mut loop_body = Vec::new();
    loop_body.push(Op::LocalGet(lp.idx));
    loop_body.push(Op::LocalGet(lp.len));
    loop_body.push(Op::BinOp(B::I32GeU));
    loop_body.push(Op::BrIf(1));
    loop_body.extend(load_elem(lp.xs, lp.idx, es, lk));
    loop_body.push(Op::LocalSet(elem));
    loop_body.push(Op::LocalGet(elem));
    loop_body.push(Op::LocalGet(xval));
    loop_body.extend(eq);
    loop_body.push(Op::IfVoid {
        then: vec![Op::Const(Const::I32(1)), Op::LocalSet(result), Op::Br(2)],
        else_: vec![],
    });
    loop_body.push(Op::LocalGet(lp.idx));
    loop_body.push(Op::Const(Const::I32(1)));
    loop_body.push(Op::BinOp(B::I32Add));
    loop_body.push(Op::LocalSet(lp.idx));
    loop_body.push(Op::Br(0));
    ops.push(Op::Block(vec![Op::Loop(loop_body)]));
    ops.push(Op::LocalGet(result));
    Some(ops)
}

/// `result.map(r, f)` — `Ok(f(x))` when Ok, the original `Err(e)` otherwise.
fn result_map(r: &IrExpr, f: &IrExpr, ret_ty: &Ty, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (pvar, pty, body) = inline_lambda(f, 1, ctx)?;
    let in_lk = load_kind_of(ty_to_wasm(&pty));
    let out_ok_ty = result_ok_ty(ret_ty).unwrap_or(Ty::Int);
    let out_sk = store_kind_of(ty_to_wasm(&out_ok_ty));
    // Err payload type (E) for passthrough copy.
    let err_ty = result_err_ty(&r.ty).unwrap_or(Ty::String);
    let err_lk = load_kind_of(ty_to_wasm(&err_ty));
    let err_sk = store_kind_of(ty_to_wasm(&err_ty));

    let r_local = ctx.alloc_local(WasmTy::I32);
    let out = ctx.alloc_local(WasmTy::I32);
    let elem = ctx.alloc_local(ty_to_wasm(&pty));
    ctx.map_var(pvar, elem);
    let alloc = (ctx.func_idx)("__alloc")?;

    let mut ops = lower_expr(r, ctx);
    ops.push(Op::LocalSet(r_local));
    ops.push(Op::Const(Const::I32(12)));
    ops.push(Op::Call { idx: alloc, pops: 1, pushes: 1 });
    ops.push(Op::LocalSet(out));

    // tag != 0 (Err): passthrough; tag == 0 (Ok): map.
    let err_branch = vec![
        Op::LocalGet(out), Op::Const(Const::I32(1)), Op::Store(StoreKind::I32),
        Op::LocalGet(out), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add),
        Op::LocalGet(r_local), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::Load(err_lk),
        Op::Store(err_sk),
    ];
    let ok_branch = {
        let mut t = vec![
            Op::LocalGet(r_local), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add),
            Op::Load(in_lk), Op::LocalSet(elem),
            Op::LocalGet(out), Op::Const(Const::I32(0)), Op::Store(StoreKind::I32),
            Op::LocalGet(out), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add),
        ];
        t.extend(lower_expr(&body, ctx));
        t.push(Op::Store(out_sk));
        t
    };
    ops.push(Op::LocalGet(r_local));
    ops.push(Op::Load(LoadKind::I32)); // tag
    ops.push(Op::IfVoid { then: err_branch, else_: ok_branch });
    ops.push(Op::LocalGet(out));
    Some(ops)
}

/// `result.map_err(r, f)` — `Err(f(e))` when Err, the original `Ok(x)` otherwise.
fn result_map_err(r: &IrExpr, f: &IrExpr, ret_ty: &Ty, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (pvar, pty, body) = inline_lambda(f, 1, ctx)?;
    let in_lk = load_kind_of(ty_to_wasm(&pty)); // E (incoming err)
    let out_err_ty = result_err_ty(ret_ty).unwrap_or(Ty::String); // F
    let out_sk = store_kind_of(ty_to_wasm(&out_err_ty));
    let ok_ty = result_ok_ty(&r.ty).unwrap_or(Ty::Int); // A (passthrough)
    let ok_lk = load_kind_of(ty_to_wasm(&ok_ty));
    let ok_sk = store_kind_of(ty_to_wasm(&ok_ty));

    let r_local = ctx.alloc_local(WasmTy::I32);
    let out = ctx.alloc_local(WasmTy::I32);
    let elem = ctx.alloc_local(ty_to_wasm(&pty));
    ctx.map_var(pvar, elem);
    let alloc = (ctx.func_idx)("__alloc")?;

    let mut ops = lower_expr(r, ctx);
    ops.push(Op::LocalSet(r_local));
    ops.push(Op::Const(Const::I32(12)));
    ops.push(Op::Call { idx: alloc, pops: 1, pushes: 1 });
    ops.push(Op::LocalSet(out));

    // tag != 0 (Err): map; tag == 0 (Ok): passthrough.
    let err_branch = {
        let mut t = vec![
            Op::LocalGet(r_local), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add),
            Op::Load(in_lk), Op::LocalSet(elem),
            Op::LocalGet(out), Op::Const(Const::I32(1)), Op::Store(StoreKind::I32),
            Op::LocalGet(out), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add),
        ];
        t.extend(lower_expr(&body, ctx));
        t.push(Op::Store(out_sk));
        t
    };
    let ok_branch = vec![
        Op::LocalGet(out), Op::Const(Const::I32(0)), Op::Store(StoreKind::I32),
        Op::LocalGet(out), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add),
        Op::LocalGet(r_local), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::Load(ok_lk),
        Op::Store(ok_sk),
    ];
    ops.push(Op::LocalGet(r_local));
    ops.push(Op::Load(LoadKind::I32)); // tag
    ops.push(Op::IfVoid { then: err_branch, else_: ok_branch });
    ops.push(Op::LocalGet(out));
    Some(ops)
}

fn result_ok_ty(ty: &Ty) -> Option<Ty> {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    match ty { Ty::Applied(TC::Result, a) if !a.is_empty() => Some(a[0].clone()), _ => None }
}
fn result_err_ty(ty: &Ty) -> Option<Ty> {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    match ty { Ty::Applied(TC::Result, a) if a.len() >= 2 => Some(a[1].clone()), _ => None }
}

/// Load a tagged-union tag (i32 at offset 0): Some=1/None=0, Err=1/Ok=0.
fn load_tag(arg: &IrExpr, ctx: &mut LowerCtx) -> Vec<Op> {
    let mut ops = lower_expr(arg, ctx);
    ops.push(Op::Load(LoadKind::I32));
    ops
}

/// `tag == 0` — None for Option, Ok for Result.
fn tag_eqz(arg: &IrExpr, ctx: &mut LowerCtx) -> Vec<Op> {
    let mut ops = load_tag(arg, ctx);
    ops.push(Op::UnOp(U::I32Eqz));
    ops
}

/// `unwrap_or(v, default)`. `payload_when_nonzero` selects which tag yields the
/// payload: Option → nonzero (Some); Result → zero (Ok).
fn tagged_unwrap_or(v: &IrExpr, default: &IrExpr, payload_when_nonzero: bool, ret_ty: &Ty, ctx: &mut LowerCtx) -> Vec<Op> {
    let wt = ty_to_wasm(ret_ty);
    let lk = load_kind_of(wt);
    let ptr = ctx.alloc_local(WasmTy::I32);
    let mut ops = lower_expr(v, ctx);
    ops.push(Op::LocalTee(ptr));
    ops.push(Op::Load(LoadKind::I32)); // tag
    let payload = vec![
        Op::LocalGet(ptr), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::Load(lk),
    ];
    let fallback = lower_expr(default, ctx);
    // If condition is the tag (nonzero → then). Place payload/fallback so the
    // payload corresponds to the right tag.
    let (then_ops, else_ops) = if payload_when_nonzero {
        (payload, fallback) // Option: Some(tag!=0) → payload
    } else {
        (fallback, payload) // Result: Err(tag!=0) → default, Ok → payload
    };
    ops.push(Op::If { ty: wt, then: then_ops, else_: else_ops });
    ops
}

/// `option.map(o, f)` — `Some(f(x))` when `o` is Some, else None.
fn option_map(o: &IrExpr, f: &IrExpr, ret_ty: &Ty, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (pvar, pty, body) = inline_lambda(f, 1, ctx)?;
    let in_lk = load_kind_of(ty_to_wasm(&pty));
    let out_ty = option_payload_ty(ret_ty).unwrap_or(Ty::Int);
    let out_sk = store_kind_of(ty_to_wasm(&out_ty));

    let o_local = ctx.alloc_local(WasmTy::I32);
    let out = ctx.alloc_local(WasmTy::I32);
    let elem = ctx.alloc_local(ty_to_wasm(&pty));
    ctx.map_var(pvar, elem);
    let alloc = (ctx.func_idx)("__alloc")?;

    let mut ops = lower_expr(o, ctx);
    ops.push(Op::LocalSet(o_local));
    // out = __alloc(12)
    ops.push(Op::Const(Const::I32(12)));
    ops.push(Op::Call { idx: alloc, pops: 1, pushes: 1 });
    ops.push(Op::LocalSet(out));

    // elem = o.payload (used only in the Some branch)
    let some_branch = {
        let mut t = vec![
            Op::LocalGet(o_local), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add),
            Op::Load(in_lk), Op::LocalSet(elem),
            // out.tag = 1
            Op::LocalGet(out), Op::Const(Const::I32(1)), Op::Store(StoreKind::I32),
            // out.payload = f(elem)
            Op::LocalGet(out), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add),
        ];
        t.extend(lower_expr(&body, ctx));
        t.push(Op::Store(out_sk));
        t
    };
    let none_branch = vec![Op::LocalGet(out), Op::Const(Const::I32(0)), Op::Store(StoreKind::I32)];

    ops.push(Op::LocalGet(o_local));
    ops.push(Op::Load(LoadKind::I32)); // tag (nonzero = Some)
    ops.push(Op::IfVoid { then: some_branch, else_: none_branch });
    ops.push(Op::LocalGet(out));
    Some(ops)
}

/// The map key kind code for the runtime: Int → 0, String → 1.
fn map_key_kind(k: &Ty) -> Option<i32> {
    match k {
        Ty::Int => Some(0),
        Ty::String => Some(1),
        _ => None,
    }
}

/// A value type the map runtime can store in an i64 slot: i32-width (pointers,
/// Bool, String) or i64 (Int). Float is excluded (would need a bitcast).
fn map_val_ok(v: &Ty) -> bool {
    matches!(ty_to_wasm(v), WasmTy::I32 | WasmTy::I64) && !matches!(v, Ty::Float)
}

/// `Map[K, V]` with a supported key kind and value type.
pub(super) fn map_supported(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    matches!(ty, Ty::Applied(TC::Map, a) if a.len() == 2
        && map_key_kind(&a[0]).is_some() && map_val_ok(&a[1]))
}

/// Extract (kind_const, key_ty, val_ty) from a supported `Map[K,V]`.
fn map_kv(ty: &Ty) -> Option<(i32, Ty, Ty)> {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    match ty {
        Ty::Applied(TC::Map, a) if a.len() == 2 =>
            Some((map_key_kind(&a[0])?, a[0].clone(), a[1].clone())),
        _ => None,
    }
}

/// Lower an expr and widen an i32-width result to i64 (map slots are i64).
fn lower_widened(e: &IrExpr, ty: &Ty, ctx: &mut LowerCtx) -> Vec<Op> {
    let mut ops = lower_expr(e, ctx);
    if matches!(ty_to_wasm(ty), WasmTy::I32) {
        ops.push(Op::UnOp(U::I64ExtendI32U));
    }
    ops
}

/// `map.get(m, k) -> Option[V]`.
fn map_get(m: &IrExpr, k: &IrExpr, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (kind, key_ty, _v) = map_kv(&m.ty)?;
    let idx = (ctx.func_idx)("__map_get")?;
    let mut ops = lower_expr(m, ctx);
    ops.extend(lower_widened(k, &key_ty, ctx));
    ops.push(Op::Const(Const::I32(kind)));
    ops.push(Op::Call { idx, pops: 3, pushes: 1 });
    Some(ops)
}

/// `map.get(m, k) ?? default -> V` (fused). Narrows the i64 result for i32 V.
fn map_get_or(m: &IrExpr, k: &IrExpr, default: &IrExpr, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (kind, key_ty, val_ty) = map_kv(&m.ty)?;
    let idx = (ctx.func_idx)("__map_get_or")?;
    let mut ops = lower_expr(m, ctx);
    ops.extend(lower_widened(k, &key_ty, ctx));
    ops.extend(lower_widened(default, &val_ty, ctx));
    ops.push(Op::Const(Const::I32(kind)));
    ops.push(Op::Call { idx, pops: 4, pushes: 1 });
    if matches!(ty_to_wasm(&val_ty), WasmTy::I32) {
        ops.push(Op::UnOp(U::I32WrapI64));
    }
    Some(ops)
}

/// `map.set(m, k, v) -> Map[K,V]`.
fn map_set_op(m: &IrExpr, k: &IrExpr, v: &IrExpr, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (kind, key_ty, val_ty) = map_kv(&m.ty)?;
    let idx = (ctx.func_idx)("__map_set")?;
    let mut ops = lower_expr(m, ctx);
    ops.extend(lower_widened(k, &key_ty, ctx));
    ops.extend(lower_widened(v, &val_ty, ctx));
    ops.push(Op::Const(Const::I32(kind)));
    ops.push(Op::Call { idx, pops: 4, pushes: 1 });
    Some(ops)
}

/// `map.keys(m) -> List[K]` (field_off 0) / `map.values(m) -> List[V]` (8).
fn map_collect(m: &IrExpr, field_off: i32, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (_kind, key_ty, val_ty) = map_kv(&m.ty)?;
    let idx = (ctx.func_idx)("__map_collect")?;
    let elem_size = wasm_byte_size(if field_off == 0 { &key_ty } else { &val_ty });
    let mut ops = lower_expr(m, ctx);
    ops.push(Op::Const(Const::I32(field_off)));
    ops.push(Op::Const(Const::I32(elem_size)));
    ops.push(Op::Call { idx, pops: 3, pushes: 1 });
    Some(ops)
}

/// `map.remove(m, k) -> Map[K,V]`.
fn map_remove_op(m: &IrExpr, k: &IrExpr, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (kind, key_ty, _v) = map_kv(&m.ty)?;
    let idx = (ctx.func_idx)("__map_remove")?;
    let mut ops = lower_expr(m, ctx);
    ops.extend(lower_widened(k, &key_ty, ctx));
    ops.push(Op::Const(Const::I32(kind)));
    ops.push(Op::Call { idx, pops: 3, pushes: 1 });
    Some(ops)
}

/// `map.map(m, f: (V) -> B) -> Map[K, B]` — transform each value, keys kept.
/// Iterates the source slots, lowering `f`'s body per entry, rebuilding via
/// __map_set (functional). Pre-sizing via __map_put would be faster but set is
/// safe on an initially-empty table.
fn map_map_values(m: &IrExpr, f: &IrExpr, ret_ty: &Ty, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (kind, _k, _v) = map_kv(&m.ty)?;
    let (pvar, pty, body) = inline_lambda(f, 1, ctx)?; // f: (V) -> B
    let in_lk = load_kind_of(ty_to_wasm(&pty));
    let in_narrow = matches!(ty_to_wasm(&pty), WasmTy::I32);
    let out_val_ty = map_kv(ret_ty)?.2; // B
    let out_widen = matches!(ty_to_wasm(&out_val_ty), WasmTy::I32);
    let (new_idx, set_idx) = ((ctx.func_idx)("__map_new")?, (ctx.func_idx)("__map_set")?);

    let m_l = ctx.alloc_local(WasmTy::I32);
    let cap_l = ctx.alloc_local(WasmTy::I32);
    let out_l = ctx.alloc_local(WasmTy::I32);
    let slot_l = ctx.alloc_local(WasmTy::I32);
    let ea = ctx.alloc_local(WasmTy::I32);
    let elem = ctx.alloc_local(ty_to_wasm(&pty));
    ctx.map_var(pvar, elem);

    let mut ops = lower_expr(m, ctx);
    ops.push(Op::LocalSet(m_l));
    ops.push(Op::LocalGet(m_l)); ops.push(Op::Const(Const::I32(4))); ops.push(Op::BinOp(B::I32Add)); ops.push(Op::Load(LoadKind::I32)); ops.push(Op::LocalSet(cap_l));
    ops.push(Op::Call { idx: new_idx, pops: 0, pushes: 1 }); ops.push(Op::LocalSet(out_l));
    ops.push(Op::Const(Const::I32(0))); ops.push(Op::LocalSet(slot_l));

    // entry addr = m + 8 + cap + slot*16
    let entry_addr = vec![
        Op::LocalGet(m_l), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::LocalGet(cap_l), Op::BinOp(B::I32Add),
        Op::LocalGet(slot_l), Op::Const(Const::I32(16)), Op::BinOp(B::I32Mul), Op::BinOp(B::I32Add),
    ];
    let mut on_occupied = entry_addr.clone();
    on_occupied.push(Op::LocalSet(ea));
    // elem = entry.val (narrow if i32)
    on_occupied.push(Op::LocalGet(ea)); on_occupied.push(Op::Const(Const::I32(8))); on_occupied.push(Op::BinOp(B::I32Add)); on_occupied.push(Op::Load(in_lk));
    if in_narrow { /* value already loaded at i32 width via in_lk */ }
    on_occupied.push(Op::LocalSet(elem));
    // out = __map_set(out, key, widen(f(elem)), kind)
    on_occupied.push(Op::LocalGet(out_l));
    on_occupied.push(Op::LocalGet(ea)); on_occupied.push(Op::Load(LoadKind::I64)); // key (i64)
    on_occupied.extend(lower_expr(&body, ctx));
    if out_widen { on_occupied.push(Op::UnOp(U::I64ExtendI32U)); }
    on_occupied.push(Op::Const(Const::I32(kind)));
    on_occupied.push(Op::Call { idx: set_idx, pops: 4, pushes: 1 });
    on_occupied.push(Op::LocalSet(out_l));

    let mut loop_body = vec![
        Op::LocalGet(slot_l), Op::LocalGet(cap_l), Op::BinOp(B::I32GeU), Op::BrIf(1),
        Op::LocalGet(m_l), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::LocalGet(slot_l), Op::BinOp(B::I32Add), Op::Load(LoadKind::U8),
    ];
    loop_body.push(Op::IfVoid { then: on_occupied, else_: vec![] });
    loop_body.extend([Op::LocalGet(slot_l), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(slot_l), Op::Br(0)]);
    ops.push(Op::Block(vec![Op::Loop(loop_body)]));
    ops.push(Op::LocalGet(out_l));
    Some(ops)
}

/// `map.merge(a, b) -> Map[K,V]` (b wins on duplicate keys).
fn map_merge_op(a: &IrExpr, b: &IrExpr, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (kind, _k, _v) = map_kv(&a.ty)?;
    let idx = (ctx.func_idx)("__map_merge")?;
    let mut ops = lower_expr(a, ctx);
    ops.extend(lower_expr(b, ctx));
    ops.push(Op::Const(Const::I32(kind)));
    ops.push(Op::Call { idx, pops: 3, pushes: 1 });
    Some(ops)
}

/// `map.contains(m, k) -> Bool`.
fn map_contains_op(m: &IrExpr, k: &IrExpr, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (kind, key_ty, _v) = map_kv(&m.ty)?;
    let idx = (ctx.func_idx)("__map_contains")?;
    let mut ops = lower_expr(m, ctx);
    ops.extend(lower_widened(k, &key_ty, ctx));
    ops.push(Op::Const(Const::I32(kind)));
    ops.push(Op::Call { idx, pops: 3, pushes: 1 });
    Some(ops)
}

/// Payload type of an `Option[T]` (None if not an Option).
fn option_payload_ty(ty: &Ty) -> Option<Ty> {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    match ty {
        Ty::Applied(TC::Option, args) if !args.is_empty() => Some(args[0].clone()),
        _ => None,
    }
}

/// `list.reverse(xs)` — new list with elements in reverse order.
fn list_reverse(xs_expr: &IrExpr, ret_ty: &Ty, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let elem_ty = super::lower::list_element_ty(ret_ty).unwrap_or(Ty::Int);
    let es = wasm_byte_size(&elem_ty);
    let lk = load_kind_of(ty_to_wasm(&elem_ty));
    let sk = store_kind_of(ty_to_wasm(&elem_ty));
    let (lp, mut ops) = list_loop_header(xs_expr, ctx);
    let out = ctx.alloc_local(WasmTy::I32);
    let alloc = (ctx.func_idx)("__alloc")?;
    ops.extend(alloc_list(out, lp.len, es, alloc));

    let mut loop_body = Vec::new();
    loop_body.push(Op::LocalGet(lp.idx));
    loop_body.push(Op::LocalGet(lp.len));
    loop_body.push(Op::BinOp(B::I32GeU));
    loop_body.push(Op::BrIf(1));
    // out[idx] = xs[len-1-idx]
    loop_body.push(Op::LocalGet(out));
    loop_body.push(Op::Const(Const::I32(8)));
    loop_body.push(Op::BinOp(B::I32Add));
    loop_body.push(Op::LocalGet(lp.idx));
    loop_body.push(Op::Const(Const::I32(es)));
    loop_body.push(Op::BinOp(B::I32Mul));
    loop_body.push(Op::BinOp(B::I32Add));
    // src = xs + 8 + (len-1-idx)*es
    loop_body.push(Op::LocalGet(lp.xs));
    loop_body.push(Op::Const(Const::I32(8)));
    loop_body.push(Op::BinOp(B::I32Add));
    loop_body.push(Op::LocalGet(lp.len));
    loop_body.push(Op::Const(Const::I32(1)));
    loop_body.push(Op::BinOp(B::I32Sub));
    loop_body.push(Op::LocalGet(lp.idx));
    loop_body.push(Op::BinOp(B::I32Sub));
    loop_body.push(Op::Const(Const::I32(es)));
    loop_body.push(Op::BinOp(B::I32Mul));
    loop_body.push(Op::BinOp(B::I32Add));
    loop_body.push(Op::Load(lk));
    loop_body.push(Op::Store(sk));
    loop_body.push(Op::LocalGet(lp.idx));
    loop_body.push(Op::Const(Const::I32(1)));
    loop_body.push(Op::BinOp(B::I32Add));
    loop_body.push(Op::LocalSet(lp.idx));
    loop_body.push(Op::Br(0));
    ops.push(Op::Block(vec![Op::Loop(loop_body)]));
    ops.push(Op::LocalGet(out));
    Some(ops)
}

/// `list.filter_map(xs, f)` — apply `f: (A) -> Option[B]`, keep the Some values.
fn list_filter_map(xs_expr: &IrExpr, f: &IrExpr, ret_ty: &Ty, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (pvar, pty, body) = inline_lambda(f, 1, ctx)?;
    let in_es = wasm_byte_size(&pty);
    let in_lk = load_kind_of(ty_to_wasm(&pty));
    let out_ty = super::lower::list_element_ty(ret_ty).unwrap_or(Ty::Int);
    let out_es = wasm_byte_size(&out_ty);
    let out_sk = store_kind_of(ty_to_wasm(&out_ty));
    let out_lk = load_kind_of(ty_to_wasm(&out_ty));

    let (lp, mut ops) = list_loop_header(xs_expr, ctx);
    let out = ctx.alloc_local(WasmTy::I32);
    let oc = ctx.alloc_local(WasmTy::I32);
    let elem = ctx.alloc_local(ty_to_wasm(&pty));
    let opt = ctx.alloc_local(WasmTy::I32);
    ctx.map_var(pvar, elem);
    let alloc = (ctx.func_idx)("__alloc")?;
    ops.extend(alloc_list(out, lp.len, out_es, alloc)); // worst-case capacity
    ops.push(Op::Const(Const::I32(0)));
    ops.push(Op::LocalSet(oc));

    // on Some: out[oc] = opt.payload; oc++
    let keep = vec![
        Op::LocalGet(out), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
        Op::LocalGet(oc), Op::Const(Const::I32(out_es)), Op::BinOp(B::I32Mul), Op::BinOp(B::I32Add),
        Op::LocalGet(opt), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::Load(out_lk),
        Op::Store(out_sk),
        Op::LocalGet(oc), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(oc),
    ];
    let mut loop_body = Vec::new();
    loop_body.push(Op::LocalGet(lp.idx));
    loop_body.push(Op::LocalGet(lp.len));
    loop_body.push(Op::BinOp(B::I32GeU));
    loop_body.push(Op::BrIf(1));
    loop_body.extend(load_elem(lp.xs, lp.idx, in_es, in_lk));
    loop_body.push(Op::LocalSet(elem));
    loop_body.extend(lower_expr(&body, ctx)); // f(elem) → Option ptr
    loop_body.push(Op::LocalSet(opt));
    loop_body.push(Op::LocalGet(opt));
    loop_body.push(Op::Load(LoadKind::I32)); // tag (nonzero = Some)
    loop_body.push(Op::IfVoid { then: keep, else_: vec![] });
    loop_body.push(Op::LocalGet(lp.idx));
    loop_body.push(Op::Const(Const::I32(1)));
    loop_body.push(Op::BinOp(B::I32Add));
    loop_body.push(Op::LocalSet(lp.idx));
    loop_body.push(Op::Br(0));
    ops.push(Op::Block(vec![Op::Loop(loop_body)]));
    ops.extend(set_list_len(out, oc));
    ops.push(Op::LocalGet(out));
    Some(ops)
}

/// `list.flat_map(xs, f)` — `f: (A) -> List[B]`, concatenating all results.
/// Built incrementally via __list_concat (accumulator starts empty).
fn list_flat_map(xs_expr: &IrExpr, f: &IrExpr, ret_ty: &Ty, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (pvar, pty, body) = inline_lambda(f, 1, ctx)?;
    let in_es = wasm_byte_size(&pty);
    let in_lk = load_kind_of(ty_to_wasm(&pty));
    let out_ty = super::lower::list_element_ty(ret_ty).unwrap_or(Ty::Int);
    let out_es = wasm_byte_size(&out_ty);

    let (lp, mut ops) = list_loop_header(xs_expr, ctx);
    let out = ctx.alloc_local(WasmTy::I32);
    let sub = ctx.alloc_local(WasmTy::I32);
    let elem = ctx.alloc_local(ty_to_wasm(&pty));
    ctx.map_var(pvar, elem);
    let alloc = (ctx.func_idx)("__alloc")?;
    let concat = (ctx.func_idx)("__list_concat")?;

    // out = empty list (alloc header only, len = cap = 0)
    ops.push(Op::Const(Const::I32(8)));
    ops.push(Op::Call { idx: alloc, pops: 1, pushes: 1 });
    ops.push(Op::LocalSet(out));
    ops.push(Op::LocalGet(out));
    ops.push(Op::Const(Const::I32(0)));
    ops.push(Op::Store(StoreKind::I32));
    ops.push(Op::LocalGet(out));
    ops.push(Op::Const(Const::I32(4)));
    ops.push(Op::BinOp(B::I32Add));
    ops.push(Op::Const(Const::I32(0)));
    ops.push(Op::Store(StoreKind::I32));

    let mut loop_body = Vec::new();
    loop_body.push(Op::LocalGet(lp.idx));
    loop_body.push(Op::LocalGet(lp.len));
    loop_body.push(Op::BinOp(B::I32GeU));
    loop_body.push(Op::BrIf(1));
    loop_body.extend(load_elem(lp.xs, lp.idx, in_es, in_lk));
    loop_body.push(Op::LocalSet(elem));
    loop_body.extend(lower_expr(&body, ctx)); // f(elem) → List[B] ptr
    loop_body.push(Op::LocalSet(sub));
    // out = __list_concat(out, sub, out_es)
    loop_body.push(Op::LocalGet(out));
    loop_body.push(Op::LocalGet(sub));
    loop_body.push(Op::Const(Const::I32(out_es)));
    loop_body.push(Op::Call { idx: concat, pops: 3, pushes: 1 });
    loop_body.push(Op::LocalSet(out));
    loop_body.push(Op::LocalGet(lp.idx));
    loop_body.push(Op::Const(Const::I32(1)));
    loop_body.push(Op::BinOp(B::I32Add));
    loop_body.push(Op::LocalSet(lp.idx));
    loop_body.push(Op::Br(0));
    ops.push(Op::Block(vec![Op::Loop(loop_body)]));
    ops.push(Op::LocalGet(out));
    Some(ops)
}

/// `out = __alloc(8 + count*elem_size); out.len = out.cap = count`.
fn alloc_list(out: u32, count: u32, es: i32, alloc: super::ir::FuncIdx) -> Vec<Op> {
    vec![
        Op::Const(Const::I32(8)),
        Op::LocalGet(count),
        Op::Const(Const::I32(es)),
        Op::BinOp(B::I32Mul),
        Op::BinOp(B::I32Add),
        Op::Call { idx: alloc, pops: 1, pushes: 1 },
        Op::LocalSet(out),
        Op::LocalGet(out), Op::LocalGet(count), Op::Store(StoreKind::I32),
        Op::LocalGet(out), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add),
        Op::LocalGet(count), Op::Store(StoreKind::I32),
    ]
}

/// `out.len = out.cap = count` (used after a filtering pass).
fn set_list_len(out: u32, count: u32) -> Vec<Op> {
    vec![
        Op::LocalGet(out), Op::LocalGet(count), Op::Store(StoreKind::I32),
        Op::LocalGet(out), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add),
        Op::LocalGet(count), Op::Store(StoreKind::I32),
    ]
}

/// `list.find(xs, pred) -> Option[A]` — first matching element wrapped in Some,
/// else None. Option layout `[tag:i32 @0][payload @4]` (tag 1=Some, 0=None).
fn list_find(xs_expr: &IrExpr, f: &IrExpr, _ret_ty: &Ty, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (pvar, pty, body) = inline_lambda(f, 1, ctx)?;
    let es = wasm_byte_size(&pty);
    let lk = load_kind_of(ty_to_wasm(&pty));
    let sk = store_kind_of(ty_to_wasm(&pty));
    let (lp, mut ops) = list_loop_header(xs_expr, ctx);
    let opt = ctx.alloc_local(WasmTy::I32);
    let elem = ctx.alloc_local(ty_to_wasm(&pty));
    ctx.map_var(pvar, elem);

    // opt = __alloc(12); opt.tag = 0 (None by default)
    let alloc = (ctx.func_idx)("__alloc")?;
    ops.push(Op::Const(Const::I32(12)));
    ops.push(Op::Call { idx: alloc, pops: 1, pushes: 1 });
    ops.push(Op::LocalSet(opt));
    ops.push(Op::LocalGet(opt));
    ops.push(Op::Const(Const::I32(0)));
    ops.push(Op::Store(StoreKind::I32));

    // on match: opt.tag = 1; opt.payload = elem; break
    let on_match = vec![
        Op::LocalGet(opt), Op::Const(Const::I32(1)), Op::Store(StoreKind::I32),
        Op::LocalGet(opt), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add),
        Op::LocalGet(elem), Op::Store(sk),
        Op::Br(2),
    ];
    let mut loop_body = Vec::new();
    loop_body.push(Op::LocalGet(lp.idx));
    loop_body.push(Op::LocalGet(lp.len));
    loop_body.push(Op::BinOp(B::I32GeU));
    loop_body.push(Op::BrIf(1));
    loop_body.extend(load_elem(lp.xs, lp.idx, es, lk));
    loop_body.push(Op::LocalSet(elem));
    loop_body.extend(lower_expr(&body, ctx));
    loop_body.push(Op::IfVoid { then: on_match, else_: vec![] });
    loop_body.push(Op::LocalGet(lp.idx));
    loop_body.push(Op::Const(Const::I32(1)));
    loop_body.push(Op::BinOp(B::I32Add));
    loop_body.push(Op::LocalSet(lp.idx));
    loop_body.push(Op::Br(0));
    ops.push(Op::Block(vec![Op::Loop(loop_body)]));
    ops.push(Op::LocalGet(opt));
    Some(ops)
}

/// Build the per-element predicate loop for any/all/count.
///
/// Sets up the list header + element local, binds the lambda param, runs the
/// predicate body each iteration (negated when `negate`), and on a truthy
/// result runs `on_match` inside an `IfVoid`. Branch depth to the outer Block
/// from inside `on_match` is 2 (If→Loop→Block) — used for early break.
/// Returns the assembled ops (caller pre-initialises and post-reads its locals).
fn predicate_loop(
    xs_expr: &IrExpr, f: &IrExpr, negate: bool, on_match: Vec<Op>, ctx: &mut LowerCtx,
) -> Option<Vec<Op>> {
    let (pvar, pty, body) = inline_lambda(f, 1, ctx)?;
    let es = wasm_byte_size(&pty);
    let lk = load_kind_of(ty_to_wasm(&pty));
    let (lp, mut ops) = list_loop_header(xs_expr, ctx);
    let elem = ctx.alloc_local(ty_to_wasm(&pty));
    ctx.map_var(pvar, elem);

    let mut loop_body = Vec::new();
    loop_body.push(Op::LocalGet(lp.idx));
    loop_body.push(Op::LocalGet(lp.len));
    loop_body.push(Op::BinOp(B::I32GeU));
    loop_body.push(Op::BrIf(1));
    loop_body.extend(load_elem(lp.xs, lp.idx, es, lk));
    loop_body.push(Op::LocalSet(elem));
    loop_body.extend(lower_expr(&body, ctx)); // predicate → i32 cond
    if negate {
        loop_body.push(Op::UnOp(U::I32Eqz));
    }
    loop_body.push(Op::IfVoid { then: on_match, else_: vec![] });
    loop_body.push(Op::LocalGet(lp.idx));
    loop_body.push(Op::Const(Const::I32(1)));
    loop_body.push(Op::BinOp(B::I32Add));
    loop_body.push(Op::LocalSet(lp.idx));
    loop_body.push(Op::Br(0));
    ops.push(Op::Block(vec![Op::Loop(loop_body)]));
    Some(ops)
}

/// `list.any` / `list.all`. `any`: result starts 0, first match sets 1 + breaks.
/// `all`: result starts 1, first non-match (negated predicate) sets 0 + breaks.
fn list_any_all(xs_expr: &IrExpr, f: &IrExpr, is_any: bool, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let result = ctx.alloc_local(WasmTy::I32);
    let set_val = if is_any { 1 } else { 0 };
    let on_match = vec![Op::Const(Const::I32(set_val)), Op::LocalSet(result), Op::Br(2)];
    let mut ops = vec![
        Op::Const(Const::I32(if is_any { 0 } else { 1 })),
        Op::LocalSet(result),
    ];
    ops.extend(predicate_loop(xs_expr, f, !is_any, on_match, ctx)?);
    ops.push(Op::LocalGet(result));
    Some(ops)
}

/// `list.count(xs, pred)` — number of matching elements (as Int/i64).
fn list_count(xs_expr: &IrExpr, f: &IrExpr, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let c = ctx.alloc_local(WasmTy::I32);
    let on_match = vec![
        Op::LocalGet(c), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(c),
    ];
    let mut ops = vec![Op::Const(Const::I32(0)), Op::LocalSet(c)];
    ops.extend(predicate_loop(xs_expr, f, false, on_match, ctx)?);
    ops.push(Op::LocalGet(c));
    ops.push(Op::UnOp(U::I64ExtendI32U));
    Some(ops)
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
    let (pvar, pty, body) = inline_lambda(f, 1, ctx)?;
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
    loop_body.extend(lower_expr(&body, ctx));
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
    let (pvar, pty, body) = inline_lambda(f, 1, ctx)?;
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
    loop_body.extend(lower_expr(&body, ctx)); // predicate → i32 bool
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
    let (params, body) = inline_lambda_n(f, 2, ctx)?;
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
    loop_body.extend(lower_expr(&body, ctx));
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

/// Resolve an inline lambda with exactly `n` params from `f`, which may be a
/// `Lambda` directly or a `Var` bound to one (ClosureConversion hoists
/// non-capturing lambdas to `let`s). Returns owned (params, body).
fn inline_lambda_n(f: &IrExpr, n: usize, ctx: &LowerCtx) -> Option<(Vec<(almide_ir::VarId, Ty)>, IrExpr)> {
    let lambda = match &f.kind {
        almide_ir::IrExprKind::Lambda { .. } => f,
        almide_ir::IrExprKind::Var { id } => ctx.lambda_binds.get(id)?,
        _ => return None, // ClosureCreate / fn-ref args are not inlined (yet)
    };
    match &lambda.kind {
        almide_ir::IrExprKind::Lambda { params, body, .. } if params.len() == n => {
            Some((params.clone(), (**body).clone()))
        }
        _ => None,
    }
}

/// Single-param convenience wrapper around `inline_lambda_n`.
fn inline_lambda(f: &IrExpr, n: usize, ctx: &LowerCtx) -> Option<(almide_ir::VarId, Ty, IrExpr)> {
    let (params, body) = inline_lambda_n(f, n, ctx)?;
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

/// `string.len` — count UTF-8 code points (bytes whose top two bits are not
/// `10`, i.e. not continuation bytes), widened to i64.
fn string_char_len(arg: &IrExpr, ctx: &mut LowerCtx) -> Vec<Op> {
    let p = ctx.alloc_local(WasmTy::I32);
    let bl = ctx.alloc_local(WasmTy::I32);
    let cnt = ctx.alloc_local(WasmTy::I32);
    let i = ctx.alloc_local(WasmTy::I32);
    let mut ops = lower_expr(arg, ctx);
    ops.push(Op::LocalSet(p));
    ops.push(Op::LocalGet(p));
    ops.push(Op::FieldLoad { layout: layout::STRING, field: string::LEN, kind: LoadKind::I32 });
    ops.push(Op::LocalSet(bl));
    ops.push(Op::Const(Const::I32(0)));
    ops.push(Op::LocalSet(cnt));
    ops.push(Op::Const(Const::I32(0)));
    ops.push(Op::LocalSet(i));

    let bump = vec![Op::LocalGet(cnt), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(cnt)];
    let mut loop_body = Vec::new();
    loop_body.push(Op::LocalGet(i));
    loop_body.push(Op::LocalGet(bl));
    loop_body.push(Op::BinOp(B::I32GeU));
    loop_body.push(Op::BrIf(1));
    // b = byte[p + 8 + i] ; if (b & 0xC0) != 0x80 → count++
    loop_body.push(Op::LocalGet(p));
    loop_body.push(Op::Const(Const::I32(8)));
    loop_body.push(Op::BinOp(B::I32Add));
    loop_body.push(Op::LocalGet(i));
    loop_body.push(Op::BinOp(B::I32Add));
    loop_body.push(Op::Load(LoadKind::U8));
    loop_body.push(Op::Const(Const::I32(0xC0)));
    loop_body.push(Op::BinOp(B::I32And));
    loop_body.push(Op::Const(Const::I32(0x80)));
    loop_body.push(Op::BinOp(B::I32Ne));
    loop_body.push(Op::IfVoid { then: bump, else_: vec![] });
    loop_body.push(Op::LocalGet(i));
    loop_body.push(Op::Const(Const::I32(1)));
    loop_body.push(Op::BinOp(B::I32Add));
    loop_body.push(Op::LocalSet(i));
    loop_body.push(Op::Br(0));
    ops.push(Op::Block(vec![Op::Loop(loop_body)]));
    ops.push(Op::LocalGet(cnt));
    ops.push(Op::UnOp(U::I64ExtendI32U));
    ops
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
