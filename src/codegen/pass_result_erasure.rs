//! ResultErasurePass: erase Result/Option wrapping for GC languages (TS, Python).
//!
//! In these targets:
//! - ok(x) → x
//! - err(e) → throw (rendered via template)
//! - some(x) → x
//! - none → null (rendered via template as unit_literal or none_expr)
//! - Try { expr } → expr (no ? operator)
//! - Effect fn Result<T, E> return → T
//!
//! This is the inverse of ResultPropagationPass (Rust inserts Try),
//! while this pass strips Result/Option wrappers entirely.

use crate::ir::*;
use crate::types::{Ty, TypeConstructorId};
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct ResultErasurePass;

impl NanoPass for ResultErasurePass {
    fn name(&self) -> &str { "ResultErasure" }
    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::TypeScript, Target::Python])
    }
    fn depends_on(&self) -> Vec<&'static str> { vec!["MatchLowering"] }
    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        for func in &mut program.functions {
            // Erase Result return type on effect functions
            if func.is_effect {
                func.ret_ty = erase_result_ty(func.ret_ty.clone());
            }
            func.body = erase_expr(std::mem::take(&mut func.body));
        }
        for tl in &mut program.top_lets {
            tl.value = erase_expr(std::mem::take(&mut tl.value));
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                if func.is_effect {
                    func.ret_ty = erase_result_ty(func.ret_ty.clone());
                }
                func.body = erase_expr(std::mem::take(&mut func.body));
            }
            for tl in &mut module.top_lets {
                tl.value = erase_expr(std::mem::take(&mut tl.value));
            }
        }
        PassResult { program, changed: true }
    }
}

/// Erase Result<T, E> → T, Option<T> → T
fn erase_result_ty(ty: Ty) -> Ty {
    match ty {
        Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args.into_iter().next().unwrap(),
        Ty::Applied(TypeConstructorId::Option, args) if args.len() == 1 => args.into_iter().next().unwrap(),
        other => other,
    }
}

fn erase_expr(expr: IrExpr) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;

    let kind = match expr.kind {
        // ok(x) → x
        IrExprKind::ResultOk { expr: inner } => {
            return erase_expr(*inner);
        }
        // err(e) → keep as ResultErr (template renders as throw)
        IrExprKind::ResultErr { expr: inner } => {
            IrExprKind::ResultErr { expr: Box::new(erase_expr(*inner)) }
        }
        // some(x) → x
        IrExprKind::OptionSome { expr: inner } => {
            return erase_expr(*inner);
        }
        // none → keep as OptionNone (template renders as null)
        IrExprKind::OptionNone => IrExprKind::OptionNone,
        // try(expr) / unwrap(expr) / to_option(expr) → expr (no ? operator in TS)
        IrExprKind::Try { expr: inner }
        | IrExprKind::Unwrap { expr: inner }
        | IrExprKind::ToOption { expr: inner } => {
            return erase_expr(*inner);
        }
        // unwrap_or(expr, fallback) → expr (fallback erased in TS)
        IrExprKind::UnwrapOr { expr: inner, .. } => {
            return erase_expr(*inner);
        }

        // Recurse into sub-expressions
        IrExprKind::Call { target, args, type_args } => {
            let args = args.into_iter().map(erase_expr).collect();
            let target = match target {
                CallTarget::Method { object, method } => CallTarget::Method {
                    object: Box::new(erase_expr(*object)), method,
                },
                CallTarget::Computed { callee } => CallTarget::Computed {
                    callee: Box::new(erase_expr(*callee)),
                },
                other => other,
            };
            IrExprKind::Call { target, args, type_args }
        }
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: Box::new(erase_expr(*cond)),
            then: Box::new(erase_expr(*then)),
            else_: Box::new(erase_expr(*else_)),
        },
        IrExprKind::Block { stmts, expr } => IrExprKind::Block {
            stmts: erase_stmts(stmts),
            expr: expr.map(|e| Box::new(erase_expr(*e))),
        },

        IrExprKind::Match { subject, arms } => IrExprKind::Match {
            subject: Box::new(erase_expr(*subject)),
            arms: arms.into_iter().map(|arm| IrMatchArm {
                pattern: arm.pattern,
                guard: arm.guard.map(erase_expr),
                body: erase_expr(arm.body),
            }).collect(),
        },
        IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
            op, left: Box::new(erase_expr(*left)), right: Box::new(erase_expr(*right)),
        },
        IrExprKind::UnOp { op, operand } => IrExprKind::UnOp {
            op, operand: Box::new(erase_expr(*operand)),
        },
        IrExprKind::Lambda { params, body, lambda_id } => IrExprKind::Lambda {
            params, body: Box::new(erase_expr(*body)), lambda_id,
        },
        IrExprKind::List { elements } => IrExprKind::List {
            elements: elements.into_iter().map(erase_expr).collect(),
        },
        IrExprKind::Record { name, fields } => IrExprKind::Record {
            name, fields: fields.into_iter().map(|(k, v)| (k, erase_expr(v))).collect(),
        },
        IrExprKind::Member { object, field } => IrExprKind::Member {
            object: Box::new(erase_expr(*object)), field,
        },
        IrExprKind::ForIn { var, var_tuple, iterable, body } => IrExprKind::ForIn {
            var, var_tuple, iterable: Box::new(erase_expr(*iterable)),
            body: erase_stmts(body),
        },
        IrExprKind::While { cond, body } => IrExprKind::While {
            cond: Box::new(erase_expr(*cond)), body: erase_stmts(body),
        },
        IrExprKind::StringInterp { parts } => IrExprKind::StringInterp {
            parts: parts.into_iter().map(|p| match p {
                IrStringPart::Expr { expr } => IrStringPart::Expr { expr: erase_expr(expr) },
                other => other,
            }).collect(),
        },
        IrExprKind::Tuple { elements } => IrExprKind::Tuple {
            elements: elements.into_iter().map(erase_expr).collect(),
        },
        IrExprKind::SpreadRecord { base, fields } => IrExprKind::SpreadRecord {
            base: Box::new(erase_expr(*base)),
            fields: fields.into_iter().map(|(k, v)| (k, erase_expr(v))).collect(),
        },
        IrExprKind::MapLiteral { entries } => IrExprKind::MapLiteral {
            entries: entries.into_iter().map(|(k, v)| (erase_expr(k), erase_expr(v))).collect(),
        },
        IrExprKind::IndexAccess { object, index } => IrExprKind::IndexAccess {
            object: Box::new(erase_expr(*object)),
            index: Box::new(erase_expr(*index)),
        },
        IrExprKind::MapAccess { object, key } => IrExprKind::MapAccess {
            object: Box::new(erase_expr(*object)),
            key: Box::new(erase_expr(*key)),
        },
        IrExprKind::Range { start, end, inclusive } => IrExprKind::Range {
            start: Box::new(erase_expr(*start)),
            end: Box::new(erase_expr(*end)),
            inclusive,
        },
        IrExprKind::Fan { exprs } => IrExprKind::Fan {
            exprs: exprs.into_iter().map(erase_expr).collect(),
        },
        IrExprKind::RustMacro { name, args } => IrExprKind::RustMacro {
            name, args: args.into_iter().map(erase_expr).collect(),
        },
        other => other,
    };

    IrExpr { kind, ty, span }
}

fn erase_stmts(stmts: Vec<IrStmt>) -> Vec<IrStmt> {
    stmts.into_iter().map(|s| {
        let kind = match s.kind {
            IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind {
                var, mutability, ty, value: erase_expr(value),
            },
            IrStmtKind::Assign { var, value } => IrStmtKind::Assign { var, value: erase_expr(value) },
            IrStmtKind::Expr { expr } => IrStmtKind::Expr { expr: erase_expr(expr) },
            IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
                cond: erase_expr(cond), else_: erase_expr(else_),
            },
            IrStmtKind::BindDestructure { pattern, value } => IrStmtKind::BindDestructure {
                pattern, value: erase_expr(value),
            },
            other => other,
        };
        IrStmt { kind, span: s.span }
    }).collect()
}
