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
        "almide_rt_float_to_string" => call_runtime("__float_to_string", &args[0..1], 1, ctx),
        // float.floor / ceil — direct f64 ops
        "almide_rt_float_floor" if args.len() == 1 => {
            let mut o = lower_expr(&args[0], ctx); o.push(Op::UnOp(U::F64Floor)); Some(o)
        }
        "almide_rt_float_ceil" if args.len() == 1 => {
            let mut o = lower_expr(&args[0], ctx); o.push(Op::UnOp(U::F64Ceil)); Some(o)
        }
        // int.parse(s) -> Result[Int, String]
        "almide_rt_int_parse" if args.len() == 1 => int_parse(&args[0], ctx),
        // string.lines(s) -> List[String]
        "almide_rt_string_lines" if args.len() == 1 => call_runtime("__string_lines", args, 1, ctx),
        "almide_rt_io_print" if args.len() == 1 => call_runtime("__print", args, 0, ctx),

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
        "almide_rt_option_to_list" if args.len() == 1 => option_to_list(&args[0], ret_ty, ctx),
        "almide_rt_option_and_then" if args.len() == 2 => option_and_then(&args[0], &args[1], ctx),
        "almide_rt_result_map" if args.len() == 2 => result_map(&args[0], &args[1], ret_ty, ctx),
        "almide_rt_result_map_err" if args.len() == 2 => result_map_err(&args[0], &args[1], ret_ty, ctx),
        "almide_rt_string_slice" if args.len() == 3 =>
            call_runtime("__string_slice", args, 1, ctx),
        "almide_rt_string_char_at" if args.len() == 2 =>
            call_runtime("__string_get", args, 1, ctx),
        "almide_rt_string_to_upper" if args.len() == 1 => to_case(&args[0], 1, ctx),
        "almide_rt_string_to_lower" if args.len() == 1 => to_case(&args[0], 0, ctx),
        "almide_rt_string_repeat" if args.len() == 2 =>
            call_runtime("__string_repeat", args, 1, ctx),
        "almide_rt_string_contains" if args.len() == 2 =>
            call_runtime("__string_contains", args, 1, ctx),
        "almide_rt_string_trim" if args.len() == 1 => str_trim(&args[0], 3, ctx),
        "almide_rt_string_trim_start" if args.len() == 1 => str_trim(&args[0], 1, ctx),
        "almide_rt_string_trim_end" if args.len() == 1 => str_trim(&args[0], 2, ctx),
        "almide_rt_string_index_of" if args.len() == 2 =>
            call_runtime("__string_index_of", args, 1, ctx),
        "almide_rt_string_last_index_of" if args.len() == 2 =>
            call_runtime("__string_last_index_of", args, 1, ctx),
        "almide_rt_string_replace" if args.len() == 3 => str_replace(args, 1, ctx),
        "almide_rt_string_replace_first" if args.len() == 3 => str_replace(args, 0, ctx),
        "almide_rt_string_split" if args.len() == 2 =>
            call_runtime("__string_split", args, 1, ctx),
        "almide_rt_string_join" | "almide_rt_list_join" if args.len() == 2 =>
            call_runtime("__string_join", args, 1, ctx),

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
        "almide_rt_map_fold" if args.len() == 3 && map_supported(&args[0].ty) =>
            map_fold(&args[0], &args[1], &args[2], ret_ty, ctx),
        "almide_rt_map_filter" if args.len() == 2 && map_supported(&args[0].ty) =>
            map_filter(&args[0], &args[1], ctx),
        "almide_rt_map_entries" if args.len() == 1 && map_supported(&args[0].ty) =>
            map_entries(&args[0], ctx),
        "almide_rt_map_from_entries" if args.len() == 1 && map_supported(ret_ty) =>
            map_from_entries(&args[0], ret_ty, ctx),
        "almide_rt_list_sum" if args.len() == 1 => Some(list_sum(&args[0], ctx)),
        // sort: Int lists via the runtime selection sort; other element types
        // (Float/String/composite) fall back until typed comparators land.
        "almide_rt_list_sort" if args.len() == 1
            && matches!(super::lower::list_element_ty(&args[0].ty), Some(Ty::Int)) =>
            call_runtime("__list_sort_int", args, 1, ctx),
        "almide_rt_list_contains" if args.len() == 2 => list_contains(&args[0], &args[1], ctx),
        // ── Pure list builders (sub-ranges / construction; no new runtime) ──
        "almide_rt_list_take" if args.len() == 2 => list_take(&args[0], &args[1], ret_ty, ctx),
        "almide_rt_list_drop" if args.len() == 2 => list_drop(&args[0], &args[1], ret_ty, ctx),
        "almide_rt_list_slice" if args.len() == 3 => list_slice(&args[0], &args[1], &args[2], ret_ty, ctx),
        "almide_rt_list_repeat" if args.len() == 2 => list_repeat(&args[0], &args[1], ret_ty, ctx),
        "almide_rt_list_with_capacity" if args.len() == 1 => list_with_capacity(&args[0], ret_ty, ctx),
        "almide_rt_list_enumerate" if args.len() == 1 => list_enumerate(&args[0], ctx),
        "almide_rt_string_starts_with" if args.len() == 2 => call_runtime("__string_starts_with", args, 1, ctx),
        "almide_rt_string_ends_with" if args.len() == 2 => call_runtime("__string_ends_with", args, 1, ctx),

        // ── Set[A]: represented as Map[A, A] (key = val = element). A is Int or
        // String. Core ops reuse the Map runtime verbatim; set algebra and HOFs
        // scan the table's occupied slots. ──
        "almide_rt_set_new" if set_supported(ret_ty) =>
            (ctx.func_idx)("__map_new").map(|idx| vec![Op::Call { idx, pops: 0, pushes: 1 }]),
        "almide_rt_set_insert" if args.len() == 2 && set_supported(&args[0].ty) =>
            set_insert(&args[0], &args[1], ctx),
        "almide_rt_set_remove" if args.len() == 2 && set_supported(&args[0].ty) =>
            set_remove(&args[0], &args[1], ctx),
        "almide_rt_set_contains" if args.len() == 2 && set_supported(&args[0].ty) =>
            set_contains(&args[0], &args[1], ctx),
        "almide_rt_set_len" if args.len() == 1 && set_supported(&args[0].ty) =>
            call_runtime("__map_len", args, 1, ctx),
        "almide_rt_set_is_empty" if args.len() == 1 && set_supported(&args[0].ty) =>
            (ctx.func_idx)("__map_len").map(|idx| {
                let mut ops = lower_expr(&args[0], ctx);
                ops.push(Op::Call { idx, pops: 1, pushes: 1 });
                ops.push(Op::UnOp(U::I64Eqz));
                ops
            }),
        "almide_rt_set_to_list" if args.len() == 1 && set_supported(&args[0].ty) =>
            set_to_list(&args[0], ctx),
        "almide_rt_set_from_list" if args.len() == 1 && set_supported(ret_ty) =>
            set_from_list(&args[0], ret_ty, ctx),
        "almide_rt_set_union" if args.len() == 2 && set_supported(&args[0].ty) =>
            set_merge(&args[0], &args[1], ctx),
        "almide_rt_set_intersection" if args.len() == 2 && set_supported(&args[0].ty) =>
            set_combine(&args[0], &args[1], true, ctx),
        "almide_rt_set_difference" if args.len() == 2 && set_supported(&args[0].ty) =>
            set_combine(&args[0], &args[1], false, ctx),
        "almide_rt_set_symmetric_difference" if args.len() == 2 && set_supported(&args[0].ty) =>
            set_sym_diff(&args[0], &args[1], ctx),
        "almide_rt_set_is_subset" if args.len() == 2 && set_supported(&args[0].ty) =>
            set_pair_pred(&args[0], &args[1], true, ctx),
        "almide_rt_set_is_disjoint" if args.len() == 2 && set_supported(&args[0].ty) =>
            set_pair_pred(&args[0], &args[1], false, ctx),
        "almide_rt_set_filter" if args.len() == 2 && set_supported(&args[0].ty) =>
            set_filter(&args[0], &args[1], ctx),
        "almide_rt_set_map" if args.len() == 2 && set_supported(&args[0].ty) && set_supported(ret_ty) =>
            set_map_op(&args[0], &args[1], ret_ty, ctx),
        "almide_rt_set_fold" if args.len() == 3 && set_supported(&args[0].ty) =>
            set_fold(&args[0], &args[1], &args[2], ret_ty, ctx),
        "almide_rt_set_any" if args.len() == 2 && set_supported(&args[0].ty) =>
            set_any_all(&args[0], &args[1], true, ctx),
        "almide_rt_set_all" if args.len() == 2 && set_supported(&args[0].ty) =>
            set_any_all(&args[0], &args[1], false, ctx),

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
    let elem_ty = concrete_ty(super::lower::list_element_ty(&xs_expr.ty))?;
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
    let out_ok_ty = concrete_ty(result_ok_ty(ret_ty))?;
    let out_sk = store_kind_of(ty_to_wasm(&out_ok_ty));
    // Err payload type (E) for passthrough copy.
    let err_ty = concrete_ty(result_err_ty(&r.ty))?;
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
    let out_err_ty = concrete_ty(result_err_ty(ret_ty))?; // F
    let out_sk = store_kind_of(ty_to_wasm(&out_err_ty));
    let ok_ty = concrete_ty(result_ok_ty(&r.ty))?; // A (passthrough)
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
    let out_ty = concrete_ty(option_payload_ty(ret_ty))?;
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

/// `option.to_list(o) -> List[A]` — `Some(x)` → `[x]`, `None` → `[]`.
fn option_to_list(o: &IrExpr, ret_ty: &Ty, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let elem_ty = concrete_ty(super::lower::list_element_ty(ret_ty))?;
    let lk = load_kind_of(ty_to_wasm(&elem_ty));
    let sk = store_kind_of(ty_to_wasm(&elem_ty));
    let es = wasm_byte_size(&elem_ty);
    let alloc = (ctx.func_idx)("__alloc")?;
    let o_l = ctx.alloc_local(WasmTy::I32);
    let out = ctx.alloc_local(WasmTy::I32);
    let cnt = ctx.alloc_local(WasmTy::I32);

    let mut ops = lower_expr(o, ctx);
    ops.push(Op::LocalSet(o_l));

    let mut some_branch = vec![Op::Const(Const::I32(1)), Op::LocalSet(cnt)];
    some_branch.extend(alloc_list(out, cnt, es, alloc));
    some_branch.extend(vec![
        Op::LocalGet(out), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
        Op::LocalGet(o_l), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::Load(lk),
        Op::Store(sk),
        Op::LocalGet(out),
    ]);
    let mut none_branch = vec![Op::Const(Const::I32(0)), Op::LocalSet(cnt)];
    none_branch.extend(alloc_list(out, cnt, es, alloc));
    none_branch.push(Op::LocalGet(out));

    ops.push(Op::LocalGet(o_l));
    ops.push(Op::Load(LoadKind::I32)); // tag
    ops.push(Op::If { ty: WasmTy::I32, then: some_branch, else_: none_branch });
    Some(ops)
}

/// `option.and_then(o, f: (A) -> Option[B]) -> Option[B]` — `Some(x)` → `f(x)`,
/// `None` → `None` (the original None pointer is type-agnostic, reusable).
fn option_and_then(o: &IrExpr, f: &IrExpr, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (pvar, pty, body) = inline_lambda(f, 1, ctx)?;
    let in_lk = load_kind_of(ty_to_wasm(&pty));
    let o_l = ctx.alloc_local(WasmTy::I32);
    let elem = ctx.alloc_local(ty_to_wasm(&pty));
    ctx.map_var(pvar, elem);

    let mut ops = lower_expr(o, ctx);
    ops.push(Op::LocalSet(o_l));

    let mut some_branch = vec![
        Op::LocalGet(o_l), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::Load(in_lk), Op::LocalSet(elem),
    ];
    some_branch.extend(lower_expr(&body, ctx)); // f(elem) → Option[B] pointer
    let none_branch = vec![Op::LocalGet(o_l)]; // None passes through

    ops.push(Op::LocalGet(o_l));
    ops.push(Op::Load(LoadKind::I32)); // tag (nonzero = Some)
    ops.push(Op::If { ty: WasmTy::I32, then: some_branch, else_: none_branch });
    Some(ops)
}

/// `int.parse(s) -> Result[Int, String]` — calls __int_parse with an interned
/// error message for the Err payload.
fn int_parse(s: &IrExpr, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let idx = (ctx.func_idx)("__int_parse")?;
    let err_off = ctx.interner.intern("invalid integer");
    let mut ops = lower_expr(s, ctx);
    ops.push(Op::Const(Const::I32(err_off as i32)));
    ops.push(Op::Call { idx, pops: 2, pushes: 1 });
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

/// `string.to_upper`/`to_lower` — call __string_to_case with the case flag.
fn to_case(s: &IrExpr, upper: i32, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let idx = (ctx.func_idx)("__string_to_case")?;
    let mut ops = lower_expr(s, ctx);
    ops.push(Op::Const(Const::I32(upper)));
    ops.push(Op::Call { idx, pops: 2, pushes: 1 });
    Some(ops)
}

/// `string.trim`/`trim_start`/`trim_end` — call __string_trim with the mode bits.
fn str_trim(s: &IrExpr, mode: i32, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let idx = (ctx.func_idx)("__string_trim")?;
    let mut ops = lower_expr(s, ctx);
    ops.push(Op::Const(Const::I32(mode)));
    ops.push(Op::Call { idx, pops: 2, pushes: 1 });
    Some(ops)
}

/// `string.replace`/`replace_first` — call __string_replace(s, from, to, all).
fn str_replace(args: &[IrExpr], all: i32, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let idx = (ctx.func_idx)("__string_replace")?;
    let mut ops = lower_expr(&args[0], ctx);
    ops.extend(lower_expr(&args[1], ctx));
    ops.extend(lower_expr(&args[2], ctx));
    ops.push(Op::Const(Const::I32(all)));
    ops.push(Op::Call { idx, pops: 4, pushes: 1 });
    Some(ops)
}

/// An element/payload type that resolves to a concrete WASM width. Returns
/// `None` for `Unknown`/`TypeVar` so callers reject (→ legacy) instead of
/// guessing i64 — a guessed width is silent load/store corruption.
fn concrete_ty(ty: Option<Ty>) -> Option<Ty> {
    ty.filter(|t| !t.is_unresolved())
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

/// Common map-iteration prologue: evaluate `m`, capture cap, set up a slot
/// counter. Returns (m_local, cap_local, slot_local) and the prologue ops.
fn map_iter_prologue(m: &IrExpr, ctx: &mut LowerCtx) -> (u32, u32, u32, Vec<Op>) {
    let m_l = ctx.alloc_local(WasmTy::I32);
    let cap_l = ctx.alloc_local(WasmTy::I32);
    let slot_l = ctx.alloc_local(WasmTy::I32);
    let mut ops = lower_expr(m, ctx);
    ops.push(Op::LocalSet(m_l));
    ops.push(Op::LocalGet(m_l)); ops.push(Op::Const(Const::I32(4))); ops.push(Op::BinOp(B::I32Add)); ops.push(Op::Load(LoadKind::I32)); ops.push(Op::LocalSet(cap_l));
    ops.push(Op::Const(Const::I32(0))); ops.push(Op::LocalSet(slot_l));
    (m_l, cap_l, slot_l, ops)
}

/// Ops loading the entry address for `slot` into local `ea` (m+8+cap+slot*16).
fn map_entry_into(m_l: u32, cap_l: u32, slot_l: u32, ea: u32) -> Vec<Op> {
    vec![
        Op::LocalGet(m_l), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::LocalGet(cap_l), Op::BinOp(B::I32Add),
        Op::LocalGet(slot_l), Op::Const(Const::I32(16)), Op::BinOp(B::I32Mul), Op::BinOp(B::I32Add),
        Op::LocalSet(ea),
    ]
}

/// `map.fold(m, init, f: (A, K, V) -> A) -> A`.
fn map_fold(m: &IrExpr, init: &IrExpr, f: &IrExpr, ret_ty: &Ty, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (params, body) = inline_lambda_n(f, 3, ctx)?;
    let (acc_v, _a) = params[0].clone();
    let (k_v, k_ty) = params[1].clone();
    let (v_v, v_ty) = params[2].clone();
    let acc = ctx.alloc_local(ty_to_wasm(ret_ty));
    let k_local = ctx.alloc_local(ty_to_wasm(&k_ty));
    let v_local = ctx.alloc_local(ty_to_wasm(&v_ty));
    let ea = ctx.alloc_local(WasmTy::I32);
    ctx.map_var(acc_v, acc); ctx.map_var(k_v, k_local); ctx.map_var(v_v, v_local);

    let mut ops = lower_expr(init, ctx);
    ops.push(Op::LocalSet(acc));
    let (m_l, cap_l, slot_l, pro) = map_iter_prologue(m, ctx);
    ops.extend(pro);

    let mut occ = map_entry_into(m_l, cap_l, slot_l, ea);
    occ.push(Op::LocalGet(ea)); occ.push(Op::Load(load_kind_of(ty_to_wasm(&k_ty)))); occ.push(Op::LocalSet(k_local));
    occ.push(Op::LocalGet(ea)); occ.push(Op::Const(Const::I32(8))); occ.push(Op::BinOp(B::I32Add)); occ.push(Op::Load(load_kind_of(ty_to_wasm(&v_ty)))); occ.push(Op::LocalSet(v_local));
    occ.extend(lower_expr(&body, ctx)); occ.push(Op::LocalSet(acc));

    let mut loop_body = vec![
        Op::LocalGet(slot_l), Op::LocalGet(cap_l), Op::BinOp(B::I32GeU), Op::BrIf(1),
        Op::LocalGet(m_l), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::LocalGet(slot_l), Op::BinOp(B::I32Add), Op::Load(LoadKind::U8),
    ];
    loop_body.push(Op::IfVoid { then: occ, else_: vec![] });
    loop_body.extend([Op::LocalGet(slot_l), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(slot_l), Op::Br(0)]);
    ops.push(Op::Block(vec![Op::Loop(loop_body)]));
    ops.push(Op::LocalGet(acc));
    Some(ops)
}

/// `map.filter(m, f: (K, V) -> Bool) -> Map[K,V]`.
fn map_filter(m: &IrExpr, f: &IrExpr, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (kind, _k, _v) = map_kv(&m.ty)?;
    let (params, body) = inline_lambda_n(f, 2, ctx)?;
    let (k_v, k_ty) = params[0].clone();
    let (v_v, v_ty) = params[1].clone();
    let (new_idx, set_idx) = ((ctx.func_idx)("__map_new")?, (ctx.func_idx)("__map_set")?);
    let out = ctx.alloc_local(WasmTy::I32);
    let k_local = ctx.alloc_local(ty_to_wasm(&k_ty));
    let v_local = ctx.alloc_local(ty_to_wasm(&v_ty));
    let ea = ctx.alloc_local(WasmTy::I32);
    ctx.map_var(k_v, k_local); ctx.map_var(v_v, v_local);

    let (m_l, cap_l, slot_l, mut ops) = map_iter_prologue(m, ctx);
    ops.push(Op::Call { idx: new_idx, pops: 0, pushes: 1 }); ops.push(Op::LocalSet(out));

    let mut occ = map_entry_into(m_l, cap_l, slot_l, ea);
    occ.push(Op::LocalGet(ea)); occ.push(Op::Load(load_kind_of(ty_to_wasm(&k_ty)))); occ.push(Op::LocalSet(k_local));
    occ.push(Op::LocalGet(ea)); occ.push(Op::Const(Const::I32(8))); occ.push(Op::BinOp(B::I32Add)); occ.push(Op::Load(load_kind_of(ty_to_wasm(&v_ty)))); occ.push(Op::LocalSet(v_local));
    occ.extend(lower_expr(&body, ctx)); // predicate → i32 bool
    // keep: out = __map_set(out, key_i64, val_i64, kind)
    let keep = vec![
        Op::LocalGet(out),
        Op::LocalGet(ea), Op::Load(LoadKind::I64),
        Op::LocalGet(ea), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::Load(LoadKind::I64),
        Op::Const(Const::I32(kind)),
        Op::Call { idx: set_idx, pops: 4, pushes: 1 }, Op::LocalSet(out),
    ];
    occ.push(Op::IfVoid { then: keep, else_: vec![] });

    let mut loop_body = vec![
        Op::LocalGet(slot_l), Op::LocalGet(cap_l), Op::BinOp(B::I32GeU), Op::BrIf(1),
        Op::LocalGet(m_l), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::LocalGet(slot_l), Op::BinOp(B::I32Add), Op::Load(LoadKind::U8),
    ];
    loop_body.push(Op::IfVoid { then: occ, else_: vec![] });
    loop_body.extend([Op::LocalGet(slot_l), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(slot_l), Op::Br(0)]);
    ops.push(Op::Block(vec![Op::Loop(loop_body)]));
    ops.push(Op::LocalGet(out));
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

/// `map.entries(m) -> List[(K, V)]` — one heap tuple `[K@0][V@ksize]` per
/// occupied slot, collected into a 4-byte-slot list. Tuple field offsets match
/// the engine's tuple layout (natural-width packing).
fn map_entries(m: &IrExpr, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (_kind, k_ty, v_ty) = map_kv(&m.ty)?;
    let ksize = wasm_byte_size(&k_ty);
    let vsize = wasm_byte_size(&v_ty);
    let k_lk = load_kind_of(ty_to_wasm(&k_ty));
    let k_sk = store_kind_of(ty_to_wasm(&k_ty));
    let v_lk = load_kind_of(ty_to_wasm(&v_ty));
    let v_sk = store_kind_of(ty_to_wasm(&v_ty));
    let alloc = (ctx.func_idx)("__alloc")?;
    let out = ctx.alloc_local(WasmTy::I32);
    let n = ctx.alloc_local(WasmTy::I32);
    let idx = ctx.alloc_local(WasmTy::I32);
    let ea = ctx.alloc_local(WasmTy::I32);
    let tup = ctx.alloc_local(WasmTy::I32);

    let (m_l, cap_l, slot_l, mut ops) = map_iter_prologue(m, ctx);
    ops.push(Op::LocalGet(m_l)); ops.push(Op::Load(LoadKind::I32)); ops.push(Op::LocalSet(n)); // len
    ops.extend(alloc_list(out, n, 4, alloc));
    ops.push(Op::Const(Const::I32(0))); ops.push(Op::LocalSet(idx));

    let mut occ = map_entry_into(m_l, cap_l, slot_l, ea);
    // tup = alloc(ksize + vsize)
    occ.push(Op::Const(Const::I32(ksize + vsize)));
    occ.push(Op::Call { idx: alloc, pops: 1, pushes: 1 });
    occ.push(Op::LocalSet(tup));
    // tup[0] = key (slot is i64; K's load kind narrows for i32 K)
    occ.extend(vec![Op::LocalGet(tup), Op::LocalGet(ea), Op::Load(k_lk), Op::Store(k_sk)]);
    // tup[ksize] = val
    occ.extend(vec![
        Op::LocalGet(tup), Op::Const(Const::I32(ksize)), Op::BinOp(B::I32Add),
        Op::LocalGet(ea), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::Load(v_lk), Op::Store(v_sk),
    ]);
    // out[8 + idx*4] = tup
    occ.extend(vec![
        Op::LocalGet(out), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
        Op::LocalGet(idx), Op::Const(Const::I32(4)), Op::BinOp(B::I32Mul), Op::BinOp(B::I32Add),
        Op::LocalGet(tup), Op::Store(StoreKind::I32),
    ]);
    occ.extend(vec![Op::LocalGet(idx), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(idx)]);
    ops.extend(table_scan_loop(m_l, cap_l, slot_l, occ));
    ops.push(Op::LocalGet(out));
    Some(ops)
}

/// `map.from_entries(entries: List[(K,V)]) -> Map[K,V]` — insert each tuple's
/// key/val into a fresh map.
fn map_from_entries(list: &IrExpr, ret_ty: &Ty, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (kind, k_ty, v_ty) = map_kv(ret_ty)?;
    let ksize = wasm_byte_size(&k_ty);
    let k_lk = load_kind_of(ty_to_wasm(&k_ty));
    let v_lk = load_kind_of(ty_to_wasm(&v_ty));
    let k_widen = matches!(ty_to_wasm(&k_ty), WasmTy::I32);
    let v_widen = matches!(ty_to_wasm(&v_ty), WasmTy::I32);
    let (new_idx, set_idx) = ((ctx.func_idx)("__map_new")?, (ctx.func_idx)("__map_set")?);
    let out = ctx.alloc_local(WasmTy::I32);
    let tup = ctx.alloc_local(WasmTy::I32);
    let kk = ctx.alloc_local(WasmTy::I64);
    let vv = ctx.alloc_local(WasmTy::I64);

    let (lp, mut ops) = list_loop_header(list, ctx);
    ops.push(Op::Call { idx: new_idx, pops: 0, pushes: 1 });
    ops.push(Op::LocalSet(out));

    let mut lb = vec![Op::LocalGet(lp.idx), Op::LocalGet(lp.len), Op::BinOp(B::I32GeU), Op::BrIf(1)];
    // tup = list[idx]
    lb.extend(load_elem(lp.xs, lp.idx, 4, LoadKind::I32));
    lb.push(Op::LocalSet(tup));
    // kk = widen(tup[0])
    lb.push(Op::LocalGet(tup)); lb.push(Op::Load(k_lk));
    if k_widen { lb.push(Op::UnOp(U::I64ExtendI32U)); }
    lb.push(Op::LocalSet(kk));
    // vv = widen(tup[ksize])
    lb.push(Op::LocalGet(tup)); lb.push(Op::Const(Const::I32(ksize))); lb.push(Op::BinOp(B::I32Add)); lb.push(Op::Load(v_lk));
    if v_widen { lb.push(Op::UnOp(U::I64ExtendI32U)); }
    lb.push(Op::LocalSet(vv));
    // out = __map_set(out, kk, vv, kind)
    lb.extend(vec![
        Op::LocalGet(out), Op::LocalGet(kk), Op::LocalGet(vv), Op::Const(Const::I32(kind)),
        Op::Call { idx: set_idx, pops: 4, pushes: 1 }, Op::LocalSet(out),
    ]);
    lb.extend(vec![Op::LocalGet(lp.idx), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(lp.idx), Op::Br(0)]);
    ops.push(Op::Block(vec![Op::Loop(lb)]));
    ops.push(Op::LocalGet(out));
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

// ── Set[A] = Map[A, A] (key = val = element) ─────────────────────────

/// Extract (kind_const, elem_ty) from a supported `Set[A]`.
fn set_elem(ty: &Ty) -> Option<(i32, Ty)> {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    match ty {
        Ty::Applied(TC::Set, a) if a.len() == 1 => Some((map_key_kind(&a[0])?, a[0].clone())),
        _ => None,
    }
}

/// `Set[A]` with a supported element kind (Int or String).
pub(super) fn set_supported(ty: &Ty) -> bool {
    set_elem(ty).is_some()
}

/// Standard occupied-slot scan over a Swiss table: for each slot in `[0, cap)`
/// whose tag byte is non-zero, run `occ` (which addresses the entry via a prior
/// `map_entry_into` into `ea`). `occ` must leave the stack balanced.
fn table_scan_loop(m_l: u32, cap_l: u32, slot_l: u32, occ: Vec<Op>) -> Vec<Op> {
    let mut loop_body = vec![
        Op::LocalGet(slot_l), Op::LocalGet(cap_l), Op::BinOp(B::I32GeU), Op::BrIf(1),
        Op::LocalGet(m_l), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::LocalGet(slot_l), Op::BinOp(B::I32Add), Op::Load(LoadKind::U8),
    ];
    loop_body.push(Op::IfVoid { then: occ, else_: vec![] });
    loop_body.extend([Op::LocalGet(slot_l), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(slot_l), Op::Br(0)]);
    vec![Op::Block(vec![Op::Loop(loop_body)])]
}

/// `set.insert(s, v) -> Set[A]` — Map set with key = val = element.
fn set_insert(s: &IrExpr, v: &IrExpr, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (kind, elem_ty) = set_elem(&s.ty)?;
    let idx = (ctx.func_idx)("__map_set")?;
    let s_l = ctx.alloc_local(WasmTy::I32);
    let el = ctx.alloc_local(WasmTy::I64);
    let mut ops = lower_expr(s, ctx);
    ops.push(Op::LocalSet(s_l));
    ops.extend(lower_widened(v, &elem_ty, ctx));
    ops.push(Op::LocalSet(el));
    ops.push(Op::LocalGet(s_l));
    ops.push(Op::LocalGet(el));
    ops.push(Op::LocalGet(el));
    ops.push(Op::Const(Const::I32(kind)));
    ops.push(Op::Call { idx, pops: 4, pushes: 1 });
    Some(ops)
}

/// `set.remove(s, v) -> Set[A]`.
fn set_remove(s: &IrExpr, v: &IrExpr, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (kind, elem_ty) = set_elem(&s.ty)?;
    let idx = (ctx.func_idx)("__map_remove")?;
    let mut ops = lower_expr(s, ctx);
    ops.extend(lower_widened(v, &elem_ty, ctx));
    ops.push(Op::Const(Const::I32(kind)));
    ops.push(Op::Call { idx, pops: 3, pushes: 1 });
    Some(ops)
}

/// `set.contains(s, v) -> Bool`.
fn set_contains(s: &IrExpr, v: &IrExpr, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (kind, elem_ty) = set_elem(&s.ty)?;
    let idx = (ctx.func_idx)("__map_contains")?;
    let mut ops = lower_expr(s, ctx);
    ops.extend(lower_widened(v, &elem_ty, ctx));
    ops.push(Op::Const(Const::I32(kind)));
    ops.push(Op::Call { idx, pops: 3, pushes: 1 });
    Some(ops)
}

/// `set.to_list(s) -> List[A]` — collect the key slots (= elements).
fn set_to_list(s: &IrExpr, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (_kind, elem_ty) = set_elem(&s.ty)?;
    let idx = (ctx.func_idx)("__map_collect")?;
    let elem_size = wasm_byte_size(&elem_ty);
    let mut ops = lower_expr(s, ctx);
    ops.push(Op::Const(Const::I32(0)));
    ops.push(Op::Const(Const::I32(elem_size)));
    ops.push(Op::Call { idx, pops: 3, pushes: 1 });
    Some(ops)
}

/// `set.union(a, b) -> Set[A]` — Map merge (idempotent on equal elements).
fn set_merge(a: &IrExpr, b: &IrExpr, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (kind, _e) = set_elem(&a.ty)?;
    let idx = (ctx.func_idx)("__map_merge")?;
    let mut ops = lower_expr(a, ctx);
    ops.extend(lower_expr(b, ctx));
    ops.push(Op::Const(Const::I32(kind)));
    ops.push(Op::Call { idx, pops: 3, pushes: 1 });
    Some(ops)
}

/// `set.from_list(xs) -> Set[A]` — insert each list element into a fresh set.
fn set_from_list(xs: &IrExpr, ret_ty: &Ty, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (kind, elem_ty) = set_elem(ret_ty)?;
    let (new_idx, set_idx) = ((ctx.func_idx)("__map_new")?, (ctx.func_idx)("__map_set")?);
    let es = wasm_byte_size(&elem_ty);
    let lk = load_kind_of(ty_to_wasm(&elem_ty));
    let widen = matches!(ty_to_wasm(&elem_ty), WasmTy::I32);
    let out = ctx.alloc_local(WasmTy::I32);
    let el = ctx.alloc_local(WasmTy::I64);
    let (lp, mut ops) = list_loop_header(xs, ctx);
    ops.push(Op::Call { idx: new_idx, pops: 0, pushes: 1 });
    ops.push(Op::LocalSet(out));

    let mut lb = vec![
        Op::LocalGet(lp.idx), Op::LocalGet(lp.len), Op::BinOp(B::I32GeU), Op::BrIf(1),
    ];
    lb.extend(load_elem(lp.xs, lp.idx, es, lk));
    if widen { lb.push(Op::UnOp(U::I64ExtendI32U)); }
    lb.push(Op::LocalSet(el));
    lb.extend(vec![
        Op::LocalGet(out), Op::LocalGet(el), Op::LocalGet(el), Op::Const(Const::I32(kind)),
        Op::Call { idx: set_idx, pops: 4, pushes: 1 }, Op::LocalSet(out),
        Op::LocalGet(lp.idx), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(lp.idx), Op::Br(0),
    ]);
    ops.push(Op::Block(vec![Op::Loop(lb)]));
    ops.push(Op::LocalGet(out));
    Some(ops)
}

/// `set.intersection`/`difference(a, b)` — scan `a`, keeping each element whose
/// membership in `b` matches `keep_when_in_b` (true = intersection, false =
/// difference). Copies the raw i64 key slot, so no per-element width juggling.
fn set_combine(a: &IrExpr, b: &IrExpr, keep_when_in_b: bool, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (kind, _e) = set_elem(&a.ty)?;
    let (new_idx, set_idx, contains_idx) =
        ((ctx.func_idx)("__map_new")?, (ctx.func_idx)("__map_set")?, (ctx.func_idx)("__map_contains")?);
    let b_l = ctx.alloc_local(WasmTy::I32);
    let out = ctx.alloc_local(WasmTy::I32);
    let ea = ctx.alloc_local(WasmTy::I32);

    let mut ops = lower_expr(b, ctx);
    ops.push(Op::LocalSet(b_l));
    let (m_l, cap_l, slot_l, pro) = map_iter_prologue(a, ctx);
    ops.extend(pro);
    ops.push(Op::Call { idx: new_idx, pops: 0, pushes: 1 });
    ops.push(Op::LocalSet(out));

    let mut occ = map_entry_into(m_l, cap_l, slot_l, ea);
    // b.contains(key)
    occ.extend(vec![
        Op::LocalGet(b_l), Op::LocalGet(ea), Op::Load(LoadKind::I64), Op::Const(Const::I32(kind)),
        Op::Call { idx: contains_idx, pops: 3, pushes: 1 },
    ]);
    if !keep_when_in_b { occ.push(Op::UnOp(U::I32Eqz)); }
    let keep = vec![
        Op::LocalGet(out),
        Op::LocalGet(ea), Op::Load(LoadKind::I64),
        Op::LocalGet(ea), Op::Load(LoadKind::I64),
        Op::Const(Const::I32(kind)),
        Op::Call { idx: set_idx, pops: 4, pushes: 1 }, Op::LocalSet(out),
    ];
    occ.push(Op::IfVoid { then: keep, else_: vec![] });
    ops.extend(table_scan_loop(m_l, cap_l, slot_l, occ));
    ops.push(Op::LocalGet(out));
    Some(ops)
}

/// `set.symmetric_difference(a, b)` — elements in exactly one of `a`, `b`.
/// Two scans: (a not in b) then (b not in a), inserting into one fresh set.
fn set_sym_diff(a: &IrExpr, b: &IrExpr, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (kind, _e) = set_elem(&a.ty)?;
    let (new_idx, set_idx, contains_idx) =
        ((ctx.func_idx)("__map_new")?, (ctx.func_idx)("__map_set")?, (ctx.func_idx)("__map_contains")?);
    let a_l = ctx.alloc_local(WasmTy::I32);
    let b_l = ctx.alloc_local(WasmTy::I32);
    let out = ctx.alloc_local(WasmTy::I32);

    let mut ops = lower_expr(a, ctx);
    ops.push(Op::LocalSet(a_l));
    ops.extend(lower_expr(b, ctx));
    ops.push(Op::LocalSet(b_l));
    ops.push(Op::Call { idx: new_idx, pops: 0, pushes: 1 });
    ops.push(Op::LocalSet(out));

    // One scan of `src`, inserting keys absent from `other`.
    let scan = |src_l: u32, other_l: u32, ctx: &mut LowerCtx| -> Vec<Op> {
        let cap_l = ctx.alloc_local(WasmTy::I32);
        let slot_l = ctx.alloc_local(WasmTy::I32);
        let ea = ctx.alloc_local(WasmTy::I32);
        let mut pre = vec![
            Op::LocalGet(src_l), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::Load(LoadKind::I32), Op::LocalSet(cap_l),
            Op::Const(Const::I32(0)), Op::LocalSet(slot_l),
        ];
        let mut occ = map_entry_into(src_l, cap_l, slot_l, ea);
        occ.extend(vec![
            Op::LocalGet(other_l), Op::LocalGet(ea), Op::Load(LoadKind::I64), Op::Const(Const::I32(kind)),
            Op::Call { idx: contains_idx, pops: 3, pushes: 1 }, Op::UnOp(U::I32Eqz),
        ]);
        let keep = vec![
            Op::LocalGet(out),
            Op::LocalGet(ea), Op::Load(LoadKind::I64),
            Op::LocalGet(ea), Op::Load(LoadKind::I64),
            Op::Const(Const::I32(kind)),
            Op::Call { idx: set_idx, pops: 4, pushes: 1 }, Op::LocalSet(out),
        ];
        occ.push(Op::IfVoid { then: keep, else_: vec![] });
        pre.extend(table_scan_loop(src_l, cap_l, slot_l, occ));
        pre
    };
    let s1 = scan(a_l, b_l, ctx);
    ops.extend(s1);
    let s2 = scan(b_l, a_l, ctx);
    ops.extend(s2);
    ops.push(Op::LocalGet(out));
    Some(ops)
}

/// `set.is_subset(a, b)` (every element of `a` in `b`) / `is_disjoint(a, b)`
/// (no element of `a` in `b`). Scans `a`; on the first violating element,
/// returns 0. `want_in_b` true = subset, false = disjoint.
fn set_pair_pred(a: &IrExpr, b: &IrExpr, want_in_b: bool, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (kind, _e) = set_elem(&a.ty)?;
    let contains_idx = (ctx.func_idx)("__map_contains")?;
    let b_l = ctx.alloc_local(WasmTy::I32);
    let ea = ctx.alloc_local(WasmTy::I32);

    let mut ops = lower_expr(b, ctx);
    ops.push(Op::LocalSet(b_l));
    let (m_l, cap_l, slot_l, pro) = map_iter_prologue(a, ctx);
    ops.extend(pro);

    let mut occ = map_entry_into(m_l, cap_l, slot_l, ea);
    occ.extend(vec![
        Op::LocalGet(b_l), Op::LocalGet(ea), Op::Load(LoadKind::I64), Op::Const(Const::I32(kind)),
        Op::Call { idx: contains_idx, pops: 3, pushes: 1 },
    ]);
    // subset wants in_b==true; violation = !in_b. disjoint wants in_b==false; violation = in_b.
    if want_in_b { occ.push(Op::UnOp(U::I32Eqz)); }
    occ.push(Op::IfVoid { then: vec![Op::Const(Const::I32(0)), Op::Return], else_: vec![] });
    ops.extend(table_scan_loop(m_l, cap_l, slot_l, occ));
    ops.push(Op::Const(Const::I32(1)));
    Some(ops)
}

/// `set.filter(s, f: (A) -> Bool) -> Set[A]`.
fn set_filter(s: &IrExpr, f: &IrExpr, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (kind, _e) = set_elem(&s.ty)?;
    let (pvar, pty, body) = inline_lambda(f, 1, ctx)?;
    let (new_idx, set_idx) = ((ctx.func_idx)("__map_new")?, (ctx.func_idx)("__map_set")?);
    let elem = ctx.alloc_local(ty_to_wasm(&pty));
    ctx.map_var(pvar, elem);
    let out = ctx.alloc_local(WasmTy::I32);
    let ea = ctx.alloc_local(WasmTy::I32);

    let (m_l, cap_l, slot_l, mut ops) = map_iter_prologue(s, ctx);
    ops.push(Op::Call { idx: new_idx, pops: 0, pushes: 1 });
    ops.push(Op::LocalSet(out));

    let mut occ = map_entry_into(m_l, cap_l, slot_l, ea);
    occ.push(Op::LocalGet(ea)); occ.push(Op::Load(load_kind_of(ty_to_wasm(&pty)))); occ.push(Op::LocalSet(elem));
    occ.extend(lower_expr(&body, ctx)); // predicate → i32
    let keep = vec![
        Op::LocalGet(out),
        Op::LocalGet(ea), Op::Load(LoadKind::I64),
        Op::LocalGet(ea), Op::Load(LoadKind::I64),
        Op::Const(Const::I32(kind)),
        Op::Call { idx: set_idx, pops: 4, pushes: 1 }, Op::LocalSet(out),
    ];
    occ.push(Op::IfVoid { then: keep, else_: vec![] });
    ops.extend(table_scan_loop(m_l, cap_l, slot_l, occ));
    ops.push(Op::LocalGet(out));
    Some(ops)
}

/// `set.map(s, f: (A) -> B) -> Set[B]`.
fn set_map_op(s: &IrExpr, f: &IrExpr, ret_ty: &Ty, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (out_kind, be) = set_elem(ret_ty)?;
    let (pvar, pty, body) = inline_lambda(f, 1, ctx)?;
    let out_widen = matches!(ty_to_wasm(&be), WasmTy::I32);
    let (new_idx, set_idx) = ((ctx.func_idx)("__map_new")?, (ctx.func_idx)("__map_set")?);
    let elem = ctx.alloc_local(ty_to_wasm(&pty));
    ctx.map_var(pvar, elem);
    let out = ctx.alloc_local(WasmTy::I32);
    let nv = ctx.alloc_local(WasmTy::I64);
    let ea = ctx.alloc_local(WasmTy::I32);

    let (m_l, cap_l, slot_l, mut ops) = map_iter_prologue(s, ctx);
    ops.push(Op::Call { idx: new_idx, pops: 0, pushes: 1 });
    ops.push(Op::LocalSet(out));

    let mut occ = map_entry_into(m_l, cap_l, slot_l, ea);
    occ.push(Op::LocalGet(ea)); occ.push(Op::Load(load_kind_of(ty_to_wasm(&pty)))); occ.push(Op::LocalSet(elem));
    occ.extend(lower_expr(&body, ctx)); // f(elem) → B
    if out_widen { occ.push(Op::UnOp(U::I64ExtendI32U)); }
    occ.push(Op::LocalSet(nv));
    occ.extend(vec![
        Op::LocalGet(out), Op::LocalGet(nv), Op::LocalGet(nv), Op::Const(Const::I32(out_kind)),
        Op::Call { idx: set_idx, pops: 4, pushes: 1 }, Op::LocalSet(out),
    ]);
    ops.extend(table_scan_loop(m_l, cap_l, slot_l, occ));
    ops.push(Op::LocalGet(out));
    Some(ops)
}

/// `set.fold(s, init, f: (B, A) -> B) -> B`.
fn set_fold(s: &IrExpr, init: &IrExpr, f: &IrExpr, ret_ty: &Ty, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (params, body) = inline_lambda_n(f, 2, ctx)?;
    let (acc_v, _a) = params[0].clone();
    let (e_v, e_ty) = params[1].clone();
    let acc = ctx.alloc_local(ty_to_wasm(ret_ty));
    let elem = ctx.alloc_local(ty_to_wasm(&e_ty));
    let ea = ctx.alloc_local(WasmTy::I32);
    ctx.map_var(acc_v, acc); ctx.map_var(e_v, elem);

    let mut ops = lower_expr(init, ctx);
    ops.push(Op::LocalSet(acc));
    let (m_l, cap_l, slot_l, pro) = map_iter_prologue(s, ctx);
    ops.extend(pro);

    let mut occ = map_entry_into(m_l, cap_l, slot_l, ea);
    occ.push(Op::LocalGet(ea)); occ.push(Op::Load(load_kind_of(ty_to_wasm(&e_ty)))); occ.push(Op::LocalSet(elem));
    occ.extend(lower_expr(&body, ctx)); occ.push(Op::LocalSet(acc));
    ops.extend(table_scan_loop(m_l, cap_l, slot_l, occ));
    ops.push(Op::LocalGet(acc));
    Some(ops)
}

/// `set.any(s, f)` / `set.all(s, f)` — short-circuiting predicate scan.
fn set_any_all(s: &IrExpr, f: &IrExpr, is_any: bool, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (pvar, pty, body) = inline_lambda(f, 1, ctx)?;
    let elem = ctx.alloc_local(ty_to_wasm(&pty));
    ctx.map_var(pvar, elem);
    let ea = ctx.alloc_local(WasmTy::I32);

    let (m_l, cap_l, slot_l, mut ops) = map_iter_prologue(s, ctx);
    let mut occ = map_entry_into(m_l, cap_l, slot_l, ea);
    occ.push(Op::LocalGet(ea)); occ.push(Op::Load(load_kind_of(ty_to_wasm(&pty)))); occ.push(Op::LocalSet(elem));
    occ.extend(lower_expr(&body, ctx)); // predicate → i32
    // any: first true → return 1. all: first false → return 0.
    if is_any {
        occ.push(Op::IfVoid { then: vec![Op::Const(Const::I32(1)), Op::Return], else_: vec![] });
    } else {
        occ.push(Op::UnOp(U::I32Eqz));
        occ.push(Op::IfVoid { then: vec![Op::Const(Const::I32(0)), Op::Return], else_: vec![] });
    }
    ops.extend(table_scan_loop(m_l, cap_l, slot_l, occ));
    ops.push(Op::Const(Const::I32(if is_any { 0 } else { 1 })));
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
    let elem_ty = concrete_ty(super::lower::list_element_ty(ret_ty))?;
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
    let out_ty = concrete_ty(super::lower::list_element_ty(ret_ty))?;
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
    let out_ty = concrete_ty(super::lower::list_element_ty(ret_ty))?;
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

/// Ops that clamp the i32 local `v` into `[0, hi]` in place.
fn clamp_0_hi(v: u32, hi: u32) -> Vec<Op> {
    vec![
        Op::LocalGet(v), Op::Const(Const::I32(0)), Op::BinOp(B::I32LtS),
        Op::If { ty: WasmTy::I32, then: vec![Op::Const(Const::I32(0))], else_: vec![Op::LocalGet(v)] },
        Op::LocalSet(v),
        Op::LocalGet(v), Op::LocalGet(hi), Op::BinOp(B::I32GtS),
        Op::If { ty: WasmTy::I32, then: vec![Op::LocalGet(hi)], else_: vec![Op::LocalGet(v)] },
        Op::LocalSet(v),
    ]
}

/// `out = xs[start, start+count)` as a fresh list. `start`/`count` are i32 locals
/// (caller clamps), `es` the element byte width.
fn build_sublist(xs: u32, start: u32, count: u32, es: i32, out: u32, alloc: super::ir::FuncIdx) -> Vec<Op> {
    let mut ops = alloc_list(out, count, es, alloc);
    ops.extend(vec![
        Op::LocalGet(out), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
        Op::LocalGet(xs), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
        Op::LocalGet(start), Op::Const(Const::I32(es)), Op::BinOp(B::I32Mul), Op::BinOp(B::I32Add),
        Op::LocalGet(count), Op::Const(Const::I32(es)), Op::BinOp(B::I32Mul),
        Op::MemoryCopy,
    ]);
    ops
}

/// Common prologue for the sub-range builders: evaluate `xs` into a local and
/// load its length. Returns (xs_local, len_local, ops, es, alloc).
fn sublist_prologue(xs: &IrExpr, ret_ty: &Ty, ctx: &mut LowerCtx) -> Option<(u32, u32, Vec<Op>, i32, super::ir::FuncIdx)> {
    let elem_ty = concrete_ty(super::lower::list_element_ty(ret_ty))?;
    let es = wasm_byte_size(&elem_ty);
    let alloc = (ctx.func_idx)("__alloc")?;
    let xs_l = ctx.alloc_local(WasmTy::I32);
    let len = ctx.alloc_local(WasmTy::I32);
    let mut ops = lower_expr(xs, ctx);
    ops.push(Op::LocalSet(xs_l));
    ops.push(Op::LocalGet(xs_l)); ops.push(Op::Load(LoadKind::I32)); ops.push(Op::LocalSet(len));
    Some((xs_l, len, ops, es, alloc))
}

/// `list.take(xs, n)` — first `clamp(n, 0, len)` elements.
fn list_take(xs: &IrExpr, n: &IrExpr, ret_ty: &Ty, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (xs_l, len, mut ops, es, alloc) = sublist_prologue(xs, ret_ty, ctx)?;
    let cnt = ctx.alloc_local(WasmTy::I32);
    let start = ctx.alloc_local(WasmTy::I32);
    let out = ctx.alloc_local(WasmTy::I32);
    ops.extend(lower_expr(n, ctx)); ops.push(Op::UnOp(U::I32WrapI64)); ops.push(Op::LocalSet(cnt));
    ops.extend(clamp_0_hi(cnt, len));
    ops.push(Op::Const(Const::I32(0))); ops.push(Op::LocalSet(start));
    ops.extend(build_sublist(xs_l, start, cnt, es, out, alloc));
    ops.push(Op::LocalGet(out));
    Some(ops)
}

/// `list.drop(xs, n)` — elements from index `clamp(n, 0, len)` onward.
fn list_drop(xs: &IrExpr, n: &IrExpr, ret_ty: &Ty, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (xs_l, len, mut ops, es, alloc) = sublist_prologue(xs, ret_ty, ctx)?;
    let start = ctx.alloc_local(WasmTy::I32);
    let cnt = ctx.alloc_local(WasmTy::I32);
    let out = ctx.alloc_local(WasmTy::I32);
    ops.extend(lower_expr(n, ctx)); ops.push(Op::UnOp(U::I32WrapI64)); ops.push(Op::LocalSet(start));
    ops.extend(clamp_0_hi(start, len));
    ops.extend(vec![Op::LocalGet(len), Op::LocalGet(start), Op::BinOp(B::I32Sub), Op::LocalSet(cnt)]);
    ops.extend(build_sublist(xs_l, start, cnt, es, out, alloc));
    ops.push(Op::LocalGet(out));
    Some(ops)
}

/// `list.slice(xs, start, end)` — elements `[clamp(start), clamp(end))`.
fn list_slice(xs: &IrExpr, start_e: &IrExpr, end_e: &IrExpr, ret_ty: &Ty, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let (xs_l, len, mut ops, es, alloc) = sublist_prologue(xs, ret_ty, ctx)?;
    let s = ctx.alloc_local(WasmTy::I32);
    let e = ctx.alloc_local(WasmTy::I32);
    let cnt = ctx.alloc_local(WasmTy::I32);
    let out = ctx.alloc_local(WasmTy::I32);
    ops.extend(lower_expr(start_e, ctx)); ops.push(Op::UnOp(U::I32WrapI64)); ops.push(Op::LocalSet(s));
    ops.extend(clamp_0_hi(s, len));
    ops.extend(lower_expr(end_e, ctx)); ops.push(Op::UnOp(U::I32WrapI64)); ops.push(Op::LocalSet(e));
    // e = max(e, s) then min(e, len)
    ops.extend(vec![
        Op::LocalGet(e), Op::LocalGet(s), Op::BinOp(B::I32LtS),
        Op::If { ty: WasmTy::I32, then: vec![Op::LocalGet(s)], else_: vec![Op::LocalGet(e)] }, Op::LocalSet(e),
        Op::LocalGet(e), Op::LocalGet(len), Op::BinOp(B::I32GtS),
        Op::If { ty: WasmTy::I32, then: vec![Op::LocalGet(len)], else_: vec![Op::LocalGet(e)] }, Op::LocalSet(e),
    ]);
    ops.extend(vec![Op::LocalGet(e), Op::LocalGet(s), Op::BinOp(B::I32Sub), Op::LocalSet(cnt)]);
    ops.extend(build_sublist(xs_l, s, cnt, es, out, alloc));
    ops.push(Op::LocalGet(out));
    Some(ops)
}

/// `list.repeat(val, n)` — a list of `max(n, 0)` copies of `val`.
fn list_repeat(val: &IrExpr, n: &IrExpr, ret_ty: &Ty, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let elem_ty = concrete_ty(super::lower::list_element_ty(ret_ty))?;
    let es = wasm_byte_size(&elem_ty);
    let sk = store_kind_of(ty_to_wasm(&elem_ty));
    let alloc = (ctx.func_idx)("__alloc")?;
    let val_l = ctx.alloc_local(ty_to_wasm(&elem_ty));
    let cnt = ctx.alloc_local(WasmTy::I32);
    let out = ctx.alloc_local(WasmTy::I32);
    let i = ctx.alloc_local(WasmTy::I32);
    let mut ops = lower_expr(val, ctx);
    ops.push(Op::LocalSet(val_l));
    ops.extend(lower_expr(n, ctx)); ops.push(Op::UnOp(U::I32WrapI64)); ops.push(Op::LocalSet(cnt));
    // cnt = cnt < 0 ? 0 : cnt
    ops.extend(vec![
        Op::LocalGet(cnt), Op::Const(Const::I32(0)), Op::BinOp(B::I32LtS),
        Op::If { ty: WasmTy::I32, then: vec![Op::Const(Const::I32(0))], else_: vec![Op::LocalGet(cnt)] }, Op::LocalSet(cnt),
    ]);
    ops.extend(alloc_list(out, cnt, es, alloc));
    ops.push(Op::Const(Const::I32(0))); ops.push(Op::LocalSet(i));
    let loop_body = vec![
        Op::LocalGet(i), Op::LocalGet(cnt), Op::BinOp(B::I32GeU), Op::BrIf(1),
        Op::LocalGet(out), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
        Op::LocalGet(i), Op::Const(Const::I32(es)), Op::BinOp(B::I32Mul), Op::BinOp(B::I32Add),
        Op::LocalGet(val_l), Op::Store(sk),
        Op::LocalGet(i), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(i), Op::Br(0),
    ];
    ops.push(Op::Block(vec![Op::Loop(loop_body)]));
    ops.push(Op::LocalGet(out));
    Some(ops)
}

/// `list.with_capacity(cap)` — an empty list (`len = 0`) with `max(cap,0)` slots
/// reserved (`cap` field set). Useful once mutation lands; valid empty list now.
fn list_with_capacity(cap_e: &IrExpr, ret_ty: &Ty, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let elem_ty = concrete_ty(super::lower::list_element_ty(ret_ty))?;
    let es = wasm_byte_size(&elem_ty);
    let alloc = (ctx.func_idx)("__alloc")?;
    let cap = ctx.alloc_local(WasmTy::I32);
    let out = ctx.alloc_local(WasmTy::I32);
    let mut ops = lower_expr(cap_e, ctx);
    ops.push(Op::UnOp(U::I32WrapI64)); ops.push(Op::LocalSet(cap));
    ops.extend(vec![
        Op::LocalGet(cap), Op::Const(Const::I32(0)), Op::BinOp(B::I32LtS),
        Op::If { ty: WasmTy::I32, then: vec![Op::Const(Const::I32(0))], else_: vec![Op::LocalGet(cap)] }, Op::LocalSet(cap),
    ]);
    // out = alloc(8 + cap*es) ; out.len = 0 ; out.cap = cap
    ops.extend(vec![
        Op::Const(Const::I32(8)), Op::LocalGet(cap), Op::Const(Const::I32(es)), Op::BinOp(B::I32Mul), Op::BinOp(B::I32Add),
        Op::Call { idx: alloc, pops: 1, pushes: 1 }, Op::LocalSet(out),
        Op::LocalGet(out), Op::Const(Const::I32(0)), Op::Store(StoreKind::I32),
        Op::LocalGet(out), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::LocalGet(cap), Op::Store(StoreKind::I32),
        Op::LocalGet(out),
    ]);
    Some(ops)
}

/// `list.enumerate(xs)` — `List[(Int, A)]`: a heap tuple `[Int@0][A@8]` per
/// element, collected into a 4-byte-slot list.
fn list_enumerate(xs: &IrExpr, ctx: &mut LowerCtx) -> Option<Vec<Op>> {
    let a_ty = concrete_ty(super::lower::list_element_ty(&xs.ty))?;
    let a_es = wasm_byte_size(&a_ty);
    let a_lk = load_kind_of(ty_to_wasm(&a_ty));
    let a_sk = store_kind_of(ty_to_wasm(&a_ty));
    let tup_size = 8 + a_es;
    let alloc = (ctx.func_idx)("__alloc")?;
    let xs_l = ctx.alloc_local(WasmTy::I32);
    let len = ctx.alloc_local(WasmTy::I32);
    let out = ctx.alloc_local(WasmTy::I32);
    let i = ctx.alloc_local(WasmTy::I32);
    let tup = ctx.alloc_local(WasmTy::I32);
    let mut ops = lower_expr(xs, ctx);
    ops.push(Op::LocalSet(xs_l));
    ops.push(Op::LocalGet(xs_l)); ops.push(Op::Load(LoadKind::I32)); ops.push(Op::LocalSet(len));
    ops.extend(alloc_list(out, len, 4, alloc));
    ops.push(Op::Const(Const::I32(0))); ops.push(Op::LocalSet(i));
    let loop_body = vec![
        Op::LocalGet(i), Op::LocalGet(len), Op::BinOp(B::I32GeU), Op::BrIf(1),
        // tup = alloc(tup_size)
        Op::Const(Const::I32(tup_size)), Op::Call { idx: alloc, pops: 1, pushes: 1 }, Op::LocalSet(tup),
        // tup[0] = (i64) i
        Op::LocalGet(tup), Op::LocalGet(i), Op::UnOp(U::I64ExtendI32U), Op::Store(StoreKind::I64),
        // tup[8] = xs[8 + i*a_es]
        Op::LocalGet(tup), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
        Op::LocalGet(xs_l), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
        Op::LocalGet(i), Op::Const(Const::I32(a_es)), Op::BinOp(B::I32Mul), Op::BinOp(B::I32Add), Op::Load(a_lk),
        Op::Store(a_sk),
        // out[8 + i*4] = tup
        Op::LocalGet(out), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
        Op::LocalGet(i), Op::Const(Const::I32(4)), Op::BinOp(B::I32Mul), Op::BinOp(B::I32Add),
        Op::LocalGet(tup), Op::Store(StoreKind::I32),
        Op::LocalGet(i), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(i), Op::Br(0),
    ];
    ops.push(Op::Block(vec![Op::Loop(loop_body)]));
    ops.push(Op::LocalGet(out));
    Some(ops)
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
    let out_ty = concrete_ty(super::lower::list_element_ty(ret_ty))?;
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
        // A non-capturing closure (empty env) is inlinable from its lifted body:
        // params are [env, p1..], skip env. Capturing closures need call_indirect.
        almide_ir::IrExprKind::ClosureCreate { func_name, captures } if captures.is_empty() => {
            let (params, body) = ctx.fn_bodies.get(func_name)?;
            return (params.len() == n + 1)
                .then(|| (params[1..].to_vec(), body.clone()));
        }
        _ => return None,
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
