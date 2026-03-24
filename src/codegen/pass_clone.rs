//! ClonePass: insert Clone IR nodes for heap-type variables in Rust.
//!
//! Walks the IR and wraps `Var { id }` in `Clone { expr: Var { id } }`
//! for variables of heap types (String, Vec, HashMap, records, etc.).
//! The walker renders Clone via template (Rust: `.clone()`, TS: identity).

use std::collections::HashSet;
use crate::ir::*;
use crate::types::Ty;
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct CloneInsertionPass;

impl NanoPass for CloneInsertionPass {
    fn name(&self) -> &str { "CloneInsertion" }

    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::Rust])
    }

    fn depends_on(&self) -> Vec<&'static str> { vec!["BorrowInsertion"] }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let clone_ids = collect_clone_ids(&program.var_table);
        for func in &mut program.functions {
            func.body = insert_clones(std::mem::take(&mut func.body), &clone_ids);
        }
        for tl in &mut program.top_lets {
            tl.value = insert_clones(std::mem::take(&mut tl.value), &clone_ids);
        }
        // Process module functions (each module has its own var_table)
        for module in &mut program.modules {
            let module_clone_ids = collect_clone_ids(&module.var_table);
            for func in &mut module.functions {
                func.body = insert_clones(std::mem::take(&mut func.body), &module_clone_ids);
            }
            for tl in &mut module.top_lets {
                tl.value = insert_clones(std::mem::take(&mut tl.value), &module_clone_ids);
            }
        }
        PassResult { program, changed: true }
    }
}

fn collect_clone_ids(vt: &VarTable) -> HashSet<VarId> {
    let mut ids = HashSet::new();
    for i in 0..vt.len() {
        let id = VarId(i as u32);
        if needs_clone(&vt.get(id).ty) {
            ids.insert(id);
        }
    }
    ids
}

fn needs_clone(ty: &Ty) -> bool {
    matches!(ty,
        Ty::String | Ty::Applied(_, _) |
        Ty::Record { .. } | Ty::OpenRecord { .. } |
        Ty::Named(_, _) |
        Ty::Variant { .. } | Ty::Fn { .. }
    )
}

fn insert_clones(expr: IrExpr, clone_ids: &HashSet<VarId>) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;

    let kind = match expr.kind {
        IrExprKind::Var { id } if clone_ids.contains(&id) => {
            return IrExpr {
                kind: IrExprKind::Clone {
                    expr: Box::new(IrExpr { kind: IrExprKind::Var { id }, ty: ty.clone(), span }),
                },
                ty, span,
            };
        }

        // Recurse into sub-expressions
        IrExprKind::Call { target, args, type_args } => {
            let args = args.into_iter().map(|a| insert_clones(a, clone_ids)).collect();
            let target = match target {
                CallTarget::Method { object, method } => CallTarget::Method {
                    object: Box::new(insert_clones(*object, clone_ids)), method,
                },
                CallTarget::Computed { callee } => CallTarget::Computed {
                    callee: Box::new(insert_clones(*callee, clone_ids)),
                },
                other => other,
            };
            IrExprKind::Call { target, args, type_args }
        }
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: Box::new(insert_clones(*cond, clone_ids)),
            then: Box::new(insert_clones(*then, clone_ids)),
            else_: Box::new(insert_clones(*else_, clone_ids)),
        },
        IrExprKind::Block { stmts, expr } => IrExprKind::Block {
            stmts: insert_clone_stmts(stmts, clone_ids),
            expr: expr.map(|e| Box::new(insert_clones(*e, clone_ids))),
        },
        IrExprKind::DoBlock { stmts, expr } => IrExprKind::DoBlock {
            stmts: insert_clone_stmts(stmts, clone_ids),
            expr: expr.map(|e| Box::new(insert_clones(*e, clone_ids))),
        },
        IrExprKind::Match { subject, arms } => IrExprKind::Match {
            subject: Box::new(insert_clones(*subject, clone_ids)),
            arms: arms.into_iter().map(|arm| IrMatchArm {
                pattern: arm.pattern,
                guard: arm.guard.map(|g| insert_clones(g, clone_ids)),
                body: insert_clones(arm.body, clone_ids),
            }).collect(),
        },
        IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
            op, left: Box::new(insert_clones(*left, clone_ids)), right: Box::new(insert_clones(*right, clone_ids)),
        },
        IrExprKind::UnOp { op, operand } => IrExprKind::UnOp {
            op, operand: Box::new(insert_clones(*operand, clone_ids)),
        },
        IrExprKind::Lambda { params, body, lambda_id } => IrExprKind::Lambda {
            params, body: Box::new(insert_clones(*body, clone_ids)), lambda_id,
        },
        IrExprKind::List { elements } => IrExprKind::List {
            elements: elements.into_iter().map(|e| insert_clones(e, clone_ids)).collect(),
        },
        IrExprKind::Record { name, fields } => IrExprKind::Record {
            name, fields: fields.into_iter().map(|(k, v)| (k, insert_clones(v, clone_ids))).collect(),
        },
        IrExprKind::Member { object, field } => IrExprKind::Member {
            object: Box::new(insert_clones(*object, clone_ids)), field,
        },
        IrExprKind::ForIn { var, var_tuple, iterable, body } => IrExprKind::ForIn {
            var, var_tuple, iterable: Box::new(insert_clones(*iterable, clone_ids)),
            body: insert_clone_stmts(body, clone_ids),
        },
        IrExprKind::While { cond, body } => IrExprKind::While {
            cond: Box::new(insert_clones(*cond, clone_ids)),
            body: insert_clone_stmts(body, clone_ids),
        },
        IrExprKind::StringInterp { parts } => IrExprKind::StringInterp {
            parts: parts.into_iter().map(|p| match p {
                IrStringPart::Expr { expr } => IrStringPart::Expr { expr: insert_clones(expr, clone_ids) },
                other => other,
            }).collect(),
        },
        IrExprKind::OptionSome { expr } => IrExprKind::OptionSome { expr: Box::new(insert_clones(*expr, clone_ids)) },
        IrExprKind::ResultOk { expr } => IrExprKind::ResultOk { expr: Box::new(insert_clones(*expr, clone_ids)) },
        IrExprKind::ResultErr { expr } => IrExprKind::ResultErr { expr: Box::new(insert_clones(*expr, clone_ids)) },
        IrExprKind::Try { expr } => IrExprKind::Try { expr: Box::new(insert_clones(*expr, clone_ids)) },
        IrExprKind::Fan { exprs } => IrExprKind::Fan {
            exprs: exprs.into_iter().map(|e| insert_clones(e, clone_ids)).collect(),
        },
        IrExprKind::SpreadRecord { base, fields } => IrExprKind::SpreadRecord {
            base: Box::new(insert_clones(*base, clone_ids)),
            fields: fields.into_iter().map(|(k, v)| (k, insert_clones(v, clone_ids))).collect(),
        },
        IrExprKind::IndexAccess { object, index } => IrExprKind::IndexAccess {
            object: Box::new(insert_clones(*object, clone_ids)),
            index: Box::new(insert_clones(*index, clone_ids)),
        },
        IrExprKind::MapAccess { object, key } => IrExprKind::MapAccess {
            object: Box::new(insert_clones(*object, clone_ids)),
            key: Box::new(insert_clones(*key, clone_ids)),
        },
        IrExprKind::Range { start, end, inclusive } => IrExprKind::Range {
            start: Box::new(insert_clones(*start, clone_ids)),
            end: Box::new(insert_clones(*end, clone_ids)),
            inclusive,
        },
        IrExprKind::Tuple { elements } => IrExprKind::Tuple {
            elements: elements.into_iter().map(|e| insert_clones(e, clone_ids)).collect(),
        },
        IrExprKind::MapLiteral { entries } => IrExprKind::MapLiteral {
            entries: entries.into_iter().map(|(k, v)| (insert_clones(k, clone_ids), insert_clones(v, clone_ids))).collect(),
        },
        other => other,
    };

    IrExpr { kind, ty, span }
}

fn insert_clone_stmts(stmts: Vec<IrStmt>, clone_ids: &HashSet<VarId>) -> Vec<IrStmt> {
    stmts.into_iter().map(|s| {
        let kind = match s.kind {
            IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind {
                var, mutability, ty, value: insert_clones(value, clone_ids),
            },
            IrStmtKind::Assign { var, value } => IrStmtKind::Assign { var, value: insert_clones(value, clone_ids) },
            IrStmtKind::Expr { expr } => IrStmtKind::Expr { expr: insert_clones(expr, clone_ids) },
            IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
                cond: insert_clones(cond, clone_ids), else_: insert_clones(else_, clone_ids),
            },
            IrStmtKind::BindDestructure { pattern, value } => IrStmtKind::BindDestructure {
                pattern, value: insert_clones(value, clone_ids),
            },
            other => other,
        };
        IrStmt { kind, span: s.span }
    }).collect()
}
