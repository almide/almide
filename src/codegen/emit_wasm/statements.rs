//! IrStmt → WASM instruction emission + local variable pre-scanning.

use crate::ir::{IrExpr, IrExprKind, IrStmt, IrStmtKind, VarId};
use wasm_encoder::ValType;

use super::FuncCompiler;
use super::values;

impl FuncCompiler<'_> {
    /// Emit a single IR statement.
    pub fn emit_stmt(&mut self, stmt: &IrStmt) {
        match &stmt.kind {
            IrStmtKind::Bind { var, ty, value, .. } => {
                let is_cell = self.emitter.mutable_captures.contains(&var.0);
                let effective_ty = if values::ty_to_valtype(ty) != values::ty_to_valtype(&value.ty)
                    && values::ty_to_valtype(&value.ty).is_some() {
                    &value.ty
                } else {
                    ty
                };
                if is_cell {
                    // Mutable capture: allocate heap cell, store value, local holds cell ptr
                    let cell_size = values::byte_size(effective_ty);
                    let local_idx = self.var_map[&var.0];
                    wasm!(self.func, {
                        i32_const(cell_size as i32);
                        call(self.emitter.rt.alloc);
                        local_set(local_idx);
                        local_get(local_idx);
                    });
                    self.emit_expr(value);
                    self.emit_store_at(effective_ty, 0);
                } else {
                    self.emit_expr(value);
                    if let Some(_vt) = values::ty_to_valtype(effective_ty) {
                        let local_idx = self.var_map[&var.0];
                        wasm!(self.func, { local_set(local_idx); });
                    }
                }
            }

            IrStmtKind::Assign { var, value } => {
                let is_cell = self.emitter.mutable_captures.contains(&var.0);
                let local_idx = self.var_map[&var.0];
                if is_cell {
                    // Cell: local holds ptr, store new value into cell
                    wasm!(self.func, { local_get(local_idx); });
                    self.emit_expr(value);
                    let ty = &self.var_table.get(*var).ty;
                    self.emit_store_at(ty, 0);
                } else {
                    self.emit_expr(value);
                    wasm!(self.func, { local_set(local_idx); });
                }
            }

            IrStmtKind::Expr { expr } => {
                self.emit_expr(expr);
                // Drop the value if the expression produces one
                if values::ty_to_valtype(&expr.ty).is_some() {
                    wasm!(self.func, { drop; });
                }
            }

            IrStmtKind::Guard { cond, else_ } => {
                // Guard: if cond is false, execute else_ action
                self.emit_expr(cond);
                wasm!(self.func, {
                    i32_eqz;
                    if_empty;
                });
                self.depth += 1;

                match &else_.kind {
                    // Break/Continue: emit directly (they generate the right br)
                    crate::ir::IrExprKind::Break | crate::ir::IrExprKind::Continue => {
                        self.emit_expr(else_);
                    }
                    // ResultOk/ResultErr in guard: return from function (effect fn early return)
                    crate::ir::IrExprKind::ResultOk { .. } | crate::ir::IrExprKind::ResultErr { .. } => {
                        self.emit_expr(else_);
                        wasm!(self.func, { return_; });
                    }
                    // Other expressions
                    _ => {
                        self.emit_expr(else_);
                        if let Some(labels) = self.loop_stack.last() {
                            // Inside a loop/do block: drop value and break
                            if super::values::ty_to_valtype(&else_.ty).is_some() {
                                wasm!(self.func, { drop; });
                            }
                            let relative = self.depth - labels.break_depth - 1;
                            wasm!(self.func, { br(relative); });
                        } else {
                            // Outside any loop: return the value from function
                            wasm!(self.func, { return_; });
                        }
                    }
                }

                self.depth -= 1;
                wasm!(self.func, { end; });
            }

            IrStmtKind::Comment { .. } => {
                // No-op in WASM
            }

            IrStmtKind::BindDestructure { pattern, value } => {
                // Emit value (usually a tuple/record ptr)
                self.emit_expr(value);
                let scratch = self.match_i32_base + self.match_depth;
                wasm!(self.func, { local_set(scratch); });

                // Destructure pattern
                if let crate::ir::IrPattern::Tuple { elements } = pattern {
                    let elem_types = if let crate::types::Ty::Tuple(tys) = &value.ty {
                        tys.clone()
                    } else { vec![] };

                    let mut offset = 0u32;
                    for (i, elem_pat) in elements.iter().enumerate() {
                        if let crate::ir::IrPattern::Bind { var, .. } = elem_pat {
                            if let Some(&local_idx) = self.var_map.get(&var.0) {
                                let elem_ty = elem_types.get(i).cloned().unwrap_or(crate::types::Ty::Int);
                                wasm!(self.func, { local_get(scratch); });
                                self.emit_load_at(&elem_ty, offset);
                                wasm!(self.func, { local_set(local_idx); });
                                offset += super::values::byte_size(&elem_ty);
                            }
                        } else if let crate::ir::IrPattern::Wildcard = elem_pat {
                            let elem_ty = elem_types.get(i).cloned().unwrap_or(crate::types::Ty::Int);
                            offset += super::values::byte_size(&elem_ty);
                        }
                    }
                }
            }

            IrStmtKind::IndexAssign { target, index, value } => {
                // xs[i] = v → store value at list_ptr + 4 + i * elem_size
                let elem_size = super::values::byte_size(&value.ty);
                // Compute address: target + 4 + index * elem_size
                if let Some(&local_idx) = self.var_map.get(&target.0) {
                    wasm!(self.func, {
                        local_get(local_idx); // list ptr
                        i32_const(4);
                        i32_add;
                    });
                    self.emit_expr(index);
                    if matches!(&index.ty, crate::types::Ty::Int) {
                        wasm!(self.func, { i32_wrap_i64; });
                    }
                    wasm!(self.func, {
                        i32_const(elem_size as i32);
                        i32_mul;
                        i32_add;
                    });
                    // Value
                    self.emit_expr(value);
                    self.emit_store_at(&value.ty, 0);
                }
            }
            IrStmtKind::FieldAssign { target, field, value } => {
                // record.field = value
                if let Some(&local_idx) = self.var_map.get(&target.0) {
                    let var_ty = &self.var_table.get(*target).ty;
                    let fields = self.extract_record_fields(var_ty);
                    let tag_offset = self.variant_tag_offset(var_ty);
                    if let Some((offset, _)) = super::values::field_offset(&fields, field) {
                        let total_offset = tag_offset + offset;
                        wasm!(self.func, { local_get(local_idx); });
                        self.emit_expr(value);
                        self.emit_store_at(&value.ty, total_offset);
                    }
                }
            }
            IrStmtKind::MapInsert { .. } => {
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
            1 + inner
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
        IrExprKind::BinOp { op, left, right } => {
            let inner = count_scratch_depth(left).max(count_scratch_depth(right));
            match op {
                crate::ir::BinOp::PowInt | crate::ir::BinOp::PowFloat => 4 + inner,
                // Eq/Neq: deep equality uses up to 3 scratch locals (list_eq_deep, option_eq_deep, etc.)
                crate::ir::BinOp::Eq | crate::ir::BinOp::Neq => 3 + inner,
                _ => inner,
            }
        }
        IrExprKind::UnOp { operand, .. } => count_scratch_depth(operand),
        IrExprKind::Call { args, target, .. } => {
            // Calls may use scratch: option/result ops (3), list ops (4), variant constructors (1), computed calls (1)
            let inner = args.iter().map(|a| count_scratch_depth(a)).max().unwrap_or(0);
            4 + inner
        }
        // SpreadRecord needs 2 i32 scratch (result + base ptrs) + 1 i64 scratch (copy counter)
        IrExprKind::SpreadRecord { base, fields, .. } => {
            let inner = fields.iter().map(|(_, e)| count_scratch_depth(e)).max().unwrap_or(0);
            2 + count_scratch_depth(base).max(inner)
        }
        // Option/Result construction: 1 scratch + inner (scratch held while emitting inner expr)
        IrExprKind::OptionSome { expr } => 1 + count_scratch_depth(expr),
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr } => 1 + count_scratch_depth(expr),
        IrExprKind::EmptyMap => 1,
        IrExprKind::MapLiteral { entries } => {
            let inner = entries.iter()
                .flat_map(|(k, v)| [count_scratch_depth(k), count_scratch_depth(v)])
                .max().unwrap_or(0);
            1 + inner
        }
        IrExprKind::List { elements } => {
            let inner = elements.iter().map(|e| count_scratch_depth(e)).max().unwrap_or(0);
            1 + inner
        }
        IrExprKind::Tuple { elements } => {
            let inner = elements.iter().map(|e| count_scratch_depth(e)).max().unwrap_or(0);
            1 + inner
        }
        // FnRef needs 1 scratch, Lambda with captures needs 2 (env + closure ptr)
        IrExprKind::FnRef { .. } => 1,
        IrExprKind::Lambda { .. } => 2,
        IrExprKind::Try { expr } => 1 + count_scratch_depth(expr),
        IrExprKind::ForIn { iterable, body, .. } => {
            let b = body.iter().map(|s| count_scratch_depth_stmt(s)).max().unwrap_or(0);
            // List for...in reserves 2 scratch slots (list ptr + index counter)
            // via match_depth += 2 BEFORE emitting iterable and body,
            // so both iterable and body scratch are additive to the 2 reserved slots.
            let for_in_need = match &iterable.kind {
                IrExprKind::Range { .. } => 0,
                _ => 2,
            };
            for_in_need + count_scratch_depth(iterable).max(b)
        }
        IrExprKind::StringInterp { parts } => {
            parts.iter().map(|p| match p {
                crate::ir::IrStringPart::Expr { expr } => count_scratch_depth(expr),
                _ => 0,
            }).max().unwrap_or(0)
        }
        IrExprKind::Clone { expr } | IrExprKind::Deref { expr } => count_scratch_depth(expr),
        IrExprKind::Member { object, .. } | IrExprKind::IndexAccess { object, .. }
        | IrExprKind::TupleIndex { object, .. } => count_scratch_depth(object),
        _ => 0,
    }
}

fn count_scratch_depth_stmt(stmt: &IrStmt) -> usize {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } => count_scratch_depth(value),
        IrStmtKind::BindDestructure { value, .. } => 1 + count_scratch_depth(value),
        IrStmtKind::Expr { expr } => count_scratch_depth(expr),
        IrStmtKind::Guard { cond, else_ } => count_scratch_depth(cond).max(count_scratch_depth(else_)),
        IrStmtKind::IndexAssign { index, value, .. } => count_scratch_depth(index).max(count_scratch_depth(value)),
        IrStmtKind::FieldAssign { value, .. } => count_scratch_depth(value),
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
            // Determine loop variable type from iterable's element type (more reliable than VarTable)
            let elem_ty = match &iterable.ty {
                crate::types::Ty::Applied(crate::types::TypeConstructorId::List, args) if args.len() == 1 => args[0].clone(),
                crate::types::Ty::Applied(crate::types::TypeConstructorId::Map, args) if args.len() == 2 =>
                    crate::types::Ty::Tuple(vec![args[0].clone(), args[1].clone()]),
                _ => vt.get(*var).ty.clone(),
            };
            let val_type = values::ty_to_valtype(&elem_ty).unwrap_or(ValType::I64);
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
            let resolved_ty = resolve_scan_subject_ty(subject, arms, vt);
            // Log: find the match with Right/Left constructors
            let has_either = arms.iter().any(|a| matches!(&a.pattern, crate::ir::IrPattern::Constructor { name, .. } if name == "Right" || name == "Left"));
            if has_either {
                eprintln!("[SCAN MATCH EITHER] subject.ty={:?} resolved_ty={:?}", subject.ty, resolved_ty);
            }
            for arm in arms {
                scan_pattern(&arm.pattern, &resolved_ty, locals, vt);
                scan_expr(&arm.body, locals, vt);
            }
        }
        IrExprKind::StringInterp { parts } => {
            for part in parts {
                if let crate::ir::IrStringPart::Expr { expr } = part {
                    scan_expr(expr, locals, vt);
                }
            }
        }
        IrExprKind::OptionSome { expr } | IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::Clone { expr } | IrExprKind::Deref { expr } | IrExprKind::Try { expr } => {
            scan_expr(expr, locals, vt);
        }
        IrExprKind::Record { fields, .. } | IrExprKind::SpreadRecord { fields, .. } => {
            for (_, e) in fields { scan_expr(e, locals, vt); }
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } | IrExprKind::Fan { exprs: elements } => {
            for e in elements { scan_expr(e, locals, vt); }
        }
        IrExprKind::Member { object, .. } | IrExprKind::IndexAccess { object, .. } => {
            scan_expr(object, locals, vt);
        }
        IrExprKind::Lambda { body, .. } => {
            scan_expr(body, locals, vt);
        }
        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries { scan_expr(k, locals, vt); scan_expr(v, locals, vt); }
        }
        _ => {}
    }
}

fn scan_stmt(stmt: &IrStmt, locals: &mut Vec<(VarId, ValType)>, vt: &crate::ir::VarTable) {
    match &stmt.kind {
        IrStmtKind::Bind { var, ty, value, .. } => {
            let val_type = values::ty_to_valtype(&value.ty)
                .or_else(|| values::ty_to_valtype(ty));
            if let Some(vt_wasm) = val_type {
                locals.push((*var, vt_wasm));
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

/// Resolve match subject type, fixing IR type inference gaps.
fn resolve_scan_subject_ty(subject: &IrExpr, arms: &[crate::ir::IrMatchArm], vt: &crate::ir::VarTable) -> crate::types::Ty {
    let has_container = arms.iter().any(|a| matches!(
        &a.pattern,
        crate::ir::IrPattern::Ok { .. } | crate::ir::IrPattern::Err { .. }
        | crate::ir::IrPattern::Some { .. } | crate::ir::IrPattern::None
    ));
    if has_container && !matches!(&subject.ty, crate::types::Ty::Applied(_, _)) {
        if let IrExprKind::Var { id } = &subject.kind {
            let info = vt.get(*id);
            if matches!(&info.ty, crate::types::Ty::Applied(_, _)) {
                return info.ty.clone();
            }
        }
    }
    subject.ty.clone()
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
        crate::ir::IrPattern::Bind { var, .. } => {
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
        crate::ir::IrPattern::Bind { var, ty } => {
            // Use pattern's own type (set by lowering, updated by mono) — no VarTable dependency
            let effective_ty = if matches!(ty, crate::types::Ty::Unknown) { subject_ty } else { ty };
            if let Some(val_type) = values::ty_to_valtype(effective_ty) {
                locals.push((*var, val_type));
            }
        }
        crate::ir::IrPattern::Constructor { name: ctor_name, args } => {
            // Resolve field types from subject_ty's type_args for generic variants
            let subject_type_args: Vec<crate::types::Ty> = match subject_ty {
                crate::types::Ty::Named(_, args) if !args.is_empty() => args.clone(),
                crate::types::Ty::Applied(_, args) if !args.is_empty() => args.clone(),
                crate::types::Ty::Variant { .. } => {
                    // Use pattern.ty (set by mono substitute_pattern_types) — no VarTable
                    for arg in args.iter() {
                        if let crate::ir::IrPattern::Bind { var, ty } = arg {
                            let effective_ty = if matches!(ty, crate::types::Ty::Unknown | crate::types::Ty::TypeVar(_)) {
                                &vt.get(*var).ty // fallback only
                            } else { ty };
                            if let Some(val_type) = values::ty_to_valtype(effective_ty) {
                                locals.push((*var, val_type));
                            }
                        }
                    }
                    return;
                }
                _ => vec![],
            };
            for (i, arg) in args.iter().enumerate() {
                if let crate::ir::IrPattern::Bind { var, ty: pat_ty } = arg {
                    // Use pattern.ty first (set by mono), fall back to VarTable + substitution
                    let resolved = if !matches!(pat_ty, crate::types::Ty::Unknown | crate::types::Ty::TypeVar(_))
                        && !matches!(pat_ty, crate::types::Ty::Named(n, a) if a.is_empty() && n.len() <= 2 && n.chars().next().map_or(false, |c| c.is_uppercase()))
                    {
                        pat_ty.clone()
                    } else if !subject_type_args.is_empty() {
                        let var_ty = vt.get(*var).ty.clone();
                        let mut gnames = Vec::new();
                        super::expressions::collect_type_param_names(&var_ty, &mut gnames);
                        if gnames.is_empty() { var_ty } else {
                            super::expressions::substitute_type_params(&var_ty, &gnames, &subject_type_args)
                        }
                    } else { vt.get(*var).ty.clone() };
                    if let Some(val_type) = values::ty_to_valtype(&resolved) {
                        locals.push((*var, val_type));
                    }
                } else {
                    scan_pattern(arg, subject_ty, locals, vt);
                }
            }
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
            } else {
                // subject_ty is not Applied — try VarTable for inner binding
                if let crate::ir::IrPattern::Bind { var, .. } = inner.as_ref() {
                    let vt_ty = &vt.get(*var).ty;
                    if !matches!(vt_ty, crate::types::Ty::Unknown | crate::types::Ty::TypeVar(_)) {
                        vt_ty.clone()
                    } else { subject_ty.clone() }
                } else { subject_ty.clone() }
            };
            scan_pattern(inner, &inner_ty, locals, vt);
        }
        crate::ir::IrPattern::Err { inner } => {
            let inner_ty = if let crate::types::Ty::Applied(_, args) = subject_ty {
                args.get(1).cloned().unwrap_or(subject_ty.clone())
            } else {
                if let crate::ir::IrPattern::Bind { var, .. } = inner.as_ref() {
                    let vt_ty = &vt.get(*var).ty;
                    if !matches!(vt_ty, crate::types::Ty::Unknown | crate::types::Ty::TypeVar(_)) {
                        vt_ty.clone()
                    } else { subject_ty.clone() }
                } else { subject_ty.clone() }
            };
            scan_pattern(inner, &inner_ty, locals, vt);
        }
        crate::ir::IrPattern::RecordPattern { name: _, fields, .. } => {
            // For pattern=None fields, the binding is implicit (field name = var name).
            // The lowerer has already allocated VarIds for these in the VarTable.
            // Search from the END of VarTable to find the most recent (correct scope) VarId,
            // and skip VarIds already registered in locals to avoid duplicates.
            let existing_ids: std::collections::HashSet<u32> = locals.iter().map(|(v, _)| v.0).collect();
            for field in fields {
                if let Some(pat) = &field.pattern {
                    scan_pattern(pat, subject_ty, locals, vt);
                } else {
                    // Implicit bind: find VarId by field name, searching from end (most recent scope)
                    for i in (0..vt.len()).rev() {
                        let info = vt.get(crate::ir::VarId(i as u32));
                        if info.name == field.name && !existing_ids.contains(&(i as u32)) {
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
