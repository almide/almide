//! ShadowResolvePass: convert let-shadowing to assignment for GC languages.
//!
//! In Almide/Rust, `let x = 1; let x = 2;` creates a new binding that shadows
//! the first. In TS/JS, `let x = 1; let x = 2;` is a SyntaxError (redeclaration).
//!
//! This pass detects shadowed bindings in the same scope and converts the second
//! `Bind` to `Assign` (reuse the existing variable).

use std::collections::HashMap;
use crate::ir::*;
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct ShadowResolvePass;

impl NanoPass for ShadowResolvePass {
    fn name(&self) -> &str { "ShadowResolve" }
    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::TypeScript, Target::Python])
    }
    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        for func in &mut program.functions {
            let mut seen: HashMap<String, VarId> = HashMap::new();
            resolve_stmts_block(&mut func.body, &program.var_table, &mut seen);
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                let mut seen: HashMap<String, VarId> = HashMap::new();
                resolve_stmts_block(&mut func.body, &module.var_table, &mut seen);
            }
        }
        PassResult { program, changed: true }
    }
}

fn resolve_stmts_block(expr: &mut IrExpr, vt: &VarTable, seen: &mut HashMap<String, VarId>) {
    match &mut expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            resolve_stmts(stmts, vt, seen);
            if let Some(e) = tail {
                resolve_stmts_block(e, vt, seen);
            }
        }
        IrExprKind::DoBlock { stmts, expr: tail } => {
            resolve_stmts(stmts, vt, seen);
            if let Some(e) = tail {
                resolve_stmts_block(e, vt, seen);
            }
        }
        IrExprKind::If { cond, then, else_ } => {
            resolve_stmts_block(cond, vt, seen);
            let mut then_seen = seen.clone();
            resolve_stmts_block(then, vt, &mut then_seen);
            let mut else_seen = seen.clone();
            resolve_stmts_block(else_, vt, &mut else_seen);
        }
        IrExprKind::ForIn { body, .. } => {
            let mut inner = seen.clone();
            resolve_stmts(body, vt, &mut inner);
        }
        IrExprKind::While { body, .. } => {
            let mut inner = seen.clone();
            resolve_stmts(body, vt, &mut inner);
        }
        IrExprKind::Lambda { body, .. } => {
            let mut inner = HashMap::new();
            resolve_stmts_block(body, vt, &mut inner);
        }
        IrExprKind::Match { arms, .. } => {
            for arm in arms {
                let mut arm_seen = seen.clone();
                resolve_stmts_block(&mut arm.body, vt, &mut arm_seen);
            }
        }
        _ => {}
    }
}

fn resolve_stmts(stmts: &mut Vec<IrStmt>, vt: &VarTable, seen: &mut HashMap<String, VarId>) {
    for stmt in stmts.iter_mut() {
        match &mut stmt.kind {
            IrStmtKind::Bind { var, value, .. } => {
                // Recurse into value first
                resolve_stmts_block(value, vt, seen);

                let name = vt.get(*var).name.to_string();
                if seen.contains_key(&name) {
                    // Shadow: convert Bind → Assign (reuse existing variable)
                    let prev_var = seen[&name];
                    stmt.kind = IrStmtKind::Assign {
                        var: prev_var,
                        value: value.clone(),
                    };
                } else {
                    seen.insert(name, *var);
                }
            }
            IrStmtKind::Expr { expr } => {
                resolve_stmts_block(expr, vt, seen);
            }
            IrStmtKind::Guard { cond, else_ } => {
                resolve_stmts_block(cond, vt, seen);
                resolve_stmts_block(else_, vt, seen);
            }
            _ => {}
        }
    }
}
