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

include!("expressions_p3.rs");
