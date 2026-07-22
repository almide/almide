// ── Call lowering ───────────────────────────────────────────────

use almide_lang::ast;
use almide_ir::*;
use crate::types::{Ty, TypeConstructorId};
use almide_base::intern::{sym, Sym};
use super::LowerCtx;
use super::expressions::lower_expr;
use super::types::resolve_type_expr;

pub(super) fn lower_call(ctx: &mut LowerCtx, callee: &ast::Expr, args: &[ast::Expr], named_args: &[(almide_base::intern::Sym, ast::Expr)], type_args: Option<&Vec<ast::TypeExpr>>, ty: Ty, span: Option<ast::Span>) -> IrExpr {
    if let Some(converted) = lower_call_json_convenience(ctx, callee, args, type_args, ty.clone(), span) {
        return converted;
    }

    let mut ir_args: Vec<IrExpr> = Vec::new();
    let ta_raw: Vec<Ty> = type_args.map(|tas| tas.iter().map(|t| resolve_type_expr(t)).collect()).unwrap_or_default();

    // Extract const value type args and prepend them as positional arguments.
    // E.g., `make_list[3]("hello")` → `make_list(3, "hello")` at IR level.
    let mut ta = Vec::new();
    for t in &ta_raw {
        if let Ty::ConstValue { value, ty: vty } = t {
            ir_args.push(ctx.mk(IrExprKind::LitInt { value: *value }, *vty.clone(), span));
        } else {
            ta.push(t.clone());
        }
    }

    ir_args.extend(args.iter().map(|a| lower_expr(ctx, a)));
    let mut target = lower_call_target(ctx, callee);

    // Cross-module UFCS: Method { object, "module.func" } → Module { module, func }
    // with object prepended to args. This lets module-level mono discover and rename.
    if let CallTarget::Method { ref object, ref method } = target {
        if let Some(dot) = method.as_str().find('.') {
            let mod_str = &method.as_str()[..dot];
            let func_str = &method.as_str()[dot+1..];
            // Only convert if it's a user module (not Convention like Dog.repr)
            if mod_str.chars().next().map_or(false, |c| c.is_lowercase()) {
                let obj_expr = (**object).clone();
                ir_args.insert(0, obj_expr);
                target = CallTarget::Module { module: sym(mod_str), func: sym(func_str), def_id: ctx.def_map.get(&sym(&format!("{}.{}", mod_str, func_str))).copied() };
            }
        }
    }

    // Named args: resolve to positional order using function signature
    if let (false, CallTarget::Named { name }) = (named_args.is_empty(), &target) {
        let param_names: Vec<String> = ctx.env.functions.get(name)
            .map(|sig| sig.params.iter().map(|(n, _)| n.to_string()).collect())
            .unwrap_or_default();
        let defaults = ctx.fn_defaults.get(name).cloned();
        let positional_count = ir_args.len();
        ir_args.extend(param_names[positional_count..].iter().filter_map(|param_name| {
            named_args.iter()
                .find(|(n, _)| n == param_name)
                .map(|(_, expr)| lower_expr(ctx, expr))
                .or_else(|| defaults.as_ref()
                    .and_then(|defs| defs.get(positional_count + param_names[positional_count..].iter().position(|p| p == param_name).unwrap_or(0)))
                    .and_then(|d| d.as_ref())
                    .map(|default_expr| lower_expr(ctx, default_expr)))
        }));
    }

    // Default args: fill in remaining defaults (for calls without named args).
    if named_args.is_empty() {
        lower_call_fill_defaults(ctx, &mut ir_args, args, &target);
    }

    // #558: UFCS default-arg fill. A bare `x.foo()` lowers to a `Method`
    // target whose object the EMITTER prepends as arg 0 — but the Named
    // branches above never fire, so a free fn with defaults (`fn foo(a, b=10)`
    // called `x.foo()`) reached codegen one arg short (invalid Rust / wasm
    // stack underflow). When the method names a known free fn with defaults,
    // fill them here, counting the (not-yet-prepended) object as arg 0.
    if let CallTarget::Method { method, .. } = &target {
        if !method.as_str().contains('.') {
            if let Some(defaults) = ctx.fn_defaults.get(method).cloned() {
                // provided = object (1) + explicit positional args
                let provided = 1 + ir_args.len();
                ir_args.extend(
                    defaults.iter().skip(provided)
                        .filter_map(|d| d.as_ref().map(|expr| lower_expr(ctx, expr)))
                );
            }
        }
    }

    // Stage 1b: retype Int/Float literal args that flow into sized
    // numeric params (`Int32`, `UInt8`, `Float32`, ...).
    lower_call_coerce_args(ctx, &mut ir_args, &target);

    // ALS-T18: the assert family OUTSIDE a test block desugars ONCE into
    // the normalized abort form, so every consumer (the native walker, the v0
    // wasm emit, the v1 MIR leg, the interp oracle) inherits identical
    // observables: ONE stderr line + exit 1 — never a raw Rust panic (exit
    // 101) or a bare wasm trap (exit 134). Fuzz seed-20260718 index 10:
    // `assert_eq` in main leaked the native panic banner with exit 101 while
    // wasm printed a value-less line with exit 1. Test blocks keep the harness
    // assertion forms (cargo / the wasm test runner report those).
    if !ctx.in_test {
        if let CallTarget::Named { name } = &target {
            let n = name.as_str();
            if (matches!(n, "assert_eq" | "assert_ne") && ir_args.len() == 2)
                || (n == "assert" && !ir_args.is_empty())
            {
                return desugar_assert_abort(ctx, n, ir_args, span);
            }
        }
    }

    // A call to a closure VALUE (Computed target) has, by definition, the
    // callee's RETURN type — even when the inferred `ty` came back as the whole
    // `Fn` type (which happens for a HOF lambda parameter whose concrete type is
    // only fixed by the enclosing call's unification, after the body was checked).
    // Leaving the node typed `fn(..) -> T` makes a later `acc + f(x)` trip the IR
    // verifier (AddInt on a function value).
    let ty = match &target {
        CallTarget::Computed { callee } => match &callee.ty {
            Ty::Fn { ret, .. } if !ret.has_unresolved_deep() => (**ret).clone(),
            _ => ty,
        },
        _ => ty,
    };
    ctx.mk(IrExprKind::Call { target, args: ir_args, type_args: ta }, ty, span)
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
        // Builtin comparison macros (assert_eq / assert_ne) aren't
        // registered in env.functions, but their semantics demand
        // width-matched operands on both targets. Coerce literal-side
        // args toward their typed peer here, before the target-specific
        // lowering picks up a Macro / RustMacro / direct-emit path.
        if matches!(name.as_str(), "assert_eq" | "assert_ne") && ir_args.len() == 2 {
            let l_ty = ir_args[0].ty.clone();
            let r_ty = ir_args[1].ty.clone();
            super::statements::coerce_literal_to_sized(&mut ir_args[1], &l_ty, ctx.env);
            super::statements::coerce_literal_to_sized(&mut ir_args[0], &r_ty, ctx.env);
        }
        if let Some(sig) = ctx.env.functions.get(name).cloned() {
            for (i, (_, param_ty)) in sig.params.iter().enumerate() {
                if let Some(arg) = ir_args.get_mut(i) {
                    super::statements::coerce_literal_to_sized(arg, param_ty, ctx.env);
                }
            }
        } else if let Some((_, case)) = ctx.env.lookup_ctor(&almide_base::intern::sym(name)) {
            // Tuple-payload variant constructor (`Click(Int32, Int)`): narrow each
            // bare-literal arg to its declared payload type so `Click(42, 9)` emits
            // `Click(42i32, 9i64)` — without this the `42` stays `i64`, which native
            // rustc rejects (E0308) and WASM writes at the wrong byte width,
            // corrupting the next payload field. Mirrors the record-construction
            // coercion in `expressions.rs` (`declared_record_ty` path).
            if let crate::types::VariantPayload::Tuple(param_tys) = &case.payload {
                for (i, param_ty) in param_tys.iter().enumerate() {
                    if let Some(arg) = ir_args.get_mut(i) {
                        super::statements::coerce_literal_to_sized(arg, param_ty, ctx.env);
                    }
                }
            }
        } else if let Some((module, func)) = name.as_str().split_once('.') {
            if let Some(sig) = crate::stdlib::lookup_sig(module, func) {
                for (i, (_, param_ty)) in sig.params.iter().enumerate() {
                    if let Some(arg) = ir_args.get_mut(i) {
                        super::statements::coerce_literal_to_sized(arg, param_ty, ctx.env);
                    }
                }
            }
        }
    } else if let CallTarget::Module { module, func, .. } = target {
        if let Some(sig) = crate::stdlib::lookup_sig(module.as_str(), func.as_str()) {
            for (i, (_, param_ty)) in sig.params.iter().enumerate() {
                if let Some(arg) = ir_args.get_mut(i) {
                    super::statements::coerce_literal_to_sized(arg, param_ty, ctx.env);
                }
            }
        }
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

pub(super) fn lower_call_target(ctx: &mut LowerCtx, callee: &ast::Expr) -> CallTarget {
    match &callee.kind {
        ast::ExprKind::Ident { name, .. } | ast::ExprKind::TypeName { name, .. } => {
            // A name that resolves to a local binding is called *through that
            // variable* (Computed), never as a top-level function — a local
            // shadows any same-named fn, and Computed makes use-count / Perceus
            // liveness count the call as a use of the variable.
            //
            // Callability is decided from the callee's use-site type, which the
            // checker has already auto-`?`-unwrapped to the function type. The
            // var's *stored* type can still lag at `Result[Fn, _]` here: in an
            // effect fn the auto-`?` rewrite that unwraps the binding (auto_try)
            // runs AFTER lowering, so `var_table[add5].ty` is `Result[Fn, _]`
            // at this point. Reading only the stored type would mis-resolve
            // `add5(10)` to `Named { add5 }`, which has no such function — the
            // WASM emit then traps on an unresolved call and Perceus, seeing no
            // use of the binding, frees the closure before the call (use-after-free).
            // The Result-stripped stored type is a final fallback.
            if let Some(var_id) = ctx.lookup_var(name) {
                let use_ty = ctx.expr_ty(callee);
                let stored = ctx.var_table.get(var_id).ty.clone();
                if matches!(use_ty, Ty::Fn { .. })
                    || matches!(stored, Ty::Fn { .. })
                    || matches!(strip_result_ok(&stored), Ty::Fn { .. })
                {
                    let callee_ty = if matches!(use_ty, Ty::Fn { .. }) {
                        use_ty
                    } else {
                        strip_result_ok(&stored)
                    };
                    return CallTarget::Computed {
                        callee: Box::new(ctx.mk(IrExprKind::Var { id: var_id }, callee_ty, callee.span)),
                    };
                }
            }
            // Selective import: bare `from_string` → Module { json, from_string }.
            // (used-mark happens in checker pass; lowering only rewrites.)
            if let Some(module) = ctx.env.import_table.direct.get(name).copied() {
                return CallTarget::Module { module, func: *name, def_id: ctx.def_map.get(&sym(&format!("{}.{}", module, name))).copied() };
            }
            CallTarget::Named { name: *name }
        }
        ast::ExprKind::Member { object, field, .. } => lower_call_target_member(ctx, callee, object, field),
        _ => {
            let ir_callee = lower_expr(ctx, callee);
            CallTarget::Computed { callee: Box::new(ir_callee) }
        }
    }
}

/// The `Member { object, field }` arm of [`lower_call_target`] — resolves
/// `object.field(...)` to a module call, UFCS method, convention method,
/// protocol dispatch, or cross-module UFCS. Verbatim text move: each check is
/// an independent guard that either returns a resolved `CallTarget` or falls
/// through to the next, with no state shared across checks (codopsy pass,
/// mirrors `resolve_static_member`'s guard-chain shape).
fn lower_call_target_member(ctx: &mut LowerCtx, callee: &ast::Expr, object: &ast::Expr, field: &Sym) -> CallTarget {
    // `module.Type.method(...)` — a cross-module type's convention/Codec
    // method (`shapes.Dot.encode`). Resolve to the bare `Type.method` Named
    // call; the module prefix is reattached at codegen (#411-B). Mirrors the
    // checker's `resolve_static_member` (新①).
    if let ast::ExprKind::Member { object: inner, field: type_name } = &object.kind {
        if let ast::ExprKind::Ident { name: module, .. } = &inner.kind {
            if ctx.lookup_var(module).is_none()
                && ctx.env.import_table.resolve(module).is_some()
            {
                let key = sym(&format!("{}.{}", type_name, field));
                if ctx.env.functions.contains_key(&key) {
                    return CallTarget::Named { name: key };
                }
            }
        }
    }
    // Check if this is a module call (e.g., string.trim, list.map)
    if let ast::ExprKind::Ident { name: module, .. } = &object.kind {
        // Local variables take precedence over module names
        if ctx.lookup_var(module).is_none() && (module == "fan"
            || crate::stdlib::is_stdlib_module(module) || crate::stdlib::is_any_stdlib(module)
            || ctx.env.user_modules.contains(module)
            || ctx.env.import_table.aliases.contains_key(module))
        {
            // Cross-module variant constructor call: binary.ImportFunc(0)
            if let Some((type_name, _)) = ctx.env.lookup_ctor(field) {
                let resolved = ctx.env.import_table.aliases.get(module).copied()
                    .unwrap_or(*module);
                let qualified = format!("{}.{}", resolved.as_str(), type_name.as_str());
                if ctx.env.types.contains_key(&sym(&qualified)) {
                    return CallTarget::Named { name: *field };
                }
            }
            let resolved = ctx.env.import_table.aliases.get(module).copied()
                .unwrap_or(*module);
            return CallTarget::Module { module: resolved, func: *field, def_id: ctx.def_map.get(&sym(&format!("{}.{}", resolved, field))).copied() };
        }
        // Ident that's not a module: check if Type.method (protocol impl, e.g. Val.double)
        if ctx.lookup_var(module).is_none() {
            let key = format!("{}.{}", module, field);
            if ctx.env.functions.contains_key(&sym(&key))
                || ctx.find_convention_fn(&Ty::Named(sym(module), vec![]), field).is_some()
            {
                return CallTarget::Named { name: sym(&key) };
            }
        }
    }
    // Dot-chain submodule fallback: still resolve so codegen doesn't break
    // (checker emits error for these, but lowering must still produce valid IR)
    if let Some(dotted) = ctx.env.import_table.resolve_dotted_path(&object.kind) {
        return CallTarget::Module { module: sym(&dotted), func: *field, def_id: ctx.def_map.get(&sym(&format!("{}.{}", dotted, field))).copied() };
    }
    // TypeName.method(args) → direct named call (not UFCS, no object prepend)
    if let ast::ExprKind::TypeName { name: type_name, .. } = &object.kind {
        let key = format!("{}.{}", type_name, field);
        if ctx.env.functions.contains_key(&sym(&key))
            || ctx.find_convention_fn(&Ty::Named(sym(type_name), vec![]), field).is_some()
        {
            return CallTarget::Named { name: sym(&key) };
        }
    }
    // Record field call: h.run("hello") where run is a Fn-typed field
    // Must check before UFCS so field-access + call takes priority
    let obj_ty = ctx.expr_ty(object);
    {
        let resolved = ctx.env.resolve_named(&obj_ty);
        let fn_field = match &resolved {
            Ty::Record { fields } | Ty::OpenRecord { fields } => {
                fields.iter().find(|(n, _)| *n == *field)
                    .and_then(|(_, t)| if matches!(t, Ty::Fn { .. }) { Some(()) } else { None })
            }
            _ => None,
        };
        if fn_field.is_some() {
            let ir_obj = lower_expr(ctx, object);
            let field_ty = ctx.expr_ty(callee);
            let member = ctx.mk(IrExprKind::Member { object: Box::new(ir_obj), field: *field }, field_ty, callee.span);
            return CallTarget::Computed { callee: Box::new(member) };
        }
    }
    // Built-in generic types: xs.len() → list.len(xs) for List, Map, etc.
    let builtin_module = match &obj_ty {
        Ty::Applied(TypeConstructorId::List, _) => Some("list"),
        Ty::Applied(TypeConstructorId::Map, _) => Some("map"),
        Ty::Applied(TypeConstructorId::Set, _) => Some("set"),
        Ty::String => Some("string"),
        Ty::Int => Some("int"),
        Ty::Float => Some("float"),
        // Sized numeric types (Stage 3 of the sized-numeric-types arc).
        Ty::Int8 => Some("int8"),
        Ty::Int16 => Some("int16"),
        Ty::Int32 => Some("int32"),
        Ty::UInt8 => Some("uint8"),
        Ty::UInt16 => Some("uint16"),
        Ty::UInt32 => Some("uint32"),
        Ty::UInt64 => Some("uint64"),
        Ty::Float32 => Some("float32"),
        Ty::Applied(TypeConstructorId::Result, _) => Some("result"),
        Ty::Applied(TypeConstructorId::Option, _) => Some("option"),
        _ => None,
    };
    if let Some(module) = builtin_module {
        let key = format!("{}.{}", module, field);
        if ctx.env.functions.contains_key(&sym(&key))
            || crate::stdlib::resolve_ufcs_candidates(field).contains(&module)
        {
            let ir_obj = lower_expr(ctx, object);
            return CallTarget::Method { object: Box::new(ir_obj), method: sym(&key) };
        }
    }
    // Check for convention method: dog.repr() → Dog.repr(dog)
    let type_name_opt = match &obj_ty {
        Ty::Named(name, _) => Some(name.to_string()),
        Ty::Record { .. } | Ty::Variant { .. } => {
            ctx.env.types.iter().find_map(|(name, ty)| {
                if ty == &obj_ty && name.chars().next().map_or(false, |c| c.is_uppercase()) {
                    Some(name.to_string())
                } else { None }
            })
        }
        _ => None,
    };
    if let Some(type_name) = type_name_opt {
        let convention_key = format!("{}.{}", type_name, field);
        if ctx.env.functions.contains_key(&sym(&convention_key))
            || ctx.find_convention_fn(&Ty::Named(sym(&type_name), vec![]), field).is_some()
        {
            let ir_obj = lower_expr(ctx, object);
            return CallTarget::Method { object: Box::new(ir_obj), method: sym(&convention_key) };
        }
    }
    // Protocol method on TypeVar: item.show() where item: T, T: Showable
    // Lower as "T.show" convention key — monomorphizer will substitute T → ConcreteType
    // Also check variable's declared type: inside lambdas, the type checker may
    // resolve the expression type to Fn (partial application), but the variable's
    // declared type retains the TypeVar.
    // Also check for TypeVar behind Fn wrapper: inside lambdas, the type checker
    // may assign Fn type to the parameter (partial application of protocol method),
    // but the generic function's param list may have the real TypeVar.
    let tv_from_obj = match &obj_ty {
        Ty::TypeVar(tv) => Some(tv.clone()),
        _ => {
            // Check all protocol bounds to see if this method belongs to one,
            // and identify which TypeVar it corresponds to.
            let mut found = None;
            for (tv, protos) in ctx.protocol_bounds.iter() {
                for proto_name in protos {
                    if let Some(proto_def) = ctx.env.protocols.get(&sym(proto_name)) {
                        if proto_def.methods.iter().any(|m| m.name == *field) {
                            found = Some(tv.clone());
                            break;
                        }
                    }
                }
                if found.is_some() { break; }
            }
            found
        }
    };
    if let Some(tv) = tv_from_obj {
        if let Some(proto_names) = ctx.protocol_bounds.get(&tv).cloned() {
            for proto_name in &proto_names {
                if let Some(proto_def) = ctx.env.protocols.get(&sym(proto_name)) {
                    if proto_def.methods.iter().any(|m| m.name == *field) {
                        let ir_obj = lower_expr(ctx, object);
                        let convention_key = sym(&format!("{}.{}", tv, field));
                        return CallTarget::Method { object: Box::new(ir_obj), method: convention_key };
                    }
                }
            }
        }
    }
    // Cross-module UFCS: object type is Named → find defining module
    if let Ty::Named(type_name, _) = &obj_ty {
        // A pinned QUALIFIED name (`box.Box`) carries its defining
        // module directly (same repair as the checker's UFCS arm —
        // the suffix scan only matched historical bare names).
        let defining_module = match type_name.as_str().rsplit_once('.') {
            Some((m, _)) => Some(m.to_string()),
            None => ctx.env.types.keys()
                .find(|k| {
                    let s = k.as_str();
                    s.ends_with(&format!(".{}", type_name.as_str()))
                        && s.len() > type_name.as_str().len() + 1
                })
                .map(|k| k.as_str()[..k.as_str().len() - type_name.as_str().len() - 1].to_string()),
        };
        if let Some(module) = defining_module {
            let key = format!("{}.{}", module, field);
            if ctx.env.functions.contains_key(&sym(&key)) {
                let ir_obj = lower_expr(ctx, object);
                // Return Method with "module.func" key — lower_call converts to Module target
                return CallTarget::Method { object: Box::new(ir_obj), method: sym(&key) };
            }
        }
    }
    // Generic method call: obj.method(args) → UFCS
    let ir_obj = lower_expr(ctx, object);
    CallTarget::Method { object: Box::new(ir_obj), method: *field }
}

/// Replace each `Ident { name }` that names a call parameter with the AST of the
/// value bound to that parameter, used to fill a default value that references an
/// earlier parameter (`fn rect(w, h: Int = w)`) with the actual argument instead
/// of the callee-local name, which is out of scope at the call site (E0425) (#664).
/// A self-referential argument (`rect(w)` passing a caller-local `w` for param
/// `w`) is left untouched: that name already resolves correctly at the call site,
/// and replacing it would re-enter this pre-order visitor forever.
fn substitute_call_params(expr: &mut ast::Expr, param_values: &std::collections::HashMap<Sym, ast::Expr>) {
    ast::visit_expr_mut(expr, &mut |e| {
        if let ast::ExprKind::Ident { name } = &e.kind {
            if let Some(repl) = param_values.get(name) {
                if !matches!(&repl.kind, ast::ExprKind::Ident { name: rn } if rn == name) {
                    *e = repl.clone();
                }
            }
        }
    });
}
