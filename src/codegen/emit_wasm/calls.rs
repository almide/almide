//! Function call emission — emit_call and related helpers.

use crate::ir::{CallTarget, IrExpr};
use crate::types::Ty;
use wasm_encoder::ValType;

use super::FuncCompiler;
use super::values;
use super::wasm_macro::wasm;

impl FuncCompiler<'_> {
    pub(super) fn emit_call(&mut self, target: &CallTarget, args: &[IrExpr], _ret_ty: &Ty) {
        // Set return type context for stub calls
        self.stub_ret_ty = _ret_ty.clone();
        match target {
            CallTarget::Named { name } => {
                match name.as_str() {
                    "println" => {
                        let arg = &args[0];
                        match &arg.ty {
                            Ty::String => {
                                self.emit_expr(arg);
                                wasm!(self.func, { call(self.emitter.rt.println_str); });
                            }
                            Ty::Int => {
                                self.emit_expr(arg);
                                wasm!(self.func, { call(self.emitter.rt.println_int); });
                            }
                            Ty::Bool => {
                                // Convert bool to "true"/"false"
                                self.emit_expr(arg);
                                let true_str = self.emitter.intern_string("true");
                                let false_str = self.emitter.intern_string("false");
                                wasm!(self.func, {
                                    if_i32;
                                    i32_const(true_str as i32);
                                    else_;
                                    i32_const(false_str as i32);
                                    end;
                                    call(self.emitter.rt.println_str);
                                });
                            }
                            Ty::Float => {
                                self.emit_expr(arg);
                                wasm!(self.func, {
                                    call(self.emitter.rt.float_to_string);
                                    call(self.emitter.rt.println_str);
                                });
                            }
                            _ => {
                                // Unsupported type: skip arg and print "<unsupported>"
                                let s = self.emitter.intern_string("<unsupported>");
                                wasm!(self.func, {
                                    i32_const(s as i32);
                                    call(self.emitter.rt.println_str);
                                });
                            }
                        }
                    }
                    "assert_eq" => {
                        self.emit_assert_eq(&args[0], &args[1]);
                    }
                    "assert" => {
                        // assert(cond) or assert(cond, msg) — trap if false
                        self.emit_expr(&args[0]);
                        wasm!(self.func, {
                            i32_eqz;
                            if_empty;
                            unreachable;
                            end;
                        });
                        // Drop message arg if present (evaluated but unused)
                    }
                    "assert_ne" => {
                        // assert_ne(left, right) — trap if equal
                        self.emit_eq(&args[0], &args[1], false);
                        // If equal → trap
                        wasm!(self.func, {
                            if_empty;
                            unreachable;
                            end;
                        });
                    }
                    // Codec runtime value constructors (auto-derived)
                    "almide_rt_value_null" => {
                        self.emit_value_call("null", args);
                    }
                    "almide_rt_value_array" => {
                        self.emit_value_call("array", args);
                    }
                    "almide_rt_value_object" => {
                        self.emit_value_call("object", args);
                    }
                    "almide_rt_value_tagged_variant" => {
                        // tagged_variant(v: Value) → (String, Value): extract tag+payload from object
                        // Value object expected to have "tag" and "value" keys
                        self.emit_value_tagged_variant(args);
                    }
                    _ => {
                        // Check if this is a variant constructor
                        if let Some((tag, is_unit)) = self.find_variant_ctor_tag(name) {
                            if is_unit && args.is_empty() {
                                // Unit variant: allocate [tag:i32]
                                let scratch = self.scratch.alloc_i32();
                                wasm!(self.func, {
                                    i32_const(4);
                                    call(self.emitter.rt.alloc);
                                    local_set(scratch);
                                    local_get(scratch);
                                    i32_const(tag as i32);
                                    i32_store(0);
                                    local_get(scratch);
                                });
                                self.scratch.free_i32(scratch);
                                return;
                            } else if !is_unit {
                                // Tuple payload variant: [tag:i32][arg0][arg1]...
                                let mut total_size = 4u32; // tag
                                for arg in args { total_size += values::byte_size(&arg.ty); }
                                let scratch = self.scratch.alloc_i32();
                                wasm!(self.func, {
                                    i32_const(total_size as i32);
                                    call(self.emitter.rt.alloc);
                                    local_set(scratch);
                                    // Write tag
                                    local_get(scratch);
                                    i32_const(tag as i32);
                                    i32_store(0);
                                });
                                // Write args
                                let mut offset = 4u32;
                                for arg in args {
                                    wasm!(self.func, { local_get(scratch); });
                                    self.emit_expr(arg);
                                    self.emit_store_at(&arg.ty, offset);
                                    offset += values::byte_size(&arg.ty);
                                }
                                wasm!(self.func, { local_get(scratch); });
                                self.scratch.free_i32(scratch);
                                return;
                            }
                        }
                        // User-defined function call
                        for arg in args {
                            self.emit_expr(arg);
                        }
                        if let Some(&func_idx) = self.emitter.func_map.get(name.as_str()) {
                            wasm!(self.func, { call(func_idx); });
                        } else {
                            wasm!(self.func, { unreachable; });
                        }
                    }
                }
            }

            CallTarget::Module { module, func } => {
                match (module.as_str(), func.as_str()) {
                    _ if module == "int" => {
                        if !self.emit_int_call(func, args) {
                            self.emit_stub_call(args);
                        }
                    }
                    _ if module == "float" => {
                        if !self.emit_float_call(func, args) {
                            self.emit_stub_call(args);
                        }
                    }
                    _ if module == "string" => {
                        if !self.emit_string_call(func, args) {
                            self.emit_stub_call(args);
                        }
                    }
                    _ if module == "option" => {
                        if !self.emit_option_call(func, args) {
                            self.emit_stub_call(args);
                        }
                    }
                    _ if module == "result" => {
                        if !self.emit_result_call(func, args) {
                            self.emit_stub_call(args);
                        }
                    }
                    _ if module == "list" => {
                        if !self.emit_list_call(func, args) {
                            self.emit_stub_call(args);
                        }
                    }
                    _ if module == "map" => {
                        if !self.emit_map_call(func, args) {
                            self.emit_stub_call(args);
                        }
                    }
                    _ if module == "math" => {
                        if !self.emit_math_call(func, args) {
                            self.emit_stub_call(args);
                        }
                    }
                    ("error", "message") => {
                        // error.message(r: Result[T, String]) → String
                        // tag==0(ok): empty string, tag==1(err): load string at offset 4
                        let s = self.scratch.alloc_i32();
                        let s1 = self.scratch.alloc_i32();
                        self.emit_expr(&args[0]);
                        wasm!(self.func, {
                            local_set(s);
                            local_get(s); i32_load(0); i32_eqz; // tag == 0?
                            if_i32;
                              // ok → empty string
                              i32_const(4); call(self.emitter.rt.alloc); local_set(s1);
                              local_get(s1); i32_const(0); i32_store(0);
                              local_get(s1);
                            else_;
                              local_get(s); i32_load(4); // err string ptr
                            end;
                        });
                        self.scratch.free_i32(s1);
                        self.scratch.free_i32(s);
                    }
                    ("error", "context") => {
                        // error.context(result, msg) → Result[T, String]
                        // If err: wrap error message with context. If ok: pass through.
                        let s = self.scratch.alloc_i32();
                        let s1 = self.scratch.alloc_i32();
                        let s2 = self.scratch.alloc_i32();
                        let s3 = self.scratch.alloc_i32();
                        self.emit_expr(&args[0]);
                        wasm!(self.func, {
                            local_set(s);
                            local_get(s); i32_load(0); i32_eqz; // tag == 0 (ok)?
                            if_i32;
                              local_get(s); // pass ok through
                            else_;
                              // Build new err with context: "msg: original_err"
                              local_get(s); i32_load(4); local_set(s1); // original err string
                        });
                        self.emit_expr(&args[1]); // context msg
                        wasm!(self.func, {
                              local_set(s2);
                              // Build ": " separator
                              i32_const(6); call(self.emitter.rt.alloc); local_set(s3);
                              local_get(s3); i32_const(2); i32_store(0);
                              local_get(s3); i32_const(58); i32_store8(4);
                              local_get(s3); i32_const(32); i32_store8(5);
                              // concat: msg + ": " + original
                              local_get(s2); local_get(s3); call(self.emitter.rt.concat_str);
                              local_get(s1); call(self.emitter.rt.concat_str);
                              local_set(s1);
                              // Build new err Result
                              i32_const(8); call(self.emitter.rt.alloc); local_set(s);
                              local_get(s); i32_const(1); i32_store(0);
                              local_get(s); local_get(s1); i32_store(4);
                              local_get(s);
                            end;
                        });
                        self.scratch.free_i32(s3);
                        self.scratch.free_i32(s2);
                        self.scratch.free_i32(s1);
                        self.scratch.free_i32(s);
                    }
                    ("error", "chain") => {
                        // error.chain(outer, cause) → "outer: cause"
                        self.emit_expr(&args[0]);
                        // concat outer + ": " + cause
                        // Build ": " string
                        let s = self.scratch.alloc_i32();
                        let s1 = self.scratch.alloc_i32();
                        wasm!(self.func, {
                            local_set(s);
                            i32_const(6); call(self.emitter.rt.alloc); local_set(s1);
                            local_get(s1); i32_const(2); i32_store(0);
                            local_get(s1); i32_const(58); i32_store8(4); // ':'
                            local_get(s1); i32_const(32); i32_store8(5); // ' '
                            local_get(s); local_get(s1); call(self.emitter.rt.concat_str);
                        });
                        self.emit_expr(&args[1]);
                        wasm!(self.func, { call(self.emitter.rt.concat_str); });
                        self.scratch.free_i32(s1);
                        self.scratch.free_i32(s);
                    }
                    _ if module == "set" => {
                        if !self.emit_set_call(func, args) {
                            self.emit_stub_call(args);
                        }
                    }
                    _ if module == "fan" => {
                        self.emit_fan_call(func, args, _ret_ty);
                    }
                    _ if module == "value" => {
                        self.emit_value_call(func, args);
                    }
                    _ if module == "json" => {
                        self.emit_json_call(func, args);
                    }
                    _ if module == "env" => {
                        self.emit_env_call(func, args);
                    }
                    _ if module == "random" => {
                        self.emit_random_call(func, args);
                    }
                    // datetime module will be added when implementation is ready
                    // _ if module == "datetime" => { self.emit_datetime_call(func, args); }
                    _ if module == "http" => {
                        self.emit_http_call(func, args);
                    }
                    _ => {
                        self.emit_stub_call(args);
                    }
                }
            }

            CallTarget::Method { object, method } => {
                // UFCS method calls: obj.method(args)
                match method.as_str() {
                    "to_string" | "int.to_string" if matches!(object.ty, Ty::Int) => {
                        self.emit_expr(object);
                        wasm!(self.func, { call(self.emitter.rt.int_to_string); });
                    }
                    "len" | "length" | "string.len" | "list.len" | "map.len" => {
                        // .len() for String, List, Map — all store length at offset 0
                        self.emit_expr(object);
                        wasm!(self.func, {
                            i32_load(0);
                            i64_extend_i32_u;
                        });
                    }
                    "to_string" | "float.to_string" if matches!(object.ty, Ty::Float) => {
                        self.emit_expr(object);
                        wasm!(self.func, {
                            call(self.emitter.rt.float_to_string);
                        });
                    }
                    "sort" | "list.sort" if matches!(&object.ty, Ty::Applied(_, _)) => {
                        let fake = [(**object).clone()];
                        let target = CallTarget::Module { module: "list".into(), func: "sort".into() };
                        self.emit_call(&target, &fake, _ret_ty);
                    }
                    "reverse" | "list.reverse" if matches!(&object.ty, Ty::Applied(_, _)) => {
                        let fake = [(**object).clone()];
                        let target = CallTarget::Module { module: "list".into(), func: "reverse".into() };
                        self.emit_call(&target, &fake, _ret_ty);
                    }
                    "filter" | "list.filter" if matches!(&object.ty, Ty::Applied(_, _)) => {
                        let fake = [(**object).clone(), args[0].clone()];
                        let target = CallTarget::Module { module: "list".into(), func: "filter".into() };
                        self.emit_call(&target, &fake, _ret_ty);
                    }
                    "fold" | "list.fold" if matches!(&object.ty, Ty::Applied(_, _)) => {
                        let fake = [(**object).clone(), args[0].clone(), args[1].clone()];
                        let target = CallTarget::Module { module: "list".into(), func: "fold".into() };
                        self.emit_call(&target, &fake, _ret_ty);
                    }
                    "map" | "list.map" if matches!(&object.ty, Ty::Applied(_, _)) => {
                        // .map(fn) → list.map(self, fn)
                        self.emit_list_map(object, &args[0], _ret_ty);
                    }
                    "trim" | "string.trim" if matches!(object.ty, Ty::String) => {
                        self.emit_expr(object);
                        wasm!(self.func, { call(self.emitter.rt.string.trim); });
                    }
                    "to_upper" | "string.to_upper" if matches!(object.ty, Ty::String) => {
                        self.emit_expr(object);
                        self.emit_str_case_convert(true);
                    }
                    "to_lower" | "string.to_lower" if matches!(object.ty, Ty::String) => {
                        self.emit_expr(object);
                        self.emit_str_case_convert(false);
                    }
                    "starts_with" | "string.starts_with" | "ends_with" | "string.ends_with" if matches!(object.ty, Ty::String) => {
                        // Delegate to Module call handler
                        self.emit_expr(object);
                        for arg in args { self.emit_expr(arg); }
                        wasm!(self.func, { unreachable; }); // TODO: wire up properly
                    }
                    "contains" | "string.contains" if matches!(object.ty, Ty::String) => {
                        self.emit_expr(object);
                        self.emit_expr(&args[0]);
                        wasm!(self.func, { call(self.emitter.rt.string.contains); });
                    }
                    _ if matches!(&object.ty, Ty::Applied(crate::types::constructor::TypeConstructorId::Option, _)) => {
                        let mut fake_args = vec![(**object).clone()];
                        fake_args.extend(args.iter().cloned());
                        let m = method.strip_prefix("option.").unwrap_or(method);
                        if !self.emit_option_call(m, &fake_args) {
                            self.emit_stub_call(args);
                        }
                    }
                    _ if matches!(&object.ty, Ty::Applied(crate::types::constructor::TypeConstructorId::Result, _)) => {
                        let mut fake_args = vec![(**object).clone()];
                        fake_args.extend(args.iter().cloned());
                        let m = method.strip_prefix("result.").unwrap_or(method);
                        if !self.emit_result_call(m, &fake_args) {
                            self.emit_ufcs_fallback(object, method, args);
                        }
                    }
                    _ if matches!(&object.ty, Ty::String) => {
                        let mut fake_args = vec![(**object).clone()];
                        fake_args.extend(args.iter().cloned());
                        let m = method.strip_prefix("string.").unwrap_or(method);
                        if !self.emit_string_call(m, &fake_args) {
                            self.emit_ufcs_fallback(object, method, args);
                        }
                    }
                    _ if matches!(&object.ty, Ty::Int) => {
                        let mut fake_args = vec![(**object).clone()];
                        fake_args.extend(args.iter().cloned());
                        let m = method.strip_prefix("int.").unwrap_or(method);
                        if !self.emit_int_call(m, &fake_args) {
                            self.emit_ufcs_fallback(object, method, args);
                        }
                    }
                    _ if matches!(&object.ty, Ty::Float) => {
                        let mut fake_args = vec![(**object).clone()];
                        fake_args.extend(args.iter().cloned());
                        let m = method.strip_prefix("float.").unwrap_or(method);
                        if !self.emit_float_call(m, &fake_args) {
                            self.emit_ufcs_fallback(object, method, args);
                        }
                    }
                    _ if matches!(&object.ty, Ty::Applied(crate::types::constructor::TypeConstructorId::List, _)) => {
                        let mut fake_args = vec![(**object).clone()];
                        fake_args.extend(args.iter().cloned());
                        let m = method.strip_prefix("list.").unwrap_or(method);
                        if !self.emit_list_call(m, &fake_args) {
                            self.emit_ufcs_fallback(object, method, args);
                        }
                    }
                    _ if matches!(&object.ty, Ty::Applied(crate::types::constructor::TypeConstructorId::Map, _)) => {
                        let mut fake_args = vec![(**object).clone()];
                        fake_args.extend(args.iter().cloned());
                        let m = method.strip_prefix("map.").unwrap_or(method);
                        if !self.emit_map_call(m, &fake_args) {
                            self.emit_ufcs_fallback(object, method, args);
                        }
                    }
                    _ => {
                        // Try to resolve as TypeName.method convention call
                        let type_name = match &object.ty {
                            Ty::Named(n, _) => Some(n.clone()),
                            Ty::Record { .. } => None,
                            _ => None,
                        };
                        let qualified = type_name.as_ref().map(|tn| format!("{}.{}", tn, method));
                        if let Some(ref qn) = qualified {
                            if let Some(&func_idx) = self.emitter.func_map.get(qn.as_str()) {
                                // Convention/protocol method: call TypeName.method(self, args...)
                                self.emit_expr(object);
                                for arg in args { self.emit_expr(arg); }
                                wasm!(self.func, { call(func_idx); });
                                return;
                            }
                        }
                        // Also try: method name itself might be fully qualified (e.g., "Pair.to_str")
                        if let Some(&func_idx) = self.emitter.func_map.get(method.as_str()) {
                            self.emit_expr(object);
                            for arg in args { self.emit_expr(arg); }
                            wasm!(self.func, { call(func_idx); });
                            return;
                        }
                        // Fallback: stub
                        self.emit_expr(object);
                        if values::ty_to_valtype(&object.ty).is_some() {
                            wasm!(self.func, { drop; });
                        }
                        self.emit_stub_call(args);
                    }
                }
            }

            CallTarget::Computed { callee } => {
                // Closure call: callee is a closure ptr [table_idx: i32][env_ptr: i32]
                let scratch = self.scratch.alloc_i32();

                // Evaluate callee → closure ptr
                self.emit_expr(callee);
                wasm!(self.func, { local_set(scratch); });

                // Push env_ptr (first hidden arg)
                wasm!(self.func, {
                    local_get(scratch);
                    i32_load(4);
                });

                // Push declared args (may contain nested closure calls)
                for arg in args {
                    self.emit_expr(arg);
                }

                // Push table_idx (on top of stack for call_indirect)
                wasm!(self.func, {
                    local_get(scratch);
                    i32_load(0);
                });

                // Closure calling convention type: (env: i32, params...) -> ret
                if let Ty::Fn { params, ret } = &callee.ty {
                    let mut closure_params = vec![ValType::I32]; // env_ptr
                    for p in params {
                        if let Some(vt) = values::ty_to_valtype(p) {
                            closure_params.push(vt);
                        }
                    }
                    let ret_types = values::ret_type(ret);
                    let type_idx = self.emitter.register_type(closure_params, ret_types);
                    wasm!(self.func, { call_indirect(type_idx, 0); });
                } else {
                    wasm!(self.func, { unreachable; });
                }
                self.scratch.free_i32(scratch);
            }
        }
    }

    /// Emit a stub for an unimplemented call: evaluate args (for side effects), drop values, unreachable.
    pub(super) fn emit_stub_call_logged(&mut self, args: &[IrExpr], context: &str) {
        eprintln!("[STUB] {}", context);
        self.emit_stub_call(args);
    }

    pub(super) fn emit_stub_call(&mut self, args: &[IrExpr]) {
        // Evaluate args for side effects, then return typed default instead of trapping.
        for arg in args {
            self.emit_expr(arg);
            if values::ty_to_valtype(&arg.ty).is_some() {
                wasm!(self.func, { drop; });
            }
        }
        // Return safe typed default based on return type context.
        let ret_ty = self.stub_ret_ty.clone();
        self.emit_typed_default(&ret_ty);
    }

    /// Emit a safe default value for a given type.
    /// String → empty string, List → empty list, Option → none, Bool → false, etc.
    pub(super) fn emit_typed_default(&mut self, ty: &Ty) {
        use crate::types::constructor::TypeConstructorId;
        match ty {
            Ty::Int => { wasm!(self.func, { i64_const(0); }); }
            Ty::Float => { wasm!(self.func, { f64_const(0.0); }); }
            Ty::Bool => { wasm!(self.func, { i32_const(0); }); }
            Ty::String => {
                // Empty string: alloc 4 bytes, len=0
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
                    i32_const(4); call(self.emitter.rt.alloc);
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

    /// Emit assert_eq(left, right): compare values, trap if not equal.
    /// UFCS fallback: try func_map lookup for user-defined functions before stubbing.
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
        // Stub
        self.emit_expr(object);
        if values::ty_to_valtype(&object.ty).is_some() {
            wasm!(self.func, { drop; });
        }
        self.emit_stub_call(args);
    }

    pub(super) fn emit_assert_eq(&mut self, left: &IrExpr, right: &IrExpr) {
        // Use the same equality logic as BinOp::Eq
        self.emit_eq(left, right, false);
        // If not equal (result == 0), trap
        wasm!(self.func, {
            i32_eqz;
            if_empty;
            unreachable;
            end;
        });
    }

    /// Fan module: concurrent execution fallback (sequential in WASM).
    /// fan.map(xs, f) → List[T]: apply f to each element, unwrap Results
    /// fan.race(fns) → T: run all, return first result (sequential: just run first)
    /// fan.any(fns) → Result[T, String]: first success
    /// fan.settle(fns) → List[Result[T, E]]: run all, collect results
    fn emit_fan_call(&mut self, func: &str, args: &[IrExpr], result_ty: &Ty) {
        match func {
            "map" => {
                // fan.map(xs, f) — apply effect fn f to each element, unwrap Results
                // f returns Result[T, E] (i32 ptr). We unwrap ok values into result list.
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                // Determine output element type from result_ty
                let out_elem_ty = if let Ty::Applied(_, a) = result_ty {
                    a.first().cloned().unwrap_or(Ty::Int)
                } else { Ty::Int };
                let out_es = values::byte_size(&out_elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let res = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    local_get(xs); i32_load(0); local_set(len);
                    i32_const(4); local_get(len); i32_const(out_es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(len); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      // Call closure(elem) — closure returns Result ptr (i32)
                      local_get(closure); i32_load(4); // env
                      local_get(xs); i32_const(4); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                      local_get(closure); i32_load(0); // table_idx
                });
                // call_indirect: (env, elem) → i32 (Result ptr)
                {
                    let mut ct = vec![ValType::I32]; // env
                    if let Some(vt) = values::ty_to_valtype(&elem_ty) { ct.push(vt); }
                    let ti = self.emitter.register_type(ct, vec![ValType::I32]);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                wasm!(self.func, {
                      local_set(res);
                      // Unwrap Result: if err, propagate
                      local_get(res); i32_load(0); i32_const(0); i32_ne;
                      if_empty; local_get(res); return_; end;
                      // Store unwrapped ok value into dst
                      local_get(dst); i32_const(4); i32_add;
                      local_get(i); i32_const(out_es); i32_mul; i32_add;
                      local_get(res);
                });
                self.emit_load_at(&out_elem_ty, 4);
                self.emit_store_at(&out_elem_ty, 0);
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i32(res);
                self.scratch.free_i32(i);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(len);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(xs);
            }
            "race" => {
                // fan.race(fns: List[() -> Result[T,E]]) → T
                // Sequential: call first fn, unwrap result
                // fns is a list of closures. Call fns[0]().
                let list_scratch = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(list_scratch);
                    // Get first closure: fns[0]
                    local_get(list_scratch); i32_const(4); i32_add; i32_load(0);
                });
                // Call the closure (0-arg + env)
                let res_scratch = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_set(res_scratch); // closure ptr
                    local_get(res_scratch); i32_load(4); // env
                    local_get(res_scratch); i32_load(0); // table_idx
                });
                {
                    let ti = self.emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                // Unwrap Result
                wasm!(self.func, {
                    local_set(res_scratch);
                    local_get(res_scratch); i32_load(0); i32_const(0); i32_ne;
                    if_empty; local_get(res_scratch); return_; end;
                    local_get(res_scratch);
                });
                self.emit_load_at(result_ty, 4);
                self.scratch.free_i32(res_scratch);
                self.scratch.free_i32(list_scratch);
            }
            "any" => {
                // fan.any(fns) → T: first success wins (unwrapped ok value)
                // Sequential: try each, return first ok value
                let list_scratch = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let res = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(list_scratch);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(list_scratch); i32_load(0); i32_ge_u; br_if(1);
                      local_get(list_scratch); i32_const(4); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add;
                      i32_load(0); local_set(closure);
                      local_get(closure); i32_load(4); // env
                      local_get(closure); i32_load(0); // table_idx
                });
                {
                    let ti = self.emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                wasm!(self.func, {
                      local_set(res);
                      // If ok (tag==0), unwrap and return value
                      local_get(res); i32_load(0); i32_eqz;
                      if_empty;
                        local_get(res);
                });
                self.emit_load_at(result_ty, 4);
                wasm!(self.func, {
                        return_;
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                });
                // All failed: trap (no success found)
                wasm!(self.func, { unreachable; });
                self.scratch.free_i32(closure);
                self.scratch.free_i32(res);
                self.scratch.free_i32(i);
                self.scratch.free_i32(list_scratch);
            }
            "settle" => {
                // fan.settle(fns) → List[Result[T, E]]: run all, collect results
                // Sequential: just map and collect
                let list_scratch = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(list_scratch);
                    // Alloc result list
                    i32_const(4); local_get(list_scratch); i32_load(0); i32_const(4); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); local_get(list_scratch); i32_load(0); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(list_scratch); i32_load(0); i32_ge_u; br_if(1);
                      local_get(list_scratch); i32_const(4); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add;
                      i32_load(0); local_set(closure);
                      local_get(closure); i32_load(4);
                      local_get(closure); i32_load(0);
                });
                {
                    let ti = self.emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                wasm!(self.func, {
                      // Store result[i] = closure result
                      local_get(result); i32_const(4); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add;
                });
                // swap: [result_ptr, result_i32_addr] → need [addr, value]
                // Actually: stack has [closure_result, result_addr]. swap needed.
                // Restructure: push addr first
                // Let me fix the order:
                wasm!(self.func, {
                      i32_store(0); // store closure result at result[i]
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result);
                });
                self.scratch.free_i32(closure);
                self.scratch.free_i32(i);
                self.scratch.free_i32(result);
                self.scratch.free_i32(list_scratch);
            }
            "timeout" => {
                // fan.timeout(ms, fn) → Result[T, E]: just call fn (no timeout in WASM)
                // args[0] = ms (Int), args[1] = closure () -> Result[T, E]
                let closure = self.scratch.alloc_i32();
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    local_get(closure); i32_load(4); // env
                    local_get(closure); i32_load(0); // table_idx
                });
                {
                    let ti = self.emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                self.scratch.free_i32(closure);
            }
            _ => {
                self.emit_stub_call(args);
            }
        }
    }

    /// Env module: environment access.
    fn emit_env_call(&mut self, func: &str, args: &[IrExpr]) {
        match func {
            "args" => {
                // env.args() → List[String]: return empty list (WASI args not implemented yet)
                let s = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc); local_set(s);
                    local_get(s); i32_const(0); i32_store(0);
                    local_get(s);
                });
                self.scratch.free_i32(s);
            }
            "unix_timestamp" => {
                wasm!(self.func, { i64_const(1711000000); }); // ~2024-03-21
            }
            "millis" => {
                wasm!(self.func, { i64_const(1711000000000); }); // ms since epoch
            }
            "os" => {
                let s = self.emitter.intern_string("wasi");
                wasm!(self.func, { i32_const(s as i32); });
            }
            "temp_dir" => {
                let s = self.emitter.intern_string("/tmp");
                wasm!(self.func, { i32_const(s as i32); });
            }
            _ => {
                self.emit_stub_call(args);
            }
        }
    }

    /// Random module: PRNG-based random number generation.
    /// Uses xorshift64 state stored at linear memory address 0 (8 bytes).
    pub(super) fn emit_random_call(&mut self, func: &str, args: &[IrExpr]) {
        match func {
            "int" => {
                // random.int(min, max) → Int in [min, max]
                let min = self.scratch.alloc_i64();
                let max = self.scratch.alloc_i64();
                let state = self.scratch.alloc_i64();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(min); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(max);
                    // Load PRNG state from mem[0..8]
                    i32_const(0); i64_load(0); local_set(state);
                    // If state == 0, initialize with seed
                    local_get(state); i64_eqz;
                    if_empty;
                      i64_const(0x2545F4914F6CDD1D); local_set(state);
                    end;
                    // xorshift64
                    local_get(state); local_get(state); i64_const(13); i64_shl; i64_xor; local_set(state);
                    local_get(state); local_get(state); i64_const(7); i64_shr_u; i64_xor; local_set(state);
                    local_get(state); local_get(state); i64_const(17); i64_shl; i64_xor; local_set(state);
                    // Store back
                    i32_const(0); local_get(state); i64_store(0);
                    // result = min + abs(state) % (max - min + 1)
                    local_get(min);
                    // abs(state)
                    local_get(state); i64_const(0); i64_lt_s;
                    if_i64; i64_const(0); local_get(state); i64_sub; else_; local_get(state); end;
                    local_get(max); local_get(min); i64_sub; i64_const(1); i64_add;
                    i64_rem_u;
                    i64_add;
                });
                self.scratch.free_i64(state);
                self.scratch.free_i64(max);
                self.scratch.free_i64(min);
            }
            "float" => {
                // random.float() → Float in [0.0, 1.0)
                let state = self.scratch.alloc_i64();
                wasm!(self.func, {
                    i32_const(0); i64_load(0); local_set(state);
                    local_get(state); i64_eqz;
                    if_empty; i64_const(0x2545F4914F6CDD1D); local_set(state); end;
                    local_get(state); local_get(state); i64_const(13); i64_shl; i64_xor; local_set(state);
                    local_get(state); local_get(state); i64_const(7); i64_shr_u; i64_xor; local_set(state);
                    local_get(state); local_get(state); i64_const(17); i64_shl; i64_xor; local_set(state);
                    i32_const(0); local_get(state); i64_store(0);
                });
                // Convert to float in [0, 1): abs(state) >>> 11 * (1/2^53)
                wasm!(self.func, {
                    local_get(state); i64_const(0); i64_lt_s;
                    if_i64; i64_const(0); local_get(state); i64_sub; else_; local_get(state); end;
                    i64_const(11); i64_shr_u;
                    f64_convert_i64_u;
                    f64_const(1.1102230246251565e-16); // 1.0 / 2^53
                    f64_mul;
                });
                self.scratch.free_i64(state);
            }
            "choice" => {
                // random.choice(xs: List[T]) → T: pick random element
                // Emit random.int(0, len-1) then list.get
                let xs = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                // Build random.int(0, len-1) args inline
                let state = self.scratch.alloc_i64();
                wasm!(self.func, {
                    i32_const(0); i64_load(0); local_set(state);
                    local_get(state); i64_eqz;
                    if_empty; i64_const(0x2545F4914F6CDD1D); local_set(state); end;
                    local_get(state); local_get(state); i64_const(13); i64_shl; i64_xor; local_set(state);
                    local_get(state); local_get(state); i64_const(7); i64_shr_u; i64_xor; local_set(state);
                    local_get(state); local_get(state); i64_const(17); i64_shl; i64_xor; local_set(state);
                    i32_const(0); local_get(state); i64_store(0);
                    // idx = abs(state) % len
                    local_get(state); i64_const(0); i64_lt_s;
                    if_i64; i64_const(0); local_get(state); i64_sub; else_; local_get(state); end;
                    local_get(xs); i32_load(0); i64_extend_i32_u;
                    i64_rem_u;
                    i32_wrap_i64;
                });
                // Load xs[idx]
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                wasm!(self.func, {
                    i32_const(es); i32_mul;
                    local_get(xs); i32_const(4); i32_add; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                self.scratch.free_i64(state);
                self.scratch.free_i32(xs);
            }
            "shuffle" => {
                // random.shuffle(xs) → List[T]: Fisher-Yates shuffle on a copy
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let src = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let j = self.scratch.alloc_i32();
                let state = self.scratch.alloc_i64();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(src);
                    local_get(src); i32_load(0); local_set(len);
                    // Alloc copy
                    i32_const(4); local_get(len); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(len); i32_store(0);
                    // Copy all elements
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(4); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                      local_get(src); i32_const(4); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    // Fisher-Yates shuffle (backwards)
                    local_get(len); i32_const(1); i32_sub; local_set(i);
                    i32_const(0); i64_load(0); local_set(state);
                    local_get(state); i64_eqz;
                    if_empty; i64_const(0x2545F4914F6CDD1D); local_set(state); end;
                    block_empty; loop_empty;
                      local_get(i); i32_const(0); i32_le_s; br_if(1);
                      // xorshift
                      local_get(state); local_get(state); i64_const(13); i64_shl; i64_xor; local_set(state);
                      local_get(state); local_get(state); i64_const(7); i64_shr_u; i64_xor; local_set(state);
                      local_get(state); local_get(state); i64_const(17); i64_shl; i64_xor; local_set(state);
                      // j = abs(state) % (i + 1)
                      local_get(state);
                      local_get(state); i64_const(0); i64_lt_s;
                      if_i64; i64_const(0); local_get(state); i64_sub; else_; local_get(state); end;
                      local_get(i); i32_const(1); i32_add; i64_extend_i32_u;
                      i64_rem_u; i32_wrap_i64; local_set(j);
                      // swap dst[i] and dst[j] using mem[0..es] as temp
                      // Copy dst[i] to temp
                      i32_const(0);
                      local_get(dst); i32_const(4); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                      i32_load(0); i32_store(0);
                      // Copy dst[j] to dst[i]
                      local_get(dst); i32_const(4); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                      local_get(dst); i32_const(4); i32_add;
                      local_get(j); i32_const(es); i32_mul; i32_add;
                      i32_load(0); i32_store(0);
                      // Copy temp to dst[j]
                      local_get(dst); i32_const(4); i32_add;
                      local_get(j); i32_const(es); i32_mul; i32_add;
                      i32_const(0); i32_load(0); i32_store(0);
                      local_get(i); i32_const(1); i32_sub; local_set(i);
                      br(0);
                    end; end;
                    i32_const(0); local_get(state); i64_store(0); // save state
                    local_get(dst);
                });
                self.scratch.free_i64(state);
                self.scratch.free_i32(j);
                self.scratch.free_i32(i);
                self.scratch.free_i32(len);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(src);
            }
            _ => {
                self.emit_stub_call(args);
            }
        }
    }

    /// HTTP module: response construction and header manipulation.
    /// Response layout: [status:i64][body:i32 str ptr][headers:i32 list ptr]
    /// Headers = List[(String, String)] — list of tuple pointers.
    fn emit_http_call(&mut self, func: &str, args: &[IrExpr]) {
        match func {
            "response" => {
                // http.response(status: Int, body: String) → Response
                let s = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(16); call(self.emitter.rt.alloc); local_set(s);
                    local_get(s);
                });
                self.emit_expr(&args[0]); // status: i64
                wasm!(self.func, { i64_store(0); local_get(s); });
                self.emit_expr(&args[1]); // body: i32 str
                wasm!(self.func, {
                    i32_store(8);
                    // Empty headers list
                    local_get(s);
                    i32_const(4); call(self.emitter.rt.alloc);
                });
                let empty_list = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_set(empty_list);
                    local_get(empty_list); i32_const(0); i32_store(0);
                    local_get(empty_list);
                    i32_store(12);
                    local_get(s);
                });
                self.scratch.free_i32(empty_list);
                self.scratch.free_i32(s);
            }
            "json" => {
                // http.json(status: Int, body: String) → Response (with Content-Type header)
                let s = self.scratch.alloc_i32();
                let hdr_list = self.scratch.alloc_i32();
                let tuple_ptr = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(16); call(self.emitter.rt.alloc); local_set(s);
                    local_get(s);
                });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i64_store(0); local_get(s); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { i32_store(8); });
                // Build headers list with Content-Type: application/json
                let ct_key = self.emitter.intern_string("Content-Type");
                let ct_val = self.emitter.intern_string("application/json");
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(tuple_ptr);
                    local_get(tuple_ptr); i32_const(ct_key as i32); i32_store(0);
                    local_get(tuple_ptr); i32_const(ct_val as i32); i32_store(4);
                    i32_const(8); call(self.emitter.rt.alloc); local_set(hdr_list);
                    local_get(hdr_list); i32_const(1); i32_store(0);
                    local_get(hdr_list); local_get(tuple_ptr); i32_store(4);
                    local_get(s); local_get(hdr_list); i32_store(12);
                    local_get(s);
                });
                self.scratch.free_i32(tuple_ptr);
                self.scratch.free_i32(hdr_list);
                self.scratch.free_i32(s);
            }
            "status" => {
                // http.status(resp) → Int
                self.emit_expr(&args[0]);
                wasm!(self.func, { i64_load(0); });
            }
            "body" => {
                // http.body(resp) → String
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_load(8); });
            }
            "get_header" => {
                // http.get_header(resp, key) → Option[String]
                let resp = self.scratch.alloc_i32();
                let key = self.scratch.alloc_i32();
                let hdrs = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let pair_ptr = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(resp); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(key);
                    local_get(resp); i32_load(12); local_set(hdrs);
                    local_get(hdrs); i32_load(0); local_set(len);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      local_get(hdrs); i32_const(4); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add;
                      i32_load(0); local_set(pair_ptr);
                      local_get(pair_ptr); i32_load(0);
                      local_get(key);
                      call(self.emitter.rt.string.eq);
                      if_empty;
                        local_get(pair_ptr); i32_load(4); return_;
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    i32_const(0); // none
                });
                self.scratch.free_i32(pair_ptr);
                self.scratch.free_i32(i);
                self.scratch.free_i32(len);
                self.scratch.free_i32(hdrs);
                self.scratch.free_i32(key);
                self.scratch.free_i32(resp);
            }
            "set_header" => {
                // http.set_header(resp, key, value) → Response (new response with header added/replaced)
                let resp = self.scratch.alloc_i32();
                let new_resp = self.scratch.alloc_i32();
                let old_hdrs = self.scratch.alloc_i32();
                let new_hdrs = self.scratch.alloc_i32();
                let tuple_ptr = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(resp); });
                // Build new tuple (key, value)
                let key_scratch = self.scratch.alloc_i32();
                let val_scratch = self.scratch.alloc_i32();
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(key_scratch); });
                self.emit_expr(&args[2]);
                wasm!(self.func, {
                    local_set(val_scratch);
                    i32_const(8); call(self.emitter.rt.alloc); local_set(tuple_ptr);
                    local_get(tuple_ptr); local_get(key_scratch); i32_store(0);
                    local_get(tuple_ptr); local_get(val_scratch); i32_store(4);
                    // Copy old headers + append new
                    local_get(resp); i32_load(12); local_set(old_hdrs);
                    local_get(old_hdrs); i32_load(0); i32_const(1); i32_add;
                    local_set(val_scratch); // reuse as new_len
                    i32_const(4); local_get(val_scratch); i32_const(4); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(new_hdrs);
                    local_get(new_hdrs); local_get(val_scratch); i32_store(0);
                    // Copy old header ptrs
                    i32_const(0); local_set(key_scratch); // reuse as i
                    block_empty; loop_empty;
                      local_get(key_scratch); local_get(old_hdrs); i32_load(0); i32_ge_u; br_if(1);
                      local_get(new_hdrs); i32_const(4); i32_add;
                      local_get(key_scratch); i32_const(4); i32_mul; i32_add;
                      local_get(old_hdrs); i32_const(4); i32_add;
                      local_get(key_scratch); i32_const(4); i32_mul; i32_add;
                      i32_load(0); i32_store(0);
                      local_get(key_scratch); i32_const(1); i32_add; local_set(key_scratch);
                      br(0);
                    end; end;
                    // Append new tuple at end
                    local_get(new_hdrs); i32_const(4); i32_add;
                    local_get(old_hdrs); i32_load(0); i32_const(4); i32_mul; i32_add;
                    local_get(tuple_ptr); i32_store(0);
                    // Build new response
                    i32_const(16); call(self.emitter.rt.alloc); local_set(new_resp);
                    local_get(new_resp); local_get(resp); i64_load(0); i64_store(0);
                    local_get(new_resp); local_get(resp); i32_load(8); i32_store(8);
                    local_get(new_resp); local_get(new_hdrs); i32_store(12);
                    local_get(new_resp);
                });
                self.scratch.free_i32(val_scratch);
                self.scratch.free_i32(key_scratch);
                self.scratch.free_i32(tuple_ptr);
                self.scratch.free_i32(new_hdrs);
                self.scratch.free_i32(old_hdrs);
                self.scratch.free_i32(new_resp);
                self.scratch.free_i32(resp);
            }
            _ => {
                self.emit_stub_call(args);
            }
        }
    }
}
