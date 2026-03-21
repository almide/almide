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
                    _ => {
                        // Check if this is a variant constructor
                        if let Some((tag, is_unit)) = self.find_variant_ctor_tag(name) {
                            if is_unit && args.is_empty() {
                                // Unit variant: allocate [tag:i32]
                                let scratch = self.match_i32_base + self.match_depth;
                                wasm!(self.func, {
                                    i32_const(4);
                                    call(self.emitter.rt.alloc);
                                    local_set(scratch);
                                    local_get(scratch);
                                    i32_const(tag as i32);
                                    i32_store(0);
                                    local_get(scratch);
                                });
                                return;
                            } else if !is_unit {
                                // Tuple payload variant: [tag:i32][arg0][arg1]...
                                let mut total_size = 4u32; // tag
                                for arg in args { total_size += values::byte_size(&arg.ty); }
                                let scratch = self.match_i32_base + self.match_depth;
                                self.match_depth += 1;
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
                                self.match_depth -= 1;
                                wasm!(self.func, { local_get(scratch); });
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
                        let s = self.match_i32_base + self.match_depth;
                        self.emit_expr(&args[0]);
                        wasm!(self.func, {
                            local_set(s);
                            local_get(s); i32_load(0); i32_eqz; // tag == 0?
                            if_i32;
                              // ok → empty string
                              i32_const(4); call(self.emitter.rt.alloc); local_set(s + 1);
                              local_get(s + 1); i32_const(0); i32_store(0);
                              local_get(s + 1);
                            else_;
                              local_get(s); i32_load(4); // err string ptr
                            end;
                        });
                    }
                    ("error", "context") => {
                        // error.context(result, msg) → Result[T, String]
                        // If err: wrap error message with context. If ok: pass through.
                        let s = self.match_i32_base + self.match_depth;
                        self.emit_expr(&args[0]);
                        wasm!(self.func, {
                            local_set(s);
                            local_get(s); i32_load(0); i32_eqz; // tag == 0 (ok)?
                            if_i32;
                              local_get(s); // pass ok through
                            else_;
                              // Build new err with context: "msg: original_err"
                              local_get(s); i32_load(4); local_set(s + 1); // original err string
                        });
                        self.emit_expr(&args[1]); // context msg
                        wasm!(self.func, {
                              local_set(s + 2);
                              // Build ": " separator
                              i32_const(6); call(self.emitter.rt.alloc); local_set(s + 3);
                              local_get(s + 3); i32_const(2); i32_store(0);
                              local_get(s + 3); i32_const(58); i32_store8(4);
                              local_get(s + 3); i32_const(32); i32_store8(5);
                              // concat: msg + ": " + original
                              local_get(s + 2); local_get(s + 3); call(self.emitter.rt.concat_str);
                              local_get(s + 1); call(self.emitter.rt.concat_str);
                              local_set(s + 1);
                              // Build new err Result
                              i32_const(8); call(self.emitter.rt.alloc); local_set(s);
                              local_get(s); i32_const(1); i32_store(0);
                              local_get(s); local_get(s + 1); i32_store(4);
                              local_get(s);
                            end;
                        });
                    }
                    ("error", "chain") => {
                        // error.chain(outer, cause) → "outer: cause"
                        self.emit_expr(&args[0]);
                        // concat outer + ": " + cause
                        // Build ": " string
                        let s = self.match_i32_base + self.match_depth;
                        wasm!(self.func, {
                            local_set(s);
                            i32_const(6); call(self.emitter.rt.alloc); local_set(s + 1);
                            local_get(s + 1); i32_const(2); i32_store(0);
                            local_get(s + 1); i32_const(58); i32_store8(4); // ':'
                            local_get(s + 1); i32_const(32); i32_store8(5); // ' '
                            local_get(s); local_get(s + 1); call(self.emitter.rt.concat_str);
                        });
                        self.emit_expr(&args[1]);
                        wasm!(self.func, { call(self.emitter.rt.concat_str); });
                    }
                    _ if module == "set" => {
                        if !self.emit_set_call(func, args) {
                            self.emit_stub_call(args);
                        }
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
                let scratch = self.match_i32_base + self.match_depth;

                // Evaluate callee → closure ptr
                self.emit_expr(callee);
                wasm!(self.func, { local_set(scratch); });

                // Reserve this scratch so nested calls use different locals
                self.match_depth += 1;

                // Push env_ptr (first hidden arg)
                wasm!(self.func, {
                    local_get(scratch);
                    i32_load(4);
                });

                // Push declared args (may contain nested closure calls)
                for arg in args {
                    self.emit_expr(arg);
                }

                // Restore depth
                self.match_depth -= 1;

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
                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc);
                    local_set(self.match_i32_base + self.match_depth);
                    local_get(self.match_i32_base + self.match_depth);
                    i32_const(0); i32_store(0);
                    local_get(self.match_i32_base + self.match_depth);
                });
            }
            Ty::Applied(TypeConstructorId::List, _) => {
                // Empty list: alloc 4 bytes, len=0
                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc);
                    local_set(self.match_i32_base + self.match_depth);
                    local_get(self.match_i32_base + self.match_depth);
                    i32_const(0); i32_store(0);
                    local_get(self.match_i32_base + self.match_depth);
                });
            }
            Ty::Applied(TypeConstructorId::Option, _) => {
                // none
                wasm!(self.func, { i32_const(0); });
            }
            Ty::Applied(TypeConstructorId::Result, _) => {
                // err("stub") — tag=1, value=empty string
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc);
                    local_set(self.match_i32_base + self.match_depth);
                    local_get(self.match_i32_base + self.match_depth);
                    i32_const(1); i32_store(0); // tag=err
                    local_get(self.match_i32_base + self.match_depth);
                    i32_const(4); call(self.emitter.rt.alloc);
                    i32_store(4); // empty string at offset 4
                    local_get(self.match_i32_base + self.match_depth);
                });
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
}
