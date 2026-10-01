// Continuation of statements.rs: box-pattern unboxing (#610), match-arm
// rendering, and IrPattern rendering (split out for the 800-line file cap).

// ── #610: nested patterns through a `Box` ──
//
// Rust cannot pattern-match through a `Box` on stable (box-patterns are
// unstable). A refutable pattern in a BOXED recursive field —
// `Node(Leaf(a), Leaf(b))` where `Node`'s fields are `Box<Tree>` — used to render
// `Tree::Node(Tree::Leaf(a), ..)`, which rustc rejects: the field is `Box<Tree>`,
// not `Tree` (E0308). We rewrite the arm instead:
//   * each boxed-nested position becomes a fresh `Box` binding in a FLAT pattern,
//   * a `matches!` shape-guard verifies the nested structure (a non-match falls
//     through to a later arm — refinement, exactly like the wasm emitter),
//   * a `let-else` moves the value out of the box in the body and binds the inner
//     names by value (matching the by-value `*box` convention of simple arms).
// All stable since 1.65, edition-agnostic, any nesting depth. The recursive
// constructor may itself sit under any wrapper (`some(Pair(Leaf(x), _))`,
// a tuple, a non-recursive case's payload — #3174): the flattening walks the
// whole pattern. With no boxed-nested position `unbox_arm_pattern` returns None
// and the arm renders unchanged.

/// A pattern in a boxed field position that must move out of the box: it is
/// refutable (or holds a refutable part), so Rust would have to match through
/// the `Box`. A binder or wildcard binds the box itself (BoxDeref derefs its
/// reads); an as-pattern keeps that binding too.
fn needs_unbox(p: &IrPattern) -> bool {
    match p {
        IrPattern::Wildcard | IrPattern::Bind { .. } | IrPattern::As { .. } => false,
        IrPattern::Tuple { elements } => elements.iter().any(needs_unbox),
        _ => true,
    }
}

fn fresh_box_var(counter: &mut usize) -> String {
    let v = format!("__bx{}", *counter);
    *counter += 1;
    v
}

fn qualify_ctor(ctx: &RenderContext, name: &str, subject: Option<&Ty>) -> String {
    match super::ctor_enum_for(ctx, name, subject) {
        Some(en) => ctx.templates.render_with("ctor_qualify", None, &[],
            &[("enum_name", en.as_str()), ("ctor_name", name)])
            .unwrap_or_else(|| format!("{}::{}", en, name)),
        None => name.to_string(),
    }
}

/// The type of a pattern's `i`-th sub-position under a value typed `ty`: an
/// `Option` / `Result` / `List` argument or a tuple element. `Err` is the
/// `Result`'s second argument. None when the walker cannot tell — the
/// sub-pattern then qualifies by the bare case name alone.
fn sub_pattern_ty(ty: Option<&Ty>, i: usize) -> Option<Ty> {
    match ty? {
        Ty::Applied(_, args) => args.get(i).cloned(),
        Ty::Tuple(elems) => elems.get(i).cloned(),
        _ => None,
    }
}

/// The declared type of payload position `field` (a tuple index or a record
/// field name) of case `ctor`, under a value typed `ty` (#3176). A nested
/// pattern has no subject of its own, so this is how it learns which of two
/// same-named cases it names.
fn case_field_ty(ctx: &RenderContext, ty: Option<&Ty>, ctor: &str, field: &str) -> Option<Ty> {
    let owner = match ty {
        Some(Ty::Named(n, _)) if ctx.ann.case_fields.contains_key(&(n.to_string(), ctor.to_string())) => n.to_string(),
        _ => super::ctor_enum_for(ctx, ctor, ty)?,
    };
    ctx.ann.case_fields.get(&(owner, ctor.to_string()))?
        .iter().find(|(f, _)| f == field).map(|(_, t)| t.clone())
}

/// Is payload position `field` (a tuple index or a record field name) of case
/// `ctor` boxed, for a value typed `ty`?
fn is_boxed_field(ctx: &RenderContext, ty: Option<&Ty>, ctor: &str, field: &str) -> bool {
    super::case_field_is_boxed(ctx, ty, ctor, field)
}

/// `matches!`-shaped boolean that `access` (a reference to the boxed value,
/// typed `ty` when known) structurally matches `pat`, deref-ing one box per
/// level. The shape must carry EVERY refutable constraint the body's
/// `let-else` will re-assert — the guard is the only thing standing between a
/// non-matching value and the let-else's `unreachable!()` (#757: erasing a
/// non-boxed inner tag like `Color::Red` to `_` let a `Black` node through the
/// guard and panicked instead of falling through to the next arm).
fn box_shape_guard(ctx: &RenderContext, pat: &IrPattern, ty: Option<&Ty>, access: &str, counter: &mut usize) -> String {
    let mut subs = Vec::new();
    let shape = guard_shape(ctx, pat, ty, counter, &mut subs);
    if subs.is_empty() {
        format!("matches!({}, {})", access, shape)
    } else {
        format!("matches!({}, {} if {})", access, shape, subs.join(" && "))
    }
}

/// Guard shape of a sub-pattern inside a `matches!` clause: bindings and
/// wildcards impose no constraint (`_` — a real binding here would be dead and
/// warn), literals and non-boxed constructors keep their refutable structure
/// inline, and a boxed-nested sub-pattern binds a fresh var whose deref guard
/// joins `subs` (Rust can't pattern-match through a `Box` on stable).
fn guard_shape(ctx: &RenderContext, pat: &IrPattern, ty: Option<&Ty>, counter: &mut usize, subs: &mut Vec<String>) -> String {
    match pat {
        IrPattern::Wildcard | IrPattern::Bind { .. } => "_".to_string(),
        IrPattern::As { inner, .. } => guard_shape(ctx, inner, ty, counter, subs),
        IrPattern::Literal { .. } => render_pattern_hinted(ctx, pat, None),
        IrPattern::Some { inner } => format!("Some({})", guard_shape(ctx, inner, sub_pattern_ty(ty, 0).as_ref(), counter, subs)),
        IrPattern::None => "None".to_string(),
        IrPattern::Ok { inner } => format!("Ok({})", guard_shape(ctx, inner, sub_pattern_ty(ty, 0).as_ref(), counter, subs)),
        IrPattern::Err { inner } => format!("Err({})", guard_shape(ctx, inner, sub_pattern_ty(ty, 1).as_ref(), counter, subs)),
        IrPattern::Tuple { elements } => {
            let shapes: Vec<String> = elements.iter().enumerate()
                .map(|(i, p)| guard_shape(ctx, p, sub_pattern_ty(ty, i).as_ref(), counter, subs)).collect();
            format!("({})", super::helpers::tuple_elems_join(&shapes))
        }
        IrPattern::Constructor { name, args } => {
            let qualified = qualify_ctor(ctx, name.as_str(), ty);
            if args.is_empty() {
                return qualified;
            }
            let shapes: Vec<String> = args.iter().enumerate()
                .map(|(i, arg)| guard_slot(ctx, ty, name.as_str(), &i.to_string(), arg, counter, subs))
                .collect();
            format!("{}({})", qualified, shapes.join(", "))
        }
        IrPattern::RecordPattern { name, fields, .. } => {
            let qualified = qualify_ctor(ctx, name.as_str(), ty);
            let shapes: Vec<String> = fields.iter()
                .map(|fp| match &fp.pattern {
                    Some(p) => format!("{}: {}", ctx.field_ident(fp.name.as_str()),
                        guard_slot(ctx, ty, name.as_str(), fp.name.as_str(), p, counter, subs)),
                    None => format!("{}: _", ctx.field_ident(fp.name.as_str())),
                })
                .collect();
            format!("{} {{ {}, .. }}", qualified, shapes.join(", "))
        }
        // ListPatternLowering rewrites list patterns before rendering reaches
        // this point; nothing refutable can arrive here.
        IrPattern::List { .. } => "_".to_string(),
    }
}

/// One payload position `field` of case `ctor` in a guard shape: a boxed
/// position holding a refutable pattern binds a fresh box var and guards
/// through it.
fn guard_slot(ctx: &RenderContext, ty: Option<&Ty>, ctor: &str, field: &str, arg: &IrPattern, counter: &mut usize, subs: &mut Vec<String>) -> String {
    let arg_ty = case_field_ty(ctx, ty, ctor, field);
    if is_boxed_field(ctx, ty, ctor, field) && needs_unbox(arg) {
        let g = fresh_box_var(counter);
        subs.push(box_shape_guard(ctx, arg, arg_ty.as_ref(), &format!("&**{}", g), counter));
        g
    } else {
        guard_shape(ctx, arg, arg_ty.as_ref(), counter, subs)
    }
}

/// Does `pat` (matched against a value typed `ty`) hold a refutable pattern
/// in a boxed field position anywhere — at the top, or under a wrapper
/// (`some(..)`, a tuple, another case's payload)?
fn has_box_nest(ctx: &RenderContext, pat: &IrPattern, ty: Option<&Ty>) -> bool {
    let slot = |ctor: &str, field: &str, p: &IrPattern| {
        (is_boxed_field(ctx, ty, ctor, field) && needs_unbox(p))
            || has_box_nest(ctx, p, case_field_ty(ctx, ty, ctor, field).as_ref())
    };
    match pat {
        IrPattern::Constructor { name, args } => args.iter().enumerate()
            .any(|(i, a)| slot(name.as_str(), &i.to_string(), a)),
        IrPattern::RecordPattern { name, fields, .. } => fields.iter()
            .any(|fp| fp.pattern.as_ref().is_some_and(|p| slot(name.as_str(), fp.name.as_str(), p))),
        IrPattern::Some { inner } | IrPattern::Ok { inner } => has_box_nest(ctx, inner, sub_pattern_ty(ty, 0).as_ref()),
        IrPattern::Err { inner } => has_box_nest(ctx, inner, sub_pattern_ty(ty, 1).as_ref()),
        IrPattern::As { inner, .. } => has_box_nest(ctx, inner, ty),
        IrPattern::Tuple { elements } => elements.iter().enumerate()
            .any(|(i, e)| has_box_nest(ctx, e, sub_pattern_ty(ty, i).as_ref())),
        _ => false,
    }
}

/// Accumulates the fresh-box-var counter, structural shape-guards, and box
/// move-out binds of one arm's rewrite.
#[derive(Default)]
struct UnboxState {
    counter: usize,
    guards: Vec<String>,
    binds: Vec<String>,
    /// The subject is matched by reference (a `&T` param or binder): a box
    /// var is `&Box<T>`, so the shape guard looks through two derefs and
    /// the move-out binds by reference (`&**b`) instead of moving (`*b`).
    borrowed: bool,
}

impl UnboxState {
    fn through_box(&self, v: &str) -> String {
        if self.borrowed { format!("&**{v}") } else { format!("&*{v}") }
    }
    fn out_of_box(&self, v: &str) -> String {
        if self.borrowed { format!("&**{v}") } else { format!("*{v}") }
    }
}

/// A boxed position the flattening replaced by a fresh box var: the var, the
/// pattern it must match once moved out, and that value's type when known.
type Deferred<'p> = Vec<(String, &'p IrPattern, Option<Ty>)>;

/// The FLAT spelling of `pat` (matched against a value typed `ty`): every
/// refutable pattern in a boxed position is replaced by a fresh box var,
/// queued in `deferred`. A sub-pattern with no boxed nest renders as before.
fn flatten<'p>(ctx: &RenderContext, pat: &'p IrPattern, ty: Option<&Ty>, st: &mut UnboxState, deferred: &mut Deferred<'p>) -> String {
    if !has_box_nest(ctx, pat, ty) {
        return render_pattern_hinted(ctx, pat, ty);
    }
    let mut slot = |boxed: bool, p: &'p IrPattern, sub_ty: Option<Ty>, st: &mut UnboxState| {
        if boxed && needs_unbox(p) {
            let v = fresh_box_var(&mut st.counter);
            deferred.push((v.clone(), p, sub_ty));
            v
        } else {
            flatten(ctx, p, sub_ty.as_ref(), st, deferred)
        }
    };
    match pat {
        IrPattern::Constructor { name, args } => {
            let parts: Vec<String> = args.iter().enumerate().map(|(i, a)| {
                let f = i.to_string();
                slot(is_boxed_field(ctx, ty, name.as_str(), &f), a, case_field_ty(ctx, ty, name.as_str(), &f), st)
            }).collect();
            format!("{}({})", qualify_ctor(ctx, name.as_str(), ty), parts.join(", "))
        }
        IrPattern::RecordPattern { name, fields, rest } => {
            let parts: Vec<String> = fields.iter().map(|fp| match &fp.pattern {
                Some(p) => {
                    let (boxed, fty) = (is_boxed_field(ctx, ty, name.as_str(), fp.name.as_str()),
                        case_field_ty(ctx, ty, name.as_str(), fp.name.as_str()));
                    format!("{}: {}", ctx.field_ident(fp.name.as_str()), slot(boxed, p, fty, st))
                }
                None => ctx.field_ident(fp.name.as_str()),
            }).collect();
            let dots = if *rest { ", .." } else { "" };
            format!("{} {{ {}{} }}", qualify_ctor(ctx, name.as_str(), ty), parts.join(", "), dots)
        }
        IrPattern::Some { inner } => format!("Some({})", slot(false, inner, sub_pattern_ty(ty, 0), st)),
        IrPattern::Ok { inner } => format!("Ok({})", slot(false, inner, sub_pattern_ty(ty, 0), st)),
        IrPattern::Err { inner } => format!("Err({})", slot(false, inner, sub_pattern_ty(ty, 1), st)),
        IrPattern::As { var, inner, .. } => format!("{} @ {}", ctx.var_name(*var), slot(false, inner, ty.cloned(), st)),
        IrPattern::Tuple { elements } => {
            let parts: Vec<String> = elements.iter().enumerate()
                .map(|(i, e)| slot(false, e, sub_pattern_ty(ty, i), st)).collect();
            format!("({})", super::helpers::tuple_elems_join(&parts))
        }
        _ => render_pattern_hinted(ctx, pat, ty),
    }
}

/// Emit `let <flat pat> = <move_expr> else { unreachable!() };` to move the value
/// out of its box and bind the inner names by value, recursing for deeper boxes.
/// The guard has already verified the structure, so `else` is dead.
/// `ty` is the boxed value's type, when known.
fn box_extract(ctx: &RenderContext, pat: &IrPattern, ty: Option<&Ty>, move_expr: &str, st: &mut UnboxState) {
    let mut deferred = Vec::new();
    let flat = flatten(ctx, pat, ty, st, &mut deferred);
    st.binds.push(format!("let {} = {} else {{ unreachable!() }};", flat, move_expr));
    for (e, sub, sub_ty) in deferred {
        let deeper = st.out_of_box(&e);
        box_extract(ctx, sub, sub_ty.as_ref(), &deeper, st);
    }
}

/// Rewrite an arm whose pattern holds a boxed-nested refutable pattern.
/// Returns `(flat_pattern, shape_guards, body_let_else_binds)` or None if the arm
/// has no boxed-nested position (the common case → no rewrite).
fn unbox_arm_pattern(ctx: &RenderContext, pat: &IrPattern, subject: Option<&Ty>, borrowed: bool)
    -> Option<(String, Vec<String>, Vec<String>)>
{
    if !has_box_nest(ctx, pat, subject) {
        return None;
    }
    let mut st = UnboxState { borrowed, ..UnboxState::default() };
    let mut deferred = Vec::new();
    let flat = flatten(ctx, pat, subject, &mut st, &mut deferred);
    for (v, sub, sub_ty) in deferred {
        let access = st.through_box(&v);
        let guard = box_shape_guard(ctx, sub, sub_ty.as_ref(), &access, &mut st.counter);
        st.guards.push(guard);
        let mv = st.out_of_box(&v);
        box_extract(ctx, sub, sub_ty.as_ref(), &mv, &mut st);
    }
    Some((flat, st.guards, st.binds))
}

// ── Match arm rendering ──

/// True when this match needs the refinement BACKSTOP arm: some arm was
/// guard-lowered by the #610 box-pattern rewrite — a `matches!` shape-guard
/// no longer counts toward rustc's exhaustiveness — and no remaining arm is
/// an unguarded irrefutable row. The checker already proved the SOURCE match
/// exhaustive, so the appended `_ => unreachable!()` is dead by construction;
/// without it a TOTAL nested pattern (e.g. through a single-constructor
/// payload type, `FCons(Node(s, _), _)`) died at the native build as rustc
/// E0004 (grain/koka port findings, 2026-08-17).
pub fn match_needs_unreachable_backstop(
    ctx: &RenderContext,
    arms: &[IrMatchArm],
    subject_ty: &almide_lang::types::Ty,
) -> bool {
    if !arms.iter().any(|a| unbox_arm_pattern(ctx, &a.pattern, Some(subject_ty), false).is_some()) {
        return false;
    }
    let has_irrefutable = arms.iter().any(|a| {
        a.guard.is_none()
            && matches!(a.pattern, IrPattern::Wildcard | IrPattern::Bind { .. })
    });
    !has_irrefutable
}

/// Is a match on `subject` a match by REFERENCE — a variable `BorrowLowering`
/// recorded as a reference binding (`ref_binders`: a by-reference param it
/// matched on, or a binder such a match bound), or an explicit borrow? Its
/// payloads then bind `&T`, and a boxed-nested pattern's guards and move-outs
/// read through the reference.
pub fn subject_is_borrowed(ctx: &RenderContext, subject: &IrExpr) -> bool {
    match &subject.kind {
        IrExprKind::Var { id } => ctx.ann.ref_binders.contains(id),
        IrExprKind::Borrow { mutable: false, .. } => true,
        _ => false,
    }
}

pub fn render_match_arm(ctx: &RenderContext, arm: &IrMatchArm, match_ty: &almide_lang::types::Ty, subject_ty: &almide_lang::types::Ty, borrowed: bool) -> String {
    // #413/#3176: the arm's patterns are rendered against the SUBJECT's type,
    // so a case name two enums share qualifies with the enum the value has,
    // at the top level and at every nested position the type reaches.
    // #610: a boxed-nested constructor pattern is rewritten to a flat pattern + a
    // `matches!` shape-guard + `let-else` box move-outs in the body. None when the
    // arm has no boxed-nested position (the common case → identical to before).
    let (pattern, shape_guards, box_binds) = match unbox_arm_pattern(ctx, &arm.pattern, Some(subject_ty), borrowed) {
        Some((flat, guards, binds)) => (flat, guards, binds),
        None => (render_pattern_hinted(ctx, &arm.pattern, Some(subject_ty)), Vec::new(), Vec::new()),
    };
    // err() in a match arm where the match type is NOT Result: early return.
    // This handles `let x: T = match ... { none => err("msg") }` in
    // functions returning Result — the err() doesn't contribute a T value,
    // it exits the function with an error.
    let raw_body = if matches!(&arm.body.kind, IrExprKind::ResultErr { .. }) && !match_ty.is_result() {
        format!("return {}", render_expr(ctx, &arm.body))
    } else if matches!(&arm.body.kind, IrExprKind::Continue) {
        // Arm position takes the bare keyword: the `continue_stmt` template is
        // the STATEMENT spelling (`continue;`), whose `;` inside an arm is a
        // parse error (`_ => continue;,` — the guard-let-in-a-loop else,
        // #1543's sibling shape).
        "continue".to_string()
    } else if matches!(&arm.body.kind, IrExprKind::Break) {
        "break".to_string()
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

/// `render_pattern_hinted`'s `Constructor` arm, extracted verbatim.
fn render_pattern_constructor(ctx: &RenderContext, name: &str, args: &[IrPattern], subject: Option<&Ty>) -> String {
    let qualified = qualify_ctor(ctx, name, subject);
    if args.is_empty() {
        qualified
    } else {
        let args_str = args.iter().enumerate()
            .map(|(i, a)| render_pattern_hinted(ctx, a, case_field_ty(ctx, subject, name, &i.to_string()).as_ref()))
            .collect::<Vec<_>>().join(", ");
        format!("{}({})", qualified, args_str)
    }
}

/// `render_pattern_hinted`'s `RecordPattern` arm, extracted verbatim.
fn render_pattern_record(ctx: &RenderContext, name: &str, fields: &[almide_ir::IrFieldPattern], rest: bool, subject: Option<&Ty>) -> String {
    // Qualify enum variant record patterns: Circle → Shape::Circle. A plain
    // record pattern on a runtime-owned struct (`FileStat`, #1821) spells the
    // runtime's reserved name.
    // With no enum subject, a name the program declares as a struct is that
    // struct's pattern, not a same-spelled case of another module (#2636).
    let enum_subject = subject.is_some_and(|t| super::names_enum(ctx, t));
    let struct_pattern = !enum_subject && ctx.ann.record_field_counts.contains_key(name);
    let qualified_name = if let Some(enum_name) = super::ctor_enum_for(ctx, name, subject).filter(|_| !struct_pattern) {
        format!("{}::{}", enum_name, name)
    } else {
        ctx.ann.runtime_owned_types.get(name).cloned().unwrap_or_else(|| name.to_string())
    };
    let fields_str = fields.iter()
        .map(|f| match &f.pattern {
            Some(p) => {
                let fty = case_field_ty(ctx, subject, name, f.name.as_str());
                format!("{}: {}", ctx.field_ident(f.name.as_str()), render_pattern_hinted(ctx, p, fty.as_ref()))
            }
            None => ctx.field_ident(f.name.as_str()),
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

/// Like `render_pattern`, but `subject` is the type of the value the pattern
/// is matched against, when known. A variant pattern qualifies with the enum
/// that value has, which disambiguates a case name shared across packages
/// (#413) — the global `ctor_to_enum` entry keeps only one of them — and the
/// type is carried into every nested position it determines (#3176): an
/// `Option` / `Result` / `List` argument, a tuple element, a case's payload.
pub fn render_pattern_hinted(ctx: &RenderContext, pat: &IrPattern, subject: Option<&Ty>) -> String {
    let at = |i: usize| sub_pattern_ty(subject, i);
    match pat {
        IrPattern::Wildcard => template_or(ctx, "pattern_wildcard", &[], "_"),
        IrPattern::Bind { var, .. } => ctx.var_name(*var).to_string(),
        // As-pattern (#1461): Rust's own `name @ pat` carries it 1:1.
        IrPattern::As { var, inner, .. } => {
            format!("{} @ {}", ctx.var_name(*var), render_pattern_hinted(ctx, inner, subject))
        }
        IrPattern::Literal { expr } => render_pattern_literal(ctx, expr),
        IrPattern::Some { inner } => {
            let binding_s = render_pattern_hinted(ctx, inner, at(0).as_ref());
            ctx.templates.render_with("pattern_some", None, &[], &[("binding", binding_s.as_str())])
                .unwrap_or_else(|| format!("Some(_)"))
        }
        IrPattern::None => template_or(ctx, "pattern_none", &[], "None"),
        IrPattern::Ok { inner } => {
            let binding_s = render_pattern_hinted(ctx, inner, at(0).as_ref());
            ctx.templates.render_with("pattern_ok", None, &[], &[("binding", binding_s.as_str())])
                .unwrap_or_else(|| format!("Ok(_)"))
        }
        IrPattern::Err { inner } => {
            let binding_s = render_pattern_hinted(ctx, inner, at(1).as_ref());
            ctx.templates.render_with("pattern_err", None, &[], &[("binding", binding_s.as_str())])
                .unwrap_or_else(|| format!("Err(_)"))
        }
        IrPattern::Constructor { name, args } => render_pattern_constructor(ctx, name, args, subject),
        IrPattern::Tuple { elements } => {
            let elems = super::helpers::tuple_elems_join(
                &elements.iter().enumerate().map(|(i, e)| render_pattern_hinted(ctx, e, at(i).as_ref())).collect::<Vec<_>>(),
            );
            ctx.templates.render_with("tuple_literal", None, &[], &[("elements", elems.as_str())])
                .unwrap_or_else(|| "tuple(...)".into())
        }
        IrPattern::List { elements, .. } => {
            if elements.is_empty() {
                "[]".to_string()
            } else {
                let elems = elements.iter().map(|e| render_pattern_hinted(ctx, e, at(0).as_ref())).collect::<Vec<_>>().join(", ");
                format!("[{}]", elems)
            }
        }
        IrPattern::RecordPattern { name, fields, rest } => render_pattern_record(ctx, name, fields, *rest, subject),
    }
}
