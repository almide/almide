// List stdlib closure-based call dispatch for WASM codegen (part 2, helpers).
//
// Sibling helpers of `emit_list_closure_call2`, moved out verbatim to keep the
// parent file under the line cap. Included into `calls_list_closure2.rs` via
// `include!`; shares its module imports (FuncCompiler, values, IrExpr, Ty,
// ValType, BinOp, IrExprKind) and the `PipelineStage`/`SimdMapOp` enums.

impl FuncCompiler<'_> {
    // ── Stream Fusion helpers ──

    /// Detect fusible pipeline stages in a list expression.
    /// Returns non-empty vec if the expr is a chain of list.map/filter calls.
    fn detect_pipeline(&self, expr: &IrExpr) -> Vec<&str> {
        let mut stages = Vec::new();
        let mut cur = expr;
        loop {
            if let Some((op, fn_arg, source)) = self.match_list_pipeline_stage(cur) {
                if !matches!(&fn_arg.kind, IrExprKind::Lambda { .. }) {
                    break;
                }
                stages.push(op);
                cur = source;
            } else {
                break;
            }
        }
        stages
    }

    fn extract_pipeline<'b>(&self, expr: &'b IrExpr) -> (&'b IrExpr, Vec<PipelineStage<'b>>) {
        let mut stages = Vec::new();
        let mut cur = expr;
        loop {
            if let Some((op, fn_arg, source)) = self.match_list_pipeline_stage(cur) {
                if !matches!(&fn_arg.kind, IrExprKind::Lambda { .. }) {
                    break;
                }
                match op {
                    "map" => stages.push(PipelineStage::Map(fn_arg)),
                    "filter" => stages.push(PipelineStage::Filter(fn_arg)),
                    _ => break,
                }
                cur = source;
            } else {
                break;
            }
        }
        stages.reverse();
        (cur, stages)
    }

    /// Match a list.map or list.filter call, handling both Module and RuntimeCall forms.
    /// Returns (op_name, fn_arg, source_list) if matched.
    fn match_list_pipeline_stage<'b>(&self, expr: &'b IrExpr) -> Option<(&'static str, &'b IrExpr, &'b IrExpr)> {
        match &expr.kind {
            IrExprKind::Call { target: almide_ir::CallTarget::Module { module, func, .. }, args, .. }
                if module.as_str() == "list" && args.len() >= 2 =>
            {
                match func.as_str() {
                    "map" => Some(("map", &args[1], &args[0])),
                    "filter" => Some(("filter", &args[1], &args[0])),
                    _ => None,
                }
            }
            IrExprKind::RuntimeCall { symbol, args } if args.len() >= 2 => {
                let s = symbol.as_str();
                if s.contains("list_map") {
                    Some(("map", &args[1], &args[0]))
                } else if s.contains("list_filter") {
                    Some(("filter", &args[1], &args[0]))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Emit list.map(list, fn) → new list with fn applied to each element.
    /// Uses scratch locals (not mem[]) to survive nested calls from call_indirect.
    /// Key insight: compute dst address BEFORE call_indirect so result goes
    /// directly onto the stack in the right position for store.
    pub(super) fn emit_list_map(&mut self, list_arg: &IrExpr, fn_arg: &IrExpr, ret_ty: &Ty) {
        use super::engine::layout::{LIST, list as ll};
        let list_data_off = self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32;
        let list_hdr = self.emitter.layout_reg.header_size(LIST) as i32;
        let in_elem_ty = self.resolve_list_elem(list_arg, Some(fn_arg));
        let mut out_elem_ty = if let Ty::Applied(_, args) = ret_ty {
            args.first().cloned().unwrap_or(Ty::Int)
        } else { Ty::Int };
        // When the return type is unresolved (TypeVar/Unknown), derive the output
        // element type from the lambda body.  The call-site ret_ty can remain
        // unresolved when the map result is unused or only used in a type-agnostic
        // context, but the lambda body always carries the concrete type.
        if out_elem_ty.is_unresolved() {
            if let IrExprKind::Lambda { body, .. } = &fn_arg.kind {
                if !body.ty.is_unresolved() {
                    out_elem_ty = body.ty.clone();
                }
            }
            // Also try fn_arg.ty.ret as a secondary source
            if out_elem_ty.is_unresolved() {
                if let Ty::Fn { ret, .. } = &fn_arg.ty {
                    if !ret.is_unresolved() {
                        out_elem_ty = *ret.clone();
                    }
                }
            }
        }
        // Final fallback: inspect the lifted closure's actual registered WASM
        // param/ret valtypes. This handles the case where inference left both
        // the list element and lambda body as Unknown but the lifted function
        // has a concrete signature (e.g. from our closure-conversion VarTable
        // propagation + anonymous record fallback).
        let in_vt = values::ty_to_valtype(&in_elem_ty);
        let in_elem_ty = match self.resolve_closure_param_valtype(fn_arg, 0) {
            Some(actual) if Some(actual) != in_vt => values::vt_to_placeholder_ty(actual),
            _ => in_elem_ty,
        };
        let out_vt = values::ty_to_valtype(&out_elem_ty);
        let out_elem_ty = match self.resolve_closure_ret_valtype(fn_arg) {
            Some(actual) if Some(actual) != out_vt => values::vt_to_placeholder_ty(actual),
            _ => out_elem_ty,
        };
        let in_size = values::byte_size(&in_elem_ty);
        let out_size = values::byte_size(&out_elem_ty);

        // SIMD detection: Int→Int with simple arithmetic lambda
        let simd_op = if matches!(&in_elem_ty, Ty::Int) && matches!(&out_elem_ty, Ty::Int) {
            Self::detect_simd_map_op(fn_arg)
        } else { None };

        let src_local = self.scratch.alloc_i32();
        let closure_local = self.scratch.alloc_i32();
        let dst_local = self.scratch.alloc_i32();
        let src_ptr = self.scratch.alloc_i32();
        let dst_ptr = self.scratch.alloc_i32();
        let end_ptr = self.scratch.alloc_i32();

        let direct_fn = self.try_resolve_direct_call(fn_arg);

        // Perceus in-place reuse: if source is single-use AND element sizes match,
        // skip allocation and write mapped results directly into the source list.
        let in_place = self.is_single_use_var(list_arg) && in_size == out_size;

        self.emit_expr(list_arg);
        wasm!(self.func, { local_set(src_local); });
        if direct_fn.is_none() && !matches!(&fn_arg.kind, almide_ir::IrExprKind::Lambda { .. }) {
            self.emit_expr(fn_arg);
            wasm!(self.func, { local_set(closure_local); });
        }
        let len_local = self.scratch.alloc_i32();
        if in_place {
            // In-place: dst = src (no allocation)
            wasm!(self.func, {
                local_get(src_local); i32_load(0); local_set(len_local);
                local_get(src_local); local_set(dst_local);
                local_get(src_local); i32_const(list_data_off); i32_add; local_set(src_ptr);
                local_get(src_local); i32_const(list_data_off); i32_add; local_set(dst_ptr);
                local_get(src_ptr); local_get(len_local); i32_const(in_size as i32); i32_mul; i32_add; local_set(end_ptr);
            });
        } else {
            wasm!(self.func, {
                local_get(src_local); i32_load(0); local_set(len_local);
                // alloc dst
                i32_const(list_hdr);
                local_get(len_local); i32_const(out_size as i32); i32_mul; i32_add;
                call(self.emitter.rt.alloc); local_set(dst_local);
                local_get(dst_local); local_get(len_local); i32_store(0);
                // Pointer-based iteration
                local_get(src_local); i32_const(list_data_off); i32_add; local_set(src_ptr);
                local_get(dst_local); i32_const(list_data_off); i32_add; local_set(dst_ptr);
                local_get(src_ptr); local_get(len_local); i32_const(in_size as i32); i32_mul; i32_add; local_set(end_ptr);
            });
        }

        // SIMD fast path: process 8 i64 elements per iteration (4× v128 unrolled)
        if let Some((simd_kind, simd_const_val)) = simd_op {
            let simd_end = self.scratch.alloc_i32();
            let use_shift = matches!(simd_kind, SimdMapOp::Mul)
                && simd_const_val > 0
                && (simd_const_val as u64).is_power_of_two();
            let simd_vec = if !use_shift { Some(self.scratch.alloc_v128()) } else { None };
            // simd_end = src_ptr + (len / 8) * 64  (round down to multiple of 8)
            wasm!(self.func, {
                local_get(src_ptr);
                local_get(len_local); i32_const(3); i32_shr_u; // len / 8
                i32_const(64); i32_mul;
                i32_add; local_set(simd_end);
            });
            if let Some(sv) = simd_vec {
                wasm!(self.func, {
                    i64_const(simd_const_val);
                    i64x2_splat;
                    local_set(sv);
                });
            }

            // Emit a single SIMD operation: load from src_ptr+offset, apply op, store to dst_ptr+offset
            let emit_simd_op = |fc: &mut FuncCompiler, offset: u64, sv: Option<u32>| {
                wasm!(fc.func, {
                    local_get(dst_ptr);
                    local_get(src_ptr);
                    v128_load(offset);
                });
                if use_shift {
                    let shift = (simd_const_val as u64).trailing_zeros();
                    wasm!(fc.func, { i32_const(shift as i32); });
                    fc.func.instruction(&wasm_encoder::Instruction::I64x2Shl);
                } else {
                    let sv = sv.unwrap();
                    wasm!(fc.func, { local_get(sv); });
                    match simd_kind {
                        SimdMapOp::Mul => { wasm!(fc.func, { i64x2_mul; }); }
                        SimdMapOp::Add => { wasm!(fc.func, { i64x2_add; }); }
                        SimdMapOp::Sub => { wasm!(fc.func, { i64x2_sub; }); }
                    }
                }
                wasm!(fc.func, { v128_store(offset); });
            };

            wasm!(self.func, {
                block_empty; loop_empty;
                  local_get(src_ptr); local_get(simd_end); i32_ge_u; br_if(1);
            });
            // 4× unrolled: process 8 elements (64 bytes) per iteration
            emit_simd_op(self, 0, simd_vec);
            emit_simd_op(self, 16, simd_vec);
            emit_simd_op(self, 32, simd_vec);
            emit_simd_op(self, 48, simd_vec);
            wasm!(self.func, {
                  local_get(src_ptr); i32_const(64); i32_add; local_set(src_ptr);
                  local_get(dst_ptr); i32_const(64); i32_add; local_set(dst_ptr);
                  br(0);
                end; end;
            });
            if let Some(sv) = simd_vec { self.scratch.free_v128(sv); }
            self.scratch.free_i32(simd_end);
        }

        // Scalar loop (handles tail after SIMD, or all elements if no SIMD)
        wasm!(self.func, {
            block_empty; loop_empty;
        });
        let depth_guard = self.depth_push_n(2);

        wasm!(self.func, {
            local_get(src_ptr); local_get(end_ptr); i32_ge_u; br_if(1);
            // dst addr on stack
            local_get(dst_ptr);
        });
        if let almide_ir::IrExprKind::Lambda { params, body, .. } = &fn_arg.kind {
            let param_var = params.first().map(|(v, _)| *v);
            let param_local = self.scratch.alloc(values::ty_to_valtype(&in_elem_ty).unwrap_or(ValType::I32));
            wasm!(self.func, { local_get(src_ptr); });
            self.emit_load_at(&in_elem_ty, 0);
            wasm!(self.func, { local_set(param_local); });
            // Bind param var to local
            if let Some(vid) = param_var {
                self.var_map.insert(vid.0, param_local);
            }
            self.emit_expr(body);
            // Clean up
            if let Some(vid) = param_var {
                self.var_map.remove(&vid.0);
            }
            self.scratch.free(param_local, values::ty_to_valtype(&in_elem_ty).unwrap_or(ValType::I32));
        } else if let Some(fn_idx) = direct_fn {
            wasm!(self.func, {
                i32_const(0);
                local_get(src_ptr);
            });
            self.emit_load_at(&in_elem_ty, 0);
            wasm!(self.func, { call(fn_idx); });
        } else {
            wasm!(self.func, {
                local_get(closure_local); i32_load(4);
                local_get(src_ptr);
            });
            self.emit_load_at(&in_elem_ty, 0);
            wasm!(self.func, { local_get(closure_local); i32_load(0); });
            self.emit_closure_call(&in_elem_ty, &out_elem_ty);
        }
        self.emit_store_at(&out_elem_ty, 0);

        wasm!(self.func, {
            local_get(src_ptr); i32_const(in_size as i32); i32_add; local_set(src_ptr);
            local_get(dst_ptr); i32_const(out_size as i32); i32_add; local_set(dst_ptr);
            br(0);
        });
        self.depth_pop(depth_guard);
        wasm!(self.func, { end; end; });

        // Perceus: if source was single-use and NOT in-place, free via rc_dec.
        if !in_place && self.is_single_use_var(list_arg) {
            wasm!(self.func, { local_get(src_local); call(self.emitter.rt.rc_dec); });
        }

        wasm!(self.func, { local_get(dst_local); });

        self.scratch.free_i32(len_local);
        self.scratch.free_i32(end_ptr);
        self.scratch.free_i32(dst_ptr);
        self.scratch.free_i32(src_ptr);
        self.scratch.free_i32(dst_local);
        self.scratch.free_i32(closure_local);
        self.scratch.free_i32(src_local);
    }

    /// Check if a list expression is consumed exactly once (safe for in-place reuse).
    /// True for: single-use variables (use_count == 1) OR temporary expressions
    /// (Call results, RuntimeCall results) that are not bound to any variable.
    fn is_single_use_var(&self, expr: &IrExpr) -> bool {
        // Under real frees (ALMIDE_WASM_FREES=1) the IR-level Perceus pass is
        // the ONLY owner of ownership decisions: it Decs every heap VDecl at
        // scope end, so an emitter-level in-place reuse or raw rc_dec here
        // would create a SECOND owner of the same block — the double-free the
        // sentinel now catches at teardown (caught by the byte gate on
        // wasm_list_map). The reuse this disables is a micro-optimization;
        // block recycling still happens through the free list. Re-enabling it
        // as an IR-level move (visible to the verifier) is the tracked
        // follow-up in wasm-frees-ownership-discipline.md.
        if super::runtime::wasm_frees_enabled() { return false; }
        match &expr.kind {
            IrExprKind::Var { id } => self.var_table.get(*id).use_count == 1,
            // Temporary expression results: consumed exactly here, never aliased
            IrExprKind::Call { .. }
            | IrExprKind::TailCall { .. }
            | IrExprKind::RuntimeCall { .. } => true,
            _ => false,
        }
    }

    fn detect_simd_map_op(fn_arg: &IrExpr) -> Option<(SimdMapOp, i64)> {
        if let IrExprKind::Lambda { params, body, .. } = &fn_arg.kind {
            let param_id = params.first().map(|(v, _)| v.0)?;
            match &body.kind {
                IrExprKind::BinOp { op, left, right } => {
                    let (var_side, lit_side, commutative) = match op {
                        BinOp::MulInt => (true, true, true),
                        BinOp::AddInt => (true, true, true),
                        BinOp::SubInt => (true, true, false), // x - k only
                        _ => return None,
                    };
                    let _ = (var_side, lit_side, commutative);
                    let simd_kind = match op {
                        BinOp::MulInt => SimdMapOp::Mul,
                        BinOp::AddInt => SimdMapOp::Add,
                        BinOp::SubInt => SimdMapOp::Sub,
                        _ => unreachable!(),
                    };
                    // x op k
                    if let (IrExprKind::Var { id }, IrExprKind::LitInt { value: k }) = (&left.kind, &right.kind) {
                        if id.0 == param_id { return Some((simd_kind, *k)); }
                    }
                    // k op x (commutative only)
                    if matches!(op, BinOp::MulInt | BinOp::AddInt) {
                        if let (IrExprKind::LitInt { value: k }, IrExprKind::Var { id }) = (&left.kind, &right.kind) {
                            if id.0 == param_id { return Some((simd_kind, *k)); }
                        }
                    }
                    None
                }
                _ => None,
            }
        } else {
            None
        }
    }
}
