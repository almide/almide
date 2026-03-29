//! IrStmt → WASM instruction emission + local variable pre-scanning.

use crate::ir::{IrExpr, IrExprKind, IrStmt, IrStmtKind, VarId};
use crate::types::Ty;
use wasm_encoder::ValType;

use super::FuncCompiler;
use super::values;

impl FuncCompiler<'_> {
    /// Get the element type of a list variable from VarTable.
    fn list_elem_ty_var(&self, var: VarId) -> Ty {
        self.list_elem_ty(&self.var_table.get(var).ty)
    }

    /// Allocate a scratch local appropriate for the given type.
    fn scratch_for_ty(&mut self, ty: &Ty) -> u32 {
        match values::ty_to_valtype(ty) {
            Some(ValType::I64) => self.scratch.alloc_i64(),
            Some(ValType::F64) => self.scratch.alloc_f64(),
            _ => self.scratch.alloc_i32(),
        }
    }

    /// Free a scratch local allocated by scratch_for_ty.
    fn free_scratch_for_ty(&mut self, idx: u32, ty: &Ty) {
        match values::ty_to_valtype(ty) {
            Some(ValType::I64) => self.scratch.free_i64(idx),
            Some(ValType::F64) => self.scratch.free_f64(idx),
            _ => self.scratch.free_i32(idx),
        }
    }

    /// Set a scratch local from the stack.
    fn emit_set_scratch(&mut self, idx: u32, _ty: &Ty) {
        wasm!(self.func, { local_set(idx); });
    }

    /// Get a scratch local onto the stack.
    fn emit_get_scratch(&mut self, idx: u32, _ty: &Ty) {
        wasm!(self.func, { local_get(idx); });
    }
}

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
                let _g = self.depth_push();

                match &else_.kind {
                    // Break/Continue: emit directly (they generate the right br)
                    crate::ir::IrExprKind::Break | crate::ir::IrExprKind::Continue => {
                        self.emit_expr(else_);
                    }
                    // ResultOk/ResultErr in guard
                    crate::ir::IrExprKind::ResultOk { expr: inner } | crate::ir::IrExprKind::ResultErr { expr: inner } => {
                        // ok(()) inside loop → break out of loop (not function return)
                        let is_unit_ok = matches!(&else_.kind, crate::ir::IrExprKind::ResultOk { .. })
                            && matches!(&inner.ty, crate::types::Ty::Unit);
                        if is_unit_ok && self.loop_stack.last().is_some() {
                            // Emit the ok(()) but then break out of the loop
                            self.emit_expr(else_);
                            if super::values::ty_to_valtype(&else_.ty).is_some() {
                                wasm!(self.func, { drop; });
                            }
                            let labels = self.loop_stack.last().unwrap();
                            let relative = self.depth - labels.break_depth - 1;
                            wasm!(self.func, { br(relative); });
                        } else {
                            // Non-unit ok/err → return from function
                            self.emit_expr(else_);
                            wasm!(self.func, { return_; });
                        }
                    }
                    // Other expressions
                    _ => {
                        self.emit_expr(else_);
                        if let Some(labels) = self.loop_stack.last() {
                            // Inside a loop: drop value and break
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

                self.depth_pop(_g);
                wasm!(self.func, { end; });
            }

            IrStmtKind::Comment { .. } => {
                // No-op in WASM
            }

            IrStmtKind::BindDestructure { pattern, value } => {
                self.emit_expr(value);
                let scratch = self.scratch.alloc_i32();
                wasm!(self.func, { local_set(scratch); });

                // Destructure pattern
                match pattern {
                    crate::ir::IrPattern::Tuple { elements } => {
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
                    crate::ir::IrPattern::RecordPattern { fields: pat_fields, .. } => {
                        // Record destructure: load each field from record ptr at its offset.
                        // Field order and types come from the value's type.
                        let record_fields = self.extract_record_fields(&value.ty);
                        for pf in pat_fields {
                            if let Some((offset, field_ty)) = super::values::field_offset(&record_fields, &pf.name) {
                                // find_var_by_field searches var_map by name
                                if let Some(&local_idx) = self.find_var_by_field(&pf.name, &record_fields) {
                                    wasm!(self.func, { local_get(scratch); });
                                    self.emit_load_at(&field_ty, offset);
                                    wasm!(self.func, { local_set(local_idx); });
                                }
                            }
                        }
                    }
                    _ => {}
                }
                self.scratch.free_i32(scratch);
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
            IrStmtKind::ListSwap { target, a, b } => {
                // xs.swap(a, b): swap elements at indices a and b
                // Layout: list_ptr + 4 + idx * elem_size
                let elem_ty = self.list_elem_ty_var(*target);
                let elem_size = values::byte_size(&elem_ty) as i32;
                if let Some(&list_local) = self.var_map.get(&target.0) {
                    let addr_a = self.scratch.alloc_i32();
                    let addr_b = self.scratch.alloc_i32();
                    let tmp = self.scratch_for_ty(&elem_ty);

                    // addr_a = list_ptr + 4 + a * elem_size
                    wasm!(self.func, { local_get(list_local); i32_const(4); i32_add; });
                    self.emit_expr(a);
                    if matches!(&a.ty, Ty::Int) { wasm!(self.func, { i32_wrap_i64; }); }
                    wasm!(self.func, { i32_const(elem_size); i32_mul; i32_add; local_set(addr_a); });

                    // addr_b = list_ptr + 4 + b * elem_size
                    wasm!(self.func, { local_get(list_local); i32_const(4); i32_add; });
                    self.emit_expr(b);
                    if matches!(&b.ty, Ty::Int) { wasm!(self.func, { i32_wrap_i64; }); }
                    wasm!(self.func, { i32_const(elem_size); i32_mul; i32_add; local_set(addr_b); });

                    // tmp = *addr_a
                    wasm!(self.func, { local_get(addr_a); });
                    self.emit_load_at(&elem_ty, 0);
                    self.emit_set_scratch(tmp, &elem_ty);

                    // *addr_a = *addr_b
                    wasm!(self.func, { local_get(addr_a); local_get(addr_b); });
                    self.emit_load_at(&elem_ty, 0);
                    self.emit_store_at(&elem_ty, 0);

                    // *addr_b = tmp
                    wasm!(self.func, { local_get(addr_b); });
                    self.emit_get_scratch(tmp, &elem_ty);
                    self.emit_store_at(&elem_ty, 0);

                    self.scratch.free_i32(addr_a);
                    self.scratch.free_i32(addr_b);
                    self.free_scratch_for_ty(tmp, &elem_ty);
                }
            }
            IrStmtKind::ListReverse { target, end } => {
                // xs[..=end].reverse(): swap from both ends inward
                let elem_ty = self.list_elem_ty_var(*target);
                let elem_size = values::byte_size(&elem_ty) as i32;
                if let Some(&list_local) = self.var_map.get(&target.0) {
                    let lo = self.scratch.alloc_i32();
                    let hi = self.scratch.alloc_i32();
                    let addr_lo = self.scratch.alloc_i32();
                    let addr_hi = self.scratch.alloc_i32();
                    let tmp = self.scratch_for_ty(&elem_ty);

                    // lo = 0; hi = end (as i32)
                    wasm!(self.func, { i32_const(0); local_set(lo); });
                    self.emit_expr(end);
                    if matches!(&end.ty, Ty::Int) { wasm!(self.func, { i32_wrap_i64; }); }
                    wasm!(self.func, { local_set(hi); });

                    // while (lo < hi)
                    let base_ptr = self.scratch.alloc_i32();
                    wasm!(self.func, { local_get(list_local); i32_const(4); i32_add; local_set(base_ptr); });
                    wasm!(self.func, {
                        block_empty;
                        loop_empty;
                        // break if lo >= hi
                        local_get(lo); local_get(hi); i32_ge_s; br_if(1);
                        // addr_lo = base + lo * elem_size
                        local_get(base_ptr); local_get(lo); i32_const(elem_size); i32_mul; i32_add; local_set(addr_lo);
                        // addr_hi = base + hi * elem_size
                        local_get(base_ptr); local_get(hi); i32_const(elem_size); i32_mul; i32_add; local_set(addr_hi);
                    });
                    // tmp = *addr_lo
                    wasm!(self.func, { local_get(addr_lo); });
                    self.emit_load_at(&elem_ty, 0);
                    self.emit_set_scratch(tmp, &elem_ty);
                    // *addr_lo = *addr_hi
                    wasm!(self.func, { local_get(addr_lo); local_get(addr_hi); });
                    self.emit_load_at(&elem_ty, 0);
                    self.emit_store_at(&elem_ty, 0);
                    // *addr_hi = tmp
                    wasm!(self.func, { local_get(addr_hi); });
                    self.emit_get_scratch(tmp, &elem_ty);
                    self.emit_store_at(&elem_ty, 0);
                    // lo++; hi--
                    wasm!(self.func, {
                        local_get(lo); i32_const(1); i32_add; local_set(lo);
                        local_get(hi); i32_const(1); i32_sub; local_set(hi);
                        br(0);
                        end; // loop
                        end; // block
                    });

                    self.scratch.free_i32(base_ptr);
                    self.scratch.free_i32(lo);
                    self.scratch.free_i32(hi);
                    self.scratch.free_i32(addr_lo);
                    self.scratch.free_i32(addr_hi);
                    self.free_scratch_for_ty(tmp, &elem_ty);
                }
            }
            IrStmtKind::ListRotateLeft { target, end } => {
                // xs[..=end].rotate_left(1): save xs[0], shift left, put saved at end
                let elem_ty = self.list_elem_ty_var(*target);
                let elem_size = values::byte_size(&elem_ty) as i32;
                if let Some(&list_local) = self.var_map.get(&target.0) {
                    let tmp = self.scratch_for_ty(&elem_ty);
                    let base = self.scratch.alloc_i32();
                    let end_i32 = self.scratch.alloc_i32();

                    wasm!(self.func, { local_get(list_local); i32_const(4); i32_add; local_set(base); });
                    self.emit_expr(end);
                    if matches!(&end.ty, Ty::Int) { wasm!(self.func, { i32_wrap_i64; }); }
                    wasm!(self.func, { local_set(end_i32); });

                    // tmp = xs[0]
                    wasm!(self.func, { local_get(base); });
                    self.emit_load_at(&elem_ty, 0);
                    self.emit_set_scratch(tmp, &elem_ty);

                    // memory.copy: dst=base, src=base+elem_size, len=end*elem_size
                    wasm!(self.func, {
                        local_get(base);
                        local_get(base); i32_const(elem_size); i32_add;
                        local_get(end_i32); i32_const(elem_size); i32_mul;
                        memory_copy;
                    });

                    // xs[end] = tmp
                    wasm!(self.func, { local_get(base); local_get(end_i32); i32_const(elem_size); i32_mul; i32_add; });
                    self.emit_get_scratch(tmp, &elem_ty);
                    self.emit_store_at(&elem_ty, 0);

                    self.free_scratch_for_ty(tmp, &elem_ty);
                    self.scratch.free_i32(base);
                    self.scratch.free_i32(end_i32);
                }
            }
            IrStmtKind::ListCopySlice { dst, src, len } => {
                // dst[..n].copy_from_slice(&src[..n])
                if let (Some(&dst_local), Some(&src_local)) = (self.var_map.get(&dst.0), self.var_map.get(&src.0)) {
                    let elem_ty = self.list_elem_ty_var(*dst);
                    let elem_size = values::byte_size(&elem_ty) as i32;

                    // memory.copy: dst=dst_ptr+4, src=src_ptr+4, len=n*elem_size
                    wasm!(self.func, {
                        local_get(dst_local); i32_const(4); i32_add;
                        local_get(src_local); i32_const(4); i32_add;
                    });
                    self.emit_expr(len);
                    if matches!(&len.ty, Ty::Int) { wasm!(self.func, { i32_wrap_i64; }); }
                    wasm!(self.func, {
                        i32_const(elem_size); i32_mul;
                        memory_copy;
                    });
                }
            }
            IrStmtKind::MapInsert { target, key, value } => {
                // m[k] = v  →  target = map.set(target, key, value)
                if let Some(&local_idx) = self.var_map.get(&target.0) {
                    // Emit map.set(target, key, value) using the existing map call infrastructure
                    let set_args = vec![
                        crate::ir::IrExpr { kind: crate::ir::IrExprKind::Var { id: *target }, ty: self.var_table.get(*target).ty.clone(), span: None },
                        key.clone(),
                        value.clone(),
                    ];
                    self.emit_map_call("set", &set_args);
                    wasm!(self.func, { local_set(local_idx); });
                }
            }
        }
    }
}

/// Infer the type of a bind value from its IR expression structure.
/// Used when value.ty and stmt ty are both Unknown.
fn infer_bind_type(expr: &IrExpr) -> Ty {
    match &expr.kind {
        IrExprKind::LitInt { .. } => Ty::Int,
        IrExprKind::LitFloat { .. } => Ty::Float,
        IrExprKind::LitBool { .. } => Ty::Bool,
        IrExprKind::LitStr { .. } => Ty::String,
        // TupleIndex: infer from parent tuple type
        IrExprKind::TupleIndex { object, index } => {
            if let Ty::Tuple(elems) = &object.ty {
                elems.get(*index).cloned().unwrap_or(Ty::Unknown)
            } else {
                Ty::Unknown
            }
        }
        // BinOp: infer from operation kind
        IrExprKind::BinOp { op, .. } => {
            use crate::ir::BinOp;
            match op {
                BinOp::AddInt | BinOp::SubInt | BinOp::MulInt | BinOp::DivInt
                | BinOp::ModInt | BinOp::PowInt => Ty::Int,
                BinOp::AddFloat | BinOp::SubFloat | BinOp::MulFloat | BinOp::DivFloat
                | BinOp::ModFloat | BinOp::PowFloat => Ty::Float,
                BinOp::ConcatStr => Ty::String,
                BinOp::MulMatrix | BinOp::AddMatrix | BinOp::SubMatrix | BinOp::ScaleMatrix => Ty::Matrix,
                BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte
                | BinOp::And | BinOp::Or => Ty::Bool,
                BinOp::ConcatList => Ty::Unknown,
            }
        }
        // Try/Unwrap/ToOption: unwrap inner type
        IrExprKind::Try { expr: inner }
        | IrExprKind::Unwrap { expr: inner }
        | IrExprKind::ToOption { expr: inner } => {
            infer_bind_type(inner)
        }
        IrExprKind::UnwrapOr { expr: inner, .. } => {
            infer_bind_type(inner)
        }
        // Call: infer return type from module+func name
        IrExprKind::Call { target, .. } => {
            match target {
                crate::ir::CallTarget::Module { module, func } => {
                    match (module.as_str(), func.as_str()) {
                        ("random", "int") | ("datetime", _)
                        | ("env", "unix_timestamp") | ("env", "millis")
                        | ("list", "len") | ("string", "len") | ("map", "len") => Ty::Int,
                        ("random", "float") => Ty::Float,
                        _ => Ty::Unknown,
                    }
                }
                _ => Ty::Unknown,
            }
        }
        _ => Ty::Unknown,
    }
}

/// Result of pre-scanning a function body for local variables.
pub struct LocalScanResult {
    pub binds: Vec<(VarId, ValType)>,
}

/// Pre-scan a function body to collect all local variable bindings
/// and count scratch local depth.
pub fn collect_locals(body: &IrExpr, var_table: &crate::ir::VarTable) -> LocalScanResult {
    let mut binds = Vec::new();
    scan_expr(body, &mut binds, var_table);
    LocalScanResult { binds }
}

fn scan_expr(expr: &IrExpr, locals: &mut Vec<(VarId, ValType)>, vt: &crate::ir::VarTable) {
    match &expr.kind {
        IrExprKind::Block { stmts, expr } => {
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
        | IrExprKind::Clone { expr } | IrExprKind::Deref { expr } | IrExprKind::Try { expr }
        | IrExprKind::Unwrap { expr } | IrExprKind::ToOption { expr }
        | IrExprKind::OptionalChain { expr, .. } => {
            scan_expr(expr, locals, vt);
        }
        IrExprKind::UnwrapOr { expr, fallback } => {
            scan_expr(expr, locals, vt);
            scan_expr(fallback, locals, vt);
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
        IrExprKind::MapAccess { object, key } => {
            scan_expr(object, locals, vt);
            scan_expr(key, locals, vt);
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
            // Resolve bind type.
            // For Try(Call(...)) (effect fn unwrap), use the Result's inner type, not Result itself.
            let effective_ty = if let IrExprKind::Try { expr: inner }
                | IrExprKind::Unwrap { expr: inner }
                | IrExprKind::ToOption { expr: inner } = &value.kind {
                if let Ty::Applied(crate::types::constructor::TypeConstructorId::Result, args) = &value.ty {
                    args.first().cloned().unwrap_or(value.ty.clone())
                } else if let Ty::Applied(crate::types::constructor::TypeConstructorId::Result, args) = &inner.ty {
                    args.first().cloned().unwrap_or(value.ty.clone())
                } else {
                    value.ty.clone()
                }
            } else {
                value.ty.clone()
            };
            let resolved_ty = if !matches!(&effective_ty, Ty::Unknown | Ty::TypeVar(_)) {
                effective_ty
            } else if !matches!(ty, Ty::Unknown | Ty::TypeVar(_)) {
                ty.clone()
            } else {
                infer_bind_type(value)
            };
            let val_type = values::ty_to_valtype(&resolved_ty);
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
        crate::ir::IrPattern::RecordPattern { fields, .. } => {
            // Record destructure: resolve field types from value_ty (authoritative).
            let record_fields: Vec<(String, crate::types::Ty)> = match value_ty {
                crate::types::Ty::Record { fields } => fields.iter().map(|(n, t)| (n.to_string(), t.clone())).collect(),
                crate::types::Ty::OpenRecord { fields } => fields.iter().map(|(n, t)| (n.to_string(), t.clone())).collect(),
                _ => vec![],
            };
            let existing_ids: std::collections::HashSet<u32> = locals.iter().map(|(v, _)| v.0).collect();
            for field in fields {
                // Resolve field type from the record type (not VarTable -- VarTable may have stale types)
                let field_ty = record_fields.iter()
                    .find(|(n, _)| n == &field.name)
                    .map(|(_, t)| t.clone())
                    .unwrap_or(crate::types::Ty::Int);
                if let Some(pat) = &field.pattern {
                    scan_destructure_pattern(pat, &field_ty, locals, vt);
                } else {
                    // Implicit bind: field name = var name. Look up VarId from VarTable.
                    // Use field_ty from the record type for the WASM local declaration.
                    for i in (0..vt.len()).rev() {
                        let info = vt.get(crate::ir::VarId(i as u32));
                        if info.name == field.name && !existing_ids.contains(&(i as u32)) {
                            if let Some(val_type) = values::ty_to_valtype(&field_ty) {
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
        crate::ir::IrPattern::Constructor { name: _ctor_name, args } => {
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
            for (_i, arg) in args.iter().enumerate() {
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
