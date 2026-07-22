//! Expression rendering: converts IrExpr nodes to target-specific code strings.

use almide_ir::*;
use almide_ir::annotations::VarStorage;
use almide_lang::types::{Ty, TypeConstructorId};
use super::RenderContext;
use super::types::render_type;
use super::statements::{render_stmt, render_match_arm};
use super::helpers::{template_or, terminate_stmt, indent_lines, render_body_content, contains_loop_control, ty_has_named_typevar, erase_named_typevars, ty_contains_name};

/// Render a statement list. Peephole patterns are detected at IR level
/// by PeepholePass; this just renders the resulting IR nodes.
fn render_stmts(ctx: &RenderContext, stmts: &[IrStmt]) -> Vec<String> {
    stmts.iter().map(|s| render_stmt(ctx, s)).collect()
}

/// Mangle a type into the monomorphization suffix form (mirrors mono/utils.rs).
fn mangle_ty_for_mono(ty: &Ty) -> String {
    match ty {
        Ty::Int => "Int".into(),
        Ty::Float => "Float".into(),
        Ty::String => "String".into(),
        Ty::Bool => "Bool".into(),
        Ty::Int8 => "Int8".into(),
        Ty::Int16 => "Int16".into(),
        Ty::Int32 => "Int32".into(),
        Ty::UInt8 => "UInt8".into(),
        Ty::UInt16 => "UInt16".into(),
        Ty::UInt32 => "UInt32".into(),
        Ty::UInt64 => "UInt64".into(),
        Ty::Float32 => "Float32".into(),
        Ty::Bytes => "Bytes".into(),
        Ty::Unit => "Unit".into(),
        Ty::Named(name, args) => {
            if args.is_empty() { name.to_string() }
            else { format!("{}_{}", name, args.iter().map(mangle_ty_for_mono).collect::<Vec<_>>().join("_")) }
        }
        Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 =>
            format!("List_{}", mangle_ty_for_mono(&args[0])),
        Ty::Applied(id, args) => {
            let name = format!("{:?}", id);
            if args.is_empty() { name } else {
                format!("{}_{}", name, args.iter().map(mangle_ty_for_mono).collect::<Vec<_>>().join("_"))
            }
        }
        _ => "Unknown".into(),
    }
}

/// Render an expression ensuring an owned value (not RcCow wrapper).
/// For RcCow vars, produces `(*var).clone()` to yield the unwrapped T.
/// Used at sites that need owned T: function args, record fields, concat operands.
pub(crate) fn render_expr_owned(ctx: &RenderContext, expr: &IrExpr) -> String {
    if let IrExprKind::Var { id } = &expr.kind {
        if ctx.ann.is_rc_cow(id) {
            return format!("(*{}).clone()", ctx.var_name(*id));
        }
    }
    render_expr(ctx, expr)
}

/// Render a closure literal (`move |params| body`). `annotate` adds explicit
/// param types: a BOXED closure (`Rc::new(move |k| …) as Rc<dyn Fn(String)>`)
/// needs them because the `as` cast does NOT back-infer the closure's param type
/// (`|k|` alone is E0282). A bare combinator-consumed lambda passes
/// `annotate = false` — a fused iterator adapter infers `&T` and an explicit `T`
/// would mismatch.
fn render_lambda(ctx: &RenderContext, params: &[(VarId, Ty)], body: &IrExpr, annotate: bool) -> String {
    let params_str = params.iter()
        .map(|(id, ty)| {
            let name = ctx.var_name(*id).to_string();
            if annotate && !ty.has_unresolved_deep() {
                format!("{}: {}", name, render_type(ctx, ty))
            } else {
                name
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let mut body_str = if let IrExprKind::Var { id } = &body.kind {
        if ctx.ann.is_rc_cow(id) {
            format!("(*{}).clone()", ctx.var_name(*id))
        } else { render_expr(ctx, body) }
    } else { render_expr(ctx, body) };
    // Nested (curried) lambda: box the returned closure as `Rc<dyn Fn>` with an
    // explicit cast so `Rc<concrete-closure>` coerces to the trait object the
    // outer closure's return type demands (E0271 without the cast).
    if matches!(&body.kind, IrExprKind::Lambda { .. }) {
        let wrapped = ctx.templates.render_with("box_wrap", None, &[], &[("inner", body_str.as_str())])
            .unwrap_or_else(|| format!("std::rc::Rc::new({})", body_str));
        let cast = super::helpers::render_type_rc_fn(ctx, &body.ty);
        body_str = format!("{} as {}", wrapped, cast);
    }
    ctx.templates.render_with("lambda_single", None, &[], &[("params", params_str.as_str()), ("body", body_str.as_str())])
        .unwrap_or_else(|| "|_| { }".to_string())
}

/// `IrExprKind::LitInt` case of `render_expr`, extracted verbatim (cog>30
/// decomposition, second round on top of round 1's extraction). Pick the
/// Rust literal suffix from `expr.ty` so sized numeric types (Stage 1a/1b)
/// emit the right width: `Ty::Int32` → `i32`, `Ty::UInt8` → `u8`, and the
/// canonical `Ty::Int` keeps the legacy `i64`. Falls through to the
/// `int_literal` template for backward compatibility when ty is
/// Int / Unknown.
fn render_expr_lit_int(ctx: &RenderContext, expr: &IrExpr, value: i64) -> String {
    let value_s = value.to_string();
    match &expr.ty {
        Ty::Int8 => format!("{}i8", value_s),
        Ty::Int16 => format!("{}i16", value_s),
        Ty::Int32 => format!("{}i32", value_s),
        Ty::UInt8 => format!("{}u8", value_s),
        Ty::UInt16 => format!("{}u16", value_s),
        Ty::UInt32 => format!("{}u32", value_s),
        Ty::UInt64 => format!("{}u64", value_s),
        _ => ctx.templates.render_with("int_literal", None, &[], &[("value", value_s.as_str())])
            .unwrap_or_else(|| value.to_string()),
    }
}

/// `IrExprKind::LitFloat` case of `render_expr`, extracted verbatim.
fn render_expr_lit_float(ctx: &RenderContext, expr: &IrExpr, value: f64) -> String {
    // Non-finite floats (a const-fold can produce inf/NaN, e.g.
    // `1e300 * 1e300`) have no Rust literal form: `format!("{}", inf)`
    // is the bare identifier `inf`, which the `{value}f64` template
    // turns into the undefined `inff64` (E0425). Emit the associated
    // constant directly — it needs no numeric suffix and is valid for
    // both f64 and f32.
    if !value.is_finite() {
        let f32_suffix = matches!(expr.ty, Ty::Float32);
        let assoc = if f32_suffix { "f32" } else { "f64" };
        return if value.is_nan() {
            format!("{}::NAN", assoc)
        } else if value > 0.0 {
            format!("{}::INFINITY", assoc)
        } else {
            format!("{}::NEG_INFINITY", assoc)
        };
    }
    let value_s = format!("{}", value);
    if matches!(expr.ty, Ty::Float32) {
        format!("{}f32", value_s)
    } else {
        ctx.templates.render_with("float_literal", None, &[], &[("value", value_s.as_str())])
            .unwrap_or_else(|| format!("{}", value))
    }
}

/// `IrExprKind::Match` case of `render_expr`, extracted verbatim.
fn render_expr_match(ctx: &RenderContext, expr: &IrExpr) -> String {
    let IrExprKind::Match { subject, arms } = &expr.kind else { unreachable!() };
    // Match subject transforms (.as_str(), .as_deref()) are handled by
    // MatchSubjectPass nanopass — walker just renders what's in the IR.
    let subj = render_expr(ctx, subject);
    // Rust's grammar forbids a bare struct literal in match-subject
    // position ("struct literals are not allowed here") — a record/
    // variant brace construction must be parenthesized (#490).
    let subj = if matches!(&subject.kind, IrExprKind::Record { name: Some(_), .. } | IrExprKind::SpreadRecord { .. }) {
        format!("({})", subj)
    } else {
        subj
    };
    let arms_raw = arms.iter()
        .map(|arm| render_match_arm(ctx, arm, &expr.ty, &subject.ty))
        .collect::<Vec<_>>()
        .join("\n");
    let arms_str = indent_lines(&arms_raw, 4);
    let fallback = format!("match {} {{\n{}\n}}", &subj, &arms_str);
    ctx.templates.render_with("match_expr", None, &[], &[("subject", subj.as_str()), ("arms", arms_str.as_str())])
        .unwrap_or(fallback)
}

pub fn render_expr(ctx: &RenderContext, expr: &IrExpr) -> String {
    match &expr.kind {
        // ── Literals ──
        IrExprKind::LitInt { value } => render_expr_lit_int(ctx, expr, *value),
        IrExprKind::LitFloat { value } => render_expr_lit_float(ctx, expr, *value),
        IrExprKind::LitStr { value } => {
            let escaped = value.replace('\\', "\\\\").replace('"', "\\\"")
                .replace('\n', "\\n").replace('\t', "\\t").replace('\r', "\\r");
            ctx.templates.render_with("string_literal", None, &[], &[("value", escaped.as_str())])
                .unwrap_or_else(|| format!("\"{}\"", value))
        }
        IrExprKind::LitBool { value } => {
            let key = if *value { "bool_literal_true" } else { "bool_literal_false" };
            template_or(ctx, key, &[], &value.to_string())
        }
        IrExprKind::Unit => template_or(ctx, "unit_literal", &[], "()"),

        // ── Variables ──
        IrExprKind::Var { .. } => render_expr_var(ctx, expr),
        IrExprKind::FnRef { name } => name.to_string(),

        // ── Operators ──
        IrExprKind::BinOp { op, left, right } => {
            render_binop(ctx, *op, left, right, &expr.ty)
        }
        IrExprKind::UnOp { op, operand } => {
            let inner = render_expr(ctx, operand);
            match op {
                UnOp::NegInt | UnOp::NegFloat => format!("(-{})", inner),
                UnOp::Not => format!("(!{})", inner),
            }
        }

        // ── Control flow ──
        IrExprKind::If { .. } => render_expr_if(ctx, expr),

        IrExprKind::Match { .. } => render_expr_match(ctx, expr),

        IrExprKind::Block { .. } => render_expr_block(ctx, expr),

        // ── Loops ──
        IrExprKind::ForIn { .. } => render_expr_for_in(ctx, expr),

        IrExprKind::While { cond, body } => {
            let cond_str = render_expr(ctx, cond);
            let body_raw = render_stmts(ctx, body).join("\n");
            let body_str = indent_lines(&body_raw, 4);
            ctx.templates.render_with("while_loop", None, &[], &[("cond", cond_str.as_str()), ("body", body_str.as_str())])
                .unwrap_or_else(|| format!("while _ {{ }}"))
        }

        IrExprKind::Break => template_or(ctx, "break_stmt", &[], "break"),
        IrExprKind::Continue => template_or(ctx, "continue_stmt", &[], "continue"),

        // ── Codegen pre-rendered call ──
        IrExprKind::RenderedCall { code } => code.clone(),

        // ── @inline_rust template dispatch (Stdlib Unification Stage 1) ──
        // Produced by `pass_stdlib_lowering` for calls to bundled stdlib
        // fns whose IrFunction carries an `@inline_rust("...")` attribute.
        // Render each arg into the param-keyed placeholder.
        IrExprKind::InlineRust { .. } => render_expr_inline_rust(ctx, expr),

        // ── Pre-resolved runtime call (from @intrinsic / NormalizeRuntimeCalls) ──
        // #617: a raw NATIVE-runtime result whose type reaches Bytes/Matrix
        // converts to the RcCow value shape at this boundary. A user-module fn
        // normalized into the same RuntimeCall spelling already returns the
        // mapped types — no glue (double-wrap otherwise).
        IrExprKind::RuntimeCall { symbol, args } => {
            let call = render_runtime_call(ctx, symbol, args);
            if rc_cow_symbol_is_native_runtime(symbol.as_str()) {
                rc_cow_result_glue(call, &expr.ty)
            } else {
                call
            }
        }

        // ── Calls ──
        IrExprKind::Call { .. } | IrExprKind::TailCall { .. } => render_expr_call(ctx, expr),

        // ── Collections ──
        IrExprKind::List { .. } => render_expr_list(ctx, expr),

        IrExprKind::Record { .. } => render_expr_record(ctx, expr),

        // ── Access ──
        IrExprKind::Member { object, field } => {
            let expr_s = render_expr(ctx, object);
            ctx.templates.render_with("field_access", None, &[], &[("expr", expr_s.as_str()), ("field", field.as_str())])
                .unwrap_or_else(|| format!("{}.{}", render_expr(ctx, object), field))
        }

        // ── Option / Result ──
        IrExprKind::OptionSome { expr: inner } => {
            let inner_s = render_expr_owned(ctx, inner);
            ctx.templates.render_with("some_expr", None, &[], &[("inner", inner_s.as_str())])
                .unwrap_or_else(|| format!("Some({})", inner_s))
        }
        IrExprKind::OptionNone => {
            // Typed None: pass inner type via bindings + attribute for template guard
            if let Ty::Applied(TypeConstructorId::Option, args) = &expr.ty {
                if args.len() == 1 && !args[0].is_unresolved() {
                    let type_hint_s = render_type(ctx, &args[0]);
                    return ctx.templates.render_with("none_expr", None, &["none_type_hint"], &[("type_hint", type_hint_s.as_str())])
                        .unwrap_or_else(|| "None".into());
                }
            }
            template_or(ctx, "none_expr", &[], "None")
        }
        IrExprKind::ResultOk { expr: inner } => {
            let inner_s = render_expr_owned(ctx, inner);
            // A bare `Ok(x)` is `Result<TyOf(x), _>` — the error type stays open
            // for rustc to infer from context. That fails (E0282) when the
            // surrounding call leaves E unconstrained, e.g.
            // `result.unwrap_or(ok(10), -1)`: `unwrap_or<T, E>` names E only in its
            // `Result<T, E>` parameter, so nothing pins it. The checker already
            // resolved the full Result type (`expr.ty`), so emit a turbofish that
            // carries it — defaulting a still-unconstrained error to String
            // (Almide's conventional error type; mirrors the render_type fix).
            if let Some((ok_s, err_s)) = result_turbofish_args(ctx, &expr.ty) {
                format!("Ok::<{}, {}>({})", ok_s, err_s, inner_s)
            } else {
                ctx.templates.render_with("ok_expr", None, &[], &[("inner", inner_s.as_str())])
                    .unwrap_or_else(|| format!("Ok({})", inner_s))
            }
        }
        IrExprKind::ResultErr { expr: inner } => {
            let inner_str = render_expr(ctx, inner);
            let construct = if matches!(&inner.ty, Ty::String) { "err_inner_string" } else { "err_inner_other" };
            ctx.templates.render_with(construct, None, &[], &[("inner", inner_str.as_str())])
                .or_else(|| ctx.templates.render_with("err_expr", None, &[], &[("inner", inner_str.as_str())]))
                .unwrap_or_else(|| format!("Err({})", render_expr(ctx, inner)))
        }

        // ── Lambda ──
        // A bare (combinator-consumed) lambda leaves its params UNANNOTATED — a
        // fused iterator adapter infers `&T`, and annotating `T` would mismatch.
        IrExprKind::Lambda { params, body, .. } => render_lambda(ctx, params, body, false),

        // ── String interpolation ──
        IrExprKind::StringInterp { parts } => render_string_interp(ctx, parts),

        // ── Range ──
        IrExprKind::Range { start, end, inclusive } => {
            let s = render_expr(ctx, start);
            let e = render_expr(ctx, end);
            let construct = if *inclusive { "range_inclusive" } else { "range_expr" };
            ctx.templates.render_with(construct, None, &[], &[("start", s.as_str()), ("end", e.as_str())])
                .unwrap_or_else(|| "range(...)".into())
        }

        // ── Tuple ──
        IrExprKind::Tuple { elements } => {
            let parts = elements.iter().map(|e| render_expr_owned(ctx, e)).collect::<Vec<_>>().join(", ");
            ctx.templates.render_with("tuple_literal", None, &[], &[("elements", parts.as_str())])
                .unwrap_or_else(|| "tuple(...)".into())
        }
        IrExprKind::TupleIndex { object, index } => {
            let object_s = render_expr(ctx, object);
            let index_s = format!("{}", index);
            ctx.templates.render_with("tuple_index", None, &[], &[("object", object_s.as_str()), ("index", index_s.as_str())])
                .unwrap_or_else(|| format!("{}.{}", render_expr(ctx, object), index))
        }
        IrExprKind::IndexAccess { object, index } => {
            let obj_str = render_expr(ctx, object);
            let idx = render_expr(ctx, index);
            let base = ctx.templates.render_with("index_access", None, &[], &[("object", obj_str.as_str()), ("index", idx.as_str())])
                .unwrap_or_else(|| "idx[...]".into());
            if matches!(object.ty, Ty::Bytes) {
                format!("{} as i64", base)
            } else {
                base
            }
        }
        IrExprKind::MapAccess { object, key } => {
            let obj_str = render_expr(ctx, object);
            let key_str = render_expr(ctx, key);
            ctx.templates.render_with("map_get", None, &[], &[("object", obj_str.as_str()), ("key", key_str.as_str())])
                .unwrap_or_else(|| "map_get(...)".into())
        }

        // ── Map ──
        IrExprKind::MapLiteral { entries } => {
            let entry_template = ctx.templates.render_with("map_entry", None, &[], &[])
                .unwrap_or_else(|| "({key}, {value})".into());
            let parts: Vec<String> = entries.iter()
                .map(|(k, v)| {
                    entry_template.replace("{key}", &render_expr(ctx, k))
                        .replace("{value}", &render_expr(ctx, v))
                })
                .collect();
            let entries_s = parts.join(", ");
            ctx.templates.render_with("map_literal", None, &[], &[("entries", entries_s.as_str())])
                .unwrap_or_else(|| format!("map([{}])", parts.join(", ")))
        }
        IrExprKind::EmptyMap => {
            // Render `AlmideMap::<K, V>::new()` with the key/value types from the
            // literal's resolved type, so an annotated empty map (`[:]: Map[K,V]`)
            // routed through `almide_repr` infers (native E0282 otherwise). When
            // the types are unknown they erase to `_` (inference fills them from
            // the surrounding context, as before this turbofish).
            let (key_ty, value_ty) = match &expr.ty {
                Ty::Applied(TypeConstructorId::Map, args) if args.len() == 2 => {
                    (render_map_type_arg(ctx, &args[0]), render_map_type_arg(ctx, &args[1]))
                }
                _ => ("_".to_string(), "_".to_string()),
            };
            ctx.templates.render_with("empty_map", None, &[], &[("key_type", key_ty.as_str()), ("value_type", value_ty.as_str())])
                .unwrap_or_else(|| format!("AlmideMap::<{}, {}>::new()", key_ty, value_ty))
        }

        // ── SpreadRecord ──
        IrExprKind::SpreadRecord { .. } => render_expr_spread_record(ctx, expr),

        // ── Try / Await / Unwrap / ToOption ──
        IrExprKind::Try { expr: inner } => {
            let s = render_expr(ctx, inner);
            ctx.templates.render_with("try_expr", None, &[], &[("inner", s.as_str())])
                .unwrap_or_else(|| "try(...)".into())
        }
        IrExprKind::Unwrap { .. } => render_expr_unwrap(ctx, expr),
        IrExprKind::UnwrapOr { expr: inner, fallback } => {
            let s = render_expr(ctx, inner);
            let f = render_expr(ctx, fallback);
            // Rc wrapping for List[Fn] fallback is now handled by RustLoweringPass
            // which inserts RcWrap nodes into the IR.
            let when_type = if inner.ty.is_option() { Some("Option") } else { None };
            // When inner.ty is Unknown, defaults to Result template.
            // This is correct if type inference produced Unknown due to a bug;
            // the Rust compiler will catch any mismatch.
            ctx.templates.render_with("unwrap_or_expr", when_type, &[], &[("inner", s.as_str()), ("fallback", f.as_str())])
                .unwrap_or_else(|| format!("{}.unwrap_or({})", s, f))
        }
        IrExprKind::ToOption { expr: inner } => {
            if inner.ty.is_option() {
                render_expr(ctx, inner)
            } else {
                let s = render_expr(ctx, inner);
                ctx.templates.render_with("to_option_expr", None, &[], &[("inner", s.as_str())])
                    .unwrap_or_else(|| format!("({}).ok()", s))
            }
        }
        IrExprKind::OptionalChain { expr: inner, field } => {
            let s = render_expr(ctx, inner);
            ctx.templates.render_with("optional_chain_expr", None, &[], &[("inner", s.as_str()), ("field", field)])
                .unwrap_or_else(|| format!("{}.as_ref().map(|__v| __v.{}.clone())", s, field))
        }
        IrExprKind::Await { expr: inner } => {
            let s = render_expr(ctx, inner);
            ctx.templates.render_with("await_expr", None, &[], &[("inner", s.as_str())])
                .unwrap_or_else(|| "await(...)".into())
        }

        // ── Codegen nodes (inserted by passes — walker just renders) ──
        IrExprKind::Clone { .. } => render_expr_clone(ctx, expr),
        IrExprKind::Deref { expr: inner } => {
            let name_s = render_expr(ctx, inner);
            ctx.templates.render_with("deref_var", None, &[], &[("name", name_s.as_str())])
                .unwrap_or_else(|| format!("(*{})", name_s))
        }
        IrExprKind::Borrow { .. } => render_expr_borrow(ctx, expr),
        IrExprKind::BoxNew { expr: inner } => {
            format!("std::boxed::Box::new({})", render_expr(ctx, inner))
        }
        IrExprKind::RcWrap { expr: inner, cast_ty, wrap } => {
            // A BOXED closure literal needs annotated params (the `as` cast does
            // not back-infer them). Wrap in parens so a boxed-then-CALLED closure
            // `(Rc::new(..) as T)(args)` parses (a cast can't be followed by `(`).
            let s = if let IrExprKind::Lambda { params, body, .. } = &inner.kind {
                render_lambda(ctx, params, body, true)
            } else {
                render_expr(ctx, inner)
            };
            match wrap {
                // fan.race/any/settle thunk: `Box<dyn Fn + Send + Sync>` is itself
                // `Fn + Send + Sync`, so heterogeneous capturing thunks unify in the
                // runtime's `Vec<impl Fn() -> _ + Send + Sync>` (fixes E0308).
                almide_ir::FnBox::BoxSendSync => {
                    let ty = cast_ty.as_deref().expect("fan thunk RcWrap always carries a Fn cast_ty");
                    let box_type = super::helpers::render_type_box_fn(ctx, ty, "Send + Sync");
                    format!("(std::boxed::Box::new({}) as {})", s, box_type)
                }
                almide_ir::FnBox::Rc => {
                    if let Some(ty) = cast_ty {
                        let rc_type = super::helpers::render_type_rc_fn(ctx, ty);
                        format!("(std::rc::Rc::new({}) as {})", s, rc_type)
                    } else {
                        format!("std::rc::Rc::new({})", s)
                    }
                }
            }
        }
        IrExprKind::RustMacro { name, args } => {
            // Render macro args — LitStr rendered as bare &str (no .to_string()).
            // Control chars must be escaped here too, otherwise they land as
            // real source newlines and the Rust source formatter's continuation
            // indent leaks into the string literal at runtime (same failure
            // mode as StringInterp's Lit parts).
            let args_str = args.iter().map(|a| {
                match &a.kind {
                    IrExprKind::LitStr { value } => {
                        let escaped = value
                            .replace('\\', "\\\\")
                            .replace('"', "\\\"")
                            .replace('\n', "\\n")
                            .replace('\t', "\\t")
                            .replace('\r', "\\r");
                        format!("\"{}\"", escaped)
                    }
                    _ => render_expr(ctx, a),
                }
            }).collect::<Vec<_>>().join(", ");
            format!("{}!({})", name, args_str)
        }
        IrExprKind::ToVec { expr: inner } => {
            if matches!(&inner.kind, IrExprKind::Range { .. }) {
                format!("({}).collect::<Vec<_>>()", render_expr(ctx, inner))
            } else {
                format!("({}).to_vec()", render_expr(ctx, inner))
            }
        }

        // ── Hole / Todo ──
        IrExprKind::Hole => template_or(ctx, "hole", &[], "todo!()"),
        IrExprKind::Todo { message } => {
            template_or(ctx, "todo", &[], &format!("todo!(\"{}\")", message))
        }

        // ── Fan (concurrency) — fully template-driven ──
        IrExprKind::Fan { exprs } => render_fan(ctx, exprs),

        // ── Iterator chain (Rust-only) ──
        IrExprKind::IterChain { source, consume, steps, collector } => {
            render_iter_chain(ctx, source, *consume, steps, collector)
        }

        // ── Closure conversion nodes (WASM-only, never reached by Rust walker) ──
        IrExprKind::ClosureCreate { .. } | IrExprKind::EnvLoad { .. } => {
            unreachable!("ClosureCreate/EnvLoad should only appear in WASM pipeline")
        }
    }
}

include!("expressions_p2.rs");
