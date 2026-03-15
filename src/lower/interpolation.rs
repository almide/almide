// ── String interpolation ────────────────────────────────────────

use crate::ast;
use crate::ir::*;
use crate::types::Ty;
use super::LowerCtx;
use super::expressions::lower_expr;

pub(super) fn lower_interpolation(ctx: &mut LowerCtx, template: &str) -> Vec<IrStringPart> {
    let mut parts = Vec::new();
    let mut lit = String::new();
    let chars: Vec<char> = template.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '$' && i + 1 < chars.len() && chars[i + 1] == '{' {
            if !lit.is_empty() {
                parts.push(IrStringPart::Lit { value: std::mem::take(&mut lit) });
            }
            i += 2; // skip ${
            let mut depth = 1;
            let mut expr_str = String::new();
            while i < chars.len() && depth > 0 {
                if chars[i] == '{' { depth += 1; }
                if chars[i] == '}' { depth -= 1; if depth == 0 { break; } }
                expr_str.push(chars[i]);
                i += 1;
            }
            i += 1; // skip }
            // Re-parse and lower the expression
            let tokens = crate::lexer::Lexer::tokenize(&expr_str);
            let mut parser = crate::parser::Parser::new_with_id_offset(tokens, u32::MAX / 2);
            if let Ok(parsed) = parser.parse_single_expr() {
                let mut ir_expr = lower_interpolation_expr(ctx, &parsed);
                // Fix type for simple vars
                if let IrExprKind::Var { id } = &ir_expr.kind {
                    ir_expr.ty = ctx.var_table.get(*id).ty.clone();
                }
                // Operator protocol: dispatch to Repr convention if available
                if let Some(repr_fn) = ctx.find_convention_fn(&ir_expr.ty, "repr") {
                    ir_expr = ctx.mk(IrExprKind::Call {
                        target: CallTarget::Named { name: repr_fn },
                        args: vec![ir_expr], type_args: vec![],
                    }, Ty::String, None);
                }
                parts.push(IrStringPart::Expr { expr: ir_expr });
            } else {
                parts.push(IrStringPart::Lit { value: format!("${{{}}}", expr_str) });
            }
        } else {
            lit.push(chars[i]);
            i += 1;
        }
    }
    if !lit.is_empty() {
        parts.push(IrStringPart::Lit { value: lit });
    }
    parts
}

// ── String interpolation expression lowering ────────────────────
// Interpolation expressions are re-parsed at lower time and have no
// expr_types entries. We lower them without consulting the type checker.

fn lower_interpolation_expr(ctx: &mut LowerCtx, expr: &ast::Expr) -> IrExpr {
    let span = expr.span();
    match expr {
        ast::Expr::Int { raw, .. } => {
            let value = raw.parse::<i64>().unwrap_or(0);
            ctx.mk(IrExprKind::LitInt { value }, Ty::Int, span)
        }
        ast::Expr::Float { value, .. } => ctx.mk(IrExprKind::LitFloat { value: *value }, Ty::Float, span),
        ast::Expr::String { value, .. } => ctx.mk(IrExprKind::LitStr { value: value.clone() }, Ty::String, span),
        ast::Expr::Bool { value, .. } => ctx.mk(IrExprKind::LitBool { value: *value }, Ty::Bool, span),
        ast::Expr::Ident { name, .. } => {
            if let Some(var_id) = ctx.lookup_var(name) {
                let ty = ctx.var_table.get(var_id).ty.clone();
                ctx.mk(IrExprKind::Var { id: var_id }, ty, span)
            } else {
                ctx.mk(IrExprKind::LitStr { value: name.clone() }, Ty::String, span)
            }
        }
        // Module call: int.to_string(x), string.len(s), etc.
        ast::Expr::Call { callee, args, .. } => {
            let lowered_args: Vec<IrExpr> = args.iter().map(|a| lower_interpolation_expr(ctx, a)).collect();
            if let ast::Expr::Member { object, field, .. } = callee.as_ref() {
                if let ast::Expr::Ident { name: module, .. } = object.as_ref() {
                    let target = CallTarget::Module { module: module.clone(), func: field.clone() };
                    let ret_ty = crate::stdlib::lookup_sig(module, field)
                        .map(|sig| sig.ret.clone())
                        .unwrap_or(Ty::String);
                    return ctx.mk(IrExprKind::Call { target, args: lowered_args, type_args: vec![] }, ret_ty, span);
                }
            }
            // Named call: foo(x)
            if let ast::Expr::Ident { name, .. } = callee.as_ref() {
                let target = CallTarget::Named { name: name.clone() };
                ctx.mk(IrExprKind::Call { target, args: lowered_args, type_args: vec![] }, Ty::String, span)
            } else {
                lower_expr(ctx, expr)
            }
        }
        // Anything else: fall back to lower_expr
        _ => lower_expr(ctx, expr),
    }
}
