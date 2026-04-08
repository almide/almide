//! IrStmt → WASM instruction emission + local variable pre-scanning.

use std::collections::HashMap;

use almide_ir::{IrExpr, IrExprKind, IrStmt, IrStmtKind, VarId};
use almide_ir::visit::{IrVisitor, walk_expr, walk_stmt};
use almide_lang::types::Ty;
use wasm_encoder::ValType;

use super::FuncCompiler;
use super::VariantCase;
use super::equality::extract_record_fields;
use super::values;

/// Lookup for record/variant field types by nominal name.
/// Used during pre-scan to resolve `Ty::Named` to concrete field types
/// so that destructuring patterns allocate the correct WASM local valtypes.
pub(super) type RecordFieldLookup = HashMap<String, Vec<(String, Ty)>>;

/// Lookup for variant type info by nominal name.
pub(super) type VariantInfoLookup = HashMap<String, Vec<VariantCase>>;

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
                let local_idx = match self.var_map.get(&var.0) {
                    Some(&idx) => idx,
                    None => {
                        // Variable not in local scope — skip (closure captures handled at closure level)
                        self.emit_expr(value);
                        if values::ty_to_valtype(&value.ty).is_some() {
                            wasm!(self.func, { drop; });
                        }
                        return;
                    }
                };
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
                    almide_ir::IrExprKind::Break | almide_ir::IrExprKind::Continue => {
                        self.emit_expr(else_);
                    }
                    // ResultOk/ResultErr in guard
                    almide_ir::IrExprKind::ResultOk { expr: inner } | almide_ir::IrExprKind::ResultErr { expr: inner } => {
                        // ok(()) inside loop → break out of loop (not function return)
                        let is_unit_ok = matches!(&else_.kind, almide_ir::IrExprKind::ResultOk { .. })
                            && matches!(&inner.ty, almide_lang::types::Ty::Unit);
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
                    almide_ir::IrPattern::Tuple { elements } => {
                        let elem_types = if let almide_lang::types::Ty::Tuple(tys) = &value.ty {
                            tys.clone()
                        } else { vec![] };

                        let mut offset = 0u32;
                        for (i, elem_pat) in elements.iter().enumerate() {
                            if let almide_ir::IrPattern::Bind { var, .. } = elem_pat {
                                if let Some(&local_idx) = self.var_map.get(&var.0) {
                                    let elem_ty = elem_types.get(i).cloned().unwrap_or(almide_lang::types::Ty::Int);
                                    wasm!(self.func, { local_get(scratch); });
                                    self.emit_load_at(&elem_ty, offset);
                                    wasm!(self.func, { local_set(local_idx); });
                                    offset += super::values::byte_size(&elem_ty);
                                }
                            } else if let almide_ir::IrPattern::Wildcard = elem_pat {
                                let elem_ty = elem_types.get(i).cloned().unwrap_or(almide_lang::types::Ty::Int);
                                offset += super::values::byte_size(&elem_ty);
                            }
                        }
                    }
                    almide_ir::IrPattern::RecordPattern { fields: pat_fields, .. } => {
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
                if let Some(&local_idx) = self.var_map.get(&target.0) {
                    // Optimize constant index: compute offset at compile time
                    if let IrExprKind::LitInt { value: idx_val } = &index.kind {
                        let offset = 4 + (*idx_val as u32) * (elem_size as u32);
                        wasm!(self.func, { local_get(local_idx); });
                        self.emit_expr(value);
                        self.emit_store_at(&value.ty, offset);
                    } else {
                        wasm!(self.func, {
                            local_get(local_idx);
                            i32_const(4);
                            i32_add;
                        });
                        self.emit_expr(index);
                        if matches!(&index.ty, almide_lang::types::Ty::Int) {
                            wasm!(self.func, { i32_wrap_i64; });
                        }
                        wasm!(self.func, {
                            i32_const(elem_size as i32);
                            i32_mul;
                            i32_add;
                        });
                        self.emit_expr(value);
                        self.emit_store_at(&value.ty, 0);
                    }
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
                // Optimized: use shl for power-of-2 elem sizes, local.tee for addr reuse
                let elem_ty = self.list_elem_ty_var(*target);
                let elem_size = values::byte_size(&elem_ty) as i32;
                let elem_shift = (elem_size as u32).trailing_zeros();
                let use_shift = (elem_size as u32).is_power_of_two() && elem_shift > 0;
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

                    let base_ptr = self.scratch.alloc_i32();
                    wasm!(self.func, { local_get(list_local); i32_const(4); i32_add; local_set(base_ptr); });
                    wasm!(self.func, {
                        block_empty;
                        loop_empty;
                        local_get(lo); local_get(hi); i32_ge_s; br_if(1);
                    });
                    // addr_lo = base + lo << shift (using local.tee)
                    wasm!(self.func, { local_get(base_ptr); local_get(lo); });
                    if use_shift {
                        wasm!(self.func, { i32_const(elem_shift as i32); i32_shl; });
                    } else {
                        wasm!(self.func, { i32_const(elem_size); i32_mul; });
                    }
                    wasm!(self.func, { i32_add; local_tee(addr_lo); });
                    // addr_hi = base + hi << shift
                    wasm!(self.func, { local_get(base_ptr); local_get(hi); });
                    if use_shift {
                        wasm!(self.func, { i32_const(elem_shift as i32); i32_shl; });
                    } else {
                        wasm!(self.func, { i32_const(elem_size); i32_mul; });
                    }
                    wasm!(self.func, { i32_add; local_tee(addr_hi); });
                    // tmp = *addr_lo (addr_hi still on stack — save it, load from addr_lo)
                    // Stack: [addr_hi]. Save addr_hi, load *addr_lo
                    wasm!(self.func, { drop; }); // clear stack from tee
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
                        almide_ir::IrExpr { kind: almide_ir::IrExprKind::Var { id: *target }, ty: self.var_table.get(*target).ty.clone(), span: None },
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
        IrExprKind::BinOp { op, .. } => op.result_ty().unwrap_or(Ty::Unknown),
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
                almide_ir::CallTarget::Module { module, func } => {
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
pub fn collect_locals(
    body: &IrExpr,
    var_table: &almide_ir::VarTable,
    record_fields: &RecordFieldLookup,
    variant_info: &VariantInfoLookup,
) -> LocalScanResult {
    let mut binds = Vec::new();
    scan_expr(body, &mut binds, var_table, record_fields, variant_info);
    LocalScanResult { binds }
}

// ── LocalScanner: IrVisitor-based local variable collector ──────────
//
// Collects all local variable bindings in a function body for WASM local
// allocation. Uses walk_expr/walk_stmt for exhaustive traversal; only
// overrides ForIn, Match (which register bindings) and Bind/BindDestructure.

struct LocalScanner<'a> {
    locals: &'a mut Vec<(VarId, ValType)>,
    vt: &'a almide_ir::VarTable,
    record_fields: &'a RecordFieldLookup,
    variant_info: &'a VariantInfoLookup,
}

impl IrVisitor for LocalScanner<'_> {
    fn visit_expr(&mut self, expr: &IrExpr) {
        match &expr.kind {
            IrExprKind::ForIn { var, var_tuple, iterable, body } => {
                let elem_ty = match &iterable.ty {
                    almide_lang::types::Ty::Applied(almide_lang::types::TypeConstructorId::List, args) if args.len() == 1 => args[0].clone(),
                    almide_lang::types::Ty::Applied(almide_lang::types::TypeConstructorId::Map, args) if args.len() == 2 =>
                        almide_lang::types::Ty::Tuple(vec![args[0].clone(), args[1].clone()]),
                    _ => self.vt.get(*var).ty.clone(),
                };
                self.locals.push((*var, values::ty_to_valtype(&elem_ty).unwrap_or(ValType::I64)));
                if let Some(tuple_vars) = var_tuple {
                    for tv in tuple_vars {
                        let tv_type = values::ty_to_valtype(&self.vt.get(*tv).ty).unwrap_or(ValType::I64);
                        self.locals.push((*tv, tv_type));
                    }
                }
                self.visit_expr(iterable);
                for stmt in body { self.visit_stmt(stmt); }
            }
            IrExprKind::Match { subject, arms } => {
                self.visit_expr(subject);
                let resolved_ty = resolve_scan_subject_ty(subject, arms, self.vt);
                for arm in arms {
                    scan_pattern(&arm.pattern, &resolved_ty, self.locals, self.vt, self.record_fields, self.variant_info);
                    self.visit_expr(&arm.body);
                }
            }
            _ => walk_expr(self, expr),
        }
    }

    fn visit_stmt(&mut self, stmt: &IrStmt) {
        match &stmt.kind {
            IrStmtKind::Bind { var, ty, value, .. } => {
                let effective_ty = if let IrExprKind::Try { expr: inner }
                    | IrExprKind::Unwrap { expr: inner } = &value.kind {
                    if let Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, args) = &value.ty {
                        args.first().cloned().unwrap_or(value.ty.clone())
                    } else if let Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, args) = &inner.ty {
                        args.first().cloned().unwrap_or(value.ty.clone())
                    } else {
                        value.ty.clone()
                    }
                } else {
                    value.ty.clone()
                };
                let resolved_ty = if !effective_ty.is_unresolved() {
                    effective_ty
                } else if !ty.is_unresolved() {
                    ty.clone()
                } else {
                    infer_bind_type(value)
                };
                if let Some(vt_wasm) = values::ty_to_valtype(&resolved_ty) {
                    self.locals.push((*var, vt_wasm));
                }
                self.visit_expr(value);
            }
            IrStmtKind::BindDestructure { pattern, value } => {
                scan_destructure_pattern(pattern, &value.ty, self.locals, self.vt, self.record_fields, self.variant_info);
                self.visit_expr(value);
            }
            _ => walk_stmt(self, stmt),
        }
    }
}

fn scan_expr(
    expr: &IrExpr,
    locals: &mut Vec<(VarId, ValType)>,
    vt: &almide_ir::VarTable,
    record_fields: &RecordFieldLookup,
    variant_info: &VariantInfoLookup,
) {
    LocalScanner { locals, vt, record_fields, variant_info }.visit_expr(expr);
}

/// Resolve match subject type, fixing IR type inference gaps.
fn resolve_scan_subject_ty(subject: &IrExpr, arms: &[almide_ir::IrMatchArm], vt: &almide_ir::VarTable) -> almide_lang::types::Ty {
    let has_container = arms.iter().any(|a| matches!(
        &a.pattern,
        almide_ir::IrPattern::Ok { .. } | almide_ir::IrPattern::Err { .. }
        | almide_ir::IrPattern::Some { .. } | almide_ir::IrPattern::None
    ));
    if has_container && !matches!(&subject.ty, almide_lang::types::Ty::Applied(_, _)) {
        if let IrExprKind::Var { id } = &subject.kind {
            let info = vt.get(*id);
            if matches!(&info.ty, almide_lang::types::Ty::Applied(_, _)) {
                return info.ty.clone();
            }
        }
    }
    subject.ty.clone()
}

/// Scan a destructuring pattern (let (a, b) = ...) for variable bindings.
fn scan_destructure_pattern(
    pattern: &almide_ir::IrPattern,
    value_ty: &almide_lang::types::Ty,
    locals: &mut Vec<(VarId, ValType)>,
    vt: &almide_ir::VarTable,
    record_fields: &RecordFieldLookup,
    variant_info: &VariantInfoLookup,
) {
    match pattern {
        almide_ir::IrPattern::Tuple { elements } => {
            let elem_types = if let almide_lang::types::Ty::Tuple(tys) = value_ty { tys.clone() } else { vec![] };
            for (i, elem) in elements.iter().enumerate() {
                let elem_ty = elem_types.get(i).cloned().unwrap_or(almide_lang::types::Ty::Int);
                scan_destructure_pattern(elem, &elem_ty, locals, vt, record_fields, variant_info);
            }
        }
        almide_ir::IrPattern::Bind { var, .. } => {
            if let Some(val_type) = values::ty_to_valtype(value_ty) {
                locals.push((*var, val_type));
            }
        }
        almide_ir::IrPattern::RecordPattern { fields, .. } => {
            // Record destructure: resolve field types from value_ty (authoritative).
            // Uses extract_record_fields for full generic substitution.
            let resolved_fields = extract_record_fields(value_ty, record_fields, variant_info);
            let existing_ids: std::collections::HashSet<u32> = locals.iter().map(|(v, _)| v.0).collect();
            for field in fields {
                // Resolve field type from the record type (not VarTable -- VarTable may have stale types)
                let field_ty = resolved_fields.iter()
                    .find(|(n, _)| n == &field.name)
                    .map(|(_, t)| t.clone())
                    .unwrap_or(almide_lang::types::Ty::Int);
                if let Some(pat) = &field.pattern {
                    scan_destructure_pattern(pat, &field_ty, locals, vt, record_fields, variant_info);
                } else {
                    // Implicit bind: field name = var name. Look up VarId from VarTable.
                    // Use field_ty from the record type for the WASM local declaration.
                    for i in (0..vt.len()).rev() {
                        let info = vt.get(almide_ir::VarId(i as u32));
                        if info.name == field.name && !existing_ids.contains(&(i as u32)) {
                            if let Some(val_type) = values::ty_to_valtype(&field_ty) {
                                locals.push((almide_ir::VarId(i as u32), val_type));
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
fn scan_pattern(
    pattern: &almide_ir::IrPattern,
    subject_ty: &almide_lang::types::Ty,
    locals: &mut Vec<(VarId, ValType)>,
    vt: &almide_ir::VarTable,
    record_fields: &RecordFieldLookup,
    variant_info: &VariantInfoLookup,
) {
    match pattern {
        almide_ir::IrPattern::Bind { var, ty } => {
            // Use pattern's own type (set by lowering, updated by mono) — no VarTable dependency
            let effective_ty = if matches!(ty, almide_lang::types::Ty::Unknown) { subject_ty } else { ty };
            if let Some(val_type) = values::ty_to_valtype(effective_ty) {
                locals.push((*var, val_type));
            }
        }
        almide_ir::IrPattern::Constructor { name: _ctor_name, args } => {
            // Resolve field types from subject_ty's type_args for generic variants
            let subject_type_args: Vec<almide_lang::types::Ty> = match subject_ty {
                almide_lang::types::Ty::Named(_, args) if !args.is_empty() => args.clone(),
                almide_lang::types::Ty::Applied(_, args) if !args.is_empty() => args.clone(),
                almide_lang::types::Ty::Variant { .. } => {
                    // Use pattern.ty (set by mono substitute_pattern_types) — no VarTable
                    for arg in args.iter() {
                        if let almide_ir::IrPattern::Bind { var, ty } = arg {
                            let effective_ty = if ty.is_unresolved() {
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
                if let almide_ir::IrPattern::Bind { var, ty: pat_ty } = arg {
                    // Use pattern.ty first (set by mono), fall back to VarTable + substitution
                    let resolved = if !pat_ty.is_unresolved()
                        && !matches!(pat_ty, almide_lang::types::Ty::Named(n, a) if a.is_empty() && n.len() <= 2 && n.chars().next().map_or(false, |c| c.is_uppercase()))
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
                    scan_pattern(arg, subject_ty, locals, vt, record_fields, variant_info);
                }
            }
        }
        almide_ir::IrPattern::Tuple { elements } => {
            let elem_types = if let almide_lang::types::Ty::Tuple(tys) = subject_ty { tys.clone() } else { vec![] };
            for (i, elem) in elements.iter().enumerate() {
                let et = elem_types.get(i).cloned().unwrap_or(subject_ty.clone());
                scan_pattern(elem, &et, locals, vt, record_fields, variant_info);
            }
        }
        almide_ir::IrPattern::Some { inner } | almide_ir::IrPattern::Ok { inner } => {
            let inner_ty = if let almide_lang::types::Ty::Applied(_, args) = subject_ty {
                args.first().cloned().unwrap_or(subject_ty.clone())
            } else {
                // subject_ty is not Applied — try VarTable for inner binding
                if let almide_ir::IrPattern::Bind { var, .. } = inner.as_ref() {
                    let vt_ty = &vt.get(*var).ty;
                    if !vt_ty.is_unresolved() {
                        vt_ty.clone()
                    } else { subject_ty.clone() }
                } else { subject_ty.clone() }
            };
            scan_pattern(inner, &inner_ty, locals, vt, record_fields, variant_info);
        }
        almide_ir::IrPattern::Err { inner } => {
            let inner_ty = if let almide_lang::types::Ty::Applied(_, args) = subject_ty {
                args.get(1).cloned().unwrap_or(subject_ty.clone())
            } else {
                if let almide_ir::IrPattern::Bind { var, .. } = inner.as_ref() {
                    let vt_ty = &vt.get(*var).ty;
                    if !vt_ty.is_unresolved() {
                        vt_ty.clone()
                    } else { subject_ty.clone() }
                } else { subject_ty.clone() }
            };
            scan_pattern(inner, &inner_ty, locals, vt, record_fields, variant_info);
        }
        almide_ir::IrPattern::RecordPattern { name: _, fields, .. } => {
            // For pattern=None fields, the binding is implicit (field name = var name).
            // The lowerer has already allocated VarIds for these in the VarTable.
            // Search from the END of VarTable to find the most recent (correct scope) VarId,
            // and skip VarIds already registered in locals to avoid duplicates.
            // Resolve field types from subject_ty (structural or nominal) so local
            // valtypes match the actual value layout — falls back to VarTable only
            // when the record type cannot be resolved.
            let resolved_fields = extract_record_fields(subject_ty, record_fields, variant_info);
            let existing_ids: std::collections::HashSet<u32> = locals.iter().map(|(v, _)| v.0).collect();
            for field in fields {
                if let Some(pat) = &field.pattern {
                    scan_pattern(pat, subject_ty, locals, vt, record_fields, variant_info);
                } else {
                    // Implicit bind: find VarId by field name, searching from end (most recent scope)
                    for i in (0..vt.len()).rev() {
                        let info = vt.get(almide_ir::VarId(i as u32));
                        if info.name == field.name && !existing_ids.contains(&(i as u32)) {
                            let field_ty = resolved_fields.iter()
                                .find(|(n, _)| n == &field.name)
                                .map(|(_, t)| t.clone())
                                .unwrap_or_else(|| info.ty.clone());
                            if let Some(val_type) = values::ty_to_valtype(&field_ty) {
                                locals.push((almide_ir::VarId(i as u32), val_type));
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
