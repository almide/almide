// IrExpr → WASM: emit_expr group 3 (Option/Result construction + unwrapping).
//
// Part of `expressions.rs` — `include!`d at the END of the parent, so it shares
// the parent module's imports. Sub-match over the SAME `&expr.kind` scrutinee
// restricted to a DISJOINT set of arms; returns `true` when it handled the expr.
// Arm bodies are moved VERBATIM (only a trailing `true` and the `_ => false`
// fallthrough are added). Chained from `emit_expr` before its group-1 match.

impl FuncCompiler<'_> {
    pub(super) fn emit_expr_g3(&mut self, expr: &IrExpr) -> bool {
        match &expr.kind {
            // ── Option/Result ──
            IrExprKind::OptionSome { expr: inner } => {
                // Resolve inner type: if Unknown, infer from outer Option type or inner expr
                let inner_ty = if matches!(inner.ty, Ty::Unknown) {
                    if let Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, args) = &expr.ty {
                        let candidate = args.first().cloned().unwrap_or(Ty::Unknown);
                        if !matches!(candidate, Ty::Unknown) { candidate }
                        else { self.infer_type_from_expr(inner) }
                    } else { self.infer_type_from_expr(inner) }
                } else { inner.ty.clone() };
                let inner_size = values::byte_size(&inner_ty);
                let scratch = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(inner_size as i32);
                    call(self.emitter.rt.alloc);
                    local_set(scratch);
                    local_get(scratch);
                });
                self.emit_stored_field(inner);
                self.emit_store_at(&inner_ty, 0);
                wasm!(self.func, { local_get(scratch); });
                self.scratch.free_i32(scratch);
                true
            }
            IrExprKind::OptionNone => {
                wasm!(self.func, { i32_const(0); });
                true
            }

            // ── Result ok/err ──
            IrExprKind::ResultOk { expr: inner } => {
                // ok(x) = [tag:0, value]
                // Resolve inner type: if Unknown, try to infer from the outer Result type or expr
                let inner_ty = if matches!(inner.ty, Ty::Unknown) {
                    self.resolve_result_inner_ty(expr, true)
                } else { inner.ty.clone() };
                let inner_size = values::byte_size(&inner_ty);
                let scratch = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const((4 + inner_size) as i32);
                    call(self.emitter.rt.alloc);
                    local_set(scratch);
                    // tag = 0
                    local_get(scratch);
                    i32_const(0);
                    i32_store(0);
                });
                if values::ty_to_valtype(&inner_ty).is_some() {
                    wasm!(self.func, { local_get(scratch); });
                    self.emit_stored_field(inner);
                    self.emit_store_at(&inner_ty, 4);
                } else {
                    // Unit or zero-sized: still emit for side effects
                    self.emit_expr(inner);
                }
                wasm!(self.func, { local_get(scratch); });
                self.scratch.free_i32(scratch);
                true
            }
            IrExprKind::ResultErr { expr: inner } => {
                // err(e) = [tag:1, value]
                let inner_ty = if matches!(inner.ty, Ty::Unknown) {
                    self.resolve_result_inner_ty(expr, false)
                } else { inner.ty.clone() };
                let inner_size = values::byte_size(&inner_ty);
                let scratch = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const((4 + inner_size) as i32);
                    call(self.emitter.rt.alloc);
                    local_set(scratch);
                    // tag = 1
                    local_get(scratch);
                    i32_const(1);
                    i32_store(0);
                });
                if values::ty_to_valtype(&inner_ty).is_some() {
                    wasm!(self.func, { local_get(scratch); });
                    self.emit_stored_field(inner);
                    self.emit_store_at(&inner_ty, 4);
                } else {
                    // Unit or zero-sized: still emit for side effects
                    self.emit_expr(inner);
                }
                wasm!(self.func, { local_get(scratch); });
                self.scratch.free_i32(scratch);
                true
            }

            // ── Try (auto-unwrap Result in effect fn) ──
            IrExprKind::Try { expr: inner } => {
                // Evaluate inner (returns Result ptr: [tag:i32][value])
                // If tag == 0 (ok): unwrap → push value
                // If tag != 0 (err): return the Result as-is
                self.emit_expr(inner);
                let scratch = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_set(scratch);
                    // Check tag
                    local_get(scratch);
                    i32_load(0);
                    i32_const(0);
                    i32_ne;
                    if_empty;
                });
                // EARLY-RETURN LEAK FIX: free the heap locals live here before the bare
                // `return_` (which skips the Perceus terminal rc_decs) — else they leak.
                self.emit_early_return_decs();
                wasm!(self.func, {
                    // Err: return the Result ptr
                    local_get(scratch);
                    return_;
                    end;
                });
                // Ok: load the unwrapped value (skip for Unit — nothing to load)
                if !matches!(&expr.ty, Ty::Unit) {
                    wasm!(self.func, { local_get(scratch); });
                    self.emit_load_at(&expr.ty, 4);
                }
                self.scratch.free_i32(scratch);
                true
            }

            // ── Unwrap (propagate err on failure) ──
            IrExprKind::Unwrap { expr: inner } => {
                let is_option = inner.ty.is_option();
                self.emit_expr(inner);
                let scratch = self.scratch.alloc_i32();
                if is_option {
                    // Option: ptr==0 → None (propagate as err), ptr!=0 → Some (payload at ptr)
                    let err_ptr = self.scratch.alloc_i32();
                    let none_str = self.emitter.intern_string("none") as i32;
                    let alloc = self.emitter.rt.alloc;
                    wasm!(self.func, {
                        local_set(scratch);
                        local_get(scratch);
                        i32_eqz;
                        if_empty;
                    });
                    // EARLY-RETURN LEAK FIX: free live heap locals on the None err path
                    // before building+returning err("none") — else they leak on wasm.
                    self.emit_early_return_decs();
                    wasm!(self.func, {
                        // None → return `err("none")` so the unwrap propagates a real
                        // Err Result, matching native's `.ok_or("none")?`. (Previously
                        // returned a null ptr `0`, which the caller mis-read as `Ok`
                        // tag → silent success / exit 0 on wasm.)
                        i32_const(8);          // [tag:i32@0][String ptr@4]
                        call(alloc);
                        local_set(err_ptr);
                        local_get(err_ptr);
                        i32_const(1);          // tag = 1 (Err)
                        i32_store(0);
                        local_get(err_ptr);
                        i32_const(none_str);   // err payload = "none"
                        i32_store(4);
                        local_get(err_ptr);
                        return_;
                        end;
                    });
                    self.scratch.free_i32(err_ptr);
                    // Some: load payload from ptr
                    if !matches!(&expr.ty, Ty::Unit) {
                        wasm!(self.func, { local_get(scratch); });
                        self.emit_load_at(&expr.ty, 0);
                    }
                } else {
                    // Result: [tag:i32, payload]. tag==0 → Ok, tag!=0 → Err
                    wasm!(self.func, {
                        local_set(scratch);
                        local_get(scratch);
                        i32_load(0);
                        i32_const(0);
                        i32_ne;
                        if_empty;
                    });
                    // EARLY-RETURN LEAK FIX: free live heap locals before the bare
                    // `return_` (skips the terminal rc_decs) — else they leak on wasm.
                    self.emit_early_return_decs();
                    wasm!(self.func, {
                        // Err path: propagate the Result pointer (early return)
                        local_get(scratch);
                        return_;
                        end;
                    });
                    if !matches!(&expr.ty, Ty::Unit) {
                        wasm!(self.func, { local_get(scratch); });
                        self.emit_load_at(&expr.ty, 4);
                    }
                }
                self.scratch.free_i32(scratch);
                true
            }

            // ── UnwrapOr (fallback on err/none) ──
            IrExprKind::UnwrapOr { expr: inner, fallback } => {
                let is_option = inner.ty.is_option();
                self.emit_expr(inner);
                let scratch = self.scratch.alloc_i32();
                let bt = values::block_type(&expr.ty);
                if is_option {
                    // Option: ptr==0 → fallback, ptr!=0 → load payload from ptr
                    wasm!(self.func, {
                        local_set(scratch);
                        local_get(scratch);
                        i32_eqz;
                    });
                    self.func.instruction(&Instruction::If(bt));
                    self.emit_expr(fallback);
                    wasm!(self.func, { else_; local_get(scratch); });
                    self.emit_load_at(&expr.ty, 0);
                    wasm!(self.func, { end; });
                } else {
                    // Result: tag==0 → ok (load payload at +4), tag!=0 → fallback
                    wasm!(self.func, {
                        local_set(scratch);
                        local_get(scratch);
                        i32_load(0);
                        i32_eqz;
                    });
                    self.func.instruction(&Instruction::If(bt));
                    wasm!(self.func, { local_get(scratch); });
                    self.emit_load_at(&expr.ty, 4);
                    wasm!(self.func, { else_; });
                    self.emit_expr(fallback);
                    wasm!(self.func, { end; });
                }
                self.scratch.free_i32(scratch);
                true
            }

            // ── ToOption (Result → Option, Option passthrough) ──
            IrExprKind::ToOption { expr: inner } => {
                if inner.ty.is_option() {
                    // Option → Option: identity
                    self.emit_expr(inner);
                } else {
                    // Result → Option: ok(v) → some(v), err(_) → none (ptr=0)
                    self.emit_expr(inner);
                    let scratch = self.scratch.alloc_i32();
                    wasm!(self.func, {
                        local_set(scratch);
                        local_get(scratch);
                        i32_load(0);
                        i32_eqz;
                        if_i32;
                    });
                    // Ok: allocate Some ptr and copy payload
                    let inner_ty = inner.ty.result_ok_ty().unwrap_or(Ty::Unknown);
                    let inner_size = values::byte_size(&inner_ty);
                    if inner_size > 0 {
                        wasm!(self.func, {
                            i32_const(inner_size as i32);
                            call(self.emitter.rt.alloc);
                        });
                        let some_scratch = self.scratch.alloc_i32();
                        wasm!(self.func, { local_tee(some_scratch); });
                        wasm!(self.func, { local_get(scratch); });
                        self.emit_load_at(&inner_ty, 4);
                        self.emit_store_at(&inner_ty, 0);
                        wasm!(self.func, { local_get(some_scratch); });
                        self.scratch.free_i32(some_scratch);
                    } else {
                        // Unit payload — Some is just a non-zero ptr
                        wasm!(self.func, {
                            i32_const(1);
                        });
                    }
                    wasm!(self.func, {
                        else_;
                        // Err: return 0 (None)
                        i32_const(0);
                        end;
                    });
                    self.scratch.free_i32(scratch);
                }
                true
            }

            // ── Map index access: m[key] → Option[V] ──
            IrExprKind::MapAccess { object, key } => {
                let fake_args = vec![(**object).clone(), (**key).clone()];
                self.emit_map_call("get", &fake_args);
                true
            }

            // ── Optional chaining: expr?.field → None if expr is None, else Some(expr.field) ──
            IrExprKind::OptionalChain { expr: inner, field } => {
                // inner is Option<RecordType> — ptr: 0=None, nonzero=Some wrapper
                // Some wrapper layout: [payload_ptr:i32] where payload_ptr → record
                self.emit_expr(inner);
                let scratch = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_set(scratch);
                    local_get(scratch);
                    i32_eqz;
                    if_i32;
                    // None path → propagate None (ptr=0)
                    i32_const(0);
                    else_;
                });
                // Some path: dereference Some wrapper to get the actual record pointer
                let payload_ty = inner.ty.option_inner().unwrap_or_else(|| inner.ty.clone());
                let payload_size = values::byte_size(&payload_ty);
                if payload_size > 0 {
                    wasm!(self.func, { local_get(scratch); i32_load(0); local_set(scratch); });
                }
                let fields = self.extract_record_fields(&payload_ty);
                let tag_offset = self.variant_tag_offset(&payload_ty);
                if let Some((field_offset, field_ty)) = values::field_offset(&fields, field) {
                    let total_offset = tag_offset + field_offset;
                    let field_size = values::byte_size(&field_ty);
                    if field_size > 0 {
                        // Allocate Some wrapper for the field value
                        wasm!(self.func, { i32_const(field_size as i32); call(self.emitter.rt.alloc); });
                        let some_ptr = self.scratch.alloc_i32();
                        wasm!(self.func, { local_tee(some_ptr); local_get(scratch); });
                        self.emit_load_at(&field_ty, total_offset);
                        self.emit_store_at(&field_ty, 0);
                        wasm!(self.func, { local_get(some_ptr); });
                        self.scratch.free_i32(some_ptr);
                    } else {
                        // Unit field → Some is just a non-zero ptr
                        wasm!(self.func, { i32_const(1); });
                    }
                } else {
                    wasm!(self.func, { unreachable; });
                }
                wasm!(self.func, { end; });
                self.scratch.free_i32(scratch);
                true
            }
            _ => false,
        }
    }
}
