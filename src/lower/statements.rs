// ── Statement lowering ──────────────────────────────────────────

use crate::ast;
use crate::ir::*;
use crate::types::{Ty, TypeConstructorId};
use super::LowerCtx;
use super::expressions::lower_expr;

pub(super) fn lower_stmt(ctx: &mut LowerCtx, stmt: &ast::Stmt) -> IrStmt {
    let span = match stmt {
        ast::Stmt::Let { span, .. } | ast::Stmt::Var { span, .. }
        | ast::Stmt::Assign { span, .. } | ast::Stmt::Guard { span, .. }
        | ast::Stmt::Expr { span, .. } | ast::Stmt::IndexAssign { span, .. }
        | ast::Stmt::FieldAssign { span, .. } | ast::Stmt::LetDestructure { span, .. }
        | ast::Stmt::Error { span, .. } => *span,
        ast::Stmt::Comment { .. } => None,
    };

    let kind = match stmt {
        ast::Stmt::Let { name, value, .. } => {
            let ir_val = lower_expr(ctx, value);
            let val_ty = ir_val.ty.clone();
            let var = ctx.define_var(name, val_ty.clone(), Mutability::Let, span);
            IrStmtKind::Bind { var, mutability: Mutability::Let, ty: val_ty, value: ir_val }
        }
        ast::Stmt::Var { name, value, .. } => {
            let ir_val = lower_expr(ctx, value);
            let val_ty = ir_val.ty.clone();
            let var = ctx.define_var(name, val_ty.clone(), Mutability::Var, span);
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
            let var = ctx.lookup_var(target).unwrap_or(VarId(0));
            let ir_val = lower_expr(ctx, value);
            IrStmtKind::FieldAssign { target: var, field: field.clone(), value: ir_val }
        }
        ast::Stmt::Guard { cond, else_, .. } => {
            let ir_cond = lower_expr(ctx, cond);
            let ir_else = lower_expr(ctx, else_);
            IrStmtKind::Guard { cond: ir_cond, else_: ir_else }
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

// ── Pattern lowering ────────────────────────────────────────────

pub(super) fn lower_pattern(ctx: &mut LowerCtx, pat: &ast::Pattern, ty: &Ty) -> IrPattern {
    match pat {
        ast::Pattern::Wildcard => IrPattern::Wildcard,
        ast::Pattern::Ident { name } => {
            let var = ctx.define_var(name, ty.clone(), Mutability::Let, None);
            IrPattern::Bind { var }
        }
        ast::Pattern::Literal { value } => {
            // Pattern literals may not have expr_types entries (they're patterns,
            // not expressions), so construct IR directly without calling lower_expr.
            let (kind, ty) = match value.as_ref() {
                ast::Expr::Int { raw, .. } => {
                    let v = raw.parse::<i64>().unwrap_or(0);
                    (IrExprKind::LitInt { value: v }, Ty::Int)
                }
                ast::Expr::Float { value: v, .. } => (IrExprKind::LitFloat { value: *v }, Ty::Float),
                ast::Expr::String { value: v, .. } => (IrExprKind::LitStr { value: v.clone() }, Ty::String),
                ast::Expr::Bool { value: v, .. } => (IrExprKind::LitBool { value: *v }, Ty::Bool),
                _ => {
                    let ir_expr = lower_expr(ctx, value);
                    return IrPattern::Literal { expr: ir_expr };
                }
            };
            let ir_expr = ctx.mk(kind, ty, value.span());
            IrPattern::Literal { expr: ir_expr }
        }
        ast::Pattern::Constructor { name, args } => {
            let payload_tys = get_constructor_payload_tys(ctx, name);
            let ir_args = args.iter().enumerate().map(|(i, a)| {
                let arg_ty = payload_tys.get(i).cloned().unwrap_or(Ty::Unknown);
                lower_pattern(ctx, a, &arg_ty)
            }).collect();
            IrPattern::Constructor { name: name.clone(), args: ir_args }
        }
        ast::Pattern::RecordPattern { name, fields, rest } => {
            let ir_fields = fields.iter().map(|f| {
                let field_ty = resolve_record_field_ty(ctx, name, &f.name);
                IrFieldPattern {
                    name: f.name.clone(),
                    pattern: f.pattern.as_ref().map(|p| lower_pattern(ctx, p, &field_ty)),
                }
            }).collect();
            // Bind unmatched fields as variables
            for f in fields {
                if f.pattern.is_none() {
                    let field_ty = resolve_record_field_ty(ctx, name, &f.name);
                    ctx.define_var(&f.name, field_ty, Mutability::Let, None);
                }
            }
            IrPattern::RecordPattern { name: name.clone(), fields: ir_fields, rest: *rest }
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
    }
}

fn get_constructor_payload_tys(ctx: &LowerCtx, ctor_name: &str) -> Vec<Ty> {
    if let Some((_, case)) = ctx.env.constructors.get(ctor_name) {
        match &case.payload {
            crate::types::VariantPayload::Tuple(tys) => tys.clone(),
            crate::types::VariantPayload::Record(fs) => fs.iter().map(|(_, t, _)| t.clone()).collect(),
            crate::types::VariantPayload::Unit => vec![],
        }
    } else {
        vec![]
    }
}

fn resolve_record_field_ty(ctx: &LowerCtx, record_name: &str, field_name: &str) -> Ty {
    if let Some(type_def) = ctx.env.types.get(record_name) {
        ctx.resolve_field_ty(type_def, field_name)
    } else if let Some((_, case)) = ctx.env.constructors.get(record_name) {
        if let crate::types::VariantPayload::Record(fs) = &case.payload {
            fs.iter().find(|(n, _, _)| n == field_name).map(|(_, t, _)| t.clone()).unwrap_or(Ty::Unknown)
        } else { Ty::Unknown }
    } else { Ty::Unknown }
}
