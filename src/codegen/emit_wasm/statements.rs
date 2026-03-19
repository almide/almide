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

/// Result of pre-scanning a function body for local variables.
pub struct LocalScanResult {
    pub binds: Vec<(VarId, ValType)>,
    /// Max nesting depth of match expressions (for scratch locals).
    pub match_depth: usize,
}

/// Pre-scan a function body to collect all local variable bindings
/// and count match nesting depth for scratch local allocation.
pub fn collect_locals(body: &IrExpr) -> LocalScanResult {
    let mut binds = Vec::new();
    scan_expr(body, &mut binds);
    let match_depth = count_match_depth(body);
    LocalScanResult { binds, match_depth }
}

/// Count the maximum nesting depth of match expressions.
fn count_match_depth(expr: &IrExpr) -> usize {
    match &expr.kind {
        IrExprKind::Match { subject, arms } => {
            let inner = arms.iter()
                .map(|a| count_match_depth(&a.body))
                .max().unwrap_or(0);
            let subj = count_match_depth(subject);
            1 + inner.max(subj)
        }
        IrExprKind::Block { stmts, expr } | IrExprKind::DoBlock { stmts, expr } => {
            let s = stmts.iter().map(|s| count_match_depth_stmt(s)).max().unwrap_or(0);
            let e = expr.as_ref().map(|e| count_match_depth(e)).unwrap_or(0);
            s.max(e)
        }
        IrExprKind::If { cond, then, else_ } => {
            count_match_depth(cond)
                .max(count_match_depth(then))
                .max(count_match_depth(else_))
        }
        IrExprKind::While { cond, body } => {
            let b = body.iter().map(|s| count_match_depth_stmt(s)).max().unwrap_or(0);
            count_match_depth(cond).max(b)
        }
        IrExprKind::BinOp { left, right, .. } => {
            count_match_depth(left).max(count_match_depth(right))
        }
        IrExprKind::UnOp { operand, .. } => count_match_depth(operand),
        IrExprKind::Call { args, .. } => {
            args.iter().map(|a| count_match_depth(a)).max().unwrap_or(0)
        }
        _ => 0,
    }
}

fn count_match_depth_stmt(stmt: &IrStmt) -> usize {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } => count_match_depth(value),
        IrStmtKind::Expr { expr } => count_match_depth(expr),
        IrStmtKind::Guard { cond, else_ } => count_match_depth(cond).max(count_match_depth(else_)),
        _ => 0,
    }
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
                scan_pattern(&arm.pattern, &subject.ty, locals);
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

/// Scan a match pattern for variable bindings.
fn scan_pattern(pattern: &crate::ir::IrPattern, subject_ty: &crate::types::Ty, locals: &mut Vec<(VarId, ValType)>) {
    match pattern {
        crate::ir::IrPattern::Bind { var } => {
            if let Some(vt) = values::ty_to_valtype(subject_ty) {
                locals.push((*var, vt));
            }
        }
        crate::ir::IrPattern::Constructor { args, .. } => {
            for arg in args {
                scan_pattern(arg, subject_ty, locals);
            }
        }
        crate::ir::IrPattern::Tuple { elements } => {
            for elem in elements {
                scan_pattern(elem, subject_ty, locals);
            }
        }
        crate::ir::IrPattern::Some { inner } | crate::ir::IrPattern::Ok { inner } | crate::ir::IrPattern::Err { inner } => {
            scan_pattern(inner, subject_ty, locals);
        }
        crate::ir::IrPattern::RecordPattern { fields, .. } => {
            for field in fields {
                if let Some(pat) = &field.pattern {
                    scan_pattern(pat, subject_ty, locals);
                }
            }
        }
        _ => {} // Wildcard, Literal, None — no bindings
    }
}
