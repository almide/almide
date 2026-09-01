// ── Statement lowering ──────────────────────────────────────────

use almide_lang::ast;
use almide_base::intern::sym;
use almide_ir::*;
use crate::types::{Ty, TypeConstructorId, TypeEnv};
use super::LowerCtx;
use super::expressions::lower_expr;

pub(super) fn lower_stmt(ctx: &mut LowerCtx, stmt: &ast::Stmt) -> IrStmt {
    let span = stmt_span(stmt);
    let kind = match stmt {
        ast::Stmt::Let { name, ty, value, .. } =>
            lower_bind(ctx, name, ty.as_ref(), value, Mutability::Let, span),
        ast::Stmt::Var { name, ty, value, .. } =>
            lower_bind(ctx, name, ty.as_ref(), value, Mutability::Var, span),
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

/// The source span of a statement.
///
/// A comment has no span of its own: it is attached to whatever follows it, so
/// pointing a diagnostic at the comment would point away from the code.
fn stmt_span(stmt: &ast::Stmt) -> Option<ast::Span> {
    match stmt {
        ast::Stmt::Let { span, .. } | ast::Stmt::Var { span, .. }
        | ast::Stmt::Assign { span, .. } | ast::Stmt::Guard { span, .. }
        | ast::Stmt::GuardLet { span, .. }
        | ast::Stmt::Expr { span, .. } | ast::Stmt::IndexAssign { span, .. }
        | ast::Stmt::FieldAssign { span, .. } | ast::Stmt::LetDestructure { span, .. }
        | ast::Stmt::Error { span, .. } => *span,
        ast::Stmt::Comment { .. } => None,
    }
}

/// Lower a `let` or `var` binding.
///
/// The two differ only in mutability, and the difference was previously two
/// copies of this body — one of which had lost the explanatory comments. One
/// body, `mutability` as the parameter.
fn lower_bind(
    ctx: &mut LowerCtx,
    name: &str,
    ty: Option<&ast::TypeExpr>,
    value: &ast::Expr,
    mutability: Mutability,
    span: Option<ast::Span>,
) -> IrStmtKind {
    let mut ir_val = lower_expr(ctx, value);
    // An explicit `let x: T = ...` annotation wins over the structurally
    // inferred type of the value. Otherwise two nominal record types with
    // identical fields (`Dog` and `Cat`, both `{ name: String }`) collide at
    // codegen, because the value keeps its structural type and
    // `collect_named_records` keys by sorted field names.
    let val_ty = match ty {
        Some(te) => {
            let declared = crate::canonicalize::resolve::resolve_type_expr_in(
                te, Some(&ctx.env.types), ctx.current_module.as_ref().map(|s| s.as_str()));
            override_record_literal_ty(&mut ir_val, &declared, ctx.env);
            ir_val = super::expressions::adapt_fn_value_to_effect_slot(ctx, ir_val, &declared);
            declared
        }
        None => ir_val.ty.clone(),
    };
    let var = ctx.define_var(name, val_ty.clone(), mutability, span);
    // #485: an EXPLICIT `Result[..]` annotation is the only signal that this
    // binding keeps the Result (auto_try must not insert `?`). Un-annotated
    // binds share the same `Bind.ty` shape when the callee itself declares
    // `-> Result[..]`, so the distinction has to be recorded per VarId.
    // ADR-0008 D2 (#1123 N+1): `let _ = f()` is the sanctioned DISCARD — the
    // Result binds dead, nothing propagates, so it keeps the Result too.
    if (ty.is_some() || name == "_") && val_ty.is_result() {
        ctx.annotated_result_vars.insert(var);
    }
    IrStmtKind::Bind { var, mutability, ty: val_ty, value: ir_val }
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
    // #880: an `if` / `match` carries no literal of its own — its ARMS are the
    // peers that do. The slot's width belongs to each arm the same way it
    // belongs to a block tail, so descend and let every arm coerce on its own
    // (`let v: UInt8 = if b then 1 else u8v` emitted `if … { 1i64 } else { 3u8 }`
    // into a `u8` binding — one arm retyped by the peer join, the other not).
    match &mut ir_val.kind {
        IrExprKind::If { then, else_, .. } => {
            coerce_literal_to_sized(then, declared, env);
            coerce_literal_to_sized(else_, declared, env);
            return;
        }
        IrExprKind::Match { arms, .. } => {
            for arm in arms.iter_mut() {
                coerce_literal_to_sized(&mut arm.body, declared, env);
            }
            return;
        }
        _ => {}
    }
    // Resolve a named type alias to its structural form so a record / sized
    // alias declared via `type Rec = { b: Int8, .. }` (a `Ty::Named`) becomes
    // its `Ty::Record { .. }` / `Ty::Int8` / etc. before the match below.
    // `resolve_named` is a no-op for non-Named types, so the scalar / List /
    // Tuple arms are unaffected.
    let resolved = env.resolve_named(declared);
    let declared = &resolved;
    if is_sized_numeric(declared) {
        retype_scalar_literal(ir_val, declared);
        return;
    }
    match declared {
        // List[T]: every element literal is coerced against T.
        Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => {
            if let IrExprKind::List { elements } = &mut ir_val.kind {
                for e in elements.iter_mut() {
                    coerce_literal_to_sized(e, &args[0], env);
                }
            }
        }
        // Tuple([t0, t1, ...]): element i is coerced against t_i.
        Ty::Tuple(elem_tys) => coerce_tuple_elements(ir_val, elem_tys, env),
        // Structural record annotation `{ b: Int8, n: Int }`: coerce each
        // field value against its declared field type, matched by name.
        Ty::Record { fields: decl_fields } | Ty::OpenRecord { fields: decl_fields } =>
            coerce_record_fields(ir_val, decl_fields, env),
        _ => {}
    }
}

/// Retype a bare default-typed literal to a sized numeric slot.
///
/// Only a LITERAL-ONLY expression is retyped. A tree with a non-literal leaf
/// already has whatever width its own operands gave it, and silently widening or
/// narrowing that would change the arithmetic rather than just record the
/// annotation — that half is the checker's to reject (#880).
///
/// "Literal-only" reaches through negation and int arithmetic, not just a bare
/// literal: `let b: Int32 = 5 - 3` has no operand that could have supplied a
/// width, so every node in it is still the default `Int` and the emitted value
/// landed in the `i32` slot as `2i64` — invalid Rust that `check` accepted
/// (#895 follow-on, the fourth shape of #899). The negation case matters for the
/// same reason: the parser produces `-(1)`, not a signed literal, so both the
/// operand and the negation node have to carry the sized type or codegen emits a
/// width mismatch between them.
fn retype_scalar_literal(ir_val: &mut IrExpr, declared: &Ty) {
    let default = match declared {
        Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64 | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64 => Ty::Int,
        Ty::Float32 | Ty::Float64 => Ty::Float,
        _ => return,
    };
    if is_default_numeric_tree(ir_val, &default) {
        stamp_numeric_tree(ir_val, declared);
    }
}

/// Whether every node of this expression is still the `default` numeric type
/// and every leaf is a literal of it — i.e. nothing in the tree chose a width.
fn is_default_numeric_tree(e: &IrExpr, default: &Ty) -> bool {
    if e.ty != *default {
        return false;
    }
    let (neg_op, ops): (almide_ir::UnOp, &[BinOp]) = match (default, &e.kind) {
        (Ty::Int, IrExprKind::LitInt { .. }) | (Ty::Float, IrExprKind::LitFloat { .. }) => return true,
        (Ty::Int, _) => (
            almide_ir::UnOp::NegInt,
            &[BinOp::AddInt, BinOp::SubInt, BinOp::MulInt, BinOp::DivInt, BinOp::ModInt, BinOp::PowInt],
        ),
        (Ty::Float, _) => (
            almide_ir::UnOp::NegFloat,
            &[BinOp::AddFloat, BinOp::SubFloat, BinOp::MulFloat, BinOp::DivFloat, BinOp::ModFloat, BinOp::PowFloat],
        ),
        _ => return false,
    };
    match &e.kind {
        IrExprKind::UnOp { op, operand } if *op == neg_op => is_default_numeric_tree(operand, default),
        IrExprKind::BinOp { op, left, right } => {
            ops.contains(op)
                && is_default_numeric_tree(left, default)
                && is_default_numeric_tree(right, default)
        }
        _ => false,
    }
}

/// Stamp `declared` on every node of a tree `is_default_numeric_tree` accepted.
/// Whole-tree, because an operator node and its operands must agree on width.
fn stamp_numeric_tree(e: &mut IrExpr, declared: &Ty) {
    e.ty = declared.clone();
    match &mut e.kind {
        IrExprKind::UnOp { operand, .. } => stamp_numeric_tree(operand, declared),
        IrExprKind::BinOp { left, right, .. } => {
            stamp_numeric_tree(left, declared);
            stamp_numeric_tree(right, declared);
        }
        _ => {}
    }
}

/// Coerce a tuple literal's elements positionally.
///
/// A length mismatch is left alone: the checker reports it, and coercing a
/// prefix would leave the value half-retyped behind that diagnostic.
fn coerce_tuple_elements(ir_val: &mut IrExpr, elem_tys: &[Ty], env: &TypeEnv) {
    let IrExprKind::Tuple { elements } = &mut ir_val.kind else { return };
    if elements.len() != elem_tys.len() {
        return;
    }
    for (e, t) in elements.iter_mut().zip(elem_tys.iter()) {
        coerce_literal_to_sized(e, t, env);
    }
}

/// Coerce a record literal's field values, matched by field name.
///
/// Matching by name rather than position is required: a record literal's fields
/// are in source order while the declared fields are in declaration order.
fn coerce_record_fields(ir_val: &mut IrExpr, decl_fields: &[(almide_base::intern::Sym, Ty)], env: &TypeEnv) {
    let IrExprKind::Record { fields, .. } = &mut ir_val.kind else { return };
    for (fname, fvalue) in fields.iter_mut() {
        if let Some((_, fty)) = decl_fields.iter().find(|(n, _)| n == fname) {
            coerce_literal_to_sized(fvalue, fty, env);
        }
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
        ast::Pattern::Literal { value } => lower_pattern_literal(ctx, value),
        ast::Pattern::Constructor { name, args } => {
            let bare_name = bare_ctor_name(name);
            let payload_tys = get_constructor_payload_tys_from_subject(ctx, &bare_name, ty);
            let ir_args = args.iter().enumerate().map(|(i, a)| {
                let arg_ty = payload_tys.get(i).cloned().unwrap_or(Ty::Unknown);
                lower_pattern(ctx, a, &arg_ty)
            }).collect();
            IrPattern::Constructor { name: bare_name.to_string(), args: ir_args }
        }
        ast::Pattern::RecordPattern { name, fields, rest } =>
            lower_pattern_record(ctx, name, fields, *rest),
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
) -> IrPattern {
    let bare_name = bare_ctor_name(name);
    let mut ir_fields: Vec<IrFieldPattern> = fields.iter().map(|f| {
        let field_ty = resolve_record_field_ty(ctx, &bare_name, &f.name);
        IrFieldPattern {
            name: f.name.to_string(),
            pattern: f.pattern.as_ref().map(|p| lower_pattern(ctx, p, &field_ty)),
        }
    }).collect();
    for (i, f) in fields.iter().enumerate() {
        if f.pattern.is_none() {
            let field_ty = resolve_record_field_ty(ctx, &bare_name, &f.name);
            let var = ctx.define_var(&f.name, field_ty.clone(), Mutability::Let, None);
            ir_fields[i].pattern = Some(IrPattern::Bind { var, ty: field_ty });
        }
    }
    IrPattern::RecordPattern { name: bare_name.to_string(), fields: ir_fields, rest }
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

fn resolve_record_field_ty(ctx: &LowerCtx, record_name: &str, field_name: &str) -> Ty {
    if let Some(type_def) = ctx.env.types.get(&sym(record_name)) {
        ctx.resolve_field_ty(type_def, field_name)
    } else if let Some((_, case)) = ctx.env.lookup_ctor_in(&sym(record_name), ctx.current_module.map(|s| s.as_str())) {
        if let crate::types::VariantPayload::Record(fs) = &case.payload {
            fs.iter().find(|(n, _)| n == field_name).map(|(_, t)| t.clone()).unwrap_or(Ty::Unknown)
        } else { Ty::Unknown }
    } else { Ty::Unknown }
}
