//! Materialize literal defaults from admitted runtime declarations before
//! native MIR lowering. Omitted arguments are values of the declaration,
//! not a different self-host implementation (e.g. the short slice form).
//! Only side-effect-free literals with the exact declared floor type are
//! admitted here; other defaults keep their existing lowering or refusal.
use almide_ir::{CallTarget, IrExpr, IrExprKind, IrMutVisitor, IrProgram};
use almide_lang::{ast::{Decl, ExprKind}, types::Ty};
use crate::native_floor::{FloorParam, declared};

pub(crate) fn expand(ir: &mut IrProgram) {
    struct Fill;
    impl IrMutVisitor for Fill {
        fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
            almide_ir::walk_expr_mut(self, expr);
            let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &mut expr.kind else { return };
            let Some(tail) = literal_defaults(module.as_str(), func.as_str(), args.len(), expr.span) else { return };
            args.extend(tail);
        }
    }
    for f in &mut ir.functions { Fill.visit_expr_mut(&mut f.body); }
    for t in &mut ir.top_lets { Fill.visit_expr_mut(&mut t.value); }
}

fn literal_defaults(module: &str, func: &str, supplied: usize, span: Option<almide_ir::Span>) -> Option<Vec<IrExpr>> {
    let source = almide_lang::stdlib_info::bundled_source(module)?;
    let program = almide_lang::parse_cached(source)?;
    let params = program.decls.iter().find_map(|d| match d {
        Decl::Fn { name, params, .. } if name.as_str() == func => Some(params),
        _ => None,
    })?;
    // Most calls are already complete. Do not resolve runtime bodies merely
    // to discover that they have no missing default arguments.
    if supplied >= params.len() || params[supplied..].iter().any(|p| p.default.is_none()) { return None; }
    let floor = declared(&format!("{module}.{func}"))?;
    if params.len() != floor.params.len() { return None; }
    params[supplied..].iter().zip(&floor.params[supplied..]).map(|(p, kind)| {
        let default = p.default.as_ref()?;
        let (kind, ty) = match (&default.kind, kind) {
            // The AST's legacy JSON number rounds through f64. Use the same
            // raw-token decoder as the frontend, including radix/separators.
            (ExprKind::Int { raw, .. }, FloorParam::Int) => (IrExprKind::LitInt { value: almide_frontend::decode_int_literal(raw) }, Ty::Int),
            (ExprKind::Unary { op, operand }, FloorParam::Int) if op.as_str() == "-" => {
                let ExprKind::Int { raw, .. } = &operand.kind else { return None };
                (IrExprKind::LitInt { value: almide_frontend::decode_negated_int_literal(raw)? }, Ty::Int)
            }
            (ExprKind::Float { value, .. }, FloorParam::Float) => (IrExprKind::LitFloat { value: *value }, Ty::Float),
            (ExprKind::Bool { value }, FloorParam::Bool) => (IrExprKind::LitBool { value: *value }, Ty::Bool),
            (ExprKind::String { value, .. }, FloorParam::Str) => (IrExprKind::LitStr { value: value.clone() }, Ty::String),
            _ => return None,
        };
        Some(IrExpr { kind, ty, span, def_id: None })
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn omitted_literals_come_from_the_declaration_and_required_arguments_stay_required() {
        let tail = literal_defaults("string", "slice", 2, None).expect("declared default");
        assert_eq!(tail.len(), 1);
        assert!(matches!(tail[0].kind, IrExprKind::LitInt { value: i64::MAX }));
        assert_eq!(tail[0].ty, Ty::Int);
        assert!(literal_defaults("string", "slice", 1, None).is_none(), "start is required");
        assert!(literal_defaults("string", "slice", 3, None).is_none(), "explicit end preserved");
        assert!(literal_defaults("string", "slice", 4, None).is_none(), "excess args not hidden");
        assert!(literal_defaults("user", "slice", 2, None).is_none(), "no declaration, no defaults");
    }
}
