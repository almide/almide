fn render_iter_chain(ctx: &RenderContext, source: &IrExpr, consume: bool, steps: &[IterStep], collector: &IterCollector) -> String {
    let src = render_expr(ctx, source);
    let mut chain = if consume {
        format!("({}).into_iter()", src)
    } else {
        format!("({}).iter()", src)
    };

    for step in steps {
        match step {
            IterStep::Map { lambda } => chain = format!("{}.map({})", chain, render_expr(ctx, lambda)),
            IterStep::Filter { lambda } => chain = format!("{}.filter({})", chain, render_expr(ctx, lambda)),
            IterStep::FlatMap { lambda } => chain = format!("{}.flat_map({})", chain, render_expr(ctx, lambda)),
            IterStep::FilterMap { lambda } => chain = format!("{}.filter_map({})", chain, render_expr(ctx, lambda)),
        }
    }

    match collector {
        IterCollector::Collect => format!("{}.collect::<Vec<_>>()", chain),
        IterCollector::Fold { init, lambda } => format!("{}.fold({}, {})", chain, render_expr(ctx, init), render_expr(ctx, lambda)),
        IterCollector::Any { lambda } => format!("{}.any({})", chain, render_expr(ctx, lambda)),
        IterCollector::All { lambda } => format!("{}.all({})", chain, render_expr(ctx, lambda)),
        IterCollector::Find { lambda } => format!("{}.find({})", chain, render_expr(ctx, lambda)),
        IterCollector::Count { lambda } => format!("{}.filter({}).count() as i64", chain, render_expr(ctx, lambda)),
    }
}

// ── Binary operator rendering ──

/// #617: the matrix operators return a RAW AlmideMatrix from the runtime —
/// wrap into the RcCow value shape like every runtime-call result.
/// Extracted from `render_binop`'s BinOp::{Mul,Add,Sub,Scale}Matrix arms
/// (cog>30 decomposition, pattern 2, uniform-shaped arms grouped by a
/// shared theme). Only ever called for those four ops.
fn render_binop_matrix(ctx: &RenderContext, op: BinOp, left: &IrExpr, l: &str, r: &str) -> String {
    match op {
        BinOp::MulMatrix => rc_cow_result_glue(
            ctx.templates.render_with("matrix_mul", None, &[], &[("left", l), ("right", r)])
                .unwrap_or_else(|| format!("almide_rt_matrix_mul(&{}, &{})", l, r)),
            &Ty::Matrix,
        ),
        BinOp::AddMatrix => rc_cow_result_glue(
            ctx.templates.render_with("matrix_add", None, &[], &[("left", l), ("right", r)])
                .unwrap_or_else(|| format!("almide_rt_matrix_add(&{}, &{})", l, r)),
            &Ty::Matrix,
        ),
        BinOp::SubMatrix => rc_cow_result_glue(
            ctx.templates.render_with("matrix_sub", None, &[], &[("left", l), ("right", r)])
                .unwrap_or_else(|| format!("almide_rt_matrix_sub(&{}, &{})", l, r)),
            &Ty::Matrix,
        ),
        BinOp::ScaleMatrix => {
            // Ensure matrix is first arg, scalar is second
            let (mat, scalar) = if matches!(&left.ty, Ty::Matrix) {
                (l, r)
            } else {
                (r, l)
            };
            rc_cow_result_glue(
                ctx.templates.render_with("matrix_scale", None, &[], &[("left", mat), ("right", scalar)])
                    .unwrap_or_else(|| format!("almide_rt_matrix_scale(&{}, {})", mat, scalar)),
                &Ty::Matrix,
            )
        }
        _ => unreachable!("render_binop_matrix called for non-matrix BinOp"),
    }
}

/// Bare infix operator string for ops that render via the generic
/// `binary_op` template (not one with dedicated totality-macro/matrix
/// handling above). Extracted from `render_binop`'s fallback arm.
fn binop_symbol(op: BinOp) -> &'static str {
    match op {
        BinOp::AddInt | BinOp::AddFloat => "+",
        BinOp::SubInt | BinOp::SubFloat => "-",
        BinOp::MulInt | BinOp::MulFloat => "*",
        // DivInt/ModInt are matched above (totality macros) and must never
        // reach this bare-operator fallback — fall to "??" loudly if they do.
        BinOp::DivFloat => "/",
        BinOp::ModFloat => "%",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::Lte => "<=",
        BinOp::Gte => ">=",
        BinOp::And => "&&",
        BinOp::Or => "||",
        _ => "??",
    }
}

fn render_binop(ctx: &RenderContext, op: BinOp, left: &IrExpr, right: &IrExpr, _ty: &Ty) -> String {
    let l = render_expr(ctx, left);
    let r = render_expr(ctx, right);

    // Type-dispatched operators
    match op {
        BinOp::ConcatStr | BinOp::ConcatList => {
            let ty_tag = if op == BinOp::ConcatStr { "String" } else { "List" };
            // Unwrap RcCow operands to owned T for concat
            let lo = render_expr_owned(ctx, left);
            let ro = render_expr_owned(ctx, right);
            ctx.templates.render_with("concat_expr", Some(ty_tag), &[], &[("left", lo.as_str()), ("right", ro.as_str())])
                .unwrap_or_else(|| format!("concat(_, _)"))
        }
        BinOp::MulMatrix | BinOp::AddMatrix | BinOp::SubMatrix | BinOp::ScaleMatrix =>
            render_binop_matrix(ctx, op, left, l.as_str(), r.as_str()),
        BinOp::Eq => {
            ctx.templates.render_with("eq_expr", None, &[], &[("left", l.as_str()), ("right", r.as_str())])
                .unwrap_or_else(|| format!("_ == _"))
        }
        BinOp::Neq => {
            ctx.templates.render_with("ne_expr", None, &[], &[("left", l.as_str()), ("right", r.as_str())])
                .unwrap_or_else(|| format!("_ != _"))
        }
        BinOp::PowInt => {
            ctx.templates.render_with("power_expr", Some("Int"), &[], &[("left", l.as_str()), ("right", r.as_str())])
                .unwrap_or_else(|| format!("pow(_, _)"))
        }
        // Integer `/` and `%` are total: a zero divisor or signed MIN/-1 overflow
        // aborts via the almide_div!/almide_mod! prelude macros (`Error: <msg>\n` +
        // exit 1) instead of panicking. The macro is generic over all int widths and
        // is not const-evaluable, so a literal `10 / 0` compiles (no rustc
        // `unconditional_panic`) and aborts at runtime — matching the WASM trap.
        // Float div/mod keep IEEE semantics and fall through to the bare-operator arm.
        BinOp::DivInt => {
            ctx.templates.render_with("div_int", None, &[], &[("left", l.as_str()), ("right", r.as_str())])
                .unwrap_or_else(|| format!("almide_div!({}, {})", l, r))
        }
        BinOp::ModInt => {
            ctx.templates.render_with("mod_int", None, &[], &[("left", l.as_str()), ("right", r.as_str())])
                .unwrap_or_else(|| format!("almide_mod!({}, {})", l, r))
        }
        BinOp::PowFloat => {
            ctx.templates.render_with("power_expr", Some("Float"), &[], &[("left", l.as_str()), ("right", r.as_str())])
                .unwrap_or_else(|| format!("pow(_, _)"))
        }
        _ => {
            let op_str = binop_symbol(op);
            let op_s = op_str.to_string();
            ctx.templates.render_with("binary_op", None, &[], &[("left", l.as_str()), ("op", op_s.as_str()), ("right", r.as_str())])
                .unwrap_or_else(|| format!("({} {} {})", "l", op_str, "r"))
        }
    }
}

/// Render a generic call expression (Named, Method, or Computed target).
/// Shared trailing step of every `render_generic_call` arm: given a
/// fully-resolved callee expression string, render `callee(args...)` via
/// the `call_expr` template. Extracted from `render_generic_call`'s
/// post-match tail (cog>30 decomposition): each arm below now calls this
/// itself instead of falling through to one shared tail, since several
/// arms also need to bypass it entirely via an early `return`.
fn render_call_expr(ctx: &RenderContext, callee: &str, args: &[IrExpr]) -> String {
    // A closure ARG is already `Rc<dyn Fn>`: the box-by-default pass boxed every
    // closure literal where it sits — including a Named user-HOF arg, whose param
    // is `Rc<dyn Fn>` under the uniform repr (top-level fns no longer take
    // `impl Fn`). No call-site boxing here — re-wrapping a capture-clone
    // `{ let __cap; lambda }` arg (kind `Block`, not `RcWrap`) double-boxed it.
    let args_str = args.iter().map(|a| render_expr_owned(ctx, a))
        .collect::<Vec<_>>().join(", ");
    ctx.templates.render_with("call_expr", None, &[], &[("callee", callee), ("args", args_str.as_str())])
        .unwrap_or_else(|| format!("call(...)"))
}

/// `CallTarget::Named` case of `render_generic_call`, extracted verbatim
/// (cog>30 decomposition, pattern 2: uniform match arms on `CallTarget`,
/// mirrors the `lower_expr`/`infer_expr_inner` extraction shape).
fn render_generic_call_named(ctx: &RenderContext, name: almide_base::intern::Sym, args: &[IrExpr], result_ty: &almide_lang::types::Ty) -> String {
    // Invariant: NormalizeRuntimeCallsPass collapses every
    // `Named { "almide_rt_*" }` into `RuntimeCall { symbol }`.
    // A `Named` target reaching the walker therefore must
    // refer to a user-defined or external function — never
    // a runtime helper. If this assertion fires, a generator
    // produced a `Named { "almide_rt_*" }` after the
    // normalize pass, or the pass was removed from the
    // pipeline.
    assert!(
        !name.as_str().starts_with("almide_rt_"),
        "walker received Named call with reserved runtime prefix: {} \
         (expected RuntimeCall — see pass_normalize_runtime_calls)",
        name.as_str()
    );
    if let Some(mapped) = ctx.ann.ctor_to_enum.get(name.as_str()) {
        // `name` is a variant constructor. The global `ctor_to_enum` map
        // collapses a constructor name shared across packages to the
        // last-registered enum (#413). When the construction's RESOLVED
        // type (`.ty`, disambiguated by the type checker) names a DIFFERENT
        // but valid enum, prefer it; otherwise keep the mapped enum (no
        // change for the common, non-colliding case — and for non-variant
        // ctors like newtypes where `.ty` isn't a known enum).
        let enum_name = match result_ty {
            almide_lang::types::Ty::Named(n, _)
                if n.as_str() != mapped.as_str()
                   && ctx.ann.ctor_to_enum.values().any(|e| e.as_str() == n.as_str())
                => n.to_string(),
            _ => mapped.clone(),
        };
        return render_enum_constructor(ctx, &name, &enum_name, args);
    }
    // Convention methods: "Type.method" → "Type_method" (free functions in all targets)
    let callee = if name.contains('.') {
        name.replace('.', "_")
    } else {
        // A user fn named `box`/`move`/`dyn`/… is escaped at its
        // definition; the call site must match or rustc rejects `box(…)`
        // as a reserved keyword (#659).
        super::escape_rust_ident(name.as_str(), ctx.templates)
    };
    render_call_expr(ctx, &callee, args)
}

/// `CallTarget::Method` case of `render_generic_call`, extracted verbatim.
fn render_generic_call_method(ctx: &RenderContext, object: &IrExpr, method: almide_base::intern::Sym, args: &[IrExpr]) -> String {
    if let Some(full) = render_method_call_full(ctx, object, &method, args) {
        return full;
    }
    // Val-wrapped var: mutating method calls need .make_mut()
    if let IrExprKind::Var { id } = &object.kind {
        if ctx.ann.is_rc_cow(id) {
            let is_mutating_method = matches!(method.as_str(),
                "push" | "pop" | "clear" | "extend" | "insert" | "remove"
                | "sort" | "sort_by" | "reverse" | "truncate" | "retain");
            if is_mutating_method {
                let var_name = ctx.var_name(*id).to_string();
                let args_str = args.iter().map(|a| render_expr(ctx, a))
                    .collect::<Vec<_>>().join(", ");
                if args_str.is_empty() {
                    return format!("{}.make_mut().{}()", var_name, method);
                } else {
                    return format!("{}.make_mut().{}({})", var_name, method, args_str);
                }
            }
        }
    }
    let callee = {
        let obj_str = render_expr(ctx, object);
        // Wrap in parens if the object expression needs it for method call precedence
        let needs_parens = matches!(&object.kind,
            IrExprKind::UnOp { .. } | IrExprKind::BinOp { .. }
            | IrExprKind::If { .. } | IrExprKind::Match { .. }
        ) || matches!(&object.kind, IrExprKind::LitFloat { value } if *value < 0.0)
          || matches!(&object.kind, IrExprKind::LitInt { value } if *value < 0);
        if needs_parens {
            format!("({}).{}", obj_str, method)
        } else {
            format!("{}.{}", obj_str, method)
        }
    };
    render_call_expr(ctx, &callee, args)
}

/// `CallTarget::Computed` case of `render_generic_call`, extracted verbatim.
fn render_generic_call_computed(ctx: &RenderContext, callee_expr: &IrExpr, args: &[IrExpr]) -> String {
    // Pipe terminus case: `expr |> (lambda)` lowers to `(lambda)(expr)`.
    // The lambda is the computed callee. Here — and ONLY here — we
    // annotate the lambda's params so rustc can infer types.
    // (Lambda elsewhere, e.g. as arg to `.filter(...)`, must stay
    // unannotated because iterator adapters want `&T` not `T`.)
    let callee = if let IrExprKind::Lambda { params, body, .. } = &callee_expr.kind {
        let params_str = params.iter()
            .map(|(id, ty)| {
                let name = ctx.var_name(*id).to_string();
                if ty.has_unresolved_deep() {
                    name
                } else {
                    let ty_str = super::types::render_type(ctx, ty);
                    format!("{}: {}", name, ty_str)
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        let body_str = render_expr(ctx, body);
        format!("(move |{}| {})", params_str, body_str)
    } else {
        // Parenthesize ANY computed callee: a `Member` (h.run)("x"), a
        // `??`/`match`/`if` that yields a closure — `match … {}(x)` is
        // invalid Rust ("expected ;"), `(match … {})(x)` is correct — and a
        // bare `(f)(x)` is harmless.
        format!("({})", render_expr(ctx, callee_expr))
    };
    render_call_expr(ctx, &callee, args)
}

fn render_generic_call(ctx: &RenderContext, target: &CallTarget, args: &[IrExpr], result_ty: &almide_lang::types::Ty) -> String {
    match target {
        CallTarget::Named { name } => render_generic_call_named(ctx, *name, args, result_ty),
        CallTarget::Method { object, method } => render_generic_call_method(ctx, object, *method, args),
        CallTarget::Computed { callee } => render_generic_call_computed(ctx, callee, args),
        CallTarget::Module { .. } => unreachable!(),
    }
}

/// Render a method call as a full expression for UFCS and module.func patterns.
/// Returns Some(full_expr) if the method call was handled, None for normal obj.method calls.
///
/// Dispatch strategy (type-driven):
///   1. join fallback (StdlibLowering miss)
///   2. Dot-qualified → module/convention
///   3. User-defined type (Ty::Named) → ALWAYS UFCS free function
///   4. Builtin type + native Rust method → None (emit obj.method())
///   5. Builtin type + non-native method → UFCS (user fn on builtin type)
fn render_method_call_full(ctx: &RenderContext, object: &IrExpr, method: &str, args: &[IrExpr]) -> Option<String> {
    // 1. join fallback: route to almide_rt_list_join when StdlibLowering missed it
    if method == "join" && args.len() == 1 {
        let obj_str = render_expr(ctx, object);
        let sep_str = render_expr(ctx, &args[0]);
        return Some(format!("almide_rt_list_join(&{}, &*{})", obj_str, sep_str));
    }

    // 2. Dot-qualified: handled after type-based dispatch (below)

    // 3. User-defined types: ALWAYS UFCS — Rust structs have no methods.
    //    Derive monomorphized name from type args when generic.
    if let Ty::Named(_type_name, type_args) = &object.ty {
        if !method.contains('.') {
            let obj_str = render_expr_owned(ctx, object);
            let mut all_args = vec![obj_str];
            all_args.extend(args.iter().map(|a| render_expr_owned(ctx, a)));
            let func_name = if type_args.is_empty() {
                method.to_string()
            } else {
                let suffix = type_args.iter()
                    .map(|t| mangle_ty_for_mono(t))
                    .collect::<Vec<_>>().join("_");
                format!("{}__{}", method, suffix)
            };
            return Some(format!("{}({})", func_name, all_args.join(", ")));
        }
    }

    // 4+5. Builtin types: distinguish native Rust methods from user UFCS.
    //    Native methods exist on the Rust type and should remain as obj.method().
    //    Non-native methods are user-defined UFCS → func(obj, args).
    let is_native_rust_method = matches!(method,
        // Universal traits (Clone, Display)
        "clone" | "to_string"
        // Vec<T>
        | "len" | "push" | "pop" | "insert" | "remove" | "contains"
        | "iter" | "into_iter" | "collect" | "is_empty" | "to_vec" | "get"
        // HashMap<K,V>
        | "keys" | "values" | "contains_key" | "entry" | "or_insert"
        // String
        | "split" | "trim" | "starts_with" | "ends_with" | "replace" | "chars"
        | "to_owned" | "as_str"
        // Option<T> / Result<T,E>
        | "is_some" | "is_none" | "unwrap" | "unwrap_or" | "expect"
        | "ok" | "err" | "and_then" | "map_err" | "unwrap_or_else"
        | "ok_or" | "flatten" | "as_ref" | "as_deref"
        // Iterator adapters (from collect/filter chains)
        | "map" | "filter"
        // f64 math
        | "abs" | "powi" | "powf" | "sqrt" | "floor" | "ceil" | "round"
        | "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "atan2"
        | "exp" | "ln" | "log2" | "log10" | "is_nan" | "is_infinite"
    );

    if !method.contains('.') && !is_native_rust_method {
        // 5. Non-native method on builtin type → UFCS
        let obj_str = render_expr(ctx, object);
        let mut all_args = vec![obj_str];
        all_args.extend(args.iter().map(|a| render_expr(ctx, a)));
        return Some(format!("{}({})", method, all_args.join(", ")));
    }
    // Module.func or Convention method UFCS
    if let Some(dot_pos) = method.find('.') {
        let obj_str = render_expr(ctx, object);
        let mut all_args = vec![obj_str];
        all_args.extend(args.iter().map(|a| render_expr(ctx, a)));
        let module = &method[..dot_pos];
        let func = &method[dot_pos+1..];
        // Convention methods (Type.method): first char uppercase → emit as Type_method(args)
        let is_convention = module.chars().next().map_or(false, |c| c.is_uppercase());
        if is_convention {
            let callee = format!("{}_{}", module, func);
            return Some(format!("{}({})", callee, all_args.join(", ")));
        }
        return Some(ctx.templates.render_with("module_call", None, &[], &[("module", module), ("func", func), ("args", all_args.join(", ").as_str())])
            .unwrap_or_else(|| format!("{}.{}()", module, func)));
    }
    None
}

/// Render an enum constructor call with optional Box wrapping for recursive types.
/// For a `Result<Ok, Err>` type, render the `(ok, err)` turbofish arguments used
/// to pin a `Ok(..)` / `Err(..)` constructor's phantom type parameters. The error
/// type defaults to `String` when still unresolved (`Unknown` or an inference
/// typevar), exactly as the `render_type` Result arm does for type positions
/// (dv_13). Returns `None` for a non-Result type, leaving the bare constructor.
fn result_turbofish_args(ctx: &RenderContext, ty: &Ty) -> Option<(String, String)> {
    match ty {
        Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => {
            let ok_s = render_type(ctx, &args[0]);
            let err_s = match &args[1] {
                Ty::Unknown => "String".to_string(),
                Ty::TypeVar(n) if n.starts_with('?') => "String".to_string(),
                _ => render_type(ctx, &args[1]),
            };
            Some((ok_s, err_s))
        }
        _ => None,
    }
}

fn render_enum_constructor(ctx: &RenderContext, ctor_name: &str, enum_name: &str, args: &[IrExpr]) -> String {
    let boxed_args: Vec<String> = args.iter().enumerate().map(|(i, a)| {
        let rendered = render_expr(ctx, a);
        let needs_box = ctx.ann.recursive_enums.contains(enum_name)
            && (ty_contains_name(&a.ty, enum_name)
                || ctx.ann.boxed_fields.contains(&(ctor_name.to_string(), format!("{}", i))));
        // Unwrap RcCow for var bindings used as variant constructor args.
        let is_rc_cow_var = matches!(&a.kind, IrExprKind::Var { id } if ctx.ann.is_rc_cow(id));
        let rendered = if is_rc_cow_var {
            format!("{}.into_inner()", rendered)
        } else { rendered };
        // A closure payload field is `Rc<dyn Fn>`; the box-by-default pass already
        // boxed the closure value (a direct lambda → `RcWrap`, a capture-clone
        // `{ let __cap; lambda }` → boxes the tail, a `Var` is already `Rc`), so no
        // ctor-side boxing — wrapping again here double-boxed `Block`-shaped args.
        if needs_box {
            format!("std::boxed::Box::new({})", rendered)
        } else {
            rendered
        }
    }).collect();
    let args_str = boxed_args.join(", ");
    if args.is_empty() {
        ctx.templates.render_with("ctor_unit", None, &[], &[("enum_name", enum_name), ("ctor_name", ctor_name), ("args", args_str.as_str())])
            .unwrap_or_else(|| format!("{}::{}", enum_name, ctor_name))
    } else {
        ctx.templates.render_with("ctor_call", None, &[], &[("enum_name", enum_name), ("ctor_name", ctor_name), ("args", args_str.as_str())])
            .unwrap_or_else(|| format!("{}::{}({})", enum_name, ctor_name, args_str))
    }
}

/// Wrap a rendered branch with `.to_string()` when it likely evaluates to
/// `&str` at the Rust level but the surrounding context needs an owned
/// `String`. Used by the If walker to avoid type mismatches between a
/// literal branch (already `String` via `"...".to_string()`) and a bare
/// `Var` branch whose param was emitted as `&str`.
///
/// Applied only when:
///   - the expression is a bare `Var` of `Ty::String`, or
///   - the expression is a block whose tail is such a Var.
///
/// Rust's `.to_string()` is idempotent on owned `String` (it clones), so
/// even when the branch is already owned the result is correct, just with
/// one redundant allocation.
fn coerce_to_owned_string(rendered: &str, expr: &IrExpr) -> String {
    let is_bare_string_var = match &expr.kind {
        IrExprKind::Var { .. } => matches!(expr.ty, Ty::String),
        IrExprKind::Block { stmts, expr: tail } if stmts.is_empty() => {
            if let Some(t) = tail {
                matches!(t.kind, IrExprKind::Var { .. }) && matches!(t.ty, Ty::String)
            } else {
                false
            }
        }
        _ => false,
    };
    if is_bare_string_var {
        format!("{}.to_string()", rendered)
    } else {
        rendered.to_string()
    }
}

// ── Extracted sub-functions (reduce render_expr complexity) ──

/// #617: does this type reach a `Bytes`/`Matrix` anywhere a runtime boundary
/// would hand back RAW values (`Vec<u8>` / `AlmideMatrix`) where the generated
/// code stores the RcCow value type?
pub(super) fn rc_cow_needs_glue(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    match ty {
        Ty::Bytes | Ty::Matrix => true,
        Ty::Applied(TC::Matrix, _) => true,
        Ty::Applied(TC::List | TC::Option, args) => args.first().is_some_and(rc_cow_needs_glue),
        Ty::Applied(TC::Result, args) => args.iter().any(rc_cow_needs_glue),
        Ty::Tuple(ts) => ts.iter().any(rc_cow_needs_glue),
        _ => false,
    }
}

/// #617: conversion glue from a RAW runtime result to the RcCow-typed value the
/// generated code stores — `Bytes`/`Matrix` map to `RcCow<…>` (rust.toml
/// `type_bytes`/`type_matrix`), while the runtime keeps its raw signatures.
/// Immutable/mutable ARG borrows need no glue (`&RcCow<T>` / `&mut RcCow<T>`
/// deref-coerce; the `&mut` path IS the `make_mut` copy-on-write that preserves
/// value semantics). Identity when no Bytes/Matrix is reachable in `ty`.
pub(super) fn rc_cow_result_glue(expr_str: String, ty: &Ty) -> String {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    if !rc_cow_needs_glue(ty) {
        return expr_str;
    }
    match ty {
        Ty::Bytes | Ty::Matrix | Ty::Applied(TC::Matrix, _) => {
            format!("RcCow::from({expr_str})")
        }
        Ty::Applied(TC::List, args) => {
            let inner = rc_cow_result_glue("__e".to_string(), &args[0]);
            format!("{expr_str}.into_iter().map(|__e| {inner}).collect::<Vec<_>>()")
        }
        Ty::Applied(TC::Option, args) => {
            let inner = rc_cow_result_glue("__e".to_string(), &args[0]);
            format!("{expr_str}.map(|__e| {inner})")
        }
        Ty::Applied(TC::Result, args) => {
            let mut out = expr_str;
            if rc_cow_needs_glue(&args[0]) {
                let ok_g = rc_cow_result_glue("__e".to_string(), &args[0]);
                out = format!("{out}.map(|__e| {ok_g})");
            }
            if args.len() > 1 && rc_cow_needs_glue(&args[1]) {
                let err_g = rc_cow_result_glue("__e".to_string(), &args[1]);
                out = format!("{out}.map_err(|__e| {err_g})");
            }
            out
        }
        Ty::Tuple(ts) => {
            let names: Vec<String> = (0..ts.len()).map(|i| format!("__t{i}")).collect();
            let parts: Vec<String> = ts
                .iter()
                .zip(&names)
                .map(|(t, n)| rc_cow_result_glue(n.clone(), t))
                .collect();
            format!("{{ let ({}) = {expr_str}; ({}) }}", names.join(", "), parts.join(", "))
        }
        _ => expr_str,
    }
}

/// #617: the reverse boundary — a runtime fn whose signature takes a CONCRETE
/// container of raw elements (`&[AlmideMatrix]`), where deref coercion cannot
/// see through the RcCow ELEMENTS. Element-cloned at the call site; the only
/// such runtime family today is `matrix.concat_cols_many`.
fn rc_cow_arg_needs_raw_elems(symbol: &str) -> bool {
    symbol == "almide_rt_matrix_concat_cols_many"
}

/// #617: the inverse of [`rc_cow_result_glue`] — convert an RcCow-shaped value
/// expression back to the RAW shape a shared STATIC stores (an `Rc` inside a
/// static is not `Sync`, and fan threads read globals, so top-lets keep the raw
/// `Vec<u8>` / `AlmideMatrix` layout; locals re-wrap at the read boundary).
pub(super) fn rc_cow_unglue(expr_str: String, ty: &Ty) -> String {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    if !rc_cow_needs_glue(ty) {
        return expr_str;
    }
    match ty {
        Ty::Bytes | Ty::Matrix | Ty::Applied(TC::Matrix, _) => {
            format!("RcCow::into_inner({expr_str})")
        }
        Ty::Applied(TC::List, args) => {
            let inner = rc_cow_unglue("__e".to_string(), &args[0]);
            format!("{expr_str}.into_iter().map(|__e| {inner}).collect::<Vec<_>>()")
        }
        Ty::Applied(TC::Option, args) => {
            let inner = rc_cow_unglue("__e".to_string(), &args[0]);
            format!("{expr_str}.map(|__e| {inner})")
        }
        Ty::Applied(TC::Result, args) => {
            let mut out = expr_str;
            if rc_cow_needs_glue(&args[0]) {
                let ok_g = rc_cow_unglue("__e".to_string(), &args[0]);
                out = format!("{out}.map(|__e| {ok_g})");
            }
            if args.len() > 1 && rc_cow_needs_glue(&args[1]) {
                let err_g = rc_cow_unglue("__e".to_string(), &args[1]);
                out = format!("{out}.map_err(|__e| {err_g})");
            }
            out
        }
        Ty::Tuple(ts) => {
            let names: Vec<String> = (0..ts.len()).map(|i| format!("__t{i}")).collect();
            let parts: Vec<String> = ts
                .iter()
                .zip(&names)
                .map(|(t, n)| rc_cow_unglue(n.clone(), t))
                .collect();
            format!("{{ let ({}) = {expr_str}; ({}) }}", names.join(", "), parts.join(", "))
        }
        _ => expr_str,
    }
}

/// #617: render a type for a shared STATIC — the RcCow value shape textually
/// reverted to the raw storage shape (RcCow only ever wraps these two types,
/// so the rewrite is total and unambiguous).
pub(super) fn rc_cow_raw_type(ty_str: &str) -> String {
    ty_str
        .replace("RcCow<Vec<u8>>", "Vec<u8>")
        .replace("RcCow<AlmideMatrix>", "AlmideMatrix")
}

/// #617: TRUE iff `symbol` names a NATIVE runtime fn (raw `Vec<u8>` /
/// `AlmideMatrix` signatures) rather than a USER-module fn that received the
/// same `almide_rt_<m>_` prefixing (whose generated signature already carries
/// the mapped RcCow types — gluing it would double-wrap, the nn E0283).
/// Decided against the runtime-module registry, the one source of truth for
/// what ships raw native signatures.
fn rc_cow_symbol_is_native_runtime(symbol: &str) -> bool {
    let Some(rest) = symbol.strip_prefix("almide_rt_") else {
        return false;
    };
    crate::generated::rust_runtime::RUST_RUNTIME_MODULES
        .iter()
        .any(|(m, _)| rest.strip_prefix(m).is_some_and(|r| r.starts_with('_')))
}

/// Inline numeric casts. Extracted from `render_runtime_call` (cog>30
/// decomposition): `Some` mirrors the original's early `return`, `None`
/// falls through to the next check.
fn try_render_numeric_cast(ctx: &RenderContext, symbol: &almide_base::intern::Sym, args: &[IrExpr]) -> Option<String> {
    match symbol.as_str() {
        "almide_rt_float_from_int" | "almide_rt_int_to_float" if args.len() == 1 => {
            Some(format!("({} as f64)", render_expr(ctx, &args[0])))
        }
        "almide_rt_float_to_int" if args.len() == 1 => {
            Some(format!("({} as i64)", render_expr(ctx, &args[0])))
        }
        _ => None,
    }
}

/// Mutating stdlib calls on a module-level (`ModuleRc`) or `RcCow` var:
/// route through `Rc::make_mut`/`.make_mut()` so the mutation hits the
/// shared backing store, not a clone. The mutator set is the one source of
/// truth in `pass_closure_conversion` (list/map/string/bytes &mut-on-args[0]
/// fns); before it was only list push/pop/clear, so `map.insert`/`bytes.push`/…
/// on a global silently mutated a discarded `(**c.borrow()).clone()`.
/// Extracted from `render_runtime_call`: `Some` mirrors the original's
/// early `return`, `None` falls through to the default owned-args render.
fn try_render_mutating_runtime_call(ctx: &RenderContext, symbol: &almide_base::intern::Sym, args: &[IrExpr]) -> Option<String> {
    if args.is_empty() { return None; }
    if !crate::pass_closure_conversion::is_inplace_mutator(symbol.as_str()) { return None; }
    let (IrExprKind::Borrow { expr: inner, .. } | IrExprKind::Clone { expr: inner }) = &args[0].kind else { return None; };
    let IrExprKind::Var { id } = &inner.kind else { return None; };
    let name = ctx.var_name(*id).to_string();
    // §4 Stage 2: a global target dispatches on the attribute.
    if let Some(info) = ctx.ann.global(*id) {
        use almide_ir::top_let_storage::TopLetStorage as Tls;
        if matches!(info.storage, Tls::RcRefCell) {
            let rest_args = args[1..].iter().map(|a| render_expr(ctx, a))
                .collect::<Vec<_>>().join(", ");
            let rc_mut = "std::rc::Rc::make_mut(&mut *c.borrow_mut())";
            let call_args = if rest_args.is_empty() {
                rc_mut.to_string()
            } else {
                format!("{}, {}", rc_mut, rest_args)
            };
            return Some(format!("{}.with(|c| {}({}))", info.static_name, symbol.as_str(), call_args));
        }
    }
    match ctx.ann.get_var_storage(id) {
        VarStorage::RcCow => {
            let rest_args = args[1..].iter().map(|a| render_expr(ctx, a))
                .collect::<Vec<_>>().join(", ");
            let call_args = if rest_args.is_empty() {
                format!("{}.make_mut()", name)
            } else {
                format!("{}.make_mut(), {}", name, rest_args)
            };
            Some(format!("{}({})", symbol.as_str(), call_args))
        }
        _ => None,
    }
}

/// Default: render all args as owned. Extracted from `render_runtime_call`.
fn render_runtime_call_args_owned(ctx: &RenderContext, symbol: &almide_base::intern::Sym, args: &[IrExpr]) -> String {
    args
        .iter()
        .map(|a| {
            let r = render_expr_owned(ctx, a);
            // #617: a concrete container-of-raw runtime param cannot deref-coerce
            // through RcCow ELEMENTS — clone them out at this (rare) boundary. The
            // closure param is EXPLICITLY typed: the producer side may itself be
            // result glue ending in `collect::<Vec<_>>()`, whose `_` only resolves
            // from this consumer.
            if rc_cow_arg_needs_raw_elems(symbol.as_str()) && rc_cow_needs_glue(&a.ty) {
                let elem = match &a.ty {
                    Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, es)
                        if es.len() == 1 =>
                    {
                        render_type(ctx, &es[0])
                    }
                    _ => "_".to_string(),
                };
                format!("{r}.iter().map(|__e: &{elem}| (**__e).clone()).collect::<Vec<_>>()")
            } else {
                r
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_runtime_call(ctx: &RenderContext, symbol: &almide_base::intern::Sym, args: &[IrExpr]) -> String {
    if let Some(rendered) = try_render_numeric_cast(ctx, symbol, args) {
        return rendered;
    }
    if let Some(rendered) = try_render_mutating_runtime_call(ctx, symbol, args) {
        return rendered;
    }
    let args_str = render_runtime_call_args_owned(ctx, symbol, args);
    format!("{}({})", symbol.as_str(), args_str)
}

/// A COMPOUND interpolation part — one whose value has no `Display` and must be
/// rendered via `AlmideRepr` to its Almide-literal form. Scalars (numbers,
/// `Bool`, `Unit`) and bare `String` keep their plain `{}` Display path so the
/// emitted code for existing programs is byte-identical.
///
/// Scope: the types backed by an `AlmideRepr` impl on BOTH targets, recursively
/// composed — the structural containers (`List`, `Map`, `Set`, `Option`,
/// `Result`, `Tuple`) plus user-defined records and variants. A record/variant
/// is identified by name via `ctx.repr_named_types` (the set of types that got a
/// generated `AlmideRepr` impl); an anonymous record (`Ty::Record`/`OpenRecord`)
/// is always repr-backed. Closure-bearing types are excluded from the set, so a
/// value with an `Fn` field stays on the Display path (it has no repr).
/// Render a key/value type for an empty-map turbofish, erasing unresolved
/// named typevars to `_` exactly like the empty-list `inner_type` path.
fn render_map_type_arg(ctx: &RenderContext, ty: &Ty) -> String {
    let ty = if ty_has_named_typevar(ty) { erase_named_typevars(ty.clone()) } else { ty.clone() };
    render_type(ctx, &ty)
}

fn ty_needs_repr(ctx: &RenderContext, ty: &Ty) -> bool {
    use TypeConstructorId::{List, Map, Set, Option as OptionId, Result as ResultId};
    match ty {
        // Backed container constructors → repr.
        Ty::Applied(id, _) => matches!(id, List | Map | Set | OptionId | ResultId),
        // Tuples → repr.
        Ty::Tuple(..) => true,
        // Anonymous records get an inline literal repr.
        Ty::Record { .. } | Ty::OpenRecord { .. } => true,
        // Named records/variants → repr only when a generated impl exists.
        Ty::Named(name, _) => ctx.repr_named_types.contains(name),
        // Everything else (scalars, String, Bool, Unit, Fn, Unknown, …) stays on
        // the Display path.
        _ => false,
    }
}

fn render_string_interp(ctx: &RenderContext, parts: &[IrStringPart]) -> String {
    let mut fmt_parts = Vec::new();
    let mut arg_parts = Vec::new();
    for part in parts {
        match part {
            IrStringPart::Lit { value } => {
                fmt_parts.push(value
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('\n', "\\n")
                    .replace('\t', "\\t")
                    .replace('\r', "\\r")
                    .replace('{', "{{")
                    .replace('}', "}}"));
            }
            IrStringPart::Expr { expr } => {
                fmt_parts.push("{}".to_string());
                // A COMPOUND part (List/Map/Set/Tuple/Option/Result/record/variant)
                // has no `Display`; route it through the `AlmideRepr` trait so it
                // renders to its Almide-literal form (`[1, 2, 3]`, `["a": 1]`, …).
                // Bare String/Int/Float/Bool keep the plain `{}` Display path so
                // existing programs' emitted code is byte-identical. `almide_repr`
                // takes `&T` and has ref-forwarding impls, so `&(...)` is correct
                // whether the part renders to an owned value or an existing borrow.
                if ty_needs_repr(ctx, &expr.ty) {
                    arg_parts.push(format!("almide_repr(&({}))", render_expr(ctx, expr)));
                } else {
                    arg_parts.push(render_expr(ctx, expr));
                }
            }
        }
    }
    let format_str_s = fmt_parts.join("");
    let args_s = arg_parts.join(", ");
    let template_str_s = {
        let mut s = String::new();
        for part in parts {
            match part {
                IrStringPart::Lit { value } => s.push_str(value),
                IrStringPart::Expr { expr } => {
                    s.push_str("${");
                    s.push_str(&render_expr(ctx, expr));
                    s.push('}');
                }
            }
        }
        s
    };
    ctx.templates.render_with("string_interp", None, &[], &[("format_str", format_str_s.as_str()), ("args", args_s.as_str()), ("template_str", template_str_s.as_str())])
        .unwrap_or_else(|| "\"...\"".into())
}

// ── render_expr arm extraction (cog>100 decomposition, pattern 2) ──
//
// The following helpers are 1:1 text-moves of the largest `render_expr`
// match arms. Each re-narrows `expr.kind` via `let-else` and returns the
// exact String the inline arm used to produce — no behavior change.

fn render_expr_var(ctx: &RenderContext, expr: &IrExpr) -> String {
    let IrExprKind::Var { id } = &expr.kind else { unreachable!() };
    let raw_name = ctx.var_name(*id).to_string();
    // Shared-mut local (`Rc<Cell<T>>`): read the cell. (Closure v2, P3.)
    if ctx.ann.is_shared_mut(id) {
        return format!("{}.get()", raw_name);
    }
    // §4 Stage 2: a module global is decided by ONE alias-resolved
    // lookup — storage class AND emitted name come from the same
    // GlobalInfo the agreement verifier checks. This replaces the
    // former five-predicate chain (storage-by-name, lazy_vars,
    // lazy_top_let_names, const_top_let_vars, module_origin
    // prefixing), whose per-use re-derivation was the #486/#500
    // drift surface. It also removes the lazy_vars mid-emission
    // ordering hazard: a Lazy global ALWAYS derefs, regardless of
    // declaration/use render order.
    if let Some(info) = ctx.ann.global(*id) {
        use almide_ir::top_let_storage::TopLetStorage as Tls;
        let read = match info.storage {
            Tls::Cell => format!("{}.with(|c| c.get())", info.static_name),
            Tls::RcRefCell => format!("{}.with(|c| (**c.borrow()).clone())", info.static_name),
            Tls::Lazy { .. } => ctx.templates
                .render_with("deref_lazy", None, &[], &[("name", info.static_name.as_str())])
                .unwrap_or_else(|| info.static_name.clone()),
            Tls::Const => info.static_name.clone(),
        };
        // #617: a STATIC stores the RAW Bytes/Matrix shape (an `Rc` inside a
        // shared static is not Sync, and fan threads read globals). A BARE
        // read stays raw — borrow positions coerce against the raw runtime
        // signatures directly; OWNING positions go through `Clone{Var}` nodes
        // (CloneInsertion), whose render re-wraps into the RcCow shape.
        return read;
    }
    raw_name
}

fn render_expr_if(ctx: &RenderContext, expr: &IrExpr) -> String {
    let IrExprKind::If { cond, then, else_ } = &expr.kind else { unreachable!() };
    let cond_s = render_expr(ctx, cond);
    let then_content = render_body_content(ctx, then);
    let else_content = render_body_content(ctx, else_);

    // Coerce bare Var branches to `String` when the If's type is
    // String. Function params of type String are emitted as `&str`
    // in Rust, so `if ... { "lit".to_string() } else { param }` would
    // mix `String` and `&str`. `.to_string()` unifies both.
    let (then_content, else_content) = if matches!(ctx.target, super::super::pass::Target::Rust)
        && matches!(expr.ty, Ty::String) {
        (
            coerce_to_owned_string(&then_content, then),
            coerce_to_owned_string(&else_content, else_),
        )
    } else {
        (then_content, else_content)
    };

    if then_content.contains('\n') || else_content.contains('\n') {
        // Multi-line: indent branch bodies
        let indented_then = indent_lines(&then_content, 4);
        let indented_else = indent_lines(&else_content, 4);
        format!("if {} {{\n{}\n}} else {{\n{}\n}}", cond_s, indented_then, indented_else)
    } else {
        ctx.templates.render_with("if_expr", None, &[], &[("cond", cond_s.as_str()), ("then", then_content.as_str()), ("else", else_content.as_str())])
            .unwrap_or_else(|| format!("if {} {{ {} }} else {{ {} }}", cond_s, then_content, else_content))
    }
}

fn render_expr_block(ctx: &RenderContext, expr: &IrExpr) -> String {
    let IrExprKind::Block { stmts, expr: tail } = &expr.kind else { unreachable!() };
    let mut parts: Vec<String> = render_stmts(ctx, stmts).into_iter()
        .map(|s| terminate_stmt(ctx, s))
        .collect();
    if let Some(e) = tail {
        let expr_str = render_expr(ctx, e);
        // break/continue are statements — don't wrap in return
        let is_control_flow = matches!(&e.kind, IrExprKind::Break | IrExprKind::Continue);
        if is_control_flow {
            parts.push(expr_str);
        } else {
            parts.push(ctx.templates.render_with("block_result_expr", None, &[], &[("expr", expr_str.as_str())])
                .unwrap_or_else(|| expr_str.clone()));
        }
    }
    let body = parts.join("\n");
    let indented_body = indent_lines(&body, 4);
    // If block contains break/continue, don't wrap in IIFE — use bare block
    let has_control = stmts.iter().any(|s| match &s.kind {
        IrStmtKind::Expr { expr } => contains_loop_control(expr),
        IrStmtKind::Bind { value, .. } => contains_loop_control(value),
        _ => false,
    }) || tail.as_ref().map_or(false, |e| contains_loop_control(e));
    if has_control {
        format!("{{\n{}\n}}", indented_body)
    } else {
        ctx.templates.render_with("block_expr", None, &[], &[("body", indented_body.as_str())])
            .unwrap_or_else(|| format!("{{\n{}\n}}", indented_body))
    }
}

fn render_expr_for_in(ctx: &RenderContext, expr: &IrExpr) -> String {
    let IrExprKind::ForIn { var, var_tuple, iterable, body } = &expr.kind else { unreachable!() };
    // Optimize: for loop over empty list literal → skip entirely
    if let IrExprKind::List { elements } = &iterable.kind {
        if elements.is_empty() {
            return "{}".to_string();
        }
    }
    let var_name = if let Some(tuple_vars) = var_tuple {
        let names: Vec<String> = tuple_vars.iter().map(|id| ctx.var_name(*id).to_string()).collect();
        let vars_s = names.join(", ");
        ctx.templates.render_with("for_tuple_destructure", None, &[], &[("vars", vars_s.as_str())])
            .unwrap_or_else(|| format!("({})", names.join(", ")))
    } else {
        ctx.var_name(*var).to_string()
    };
    // Rust's `for i in 0..n` consumes a Range directly; the default
    // `range_expr` template wraps ranges in `.collect::<Vec<_>>()`
    // so they're usable as Vec values elsewhere, but that allocates
    // a Vec every time a plain range appears as a ForIn iterable.
    // A 2 M-weight inner loop inside a 16 k outer loop was paying
    // ~16 MB/tensor of throwaway Vec allocations. Render Ranges
    // that appear as a loop iterable with the bare `start..end`
    // form to skip the alloc.
    let iter = match &iterable.kind {
        IrExprKind::Range { start, end, inclusive } => {
            let s = render_expr(ctx, start);
            let e = render_expr(ctx, end);
            let op = if *inclusive { "..=" } else { ".." };
            format!("{}{}{}", s, op, e)
        }
        _ => {
            let base = render_expr(ctx, iterable);
            // List types: .iter().cloned() works for both RcCow<Vec<T>>
            // (via Deref) and plain Vec<T>, giving owned T values.
            match &iterable.ty {
                Ty::Applied(TypeConstructorId::List, _) => format!("{}.iter().cloned()", base),
                _ => base,
            }
        }
    };
    let body_raw = render_stmts(ctx, body).join("\n");
    let body_str = indent_lines(&body_raw, 4);
    ctx.templates.render_with("for_loop", None, &[], &[("var", var_name.as_str()), ("iter", iter.as_str()), ("body", body_str.as_str())])
        .unwrap_or_else(|| format!("for _ in _ {{ }}"))
}

/// Rewrite legacy AlmdRec{N} references to field-name-based names.
/// @inline_rust templates in dependency packages may contain hardcoded
/// AlmdRec0, AlmdRec1 etc. that no longer match the current naming.
/// Extract all AlmdRec{digits} tokens, then resolve each by matching
/// the constructor's field names against the anon_records map. Extracted
/// from `render_expr_inline_rust` (cog>30 decomposition): a `&mut String`
/// output param that's only written to, never read back to change its own
/// branching — safe to thread out.
fn rewrite_legacy_almd_rec_names(ctx: &RenderContext, out: &mut String) {
    if !out.contains("AlmdRec") { return; }
    let mut legacy_names: Vec<String> = Vec::new();
    let bytes = out.as_bytes();
    let prefix = b"AlmdRec";
    let mut pos = 0;
    while pos + prefix.len() < bytes.len() {
        if bytes[pos..].starts_with(prefix) {
            let start = pos;
            pos += prefix.len();
            // Consume trailing digits
            let digit_start = pos;
            while pos < bytes.len() && bytes[pos].is_ascii_digit() { pos += 1; }
            if pos > digit_start {
                let name = std::str::from_utf8(&bytes[start..pos]).unwrap().to_string();
                // Skip names that are already field-based (contain '_' after "AlmdRec")
                if !name.contains('_') {
                    legacy_names.push(name);
                }
            }
        } else {
            pos += 1;
        }
    }
    legacy_names.sort();
    legacy_names.dedup();
    for legacy in &legacy_names {
        let matched = ctx.ann.anon_records.iter().find(|(field_names, _)| {
            field_names.iter().all(|f| {
                let pattern = format!("{} {{ {}: ", legacy, f);
                let pattern2 = format!(", {}: ", f);
                out.contains(&pattern) || out.contains(&pattern2)
            })
        });
        if let Some((_, struct_name)) = matched {
            *out = out.replace(legacy, struct_name);
        }
    }
}

fn render_expr_inline_rust(ctx: &RenderContext, expr: &IrExpr) -> String {
    let IrExprKind::InlineRust { template, args } = &expr.kind else { unreachable!() };
    let mut out = template.clone();
    for (name, arg) in args {
        let rendered = render_expr(ctx, arg);
        let placeholder = format!("{{{}}}", name.as_str());
        out = out.replace(&placeholder, &rendered);
    }
    rewrite_legacy_almd_rec_names(ctx, &mut out);
    out
}

fn render_expr_call(ctx: &RenderContext, expr: &IrExpr) -> String {
    let (IrExprKind::Call { target, args, .. } | IrExprKind::TailCall { target, args }) = &expr.kind else { unreachable!() };
    match target {
        CallTarget::Module { module, func, .. } => {
            // Module calls: use template (TS/JS) or runtime function (Rust)
            let args_str = args.iter().map(|a| render_expr_owned(ctx, a)).collect::<Vec<_>>().join(", ");
            let mod_ident = module.replace('.', "_");
            let func_ident = func.replace('.', "_");
            let call = ctx.templates.render_with("module_call", None, &[], &[("module", mod_ident.as_str()), ("func", func_ident.as_str()), ("args", args_str.as_str())])
                .unwrap_or_else(|| {
                    format!("almide_rt_{}_{}({})", mod_ident, func_ident, args_str)
                });
            // #617: same raw-runtime-result → RcCow boundary as RuntimeCall —
            // but ONLY for STDLIB modules (native runtime signatures). A USER
            // module fn's generated signature already carries the mapped RcCow
            // types, so gluing it would double-wrap (E0283 in the nn repo).
            if almide_lang::stdlib_info::is_stdlib_module(module.as_str()) {
                rc_cow_result_glue(call, &expr.ty)
            } else {
                call
            }
        }
        _ => render_generic_call(ctx, target, args, &expr.ty)
    }
}

fn render_expr_list(ctx: &RenderContext, expr: &IrExpr) -> String {
    let IrExprKind::List { elements } = &expr.kind else { unreachable!() };
    // Empty list: use typed template (Rust needs Vec::<T>::new(), TS uses [])
    if elements.is_empty() {
        let inner_ty = match &expr.ty {
            Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => {
                let inner = &args[0];
                let ty = if ty_has_named_typevar(inner) {
                    erase_named_typevars(inner.clone())
                } else {
                    inner.clone()
                };
                // A `List[Fn]` empty literal must not render
                // `Vec::<impl Fn>::new()` (E0562: impl Trait in path).
                // Closures stored in a list are boxed to `Rc<dyn Fn>`
                // (see the closure-boxing pass), so the turbofish element
                // type must match.
                if matches!(&ty, Ty::Fn { .. }) {
                    super::helpers::render_type_field_fn(ctx, &ty)
                } else {
                    render_type(ctx, &ty)
                }
            }
            _ => "_".into(),
        };
        if let Some(rendered) = ctx.templates.render_with("empty_list", None, &[], &[("inner_type", inner_ty.as_str())]) {
            return rendered;
        }
    }
    let elems = elements.iter().map(|e| render_expr_owned(ctx, e)).collect::<Vec<_>>().join(", ");
    ctx.templates.render_with("list_literal", None, &[], &[("elements", elems.as_str())])
        .unwrap_or_else(|| format!("[...]"))
}

fn render_expr_record(ctx: &RenderContext, expr: &IrExpr) -> String {
    let IrExprKind::Record { name, fields } = &expr.kind else { unreachable!() };
    // Build field strings (explicit + defaults for missing)
    let ctor_name_str = name.as_ref().map(|s| s.as_str()).unwrap_or("");
    let explicit_names: std::collections::HashSet<&str> = fields.iter().map(|(k, _)| &**k).collect();
    let mut field_strs: Vec<String> = Vec::new();
    // Render explicit fields (owned: RcCow vars unwrapped to T)
    for (k, v) in fields.iter() {
        let mut val_str = render_expr_owned(ctx, v);
        // Box recursive fields (annotation is target-aware — empty for non-Rust)
        if let Some(cn) = name {
            if ctx.ann.boxed_fields.contains(&(cn.to_string(), k.to_string())) {
                val_str = format!("std::boxed::Box::new({})", val_str);
            }
        }
        // A closure stored in a struct field is `Rc<dyn Fn>`; the
        // box-by-default pass already boxed the field value, so no
        // field-side `Rc::new` — that double-boxed it (this site had no
        // `RcWrap` guard at all).
        field_strs.push(ctx.templates.render_with("record_field", None, &[], &[("name", k.as_str()), ("value", val_str.as_str())])
            .unwrap_or_else(|| format!("{}: {}", k, val_str)));
    }
    // Fill in default fields that were not explicitly provided.
    // default_fields is keyed by both bare name ("Msg") and module-qualified
    // name ("dep_pkg.Msg"), so we try the exact ctor_name_str first.
    let mut default_keys: Vec<(String, String)> = ctx.ann.default_fields.keys()
        .filter(|(cn, _)| cn == ctor_name_str)
        .cloned()
        .collect();
    if default_keys.is_empty() && std::env::var("ALMIDE_DEFAULTS_DEBUG").is_ok() {
        let all: Vec<&String> = ctx.ann.default_fields.keys().map(|(c, _)| c).collect();
        eprintln!("[defaults-miss] ctor={:?} known={:?}", ctor_name_str, all);
    }
    // `default_fields` is a HashMap, so `.keys()` iteration order is
    // per-process (RandomState). Sort the default fields we append so
    // the emitted struct literal is host-deterministic. Explicit fields
    // are already in IR order; defaults follow in (ctor, field) order.
    // Determinism Belt — Rust-target emit.
    default_keys.sort();
    for (_, field_name) in &default_keys {
        if explicit_names.contains(field_name.as_str()) { continue; }
        let Some(default_expr) = ctx.ann.default_fields.get(&(ctor_name_str.to_string(), field_name.clone())) else { continue; };
        let mut val_str = render_expr(ctx, default_expr);
        let needs_box = name.as_ref()
            .map_or(false, |cn| ctx.ann.boxed_fields.contains(&(cn.to_string(), field_name.clone())));
        if needs_box { val_str = format!("std::boxed::Box::new({})", val_str); }
        field_strs.push(ctx.templates.render_with("record_field", None, &[], &[("name", field_name.as_str()), ("value", val_str.as_str())])
            .unwrap_or_else(|| format!("{}: {}", field_name, val_str)));
    }
    let fields_str = field_strs.join(", ");
    // Resolve type name: explicit name, or from expr.ty
    // For record literals, use bare struct name (no generics — Rust infers them)
    // Strip module qualifier from names like "module.TypeName" → "TypeName"
    // since all modules are flattened into one file in generated Rust.
    let mut type_name = name.map(|n| {
        let s = n.as_str();
        s.rsplit('.').next().unwrap_or(s).to_string()
    }).unwrap_or_else(|| {
        match &expr.ty {
            Ty::Named(n, _) => n.to_string(),
            Ty::Record { fields: ty_fields } | Ty::OpenRecord { fields: ty_fields } => {
                let mut names: Vec<String> = ty_fields.iter().map(|(n, _)| n.to_string()).collect();
                names.sort();
                if let Some(n) = ctx.ann.named_records.get(&names) {
                    n.clone()
                } else if let Some(n) = ctx.ann.anon_records.get(&names) {
                    n.clone() // bare name, no generics
                } else {
                    names.join("_")
                }
            }
            _ => render_type(ctx, &expr.ty),
        }
    });
    // Qualify enum variant constructors via template
    if let Some(enum_name) = ctx.ann.ctor_to_enum.get(&type_name) {
        // Try ctor_record template first (TS: function call), fallback to record_literal
        if let Some(rendered) = ctx.templates.render_with("ctor_record", None, &[], &[("enum_name", enum_name.as_str()), ("ctor_name", type_name.as_str()), ("fields", fields_str.as_str())]) {
            return rendered;
        }
        type_name = ctx.templates.render_with("ctor_qualify", None, &[], &[("enum_name", enum_name.as_str()), ("ctor_name", type_name.as_str()), ("fields", fields_str.as_str())])
            .unwrap_or_else(|| format!("{}::{}", enum_name, type_name));
    }
    let fallback = format!("{{ {} }}", &fields_str);
    ctx.templates.render_with("record_literal", None, &[], &[("type_name", type_name.as_str()), ("fields", fields_str.as_str())])
        .unwrap_or(fallback)
}

fn render_expr_spread_record(ctx: &RenderContext, expr: &IrExpr) -> String {
    let IrExprKind::SpreadRecord { base, fields } = &expr.kind else { unreachable!() };
    let mut base_str = render_expr(ctx, base);
    // Spread requires an owned value. If base is a LazyLock deref
    // (module top_let), clone to get owned.
    if let IrExprKind::Var { id } = &base.kind {
        // §4 Stage 2: a Lazy global derefs a LazyLock and every
        // cross-module use renders through an accessor — both need
        // an owned clone for the spread. Attribute equivalent of the
        // former (module_origin || lazy_vars) probe.
        use almide_ir::top_let_storage::TopLetStorage as Tls;
        let needs_clone = ctx.ann.global_alias.contains_key(id)
            || matches!(ctx.ann.global(*id).map(|i| i.storage), Some(Tls::Lazy { .. }));
        if needs_clone && !base_str.ends_with(".clone()") {
            base_str = format!("{}.clone()", base_str);
        }
    }
    let fields_str = fields.iter()
        .map(|(k, v)| format!("{}: {}", k, render_expr(ctx, v)))
        .collect::<Vec<_>>()
        .join(", ");
    let type_name = match &expr.ty {
        Ty::Named(n, _) => n.to_string(),
        Ty::Record { fields: ty_fields } | Ty::OpenRecord { fields: ty_fields } => {
            let mut names: Vec<String> = ty_fields.iter().map(|(n, _)| n.to_string()).collect();
            names.sort();
            ctx.ann.named_records.get(&names).cloned()
                .or_else(|| ctx.ann.anon_records.get(&names).cloned())
                .unwrap_or_else(|| names.join("_"))
        }
        _ => render_type(ctx, &expr.ty),
    };
    ctx.templates.render_with("spread_record", None, &[], &[("type_name", type_name.as_str()), ("fields", fields_str.as_str()), ("base", base_str.as_str())])
        .unwrap_or_else(|| "{ ...spread }".into())
}

/// `render_expr_unwrap`'s error-coercion-template-attr computation,
/// extracted verbatim (cog>30 decomposition) — `Option<&'static str>`
/// return, no state, safe to hoist out of the surrounding function.
/// For Result with a non-String error, choose the `map_err` variant the
/// template needs (the template decides whether/how to coerce —
/// target-agnostic). `None` for `Option` unwraps (no error to coerce).
fn unwrap_err_coerce_attr(ctx: &RenderContext, inner_ty: &Ty) -> Option<&'static str> {
    if inner_ty.is_option() { return None; }
    let (_, err_ty) = inner_ty.inner2()?;
    // When the source error type already matches the enclosing fn's
    // propagated error type, `?` carries it through unchanged — no
    // coercion. This preserves a custom variant error (e.g.
    // `Result[_, AppErr]` !-ed inside a fn that also returns
    // `Result[_, AppErr]`) instead of stringifying it via Debug (which
    // would need `From<String>` and fail to compile). #630
    let matches_fn_err = ctx.fn_err_ty.as_ref() == Some(err_ty);
    if matches_fn_err {
        None
    } else if matches!(err_ty, Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 && matches!(args[0], Ty::String)) {
        Some("map_err_join")
    } else if !matches!(err_ty, Ty::String) {
        Some("map_err_debug")
    } else {
        None
    }
}

fn render_expr_unwrap(ctx: &RenderContext, expr: &IrExpr) -> String {
    let IrExprKind::Unwrap { expr: inner } = &expr.kind else { unreachable!() };
    // Short-circuit: ok(x)! = x, some(x)! = x — the unwrap is a no-op.
    if matches!(&inner.kind, IrExprKind::ResultOk { .. } | IrExprKind::OptionSome { .. }) {
        let inner_expr = match &inner.kind {
            IrExprKind::ResultOk { expr } | IrExprKind::OptionSome { expr } => expr,
            _ => unreachable!(),
        };
        return render_expr(ctx, inner_expr);
    }
    let s = render_expr(ctx, inner);
    // In test functions, ? cannot be used (return type is ()).
    // Use .unwrap() instead.
    if ctx.is_test {
        format!("({}).unwrap()", s)
    } else {
        // Determine the right template variant based on inner type.
        // For Result with non-String error, use map_err variants
        // (the template decides whether/how to coerce — target-agnostic).
        let when_type = if inner.ty.is_option() { Some("Option") } else { None };
        let err_coerce_attr = unwrap_err_coerce_attr(ctx, &inner.ty);
        let attrs: Vec<&str> = err_coerce_attr.into_iter().collect();
        ctx.templates.render_with("unwrap_expr", when_type, &attrs, &[("inner", s.as_str())])
            .unwrap_or_else(|| format!("({})?", s))
    }
}

fn render_expr_clone(ctx: &RenderContext, expr: &IrExpr) -> String {
    let IrExprKind::Clone { expr: inner } = &expr.kind else { unreachable!() };
    // Val-wrapped var: deref then clone to get T. Bind handler re-wraps in RcCow::new().
    if let IrExprKind::Var { id } = &inner.kind {
        if ctx.ann.is_rc_cow(id) {
            let var_name = ctx.var_name(*id).to_string();
            return format!("(*{}).clone()", var_name);
        }
        // #617: cloning a GLOBAL out of its raw static (Bytes/Matrix shapes)
        // produces the raw value — re-wrap into the RcCow value shape the
        // surrounding code stores. Locals are already RcCow (their clone is
        // the O(1) Rc bump) — no glue.
        if ctx.ann.global(*id).is_some() && rc_cow_needs_glue(&inner.ty) {
            let read = render_expr(ctx, inner);
            return rc_cow_result_glue(format!("{read}.clone()"), &inner.ty);
        }
    }
    let expr_s = render_expr(ctx, inner);
    // Special case: cloning a String-typed Var that's a fn param
    // (so it's actually emitted as `&str` in Rust). `.clone()` on
    // `&str` returns `&str`, not `String`. Use `.to_string()` so
    // the surrounding context (which expects an owned `String`)
    // type-checks.
    let is_borrowed_string_param = matches!(ctx.target, super::super::pass::Target::Rust)
        && matches!(inner.ty, Ty::String)
        && match &inner.kind {
            IrExprKind::Var { id } => ctx.ref_params.contains(id),
            _ => false,
        };
    if is_borrowed_string_param {
        return format!("{}.to_string()", expr_s);
    }
    ctx.templates.render_with("clone_expr", None, &[], &[("expr", expr_s.as_str())])
        .unwrap_or_else(|| format!("{}.clone()", expr_s))
}

/// Shared-mut non-Copy var (`SharedMut`, Closure v2 P6): borrow through the
/// `RefCell` rather than the `.get()` clone a bare Var read would emit, so a
/// mutating call (`list.push(acc, …)` → `&mut *acc.borrow_mut()`) writes the
/// ONE shared cell the closure also holds. A shared read uses `&*acc.borrow()`
/// (no clone). Copy shared-mut vars stay on the `Cell` `.get()` path below.
/// Extracted from `render_expr_borrow` (cog>30 decomposition): `Some`
/// mirrors the original's early `return`, `None` falls through.
fn try_render_borrow_shared_mut(ctx: &RenderContext, inner: &IrExpr, mutable: bool) -> Option<String> {
    let IrExprKind::Var { id } = &inner.kind else { return None; };
    if !ctx.ann.is_shared_mut(id) { return None; }
    if almide_ir::top_let_storage::capture_copy_cell(&ctx.var_table.get(*id).ty) { return None; }
    let var_name = ctx.var_name(*id).to_string();
    Some(if mutable {
        // In-place mutation writes the one shared cell.
        format!("&mut *{}.borrow_mut()", var_name)
    } else {
        // A shared read borrows an owned snapshot (`.get()` clones the
        // cell's value). Unlike `&*x.borrow()`, this owned temporary has
        // no lifetime tie to `x`, so it is also safe in tail position
        // where `x` is a block-local (`let outer = () => { var a = …; …; a })`.
        format!("&{}.get()", var_name)
    })
}

/// If the borrowed operand is a Var referencing a fn param already emitted
/// as a reference (`&T`, `&[T]`, `&str` for a shared borrow; `&mut T` for a
/// mutable one), skip the outer `&`/`&mut` — Rust auto-reborrows when you
/// pass the naked var, so this avoids a `&&T`/`&mut &mut T` double-borrow.
/// Extracted from `render_expr_borrow`.
fn try_render_borrow_already_ref_param(ctx: &RenderContext, inner: &IrExpr, as_str: bool, mutable: bool) -> Option<String> {
    let IrExprKind::Var { id } = &inner.kind else { return None; };
    if !as_str && !mutable && ctx.ref_params.contains(id) {
        return Some(render_expr(ctx, inner));
    }
    if mutable && ctx.ref_mut_params.contains(id) {
        return Some(render_expr(ctx, inner));
    }
    None
}

fn render_expr_borrow(ctx: &RenderContext, expr: &IrExpr) -> String {
    let IrExprKind::Borrow { expr: inner, as_str, mutable } = &expr.kind else { unreachable!() };
    if let Some(rendered) = try_render_borrow_shared_mut(ctx, inner, *mutable) {
        return rendered;
    }
    if let Some(rendered) = try_render_borrow_already_ref_param(ctx, inner, *as_str, *mutable) {
        return rendered;
    }
    if *mutable {
        // Val-wrapped var: .make_mut() for COW semantics
        if let IrExprKind::Var { id } = &inner.kind {
            if ctx.ann.is_rc_cow(id) {
                let var_name = ctx.var_name(*id).to_string();
                return format!("{}.make_mut()", var_name);
            }
        }
        format!("&mut {}", render_expr(ctx, inner))
    } else if *as_str {
        // String literal → bare &str in Rust, skip .to_string() allocation
        if let IrExprKind::LitStr { value } = &inner.kind {
            let escaped = value.replace('\\', "\\\\").replace('"', "\\\"")
                .replace('\n', "\\n").replace('\t', "\\t").replace('\r', "\\r");
            return format!("\"{}\"", escaped);
        }
        format!("&*{}", render_expr(ctx, inner))
    } else {
        format!("&{}", render_expr(ctx, inner))
    }
}

fn render_fan(ctx: &RenderContext, exprs: &[IrExpr]) -> String {
    let rendered: Vec<String> = exprs.iter().map(|e| {
        let mut body = render_expr(ctx, e);
        if e.ty.is_result() && body.ends_with('?') { body.pop(); }
        body
    }).collect();
    let exprs_s = rendered.join(", ");
    let count_s = format!("{}", exprs.len());
    let handles: Vec<String> = (0..exprs.len()).map(|i| format!("__fan_h{}", i)).collect();
    let spawns: Vec<String> = rendered.iter().enumerate()
        .map(|(i, body)| format!("let {} = __s.spawn(move || {{ {} }});", handles[i], body))
        .collect();
    let any_result = exprs.iter().any(|e| e.ty.is_result());
    let joins: Vec<String> = exprs.iter().enumerate().map(|(i, e)| {
        if e.ty.is_result() {
            if ctx.auto_unwrap { format!("{}.join().unwrap()?", handles[i]) }
            else { format!("{}.join().unwrap().unwrap()", handles[i]) }
        } else { format!("{}.join().unwrap()", handles[i]) }
    }).collect();
    let join_expr = if joins.len() == 1 { joins[0].clone() }
        else { format!("({})", joins.join(", ")) };
    let spawns_s = spawns.join(" ");
    let construct = if any_result && ctx.auto_unwrap { "fan_effect" } else { "fan_expr" };
    ctx.templates.render_with(construct, None, &[], &[("exprs", exprs_s.as_str()), ("count", count_s.as_str()), ("spawns", spawns_s.as_str()), ("join_expr", join_expr.as_str())])
        .unwrap_or_else(|| format!("fan({})", rendered.join(", ")))
}
