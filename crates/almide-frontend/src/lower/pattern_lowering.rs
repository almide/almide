// Pattern lowering. `include!`d by statements.rs (the 800-line file
// budget); it shares that module's scope and imports.

// ── Pattern lowering ────────────────────────────────────────────

/// The type argument at `index` of `ty`, when `ty` is that applied constructor
/// with at least `index + 1` arguments; `Ty::Unknown` otherwise.
///
/// The subject type reaching a pattern is not guaranteed to be the constructor
/// the pattern names — a `Some(x)` pattern can be lowered against an `Unknown`
/// subject after an inference failure upstream. `Unknown` is the recovery type
/// codegen already handles, so mismatch is not an error here.
fn applied_arg(ty: &Ty, ctor: TypeConstructorId, index: usize) -> Ty {
    match ty {
        Ty::Applied(id, args) if *id == ctor && args.len() > index => args[index].clone(),
        _ => Ty::Unknown,
    }
}

/// Strip a module qualifier from a constructor name: `command.Move` → `Move`.
///
/// A cross-module variant pattern that keeps its `mod.Ctor` name into the IR
/// defeats field-type resolution and both backends fail to find the variant
/// (#412), so this normalisation is load-bearing, not cosmetic.
fn bare_ctor_name(name: &almide_base::intern::Sym) -> almide_base::intern::Sym {
    name.as_str().rsplit_once('.').map(|(_, b)| sym(b)).unwrap_or(*name)
}

pub(super) fn lower_pattern(ctx: &mut LowerCtx, pat: &ast::Pattern, ty: &Ty) -> IrPattern {
    match pat {
        // #1461: or-pattern arms are expanded into one IR arm per
        // alternative BEFORE pattern lowering (lower_expr_match_arm);
        // the IR pattern language stays or-free by construction.
        ast::Pattern::Or { alts } => match alts.first() {
            Some(first) => lower_pattern(ctx, first, ty),
            None => IrPattern::Wildcard,
        },
        ast::Pattern::Wildcard => IrPattern::Wildcard,
        ast::Pattern::Ident { name } => {
            let var = ctx.define_var(name, ty.clone(), Mutability::Let, None);
            IrPattern::Bind { var, ty: ty.clone() }
        }
        ast::Pattern::As { name, inner } => {
            let var = ctx.define_var(name, ty.clone(), Mutability::Let, None);
            IrPattern::As {
                var,
                ty: ty.clone(),
                inner: Box::new(lower_pattern(ctx, inner, ty)),
            }
        }
        ast::Pattern::Literal { value } => lower_pattern_literal(ctx, value),
        ast::Pattern::Constructor { name, args } => {
            let bare_name = bare_ctor_name(name);
            let payload_tys = get_constructor_payload_tys_from_subject(ctx, &bare_name, ty);
            let ir_args = args.iter().enumerate().map(|(i, a)| {
                let arg_ty = payload_tys.get(i).cloned().unwrap_or(Ty::Unknown);
                lower_pattern(ctx, a, &arg_ty)
            }).collect();
            IrPattern::Constructor { name: ctor_pattern_name(ctx, &bare_name, ty), args: ir_args }
        }
        ast::Pattern::RecordPattern { name, fields, rest } =>
            lower_pattern_record(ctx, name, fields, *rest, ty),
        ast::Pattern::Tuple { elements } => {
            let elem_tys = match ty {
                Ty::Tuple(tys) => tys.clone(),
                _ => vec![Ty::Unknown; elements.len()],
            };
            let ir_elems = elements.iter().enumerate().map(|(i, e)| {
                lower_pattern(ctx, e, elem_tys.get(i).unwrap_or(&Ty::Unknown))
            }).collect();
            IrPattern::Tuple { elements: ir_elems }
        }
        ast::Pattern::Some { inner } => {
            let inner_ty = applied_arg(ty, TypeConstructorId::Option, 0);
            IrPattern::Some { inner: Box::new(lower_pattern(ctx, inner, &inner_ty)) }
        }
        ast::Pattern::None => IrPattern::None,
        ast::Pattern::Ok { inner } => {
            let inner_ty = applied_arg(ty, TypeConstructorId::Result, 0);
            IrPattern::Ok { inner: Box::new(lower_pattern(ctx, inner, &inner_ty)) }
        }
        ast::Pattern::Err { inner } => {
            let inner_ty = applied_arg(ty, TypeConstructorId::Result, 1);
            IrPattern::Err { inner: Box::new(lower_pattern(ctx, inner, &inner_ty)) }
        }
        ast::Pattern::List { elements, rest } => {
            let elem_ty = applied_arg(ty, TypeConstructorId::List, 0);
            let ir_elems = elements.iter().map(|e| lower_pattern(ctx, e, &elem_ty)).collect();
            // #1461 list-rest: a NAMED tail binds with the subject's own
            // list type; `[a, ..]` keeps the >=-length semantics with a
            // Wildcard rest.
            let ir_rest = rest.as_ref().map(|r| {
                Box::new(match r {
                    Some(name) => {
                        let var = ctx.define_var(name, ty.clone(), Mutability::Let, None);
                        IrPattern::Bind { var, ty: ty.clone() }
                    }
                    None => IrPattern::Wildcard,
                })
            });
            IrPattern::List { elements: ir_elems, rest: ir_rest }
        }
    }
}

/// Lower a literal pattern.
///
/// Pattern literals may have no `expr_types` entry — they are patterns, not
/// expressions — so the four scalar forms build their IR directly with a known
/// type instead of asking the `TypeMap`. Anything else is an expression that
/// happens to appear in pattern position and does go through `lower_expr`.
fn lower_pattern_literal(ctx: &mut LowerCtx, value: &ast::Expr) -> IrPattern {
    let Some((kind, ty)) = scalar_pattern_literal(value) else {
        return IrPattern::Literal { expr: lower_expr(ctx, value) };
    };
    IrPattern::Literal { expr: ctx.mk(kind, ty, value.span) }
}

/// The scalar forms a literal pattern can take, folded to IR with their type
/// known outright. `None` means "not a scalar literal" — an expression that
/// happens to sit in pattern position, which does go through `lower_expr`.
///
/// A NEGATIVE literal is `Unary { op: "-", .. }`, not an `Int`/`Float` node, so
/// it used to take the `lower_expr` path — where the TypeMap has no entry for a
/// pattern expression and the literal came out `ty=Unknown`, failing the
/// pre-codegen resolution check with `unresolved LitInt` (#897). Folding the
/// sign in here keeps it on the known-type path with the other scalars.
fn scalar_pattern_literal(value: &ast::Expr) -> Option<(IrExprKind, Ty)> {
    match &value.kind {
        ast::ExprKind::Int { raw, .. } =>
            Some((IrExprKind::LitInt { value: crate::literals::int_value(raw) }, Ty::Int)),
        ast::ExprKind::Float { value: v, .. } => Some((IrExprKind::LitFloat { value: *v }, Ty::Float)),
        ast::ExprKind::String { value: v, .. } => Some((IrExprKind::LitStr { value: v.clone() }, Ty::String)),
        ast::ExprKind::Bool { value: v, .. } => Some((IrExprKind::LitBool { value: *v }, Ty::Bool)),
        ast::ExprKind::Paren { expr } => scalar_pattern_literal(expr),
        ast::ExprKind::Unary { op, operand } if op.as_str() == "-" => match scalar_pattern_literal(operand)? {
            (IrExprKind::LitInt { value: v }, ty) => Some((IrExprKind::LitInt { value: -v }, ty)),
            (IrExprKind::LitFloat { value: v }, ty) => Some((IrExprKind::LitFloat { value: -v }, ty)),
            _ => None,
        },
        _ => None,
    }
}

/// Lower a record-variant pattern.
///
/// A shorthand field (`Move { x }`, no sub-pattern) both matches and binds, so
/// it needs a `Bind` pattern synthesised for it after the explicit sub-patterns
/// are lowered — the two passes cannot merge, because defining the variable
/// borrows `ctx` mutably while the first pass is still iterating.
fn lower_pattern_record(
    ctx: &mut LowerCtx,
    name: &almide_base::intern::Sym,
    fields: &[ast::FieldPattern],
    rest: bool,
    subject_ty: &Ty,
) -> IrPattern {
    let pat_name = struct_pattern_name(ctx, name);
    let field_ty_of = |ctx: &LowerCtx, field: &str| {
        record_case_field_ty_from_subject(ctx, name, field, subject_ty)
            .unwrap_or_else(|| resolve_record_field_ty(ctx, &pat_name, field))
    };
    let mut ir_fields: Vec<IrFieldPattern> = fields.iter().map(|f| {
        let field_ty = field_ty_of(ctx, &f.name);
        IrFieldPattern {
            name: f.name.to_string(),
            pattern: f.pattern.as_ref().map(|p| lower_pattern(ctx, p, &field_ty)),
        }
    }).collect();
    for (i, f) in fields.iter().enumerate() {
        if f.pattern.is_none() {
            let field_ty = field_ty_of(ctx, &f.name);
            let var = ctx.define_var(&f.name, field_ty.clone(), Mutability::Let, None);
            ir_fields[i].pattern = Some(IrPattern::Bind { var, ty: field_ty });
        }
    }
    IrPattern::RecordPattern { name: pat_name.to_string(), fields: ir_fields, rest }
}

/// The name a record pattern carries into the IR. A STRUCT pattern is pinned
/// to its qualified canonical name — `m.Cfg` inside module `m`, `self.Value`
/// for the entry program's shadow of a stdlib-owned name (#1828) — exactly
/// as the struct LITERAL is (`lower/expressions_access.rs`, #433), so the
/// native walker names the mangled struct instead of a bare spelling the
/// flat program no longer has (`Cfg { a, .. }` against `almide_rt_m_Cfg` was
/// rustc E0308 while the wasm leg ran — a divergence of this landing's
/// family). A record-VARIANT case keeps the bare case name: the ctor table is
/// keyed by it and the subject's enum qualifies it (#412).
fn struct_pattern_name(ctx: &LowerCtx, written: &almide_base::intern::Sym) -> almide_base::intern::Sym {
    let cur_mod = ctx.current_module.map(|m| m.as_str());
    match crate::canonicalize::resolve::canonical_user_type_sym(written.as_str(), &ctx.env.types, cur_mod) {
        Some(key) if matches!(ctx.env.types.get(&key), Some(Ty::Record { .. })) => key,
        _ => bare_ctor_name(written),
    }
}

/// The name a constructor pattern carries into the IR. An opaque newtype's
/// pattern (`Value(s)` against a subject of that newtype) is pinned to the
/// newtype's IDENTITY — `self.Value`, `m.Token` — the one spelling its ctor
/// call and its type decl carry (#1835), so the native flatten mangle and
/// the wasm newtype erasure treat the three as one name. A variant case
/// keeps its bare case name: the ctor table is keyed by it and the
/// subject's enum qualifies it (#412).
fn ctor_pattern_name(ctx: &LowerCtx, bare_name: &almide_base::intern::Sym, subject_ty: &Ty) -> String {
    match ctx.env.resolve_named(subject_ty) {
        Ty::Named(t, _)
            if ctx.env.opaque_alias_targets.contains_key(&t)
                && almide_ir::declared_type_name(t.as_str()) == bare_name.as_str() =>
        {
            t.to_string()
        }
        _ => bare_name.to_string(),
    }
}

/// Extract constructor payload types from the subject type first (instantiated types),
/// falling back to the constructor registry (template types) if the subject type doesn't match.
fn get_constructor_payload_tys_from_subject(ctx: &LowerCtx, ctor_name: &str, subject_ty: &Ty) -> Vec<Ty> {
    // Try to extract from the subject type (has instantiated generics)
    let resolved = ctx.env.resolve_named(subject_ty);
    if let Ty::Variant { cases, .. } = &resolved {
        if let Some(case) = cases.iter().find(|c| c.name == ctor_name) {
            return match &case.payload {
                crate::types::VariantPayload::Tuple(tys) => tys.clone(),
                crate::types::VariantPayload::Record(fs) => fs.iter().map(|(_, t)| t.clone()).collect(),
                crate::types::VariantPayload::Unit => vec![],
            };
        }
    }
    // Fallback: constructor registry (may have uninstantiated generic types).
    // Owned-first (#1426): mirror the checker's candidate choice.
    if let Some((_, case)) = ctx.env.lookup_ctor_in(&sym(ctor_name), ctx.current_module.map(|s| s.as_str())) {
        match &case.payload {
            crate::types::VariantPayload::Tuple(tys) => tys.clone(),
            crate::types::VariantPayload::Record(fs) => fs.iter().map(|(_, t)| t.clone()).collect(),
            crate::types::VariantPayload::Unit => vec![],
        }
    } else if let Ty::Named(tname, _) = subject_ty {
        // Opaque alias destructure: SafeHtml(s) → inner target type
        if let Some(target) = ctx.env.opaque_alias_targets.get(tname) {
            vec![target.clone()]
        } else {
            vec![]
        }
    } else {
        vec![]
    }
}

/// A record-case field's type read off the match SUBJECT, whose generics are
/// instantiated (`Box[String]`'s `v` is `String`, not the declaration's `T`),
/// the way `get_constructor_payload_tys_from_subject` reads a tuple case's.
/// `None` when the subject is not a variant with a record case of that name.
fn record_case_field_ty_from_subject(ctx: &LowerCtx, written: &almide_base::intern::Sym, field: &str, subject_ty: &Ty) -> Option<Ty> {
    let case_name = bare_ctor_name(written);
    let Ty::Variant { cases, .. } = ctx.env.resolve_named(subject_ty) else { return None };
    let case = cases.into_iter().find(|c| c.name == case_name)?;
    let crate::types::VariantPayload::Record(fs) = case.payload else { return None };
    fs.into_iter().find(|(n, _)| n == field).map(|(_, t)| t)
}

fn resolve_record_field_ty(ctx: &LowerCtx, record_name: &str, field_name: &str) -> Ty {
    // The type keyed by the pattern's name answers only when it is not a
    // VARIANT: in `type Opt = | Opt { n: Int } | Empty` the key `Opt` is the
    // variant itself, which has no field `n`, so the binder lowered as
    // `Unknown` and both wasm legs refused the match while native let rustc
    // infer it (#2859). The case of that name is the ctor lookup below.
    let type_def = ctx.env.types.get(&sym(record_name)).filter(|td| !matches!(td, Ty::Variant { .. }));
    if let Some(type_def) = type_def {
        ctx.resolve_field_ty(type_def, field_name)
    } else if let Some((_, case)) = ctx.env.lookup_ctor_in(&sym(record_name), ctx.current_module.map(|s| s.as_str())) {
        if let crate::types::VariantPayload::Record(fs) = &case.payload {
            fs.iter().find(|(n, _)| n == field_name).map(|(_, t)| t.clone()).unwrap_or(Ty::Unknown)
        } else { Ty::Unknown }
    } else { Ty::Unknown }
}
