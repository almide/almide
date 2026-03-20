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

            IrStmtKind::Guard { cond, else_ } => {
                // Guard: if cond is false, execute else_ action
                self.emit_expr(cond);
                self.func.instruction(&Instruction::I32Eqz);
                self.func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
                self.depth += 1;

                match &else_.kind {
                    // Break/Continue: emit directly (they generate the right br)
                    crate::ir::IrExprKind::Break | crate::ir::IrExprKind::Continue => {
                        self.emit_expr(else_);
                    }
                    // Other expressions: evaluate, drop value, then break
                    _ => {
                        self.emit_expr(else_);
                        if super::values::ty_to_valtype(&else_.ty).is_some() {
                            self.func.instruction(&Instruction::Drop);
                        }
                        if let Some(labels) = self.loop_stack.last() {
                            let relative = self.depth - labels.break_depth - 1;
                            self.func.instruction(&Instruction::Br(relative));
                        }
                    }
                }

                self.depth -= 1;
                self.func.instruction(&Instruction::End);
            }

            IrStmtKind::Comment { .. } => {
                // No-op in WASM
            }

            IrStmtKind::BindDestructure { pattern, value } => {
                // Emit value (usually a tuple/record ptr)
                self.emit_expr(value);
                let scratch = self.match_i32_base + self.match_depth;
                self.func.instruction(&Instruction::LocalSet(scratch));

                // Destructure pattern
                if let crate::ir::IrPattern::Tuple { elements } = pattern {
                    let elem_types = if let crate::types::Ty::Tuple(tys) = &value.ty {
                        tys.clone()
                    } else { vec![] };

                    let mut offset = 0u32;
                    for (i, elem_pat) in elements.iter().enumerate() {
                        if let crate::ir::IrPattern::Bind { var } = elem_pat {
                            if let Some(&local_idx) = self.var_map.get(&var.0) {
                                let elem_ty = elem_types.get(i).cloned().unwrap_or(crate::types::Ty::Int);
                                self.func.instruction(&Instruction::LocalGet(scratch));
                                self.emit_load_at(&elem_ty, offset);
                                self.func.instruction(&Instruction::LocalSet(local_idx));
                                offset += super::values::byte_size(&elem_ty);
                            }
                        } else if let crate::ir::IrPattern::Wildcard = elem_pat {
                            let elem_ty = elem_types.get(i).cloned().unwrap_or(crate::types::Ty::Int);
                            offset += super::values::byte_size(&elem_ty);
                        }
                    }
                }
            }

            IrStmtKind::IndexAssign { .. }
            | IrStmtKind::FieldAssign { .. } => {
                // Phase 3+
            }
        }
    }
}

/// Result of pre-scanning a function body for local variables.
pub struct LocalScanResult {
    pub binds: Vec<(VarId, ValType)>,
    /// Max scratch local depth needed (match subjects + record temporaries).
    pub scratch_depth: usize,
}

/// Pre-scan a function body to collect all local variable bindings
/// and count scratch local depth.
pub fn collect_locals(body: &IrExpr, var_table: &crate::ir::VarTable) -> LocalScanResult {
    let mut binds = Vec::new();
    scan_expr(body, &mut binds, var_table);
    // Always at least 1 scratch level (used by record/variant construction)
    let scratch_depth = count_scratch_depth(body).max(1);
    LocalScanResult { binds, scratch_depth }
}

/// Count the maximum scratch local depth needed.
/// Match expressions and Record constructions each consume 1 level.
/// Public access to scratch depth counting (used by init_globals compilation).
pub fn count_scratch_depth_public(expr: &IrExpr) -> usize {
    count_scratch_depth(expr)
}

fn count_scratch_depth(expr: &IrExpr) -> usize {
    match &expr.kind {
        IrExprKind::Match { subject, arms } => {
            let inner = arms.iter()
                .map(|a| count_scratch_depth(&a.body))
                .max().unwrap_or(0);
            let subj = count_scratch_depth(subject);
            1 + inner.max(subj)
        }
        IrExprKind::Record { fields, .. } => {
            let inner = fields.iter()
                .map(|(_, e)| count_scratch_depth(e))
                .max().unwrap_or(0);
            1.max(inner)
        }
        IrExprKind::Block { stmts, expr } | IrExprKind::DoBlock { stmts, expr } => {
            let s = stmts.iter().map(|s| count_scratch_depth_stmt(s)).max().unwrap_or(0);
            let e = expr.as_ref().map(|e| count_scratch_depth(e)).unwrap_or(0);
            s.max(e)
        }
        IrExprKind::If { cond, then, else_ } => {
            count_scratch_depth(cond)
                .max(count_scratch_depth(then))
                .max(count_scratch_depth(else_))
        }
        IrExprKind::While { cond, body } => {
            let b = body.iter().map(|s| count_scratch_depth_stmt(s)).max().unwrap_or(0);
            count_scratch_depth(cond).max(b)
        }
        IrExprKind::BinOp { left, right, .. } => {
            count_scratch_depth(left).max(count_scratch_depth(right))
        }
        IrExprKind::UnOp { operand, .. } => count_scratch_depth(operand),
        IrExprKind::Call { args, target, .. } => {
            let base = match target {
                crate::ir::CallTarget::Computed { .. } => 1,
                _ => 0,
            };
            let inner = args.iter().map(|a| count_scratch_depth(a)).max().unwrap_or(0);
            base.max(inner)
        }
        // SpreadRecord needs 2 i32 scratch (result + base ptrs)
        IrExprKind::SpreadRecord { base, fields, .. } => {
            let inner = fields.iter().map(|(_, e)| count_scratch_depth(e)).max().unwrap_or(0);
            2.max(count_scratch_depth(base).max(inner))
        }
        // FnRef needs 1 scratch, Lambda with captures needs 2 (env + closure ptr)
        IrExprKind::FnRef { .. } => 1,
        IrExprKind::Lambda { .. } => 2,
        IrExprKind::ForIn { iterable, body, .. } => {
            let b = body.iter().map(|s| count_scratch_depth_stmt(s)).max().unwrap_or(0);
            // List for...in needs 2 scratch slots (list ptr + index counter)
            let for_in_need = match &iterable.kind {
                IrExprKind::Range { .. } => 0,
                _ => 2,
            };
            count_scratch_depth(iterable).max(b).max(for_in_need)
        }
        _ => 0,
    }
}

fn count_scratch_depth_stmt(stmt: &IrStmt) -> usize {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } => count_scratch_depth(value),
        IrStmtKind::Expr { expr } => count_scratch_depth(expr),
        IrStmtKind::Guard { cond, else_ } => count_scratch_depth(cond).max(count_scratch_depth(else_)),
        _ => 0,
    }
}

fn scan_expr(expr: &IrExpr, locals: &mut Vec<(VarId, ValType)>, vt: &crate::ir::VarTable) {
    match &expr.kind {
        IrExprKind::Block { stmts, expr } | IrExprKind::DoBlock { stmts, expr } => {
            for stmt in stmts { scan_stmt(stmt, locals, vt); }
            if let Some(e) = expr { scan_expr(e, locals, vt); }
        }
        IrExprKind::If { cond, then, else_ } => {
            scan_expr(cond, locals, vt);
            scan_expr(then, locals, vt);
            scan_expr(else_, locals, vt);
        }
        IrExprKind::While { cond, body } => {
            scan_expr(cond, locals, vt);
            for stmt in body { scan_stmt(stmt, locals, vt); }
        }
        IrExprKind::ForIn { var, var_tuple, body, iterable } => {
            let var_info = vt.get(*var);
            let val_type = values::ty_to_valtype(&var_info.ty).unwrap_or(ValType::I64);
            locals.push((*var, val_type));
            // Also register tuple destructure vars
            if let Some(tuple_vars) = var_tuple {
                for tv in tuple_vars {
                    let tv_info = vt.get(*tv);
                    let tv_type = values::ty_to_valtype(&tv_info.ty).unwrap_or(ValType::I64);
                    locals.push((*tv, tv_type));
                }
            }
            scan_expr(iterable, locals, vt);
            for stmt in body { scan_stmt(stmt, locals, vt); }
        }
        IrExprKind::BinOp { left, right, .. } => {
            scan_expr(left, locals, vt);
            scan_expr(right, locals, vt);
        }
        IrExprKind::UnOp { operand, .. } => scan_expr(operand, locals, vt),
        IrExprKind::Call { args, target, .. } => {
            if let crate::ir::CallTarget::Method { object, .. } = target { scan_expr(object, locals, vt); }
            if let crate::ir::CallTarget::Computed { callee } = target { scan_expr(callee, locals, vt); }
            for arg in args { scan_expr(arg, locals, vt); }
        }
        IrExprKind::Match { subject, arms } => {
            scan_expr(subject, locals, vt);
            for arm in arms {
                scan_pattern(&arm.pattern, &subject.ty, locals, vt);
                scan_expr(&arm.body, locals, vt);
            }
        }
        _ => {}
    }
}

fn scan_stmt(stmt: &IrStmt, locals: &mut Vec<(VarId, ValType)>, vt: &crate::ir::VarTable) {
    match &stmt.kind {
        IrStmtKind::Bind { var, ty, value, .. } => {
            if let Some(val_type) = values::ty_to_valtype(ty) {
                locals.push((*var, val_type));
            }
            scan_expr(value, locals, vt);
        }
        IrStmtKind::BindDestructure { pattern, value } => {
            // Collect vars from destructure pattern
            scan_destructure_pattern(pattern, &value.ty, locals, vt);
            scan_expr(value, locals, vt);
        }
        IrStmtKind::Assign { value, .. } => scan_expr(value, locals, vt),
        IrStmtKind::Expr { expr } => scan_expr(expr, locals, vt),
        IrStmtKind::Guard { cond, else_ } => {
            scan_expr(cond, locals, vt);
            scan_expr(else_, locals, vt);
        }
        _ => {}
    }
}

/// Scan a destructuring pattern (let (a, b) = ...) for variable bindings.
fn scan_destructure_pattern(pattern: &crate::ir::IrPattern, value_ty: &crate::types::Ty, locals: &mut Vec<(VarId, ValType)>, vt: &crate::ir::VarTable) {
    match pattern {
        crate::ir::IrPattern::Tuple { elements } => {
            let elem_types = if let crate::types::Ty::Tuple(tys) = value_ty { tys.clone() } else { vec![] };
            for (i, elem) in elements.iter().enumerate() {
                let elem_ty = elem_types.get(i).cloned().unwrap_or(crate::types::Ty::Int);
                scan_destructure_pattern(elem, &elem_ty, locals, vt);
            }
        }
        crate::ir::IrPattern::Bind { var } => {
            if let Some(val_type) = values::ty_to_valtype(value_ty) {
                locals.push((*var, val_type));
            }
        }
        _ => {}
    }
}

/// Scan a match pattern for variable bindings.
fn scan_pattern(pattern: &crate::ir::IrPattern, subject_ty: &crate::types::Ty, locals: &mut Vec<(VarId, ValType)>, vt: &crate::ir::VarTable) {
    match pattern {
        crate::ir::IrPattern::Bind { var } => {
            if let Some(val_type) = values::ty_to_valtype(subject_ty) {
                locals.push((*var, val_type));
            }
        }
        crate::ir::IrPattern::Constructor { args, .. } => {
            for arg in args { scan_pattern(arg, subject_ty, locals, vt); }
        }
        crate::ir::IrPattern::Tuple { elements } => {
            let elem_types = if let crate::types::Ty::Tuple(tys) = subject_ty { tys.clone() } else { vec![] };
            for (i, elem) in elements.iter().enumerate() {
                let et = elem_types.get(i).cloned().unwrap_or(subject_ty.clone());
                scan_pattern(elem, &et, locals, vt);
            }
        }
        crate::ir::IrPattern::Some { inner } | crate::ir::IrPattern::Ok { inner } => {
            let inner_ty = if let crate::types::Ty::Applied(_, args) = subject_ty {
                args.first().cloned().unwrap_or(subject_ty.clone())
            } else { subject_ty.clone() };
            scan_pattern(inner, &inner_ty, locals, vt);
        }
        crate::ir::IrPattern::Err { inner } => {
            // Err inner type is the second type arg: E in Result[T, E]
            let inner_ty = if let crate::types::Ty::Applied(_, args) = subject_ty {
                args.get(1).cloned().unwrap_or(subject_ty.clone())
            } else { subject_ty.clone() };
            scan_pattern(inner, &inner_ty, locals, vt);
        }
        crate::ir::IrPattern::RecordPattern { name: _, fields, .. } => {
            // For pattern=None fields, the binding is implicit (field name = var name).
            // The lowerer has already allocated VarIds for these in the VarTable.
            // We find them by searching VarTable for entries matching the field names.
            for field in fields {
                if let Some(pat) = &field.pattern {
                    scan_pattern(pat, subject_ty, locals, vt);
                } else {
                    // Implicit bind: find VarId by field name in VarTable
                    for i in 0..vt.len() {
                        let info = vt.get(crate::ir::VarId(i as u32));
                        if info.name == field.name {
                            if let Some(val_type) = values::ty_to_valtype(&info.ty) {
                                locals.push((crate::ir::VarId(i as u32), val_type));
                            }
                            break;
                        }
                    }
                }
            }
        }
        _ => {}
    }
}
