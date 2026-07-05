// List stdlib closure-based call dispatch for WASM codegen (part 2, group 4).
//
// Sub-dispatch group for `emit_list_closure_call2`: filter, fold, map.
// Included into `calls_list_closure2.rs` via `include!`; shares its module
// imports and the `FuncCompiler` impl. Arm patterns are DISJOINT from the
// other groups so chain order is irrelevant.

impl FuncCompiler<'_> {
    /// `emit_list_closure_call2` group 4. Returns true if handled.
    fn emit_list_closure_call2_g4(&mut self, method: &str, args: &[IrExpr]) -> bool {
        use super::engine::layout::{LIST, list as ll};
        let list_data_off = self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32;
        let list_hdr = self.emitter.layout_reg.header_size(LIST) as i32;
        match method {
            "filter" => {
                // filter(list, fn) → new list with matching elements
                // Pointer-based iteration + branchless write
                let elem_ty = self.resolve_list_elem(&args[0], args.get(1));
                let elem_size = values::byte_size(&elem_ty);
                // Perceus in-place reuse: single-use source → write results into same buffer
                let in_place = self.is_single_use_var(&args[0]);
                let src = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let src_ptr = self.scratch.alloc_i32();
                let end_ptr = self.scratch.alloc_i32();
                let dst_ptr = self.scratch.alloc_i32();
                let out_count = self.scratch.alloc_i32();
                let is_inline_lambda = matches!(&args[1].kind, almide_ir::IrExprKind::Lambda { .. });
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(src); });
                if !is_inline_lambda {
                    self.emit_expr(&args[1]);
                    wasm!(self.func, { local_set(closure); });
                }
                if in_place {
                    // In-place: dst = src (compact matching elements to front)
                    wasm!(self.func, {
                        i32_const(0); local_set(out_count);
                        local_get(src); local_set(dst);
                        local_get(src); i32_const(list_data_off); i32_add;
                        local_tee(src_ptr);
                        local_get(src); i32_load(0); i32_const(elem_size as i32); i32_mul;
                        i32_add; local_set(end_ptr);
                        local_get(src); i32_const(list_data_off); i32_add;
                        local_set(dst_ptr);
                        block_empty; loop_empty;
                    });
                } else {
                    wasm!(self.func, {
                        // Alloc max-size output
                        i32_const(list_hdr);
                        local_get(src); i32_load(0);
                        i32_const(elem_size as i32); i32_mul; i32_add;
                        call(self.emitter.rt.alloc); local_set(dst);
                        i32_const(0); local_set(out_count);
                        // src_ptr = src + DATA_OFFSET
                        local_get(src); i32_const(list_data_off); i32_add;
                        local_tee(src_ptr);
                        // end_ptr = src_ptr + len * elem_size
                        local_get(src); i32_load(0); i32_const(elem_size as i32); i32_mul;
                        i32_add; local_set(end_ptr);
                        // dst_ptr = dst + DATA_OFFSET
                        local_get(dst); i32_const(list_data_off); i32_add;
                        local_set(dst_ptr);
                        block_empty; loop_empty;
                    });
                }
                let depth_guard = self.depth_push_n(2);
                wasm!(self.func, {
                    local_get(src_ptr); local_get(end_ptr); i32_ge_u; br_if(1);
                });
                let filter_param_local;
                if let almide_ir::IrExprKind::Lambda { params, body, .. } = &args[1].kind {
                    let param_var = params.first().map(|(v, _)| *v);
                    let pvt = values::ty_to_valtype(&elem_ty).unwrap_or(ValType::I32);
                    filter_param_local = Some((self.scratch.alloc(pvt), pvt));
                    let pl = filter_param_local.unwrap().0;
                    wasm!(self.func, { local_get(src_ptr); });
                    self.emit_load_at(&elem_ty, 0);
                    wasm!(self.func, { local_set(pl); });
                    if let Some(vid) = param_var {
                        self.var_map.insert(vid.0, pl);
                    }
                    self.emit_expr(body);
                    if let Some(vid) = param_var {
                        self.var_map.remove(&vid.0);
                    }
                } else {
                    filter_param_local = None;
                    wasm!(self.func, {
                        local_get(closure); i32_load(4); // env
                        local_get(src_ptr);
                    });
                    self.emit_load_at(&elem_ty, 0);
                    wasm!(self.func, { local_get(closure); i32_load(0); }); // table_idx
                    self.emit_closure_call(&elem_ty, &Ty::Bool);
                }
                // Branchless: always write, conditionally advance dst_ptr
                let pred_result = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_set(pred_result);
                    local_get(dst_ptr);
                });
                if let Some((pl, _)) = filter_param_local {
                    wasm!(self.func, { local_get(pl); });
                } else {
                    wasm!(self.func, { local_get(src_ptr); });
                    self.emit_load_at(&elem_ty, 0);
                }
                self.emit_store_at(&elem_ty, 0);
                wasm!(self.func, {
                    // dst_ptr += pred_result * elem_size (branchless)
                    local_get(dst_ptr);
                    local_get(pred_result); i32_const(elem_size as i32); i32_mul;
                    i32_add; local_set(dst_ptr);
                    // out_count += pred_result
                    local_get(out_count); local_get(pred_result); i32_add; local_set(out_count);
                    // src_ptr += elem_size
                    local_get(src_ptr); i32_const(elem_size as i32); i32_add; local_set(src_ptr);
                    br(0);
                });
                self.scratch.free_i32(pred_result);
                self.depth_pop(depth_guard);
                wasm!(self.func, {
                    end; end;
                    local_get(dst); local_get(out_count); i32_store(0);
                });
                // SHARE dup: kept elements are second references into the
                // surviving source list. Walk dst[0..out_count] once —
                // per-store incs would over-count rejected slots.
                if crate::pass_perceus::is_heap_type(&elem_ty) {
                    let wi = self.scratch.alloc_i32();
                    wasm!(self.func, {
                        i32_const(0); local_set(wi);
                        block_empty; loop_empty;
                            local_get(wi); local_get(out_count); i32_ge_u; br_if(1);
                            local_get(dst); i32_const(list_data_off); i32_add;
                            local_get(wi); i32_const(elem_size as i32); i32_mul; i32_add;
                            i32_load(0); call(self.emitter.rt.rc_inc); drop;
                            local_get(wi); i32_const(1); i32_add; local_set(wi);
                            br(0);
                        end; end;
                    });
                    self.scratch.free_i32(wi);
                }
                wasm!(self.func, { local_get(dst); });
                if let Some((pl, pvt)) = filter_param_local {
                    self.scratch.free(pl, pvt);
                }
                self.scratch.free_i32(out_count);
                self.scratch.free_i32(dst_ptr);
                self.scratch.free_i32(end_ptr);
                self.scratch.free_i32(src_ptr);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(src);
            }
            "fold" => {
                // fold(list, init, fn(acc, elem) → acc)
                // Resolve types from closure Fn signature when available
                // Derive element type from the input list (most reliable source)
                let list_elem_ty = self.resolve_list_elem(&args[0], None);

                // Resolve elem_ty: use list element type, Fn param, or lambda param — first concrete wins
                let elem_ty = [
                    Some(list_elem_ty),
                    if let Ty::Fn { params, .. } = &args[2].ty { params.get(1).cloned() } else { None },
                    if let almide_ir::IrExprKind::Lambda { params: lp, .. } = &args[2].kind { lp.get(1).map(|(_, t)| t.clone()) } else { None },
                ].into_iter().flatten()
                    .find(|t| !t.is_unresolved())
                    .unwrap_or(Ty::Int);

                // Resolve acc type: use Fn return type or lambda body type, with TypeVar→concrete fallback
                let acc_ty_resolved = {
                    // Try closure Fn ret type
                    let fn_ret = if let Ty::Fn { ret, .. } = &args[2].ty { Some(*ret.clone()) } else { None };
                    // Try lambda body type
                    let body_ty = if let almide_ir::IrExprKind::Lambda { body, .. } = &args[2].kind { Some(body.ty.clone()) } else { None };
                    // Try init type
                    let init_ty = args[1].ty.clone();
                    // Pick first concrete (non-TypeVar/Unknown) type
                    [fn_ret, body_ty, Some(init_ty)].into_iter().flatten()
                        .find(|t| !t.is_unresolved())
                        .unwrap_or_else(|| if let Ty::Fn { ret, .. } = &args[2].ty { *ret.clone() } else { args[1].ty.clone() })
                };
                // Resolve TypeVar inside Applied types (e.g., List[TypeVar(?0)] → List[elem_ty])
                let acc_ty_resolved = match acc_ty_resolved {
                    Ty::Applied(id, ref inner) if inner.iter().any(|t| matches!(t, Ty::TypeVar(_))) => {
                        let resolved_inner: Vec<Ty> = inner.iter().map(|t| {
                            if matches!(t, Ty::TypeVar(_)) { elem_ty.clone() } else { t.clone() }
                        }).collect();
                        Ty::Applied(id, resolved_inner)
                    }
                    other => other,
                };
                let elem_size = values::byte_size(&elem_ty);
                let acc_vt = values::ty_to_valtype(&acc_ty_resolved).unwrap_or(ValType::I32);
                let list_ptr = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let acc = self.scratch.alloc(acc_vt);
                let is_inline_lambda = matches!(&args[2].kind, almide_ir::IrExprKind::Lambda { .. });

                // ── Stream Fusion: detect map/filter pipeline feeding into fold ──
                let pipeline = self.detect_pipeline(&args[0]);
                if !pipeline.is_empty() && is_inline_lambda {
                    // Fused pipeline: iterate source with pointer-based iteration
                    let (source_expr, stages) = self.extract_pipeline(&args[0]);
                    self.emit_expr(source_expr);
                    wasm!(self.func, { local_set(list_ptr); });
                    let source_elem_ty = self.resolve_list_elem(source_expr, None);
                    let source_elem_size = values::byte_size(&source_elem_ty);
                    // See the non-fused path: dup an alias accumulator seed so the
                    // fold loop owns its reference and the caller's Dec of the seed
                    // local cannot double-free the (empty-list) returned result.
                    self.emit_stored_field(&args[1]);
                    wasm!(self.func, { local_set(acc); });
                    // Pointer-based iteration: ptr and end instead of idx
                    let ptr = self.scratch.alloc_i32();
                    let end_ptr = self.scratch.alloc_i32();
                    wasm!(self.func, {
                        // ptr = list_ptr + DATA_OFFSET
                        local_get(list_ptr); i32_const(list_data_off); i32_add;
                        local_set(ptr);
                        // end = ptr + len * elem_size
                        local_get(ptr);
                        local_get(list_ptr); i32_load(0); i32_const(source_elem_size as i32); i32_mul;
                        i32_add; local_set(end_ptr);
                        block_empty; loop_empty;
                    });
                    let depth_guard = self.depth_push_n(2);
                    wasm!(self.func, {
                        local_get(ptr); local_get(end_ptr); i32_ge_u; br_if(1);
                    });
                    // Load source element via pointer
                    let mut cur_ty = source_elem_ty.clone();
                    let mut cur_vt = values::ty_to_valtype(&cur_ty).unwrap_or(ValType::I32);
                    let mut cur_local = self.scratch.alloc(cur_vt);
                    wasm!(self.func, { local_get(ptr); });
                    self.emit_load_at(&cur_ty, 0);
                    wasm!(self.func, { local_set(cur_local); });
                    // Apply each pipeline stage
                    let mut skip_label_depth = 0u32;
                    for stage in &stages {
                        match stage {
                            PipelineStage::Map(lambda) => {
                                if let almide_ir::IrExprKind::Lambda { params, body, .. } = &lambda.kind {
                                    if let Some((vid, _)) = params.first() {
                                        self.var_map.insert(vid.0, cur_local);
                                    }
                                    self.emit_expr(body);
                                    // Map may change the value type (e.g. Tuple → Float).
                                    // Re-alloc cur_local with the correct type.
                                    let new_ty = body.ty.clone();
                                    let new_vt = values::ty_to_valtype(&new_ty).unwrap_or(ValType::I32);
                                    if new_vt != cur_vt {
                                        let new_local = self.scratch.alloc(new_vt);
                                        wasm!(self.func, { local_set(new_local); });
                                        self.scratch.free(cur_local, cur_vt);
                                        cur_local = new_local;
                                        cur_vt = new_vt;
                                    } else {
                                        wasm!(self.func, { local_set(cur_local); });
                                    }
                                    if let Some((vid, _)) = params.first() {
                                        self.var_map.remove(&vid.0);
                                    }
                                    cur_ty = new_ty;
                                }
                            }
                            PipelineStage::Filter(lambda) => {
                                if let almide_ir::IrExprKind::Lambda { params, body, .. } = &lambda.kind {
                                    if let Some((vid, _)) = params.first() {
                                        self.var_map.insert(vid.0, cur_local);
                                    }
                                    self.emit_expr(body);
                                    if let Some((vid, _)) = params.first() {
                                        self.var_map.remove(&vid.0);
                                    }
                                    // If false, skip to next iteration (ptr += elem_size then br to loop)
                                    wasm!(self.func, {
                                        i32_eqz;
                                        if_empty;
                                          local_get(ptr); i32_const(source_elem_size as i32); i32_add; local_set(ptr);
                                          br(1); // br to loop_empty
                                        end;
                                    });
                                }
                            }
                        }
                    }
                    // Apply fold body with cur_local as element
                    if let almide_ir::IrExprKind::Lambda { params, body, .. } = &args[2].kind {
                        let acc_param = params.first().map(|(v, _)| *v);
                        let elem_param = params.get(1).map(|(v, _)| *v);
                        if let Some(vid) = acc_param { self.var_map.insert(vid.0, acc); }
                        if let Some(vid) = elem_param { self.var_map.insert(vid.0, cur_local); }
                        self.emit_expr(body);
                        wasm!(self.func, { local_set(acc); });
                        if let Some(vid) = acc_param { self.var_map.remove(&vid.0); }
                        if let Some(vid) = elem_param { self.var_map.remove(&vid.0); }
                    }
                    self.scratch.free(cur_local, cur_vt);
                    wasm!(self.func, {
                        local_get(ptr); i32_const(source_elem_size as i32); i32_add; local_set(ptr);
                        br(0);
                    });
                    self.depth_pop(depth_guard);
                    wasm!(self.func, { end; end; local_get(acc); });
                    self.scratch.free_i32(end_ptr);
                    self.scratch.free_i32(ptr);
                    self.scratch.free(acc, acc_vt);
                    self.scratch.free_i32(idx);
                    self.scratch.free_i32(len);
                    self.scratch.free_i32(closure);
                    self.scratch.free_i32(list_ptr);
                    return true;
                }

                // Pointer-based iteration for fold
                let ptr = self.scratch.alloc_i32();
                let end_ptr = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(list_ptr); });
                // The accumulator is MOVE-STORED into the fold loop and returned
                // as the result (unchanged when the list is empty). Dup an alias
                // seed so the loop owns its own reference and the caller's
                // scope-end Dec of the seed local does not double-free the result.
                self.emit_stored_field(&args[1]);
                wasm!(self.func, { local_set(acc); });
                if !is_inline_lambda {
                    self.emit_expr(&args[2]);
                    wasm!(self.func, { local_set(closure); });
                }
                wasm!(self.func, {
                    // ptr = list_ptr + DATA_OFFSET
                    local_get(list_ptr); i32_const(list_data_off); i32_add;
                    local_tee(ptr);
                    // end_ptr = ptr + len * elem_size
                    local_get(list_ptr); i32_load(0); i32_const(elem_size as i32); i32_mul;
                    i32_add; local_set(end_ptr);
                    block_empty; loop_empty;
                });
                let depth_guard = self.depth_push_n(2);
                wasm!(self.func, {
                    local_get(ptr); local_get(end_ptr); i32_ge_u; br_if(1);
                });
                if let almide_ir::IrExprKind::Lambda { params, body, .. } = &args[2].kind {
                    let acc_param = params.first().map(|(v, _)| *v);
                    let elem_param = params.get(1).map(|(v, _)| *v);
                    let elem_local = self.scratch.alloc(values::ty_to_valtype(&elem_ty).unwrap_or(ValType::I32));
                    wasm!(self.func, { local_get(ptr); });
                    self.emit_load_at(&elem_ty, 0);
                    wasm!(self.func, { local_set(elem_local); });
                    if let Some(vid) = acc_param {
                        self.var_map.insert(vid.0, acc);
                    }
                    if let Some(vid) = elem_param {
                        self.var_map.insert(vid.0, elem_local);
                    }
                    self.emit_expr(body);
                    if let Some(vid) = acc_param {
                        self.var_map.remove(&vid.0);
                    }
                    if let Some(vid) = elem_param {
                        self.var_map.remove(&vid.0);
                    }
                    self.scratch.free(elem_local, values::ty_to_valtype(&elem_ty).unwrap_or(ValType::I32));
                } else {
                    wasm!(self.func, {
                        local_get(closure); i32_load(4); // env
                        local_get(acc);
                        local_get(ptr);
                    });
                    self.emit_load_at(&elem_ty, 0);
                    wasm!(self.func, { local_get(closure); i32_load(0); }); // table_idx
                    {
                        let mut ct = vec![ValType::I32]; // env
                        if let Some(vt) = values::ty_to_valtype(&acc_ty_resolved) { ct.push(vt); }
                        if let Some(vt) = values::ty_to_valtype(&elem_ty) { ct.push(vt); }
                        self.emit_call_indirect(ct, values::ret_type(&acc_ty_resolved));
                    }
                }
                wasm!(self.func, {
                    local_set(acc);
                    local_get(ptr); i32_const(elem_size as i32); i32_add; local_set(ptr);
                    br(0);
                });
                self.depth_pop(depth_guard);
                wasm!(self.func, { end; end; local_get(acc); });
                self.scratch.free_i32(end_ptr);
                self.scratch.free_i32(ptr);
                self.scratch.free(acc, acc_vt);
                self.scratch.free_i32(idx);
                self.scratch.free_i32(len);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(list_ptr);
            }
            "map" => {
                let ret_ty = self.stub_ret_ty.clone();
                self.emit_list_map(&args[0], &args[1], &ret_ty);
            }
            _ => return false,
        }
        true
    }
}
