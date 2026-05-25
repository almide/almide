//! IrStmt → WASM instruction emission + local variable pre-scanning.

use std::collections::HashMap;

use almide_ir::{IrExpr, IrExprKind, IrStmt, IrStmtKind, VarId};
use almide_ir::visit::{IrVisitor, walk_expr, walk_stmt};
use almide_lang::types::Ty;
use wasm_encoder::ValType;

use super::FuncCompiler;
use super::wasm_macro::wasm;

impl FuncCompiler<'_> {
    /// Emit instruction to push a var's pointer/value onto the stack.
    /// Tries local var_map first, then module-level globals.
    /// Returns true if resolved, false if not found.
    fn emit_var_get(&mut self, var: &VarId) -> bool {
        if let Some(&local_idx) = self.var_map.get(&var.0) {
            wasm!(self.func, { local_get(local_idx); });
            return true;
        }
        let name = if (var.0 as usize) < self.var_table.len() {
            self.var_table.get(*var).name.as_str()
        } else { "" };
        if let Some(&(global_idx, _)) = self.emitter.top_let_globals_by_name.get(name)
            .or_else(|| self.emitter.top_let_globals.get(&var.0))
        {
            wasm!(self.func, { global_get(global_idx); });
            return true;
        }
        false
    }
}
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
                // Peephole: s = s + "x" → string_append(s, "x") for O(1) amortized
                if let IrExprKind::BinOp { op: almide_ir::BinOp::ConcatStr, left, right } = &value.kind {
                    if let IrExprKind::Var { id } = &left.kind {
                        if *id == *var {
                            if let Some(&local_idx) = self.var_map.get(&var.0) {
                                // 1-char literal: inline capacity check + byte store
                                if let IrExprKind::LitStr { value: lit } = &right.kind {
                                    if lit.len() == 1 {
                                        let byte = lit.as_bytes()[0];
                                        let s = self.scratch.alloc_i32();
                                        let len_l = self.scratch.alloc_i32();
                                        let cap_l = self.scratch.alloc_i32();
                                        wasm!(self.func, {
                                            local_get(local_idx); local_tee(s);
                                            i32_load(0); local_tee(len_l);
                                            local_get(s);
                                            i32_load(super::list_layout::STRING_CAP_OFFSET as u32);
                                            local_tee(cap_l);
                                            i32_lt_u;
                                            if_empty;
                                              // Fast: in-place byte store (ptr unchanged, no local_set needed)
                                              local_get(s);
                                              i32_const(super::list_layout::STRING_DATA_OFFSET);
                                              i32_add;
                                              local_get(len_l);
                                              i32_add;
                                              i32_const(byte as i32);
                                              i32_store8(0);
                                              local_get(s);
                                              local_get(len_l); i32_const(1); i32_add;
                                              i32_store(0);
                                            else_;
                                              // Inline grow: new_cap = max(cap*2, 16)
                                              local_get(cap_l); i32_const(1); i32_shl; local_tee(cap_l);
                                              i32_const(16); i32_lt_u;
                                              if_empty; i32_const(16); local_set(cap_l); end;
                                              // Alloc new buffer
                                              local_get(cap_l);
                                              i32_const(super::list_layout::STRING_DATA_OFFSET);
                                              i32_add;
                                              call(self.emitter.rt.alloc); local_tee(s);
                                              // Copy old data
                                              i32_const(super::list_layout::STRING_DATA_OFFSET); i32_add;
                                              local_get(local_idx);
                                              i32_const(super::list_layout::STRING_DATA_OFFSET); i32_add;
                                              local_get(len_l);
                                              memory_copy;
                                              // Write new byte
                                              local_get(s);
                                              i32_const(super::list_layout::STRING_DATA_OFFSET);
                                              i32_add;
                                              local_get(len_l); i32_add;
                                              i32_const(byte as i32);
                                              i32_store8(0);
                                              // Set len and cap
                                              local_get(s);
                                              local_get(len_l); i32_const(1); i32_add;
                                              i32_store(0);
                                              local_get(s);
                                              local_get(cap_l);
                                              i32_store(super::list_layout::STRING_CAP_OFFSET as u32);
                                              // Update local (ptr changed)
                                              local_get(s); local_set(local_idx);
                                            end;
                                        });
                                        self.scratch.free_i32(cap_l);
                                        self.scratch.free_i32(len_l);
                                        self.scratch.free_i32(s);
                                        return;
                                    }
                                }
                                // General case
                                wasm!(self.func, { local_get(local_idx); });
                                self.emit_expr(right);
                                wasm!(self.func, {
                                    call(self.emitter.rt.string_append);
                                    local_set(local_idx);
                                });
                                return;
                            }
                        }
                    }
                }
                let is_cell = self.emitter.mutable_captures.contains(&var.0);
                let local_idx = match self.var_map.get(&var.0) {
                    Some(&idx) => idx,
                    None => {
                        // Check if this is a module-level var (top_let global)
                        let name = if (var.0 as usize) < self.var_table.len() {
                            self.var_table.get(*var).name.as_str()
                        } else { "" };
                        if let Some(&(global_idx, _)) = self.emitter.top_let_globals_by_name.get(name)
                            .or_else(|| self.emitter.top_let_globals.get(&var.0))
                        {
                            self.emit_expr(value);
                            wasm!(self.func, { global_set(global_idx); });
                            return;
                        }
                        // Variable not in local scope — skip
                        self.emit_expr(value);
                        if values::ty_to_valtype(&value.ty).is_some() {
                            wasm!(self.func, { drop; });
                        }
                        return;
                    }
                };
                // Defensive: if the RHS has no WASM value (Unit / Never), skip
                // the local_set. This protects against type-checker gaps where
                // `m = unit_returning_call(...)` leaks through — the call still
                // runs for its side effects, we just don't update the local.
                // A real type error should ideally be caught in the checker;
                // this prevents WASM validation crashes in the meantime.
                if values::ty_to_valtype(&value.ty).is_none() {
                    self.emit_expr(value);
                } else if is_cell {
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

                // Peel through Block / Unwrap / Try to find the inner
                // ResultOk/ResultErr. Covers:
                //   err("msg")!           → Unwrap { ResultErr }
                //   { err("msg")! }       → Block { Unwrap { ResultErr } }
                //   guard ... else err()! → Try { ResultErr }
                let guard_body = {
                    let mut e = else_;
                    // Peel Block { stmts: [], expr: Some(tail) }
                    if let almide_ir::IrExprKind::Block { stmts, expr: Some(tail) } = &e.kind {
                        if stmts.is_empty() { e = tail; }
                    }
                    // Peel Unwrap/Try
                    if let almide_ir::IrExprKind::Unwrap { expr: inner }
                        | almide_ir::IrExprKind::Try { expr: inner } = &e.kind
                    {
                        if matches!(&inner.kind,
                            almide_ir::IrExprKind::ResultErr { .. }
                            | almide_ir::IrExprKind::ResultOk { .. })
                        {
                            e = inner;
                        }
                    }
                    e
                };

                match &guard_body.kind {
                    // Break/Continue: emit directly (they generate the right br)
                    almide_ir::IrExprKind::Break | almide_ir::IrExprKind::Continue => {
                        self.emit_expr(guard_body);
                    }
                    // ResultOk/ResultErr in guard (bare or inside Unwrap/Try/Block)
                    almide_ir::IrExprKind::ResultOk { expr: inner } | almide_ir::IrExprKind::ResultErr { expr: inner } => {
                        // ok(()) inside loop → break out of loop (not function return)
                        let is_unit_ok = matches!(&guard_body.kind, almide_ir::IrExprKind::ResultOk { .. })
                            && matches!(&inner.ty, almide_lang::types::Ty::Unit);
                        if is_unit_ok && self.loop_stack.last().is_some() {
                            self.emit_expr(guard_body);
                            if super::values::ty_to_valtype(&guard_body.ty).is_some() {
                                wasm!(self.func, { drop; });
                            }
                            let labels = self.loop_stack.last().unwrap();
                            let relative = self.depth - labels.break_depth - 1;
                            wasm!(self.func, { br(relative); });
                        } else {
                            // Non-unit ok/err → return from function
                            self.emit_expr(guard_body);
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
                // Resolve list pointer: local var or module-level global
                let has_ptr = if let Some(&local_idx) = self.var_map.get(&target.0) {
                    wasm!(self.func, { local_get(local_idx); });
                    true
                } else {
                    let name = if (target.0 as usize) < self.var_table.len() {
                        self.var_table.get(*target).name.as_str()
                    } else { "" };
                    if let Some(&(global_idx, _)) = self.emitter.top_let_globals_by_name.get(name)
                        .or_else(|| self.emitter.top_let_globals.get(&target.0))
                    {
                        wasm!(self.func, { global_get(global_idx); });
                        true
                    } else {
                        false
                    }
                };
                if has_ptr {
                    if let IrExprKind::LitInt { value: idx_val } = &index.kind {
                        let offset = (super::list_layout::DATA_OFFSET as u32) + (*idx_val as u32) * (elem_size as u32);
                        self.emit_expr(value);
                        self.emit_store_at(&value.ty, offset);
                    } else {
                        wasm!(self.func, { i32_const(super::list_layout::DATA_OFFSET); i32_add; });
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
                let var_ty = &self.var_table.get(*target).ty;
                let fields = self.extract_record_fields(var_ty);
                let tag_offset = self.variant_tag_offset(var_ty);
                if let Some((offset, _)) = super::values::field_offset(&fields, field) {
                    let total_offset = tag_offset + offset;
                    if self.emit_var_get(target) {
                        self.emit_expr(value);
                        self.emit_store_at(&value.ty, total_offset);
                    }
                }
            }
            IrStmtKind::ListSwap { target, a, b } => {
                let elem_ty = self.list_elem_ty_var(*target);
                let elem_size = values::byte_size(&elem_ty) as i32;
                if self.emit_var_get(target) {
                    let list_ptr = self.scratch.alloc_i32();
                    wasm!(self.func, { local_set(list_ptr); });
                    let addr_a = self.scratch.alloc_i32();
                    let addr_b = self.scratch.alloc_i32();
                    let tmp = self.scratch_for_ty(&elem_ty);

                    wasm!(self.func, { local_get(list_ptr); i32_const(super::list_layout::DATA_OFFSET); i32_add; });
                    self.emit_expr(a);
                    if matches!(&a.ty, Ty::Int) { wasm!(self.func, { i32_wrap_i64; }); }
                    wasm!(self.func, { i32_const(elem_size); i32_mul; i32_add; local_set(addr_a); });

                    wasm!(self.func, { local_get(list_ptr); i32_const(super::list_layout::DATA_OFFSET); i32_add; });
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

                    self.scratch.free_i32(list_ptr);
                    self.scratch.free_i32(addr_a);
                    self.scratch.free_i32(addr_b);
                    self.free_scratch_for_ty(tmp, &elem_ty);
                }
            }
            IrStmtKind::ListReverse { target, end } => {
                let elem_ty = self.list_elem_ty_var(*target);
                let elem_size = values::byte_size(&elem_ty) as i32;
                let elem_shift = (elem_size as u32).trailing_zeros();
                let use_shift = (elem_size as u32).is_power_of_two() && elem_shift > 0;
                if self.emit_var_get(target) {
                    let list_local = self.scratch.alloc_i32();
                    wasm!(self.func, { local_set(list_local); });
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
                    wasm!(self.func, { local_get(list_local); i32_const(super::list_layout::DATA_OFFSET); i32_add; local_set(base_ptr); });
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

                    self.scratch.free_i32(list_local);
                    self.scratch.free_i32(base_ptr);
                    self.scratch.free_i32(lo);
                    self.scratch.free_i32(hi);
                    self.scratch.free_i32(addr_lo);
                    self.scratch.free_i32(addr_hi);
                    self.free_scratch_for_ty(tmp, &elem_ty);
                }
            }
            IrStmtKind::ListRotateLeft { target, end } => {
                let elem_ty = self.list_elem_ty_var(*target);
                let elem_size = values::byte_size(&elem_ty) as i32;
                if self.emit_var_get(target) {
                    let list_local = self.scratch.alloc_i32();
                    wasm!(self.func, { local_set(list_local); });
                    let tmp = self.scratch_for_ty(&elem_ty);
                    let base = self.scratch.alloc_i32();
                    let end_i32 = self.scratch.alloc_i32();

                    wasm!(self.func, { local_get(list_local); i32_const(super::list_layout::DATA_OFFSET); i32_add; local_set(base); });
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
                    self.scratch.free_i32(list_local);
                    self.scratch.free_i32(base);
                    self.scratch.free_i32(end_i32);
                }
            }
            IrStmtKind::ListCopySlice { dst, src, len } => {
                // dst[..n].copy_from_slice(&src[..n])
                let dst_ok = self.emit_var_get(dst);
                if dst_ok {
                    let dst_ptr = self.scratch.alloc_i32();
                    wasm!(self.func, { local_set(dst_ptr); });
                    let src_ok = self.emit_var_get(src);
                    if src_ok {
                        let src_ptr = self.scratch.alloc_i32();
                        wasm!(self.func, { local_set(src_ptr); });
                        let elem_ty = self.list_elem_ty_var(*dst);
                        let elem_size = values::byte_size(&elem_ty) as i32;
                        wasm!(self.func, {
                            local_get(dst_ptr); i32_const(super::list_layout::DATA_OFFSET); i32_add;
                            local_get(src_ptr); i32_const(super::list_layout::DATA_OFFSET); i32_add;
                        });
                        self.emit_expr(len);
                        if matches!(&len.ty, Ty::Int) { wasm!(self.func, { i32_wrap_i64; }); }
                        wasm!(self.func, {
                            i32_const(elem_size); i32_mul;
                            memory_copy;
                        });
                        self.scratch.free_i32(src_ptr);
                    }
                    self.scratch.free_i32(dst_ptr);
                }
            }
            IrStmtKind::MapInsert { target, key, value } => {
                // m[k] = v  →  target = map.set(target, key, value)
                // Resolve target: local or global
                let has_local = self.var_map.get(&target.0).copied();
                let global_idx = if has_local.is_none() {
                    let name = if (target.0 as usize) < self.var_table.len() {
                        self.var_table.get(*target).name.as_str()
                    } else { "" };
                    self.emitter.top_let_globals_by_name.get(name).map(|&(g, _)| g)
                        .or_else(|| self.emitter.top_let_globals.get(&target.0).map(|&(g, _)| g))
                } else { None };
                if has_local.is_some() || global_idx.is_some() {
                    let set_args = vec![
                        almide_ir::IrExpr { kind: almide_ir::IrExprKind::Var { id: *target }, ty: self.var_table.get(*target).ty.clone(), span: None, def_id: None },
                        key.clone(),
                        value.clone(),
                    ];
                    self.emit_map_call("set", &set_args);
                    if let Some(local_idx) = has_local {
                        wasm!(self.func, { local_set(local_idx); });
                    } else if let Some(g) = global_idx {
                        wasm!(self.func, { global_set(g); });
                    }
                }
            }
        }
    }

    /// Check if an expression writes to outer-scope mutable variables with heap types.
    /// Used by auto-scope to determine if heap_restore is safe.
    pub(super) fn expr_writes_outer_heap(&self, expr: &IrExpr) -> bool {
        struct HeapWriteScanner<'a> {
            var_table: &'a almide_ir::VarTable,
            found: bool,
        }
        impl IrVisitor for HeapWriteScanner<'_> {
            fn visit_stmt(&mut self, stmt: &almide_ir::IrStmt) {
                if self.found { return; }
                match &stmt.kind {
                    IrStmtKind::Assign { var, .. }
                    | IrStmtKind::MapInsert { target: var, .. }
                    | IrStmtKind::IndexAssign { target: var, .. }
                    | IrStmtKind::FieldAssign { target: var, .. } => {
                        let ty = &self.var_table.get(*var).ty;
                        if Self::is_heap_type(ty) {
                            self.found = true;
                        }
                    }
                    _ => {}
                }
                walk_stmt(self, stmt);
            }
            fn visit_expr(&mut self, expr: &IrExpr) {
                if self.found { return; }
                walk_expr(self, expr);
            }
        }
        impl HeapWriteScanner<'_> {
            fn is_heap_type(ty: &Ty) -> bool {
                matches!(ty, Ty::String
                    | Ty::Applied(_, _)
                    | Ty::Record { .. }
                    | Ty::Unknown
                )
            }
        }
        let mut scanner = HeapWriteScanner { var_table: self.var_table, found: false };
        scanner.visit_expr(expr);
        scanner.found
    }

    /// Check if an expression allocates heap memory (string/list/record construction,
    /// or calls returning heap types). Used to decide if iter_scope is worthwhile.
    pub(super) fn expr_allocates_heap(&self, expr: &IrExpr) -> bool {
        struct AllocScanner { found: bool }
        impl IrVisitor for AllocScanner {
            fn visit_expr(&mut self, expr: &IrExpr) {
                if self.found { return; }
                match &expr.kind {
                    // Direct heap allocations
                    IrExprKind::LitStr { .. }
                    | IrExprKind::StringInterp { .. }
                    | IrExprKind::List { .. }
                    | IrExprKind::Record { .. }
                    | IrExprKind::MapLiteral { .. } => {
                        self.found = true;
                        return;
                    }
                    // Calls that return heap types
                    IrExprKind::Call { .. } | IrExprKind::TailCall { .. }
                    | IrExprKind::RuntimeCall { .. } => {
                        if matches!(&expr.ty, Ty::String | Ty::Applied(_, _)
                            | Ty::Record { .. } | Ty::Unknown)
                        {
                            self.found = true;
                            return;
                        }
                    }
                    // String concat
                    IrExprKind::BinOp { op: almide_ir::BinOp::ConcatStr, .. } => {
                        self.found = true;
                        return;
                    }
                    _ => {}
                }
                walk_expr(self, expr);
            }
            fn visit_stmt(&mut self, stmt: &almide_ir::IrStmt) {
                if self.found { return; }
                walk_stmt(self, stmt);
            }
        }
        let mut scanner = AllocScanner { found: false };
        scanner.visit_expr(expr);
        scanner.found
    }

    /// Check if an expression contains function calls that may allocate heap memory.
    /// A call is "heap-allocating" if it returns a heap type (String, List, Record, etc.).
    /// Pure-int recursive calls (like fib) don't allocate and shouldn't trigger iter_scope.
    pub(super) fn expr_contains_heap_call(&self, expr: &IrExpr) -> bool {
        struct HeapCallScanner { found: bool }
        impl IrVisitor for HeapCallScanner {
            fn visit_expr(&mut self, expr: &IrExpr) {
                if self.found { return; }
                match &expr.kind {
                    IrExprKind::Call { .. } | IrExprKind::TailCall { .. }
                    | IrExprKind::RuntimeCall { .. } => {
                        if HeapWriteScanner::is_heap_type(&expr.ty) {
                            self.found = true;
                            return;
                        }
                    }
                    _ => {}
                }
                walk_expr(self, expr);
            }
            fn visit_stmt(&mut self, stmt: &almide_ir::IrStmt) {
                if self.found { return; }
                walk_stmt(self, stmt);
            }
        }
        // Reuse HeapWriteScanner's is_heap_type (already handles String, Applied, Record, Unknown)
        struct HeapWriteScanner;
        impl HeapWriteScanner {
            fn is_heap_type(ty: &Ty) -> bool {
                matches!(ty, Ty::String | Ty::Applied(_, _) | Ty::Record { .. } | Ty::Unknown)
            }
        }
        let mut scanner = HeapCallScanner { found: false };
        scanner.visit_expr(expr);
        scanner.found
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
                almide_ir::CallTarget::Module { module, func, .. } => {
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
