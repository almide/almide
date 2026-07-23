// Continuation of statements.rs: box-pattern unboxing (#610), match-arm
// rendering, and IrPattern rendering (split out for the 800-line file cap).

// ── #610: nested constructor patterns through a `Box` ──
//
// Rust cannot pattern-match through a `Box` on stable (box-patterns are
// unstable). A nested constructor on a BOXED recursive field —
// `Node(Leaf(a), Leaf(b))` where `Node`'s fields are `Box<Tree>` — used to render
// `Tree::Node(Tree::Leaf(a), ..)`, which rustc rejects: the field is `Box<Tree>`,
// not `Tree` (E0308). We rewrite the arm instead:
//   * each boxed-nested position becomes a fresh `Box` binding in a FLAT pattern,
//   * a `matches!` shape-guard verifies the nested structure (a non-match falls
//     through to a later arm — refinement, exactly like the wasm emitter),
//   * a `let-else` moves the value out of the box in the body and binds the inner
//     names by value (matching the by-value `*box` convention of simple arms).
// All stable since 1.65, edition-agnostic, any nesting depth. With no boxed-nested
// position `unbox_arm_pattern` returns None and the arm renders unchanged.

fn pattern_is_complex(p: &IrPattern) -> bool {
    matches!(p, IrPattern::Constructor { .. } | IrPattern::RecordPattern { .. })
}

fn fresh_box_var(counter: &mut usize) -> String {
    let v = format!("__bx{}", *counter);
    *counter += 1;
    v
}

fn qualify_ctor(ctx: &RenderContext, name: &str, enum_hint: Option<&str>) -> String {
    let enum_name = enum_hint.map(|s| s.to_string())
        .or_else(|| ctx.ann.ctor_to_enum.get(name).map(|s| s.to_string()));
    match enum_name {
        Some(en) => ctx.templates.render_with("ctor_qualify", None, &[],
            &[("enum_name", en.as_str()), ("ctor_name", name)])
            .unwrap_or_else(|| format!("{}::{}", en, name)),
        None => name.to_string(),
    }
}

fn is_boxed_tuple_field(ctx: &RenderContext, ctor: &str, idx: usize) -> bool {
    ctx.ann.boxed_fields.contains(&(ctor.to_string(), idx.to_string()))
}

fn is_boxed_record_field(ctx: &RenderContext, ctor: &str, field: &str) -> bool {
    ctx.ann.boxed_fields.contains(&(ctor.to_string(), field.to_string()))
}

/// `matches!`-shaped boolean that `access` (a `&Enum`-typed expr) structurally
/// matches `pat`, deref-ing one box per level. The shape must carry EVERY
/// refutable constraint the body's `let-else` will re-assert — the guard is the
/// only thing standing between a non-matching value and the let-else's
/// `unreachable!()` (#757: erasing a non-boxed inner tag like `Color::Red` to
/// `_` let a `Black` node through the guard and panicked instead of falling
/// through to the next arm).
fn box_shape_guard(ctx: &RenderContext, pat: &IrPattern, access: &str, counter: &mut usize) -> String {
    match pat {
        IrPattern::Constructor { .. } | IrPattern::RecordPattern { .. } => {
            let mut subs = Vec::new();
            let shape = guard_shape(ctx, pat, counter, &mut subs);
            if subs.is_empty() {
                format!("matches!({}, {})", access, shape)
            } else {
                format!("matches!({}, {} if {})", access, shape, subs.join(" && "))
            }
        }
        // Non-constructor pattern in a boxed position cannot occur (a recursive
        // field is always a variant); be conservative and impose no constraint.
        _ => "true".to_string(),
    }
}

/// Guard shape of a sub-pattern inside a `matches!` clause: bindings and
/// wildcards impose no constraint (`_` — a real binding here would be dead and
/// warn), literals and non-boxed constructors keep their refutable structure
/// inline, and a boxed-nested sub-pattern binds a fresh var whose deref guard
/// joins `subs` (Rust can't pattern-match through a `Box` on stable).
fn guard_shape(ctx: &RenderContext, pat: &IrPattern, counter: &mut usize, subs: &mut Vec<String>) -> String {
    match pat {
        IrPattern::Wildcard | IrPattern::Bind { .. } => "_".to_string(),
        IrPattern::Literal { .. } => render_pattern_hinted(ctx, pat, None),
        IrPattern::Some { inner } => format!("Some({})", guard_shape(ctx, inner, counter, subs)),
        IrPattern::None => "None".to_string(),
        IrPattern::Ok { inner } => format!("Ok({})", guard_shape(ctx, inner, counter, subs)),
        IrPattern::Err { inner } => format!("Err({})", guard_shape(ctx, inner, counter, subs)),
        IrPattern::Tuple { elements } => {
            let shapes: Vec<String> =
                elements.iter().map(|p| guard_shape(ctx, p, counter, subs)).collect();
            format!("({})", shapes.join(", "))
        }
        IrPattern::Constructor { name, args } => {
            let qualified = qualify_ctor(ctx, name.as_str(), None);
            if args.is_empty() {
                return qualified;
            }
            let shapes: Vec<String> = args
                .iter()
                .enumerate()
                .map(|(i, arg)| {
                    if is_boxed_tuple_field(ctx, name.as_str(), i) && pattern_is_complex(arg) {
                        let g = fresh_box_var(counter);
                        subs.push(box_shape_guard(ctx, arg, &format!("&**{}", g), counter));
                        g
                    } else {
                        guard_shape(ctx, arg, counter, subs)
                    }
                })
                .collect();
            format!("{}({})", qualified, shapes.join(", "))
        }
        IrPattern::RecordPattern { name, fields, .. } => {
            let qualified = qualify_ctor(ctx, name.as_str(), None);
            let shapes: Vec<String> = fields
                .iter()
                .map(|fp| match &fp.pattern {
                    Some(p) if is_boxed_record_field(ctx, name.as_str(), fp.name.as_str())
                        && pattern_is_complex(p) =>
                    {
                        let g = fresh_box_var(counter);
                        subs.push(box_shape_guard(ctx, p, &format!("&**{}", g), counter));
                        format!("{}: {}", fp.name, g)
                    }
                    Some(p) => format!("{}: {}", fp.name, guard_shape(ctx, p, counter, subs)),
                    None => format!("{}: _", fp.name),
                })
                .collect();
            format!("{} {{ {}, .. }}", qualified, shapes.join(", "))
        }
        // ListPatternLowering rewrites list patterns before rendering reaches
        // this point; nothing refutable can arrive here.
        IrPattern::List { .. } => "_".to_string(),
    }
}

/// Emit `let <flat pat> = <move_expr> else { unreachable!() };` to move the value
/// out of its box and bind the inner names by value, recursing for deeper boxes.
/// The guard has already verified the structure, so `else` is dead.
fn box_extract(ctx: &RenderContext, pat: &IrPattern, move_expr: &str, binds: &mut Vec<String>, counter: &mut usize) {
    match pat {
        IrPattern::Constructor { name, args } => box_extract_constructor(ctx, name, args, move_expr, binds, counter),
        IrPattern::RecordPattern { name, fields, .. } => box_extract_record(ctx, name, fields, move_expr, binds, counter),
        _ => {}
    }
}

/// `IrPattern::Constructor` case of `box_extract`, extracted verbatim
/// (cog>30 decomposition, pattern 1 — `binds`/`counter` are write-only
/// accumulators, same safety class as `check_needs_ownership`'s `needs`).
fn box_extract_constructor(ctx: &RenderContext, name: &str, args: &[IrPattern], move_expr: &str, binds: &mut Vec<String>, counter: &mut usize) {
    let qualified = qualify_ctor(ctx, name, None);
    let mut flat = Vec::with_capacity(args.len());
    let mut deeper: Vec<(String, &IrPattern)> = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        if is_boxed_tuple_field(ctx, name, i) && pattern_is_complex(arg) {
            let e = fresh_box_var(counter);
            flat.push(e.clone());
            deeper.push((e, arg));
        } else {
            flat.push(render_pattern_hinted(ctx, arg, None));
        }
    }
    let flat_pat = if args.is_empty() { qualified } else { format!("{}({})", qualified, flat.join(", ")) };
    binds.push(format!("let {} = {} else {{ unreachable!() }};", flat_pat, move_expr));
    for (e, sub) in deeper {
        box_extract(ctx, sub, &format!("*{}", e), binds, counter);
    }
}

/// `IrPattern::RecordPattern` case of `box_extract`, extracted verbatim
/// (cog>30 decomposition).
fn box_extract_record(ctx: &RenderContext, name: &str, fields: &[IrFieldPattern], move_expr: &str, binds: &mut Vec<String>, counter: &mut usize) {
    let qualified = qualify_ctor(ctx, name, None);
    let mut flat = Vec::with_capacity(fields.len());
    let mut deeper: Vec<(String, &IrPattern)> = Vec::new();
    for fp in fields {
        match &fp.pattern {
            Some(p) if is_boxed_record_field(ctx, name, fp.name.as_str())
                && pattern_is_complex(p) =>
            {
                let e = fresh_box_var(counter);
                flat.push(format!("{}: {}", fp.name, e));
                deeper.push((e, p));
            }
            Some(p) => flat.push(format!("{}: {}", fp.name, render_pattern_hinted(ctx, p, None))),
            None => flat.push(fp.name.to_string()),
        }
    }
    binds.push(format!("let {} {{ {}, .. }} = {} else {{ unreachable!() }};", qualified, flat.join(", "), move_expr));
    for (e, sub) in deeper {
        box_extract(ctx, sub, &format!("*{}", e), binds, counter);
    }
}

/// Rewrite an arm whose top-level variant pattern has a boxed-nested constructor.
/// Returns `(flat_pattern, shape_guards, body_let_else_binds)` or None if the arm
/// has no boxed-nested position (the common case → no rewrite).
/// Accumulates the fresh-box-var counter, structural shape-guards, and box
/// move-out binds threaded through both arms of [`unbox_arm_pattern`].
/// Bundled so each arm helper stays at or under the `max-params` limit.
#[derive(Default)]
struct UnboxState {
    counter: usize,
    guards: Vec<String>,
    binds: Vec<String>,
}

/// `Constructor { name, args }` arm of [`unbox_arm_pattern`].
fn unbox_constructor_pattern(ctx: &RenderContext, name: &str, args: &[IrPattern], enum_hint: Option<&str>, st: &mut UnboxState) -> String {
    let qualified = qualify_ctor(ctx, name, enum_hint);
    let mut flat = Vec::with_capacity(args.len());
    for (i, arg) in args.iter().enumerate() {
        if is_boxed_tuple_field(ctx, name, i) && pattern_is_complex(arg) {
            let v = fresh_box_var(&mut st.counter);
            st.guards.push(box_shape_guard(ctx, arg, &format!("&*{}", v), &mut st.counter));
            box_extract(ctx, arg, &format!("*{}", v), &mut st.binds, &mut st.counter);
            flat.push(v);
        } else {
            flat.push(render_pattern_hinted(ctx, arg, None));
        }
    }
    if args.is_empty() { qualified } else { format!("{}({})", qualified, flat.join(", ")) }
}

/// `RecordPattern { name, fields, .. }` arm of [`unbox_arm_pattern`].
fn unbox_record_pattern(ctx: &RenderContext, name: &str, fields: &[IrFieldPattern], enum_hint: Option<&str>, st: &mut UnboxState) -> String {
    let qualified = qualify_ctor(ctx, name, enum_hint);
    let mut flat = Vec::with_capacity(fields.len());
    for fp in fields {
        match &fp.pattern {
            Some(p) if is_boxed_record_field(ctx, name, fp.name.as_str())
                && pattern_is_complex(p) =>
            {
                let v = fresh_box_var(&mut st.counter);
                st.guards.push(box_shape_guard(ctx, p, &format!("&*{}", v), &mut st.counter));
                box_extract(ctx, p, &format!("*{}", v), &mut st.binds, &mut st.counter);
                flat.push(format!("{}: {}", fp.name, v));
            }
            Some(p) => flat.push(format!("{}: {}", fp.name, render_pattern_hinted(ctx, p, None))),
            None => flat.push(fp.name.to_string()),
        }
    }
    format!("{} {{ {} }}", qualified, flat.join(", "))
}

fn unbox_arm_pattern(ctx: &RenderContext, pat: &IrPattern, enum_hint: Option<&str>)
    -> Option<(String, Vec<String>, Vec<String>)>
{
    let mut st = UnboxState::default();
    let flat = match pat {
        IrPattern::Constructor { name, args } => unbox_constructor_pattern(ctx, name, args, enum_hint, &mut st),
        IrPattern::RecordPattern { name, fields, .. } => unbox_record_pattern(ctx, name, fields, enum_hint, &mut st),
        _ => return None,
    };
    if st.guards.is_empty() { None } else { Some((flat, st.guards, st.binds)) }
}

// ── Match arm rendering ──

pub fn render_match_arm(ctx: &RenderContext, arm: &IrMatchArm, match_ty: &almide_lang::types::Ty, subject_ty: &almide_lang::types::Ty) -> String {
    // #413: a top-level variant pattern belongs to the match SUBJECT's enum, so
    // pass that enum's (mangled) name as a hint — it disambiguates a constructor
    // name shared across packages, which the global ctor→enum map collapses.
    let enum_hint = match subject_ty {
        // Only when the subject is a known variant enum — never a struct/opaque
        // type (whose patterns must not be qualified `Type::field`).
        almide_lang::types::Ty::Named(n, _) if ctx.ann.ctor_to_enum.values().any(|e| e.as_str() == n.as_str())
            => Some(n.as_str()),
        _ => None,
    };
    // #610: a boxed-nested constructor pattern is rewritten to a flat pattern + a
    // `matches!` shape-guard + `let-else` box move-outs in the body. None when the
    // arm has no boxed-nested position (the common case → identical to before).
    let (pattern, shape_guards, box_binds) = match unbox_arm_pattern(ctx, &arm.pattern, enum_hint) {
        Some((flat, guards, binds)) => (flat, guards, binds),
        None => (render_pattern_hinted(ctx, &arm.pattern, enum_hint), Vec::new(), Vec::new()),
    };
    // err() in a match arm where the match type is NOT Result: early return.
    // This handles `let x: T = match ... { none => err("msg") }` in
    // functions returning Result — the err() doesn't contribute a T value,
    // it exits the function with an error.
    let raw_body = if matches!(&arm.body.kind, IrExprKind::ResultErr { .. }) && !match_ty.is_result() {
        format!("return {}", render_expr(ctx, &arm.body))
    } else {
        super::expressions::render_expr_owned(ctx, &arm.body)
    };
    // The box move-outs (`let Tree::Leaf(a) = *__bx0 else …`) run FIRST in the
    // arm body, then the original body sees the inner bindings.
    let body = if box_binds.is_empty() {
        raw_body
    } else {
        format!("{{ {} {} }}", box_binds.join(" "), raw_body)
    };
    // Append guards: the structural shape-guards (which must hold for the rewritten
    // arm to apply, so a non-match falls through) then any user guard.
    let mut guard_conds = shape_guards;
    if let Some(ref guard) = arm.guard {
        guard_conds.push(render_expr(ctx, guard));
    }
    let full_pattern = if guard_conds.is_empty() {
        pattern
    } else {
        format!("{} if {}", pattern, guard_conds.join(" && "))
    };
    ctx.templates.render_with("match_arm_inline", None, &[], &[("pattern", full_pattern.as_str()), ("body", body.as_str())])
        .unwrap_or_else(|| format!("_ => _,"))
}

/// Check if any match arm uses a list pattern.
pub fn arms_have_list_pattern(arms: &[IrMatchArm]) -> bool {
    arms.iter().any(|arm| matches!(&arm.pattern, IrPattern::List { .. }))
}

pub fn render_pattern(ctx: &RenderContext, pat: &IrPattern) -> String {
    render_pattern_hinted(ctx, pat, None)
}

/// `render_pattern_hinted`'s `Literal` arm, extracted verbatim (cog>25
/// decomposition). In patterns, literals must be bare (no `.to_string()`,
/// no `i64` suffix for match).
fn render_pattern_literal(ctx: &RenderContext, expr: &IrExpr) -> String {
    match &expr.kind {
        IrExprKind::LitStr { value } => {
            let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
            format!("\"{}\"", escaped)
        }
        IrExprKind::LitInt { value } => format!("{}", value),
        IrExprKind::LitFloat { value } => format!("{}", value),
        IrExprKind::LitBool { value } => format!("{}", value),
        _ => render_expr(ctx, expr),
    }
}

/// Resolve a `Constructor`/`RecordPattern` name's enum qualifier: prefer the
/// subject's enum (`enum_hint`, #413) over the collapsing global
/// `ctor_to_enum` map. Shared by `render_pattern_constructor` and
/// `render_pattern_record` (same lookup, previously duplicated inline).
fn resolve_pattern_enum_name(ctx: &RenderContext, enum_hint: Option<&str>, name: &str) -> Option<String> {
    enum_hint.map(|s| s.to_string())
        .or_else(|| ctx.ann.ctor_to_enum.get(name).map(|s| s.to_string()))
}

/// `render_pattern_hinted`'s `Constructor` arm, extracted verbatim.
fn render_pattern_constructor(ctx: &RenderContext, name: &str, args: &[IrPattern], enum_hint: Option<&str>) -> String {
    let enum_name = resolve_pattern_enum_name(ctx, enum_hint, name);
    let qualified = if let Some(enum_name) = enum_name {
        ctx.templates.render_with("ctor_qualify", None, &[], &[("enum_name", enum_name.as_str()), ("ctor_name", name)])
            .unwrap_or_else(|| format!("{}::{}", enum_name, name))
    } else {
        name.to_string()
    };
    if args.is_empty() {
        qualified
    } else {
        let args_str = args.iter().map(|a| render_pattern(ctx, a)).collect::<Vec<_>>().join(", ");
        format!("{}({})", qualified, args_str)
    }
}

/// `render_pattern_hinted`'s `RecordPattern` arm, extracted verbatim.
fn render_pattern_record(ctx: &RenderContext, name: &str, fields: &[almide_ir::IrFieldPattern], rest: bool, enum_hint: Option<&str>) -> String {
    // Qualify enum variant record patterns: Circle → Shape::Circle.
    let qualified_name = if let Some(enum_name) = resolve_pattern_enum_name(ctx, enum_hint, name) {
        format!("{}::{}", enum_name, name)
    } else {
        name.to_string()
    };
    let fields_str = fields.iter()
        .map(|f| match &f.pattern {
            Some(p) => format!("{}: {}", f.name, render_pattern(ctx, p)),
            None => f.name.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ");
    if rest {
        let construct = if fields_str.is_empty() { "record_pattern_rest_empty" } else { "record_pattern_rest" };
        ctx.templates.render_with(construct, None, &[], &[("name", qualified_name.as_str()), ("fields", fields_str.as_str())])
            .unwrap_or_else(|| format!("{} {{ {} }}", qualified_name, fields_str))
    } else {
        format!("{} {{ {} }}", qualified_name, fields_str)
    }
}

/// Like `render_pattern`, but `enum_hint` is the match subject's enum type name
/// (mangled), used to qualify a TOP-LEVEL variant pattern. This disambiguates a
/// constructor name shared across packages (#413): the pattern belongs to the
/// subject's enum, not the (collapsed) global `ctor_to_enum` entry. Nested patterns
/// recurse without a hint (fall back to `ctor_to_enum`).
pub fn render_pattern_hinted(ctx: &RenderContext, pat: &IrPattern, enum_hint: Option<&str>) -> String {
    match pat {
        IrPattern::Wildcard => template_or(ctx, "pattern_wildcard", &[], "_"),
        IrPattern::Bind { var, .. } => ctx.var_name(*var).to_string(),
        IrPattern::Literal { expr } => render_pattern_literal(ctx, expr),
        IrPattern::Some { inner } => {
            let binding_s = render_pattern(ctx, inner);
            ctx.templates.render_with("pattern_some", None, &[], &[("binding", binding_s.as_str())])
                .unwrap_or_else(|| format!("Some(_)"))
        }
        IrPattern::None => template_or(ctx, "pattern_none", &[], "None"),
        IrPattern::Ok { inner } => {
            let binding_s = render_pattern(ctx, inner);
            ctx.templates.render_with("pattern_ok", None, &[], &[("binding", binding_s.as_str())])
                .unwrap_or_else(|| format!("Ok(_)"))
        }
        IrPattern::Err { inner } => {
            let binding_s = render_pattern(ctx, inner);
            ctx.templates.render_with("pattern_err", None, &[], &[("binding", binding_s.as_str())])
                .unwrap_or_else(|| format!("Err(_)"))
        }
        IrPattern::Constructor { name, args } => render_pattern_constructor(ctx, name, args, enum_hint),
        IrPattern::Tuple { elements } => {
            let elems = elements.iter().map(|e| render_pattern(ctx, e)).collect::<Vec<_>>().join(", ");
            ctx.templates.render_with("tuple_literal", None, &[], &[("elements", elems.as_str())])
                .unwrap_or_else(|| "tuple(...)".into())
        }
        IrPattern::List { elements } => {
            if elements.is_empty() {
                "[]".to_string()
            } else {
                let elems = elements.iter().map(|e| render_pattern(ctx, e)).collect::<Vec<_>>().join(", ");
                format!("[{}]", elems)
            }
        }
        IrPattern::RecordPattern { name, fields, rest } => render_pattern_record(ctx, name, fields, *rest, enum_hint),
    }
}
