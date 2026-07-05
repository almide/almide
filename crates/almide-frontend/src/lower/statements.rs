// ── Statement lowering ──────────────────────────────────────────

use almide_lang::ast;
use almide_base::intern::sym;
use almide_ir::*;
use crate::types::{Ty, TypeConstructorId, TypeEnv};
use super::LowerCtx;
use super::expressions::lower_expr;

pub(super) fn lower_stmt(ctx: &mut LowerCtx, stmt: &ast::Stmt) -> IrStmt {
    let span = match stmt {
        ast::Stmt::Let { span, .. } | ast::Stmt::Var { span, .. }
        | ast::Stmt::Assign { span, .. } | ast::Stmt::Guard { span, .. }
        | ast::Stmt::GuardLet { span, .. }
        | ast::Stmt::Expr { span, .. } | ast::Stmt::IndexAssign { span, .. }
        | ast::Stmt::FieldAssign { span, .. } | ast::Stmt::LetDestructure { span, .. }
        | ast::Stmt::Error { span, .. } => *span,
        ast::Stmt::Comment { .. } => None,
    };

    let kind = match stmt {
        ast::Stmt::Let { name, ty, value, .. } => {
            let mut ir_val = lower_expr(ctx, value);
            // If the user wrote `let x: T = ...`, honor that annotation over the
            // structurally-inferred type of the value. Otherwise two nominal
            // record types with identical fields (e.g. `Dog` and `Cat`) collide
            // at codegen time because the value keeps its structural type.
            let val_ty = if let Some(te) = ty {
                let declared = crate::canonicalize::resolve::resolve_type_expr_in(te, Some(&ctx.env.types), ctx.current_module.as_ref().map(|s| s.as_str()));
                override_record_literal_ty(&mut ir_val, &declared, ctx.env);
                declared
            } else {
                ir_val.ty.clone()
            };
            let var = ctx.define_var(name, val_ty.clone(), Mutability::Let, span);
            // #485: an EXPLICIT `Result[..]` annotation is the only signal
            // that this binding keeps the Result (auto_try must not insert
            // `?`). Un-annotated binds share the same Bind.ty shape when the
            // callee itself declares `-> Result[..]`, so record the VarId.
            if ty.is_some() && val_ty.is_result() {
                ctx.annotated_result_vars.insert(var);
            }
            IrStmtKind::Bind { var, mutability: Mutability::Let, ty: val_ty, value: ir_val }
        }
        ast::Stmt::Var { name, ty, value, .. } => {
            let mut ir_val = lower_expr(ctx, value);
            let val_ty = if let Some(te) = ty {
                let declared = crate::canonicalize::resolve::resolve_type_expr_in(te, Some(&ctx.env.types), ctx.current_module.as_ref().map(|s| s.as_str()));
                override_record_literal_ty(&mut ir_val, &declared, ctx.env);
                declared
            } else {
                ir_val.ty.clone()
            };
            let var = ctx.define_var(name, val_ty.clone(), Mutability::Var, span);
            if ty.is_some() && val_ty.is_result() {
                ctx.annotated_result_vars.insert(var);
            }
            IrStmtKind::Bind { var, mutability: Mutability::Var, ty: val_ty, value: ir_val }
        }
        ast::Stmt::LetDestructure { pattern, value, .. } => {
            let ir_val = lower_expr(ctx, value);
            let ir_pat = lower_pattern(ctx, pattern, &ir_val.ty);
            IrStmtKind::BindDestructure { pattern: ir_pat, value: ir_val }
        }
        ast::Stmt::Assign { name, value, .. } => {
            let ir_val = lower_expr(ctx, value);
            let var = ctx.lookup_var(name).unwrap_or(VarId(0));
            IrStmtKind::Assign { var, value: ir_val }
        }
        ast::Stmt::IndexAssign { target, index, value, .. } => {
            let var = ctx.lookup_var(target).unwrap_or(VarId(0));
            let ir_idx = lower_expr(ctx, index);
            let ir_val = lower_expr(ctx, value);
            let var_ty = &ctx.var_table.get(var).ty;
            if var_ty.is_map() {
                IrStmtKind::MapInsert { target: var, key: ir_idx, value: ir_val }
            } else {
                IrStmtKind::IndexAssign { target: var, index: ir_idx, value: ir_val }
            }
        }
        ast::Stmt::FieldAssign { target, field, value, .. } => {
            let ir_val = lower_expr(ctx, value);
            match ctx.lookup_var(target) {
                Some(var) => IrStmtKind::FieldAssign { target: var, field: *field, value: ir_val },
                None => {
                    // `m.x = v` where `m` is a MODULE alias, not a local: an
                    // assignment to a cross-module top-let. Resolve through
                    // the same rule the read path uses (one rule, one place)
                    // — the old VarId(0) fallback rendered garbage like
                    // `NUMS.nums = …` (rustc E0425, #505).
                    let ty = ir_val.ty.clone();
                    if let Some((var, _)) = crate::lower::expressions::module_top_let_var(
                        ctx, sym(target), *field, &ty,
                    ) {
                        IrStmtKind::Assign { var, value: ir_val }
                    } else {
                        IrStmtKind::FieldAssign { target: VarId(0), field: *field, value: ir_val }
                    }
                }
            }
        }
        ast::Stmt::Guard { cond, else_, .. } => {
            let ir_cond = lower_expr(ctx, cond);
            let ir_else = lower_expr(ctx, else_);
            IrStmtKind::Guard { cond: ir_cond, else_: ir_else }
        }
        // `guard let` binds for the REST of the block, so the enclosing block lowering
        // (lower_block_stmts) restructures it into a match — it never reaches here.
        ast::Stmt::GuardLet { .. } => {
            unreachable!("guard let is desugared by the enclosing block, not lower_stmt")
        }
        ast::Stmt::Expr { expr, .. } => {
            let ir_expr = lower_expr(ctx, expr);
            IrStmtKind::Expr { expr: ir_expr }
        }
        ast::Stmt::Comment { text } => IrStmtKind::Comment { text: text.clone() },
        ast::Stmt::Error { .. } => IrStmtKind::Comment { text: "/* error */".to_string() },
    };

    IrStmt { kind, span }
}

/// Retag an anonymous record literal's IR type with the declared nominal type.
///
/// Record literals are inferred as structural `Ty::Record { fields }`. When
/// assigned to a let with an explicit nominal annotation (e.g. `let d: Dog`),
/// the declared type should win. Otherwise multiple nominal types with
/// identical field shapes (Dog vs Cat, both `{name: String}`) collide at
/// codegen because `collect_named_records` keys by sorted field names.
fn override_record_literal_ty(ir_val: &mut IrExpr, declared: &Ty, env: &TypeEnv) {
    // Nominal record type override — keeps `Dog` / `Cat` distinct even
    // when their structural shapes match.
    if matches!(declared, Ty::Named(_, _)) {
        match &mut ir_val.kind {
            IrExprKind::Record { .. } => {
                if matches!(ir_val.ty, Ty::Record { .. } | Ty::OpenRecord { .. } | Ty::Unknown) {
                    ir_val.ty = declared.clone();
                }
            }
            IrExprKind::Block { expr: Some(inner), .. } => {
                override_record_literal_ty(inner, declared, env);
                if matches!(ir_val.ty, Ty::Record { .. } | Ty::OpenRecord { .. } | Ty::Unknown) {
                    ir_val.ty = declared.clone();
                }
            }
            _ => {}
        }
        // A named record/alias whose structural shape carries sized fields
        // (`type Rec = { b: Int8, n: Int }`) still needs its bare-literal
        // field values narrowed to the sized field types. The nominal retag
        // above only fixes the record's *own* type tag; `coerce_literal_to_sized`
        // resolves the Named type and descends into the field literals.
        coerce_literal_to_sized(ir_val, declared, env);
        return;
    }

    // Sized numeric literal coercion (Stage 1b). When the binding is
    // annotated with a sized integer / float type (`Int32`, `UInt8`,
    // `Float32`, ...) and the value is a bare Int/Float literal whose
    // inferred type is the default `Ty::Int` / `Ty::Float`, rewrite
    // the literal's IR type to the annotation. Codegen reads
    // `expr.ty` for the literal suffix (`42i64` → `42i32`), so this
    // is the single hook that makes `let x: Int32 = 42` emit correct
    // Rust instead of an `i64` / `i32` mismatch.
    coerce_literal_to_sized(ir_val, declared, env);
}

/// Whether `ty` is one of the sized numeric types the Stage 1a/1b
/// literal coercion rule should retype bare literals into.
pub(crate) fn is_sized_numeric(ty: &Ty) -> bool {
    matches!(
        ty,
        Ty::Int8 | Ty::Int16 | Ty::Int32
            | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
            | Ty::Float32
    )
}

/// Retype a bare Int / Float literal IR node to the sized numeric
/// `declared` type, so codegen emits the right Rust suffix
/// (`42i32` / `3.14f32` / ...). Called from `override_record_literal_ty`
/// (let / var bindings) and `coerce_call_arg_to_sized_param` (fn call
/// sites). No-op when the value isn't a literal of compatible default
/// type — which matches the Stage 1b rule that literals flow into
/// sized slots but named-variable refs don't (they retype instead
/// with an explicit conversion).
///
/// Recurses through container literals so a sized field nested inside a
/// list / tuple / record annotation also coerces: `let a: List[(Int8,
/// Int)] = [(1, 100)]` retypes the `1` element to `Int8` while the type
/// checker only narrowed the *binding* type (the inner literal keeps the
/// default `Ty::Int` in `expr_types`, so codegen would otherwise emit
/// `1i64` against a `Vec<(i8, i64)>` slot — an E0308). The declared type
/// drives the descent; the value literal's own shape must match for any
/// element to be touched.
pub(crate) fn coerce_literal_to_sized(ir_val: &mut IrExpr, declared: &Ty, env: &TypeEnv) {
    use almide_lang::types::constructor::TypeConstructorId;
    // Look through blocks/parenthesized tails: a literal can be wrapped in
    // a single-tail block (e.g. `{ (1, 2) }`) by lowering.
    if let IrExprKind::Block { expr: Some(tail), .. } = &mut ir_val.kind {
        coerce_literal_to_sized(tail, declared, env);
        return;
    }
    // Resolve a named type alias to its structural form so a record / sized
    // alias declared via `type Rec = { b: Int8, .. }` (a `Ty::Named`) becomes
    // its `Ty::Record { .. }` / `Ty::Int8` / etc. before the match below.
    // `resolve_named` is a no-op for non-Named types, so the scalar / List /
    // Tuple arms are unaffected.
    let resolved = env.resolve_named(declared);
    let declared = &resolved;
    match declared {
        // Scalar sized numeric slot: retype a bare default-typed literal.
        _ if is_sized_numeric(declared) => match &mut ir_val.kind {
            IrExprKind::LitInt { .. } if ir_val.ty == Ty::Int => {
                ir_val.ty = declared.clone();
            }
            IrExprKind::LitFloat { .. } if ir_val.ty == Ty::Float => {
                ir_val.ty = declared.clone();
            }
            IrExprKind::UnOp { op: almide_ir::UnOp::NegInt, operand } => {
                if matches!(&operand.kind, IrExprKind::LitInt { .. }) && operand.ty == Ty::Int {
                    operand.ty = declared.clone();
                    ir_val.ty = declared.clone();
                }
            }
            _ => {}
        },
        // List[T]: every element literal is coerced against T.
        Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => {
            if let IrExprKind::List { elements } = &mut ir_val.kind {
                for e in elements.iter_mut() {
                    coerce_literal_to_sized(e, &args[0], env);
                }
            }
        }
        // Tuple([t0, t1, ...]): element i is coerced against t_i.
        Ty::Tuple(elem_tys) => {
            if let IrExprKind::Tuple { elements } = &mut ir_val.kind {
                if elements.len() == elem_tys.len() {
                    for (e, t) in elements.iter_mut().zip(elem_tys.iter()) {
                        coerce_literal_to_sized(e, t, env);
                    }
                }
            }
        }
        // Structural record annotation `{ b: Int8, n: Int }`: coerce each
        // field value against its declared field type, matched by name.
        Ty::Record { fields: decl_fields } | Ty::OpenRecord { fields: decl_fields } => {
            if let IrExprKind::Record { fields, .. } = &mut ir_val.kind {
                for (fname, fvalue) in fields.iter_mut() {
                    if let Some((_, fty)) = decl_fields.iter().find(|(n, _)| n == fname) {
                        coerce_literal_to_sized(fvalue, fty, env);
                    }
                }
            }
        }
        _ => {}
    }
}

/// Resolve the declared field types of a named record construction
/// (`Name { ... }`) into a structural `Ty::Record`, so the construction
/// site can narrow bare-literal field values to their sized field types
/// (`coerce_literal_to_sized`). `name` may be either:
///   - a record TYPE name (`type Rec = { a: Int8 }`) — looked up in
///     `env.types` and resolved to its `Ty::Record` shape, or
///   - a record-bearing VARIANT case (`Scroll { dy: Int8 }`) — found in
///     `env.constructors`, whose `VariantPayload::Record` carries the fields.
/// Returns `None` for anonymous records, tuple/unit cases, or unknown names
/// (nothing to coerce against).
pub(crate) fn declared_record_ty(env: &TypeEnv, name: almide_base::intern::Sym) -> Option<Ty> {
    // Variant case with a record payload takes priority: a case name and a
    // type name never collide (constructors are registered separately), but
    // checking constructors first matches the checker's resolution order.
    if let Some((_, case)) = env.lookup_ctor(&name) {
        if let crate::types::VariantPayload::Record(fields) = &case.payload {
            return Some(Ty::Record { fields: fields.clone() });
        }
        return None;
    }
    // Record type name: resolve the alias to its structural record form.
    if let Some(ty) = env.types.get(&name) {
        let resolved = env.resolve_named(ty);
        if matches!(resolved, Ty::Record { .. } | Ty::OpenRecord { .. }) {
            return Some(resolved);
        }
    }
    None
}

// ── Pattern lowering ────────────────────────────────────────────

pub(super) fn lower_pattern(ctx: &mut LowerCtx, pat: &ast::Pattern, ty: &Ty) -> IrPattern {
    match pat {
        ast::Pattern::Wildcard => IrPattern::Wildcard,
        ast::Pattern::Ident { name } => {
            let var = ctx.define_var(name, ty.clone(), Mutability::Let, None);
            IrPattern::Bind { var, ty: ty.clone() }
        }
        ast::Pattern::Literal { value } => {
            // Pattern literals may not have expr_types entries (they're patterns,
            // not expressions), so construct IR directly without calling lower_expr.
            let (kind, ty) = match &value.kind {
                ast::ExprKind::Int { raw, .. } => {
                    let clean = raw.replace('_', "");
                    let v = if clean.starts_with("0x") || clean.starts_with("0X") {
                        i64::from_str_radix(&clean[2..], 16).unwrap_or(0)
                    } else if clean.starts_with("0b") || clean.starts_with("0B") {
                        i64::from_str_radix(&clean[2..], 2).unwrap_or(0)
                    } else if clean.starts_with("0o") || clean.starts_with("0O") {
                        i64::from_str_radix(&clean[2..], 8).unwrap_or(0)
                    } else {
                        clean.parse::<i64>().unwrap_or(0)
                    };
                    (IrExprKind::LitInt { value: v }, Ty::Int)
                }
                ast::ExprKind::Float { value: v, .. } => (IrExprKind::LitFloat { value: *v }, Ty::Float),
                ast::ExprKind::String { value: v, .. } => (IrExprKind::LitStr { value: v.clone() }, Ty::String),
                ast::ExprKind::Bool { value: v, .. } => (IrExprKind::LitBool { value: *v }, Ty::Bool),
                _ => {
                    let ir_expr = lower_expr(ctx, value);
                    return IrPattern::Literal { expr: ir_expr };
                }
            };
            let ir_expr = ctx.mk(kind, ty, value.span);
            IrPattern::Literal { expr: ir_expr }
        }
        ast::Pattern::Constructor { name, args } => {
            // Normalize module-qualified names: "binary.Unreachable" → "Unreachable"
            let bare_name = name.as_str().rsplit_once('.').map(|(_, b)| sym(b)).unwrap_or(*name);
            let payload_tys = get_constructor_payload_tys_from_subject(ctx, &bare_name, ty);
            let ir_args = args.iter().enumerate().map(|(i, a)| {
                let arg_ty = payload_tys.get(i).cloned().unwrap_or(Ty::Unknown);
                lower_pattern(ctx, a, &arg_ty)
            }).collect();
            IrPattern::Constructor { name: bare_name.to_string(), args: ir_args }
        }
        ast::Pattern::RecordPattern { name, fields, rest } => {
            // Normalize module-qualified names: "command.Move" → "Move" (mirrors the
            // Constructor arm above). Without this a cross-module record-variant
            // pattern keeps its `mod.Ctor` name into the IR, so field-type resolution
            // and both backends fail to find the variant (#412).
            let bare_name = name.as_str().rsplit_once('.').map(|(_, b)| sym(b)).unwrap_or(*name);
            let ir_fields: Vec<IrFieldPattern> = fields.iter().map(|f| {
                let field_ty = resolve_record_field_ty(ctx, &bare_name, &f.name);
                IrFieldPattern {
                    name: f.name.to_string(),
                    pattern: f.pattern.as_ref().map(|p| lower_pattern(ctx, p, &field_ty)),
                }
            }).collect();
            // Bind shorthand fields as variables and attach Bind pattern
            let mut ir_fields = ir_fields;
            for (i, f) in fields.iter().enumerate() {
                if f.pattern.is_none() {
                    let field_ty = resolve_record_field_ty(ctx, &bare_name, &f.name);
                    let var = ctx.define_var(&f.name, field_ty.clone(), Mutability::Let, None);
                    ir_fields[i].pattern = Some(IrPattern::Bind { var, ty: field_ty });
                }
            }
            IrPattern::RecordPattern { name: bare_name.to_string(), fields: ir_fields, rest: *rest }
        }
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
            let inner_ty = match ty { Ty::Applied(TypeConstructorId::Option, args) if args.len() == 1 => args[0].clone(), _ => Ty::Unknown };
            IrPattern::Some { inner: Box::new(lower_pattern(ctx, inner, &inner_ty)) }
        }
        ast::Pattern::None => IrPattern::None,
        ast::Pattern::Ok { inner } => {
            let inner_ty = match ty { Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[0].clone(), _ => Ty::Unknown };
            IrPattern::Ok { inner: Box::new(lower_pattern(ctx, inner, &inner_ty)) }
        }
        ast::Pattern::Err { inner } => {
            let inner_ty = match ty { Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[1].clone(), _ => Ty::Unknown };
            IrPattern::Err { inner: Box::new(lower_pattern(ctx, inner, &inner_ty)) }
        }
        ast::Pattern::List { elements } => {
            let elem_ty = match ty {
                Ty::Applied(TypeConstructorId::List, args) if !args.is_empty() => args[0].clone(),
                _ => Ty::Unknown,
            };
            let ir_elems = elements.iter().map(|e| lower_pattern(ctx, e, &elem_ty)).collect();
            IrPattern::List { elements: ir_elems }
        }
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
    // Fallback: constructor registry (may have uninstantiated generic types)
    if let Some((_, case)) = ctx.env.lookup_ctor(&sym(ctor_name)) {
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

fn resolve_record_field_ty(ctx: &LowerCtx, record_name: &str, field_name: &str) -> Ty {
    if let Some(type_def) = ctx.env.types.get(&sym(record_name)) {
        ctx.resolve_field_ty(type_def, field_name)
    } else if let Some((_, case)) = ctx.env.lookup_ctor(&sym(record_name)) {
        if let crate::types::VariantPayload::Record(fs) = &case.payload {
            fs.iter().find(|(n, _)| n == field_name).map(|(_, t)| t.clone()).unwrap_or(Ty::Unknown)
        } else { Ty::Unknown }
    } else { Ty::Unknown }
}
