impl FuncCompiler<'_> {
    /// Emit a tail call (WASM `return_call` / `return_call_indirect`).
    /// Falls back to normal call for targets that don't resolve to a known function.
    /// Hybrid fallback for `RuntimeCall` whose `symbol` is not registered
    /// in `func_map`. Routes through the legacy `emit_<module>_call`
    /// dispatcher so fns still lowered via inline i64 ops (e.g. `int.abs`)
    /// keep working until their WASM runtime fn lands. Returns `true` on
    /// successful dispatch, `false` otherwise.
    pub(super) fn dispatch_runtime_fallback(
        &mut self,
        module: &str,
        func: &str,
        args: &[IrExpr],
        ret_ty: &Ty,
    ) -> bool {
        let _ = ret_ty;
        // Sized numeric conversion: `int.to_uint8` / `float.to_int32` /
        // `int64.to_float64` / ... flow through the name-driven
        // `emit_sized_conv_call` that covers the full kind×width
        // matrix. Before `@intrinsic` migration these rode the Module
        // dispatcher at line ~356 of this file, but the post-migration
        // `RuntimeCall` path lands here instead.
        if (func.starts_with("to_") || func.starts_with("from_"))
            && sized_type_info(module).is_some()
        {
            if self.emit_sized_conv_call(module, func, args) {
                return true;
            }
        }
        match module {
            "int" => self.emit_int_call(func, args),
            "float" => self.emit_float_call(func, args),
            "math" => self.emit_math_call(func, args),
            "string" => self.emit_string_call(func, args),
            "list" => self.emit_list_call(func, args),
            "map" => self.emit_map_call(func, args),
            "set" => self.emit_set_call(func, args),
            "option" => self.emit_option_call(func, args),
            "result" => self.emit_result_call(func, args),
            "bytes" => self.emit_bytes_call(func, args),
            "matrix" => self.emit_matrix_call(func, args),
            "io" => { self.emit_io_call(func, args); true }
            "regex" => { self.emit_regex_call(func, args); true }
            "value" => { self.emit_value_call(func, args); true }
            "http" => { self.emit_http_call(func, args); true }
            "datetime" => { self.emit_datetime_call(func, args); true }
            "process" => { self.emit_process_call(func, args); true }
            "random" => { self.emit_random_call(func, args); true }
            "env" => { self.emit_env_call(func, args); true }
            "fs" => { self.emit_fs_call(func, args); true }
            "json" => { self.emit_json_call(func, args); true }
            "testing" => {
                // Route to Named dispatch: `testing.assert_contains`
                // → `assert_contains` (handlers live under the
                // hardcoded Named dispatch at calls.rs ~line 178).
                // Mirror Module-dispatch's mono-suffix strip so
                // specialized fns (e.g. `assert_some__String`) still
                // route to the base handler.
                let base = func.split_once("__").map(|(b, _)| b).unwrap_or(func);
                let target = CallTarget::Named { name: almide_base::intern::sym(base) };
                self.emit_call(&target, args, ret_ty);
                true
            }
            "error" => {
                // `error.message` / `error.context` have inline emit
                // arms under the Module dispatcher but are reachable
                // here via `@intrinsic` → `RuntimeCall` when no WASM
                // runtime fn is registered. Re-dispatch through the
                // Module path so the inline arms stay the single
                // source of truth.
                let target = CallTarget::Module {
                    module: almide_base::intern::sym("error"),
                    func: almide_base::intern::sym(func),
                    def_id: None,
                };
                self.emit_call(&target, args, ret_ty);
                true
            }
            _ => false,
        }
    }

    pub(super) fn emit_tail_call(&mut self, target: &CallTarget, args: &[IrExpr], ret_ty: &Ty) {
        match target {
            CallTarget::Named { name } => {
                // Only user-defined functions get return_call; builtins fall back to normal call
                if let Some(&func_idx) = self.emitter.func_map.get(name.as_str()) {
                    for arg in args {
                        self.emit_expr(arg);
                    }
                    wasm!(self.func, { return_call(func_idx); });
                } else {
                    // Not a user function (builtin/stub) — fall back to normal call
                    self.emit_call(target, args, ret_ty);
                }
            }
            CallTarget::Computed { callee } => {
                // Closure tail call: same setup as emit_call but return_call_indirect
                let scratch = self.scratch.alloc_i32();
                self.emit_expr(callee);
                wasm!(self.func, { local_set(scratch); });

                // Push env_ptr (first hidden arg)
                wasm!(self.func, {
                    local_get(scratch);
                    i32_load(4);
                });

                for arg in args {
                    self.emit_expr(arg);
                }

                // Push table_idx
                wasm!(self.func, {
                    local_get(scratch);
                    i32_load(0);
                });

                let callee_fn_ty = match &callee.ty {
                    Ty::Fn { .. } => callee.ty.clone(),
                    _ => {
                        if let almide_ir::IrExprKind::Var { id } = &callee.kind {
                            self.var_table.get(*id).ty.clone()
                        } else {
                            callee.ty.clone()
                        }
                    }
                };
                if let Ty::Fn { params, ret } = &callee_fn_ty {
                    let mut closure_params = vec![ValType::I32];
                    for p in params {
                        if let Some(vt) = values::ty_to_valtype(p) {
                            closure_params.push(vt);
                        }
                    }
                    let ret_types = values::ret_type(ret);
                    let type_idx = self.emitter.register_type(closure_params, ret_types);
                    wasm!(self.func, { return_call_indirect(type_idx, 0); });
                } else {
                    panic!(
                        "[ICE] emit_wasm: tail closure call through a non-Fn type — \
                         the call signature cannot be built; resolve upstream"
                    );
                }
                self.scratch.free_i32(scratch);
            }
            // Module/Method calls in tail position — fall back to normal call
            _ => {
                self.emit_call(target, args, ret_ty);
            }
        }
    }

    /// Emit a safe default value for a given type.
    /// String → empty string, List → empty list, Option → none, Bool → false, etc.
    pub(super) fn emit_typed_default(&mut self, ty: &Ty) {
        use almide_lang::types::constructor::TypeConstructorId;
        match ty {
            Ty::Int => { wasm!(self.func, { i64_const(0); }); }
            Ty::Float => { wasm!(self.func, { f64_const(0.0); }); }
            Ty::Bool => { wasm!(self.func, { i32_const(0); }); }
            Ty::String => {
                // Empty string: alloc string_hdr() bytes, len=0
                let tmp = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(0); call(self.emitter.rt.string_alloc);
                    local_set(tmp);
                    local_get(tmp);
                    i32_const(0); i32_store(0);
                    local_get(tmp);
                });
                self.scratch.free_i32(tmp);
            }
            Ty::Applied(TypeConstructorId::List, _) => {
                // Empty list: alloc 4 bytes, len=0
                let tmp = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc);
                    local_set(tmp);
                    local_get(tmp);
                    i32_const(0); i32_store(0);
                    local_get(tmp);
                });
                self.scratch.free_i32(tmp);
            }
            Ty::Applied(TypeConstructorId::Option, _) => {
                // none
                wasm!(self.func, { i32_const(0); });
            }
            Ty::Applied(TypeConstructorId::Result, _) => {
                // err("stub") — tag=1, value=empty string
                let tmp = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc);
                    local_set(tmp);
                    local_get(tmp);
                    i32_const(1); i32_store(0); // tag=err
                    local_get(tmp);
                    i32_const(0); call(self.emitter.rt.string_alloc);
                    i32_store(4); // empty string at offset 4
                    local_get(tmp);
                });
                self.scratch.free_i32(tmp);
            }
            Ty::Unit => { /* no value */ }
            _ => {
                // Generic pointer type: return 0 (null)
                // For records/tuples, this may still crash on field access.
                match values::ty_to_valtype(ty) {
                    Some(ValType::I64) => { wasm!(self.func, { i64_const(0); }); }
                    Some(ValType::F64) => { wasm!(self.func, { f64_const(0.0); }); }
                    Some(ValType::I32) => { wasm!(self.func, { i32_const(0); }); }
                    None => {}
                    _ => { wasm!(self.func, { i32_const(0); }); }
                }
            }
        }
    }

    /// UFCS fallback: try func_map lookup for user-defined functions.
    /// Panics on miss — a resolved call should never reach here unresolved.
    fn emit_ufcs_fallback(&mut self, object: &IrExpr, method: &str, args: &[IrExpr]) {
        // Try bare method name in func_map (user-defined function)
        let bare = method.split('.').last().unwrap_or(method);
        if let Some(&func_idx) = self.emitter.func_map.get(bare) {
            self.emit_expr(object);
            for arg in args { self.emit_expr(arg); }
            wasm!(self.func, { call(func_idx); });
            return;
        }
        // Also try full method name
        if let Some(&func_idx) = self.emitter.func_map.get(method) {
            self.emit_expr(object);
            for arg in args { self.emit_expr(arg); }
            wasm!(self.func, { call(func_idx); });
            return;
        }
        panic!(
            "[ICE] emit_wasm: UFCS fallback for `.{}` (object ty={:?}) found \
             no entry in func_map — resolve upstream or register the function",
            method, object.ty
        );
    }
}
