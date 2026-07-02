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
        // §522 class kill: ONE global resolution (id → alias → origin-key →
        // bare name) shared with the value-read arm. The old bare-name-FIRST
        // probe could bind a same-named WRONG global silently.
        if let Some((global_idx, _)) = self.lookup_global(*var) {
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
pub(super) type RecordFieldLookup = std::collections::BTreeMap<String, Vec<(String, Ty)>>;

/// Lookup for variant type info by nominal name.
pub(super) type VariantInfoLookup = std::collections::BTreeMap<String, Vec<VariantCase>>;

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
                    // Perceus rc_inc is now handled by PerceusPass (IR-level RcInc node)
                    if let Some(_vt) = values::ty_to_valtype(effective_ty) {
                        let local_idx = self.var_map[&var.0];
                        wasm!(self.func, { local_set(local_idx); });
                    }
                }
                // EARLY-RETURN LEAK FIX: now that `var` is bound (after its value — so a
                // Try/Unwrap INSIDE the value did not yet see it), track it as an owned
                // heap local so a later Try/Unwrap/Fan early-return frees it (see
                // emit_early_return_decs). Exclude env-borrows (the closure env owns them)
                // and donate-only `__*` temps (they never get their own dec → decing one
                // would free a donated ref). It will be removed on its Perceus RcDec.
                if Self::is_heap_type(effective_ty)
                    && !matches!(value.kind, IrExprKind::EnvLoad { .. })
                    && !is_donate_temp(self.var_table.get(*var).name.as_str())
                {
                    self.live_heap.push(*var);
                }
            }

            IrStmtKind::Assign { var, value } => {
                // Peephole: s = s + "x" → string_append(s, "x") for O(1) amortized.
                // Skip for a shared-cell capture: its local holds the CELL pointer, not
                // the string, so the in-place byte store below would corrupt the cell.
                // The general cell-aware Assign (further down) handles it. (Closure v2 P6.)
                if let IrExprKind::BinOp { op: almide_ir::BinOp::ConcatStr, left, right } = &value.kind {
                    if let IrExprKind::Var { id } = &left.kind {
                        if *id == *var && !self.emitter.mutable_captures.contains(&var.0) {
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
                                            i32_load(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::CAP) as i32 as u32);
                                            local_tee(cap_l);
                                            i32_lt_u;
                                            if_empty;
                                              // Fast: in-place byte store (ptr unchanged, no local_set needed)
                                              local_get(s);
                                              i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32);
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
                                              i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32);
                                              i32_add;
                                              call(self.emitter.rt.alloc); local_tee(s);
                                              // Copy old data
                                              i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
                                              local_get(local_idx);
                                              i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
                                              local_get(len_l);
                                              memory_copy;
                                              // Write new byte
                                              local_get(s);
                                              i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32);
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
                                              i32_store(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::CAP) as i32 as u32);
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
                // Peephole: x = bytes.set(x, i, v) → in-place byte store. The
                // oracle `bytes.set` is VALUE-returning (native CLONES), so the
                // in-place fast path is valid ONLY for a self-update whose target
                // is provably unaliased: not COW-marked (AliasCowPass marks
                // copy-aliased and param-reachable bytes.set targets) and not a
                // shared-cell capture. Everything else takes the general
                // emit_bytes_call path, which clones a shared input.
                if let IrExprKind::Call { target: almide_ir::CallTarget::Module { module, func, .. }, args, .. } = &value.kind {
                    if module.as_str() == "bytes" && func.as_str() == "set" && args.len() == 3 {
                        if let IrExprKind::Var { id } = &args[0].kind {
                            if *id == *var
                                && !self.emitter.needs_cow.contains(&var.0)
                                && !self.emitter.mutable_captures.contains(&var.0)
                            {
                                if let Some(&local_idx) = self.var_map.get(&var.0) {
                                    let idx = self.scratch.alloc_i32();
                                    let val = self.scratch.alloc_i32();
                                    self.emit_expr(&args[1]);
                                    wasm!(self.func, { i32_wrap_i64; local_set(idx); });
                                    self.emit_expr(&args[2]);
                                    wasm!(self.func, {
                                        i32_wrap_i64; local_set(val);
                                        // bounds check: idx < len (mirrors bytes.set)
                                        local_get(idx); local_get(local_idx); i32_load(0); i32_lt_u;
                                        if_empty;
                                          local_get(local_idx);
                                          i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32);
                                          i32_add; local_get(idx); i32_add;
                                          local_get(val);
                                          i32_store8(0);
                                        end;
                                        // ptr unchanged — no local_set needed
                                    });
                                    self.scratch.free_i32(val);
                                    self.scratch.free_i32(idx);
                                    return;
                                }
                            }
                        }
                    }
                }
                let is_cell = self.emitter.mutable_captures.contains(&var.0);
                let local_idx = match self.var_map.get(&var.0) {
                    Some(&idx) => idx,
                    None => {
                        // Module-level var (top_let global) — resolve via the
                        // SHARED lookup (id → module_origin-prefixed key →
                        // bare name); the bare-name-only lookup missed
                        // cross-module synthetic lvalues (#505).
                        if let Some((global_idx, _)) = self.lookup_global(*var) {
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
                    // Perceus Assign rc_dec is now handled by PerceusPass (IR-level RcDec before Assign)
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

            IrStmtKind::RcInc { var } => {
                if let Some(&local_idx) = self.var_map.get(&var.0) {
                    wasm!(self.func, {
                        local_get(local_idx);
                        call(self.emitter.rt.rc_inc);
                        drop;
                    });
                } else if let Some((global_idx, _)) = self.lookup_global(*var) {
                    // Module-global target: emit the Inc through the global.
                    // Previously BOTH the Inc and the Dec on a global were
                    // silently dropped — balanced only by mutual omission.
                    wasm!(self.func, {
                        global_get(global_idx);
                        call(self.emitter.rt.rc_inc);
                        drop;
                    });
                } else {
                    // #523: a Perceus-mandated Inc the emitter cannot resolve
                    // was previously DROPPED IN SILENCE — an under-count no
                    // gate could see (the IR belt verifies the IR, not the
                    // emission), which under default-ON frees is the
                    // double-free class. Refuse loudly.
                    panic!(
                        "[ICE] RcInc target `{}` (VarId {}) resolved to neither local nor global (#523)",
                        if (var.0 as usize) < self.var_table.len() { self.var_table.get(*var).name.as_str() } else { "?" },
                        var.0
                    );
                }
            }
            IrStmtKind::RcDec { var } => {
                if let Some(&local_idx) = self.var_map.get(&var.0) {
                    if self.emitter.mutable_captures.contains(&var.0) {
                        // Captured shared cell: the local holds the CELL ptr, whose RC
                        // a PLAIN `rc_inc` bumps on capture (see RcInc above). Balance
                        // it with a PLAIN `rc_dec` on the cell — NOT a typed rc_dec,
                        // which walks the cell ptr AS the object: cell[0] (the object
                        // ptr) is read as the element count, so the element-drop loop
                        // decrefs garbage addresses → trap. List[Int] (Copy elems, no
                        // element drop) survived; List[String] trapped. (Closure v2 P6.)
                        wasm!(self.func, { local_get(local_idx); call(self.emitter.rt.rc_dec); });
                    } else {
                        let ty = &self.var_table.get(*var).ty;
                        self.emit_typed_rc_dec(ty, local_idx);
                    }
                } else if let Some((global_idx, _)) = self.lookup_global(*var) {
                    // Module-global target: run the typed dec through a
                    // scratch local loaded from the global (symmetric with
                    // the Inc arm above).
                    let tmp = self.scratch.alloc_i32();
                    wasm!(self.func, { global_get(global_idx); local_set(tmp); });
                    let ty = self.var_table.get(*var).ty.clone();
                    self.emit_typed_rc_dec(&ty, tmp);
                    self.scratch.free_i32(tmp);
                } else {
                    // #523 twin: a dropped Dec is "only" a leak, but the
                    // asymmetry with a dropped Inc makes the books unreadable
                    // — same loud refusal.
                    panic!(
                        "[ICE] RcDec target `{}` (VarId {}) resolved to neither local nor global (#523)",
                        if (var.0 as usize) < self.var_table.len() { self.var_table.get(*var).name.as_str() } else { "?" },
                        var.0
                    );
                }
                // EARLY-RETURN LEAK FIX: this heap local is now dropped — it is no longer
                // live, so a Try/Unwrap/Fan early-return AFTER this point must not free it
                // again (the retain is a no-op for vars never tracked, e.g. globals).
                self.live_heap.retain(|v| *v != *var);
            }
            IrStmtKind::Comment { .. } => {
                // No-op in WASM
            }

            IrStmtKind::BindDestructure { pattern, value } => {
                self.emit_expr(value);
                let scratch = self.scratch.alloc_i32();
                wasm!(self.func, { local_set(scratch); });
                // Recursive so a NESTED sub-pattern (`let (a, (b, c)) = …`) binds
                // its leaves instead of leaving them zeroed (#654); mirrors the
                // local pre-scan in `scan_destructure_pattern`.
                self.emit_bind_destructure(pattern, scratch, &value.ty);
                self.scratch.free_i32(scratch);
            }

            IrStmtKind::IndexAssign { target, index, value } => {
                // xs[i] = v → store value at ptr + data_off + i * elem_size
                // COW the (possibly aliased) target into its own local first, so a
                // sibling binding sharing this list does not see the write.
                self.cow_if_needed(target.0);
                let target_ty = &self.var_table.get(*target).ty;
                let is_bytes = matches!(target_ty, Ty::Bytes);
                let elem_size = if is_bytes { 1u32 } else { super::values::byte_size(&value.ty) };
                // Resolve list pointer: local var or module-level global
                let has_ptr = if let Some(&local_idx) = self.var_map.get(&target.0) {
                    wasm!(self.func, { local_get(local_idx); });
                    // Shared-cell capture: the local holds the CELL pointer; deref it
                    // to the list pointer. The element store is in place (same list),
                    // so no write-back to the cell is needed. (Closure v2 P6.)
                    if self.emitter.mutable_captures.contains(&target.0) {
                        wasm!(self.func, { i32_load(0); });
                    }
                    true
                } else if let Some((global_idx, _)) = self.lookup_global(*target) {
                    wasm!(self.func, { global_get(global_idx); });
                    true
                } else {
                    // Dropping a MUTATION on the floor is the silent-wrong
                    // class (#522) — refuse loudly. Locals are pre-scanned
                    // and globals resolve via lookup_global; anything else
                    // is a compiler bug.
                    panic!(
                        "[ICE] index-assign target `{}` (VarId {}) resolved to neither local nor global (#522 class)",
                        if (target.0 as usize) < self.var_table.len() { self.var_table.get(*target).name.as_str() } else { "?" },
                        target.0
                    );
                };
                if has_ptr {
                    let data_off = self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA);
                    // #554/C-072: the write path had NO bounds check — an OOB
                    // `xs[i] = v` was a silent heap WRITE into adjacent
                    // allocator memory (exit 0) while native panicked. Capture
                    // the list pointer, GUARD the index on the full i64
                    // (negative / >= 2^32 caught, no truncation), then store.
                    let ptr_l = self.scratch.alloc_i32();
                    wasm!(self.func, { local_set(ptr_l); });
                    let oob_msg = self.emitter.intern_string("Error: index out of bounds\n") as i32;
                    let div_trap = self.emitter.rt.div_trap;
                    let idx64 = self.scratch.alloc_i64();
                    if let IrExprKind::LitInt { value: idx_val } = &index.kind {
                        wasm!(self.func, { i64_const(*idx_val); local_set(idx64); });
                    } else {
                        self.emit_expr(index);
                        if matches!(&index.ty, almide_lang::types::Ty::Int) {
                            wasm!(self.func, { local_set(idx64); });
                        } else {
                            wasm!(self.func, { i64_extend_i32_s; local_set(idx64); });
                        }
                    }
                    self.emit_index_bound_guard(idx64, ptr_l, oob_msg, div_trap);
                    // addr = ptr + data_off + idx * elem_size
                    wasm!(self.func, { local_get(ptr_l); i32_const(data_off as i32); i32_add;
                                       local_get(idx64); i32_wrap_i64; });
                    if elem_size > 1 {
                        wasm!(self.func, { i32_const(elem_size as i32); i32_mul; });
                    }
                    wasm!(self.func, { i32_add; });
                    self.emit_expr(value);
                    if is_bytes {
                        wasm!(self.func, { i32_wrap_i64; i32_store8(0); });
                    } else {
                        self.emit_store_at(&value.ty, 0);
                    }
                    self.scratch.free_i64(idx64);
                    self.scratch.free_i32(ptr_l);
                }
            }
            IrStmtKind::FieldAssign { target, field, value } => {
                // record.field = value
                self.cow_if_needed(target.0);
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
                self.cow_if_needed(target.0);
                let elem_ty = self.list_elem_ty_var(*target);
                let elem_size = values::byte_size(&elem_ty) as i32;
                if self.emit_var_get(target) {
                    let list_ptr = self.scratch.alloc_i32();
                    wasm!(self.func, { local_set(list_ptr); });
                    let addr_a = self.scratch.alloc_i32();
                    let addr_b = self.scratch.alloc_i32();
                    let tmp = self.scratch_for_ty(&elem_ty);

                    wasm!(self.func, { local_get(list_ptr); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add; });
                    self.emit_expr(a);
                    if matches!(&a.ty, Ty::Int) { wasm!(self.func, { i32_wrap_i64; }); }
                    wasm!(self.func, { i32_const(elem_size); i32_mul; i32_add; local_set(addr_a); });

                    wasm!(self.func, { local_get(list_ptr); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add; });
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
                self.cow_if_needed(target.0);
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
                    wasm!(self.func, { local_get(list_local); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add; local_set(base_ptr); });
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
                self.cow_if_needed(target.0);
                let elem_ty = self.list_elem_ty_var(*target);
                let elem_size = values::byte_size(&elem_ty) as i32;
                if self.emit_var_get(target) {
                    let list_local = self.scratch.alloc_i32();
                    wasm!(self.func, { local_set(list_local); });
                    let tmp = self.scratch_for_ty(&elem_ty);
                    let base = self.scratch.alloc_i32();
                    let end_i32 = self.scratch.alloc_i32();

                    wasm!(self.func, { local_get(list_local); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add; local_set(base); });
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
                // dst[..n].copy_from_slice(&src[..n]) — only dst is written.
                self.cow_if_needed(dst.0);
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
                            local_get(dst_ptr); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                            local_get(src_ptr); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
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
                    let g = self.lookup_global(*target).map(|(g, _)| g);
                    if g.is_none() {
                        panic!(
                            "[ICE] map-insert target `{}` (VarId {}) resolved to neither local nor global (#522 class)",
                            if (target.0 as usize) < self.var_table.len() { self.var_table.get(*target).name.as_str() } else { "?" },
                            target.0
                        );
                    }
                    g
                } else { None };
                if has_local.is_some() || global_idx.is_some() {
                    let set_args = vec![
                        almide_ir::IrExpr { kind: almide_ir::IrExprKind::Var { id: *target }, ty: self.var_table.get(*target).ty.clone(), span: None, def_id: None },
                        key.clone(),
                        value.clone(),
                    ];
                    let is_cell = has_local.is_some() && self.emitter.mutable_captures.contains(&target.0);
                    if is_cell {
                        // Shared-cell capture: the local holds the CELL ptr; the new
                        // map must be stored INTO the cell (cell[0] = new_map) so the
                        // captured-and-shared map is updated, not a local copy that
                        // local_set would discard (the outer scope keeps the old map).
                        let cell_local = has_local.unwrap();
                        wasm!(self.func, { local_get(cell_local); });
                        self.emit_map_call("set", &set_args);
                        wasm!(self.func, { i32_store(0); });
                    } else {
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
    }
}

include!("statements_p2.rs");
include!("statements_p3.rs");
