// ── Call lowering ───────────────────────────────────────────────

use almide_lang::ast;
use almide_ir::*;
use crate::types::{Ty, TypeConstructorId};
use almide_base::intern::{sym, Sym};
use super::LowerCtx;
use super::expressions::lower_expr;
use super::types::resolve_type_expr;

/// The argument list of the call being lowered.
///
/// Positional args, named args and explicit type args are three slices that
/// always travel together and are meaningless apart, so they travel as one
/// value. That also stops `args` and `named_args` — adjacent slices whose
/// element types differ only in the tuple wrapper — from being transposed.
pub(super) struct CallArgs<'a> {
    pub args: &'a [ast::Expr],
    pub named_args: &'a [(almide_base::intern::Sym, ast::Expr)],
    pub type_args: Option<&'a Vec<ast::TypeExpr>>,
}

pub(super) fn lower_call(ctx: &mut LowerCtx, callee: &ast::Expr, call: CallArgs<'_>, ty: Ty, span: Option<ast::Span>) -> IrExpr {
    let CallArgs { args, named_args, type_args } = call;
    if let Some(converted) = lower_call_json_convenience(ctx, callee, args, type_args, ty.clone(), span) {
        return converted;
    }

    let mut ir_args: Vec<IrExpr> = Vec::new();
    let ta_raw: Vec<Ty> = type_args.map(|tas| tas.iter().map(|t| resolve_type_expr(t)).collect()).unwrap_or_default();
    let ta = split_const_value_type_args(ctx, &ta_raw, &mut ir_args, span);

    ir_args.extend(args.iter().map(|a| lower_expr(ctx, a)));
    let mut target = lower_call_target(ctx, callee);
    rewrite_crossmodule_ufcs(ctx, &mut target, &mut ir_args);
    rewrite_local_ufcs(ctx, &mut target, &mut ir_args);

    if named_args.is_empty() {
        // Default args for a plain positional call.
        lower_call_fill_defaults(ctx, &mut ir_args, args, &target);
    } else {
        fill_named_args(ctx, &mut ir_args, named_args, &target);
    }
    fill_ufcs_defaults(ctx, &mut ir_args, &target);

    // Stage 1b: retype Int/Float literal args that flow into sized
    // numeric params (`Int32`, `UInt8`, `Float32`, ...).
    lower_call_coerce_args(ctx, &mut ir_args, &target);

    if let Some(desugared) = desugar_assert_outside_test(ctx, &target, &ir_args, span) {
        return desugared;
    }

    let ty = call_result_ty(&target, ty);
    ctx.mk(IrExprKind::Call { target, args: ir_args, type_args: ta }, ty, span)
}

/// Split resolved type arguments into the generic ones and the const VALUES.
///
/// A const value type arg is a positional argument in disguise:
/// `make_list[3]("hello")` becomes `make_list(3, "hello")` at IR level, so the
/// value is pushed onto `ir_args` and only the remaining type args are returned.
/// Values are pushed BEFORE the call's own arguments, matching the declaration
/// order the callee's signature was registered with.
fn split_const_value_type_args(
    ctx: &mut LowerCtx,
    ta_raw: &[Ty],
    ir_args: &mut Vec<IrExpr>,
    span: Option<ast::Span>,
) -> Vec<Ty> {
    let mut ta = Vec::new();
    for t in ta_raw {
        if let Ty::ConstValue { value, ty: vty } = t {
            ir_args.push(ctx.mk(IrExprKind::LitInt { value: *value }, *vty.clone(), span));
        } else {
            ta.push(t.clone());
        }
    }
    ta
}

/// Rewrite a cross-module UFCS call: `Method { object, "module.func" }` becomes
/// `Module { module, func }` with the object prepended to the arguments.
///
/// The rewrite is what lets module-level monomorphization discover and rename
/// the callee. A capitalised prefix is a convention method (`Dog.repr`), not a
/// module, and is left alone.
fn rewrite_crossmodule_ufcs(ctx: &LowerCtx, target: &mut CallTarget, ir_args: &mut Vec<IrExpr>) {
    let CallTarget::Method { object, method } = &*target else { return };
    let Some((mod_str, func_str)) = method.as_str().split_once('.') else { return };
    if !mod_str.chars().next().is_some_and(|c| c.is_lowercase()) {
        return;
    }
    ir_args.insert(0, (**object).clone());
    *target = CallTarget::Module {
        module: sym(mod_str),
        func: sym(func_str),
        def_id: ctx.def_map.get(&sym(&format!("{}.{}", mod_str, func_str))).copied(),
    };
}

/// Rewrite UFCS on a LOCAL user function: `Method { object, "up" }` becomes
/// `Named { "up" }` with the object prepended to the arguments — the exact IR
/// the canonical spelling `up(object)` produces.
///
/// `f(x, y)` and `x.f(y)` are documented as equivalent, but the `Method` form
/// carried the receiver OUTSIDE `args`, so every downstream pass keyed on a
/// callee signature silently skipped it. Borrow inference was the visible one:
/// it wraps a call's args per the callee's `ParamBorrow` decisions and looks
/// callees up by name, and a dot-free `Method` matched no name at all — so a
/// `String`/`List[T]` parameter, which the signature pass renders as `&str` /
/// `&[T]`, received an owned value and rustc rejected the generated code after
/// `almide check` had passed (#898). Scalar params were unaffected because they
/// are never borrowed, and stdlib UFCS was unaffected because it resolves to a
/// `Module` target earlier in the guard chain.
///
/// Only a dot-free method that names a real top-level function is rewritten. A
/// dotted key is a convention/protocol method (`Dog.repr`, `T.show`) or the
/// cross-module form the rewrite above already handled, and a method that names
/// no function is the checker's error to report — inventing a `Named` call for
/// it would turn a diagnostic into a link failure.
fn rewrite_local_ufcs(ctx: &LowerCtx, target: &mut CallTarget, ir_args: &mut Vec<IrExpr>) {
    let CallTarget::Method { object, method } = &*target else { return };
    if method.as_str().contains('.') || !ctx.env.functions.contains_key(method) {
        return;
    }
    ir_args.insert(0, (**object).clone());
    *target = CallTarget::Named { name: *method };
}

/// Place named arguments into their positional slots, filling any gap from the
/// callee's defaults.
///
/// Named args arrive in SOURCE order, which need not be parameter order, so each
/// remaining parameter is matched by name. A parameter with neither a named
/// argument nor a default contributes nothing — the arity error is the checker's
/// to report, and inventing a placeholder here would hide it.
fn fill_named_args(
    ctx: &mut LowerCtx,
    ir_args: &mut Vec<IrExpr>,
    named_args: &[(almide_base::intern::Sym, ast::Expr)],
    target: &CallTarget,
) {
    let CallTarget::Named { name } = target else { return };
    let param_names: Vec<String> = ctx.env.functions.get(name)
        .map(|sig| sig.params.iter().map(|(n, _)| n.to_string()).collect())
        .unwrap_or_default();
    let defaults = ctx.fn_defaults.get(name).cloned();
    let positional_count = ir_args.len();
    if positional_count > param_names.len() {
        return;
    }
    let remaining = &param_names[positional_count..];
    ir_args.extend(remaining.iter().filter_map(|param_name| {
        named_args.iter()
            .find(|(n, _)| n == param_name)
            .map(|(_, expr)| lower_expr(ctx, expr))
            .or_else(|| defaults.as_ref()
                .and_then(|defs| defs.get(
                    positional_count + remaining.iter().position(|p| p == param_name).unwrap_or(0)))
                .and_then(|d| d.as_ref())
                .map(|default_expr| lower_expr(ctx, default_expr)))
    }));
}

/// #558: fill a UFCS call's missing default arguments.
///
/// A bare `x.foo()` lowers to a `Method` target whose object the EMITTER
/// prepends as argument 0, so the `Named` paths above never fire and a free fn
/// with defaults (`fn foo(a, b = 10)` called as `x.foo()`) reached codegen one
/// argument short — invalid Rust natively, a wasm stack underflow on the other
/// leg. The object counts as argument 0 here even though it is not in `ir_args`
/// yet.
fn fill_ufcs_defaults(ctx: &mut LowerCtx, ir_args: &mut Vec<IrExpr>, target: &CallTarget) {
    let CallTarget::Method { method, .. } = target else { return };
    if method.as_str().contains('.') {
        return;
    }
    let Some(defaults) = ctx.fn_defaults.get(method).cloned() else { return };
    let provided = 1 + ir_args.len();
    ir_args.extend(
        defaults.iter().skip(provided)
            .filter_map(|d| d.as_ref().map(|expr| lower_expr(ctx, expr)))
    );
}

/// ALS-T18: desugar the assert family OUTSIDE a test block into the normalized
/// abort form.
///
/// Desugaring ONCE here means every consumer (the native walker, the v1 MIR leg,
/// the interp oracle) inherits identical observables: one stderr line and exit
/// 1 — never a raw Rust panic (exit 101) or a bare wasm trap (exit 134). Fuzz
/// seed-20260718 index 10: `assert_eq` in `main` leaked the native panic banner
/// with exit 101 while wasm printed a value-less line with exit 1.
///
/// Test blocks keep the harness assertion forms, because cargo and the wasm test
/// runner are what report those.
fn desugar_assert_outside_test(
    ctx: &mut LowerCtx,
    target: &CallTarget,
    ir_args: &[IrExpr],
    span: Option<ast::Span>,
) -> Option<IrExpr> {
    if ctx.in_test {
        return None;
    }
    let CallTarget::Named { name } = target else { return None };
    let n = name.as_str();
    let is_assert = (matches!(n, "assert_eq" | "assert_ne") && ir_args.len() == 2)
        || (n == "assert" && !ir_args.is_empty());
    if !is_assert {
        return None;
    }
    Some(desugar_assert_abort(ctx, n, ir_args.to_vec(), span))
}

/// The type of the call node.
///
/// A call to a closure VALUE (a `Computed` target) has, by definition, the
/// callee's RETURN type — even when the inferred `ty` came back as the whole
/// `Fn` type, which happens for a HOF lambda parameter whose concrete type is
/// only fixed by the enclosing call's unification, after the body was checked.
/// Leaving the node typed `fn(..) -> T` makes a later `acc + f(x)` trip the IR
/// verifier with an `AddInt` over a function value.
fn call_result_ty(target: &CallTarget, ty: Ty) -> Ty {
    let CallTarget::Computed { callee } = target else { return ty };
    match &callee.ty {
        Ty::Fn { ret, .. } if !ret.has_unresolved_deep() => (**ret).clone(),
        _ => ty,
    }
}

/// The json Codec convenience prefix of [`lower_call`]: `json.encode(expr)` →
/// `json.stringify(T.encode(expr))` and `json.decode[T](text)` →
/// `T.decode(json.parse(text)?)`, when `expr`/`T` has a Codec-derived
/// convention fn. Verbatim text move — an independent guard chain that reads
/// only its own params and returns `Some(IrExpr)` on match, `None` (fall
/// through to the ordinary call-lowering path) otherwise.
fn lower_call_json_convenience(
    ctx: &mut LowerCtx,
    callee: &ast::Expr,
    args: &[ast::Expr],
    type_args: Option<&Vec<ast::TypeExpr>>,
    ty: Ty,
    span: Option<ast::Span>,
) -> Option<IrExpr> {
    let ast::ExprKind::Member { object, field, .. } = &callee.kind else { return None };
    let ast::ExprKind::Ident { name: module, .. } = &object.kind else { return None };
    if field == "encode" && args.len() == 1 {
        let arg_ty = ctx.expr_ty(&args[0]);
        if let Some(encode_fn) = ctx.find_convention_fn(&arg_ty, "encode") {
            let ir_arg = lower_expr(ctx, &args[0]);
            let encoded = ctx.mk(IrExprKind::Call {
                target: CallTarget::Named { name: encode_fn },
                args: vec![ir_arg], type_args: vec![],
            }, Ty::Named("Value".into(), vec![]), span);
            return Some(ctx.mk(IrExprKind::Call {
                target: CallTarget::Module { module: sym(module), func: sym("stringify"), def_id: ctx.def_map.get(&sym(&format!("{}.stringify", module))).copied() },
                args: vec![encoded], type_args: vec![],
            }, Ty::String, span));
        }
    }
    if field == "decode" && args.len() == 1
        && let Some(type_args) = type_args
        && let Some(ast::TypeExpr::Simple { name: type_name }) = type_args.first()
    {
        let ir_arg = lower_expr(ctx, &args[0]);
        // json.decode[T](text) → T.decode(json.parse(text)?)
        let parsed = ctx.mk(IrExprKind::Try { expr: Box::new(ctx.mk(IrExprKind::Call {
            target: CallTarget::Module { module: sym(module), func: sym("parse"), def_id: ctx.def_map.get(&sym(&format!("{}.parse", module))).copied() },
            args: vec![ir_arg], type_args: vec![],
        }, Ty::result(Ty::Named("Value".into(), vec![]), Ty::String), span)) },
        Ty::Named("Value".into(), vec![]), span);
        let decode_fn = sym(&format!("{}.decode", type_name));
        return Some(ctx.mk(IrExprKind::Call {
            target: CallTarget::Named { name: decode_fn },
            args: vec![parsed], type_args: vec![],
        }, ty, span));
    }
    None
}

/// The default-args fill stage of [`lower_call`], for calls WITHOUT named
/// args to a `Named` target. A default value that references an EARLIER
/// parameter (`fn rect(w, h: Int = w)`) must be filled with that parameter's
/// actual argument — the callee-local name does not exist at the call site
/// (rustc E0425) (#664). Build a param→value map from the provided args and
/// each already-filled default, then substitute before lowering. Guarded on
/// a 1:1 arg/param alignment so prepended const-type-args / UFCS objects
/// don't desync the mapping. Verbatim text move; mutates `ir_args` in place.
fn lower_call_fill_defaults(ctx: &mut LowerCtx, ir_args: &mut Vec<IrExpr>, args: &[ast::Expr], target: &CallTarget) {
    let CallTarget::Named { name } = target else { return };
    let Some(defaults) = ctx.fn_defaults.get(name).cloned() else { return };
    let param_names: Vec<Sym> = ctx.env.functions.get(name)
        .map(|sig| sig.params.iter().map(|(n, _)| almide_base::intern::sym(&n.to_string())).collect())
        .unwrap_or_default();
    let n_provided = ir_args.len();
    let aligned = n_provided == args.len() && !param_names.is_empty();
    let mut param_values: std::collections::HashMap<Sym, ast::Expr> = std::collections::HashMap::new();
    if aligned {
        for (j, arg) in args.iter().enumerate() {
            if let Some(pn) = param_names.get(j) { param_values.insert(*pn, arg.clone()); }
        }
    }
    for j in n_provided..defaults.len() {
        if let Some(default_expr) = defaults.get(j).and_then(|d| d.as_ref()) {
            if aligned {
                let mut d = default_expr.clone();
                substitute_call_params(&mut d, &param_values);
                if let Some(pn) = param_names.get(j) { param_values.insert(*pn, d.clone()); }
                ir_args.push(lower_expr(ctx, &d));
            } else {
                ir_args.push(lower_expr(ctx, default_expr));
            }
        }
    }
}

/// Stage 1b of [`lower_call`]: retype Int/Float literal args that flow into
/// sized numeric params (`Int32`, `UInt8`, `Float32`, ...). Mirrors the
/// let-binding coercion in `statements.rs::override_record_literal_ty` so
/// `f(42)` where `f(x: UInt32)` emits `f(42u32)` instead of an `i64` / `u32`
/// codegen mismatch. Verbatim text move; mutates `ir_args` in place.
fn lower_call_coerce_args(ctx: &mut LowerCtx, ir_args: &mut Vec<IrExpr>, target: &CallTarget) {
    if let CallTarget::Named { name } = target {
        lower_call_coerce_args_named(ctx, ir_args, name);
    } else if let CallTarget::Module { module, func, .. } = target {
        lower_call_coerce_args_module(ctx, ir_args, module, func);
    }
}

/// `CallTarget::Named` branch of [`lower_call_coerce_args`]: the
/// assert_eq/assert_ne width-matching special case, then coercion sourced
/// from a user fn's signature, a variant constructor's tuple payload, or a
/// dotted stdlib fn's signature. Verbatim text move.
fn lower_call_coerce_args_named(ctx: &mut LowerCtx, ir_args: &mut Vec<IrExpr>, name: &Sym) {
    lower_call_coerce_assert_macro(ctx, ir_args, name);
    if let Some(sig) = ctx.env.functions.get(name).cloned() {
        lower_call_coerce_from_sig(ctx, ir_args, &sig);
    } else if let Some((_, case)) = ctx.env.lookup_ctor(&almide_base::intern::sym(name)) {
        lower_call_coerce_from_ctor(ctx, ir_args, &case);
    } else if let Some((module, func)) = name.as_str().split_once('.') {
        if let Some(sig) = crate::stdlib::lookup_sig(module, func) {
            lower_call_coerce_from_sig(ctx, ir_args, &sig);
        }
    }
}

/// Builtin comparison macros (assert_eq / assert_ne) aren't registered in
/// env.functions, but their semantics demand width-matched operands on
/// both targets. Coerce literal-side args toward their typed peer here,
/// before the target-specific lowering picks up a Macro / RustMacro /
/// direct-emit path. Verbatim text move out of [`lower_call_coerce_args_named`].
fn lower_call_coerce_assert_macro(ctx: &mut LowerCtx, ir_args: &mut Vec<IrExpr>, name: &Sym) {
    if matches!(name.as_str(), "assert_eq" | "assert_ne") && ir_args.len() == 2 {
        let l_ty = ir_args[0].ty.clone();
        let r_ty = ir_args[1].ty.clone();
        super::statements::coerce_literal_to_sized(&mut ir_args[1], &l_ty, ctx.env);
        super::statements::coerce_literal_to_sized(&mut ir_args[0], &r_ty, ctx.env);
    }
}

/// Coerce each arg to its corresponding declared param type from a fn
/// signature. Shared by the by-name and dotted-stdlib branches of
/// [`lower_call_coerce_args_named`] (identical logic, two lookup sources)
/// and by [`lower_call_coerce_args_module`]. Verbatim text move.
fn lower_call_coerce_from_sig(ctx: &mut LowerCtx, ir_args: &mut Vec<IrExpr>, sig: &crate::types::FnSig) {
    for (i, (_, param_ty)) in sig.params.iter().enumerate() {
        if let Some(arg) = ir_args.get_mut(i) {
            super::statements::coerce_literal_to_sized(arg, param_ty, ctx.env);
        }
    }
}

/// Tuple-payload variant constructor (`Click(Int32, Int)`): narrow each
/// bare-literal arg to its declared payload type so `Click(42, 9)` emits
/// `Click(42i32, 9i64)` — without this the `42` stays `i64`, which native
/// rustc rejects (E0308) and WASM writes at the wrong byte width,
/// corrupting the next payload field. Mirrors the record-construction
/// coercion in `expressions.rs` (`declared_record_ty` path). Verbatim text
/// move out of [`lower_call_coerce_args_named`].
fn lower_call_coerce_from_ctor(ctx: &mut LowerCtx, ir_args: &mut Vec<IrExpr>, case: &crate::types::VariantCase) {
    if let crate::types::VariantPayload::Tuple(param_tys) = &case.payload {
        for (i, param_ty) in param_tys.iter().enumerate() {
            if let Some(arg) = ir_args.get_mut(i) {
                super::statements::coerce_literal_to_sized(arg, param_ty, ctx.env);
            }
        }
    }
}

/// `CallTarget::Module` branch of [`lower_call_coerce_args`]. Verbatim
/// text move.
fn lower_call_coerce_args_module(ctx: &mut LowerCtx, ir_args: &mut Vec<IrExpr>, module: &Sym, func: &Sym) {
    if let Some(sig) = crate::stdlib::lookup_sig(module.as_str(), func.as_str()) {
        lower_call_coerce_from_sig(ctx, ir_args, &sig);
    }
}

/// Unwrap `Result[T, _]` → `T`; any other type is returned unchanged.
/// Mirrors the effect-fn auto-`?` unwrap so a binding whose *stored* type still
/// carries the un-unwrapped `Result` is recognized by its Ok payload.
fn strip_result_ok(ty: &Ty) -> Ty {
    match ty {
        Ty::Applied(TypeConstructorId::Result, args) if !args.is_empty() => args[0].clone(),
        _ => ty.clone(),
    }
}

/// Build the ALS-T18 abort form for a non-test assert:
/// ```text
/// { let __a0 = l; let __a1 = r;
///   if <cond> then () else { eprintln("Error: assertion failed: …"); process.exit(1) } }
/// ```
/// Operands bind to temps FIRST so each evaluates exactly once (the failure
/// message re-references the temps, never re-runs the operand expressions).
/// The message forms: `assert_eq` → `Error: assertion failed: left = <l>,
/// right = <r>`; `assert_ne` → `Error: assertion failed: both = <l>`;
/// `assert(c)` → `Error: assertion failed`; `assert(c, msg)` → `Error:
/// assertion failed: <msg>`. Display of the operands is the ALS-R2
/// interpolation form (the same `${…}` rendering).
fn desugar_assert_abort(
    ctx: &mut LowerCtx,
    name: &str,
    ir_args: Vec<IrExpr>,
    span: Option<ast::Span>,
) -> IrExpr {
    let mut stmts: Vec<IrStmt> = Vec::new();
    let mut vars: Vec<IrExpr> = Vec::new();
    for (i, a) in ir_args.into_iter().enumerate() {
        let a_ty = a.ty.clone();
        let v = ctx.define_var(&format!("__assert_{i}"), a_ty.clone(), Mutability::Let, None);
        stmts.push(IrStmt {
            kind: IrStmtKind::Bind { var: v, mutability: Mutability::Let, ty: a_ty.clone(), value: a },
            span: None,
        });
        vars.push(ctx.mk(IrExprKind::Var { id: v }, a_ty, span));
    }
    let cond = match name {
        "assert_eq" => ctx.mk(
            IrExprKind::BinOp {
                op: BinOp::Eq,
                left: Box::new(vars[0].clone()),
                right: Box::new(vars[1].clone()),
            },
            Ty::Bool,
            span,
        ),
        "assert_ne" => ctx.mk(
            IrExprKind::BinOp {
                op: BinOp::Neq,
                left: Box::new(vars[0].clone()),
                right: Box::new(vars[1].clone()),
            },
            Ty::Bool,
            span,
        ),
        _ => vars[0].clone(),
    };
    let parts: Vec<IrStringPart> = match name {
        "assert_eq" => vec![
            IrStringPart::Lit { value: "Error: assertion failed: left = ".into() },
            IrStringPart::Expr { expr: vars[0].clone() },
            IrStringPart::Lit { value: ", right = ".into() },
            IrStringPart::Expr { expr: vars[1].clone() },
        ],
        "assert_ne" => vec![
            IrStringPart::Lit { value: "Error: assertion failed: both = ".into() },
            IrStringPart::Expr { expr: vars[0].clone() },
        ],
        _ if vars.len() >= 2 => vec![
            IrStringPart::Lit { value: "Error: assertion failed: ".into() },
            IrStringPart::Expr { expr: vars[1].clone() },
        ],
        _ => vec![IrStringPart::Lit { value: "Error: assertion failed".into() }],
    };
    let msg = ctx.mk(IrExprKind::StringInterp { parts }, Ty::String, span);
    let eprint = ctx.mk(
        IrExprKind::Call {
            target: CallTarget::Named { name: sym("eprintln") },
            args: vec![msg],
            type_args: vec![],
        },
        Ty::Unit,
        span,
    );
    let one = ctx.mk(IrExprKind::LitInt { value: 1 }, Ty::Int, span);
    let exit = ctx.mk(
        IrExprKind::Call {
            target: CallTarget::Module {
                module: sym("process"),
                func: sym("exit"),
                def_id: ctx.def_map.get(&sym("process.exit")).copied(),
            },
            args: vec![one],
            type_args: vec![],
        },
        Ty::Unit,
        span,
    );
    let fail = ctx.mk(
        IrExprKind::Block {
            stmts: vec![
                IrStmt { kind: IrStmtKind::Expr { expr: eprint }, span: None },
                IrStmt { kind: IrStmtKind::Expr { expr: exit }, span: None },
            ],
            expr: None,
        },
        Ty::Unit,
        span,
    );
    let ok = ctx.mk(IrExprKind::Block { stmts: vec![], expr: None }, Ty::Unit, span);
    let guard = ctx.mk(
        IrExprKind::If { cond: Box::new(cond), then: Box::new(ok), else_: Box::new(fail) },
        Ty::Unit,
        span,
    );
    ctx.mk(
        IrExprKind::Block {
            stmts: {
                stmts.push(IrStmt { kind: IrStmtKind::Expr { expr: guard }, span: None });
                stmts
            },
            expr: None,
        },
        Ty::Unit,
        span,
    )
}

include!("calls_target.rs");
