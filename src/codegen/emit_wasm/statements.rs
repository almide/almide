//! IrStmt → WASM instruction emission + local variable pre-scanning.

use crate::ir::{IrExpr, IrExprKind, IrStmt, IrStmtKind, VarId};
use wasm_encoder::{Instruction, ValType};

use super::FuncCompiler;
use super::values;

impl FuncCompiler<'_> {
    /// Emit a single IR statement.
    pub fn emit_stmt(&mut self, stmt: &IrStmt) {
        match &stmt.kind {
            IrStmtKind::Bind { var, ty, value, .. } => {
                self.emit_expr(value);
                if let Some(_vt) = values::ty_to_valtype(ty) {
                    let local_idx = self.var_map[&var.0];
                    self.func.instruction(&Instruction::LocalSet(local_idx));
                }
                // Unit bindings: value produces nothing, nothing to store
            }

            IrStmtKind::Assign { var, value } => {
                self.emit_expr(value);
                let local_idx = self.var_map[&var.0];
                self.func.instruction(&Instruction::LocalSet(local_idx));
            }

            IrStmtKind::Expr { expr } => {
                self.emit_expr(expr);
                // Drop the value if the expression produces one
                if values::ty_to_valtype(&expr.ty).is_some() {
                    self.func.instruction(&Instruction::Drop);
                }
            }

            IrStmtKind::Guard { cond, else_: _ } => {
                // Guard in a do/while block: if cond is false, break
                self.emit_expr(cond);
                self.func.instruction(&Instruction::I32Eqz);
                if let Some(labels) = self.loop_stack.last() {
                    let relative = self.depth - labels.break_depth - 1;
                    self.func.instruction(&Instruction::BrIf(relative));
                }
            }

            IrStmtKind::Comment { .. } => {
                // No-op in WASM
            }

            IrStmtKind::BindDestructure { .. }
            | IrStmtKind::IndexAssign { .. }
            | IrStmtKind::FieldAssign { .. } => {
                // Phase 2+: not needed for FizzBuzz
            }
        }
    }
}

/// Pre-scan a function body to collect all local variable bindings.
/// WASM requires all locals declared upfront before the function body.
pub fn collect_locals(body: &IrExpr) -> Vec<(VarId, ValType)> {
    let mut locals = Vec::new();
    scan_expr(body, &mut locals);
    locals
}

fn scan_expr(expr: &IrExpr, locals: &mut Vec<(VarId, ValType)>) {
    match &expr.kind {
        IrExprKind::Block { stmts, expr } | IrExprKind::DoBlock { stmts, expr } => {
            for stmt in stmts {
                scan_stmt(stmt, locals);
            }
            if let Some(e) = expr {
                scan_expr(e, locals);
            }
        }
        IrExprKind::If { cond, then, else_ } => {
            scan_expr(cond, locals);
            scan_expr(then, locals);
            scan_expr(else_, locals);
        }
        IrExprKind::While { cond, body } => {
            scan_expr(cond, locals);
            for stmt in body {
                scan_stmt(stmt, locals);
            }
        }
        IrExprKind::ForIn { body, iterable, .. } => {
            scan_expr(iterable, locals);
            for stmt in body {
                scan_stmt(stmt, locals);
            }
        }
        IrExprKind::BinOp { left, right, .. } => {
            scan_expr(left, locals);
            scan_expr(right, locals);
        }
        IrExprKind::UnOp { operand, .. } => {
            scan_expr(operand, locals);
        }
        IrExprKind::Call { args, target, .. } => {
            if let crate::ir::CallTarget::Method { object, .. } = target {
                scan_expr(object, locals);
            }
            if let crate::ir::CallTarget::Computed { callee } = target {
                scan_expr(callee, locals);
            }
            for arg in args {
                scan_expr(arg, locals);
            }
        }
        IrExprKind::Match { subject, arms } => {
            scan_expr(subject, locals);
            for arm in arms {
                scan_expr(&arm.body, locals);
            }
        }
        _ => {}
    }
}

fn scan_stmt(stmt: &IrStmt, locals: &mut Vec<(VarId, ValType)>) {
    match &stmt.kind {
        IrStmtKind::Bind { var, ty, value, .. } => {
            if let Some(vt) = values::ty_to_valtype(ty) {
                locals.push((*var, vt));
            }
            scan_expr(value, locals);
        }
        IrStmtKind::Assign { value, .. } => {
            scan_expr(value, locals);
        }
        IrStmtKind::Expr { expr } => {
            scan_expr(expr, locals);
        }
        IrStmtKind::Guard { cond, else_ } => {
            scan_expr(cond, locals);
            scan_expr(else_, locals);
        }
        _ => {}
    }
}
