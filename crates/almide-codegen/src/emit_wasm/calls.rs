//! Function call emission — emit_call and related helpers.

use almide_ir::{CallTarget, IrExpr};
use almide_lang::types::Ty;
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
                    "panic" => {
                        // panic(msg) — print "PANIC: " + msg to stderr, then trap
                        let prefix = self.emitter.intern_string("PANIC: ");
                        wasm!(self.func, { i32_const(prefix as i32); });
                        self.emit_expr(&args[0]);
                        wasm!(self.func, {
                            call(self.emitter.rt.concat_str);
                            call(self.emitter.rt.println_str);
                            unreachable;
                        });
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
                    "assert_throws" => {
                        // assert_throws(f, expected_msg) — call f(), expect err containing msg
                        // f is a closure returning Result (i32 ptr). tag==0 ok → trap (expected throw).
                        // tag!=0 err → check err string contains expected msg, trap if not.
                        let closure = self.scratch.alloc_i32();
                        let res = self.scratch.alloc_i32();
                        self.emit_expr(&args[0]);
                        wasm!(self.func, { local_set(closure); });
                        // Call closure: (env) → i32 (Result ptr)
                        wasm!(self.func, {
                            local_get(closure); i32_load(4); // env
                            local_get(closure); i32_load(0); // table_idx
                        });
                        {
                            let ti = self.emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
                            wasm!(self.func, { call_indirect(ti, 0); });
                        }
                        wasm!(self.func, { local_set(res); });
                        // tag==0 (ok) means no throw → trap
                        wasm!(self.func, {
                            local_get(res); i32_load(0); i32_eqz;
                            if_empty;
                            unreachable;
                            end;
                        });
                        // tag!=0 (err): check err string contains expected msg
                        wasm!(self.func, { local_get(res); i32_load(4); }); // err string ptr
                        self.emit_expr(&args[1]); // expected msg
                        wasm!(self.func, {
                            call(self.emitter.rt.string.contains);
                            i32_eqz;
                            if_empty;
                            unreachable;
                            end;
                        });
                        self.scratch.free_i32(res);
                        self.scratch.free_i32(closure);
                    }
                    "assert_contains" => {
                        // assert_contains(haystack, needle) — trap if haystack does not contain needle
                        self.emit_expr(&args[0]);
                        self.emit_expr(&args[1]);
                        wasm!(self.func, {
                            call(self.emitter.rt.string.contains);
                            i32_eqz;
                            if_empty;
                            unreachable;
                            end;
                        });
                    }
                    "assert_approx" => {
                        // assert_approx(a, b, tolerance) — trap if |a - b| >= tolerance
                        self.emit_expr(&args[0]);
                        self.emit_expr(&args[1]);
                        wasm!(self.func, { f64_sub; f64_abs; });
                        self.emit_expr(&args[2]);
                        wasm!(self.func, {
                            f64_ge;
                            if_empty;
                            unreachable;
                            end;
                        });
                    }
                    "assert_gt" => {
                        // assert_gt(a, b) — trap if a <= b
                        self.emit_expr(&args[0]);
                        self.emit_expr(&args[1]);
                        wasm!(self.func, {
                            i64_gt_s;
                            i32_eqz;
                            if_empty;
                            unreachable;
                            end;
                        });
                    }
                    "assert_lt" => {
                        // assert_lt(a, b) — trap if a >= b
                        self.emit_expr(&args[0]);
                        self.emit_expr(&args[1]);
                        wasm!(self.func, {
                            i64_lt_s;
                            i32_eqz;
                            if_empty;
                            unreachable;
                            end;
                        });
                    }
                    "assert_some" => {
                        // assert_some(opt) — trap if Option is none (ptr == 0)
                        self.emit_expr(&args[0]);
                        wasm!(self.func, {
                            i32_eqz;
                            if_empty;
                            unreachable;
                            end;
                        });
                    }
                    "assert_ok" => {
                        // assert_ok(result) — trap if Result is err (tag != 0)
                        self.emit_expr(&args[0]);
                        wasm!(self.func, {
                            i32_load(0);
                            i32_const(0);
                            i32_ne;
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
                        // Module-qualified call: list.fold, map.set, etc.
                        if let Some(dot) = name.find('.') {
                            let module = &name[..dot];
                            let func = &name[dot+1..];
                            let target = CallTarget::Module { module: almide_base::intern::sym(module), func: almide_base::intern::sym(func) };
                            self.emit_call(&target, args, _ret_ty);
                            return;
                        }
                        // Check if this is a variant constructor
                        if let Some((tag, is_unit)) = self.find_variant_ctor_tag(name) {
                            if is_unit && args.is_empty() {
                                // Unit variant: allocate with full variant size so
                                // mem_eq (which compares tag + max_payload bytes)
                                // doesn't read past the allocation.
                                let variant_size = self.variant_alloc_size(name);
                                let scratch = self.scratch.alloc_i32();
                                wasm!(self.func, {
                                    i32_const(variant_size as i32);
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
                                // Tuple/record payload variant: [tag:i32][arg0][arg1]...
                                // Allocate the FULL variant size (padded to max across
                                // all constructors) so mem_eq can safely compare any
                                // two values of the same variant type.
                                let mut payload_size = 0u32;
                                for arg in args.iter() { payload_size += values::byte_size(&arg.ty); }
                                let total_size = self.variant_alloc_size(name).max(4 + payload_size);
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
                        // Codec helper functions
                        if name.starts_with("__encode_option_") || name.starts_with("__decode_option_") || name.starts_with("__decode_default_") || name.starts_with("__encode_list_") || name.starts_with("__decode_list_") {
                            self.emit_codec_helper(name, args);
                            return;
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
                    _ if module == "bytes" => {
                        if !self.emit_bytes_call(func, args) {
                            self.emit_stub_call(args);
                        }
                    }
                    _ if module == "matrix" => {
                        if !self.emit_matrix_call(func, args) {
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
                    _ if module == "datetime" => {
                        self.emit_datetime_call(func, args);
                    }
                    _ if module == "http" => {
                        self.emit_http_call(func, args);
                    }
                    _ if module == "regex" => {
                        self.emit_regex_call(func, args);
                    }
                    _ if module == "fs" => {
                        self.emit_fs_call(func, args);
                    }
                    _ if module == "io" => {
                        self.emit_io_call(func, args);
                    }
                    _ if module == "process" => {
                        self.emit_process_call(func, args);
                    }
                    _ if module == "testing" => {
                        // Delegate to Named handler: testing.assert_gt → "assert_gt"
                        let target = CallTarget::Named { name: (*func).into() };
                        self.emit_call(&target, args, _ret_ty);
                    }
                    _ => {
                        // Try user module function: almide_rt_{module}_{func}
                        let mod_ident = module.as_str().replace('.', "_");
                        let func_ident = func.as_str().replace('.', "_");
                        let prefixed = format!("almide_rt_{}_{}", mod_ident, func_ident);
                        if let Some(&func_idx) = self.emitter.func_map.get(&prefixed) {
                            for arg in args { self.emit_expr(arg); }
                            wasm!(self.func, { call(func_idx); });
                        } else {
                            // Try Type.method dispatch (protocol implementations)
                            let qualified = format!("{}.{}", module, func);
                            if let Some(&func_idx) = self.emitter.func_map.get(qualified.as_str()) {
                                for arg in args { self.emit_expr(arg); }
                                wasm!(self.func, { call(func_idx); });
                            } else {
                                self.emit_stub_call(args);
                            }
                        }
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
                        // For String: char count (UTF-8 code points), matching Rust runtime.
                        // For List/Map: the length header at offset 0.
                        self.emit_expr(object);
                        if matches!(object.ty, Ty::String) {
                            wasm!(self.func, { call(self.emitter.rt.string.char_count); });
                        } else {
                            wasm!(self.func, {
                                i32_load(0);
                                i64_extend_i32_u;
                            });
                        }
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
                        let m = method.strip_prefix("string.").unwrap_or(method);
                        let mut full_args = vec![(**object).clone()];
                        full_args.extend(args.iter().cloned());
                        self.emit_string_call(m, &full_args);
                    }
                    "contains" | "string.contains" if matches!(object.ty, Ty::String) => {
                        self.emit_expr(object);
                        self.emit_expr(&args[0]);
                        wasm!(self.func, { call(self.emitter.rt.string.contains); });
                    }
                    _ if matches!(&object.ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, _)) => {
                        let mut fake_args = vec![(**object).clone()];
                        fake_args.extend(args.iter().cloned());
                        let m = method.strip_prefix("option.").unwrap_or(method);
                        if !self.emit_option_call(m, &fake_args) {
                            self.emit_stub_call(args);
                        }
                    }
                    _ if matches!(&object.ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, _)) => {
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
                    _ if matches!(&object.ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, _)) => {
                        let mut fake_args = vec![(**object).clone()];
                        fake_args.extend(args.iter().cloned());
                        let m = method.strip_prefix("list.").unwrap_or(method);
                        if !self.emit_list_call(m, &fake_args) {
                            self.emit_ufcs_fallback(object, method, args);
                        }
                    }
                    _ if matches!(&object.ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Map, _)) => {
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
                // Resolve callee type from multiple sources (callee.ty, VarTable)
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

    pub(super) fn emit_stub_call(&mut self, args: &[IrExpr]) {
        // Unimplemented function: trap immediately rather than returning a default value.
        // Returning silent defaults (0, empty string, etc.) is dangerous in medical contexts
        // where incorrect results could go unnoticed.
        if std::env::var("ALMIDE_WASM_STUB_WARN").is_ok() {
            eprintln!("[WASM] stub call reached — will trap at runtime");
        }
        for arg in args {
            self.emit_expr(arg);
            if values::ty_to_valtype(&arg.ty).is_some() {
                wasm!(self.func, { drop; });
            }
        }
        wasm!(self.func, { unreachable; });
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
                // WASI clock_time_get(id=0 realtime, precision=0, time_ptr)
                // Returns nanoseconds as i64, convert to seconds
                let time_ptr = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(time_ptr);
                    i32_const(0); // clock_id: realtime
                    i64_const(0); // precision
                    local_get(time_ptr);
                    call(self.emitter.rt.clock_time_get);
                    drop; // discard error code
                    local_get(time_ptr); i64_load(0);
                    i64_const(1000000000); i64_div_u;
                });
                self.scratch.free_i32(time_ptr);
            }
            "millis" => {
                // WASI clock_time_get(id=0 realtime, precision=0, time_ptr)
                // Returns nanoseconds as i64, convert to milliseconds
                let time_ptr = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(time_ptr);
                    i32_const(0); // clock_id: realtime
                    i64_const(0); // precision
                    local_get(time_ptr);
                    call(self.emitter.rt.clock_time_get);
                    drop; // discard error code
                    local_get(time_ptr); i64_load(0);
                    i64_const(1000000); i64_div_u;
                });
                self.scratch.free_i32(time_ptr);
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

    /// Log module: structured logging to stderr via WASI fd_write(2, ...).
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
                      i32_const(0); i32_const(8); call(self.emitter.rt.random_get); drop;
                      i32_const(0); i64_load(0); local_set(state);
                      local_get(state); i64_eqz;
                      if_empty; i64_const(1); local_set(state); end;
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
                    if_empty;
                      i32_const(0); i32_const(8); call(self.emitter.rt.random_get); drop;
                      i32_const(0); i64_load(0); local_set(state);
                      local_get(state); i64_eqz;
                      if_empty; i64_const(1); local_set(state); end;
                    end;
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
                // random.choice(xs: List[T]) → Option[T]: none if empty, some(random elem) if non-empty
                let xs = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let option_box = self.scratch.alloc_i32();
                let state = self.scratch.alloc_i64();
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;

                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(xs);
                    local_get(xs); i32_load(0); i32_eqz;
                    if_i32;
                      i32_const(0); // none (empty list)
                    else_;
                      // PRNG to get random index
                      i32_const(0); i64_load(0); local_set(state);
                      local_get(state); i64_eqz;
                      if_empty;
                      i32_const(0); i32_const(8); call(self.emitter.rt.random_get); drop;
                      i32_const(0); i64_load(0); local_set(state);
                      local_get(state); i64_eqz;
                      if_empty; i64_const(1); local_set(state); end;
                    end;
                      local_get(state); local_get(state); i64_const(13); i64_shl; i64_xor; local_set(state);
                      local_get(state); local_get(state); i64_const(7); i64_shr_u; i64_xor; local_set(state);
                      local_get(state); local_get(state); i64_const(17); i64_shl; i64_xor; local_set(state);
                      i32_const(0); local_get(state); i64_store(0);
                      // idx = abs(state) % len
                      local_get(state); i64_const(0); i64_lt_s;
                      if_i64; i64_const(0); local_get(state); i64_sub; else_; local_get(state); end;
                      local_get(xs); i32_load(0); i64_extend_i32_u;
                      i64_rem_u; i32_wrap_i64; local_set(idx);
                      // Alloc option box and load elem into it
                      i32_const(es); call(self.emitter.rt.alloc); local_set(option_box);
                      local_get(option_box);
                      local_get(xs); i32_const(4); i32_add;
                      local_get(idx); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                self.emit_store_at(&elem_ty, 0);
                wasm!(self.func, {
                      local_get(option_box);
                    end;
                });
                self.scratch.free_i64(state);
                self.scratch.free_i32(option_box);
                self.scratch.free_i32(idx);
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
                    if_empty;
                      i32_const(0); i32_const(8); call(self.emitter.rt.random_get); drop;
                      i32_const(0); i64_load(0); local_set(state);
                      local_get(state); i64_eqz;
                      if_empty; i64_const(1); local_set(state); end;
                    end;
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

    /// datetime module: all functions operate on i64 Unix timestamps (seconds since 1970-01-01 UTC).
    /// Uses the proleptic Gregorian calendar via Julian Day Number conversions.
    pub(super) fn emit_datetime_call(&mut self, func: &str, args: &[IrExpr]) {
        match func {
            "from_parts" => {
                // datetime.from_parts(year, month, day, hour, minute, second) → Int
                // JDN algorithm: a=(14-month)/12, y=year+4800-a, m=month+12*a-3
                // jdn = day + (153*m+2)/5 + 365*y + y/4 - y/100 + y/400 - 32045
                // timestamp = (jdn - 2440588) * 86400 + h*3600 + min*60 + sec
                let year = self.scratch.alloc_i64();
                let month = self.scratch.alloc_i64();
                let day = self.scratch.alloc_i64();
                let hour = self.scratch.alloc_i64();
                let minute = self.scratch.alloc_i64();
                let second = self.scratch.alloc_i64();
                let a = self.scratch.alloc_i64();
                let y = self.scratch.alloc_i64();
                let m = self.scratch.alloc_i64();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(year); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(month); });
                self.emit_expr(&args[2]);
                wasm!(self.func, { local_set(day); });
                self.emit_expr(&args[3]);
                wasm!(self.func, { local_set(hour); });
                self.emit_expr(&args[4]);
                wasm!(self.func, { local_set(minute); });
                self.emit_expr(&args[5]);
                wasm!(self.func, { local_set(second); });

                wasm!(self.func, {
                    i64_const(14); local_get(month); i64_sub; i64_const(12); i64_div_s; local_set(a);
                    local_get(year); i64_const(4800); i64_add; local_get(a); i64_sub; local_set(y);
                    local_get(month); i64_const(12); local_get(a); i64_mul; i64_add; i64_const(3); i64_sub; local_set(m);
                    local_get(day);
                    i64_const(153); local_get(m); i64_mul; i64_const(2); i64_add; i64_const(5); i64_div_s;
                    i64_add;
                    i64_const(365); local_get(y); i64_mul;
                    i64_add;
                    local_get(y); i64_const(4); i64_div_s;
                    i64_add;
                    local_get(y); i64_const(100); i64_div_s;
                    i64_sub;
                    local_get(y); i64_const(400); i64_div_s;
                    i64_add;
                    i64_const(32045); i64_sub;
                    i64_const(2440588); i64_sub;
                    i64_const(86400); i64_mul;
                    local_get(hour); i64_const(3600); i64_mul; i64_add;
                    local_get(minute); i64_const(60); i64_mul; i64_add;
                    local_get(second); i64_add;
                });

                self.scratch.free_i64(m);
                self.scratch.free_i64(y);
                self.scratch.free_i64(a);
                self.scratch.free_i64(second);
                self.scratch.free_i64(minute);
                self.scratch.free_i64(hour);
                self.scratch.free_i64(day);
                self.scratch.free_i64(month);
                self.scratch.free_i64(year);
            }
            "year" | "month" | "day" => {
                // Inverse JDN algorithm to extract date component from timestamp.
                let ts = self.scratch.alloc_i64();
                let d = self.scratch.alloc_i64();
                let f = self.scratch.alloc_i64();
                let e = self.scratch.alloc_i64();
                let g = self.scratch.alloc_i64();
                let h = self.scratch.alloc_i64();

                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(ts);
                    // floor(ts / 86400)
                    local_get(ts); i64_const(0); i64_ge_s;
                    if_i64;
                      local_get(ts); i64_const(86400); i64_div_s;
                    else_;
                      local_get(ts); i64_const(86399); i64_sub; i64_const(86400); i64_div_s;
                    end;
                    local_set(d);
                    local_get(d); i64_const(2440588); i64_add; local_set(d);
                    local_get(d); i64_const(1401); i64_add;
                    i64_const(4); local_get(d); i64_mul; i64_const(274277); i64_add;
                    i64_const(146097); i64_div_s; i64_const(3); i64_mul; i64_const(4); i64_div_s;
                    i64_add; i64_const(38); i64_sub;
                    local_set(f);
                    i64_const(4); local_get(f); i64_mul; i64_const(3); i64_add; local_set(e);
                    local_get(e); i64_const(1461); i64_rem_s; i64_const(4); i64_div_s; local_set(g);
                    i64_const(5); local_get(g); i64_mul; i64_const(2); i64_add; local_set(h);
                });

                match func {
                    "day" => {
                        wasm!(self.func, {
                            local_get(h); i64_const(153); i64_rem_s; i64_const(5); i64_div_s; i64_const(1); i64_add;
                        });
                    }
                    "month" => {
                        wasm!(self.func, {
                            local_get(h); i64_const(153); i64_div_s; i64_const(2); i64_add;
                            i64_const(12); i64_rem_s; i64_const(1); i64_add;
                        });
                    }
                    "year" => {
                        let mm = self.scratch.alloc_i64();
                        wasm!(self.func, {
                            local_get(h); i64_const(153); i64_div_s; i64_const(2); i64_add;
                            i64_const(12); i64_rem_s; i64_const(1); i64_add;
                            local_set(mm);
                            local_get(e); i64_const(1461); i64_div_s; i64_const(4716); i64_sub;
                            i64_const(14); local_get(mm); i64_sub; i64_const(12); i64_div_s;
                            i64_add;
                        });
                        self.scratch.free_i64(mm);
                    }
                    _ => unreachable!(),
                }

                self.scratch.free_i64(h);
                self.scratch.free_i64(g);
                self.scratch.free_i64(e);
                self.scratch.free_i64(f);
                self.scratch.free_i64(d);
                self.scratch.free_i64(ts);
            }
            "hour" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    i64_const(86400); i64_rem_s;
                    i64_const(86400); i64_add; i64_const(86400); i64_rem_s;
                    i64_const(3600); i64_div_s;
                });
            }
            "minute" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    i64_const(3600); i64_rem_s;
                    i64_const(3600); i64_add; i64_const(3600); i64_rem_s;
                    i64_const(60); i64_div_s;
                });
            }
            "second" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    i64_const(60); i64_rem_s;
                    i64_const(60); i64_add; i64_const(60); i64_rem_s;
                });
            }
            "now" => {
                // Call WASI clock_time_get(id=0 realtime, precision=0, time_ptr)
                // Returns nanoseconds as i64 at time_ptr, convert to seconds
                let time_ptr = self.scratch.alloc_i32();
                wasm!(self.func, {
                    // Allocate 8 bytes for i64 result (allocator guarantees 8-byte alignment)
                    i32_const(8); call(self.emitter.rt.alloc); local_set(time_ptr);
                    // clock_time_get(id=0, precision=0, time_ptr)
                    i32_const(0); // clock_id: realtime
                    i64_const(0); // precision
                    local_get(time_ptr); // output pointer (8-byte aligned)
                    call(self.emitter.rt.clock_time_get);
                    drop; // discard error code
                    // Load i64 nanoseconds, convert to seconds
                    local_get(time_ptr); i64_load(0);
                    i64_const(1000000000); i64_div_u;
                });
                self.scratch.free_i32(time_ptr);
            }
            "add_days" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { i64_const(86400); i64_mul; i64_add; });
            }
            "add_hours" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { i64_const(3600); i64_mul; i64_add; });
            }
            "add_minutes" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { i64_const(60); i64_mul; i64_add; });
            }
            "add_seconds" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { i64_add; });
            }
            "from_unix" | "to_unix" => {
                self.emit_expr(&args[0]);
            }
            "diff_seconds" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { i64_sub; });
            }
            "is_before" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { i64_lt_s; });
            }
            "is_after" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { i64_gt_s; });
            }
            "diff_days" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { i64_sub; i64_const(86400); i64_div_s; });
            }
            "format" => {
                // Stub: return int.to_string(ts), ignore fmt
                self.emit_expr(&args[0]);
                wasm!(self.func, { call(self.emitter.rt.int_to_string); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { drop; });
            }
            "to_iso" => {
                // datetime.to_iso(ts) → String "YYYY-MM-DDTHH:MM:SSZ"
                let ts = self.scratch.alloc_i64();
                let ptr = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(ts); });

                wasm!(self.func, {
                    i32_const(24); call(self.emitter.rt.alloc); local_set(ptr);
                    local_get(ptr); i32_const(20); i32_store(0);
                });

                let d = self.scratch.alloc_i64();
                let f = self.scratch.alloc_i64();
                let e = self.scratch.alloc_i64();
                let g = self.scratch.alloc_i64();
                let h = self.scratch.alloc_i64();
                let yr = self.scratch.alloc_i64();
                let mo = self.scratch.alloc_i64();
                let dy = self.scratch.alloc_i64();
                let hr = self.scratch.alloc_i64();
                let mi = self.scratch.alloc_i64();
                let se = self.scratch.alloc_i64();

                wasm!(self.func, {
                    local_get(ts); i64_const(0); i64_ge_s;
                    if_i64;
                      local_get(ts); i64_const(86400); i64_div_s;
                    else_;
                      local_get(ts); i64_const(86399); i64_sub; i64_const(86400); i64_div_s;
                    end;
                    local_set(d);
                    local_get(d); i64_const(2440588); i64_add; local_set(d);
                    local_get(d); i64_const(1401); i64_add;
                    i64_const(4); local_get(d); i64_mul; i64_const(274277); i64_add;
                    i64_const(146097); i64_div_s; i64_const(3); i64_mul; i64_const(4); i64_div_s;
                    i64_add; i64_const(38); i64_sub; local_set(f);
                    i64_const(4); local_get(f); i64_mul; i64_const(3); i64_add; local_set(e);
                    local_get(e); i64_const(1461); i64_rem_s; i64_const(4); i64_div_s; local_set(g);
                    i64_const(5); local_get(g); i64_mul; i64_const(2); i64_add; local_set(h);
                    local_get(h); i64_const(153); i64_rem_s; i64_const(5); i64_div_s; i64_const(1); i64_add; local_set(dy);
                    local_get(h); i64_const(153); i64_div_s; i64_const(2); i64_add;
                    i64_const(12); i64_rem_s; i64_const(1); i64_add; local_set(mo);
                    local_get(e); i64_const(1461); i64_div_s; i64_const(4716); i64_sub;
                    i64_const(14); local_get(mo); i64_sub; i64_const(12); i64_div_s;
                    i64_add; local_set(yr);
                    local_get(ts); i64_const(86400); i64_rem_s; i64_const(86400); i64_add; i64_const(86400); i64_rem_s;
                    local_set(d);
                    local_get(d); i64_const(3600); i64_div_s; local_set(hr);
                    local_get(d); i64_const(3600); i64_rem_s; i64_const(60); i64_div_s; local_set(mi);
                    local_get(d); i64_const(60); i64_rem_s; local_set(se);
                });

                self.emit_write_decimal_digits(ptr, 4, yr, 4);
                wasm!(self.func, { local_get(ptr); i32_const(45); i32_store8(8); });
                self.emit_write_decimal_digits(ptr, 9, mo, 2);
                wasm!(self.func, { local_get(ptr); i32_const(45); i32_store8(11); });
                self.emit_write_decimal_digits(ptr, 12, dy, 2);
                wasm!(self.func, { local_get(ptr); i32_const(84); i32_store8(14); });
                self.emit_write_decimal_digits(ptr, 15, hr, 2);
                wasm!(self.func, { local_get(ptr); i32_const(58); i32_store8(17); });
                self.emit_write_decimal_digits(ptr, 18, mi, 2);
                wasm!(self.func, { local_get(ptr); i32_const(58); i32_store8(20); });
                self.emit_write_decimal_digits(ptr, 21, se, 2);
                wasm!(self.func, { local_get(ptr); i32_const(90); i32_store8(23); });

                wasm!(self.func, { local_get(ptr); });

                self.scratch.free_i64(se);
                self.scratch.free_i64(mi);
                self.scratch.free_i64(hr);
                self.scratch.free_i64(dy);
                self.scratch.free_i64(mo);
                self.scratch.free_i64(yr);
                self.scratch.free_i64(h);
                self.scratch.free_i64(g);
                self.scratch.free_i64(e);
                self.scratch.free_i64(f);
                self.scratch.free_i64(d);
                self.scratch.free_i32(ptr);
                self.scratch.free_i64(ts);
            }
            "weekday" => {
                // (floor(ts/86400) + 4) % 7: 0=Sun..6=Sat
                let ts = self.scratch.alloc_i64();
                let wd = self.scratch.alloc_i64();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(ts);
                    local_get(ts); i64_const(0); i64_ge_s;
                    if_i64;
                      local_get(ts); i64_const(86400); i64_div_s;
                    else_;
                      local_get(ts); i64_const(86399); i64_sub; i64_const(86400); i64_div_s;
                    end;
                    i64_const(4); i64_add;
                    i64_const(7); i64_rem_s;
                    i64_const(7); i64_add; i64_const(7); i64_rem_s;
                    local_set(wd);
                });

                let sun = self.emitter.intern_string("Sunday");
                let mon = self.emitter.intern_string("Monday");
                let tue = self.emitter.intern_string("Tuesday");
                let wed = self.emitter.intern_string("Wednesday");
                let thu = self.emitter.intern_string("Thursday");
                let fri = self.emitter.intern_string("Friday");
                let sat = self.emitter.intern_string("Saturday");

                wasm!(self.func, {
                    local_get(wd); i64_eqz;
                    if_i32; i32_const(sun as i32);
                    else_;
                      local_get(wd); i64_const(1); i64_eq;
                      if_i32; i32_const(mon as i32);
                      else_;
                        local_get(wd); i64_const(2); i64_eq;
                        if_i32; i32_const(tue as i32);
                        else_;
                          local_get(wd); i64_const(3); i64_eq;
                          if_i32; i32_const(wed as i32);
                          else_;
                            local_get(wd); i64_const(4); i64_eq;
                            if_i32; i32_const(thu as i32);
                            else_;
                              local_get(wd); i64_const(5); i64_eq;
                              if_i32; i32_const(fri as i32);
                              else_;
                                i32_const(sat as i32);
                              end;
                            end;
                          end;
                        end;
                      end;
                    end;
                });

                self.scratch.free_i64(wd);
                self.scratch.free_i64(ts);
            }
            "parse_iso" => {
                // datetime.parse_iso(s: String) → Result[Int, String]
                let s = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let yr = self.scratch.alloc_i64();
                let mo = self.scratch.alloc_i64();
                let dy = self.scratch.alloc_i64();
                let hr = self.scratch.alloc_i64();
                let mi = self.scratch.alloc_i64();
                let se = self.scratch.alloc_i64();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(s); });

                let err_msg = self.emitter.intern_string("invalid datetime format");
                wasm!(self.func, {
                    local_get(s); i32_load(0); i32_const(19); i32_lt_u;
                    if_i32;
                      i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                      local_get(result); i32_const(1); i32_store(0);
                      local_get(result); i32_const(err_msg as i32); i32_store(4);
                      local_get(result);
                    else_;
                });

                self.emit_parse_digits(s, 0, 4, yr);
                self.emit_parse_digits(s, 5, 2, mo);
                self.emit_parse_digits(s, 8, 2, dy);
                self.emit_parse_digits(s, 11, 2, hr);
                self.emit_parse_digits(s, 14, 2, mi);
                self.emit_parse_digits(s, 17, 2, se);

                let a = self.scratch.alloc_i64();
                let y = self.scratch.alloc_i64();
                let m = self.scratch.alloc_i64();
                wasm!(self.func, {
                    i64_const(14); local_get(mo); i64_sub; i64_const(12); i64_div_s; local_set(a);
                    local_get(yr); i64_const(4800); i64_add; local_get(a); i64_sub; local_set(y);
                    local_get(mo); i64_const(12); local_get(a); i64_mul; i64_add; i64_const(3); i64_sub; local_set(m);
                    local_get(dy);
                    i64_const(153); local_get(m); i64_mul; i64_const(2); i64_add; i64_const(5); i64_div_s; i64_add;
                    i64_const(365); local_get(y); i64_mul; i64_add;
                    local_get(y); i64_const(4); i64_div_s; i64_add;
                    local_get(y); i64_const(100); i64_div_s; i64_sub;
                    local_get(y); i64_const(400); i64_div_s; i64_add;
                    i64_const(32045); i64_sub;
                    i64_const(2440588); i64_sub;
                    i64_const(86400); i64_mul;
                    local_get(hr); i64_const(3600); i64_mul; i64_add;
                    local_get(mi); i64_const(60); i64_mul; i64_add;
                    local_get(se); i64_add;
                    local_set(yr); // reuse as timestamp
                    // Build ok Result: [tag=0:i32][timestamp:i64] = 12 bytes
                    i32_const(12); call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); i32_const(0); i32_store(0);
                    local_get(result); local_get(yr); i64_store(4);
                    local_get(result);
                    end;
                });

                self.scratch.free_i64(m);
                self.scratch.free_i64(y);
                self.scratch.free_i64(a);
                self.scratch.free_i64(se);
                self.scratch.free_i64(mi);
                self.scratch.free_i64(hr);
                self.scratch.free_i64(dy);
                self.scratch.free_i64(mo);
                self.scratch.free_i64(yr);
                self.scratch.free_i32(result);
                self.scratch.free_i32(s);
            }
            _ => {
                self.emit_stub_call(args);
            }
        }
    }

    /// Write N decimal digits of an i64 value to a string buffer at a given byte offset.
    fn emit_write_decimal_digits(&mut self, ptr: u32, byte_offset: u32, val: u32, num_digits: u32) {
        let tmp = self.scratch.alloc_i64();
        wasm!(self.func, { local_get(val); local_set(tmp); });
        for i in (0..num_digits).rev() {
            let off = byte_offset + i;
            wasm!(self.func, {
                local_get(ptr);
                local_get(tmp); i64_const(10); i64_rem_s;
                i64_const(48); i64_add;
                i32_wrap_i64;
                i32_store8(off);
                local_get(tmp); i64_const(10); i64_div_s; local_set(tmp);
            });
        }
        self.scratch.free_i64(tmp);
    }

    /// Parse N decimal ASCII digits from a string buffer into an i64 local.
    fn emit_parse_digits(&mut self, str_local: u32, char_offset: u32, num_digits: u32, dest: u32) {
        wasm!(self.func, { i64_const(0); local_set(dest); });
        for i in 0..num_digits {
            let off = 4 + char_offset + i;
            wasm!(self.func, {
                local_get(dest); i64_const(10); i64_mul;
                local_get(str_local); i32_load8_u(off);
                i64_extend_i32_u; i64_const(48); i64_sub;
                i64_add;
                local_set(dest);
            });
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
            "status" if args.len() == 1 => {
                // http.status(resp) → Int (getter)
                self.emit_expr(&args[0]);
                wasm!(self.func, { i64_load(0); });
            }
            "status" if args.len() == 2 => {
                // http.status(resp, new_status) → Response (setter)
                let resp = self.scratch.alloc_i32();
                let new_resp = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(resp); });
                wasm!(self.func, {
                    i32_const(16); call(self.emitter.rt.alloc); local_set(new_resp);
                    local_get(new_resp);
                });
                self.emit_expr(&args[1]); // new status: i64
                wasm!(self.func, {
                    i64_store(0);
                    local_get(new_resp); local_get(resp); i32_load(8); i32_store(8);
                    local_get(new_resp); local_get(resp); i32_load(12); i32_store(12);
                    local_get(new_resp);
                });
                self.scratch.free_i32(new_resp);
                self.scratch.free_i32(resp);
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
            "set_cookie" => {
                // http.set_cookie(resp, name, value) = set_header(resp, "Set-Cookie", "name=value")
                let resp = self.scratch.alloc_i32();
                let cookie_val = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(resp); });
                self.emit_expr(&args[1]);
                let eq_str = self.emitter.intern_string("=");
                wasm!(self.func, { i32_const(eq_str as i32); call(self.emitter.rt.concat_str); });
                self.emit_expr(&args[2]);
                wasm!(self.func, { call(self.emitter.rt.concat_str); local_set(cookie_val); });
                let cookie_key = self.emitter.intern_string("Set-Cookie");
                let tuple_ptr = self.scratch.alloc_i32();
                let new_hdrs = self.scratch.alloc_i32();
                let old_hdrs = self.scratch.alloc_i32();
                let new_resp = self.scratch.alloc_i32();
                let new_len = self.scratch.alloc_i32();
                let ci = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(tuple_ptr);
                    local_get(tuple_ptr); i32_const(cookie_key as i32); i32_store(0);
                    local_get(tuple_ptr); local_get(cookie_val); i32_store(4);
                    local_get(resp); i32_load(12); local_set(old_hdrs);
                    local_get(old_hdrs); i32_load(0); i32_const(1); i32_add; local_set(new_len);
                    i32_const(4); local_get(new_len); i32_const(4); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(new_hdrs);
                    local_get(new_hdrs); local_get(new_len); i32_store(0);
                    i32_const(0); local_set(ci);
                    block_empty; loop_empty;
                      local_get(ci); local_get(old_hdrs); i32_load(0); i32_ge_u; br_if(1);
                      local_get(new_hdrs); i32_const(4); i32_add; local_get(ci); i32_const(4); i32_mul; i32_add;
                      local_get(old_hdrs); i32_const(4); i32_add; local_get(ci); i32_const(4); i32_mul; i32_add;
                      i32_load(0); i32_store(0);
                      local_get(ci); i32_const(1); i32_add; local_set(ci);
                      br(0);
                    end; end;
                    local_get(new_hdrs); i32_const(4); i32_add;
                    local_get(old_hdrs); i32_load(0); i32_const(4); i32_mul; i32_add;
                    local_get(tuple_ptr); i32_store(0);
                    i32_const(16); call(self.emitter.rt.alloc); local_set(new_resp);
                    local_get(new_resp); local_get(resp); i64_load(0); i64_store(0);
                    local_get(new_resp); local_get(resp); i32_load(8); i32_store(8);
                    local_get(new_resp); local_get(new_hdrs); i32_store(12);
                    local_get(new_resp);
                });
                self.scratch.free_i32(ci);
                self.scratch.free_i32(new_resp);
                self.scratch.free_i32(old_hdrs);
                self.scratch.free_i32(new_hdrs);
                self.scratch.free_i32(new_len);
                self.scratch.free_i32(tuple_ptr);
                self.scratch.free_i32(cookie_val);
                self.scratch.free_i32(resp);
            }
            "with_headers" => {
                // http.with_headers(status, body, headers_map) → Response
                // Map[String,String] layout: [len:i32][key0:i32][val0:i32][key1:i32][val1:i32]...
                // Each entry is key_size=4 + val_size=4 = 8 bytes
                let s = self.scratch.alloc_i32();
                let hdr_map = self.scratch.alloc_i32();
                let hdr_list = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let tuple = self.scratch.alloc_i32();
                wasm!(self.func, { i32_const(16); call(self.emitter.rt.alloc); local_set(s); local_get(s); });
                self.emit_expr(&args[0]); // status
                wasm!(self.func, { i64_store(0); local_get(s); });
                self.emit_expr(&args[1]); // body
                wasm!(self.func, { i32_store(8); });
                self.emit_expr(&args[2]); // headers map
                wasm!(self.func, {
                    local_set(hdr_map);
                    local_get(hdr_map); i32_load(0); local_set(len);
                    // Build headers list from map entries
                    i32_const(4); local_get(len); i32_const(4); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(hdr_list);
                    local_get(hdr_list); local_get(len); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      // Build tuple (key, val) from map entry
                      i32_const(8); call(self.emitter.rt.alloc); local_set(tuple);
                      local_get(tuple);
                      local_get(hdr_map); i32_const(4); i32_add;
                      local_get(i); i32_const(8); i32_mul; i32_add;
                      i32_load(0); i32_store(0); // key
                      local_get(tuple);
                      local_get(hdr_map); i32_const(4); i32_add;
                      local_get(i); i32_const(8); i32_mul; i32_add;
                      i32_load(4); i32_store(4); // val
                      local_get(hdr_list); i32_const(4); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add;
                      local_get(tuple); i32_store(0);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(s); local_get(hdr_list); i32_store(12);
                    local_get(s);
                });
                self.scratch.free_i32(tuple);
                self.scratch.free_i32(i);
                self.scratch.free_i32(len);
                self.scratch.free_i32(hdr_list);
                self.scratch.free_i32(hdr_map);
                self.scratch.free_i32(s);
            }
            "not_found" => {
                let s = self.scratch.alloc_i32();
                wasm!(self.func, { i32_const(16); call(self.emitter.rt.alloc); local_set(s); local_get(s); i64_const(404); i64_store(0); local_get(s); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(8); });
                let empty = self.scratch.alloc_i32();
                wasm!(self.func, { i32_const(4); call(self.emitter.rt.alloc); local_set(empty); local_get(empty); i32_const(0); i32_store(0); local_get(s); local_get(empty); i32_store(12); local_get(s); });
                self.scratch.free_i32(empty);
                self.scratch.free_i32(s);
            }
            "redirect" => {
                let s = self.scratch.alloc_i32();
                let empty_body = self.emitter.intern_string("");
                wasm!(self.func, { i32_const(16); call(self.emitter.rt.alloc); local_set(s); local_get(s); i64_const(302); i64_store(0); local_get(s); i32_const(empty_body as i32); i32_store(8); });
                let loc_key = self.emitter.intern_string("Location");
                let tuple = self.scratch.alloc_i32();
                let hdrs = self.scratch.alloc_i32();
                wasm!(self.func, { i32_const(8); call(self.emitter.rt.alloc); local_set(tuple); local_get(tuple); i32_const(loc_key as i32); i32_store(0); local_get(tuple); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(4); i32_const(8); call(self.emitter.rt.alloc); local_set(hdrs); local_get(hdrs); i32_const(1); i32_store(0); local_get(hdrs); local_get(tuple); i32_store(4); local_get(s); local_get(hdrs); i32_store(12); local_get(s); });
                self.scratch.free_i32(hdrs);
                self.scratch.free_i32(tuple);
                self.scratch.free_i32(s);
            }
            _ => {
                self.emit_stub_call(args);
            }
        }
    }

    /// fs module: read_text, write, exists
    fn emit_fs_call(&mut self, func: &str, args: &[IrExpr]) {
        match func {
            "read_text" => {
                // fs.read_text(path: String) -> Result[String, String]
                // 1. Evaluate path arg (Almide String ptr: [len:i32][data:u8...])
                let path_str = self.scratch.alloc_i32();
                let path_ptr = self.scratch.alloc_i32();
                let path_len = self.scratch.alloc_i32();
                let fd_out_ptr = self.scratch.alloc_i32();
                let opened_fd = self.scratch.alloc_i32();
                let stat_buf = self.scratch.alloc_i32();
                let file_size = self.scratch.alloc_i32();
                let data_buf = self.scratch.alloc_i32();
                let iov_ptr = self.scratch.alloc_i32();
                let nread_ptr = self.scratch.alloc_i32();
                let result_ptr = self.scratch.alloc_i32();
                let str_ptr = self.scratch.alloc_i32();
                let errno = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(path_str);
                    // path_ptr = path_str + 4 (skip length prefix)
                    local_get(path_str); i32_const(4); i32_add; local_set(path_ptr);
                    // path_len = *path_str
                    local_get(path_str); i32_load(0); local_set(path_len);
                });

                // Allocate fd_out (4 bytes) via bump allocator
                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc); local_set(fd_out_ptr);
                });

                // Strip leading '/' from path for WASI (requires relative path from preopened dir)
                wasm!(self.func, {
                    local_get(path_ptr); i32_load8_u(0); i32_const(47); i32_eq; // '/' == 47
                    if_empty;
                      local_get(path_ptr); i32_const(1); i32_add; local_set(path_ptr);
                      local_get(path_len); i32_const(1); i32_sub; local_set(path_len);
                    end;
                });
                // path_open(fd=3, dirflags=0, path_ptr, path_len, oflags=0,
                //           rights=fd_read|fd_seek (2|4=6), inheriting=0, fdflags=0, fd_out_ptr)
                wasm!(self.func, {
                    i32_const(3);
                    i32_const(0);
                    local_get(path_ptr);
                    local_get(path_len);
                    i32_const(0);
                    i64_const(6);
                    i64_const(0);
                    i32_const(0);
                    local_get(fd_out_ptr);
                    call(self.emitter.rt.path_open);
                    local_set(errno);
                });

                // If errno != 0, return err("file not found")
                wasm!(self.func, {
                    local_get(errno);
                    i32_const(0);
                    i32_ne;
                    if_i32;
                });
                // Build err result
                let err_msg = self.emitter.intern_string("file not found");
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(1); i32_store(0);
                    local_get(result_ptr); i32_const(err_msg as i32); i32_store(4);
                    local_get(result_ptr);
                    else_;
                });

                // Load opened fd
                wasm!(self.func, {
                    local_get(fd_out_ptr); i32_load(0); local_set(opened_fd);
                });

                // fd_filestat_get(fd, stat_buf) — stat_buf needs 64 bytes (allocator guarantees 8-byte alignment)
                wasm!(self.func, {
                    i32_const(64); call(self.emitter.rt.alloc); local_set(stat_buf);
                    local_get(opened_fd);
                    local_get(stat_buf);
                    call(self.emitter.rt.fd_filestat_get);
                    drop;
                });

                // file_size = i32(stat_buf[32..40]) — file size is at offset 32 as i64, take lower 32 bits
                wasm!(self.func, {
                    local_get(stat_buf); i32_const(32); i32_add; i32_load(0); local_set(file_size);
                });

                // Allocate buffer for file data
                wasm!(self.func, {
                    local_get(file_size); call(self.emitter.rt.alloc); local_set(data_buf);
                });

                // Build iov struct: [buf_ptr:i32, buf_len:i32]
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(iov_ptr);
                    local_get(iov_ptr); local_get(data_buf); i32_store(0);
                    local_get(iov_ptr); local_get(file_size); i32_store(4);
                });

                // nread_ptr
                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc); local_set(nread_ptr);
                });

                // fd_read(fd, iov_ptr, 1, nread_ptr)
                wasm!(self.func, {
                    local_get(opened_fd);
                    local_get(iov_ptr);
                    i32_const(1);
                    local_get(nread_ptr);
                    call(self.emitter.rt.fd_read);
                    drop;
                });

                // fd_close(fd)
                wasm!(self.func, {
                    local_get(opened_fd);
                    call(self.emitter.rt.fd_close);
                    drop;
                });

                // Build Almide String: [len:i32][data:u8...]
                // Use nread as actual length (may be <= file_size)
                wasm!(self.func, {
                    local_get(nread_ptr); i32_load(0); local_set(file_size);
                    local_get(file_size); i32_const(4); i32_add;
                    call(self.emitter.rt.alloc); local_set(str_ptr);
                    local_get(str_ptr); local_get(file_size); i32_store(0);
                });

                // Copy data_buf[0..file_size] to str_ptr+4
                // Byte-by-byte copy loop
                let counter = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(0); local_set(counter);
                    block_empty; loop_empty;
                    local_get(counter); local_get(file_size); i32_ge_u; br_if(1);
                    local_get(str_ptr); i32_const(4); i32_add; local_get(counter); i32_add;
                    local_get(data_buf); local_get(counter); i32_add;
                    i32_load8_u(0);
                    i32_store8(0);
                    local_get(counter); i32_const(1); i32_add; local_set(counter);
                    br(0);
                    end; end;
                });
                self.scratch.free_i32(counter);

                // Build ok result: [tag=0:i32][str_ptr:i32]
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(0); i32_store(0);
                    local_get(result_ptr); local_get(str_ptr); i32_store(4);
                    local_get(result_ptr);
                    end;
                });

                self.scratch.free_i32(errno);
                self.scratch.free_i32(str_ptr);
                self.scratch.free_i32(result_ptr);
                self.scratch.free_i32(nread_ptr);
                self.scratch.free_i32(iov_ptr);
                self.scratch.free_i32(data_buf);
                self.scratch.free_i32(file_size);
                self.scratch.free_i32(stat_buf);
                self.scratch.free_i32(opened_fd);
                self.scratch.free_i32(fd_out_ptr);
                self.scratch.free_i32(path_len);
                self.scratch.free_i32(path_ptr);
                self.scratch.free_i32(path_str);
            }
            "write" => {
                // fs.write(path: String, content: String) -> Result[Unit, String]
                let path_str = self.scratch.alloc_i32();
                let path_ptr = self.scratch.alloc_i32();
                let path_len = self.scratch.alloc_i32();
                let content_str = self.scratch.alloc_i32();
                let fd_out_ptr = self.scratch.alloc_i32();
                let opened_fd = self.scratch.alloc_i32();
                let iov_ptr = self.scratch.alloc_i32();
                let nwritten_ptr = self.scratch.alloc_i32();
                let result_ptr = self.scratch.alloc_i32();
                let errno = self.scratch.alloc_i32();

                // Evaluate path
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(path_str);
                    local_get(path_str); i32_const(4); i32_add; local_set(path_ptr);
                    local_get(path_str); i32_load(0); local_set(path_len);
                });

                // Evaluate content
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(content_str); });

                // Allocate fd_out
                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc); local_set(fd_out_ptr);
                });

                // Strip leading '/' from path for WASI (requires relative path from preopened dir)
                wasm!(self.func, {
                    local_get(path_ptr); i32_load8_u(0); i32_const(47); i32_eq;
                    if_empty;
                      local_get(path_ptr); i32_const(1); i32_add; local_set(path_ptr);
                      local_get(path_len); i32_const(1); i32_sub; local_set(path_len);
                    end;
                });
                // path_open(fd=3, dirflags=0, path_ptr, path_len,
                //           oflags=O_CREAT|O_TRUNC(=9),
                //           rights=fd_write(=64), inheriting=0, fdflags=0, fd_out_ptr)
                wasm!(self.func, {
                    i32_const(3);
                    i32_const(0);
                    local_get(path_ptr);
                    local_get(path_len);
                    i32_const(9);
                    i64_const(64);
                    i64_const(0);
                    i32_const(0);
                    local_get(fd_out_ptr);
                    call(self.emitter.rt.path_open);
                    local_set(errno);
                });

                // If errno != 0, return err
                wasm!(self.func, {
                    local_get(errno);
                    i32_const(0);
                    i32_ne;
                    if_i32;
                });
                let err_msg = self.emitter.intern_string("failed to open file for writing");
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(1); i32_store(0);
                    local_get(result_ptr); i32_const(err_msg as i32); i32_store(4);
                    local_get(result_ptr);
                    else_;
                });

                // Load opened fd
                wasm!(self.func, {
                    local_get(fd_out_ptr); i32_load(0); local_set(opened_fd);
                });

                // Build iov: [content_ptr+4, content_len]
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(iov_ptr);
                    local_get(iov_ptr); local_get(content_str); i32_const(4); i32_add; i32_store(0);
                    local_get(iov_ptr); local_get(content_str); i32_load(0); i32_store(4);
                });

                // nwritten_ptr
                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc); local_set(nwritten_ptr);
                });

                // fd_write(fd, iov_ptr, 1, nwritten_ptr)
                wasm!(self.func, {
                    local_get(opened_fd);
                    local_get(iov_ptr);
                    i32_const(1);
                    local_get(nwritten_ptr);
                    call(self.emitter.rt.fd_write);
                    drop;
                });

                // fd_close(fd)
                wasm!(self.func, {
                    local_get(opened_fd);
                    call(self.emitter.rt.fd_close);
                    drop;
                });

                // Build ok(unit) result: [tag=0:i32][0:i32]
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(0); i32_store(0);
                    local_get(result_ptr); i32_const(0); i32_store(4);
                    local_get(result_ptr);
                    end;
                });

                self.scratch.free_i32(errno);
                self.scratch.free_i32(result_ptr);
                self.scratch.free_i32(nwritten_ptr);
                self.scratch.free_i32(iov_ptr);
                self.scratch.free_i32(opened_fd);
                self.scratch.free_i32(fd_out_ptr);
                self.scratch.free_i32(content_str);
                self.scratch.free_i32(path_len);
                self.scratch.free_i32(path_ptr);
                self.scratch.free_i32(path_str);
            }
            "exists" => {
                // fs.exists(path: String) -> Bool
                let path_str = self.scratch.alloc_i32();
                let path_ptr = self.scratch.alloc_i32();
                let path_len = self.scratch.alloc_i32();
                let stat_buf = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(path_str);
                    local_get(path_str); i32_const(4); i32_add; local_set(path_ptr);
                    local_get(path_str); i32_load(0); local_set(path_len);
                });

                // Allocate 64-byte stat buffer (allocator guarantees 8-byte alignment)
                wasm!(self.func, {
                    i32_const(64); call(self.emitter.rt.alloc); local_set(stat_buf);
                });

                // path_filestat_get(fd=3, flags=0, path_ptr, path_len, stat_buf)
                wasm!(self.func, {
                    i32_const(3);
                    i32_const(0);
                    local_get(path_ptr);
                    local_get(path_len);
                    local_get(stat_buf);
                    call(self.emitter.rt.path_filestat_get);
                    // errno == 0 → true (1), else false (0)
                    i32_eqz;
                });

                self.scratch.free_i32(stat_buf);
                self.scratch.free_i32(path_len);
                self.scratch.free_i32(path_ptr);
                self.scratch.free_i32(path_str);
            }
            "read_bytes" => {
                // fs.read_bytes(path) -> Result[List[Int], String]
                let path_str = self.scratch.alloc_i32();
                let path_ptr = self.scratch.alloc_i32();
                let path_len = self.scratch.alloc_i32();
                let fd_out_ptr = self.scratch.alloc_i32();
                let opened_fd = self.scratch.alloc_i32();
                let stat_buf = self.scratch.alloc_i32();
                let file_size = self.scratch.alloc_i32();
                let data_buf = self.scratch.alloc_i32();
                let iov_ptr = self.scratch.alloc_i32();
                let nread_ptr = self.scratch.alloc_i32();
                let result_ptr = self.scratch.alloc_i32();
                let list_ptr = self.scratch.alloc_i32();
                let errno = self.scratch.alloc_i32();
                let counter = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(path_str);
                    local_get(path_str); i32_const(4); i32_add; local_set(path_ptr);
                    local_get(path_str); i32_load(0); local_set(path_len);
                });

                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc); local_set(fd_out_ptr);
                });

                // Strip leading '/'
                wasm!(self.func, {
                    local_get(path_ptr); i32_load8_u(0); i32_const(47); i32_eq;
                    if_empty;
                      local_get(path_ptr); i32_const(1); i32_add; local_set(path_ptr);
                      local_get(path_len); i32_const(1); i32_sub; local_set(path_len);
                    end;
                });

                // path_open for reading
                wasm!(self.func, {
                    i32_const(3); i32_const(0);
                    local_get(path_ptr); local_get(path_len);
                    i32_const(0); i64_const(6); i64_const(0); i32_const(0);
                    local_get(fd_out_ptr);
                    call(self.emitter.rt.path_open);
                    local_set(errno);
                });

                wasm!(self.func, {
                    local_get(errno); i32_const(0); i32_ne;
                    if_i32;
                });
                let err_msg = self.emitter.intern_string("file not found");
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(1); i32_store(0);
                    local_get(result_ptr); i32_const(err_msg as i32); i32_store(4);
                    local_get(result_ptr);
                    else_;
                });

                // stat for file size
                wasm!(self.func, {
                    local_get(fd_out_ptr); i32_load(0); local_set(opened_fd);
                    i32_const(64); call(self.emitter.rt.alloc); local_set(stat_buf);
                    local_get(opened_fd); local_get(stat_buf);
                    call(self.emitter.rt.fd_filestat_get); drop;
                    local_get(stat_buf); i32_const(32); i32_add; i32_load(0); local_set(file_size);
                });

                // Read raw bytes
                wasm!(self.func, {
                    local_get(file_size); call(self.emitter.rt.alloc); local_set(data_buf);
                    i32_const(8); call(self.emitter.rt.alloc); local_set(iov_ptr);
                    local_get(iov_ptr); local_get(data_buf); i32_store(0);
                    local_get(iov_ptr); local_get(file_size); i32_store(4);
                    i32_const(4); call(self.emitter.rt.alloc); local_set(nread_ptr);
                    local_get(opened_fd); local_get(iov_ptr); i32_const(1); local_get(nread_ptr);
                    call(self.emitter.rt.fd_read); drop;
                    local_get(opened_fd); call(self.emitter.rt.fd_close); drop;
                    local_get(nread_ptr); i32_load(0); local_set(file_size);
                });

                // Build List[Int]: [count:i32][i64 * count]
                wasm!(self.func, {
                    local_get(file_size); i32_const(8); i32_mul; i32_const(4); i32_add;
                    call(self.emitter.rt.alloc); local_set(list_ptr);
                    local_get(list_ptr); local_get(file_size); i32_store(0);
                });

                // Copy each byte as i64
                wasm!(self.func, {
                    i32_const(0); local_set(counter);
                    block_empty; loop_empty;
                    local_get(counter); local_get(file_size); i32_ge_u; br_if(1);
                    local_get(list_ptr); i32_const(4); i32_add;
                    local_get(counter); i32_const(8); i32_mul; i32_add;
                    local_get(data_buf); local_get(counter); i32_add; i32_load8_u(0);
                    i64_extend_i32_u;
                    i64_store(0);
                    local_get(counter); i32_const(1); i32_add; local_set(counter);
                    br(0);
                    end; end;
                });

                // ok(list_ptr)
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(0); i32_store(0);
                    local_get(result_ptr); local_get(list_ptr); i32_store(4);
                    local_get(result_ptr);
                    end;
                });

                self.scratch.free_i32(counter);
                self.scratch.free_i32(errno);
                self.scratch.free_i32(list_ptr);
                self.scratch.free_i32(result_ptr);
                self.scratch.free_i32(nread_ptr);
                self.scratch.free_i32(iov_ptr);
                self.scratch.free_i32(data_buf);
                self.scratch.free_i32(file_size);
                self.scratch.free_i32(stat_buf);
                self.scratch.free_i32(opened_fd);
                self.scratch.free_i32(fd_out_ptr);
                self.scratch.free_i32(path_len);
                self.scratch.free_i32(path_ptr);
                self.scratch.free_i32(path_str);
            }
            "write_bytes" => {
                // fs.write_bytes(path, bytes: List[Int]) -> Result[Unit, String]
                let path_str = self.scratch.alloc_i32();
                let path_ptr = self.scratch.alloc_i32();
                let path_len = self.scratch.alloc_i32();
                let list_ptr = self.scratch.alloc_i32();
                let fd_out_ptr = self.scratch.alloc_i32();
                let opened_fd = self.scratch.alloc_i32();
                let iov_ptr = self.scratch.alloc_i32();
                let nwritten_ptr = self.scratch.alloc_i32();
                let result_ptr = self.scratch.alloc_i32();
                let errno = self.scratch.alloc_i32();
                let byte_buf = self.scratch.alloc_i32();
                let count = self.scratch.alloc_i32();
                let counter = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(path_str);
                    local_get(path_str); i32_const(4); i32_add; local_set(path_ptr);
                    local_get(path_str); i32_load(0); local_set(path_len);
                });

                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(list_ptr); });

                // Convert List[Int] (i64 elements) to byte buffer
                wasm!(self.func, {
                    local_get(list_ptr); i32_load(0); local_set(count);
                    local_get(count); call(self.emitter.rt.alloc); local_set(byte_buf);
                    i32_const(0); local_set(counter);
                    block_empty; loop_empty;
                    local_get(counter); local_get(count); i32_ge_u; br_if(1);
                    local_get(byte_buf); local_get(counter); i32_add;
                    local_get(list_ptr); i32_const(4); i32_add;
                    local_get(counter); i32_const(8); i32_mul; i32_add;
                    i64_load(0); i32_wrap_i64;
                    i32_store8(0);
                    local_get(counter); i32_const(1); i32_add; local_set(counter);
                    br(0);
                    end; end;
                });

                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc); local_set(fd_out_ptr);
                });

                // Strip leading '/'
                wasm!(self.func, {
                    local_get(path_ptr); i32_load8_u(0); i32_const(47); i32_eq;
                    if_empty;
                      local_get(path_ptr); i32_const(1); i32_add; local_set(path_ptr);
                      local_get(path_len); i32_const(1); i32_sub; local_set(path_len);
                    end;
                });

                // path_open for writing (O_CREAT|O_TRUNC=9)
                wasm!(self.func, {
                    i32_const(3); i32_const(0);
                    local_get(path_ptr); local_get(path_len);
                    i32_const(9); i64_const(64); i64_const(0); i32_const(0);
                    local_get(fd_out_ptr);
                    call(self.emitter.rt.path_open);
                    local_set(errno);
                });

                wasm!(self.func, {
                    local_get(errno); i32_const(0); i32_ne;
                    if_i32;
                });
                let err_msg = self.emitter.intern_string("failed to open file for writing");
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(1); i32_store(0);
                    local_get(result_ptr); i32_const(err_msg as i32); i32_store(4);
                    local_get(result_ptr);
                    else_;
                });

                wasm!(self.func, {
                    local_get(fd_out_ptr); i32_load(0); local_set(opened_fd);
                    i32_const(8); call(self.emitter.rt.alloc); local_set(iov_ptr);
                    local_get(iov_ptr); local_get(byte_buf); i32_store(0);
                    local_get(iov_ptr); local_get(count); i32_store(4);
                    i32_const(4); call(self.emitter.rt.alloc); local_set(nwritten_ptr);
                    local_get(opened_fd); local_get(iov_ptr); i32_const(1); local_get(nwritten_ptr);
                    call(self.emitter.rt.fd_write); drop;
                    local_get(opened_fd); call(self.emitter.rt.fd_close); drop;
                });

                // ok(unit)
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(0); i32_store(0);
                    local_get(result_ptr); i32_const(0); i32_store(4);
                    local_get(result_ptr);
                    end;
                });

                self.scratch.free_i32(counter);
                self.scratch.free_i32(count);
                self.scratch.free_i32(byte_buf);
                self.scratch.free_i32(errno);
                self.scratch.free_i32(result_ptr);
                self.scratch.free_i32(nwritten_ptr);
                self.scratch.free_i32(iov_ptr);
                self.scratch.free_i32(opened_fd);
                self.scratch.free_i32(fd_out_ptr);
                self.scratch.free_i32(list_ptr);
                self.scratch.free_i32(path_len);
                self.scratch.free_i32(path_ptr);
                self.scratch.free_i32(path_str);
            }
            "append" => {
                // fs.append(path, content) -> Result[Unit, String]
                let path_str = self.scratch.alloc_i32();
                let path_ptr = self.scratch.alloc_i32();
                let path_len = self.scratch.alloc_i32();
                let content_str = self.scratch.alloc_i32();
                let fd_out_ptr = self.scratch.alloc_i32();
                let opened_fd = self.scratch.alloc_i32();
                let iov_ptr = self.scratch.alloc_i32();
                let nwritten_ptr = self.scratch.alloc_i32();
                let result_ptr = self.scratch.alloc_i32();
                let errno = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(path_str);
                    local_get(path_str); i32_const(4); i32_add; local_set(path_ptr);
                    local_get(path_str); i32_load(0); local_set(path_len);
                });

                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(content_str); });

                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc); local_set(fd_out_ptr);
                });

                // Strip leading '/'
                wasm!(self.func, {
                    local_get(path_ptr); i32_load8_u(0); i32_const(47); i32_eq;
                    if_empty;
                      local_get(path_ptr); i32_const(1); i32_add; local_set(path_ptr);
                      local_get(path_len); i32_const(1); i32_sub; local_set(path_len);
                    end;
                });

                // path_open: oflags=O_CREAT(1), rights=fd_write(64), fdflags=APPEND(1)
                wasm!(self.func, {
                    i32_const(3); i32_const(0);
                    local_get(path_ptr); local_get(path_len);
                    i32_const(1);
                    i64_const(64); i64_const(0);
                    i32_const(1);
                    local_get(fd_out_ptr);
                    call(self.emitter.rt.path_open);
                    local_set(errno);
                });

                wasm!(self.func, {
                    local_get(errno); i32_const(0); i32_ne;
                    if_i32;
                });
                let err_msg = self.emitter.intern_string("failed to open file for appending");
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(1); i32_store(0);
                    local_get(result_ptr); i32_const(err_msg as i32); i32_store(4);
                    local_get(result_ptr);
                    else_;
                });

                wasm!(self.func, {
                    local_get(fd_out_ptr); i32_load(0); local_set(opened_fd);
                    i32_const(8); call(self.emitter.rt.alloc); local_set(iov_ptr);
                    local_get(iov_ptr); local_get(content_str); i32_const(4); i32_add; i32_store(0);
                    local_get(iov_ptr); local_get(content_str); i32_load(0); i32_store(4);
                    i32_const(4); call(self.emitter.rt.alloc); local_set(nwritten_ptr);
                    local_get(opened_fd); local_get(iov_ptr); i32_const(1); local_get(nwritten_ptr);
                    call(self.emitter.rt.fd_write); drop;
                    local_get(opened_fd); call(self.emitter.rt.fd_close); drop;
                });

                // ok(unit)
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(0); i32_store(0);
                    local_get(result_ptr); i32_const(0); i32_store(4);
                    local_get(result_ptr);
                    end;
                });

                self.scratch.free_i32(errno);
                self.scratch.free_i32(result_ptr);
                self.scratch.free_i32(nwritten_ptr);
                self.scratch.free_i32(iov_ptr);
                self.scratch.free_i32(opened_fd);
                self.scratch.free_i32(fd_out_ptr);
                self.scratch.free_i32(content_str);
                self.scratch.free_i32(path_len);
                self.scratch.free_i32(path_ptr);
                self.scratch.free_i32(path_str);
            }
            "mkdir_p" => {
                // fs.mkdir_p(path) -> Result[Unit, String]
                let path_str = self.scratch.alloc_i32();
                let path_ptr = self.scratch.alloc_i32();
                let path_len = self.scratch.alloc_i32();
                let result_ptr = self.scratch.alloc_i32();
                let errno = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(path_str);
                    local_get(path_str); i32_const(4); i32_add; local_set(path_ptr);
                    local_get(path_str); i32_load(0); local_set(path_len);
                });

                // Strip leading '/'
                wasm!(self.func, {
                    local_get(path_ptr); i32_load8_u(0); i32_const(47); i32_eq;
                    if_empty;
                      local_get(path_ptr); i32_const(1); i32_add; local_set(path_ptr);
                      local_get(path_len); i32_const(1); i32_sub; local_set(path_len);
                    end;
                });

                // Iterative mkdir_p: create each prefix segment
                let seg_end = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(0); local_set(seg_end);
                    block_empty; loop_empty;
                    local_get(seg_end); local_get(path_len); i32_ge_u; br_if(1);
                    // Advance seg_end past current char
                    local_get(seg_end); i32_const(1); i32_add; local_set(seg_end);
                    // Skip to next '/' or end of path
                    block_empty; loop_empty;
                    local_get(seg_end); local_get(path_len); i32_ge_u; br_if(1);
                    local_get(path_ptr); local_get(seg_end); i32_add; i32_load8_u(0);
                    i32_const(47); i32_eq; br_if(1);
                    local_get(seg_end); i32_const(1); i32_add; local_set(seg_end);
                    br(0);
                    end; end;
                    // Try creating directory for path[0..seg_end]
                    i32_const(3);
                    local_get(path_ptr);
                    local_get(seg_end);
                    call(self.emitter.rt.path_create_directory);
                    drop;
                    br(0);
                    end; end;
                });
                self.scratch.free_i32(seg_end);

                // Final attempt: create the full path and check error
                wasm!(self.func, {
                    i32_const(3);
                    local_get(path_ptr);
                    local_get(path_len);
                    call(self.emitter.rt.path_create_directory);
                    local_set(errno);
                });

                // errno==0 or errno==20 (EEXIST) -> ok
                wasm!(self.func, {
                    local_get(errno); i32_eqz;
                    local_get(errno); i32_const(20); i32_eq;
                    i32_or;
                    if_i32;
                });
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(0); i32_store(0);
                    local_get(result_ptr); i32_const(0); i32_store(4);
                    local_get(result_ptr);
                    else_;
                });
                let err_msg = self.emitter.intern_string("failed to create directory");
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(1); i32_store(0);
                    local_get(result_ptr); i32_const(err_msg as i32); i32_store(4);
                    local_get(result_ptr);
                    end;
                });

                self.scratch.free_i32(errno);
                self.scratch.free_i32(result_ptr);
                self.scratch.free_i32(path_len);
                self.scratch.free_i32(path_ptr);
                self.scratch.free_i32(path_str);
            }
            "read_lines" => {
                // fs.read_lines(path) -> Result[List[String], String]
                // Call read_text internally, then split by '\n' using string.lines
                self.emit_fs_call_inner_read_text(args);
                let res = self.scratch.alloc_i32();
                let tag = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_set(res);
                    local_get(res); i32_load(0); local_set(tag);
                    local_get(tag); i32_eqz;
                    if_i32;
                });
                // ok path: split the string by '\n'
                let text_ptr = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_get(res); i32_const(4); i32_add; i32_load(0); local_set(text_ptr);
                    local_get(text_ptr);
                    call(self.emitter.rt.string.lines);
                });
                let result_ptr = self.scratch.alloc_i32();
                let list_val = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_set(list_val);
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(0); i32_store(0);
                    local_get(result_ptr); local_get(list_val); i32_store(4);
                    local_get(result_ptr);
                    else_;
                    local_get(res);
                    end;
                });

                self.scratch.free_i32(list_val);
                self.scratch.free_i32(result_ptr);
                self.scratch.free_i32(text_ptr);
                self.scratch.free_i32(tag);
                self.scratch.free_i32(res);
            }
            "list_dir" => {
                // fs.list_dir(path) -> Result[List[String], String]
                let path_str = self.scratch.alloc_i32();
                let path_ptr = self.scratch.alloc_i32();
                let path_len = self.scratch.alloc_i32();
                let fd_out_ptr = self.scratch.alloc_i32();
                let opened_fd = self.scratch.alloc_i32();
                let result_ptr = self.scratch.alloc_i32();
                let errno = self.scratch.alloc_i32();
                let dir_buf = self.scratch.alloc_i32();
                let bufused_ptr = self.scratch.alloc_i32();
                let bufused = self.scratch.alloc_i32();
                let offset = self.scratch.alloc_i32();
                let list_ptr = self.scratch.alloc_i32();
                let list_count = self.scratch.alloc_i32();
                let entry_name_len = self.scratch.alloc_i32();
                let str_ptr = self.scratch.alloc_i32();
                let counter = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(path_str);
                    local_get(path_str); i32_const(4); i32_add; local_set(path_ptr);
                    local_get(path_str); i32_load(0); local_set(path_len);
                });

                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc); local_set(fd_out_ptr);
                });

                // Strip leading '/'
                wasm!(self.func, {
                    local_get(path_ptr); i32_load8_u(0); i32_const(47); i32_eq;
                    if_empty;
                      local_get(path_ptr); i32_const(1); i32_add; local_set(path_ptr);
                      local_get(path_len); i32_const(1); i32_sub; local_set(path_len);
                    end;
                });

                // path_open for directory: dirflags=1(symlink follow), oflags=O_DIRECTORY(2)
                // rights = fd_readdir(0x4000)
                wasm!(self.func, {
                    i32_const(3); i32_const(1);
                    local_get(path_ptr); local_get(path_len);
                    i32_const(2);
                    i64_const(0x4000);
                    i64_const(0);
                    i32_const(0);
                    local_get(fd_out_ptr);
                    call(self.emitter.rt.path_open);
                    local_set(errno);
                });

                wasm!(self.func, {
                    local_get(errno); i32_const(0); i32_ne;
                    if_i32;
                });
                let err_msg = self.emitter.intern_string("failed to open directory");
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(1); i32_store(0);
                    local_get(result_ptr); i32_const(err_msg as i32); i32_store(4);
                    local_get(result_ptr);
                    else_;
                });

                wasm!(self.func, {
                    local_get(fd_out_ptr); i32_load(0); local_set(opened_fd);
                });

                // Allocate readdir buffer (4KB) and bufused output
                wasm!(self.func, {
                    i32_const(4096); call(self.emitter.rt.alloc); local_set(dir_buf);
                    i32_const(4); call(self.emitter.rt.alloc); local_set(bufused_ptr);
                });

                // fd_readdir(fd, buf, buf_len, cookie=0, bufused_ptr)
                wasm!(self.func, {
                    local_get(opened_fd);
                    local_get(dir_buf);
                    i32_const(4096);
                    i64_const(0);
                    local_get(bufused_ptr);
                    call(self.emitter.rt.fd_readdir);
                    drop;
                    local_get(bufused_ptr); i32_load(0); local_set(bufused);
                    local_get(opened_fd); call(self.emitter.rt.fd_close); drop;
                });

                // First pass: count entries (skipping "." and "..")
                // WASI dirent: d_next(8) + d_ino(8) + d_namlen(4) + d_type(4) = 24 bytes header
                wasm!(self.func, {
                    i32_const(0); local_set(offset);
                    i32_const(0); local_set(list_count);
                    block_empty; loop_empty;
                    local_get(offset); i32_const(24); i32_add;
                    local_get(bufused); i32_gt_u; br_if(1);
                    local_get(dir_buf); local_get(offset); i32_add; i32_const(16); i32_add;
                    i32_load(0); local_set(entry_name_len);
                });
                // Check for "." (namlen==1 && name[0]=='.')
                wasm!(self.func, {
                    local_get(entry_name_len); i32_const(1); i32_eq;
                    if_empty;
                      local_get(dir_buf); local_get(offset); i32_add; i32_const(24); i32_add;
                      i32_load8_u(0); i32_const(46); i32_eq;
                      if_empty;
                      else_;
                        local_get(list_count); i32_const(1); i32_add; local_set(list_count);
                      end;
                    else_;
                });
                // Check for ".." (namlen==2 && name[0]=='.' && name[1]=='.')
                wasm!(self.func, {
                      local_get(entry_name_len); i32_const(2); i32_eq;
                      if_empty;
                        local_get(list_count); i32_const(1); i32_add; local_set(list_count);
                      else_;
                        local_get(dir_buf); local_get(offset); i32_add; i32_const(24); i32_add;
                        i32_load8_u(0); i32_const(46); i32_eq;
                        local_get(dir_buf); local_get(offset); i32_add; i32_const(25); i32_add;
                        i32_load8_u(0); i32_const(46); i32_eq;
                        i32_and;
                        i32_eqz;
                        if_empty;
                        else_;
                          local_get(list_count); i32_const(1); i32_add; local_set(list_count);
                        end;
                      end;
                    end;
                });
                // Advance offset
                wasm!(self.func, {
                    local_get(offset); i32_const(24); i32_add; local_get(entry_name_len); i32_add;
                    local_set(offset);
                    br(0);
                    end; end;
                });

                // Allocate List[String]: [count:i32][ptr:i32 * count]
                wasm!(self.func, {
                    local_get(list_count); i32_const(4); i32_mul; i32_const(4); i32_add;
                    call(self.emitter.rt.alloc); local_set(list_ptr);
                    local_get(list_ptr); local_get(list_count); i32_store(0);
                });

                // Second pass: build string entries
                let copy_i = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(0); local_set(offset);
                    i32_const(0); local_set(counter);
                    block_empty; loop_empty;
                    local_get(offset); i32_const(24); i32_add;
                    local_get(bufused); i32_gt_u; br_if(1);
                    local_get(dir_buf); local_get(offset); i32_add; i32_const(16); i32_add;
                    i32_load(0); local_set(entry_name_len);
                });

                // Skip "."
                wasm!(self.func, {
                    local_get(entry_name_len); i32_const(1); i32_eq;
                    if_empty;
                      local_get(dir_buf); local_get(offset); i32_add; i32_const(24); i32_add;
                      i32_load8_u(0); i32_const(46); i32_eq;
                      if_empty;
                      else_;
                });
                // Not ".": build entry
                self.emit_fs_list_dir_build_entry(copy_i, entry_name_len, str_ptr, dir_buf, offset, list_ptr, counter);
                wasm!(self.func, {
                      end;
                    else_;
                });

                // namlen != 1: check ".."
                wasm!(self.func, {
                      local_get(entry_name_len); i32_const(2); i32_eq;
                      if_empty;
                });
                // namlen != 2: build entry
                self.emit_fs_list_dir_build_entry(copy_i, entry_name_len, str_ptr, dir_buf, offset, list_ptr, counter);
                wasm!(self.func, {
                      else_;
                        local_get(dir_buf); local_get(offset); i32_add; i32_const(24); i32_add;
                        i32_load8_u(0); i32_const(46); i32_eq;
                        local_get(dir_buf); local_get(offset); i32_add; i32_const(25); i32_add;
                        i32_load8_u(0); i32_const(46); i32_eq;
                        i32_and;
                        i32_eqz;
                        if_empty;
                });
                // Not "..": build entry
                self.emit_fs_list_dir_build_entry(copy_i, entry_name_len, str_ptr, dir_buf, offset, list_ptr, counter);
                wasm!(self.func, {
                        end;
                      end;
                    end;
                });

                // Advance offset
                wasm!(self.func, {
                    local_get(offset); i32_const(24); i32_add; local_get(entry_name_len); i32_add;
                    local_set(offset);
                    br(0);
                    end; end;
                });
                self.scratch.free_i32(copy_i);

                // Update list count
                wasm!(self.func, {
                    local_get(list_ptr); local_get(counter); i32_store(0);
                });

                // ok(list_ptr)
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(0); i32_store(0);
                    local_get(result_ptr); local_get(list_ptr); i32_store(4);
                    local_get(result_ptr);
                    end;
                });

                self.scratch.free_i32(counter);
                self.scratch.free_i32(str_ptr);
                self.scratch.free_i32(entry_name_len);
                self.scratch.free_i32(list_count);
                self.scratch.free_i32(list_ptr);
                self.scratch.free_i32(offset);
                self.scratch.free_i32(bufused);
                self.scratch.free_i32(bufused_ptr);
                self.scratch.free_i32(dir_buf);
                self.scratch.free_i32(errno);
                self.scratch.free_i32(result_ptr);
                self.scratch.free_i32(opened_fd);
                self.scratch.free_i32(fd_out_ptr);
                self.scratch.free_i32(path_len);
                self.scratch.free_i32(path_ptr);
                self.scratch.free_i32(path_str);
            }
            "is_dir" => {
                // fs.is_dir(path) -> Bool  (filetype 3 = directory)
                self.emit_fs_filetype_check(args, 3);
            }
            "is_file" => {
                // fs.is_file(path) -> Bool  (filetype 4 = regular file)
                self.emit_fs_filetype_check(args, 4);
            }
            "is_symlink" => {
                // fs.is_symlink(path) -> Bool  (filetype 7 = symbolic_link)
                // Use flags=0 (do NOT follow symlinks)
                let path_str = self.scratch.alloc_i32();
                let path_ptr = self.scratch.alloc_i32();
                let path_len = self.scratch.alloc_i32();
                let stat_buf = self.scratch.alloc_i32();
                let errno = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(path_str);
                    local_get(path_str); i32_const(4); i32_add; local_set(path_ptr);
                    local_get(path_str); i32_load(0); local_set(path_len);
                });

                wasm!(self.func, {
                    local_get(path_ptr); i32_load8_u(0); i32_const(47); i32_eq;
                    if_empty;
                      local_get(path_ptr); i32_const(1); i32_add; local_set(path_ptr);
                      local_get(path_len); i32_const(1); i32_sub; local_set(path_len);
                    end;
                });

                wasm!(self.func, {
                    i32_const(64); call(self.emitter.rt.alloc); local_set(stat_buf);
                    // flags=0: do NOT follow symlinks
                    i32_const(3); i32_const(0);
                    local_get(path_ptr); local_get(path_len);
                    local_get(stat_buf);
                    call(self.emitter.rt.path_filestat_get);
                    local_set(errno);
                    local_get(errno); i32_const(0); i32_ne;
                    if_i32;
                      i32_const(0);
                    else_;
                      local_get(stat_buf); i32_const(16); i32_add; i32_load8_u(0);
                      i32_const(7); i32_eq;
                    end;
                });

                self.scratch.free_i32(errno);
                self.scratch.free_i32(stat_buf);
                self.scratch.free_i32(path_len);
                self.scratch.free_i32(path_ptr);
                self.scratch.free_i32(path_str);
            }
            "copy" => {
                // fs.copy(src, dst) -> Result[Unit, String]
                // Read source file bytes, write to destination
                let src_str = self.scratch.alloc_i32();
                let src_ptr = self.scratch.alloc_i32();
                let src_len = self.scratch.alloc_i32();
                let dst_str = self.scratch.alloc_i32();
                let dst_ptr = self.scratch.alloc_i32();
                let dst_len = self.scratch.alloc_i32();
                let fd_out_ptr = self.scratch.alloc_i32();
                let opened_fd = self.scratch.alloc_i32();
                let stat_buf = self.scratch.alloc_i32();
                let file_size = self.scratch.alloc_i32();
                let data_buf = self.scratch.alloc_i32();
                let iov_ptr = self.scratch.alloc_i32();
                let nrw_ptr = self.scratch.alloc_i32();
                let result_ptr = self.scratch.alloc_i32();
                let errno = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(src_str);
                    local_get(src_str); i32_const(4); i32_add; local_set(src_ptr);
                    local_get(src_str); i32_load(0); local_set(src_len);
                });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(dst_str);
                    local_get(dst_str); i32_const(4); i32_add; local_set(dst_ptr);
                    local_get(dst_str); i32_load(0); local_set(dst_len);
                });

                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc); local_set(fd_out_ptr);
                });

                // Strip leading '/' from src
                wasm!(self.func, {
                    local_get(src_ptr); i32_load8_u(0); i32_const(47); i32_eq;
                    if_empty;
                      local_get(src_ptr); i32_const(1); i32_add; local_set(src_ptr);
                      local_get(src_len); i32_const(1); i32_sub; local_set(src_len);
                    end;
                });

                // Open source for reading
                wasm!(self.func, {
                    i32_const(3); i32_const(0);
                    local_get(src_ptr); local_get(src_len);
                    i32_const(0); i64_const(6); i64_const(0); i32_const(0);
                    local_get(fd_out_ptr);
                    call(self.emitter.rt.path_open);
                    local_set(errno);
                });

                wasm!(self.func, {
                    local_get(errno); i32_const(0); i32_ne;
                    if_i32;
                });
                let err_msg = self.emitter.intern_string("failed to open source file");
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(1); i32_store(0);
                    local_get(result_ptr); i32_const(err_msg as i32); i32_store(4);
                    local_get(result_ptr);
                    else_;
                });

                // Read source content
                wasm!(self.func, {
                    local_get(fd_out_ptr); i32_load(0); local_set(opened_fd);
                    i32_const(64); call(self.emitter.rt.alloc); local_set(stat_buf);
                    local_get(opened_fd); local_get(stat_buf);
                    call(self.emitter.rt.fd_filestat_get); drop;
                    local_get(stat_buf); i32_const(32); i32_add; i32_load(0); local_set(file_size);
                    local_get(file_size); call(self.emitter.rt.alloc); local_set(data_buf);
                    i32_const(8); call(self.emitter.rt.alloc); local_set(iov_ptr);
                    local_get(iov_ptr); local_get(data_buf); i32_store(0);
                    local_get(iov_ptr); local_get(file_size); i32_store(4);
                    i32_const(4); call(self.emitter.rt.alloc); local_set(nrw_ptr);
                    local_get(opened_fd); local_get(iov_ptr); i32_const(1); local_get(nrw_ptr);
                    call(self.emitter.rt.fd_read); drop;
                    local_get(opened_fd); call(self.emitter.rt.fd_close); drop;
                    local_get(nrw_ptr); i32_load(0); local_set(file_size);
                });

                // Strip leading '/' from dst
                wasm!(self.func, {
                    local_get(dst_ptr); i32_load8_u(0); i32_const(47); i32_eq;
                    if_empty;
                      local_get(dst_ptr); i32_const(1); i32_add; local_set(dst_ptr);
                      local_get(dst_len); i32_const(1); i32_sub; local_set(dst_len);
                    end;
                });

                // Open dst for writing
                wasm!(self.func, {
                    i32_const(3); i32_const(0);
                    local_get(dst_ptr); local_get(dst_len);
                    i32_const(9); i64_const(64); i64_const(0); i32_const(0);
                    local_get(fd_out_ptr);
                    call(self.emitter.rt.path_open);
                    local_set(errno);
                });

                wasm!(self.func, {
                    local_get(errno); i32_const(0); i32_ne;
                    if_i32;
                });
                let err_msg2 = self.emitter.intern_string("failed to open destination file");
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(1); i32_store(0);
                    local_get(result_ptr); i32_const(err_msg2 as i32); i32_store(4);
                    local_get(result_ptr);
                    else_;
                });

                // Write data to dst
                wasm!(self.func, {
                    local_get(fd_out_ptr); i32_load(0); local_set(opened_fd);
                    local_get(iov_ptr); local_get(data_buf); i32_store(0);
                    local_get(iov_ptr); local_get(file_size); i32_store(4);
                    local_get(opened_fd); local_get(iov_ptr); i32_const(1); local_get(nrw_ptr);
                    call(self.emitter.rt.fd_write); drop;
                    local_get(opened_fd); call(self.emitter.rt.fd_close); drop;
                });

                // ok(unit) -- close nested if blocks
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(0); i32_store(0);
                    local_get(result_ptr); i32_const(0); i32_store(4);
                    local_get(result_ptr);
                    end;
                    end;
                });

                self.scratch.free_i32(errno);
                self.scratch.free_i32(result_ptr);
                self.scratch.free_i32(nrw_ptr);
                self.scratch.free_i32(iov_ptr);
                self.scratch.free_i32(data_buf);
                self.scratch.free_i32(file_size);
                self.scratch.free_i32(stat_buf);
                self.scratch.free_i32(opened_fd);
                self.scratch.free_i32(fd_out_ptr);
                self.scratch.free_i32(dst_len);
                self.scratch.free_i32(dst_ptr);
                self.scratch.free_i32(dst_str);
                self.scratch.free_i32(src_len);
                self.scratch.free_i32(src_ptr);
                self.scratch.free_i32(src_str);
            }
            "rename" => {
                // fs.rename(src, dst) -> Result[Unit, String]
                let src_str = self.scratch.alloc_i32();
                let src_ptr = self.scratch.alloc_i32();
                let src_len = self.scratch.alloc_i32();
                let dst_str = self.scratch.alloc_i32();
                let dst_ptr = self.scratch.alloc_i32();
                let dst_len = self.scratch.alloc_i32();
                let result_ptr = self.scratch.alloc_i32();
                let errno = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(src_str);
                    local_get(src_str); i32_const(4); i32_add; local_set(src_ptr);
                    local_get(src_str); i32_load(0); local_set(src_len);
                });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(dst_str);
                    local_get(dst_str); i32_const(4); i32_add; local_set(dst_ptr);
                    local_get(dst_str); i32_load(0); local_set(dst_len);
                });

                // Strip leading '/' from src and dst
                wasm!(self.func, {
                    local_get(src_ptr); i32_load8_u(0); i32_const(47); i32_eq;
                    if_empty;
                      local_get(src_ptr); i32_const(1); i32_add; local_set(src_ptr);
                      local_get(src_len); i32_const(1); i32_sub; local_set(src_len);
                    end;
                    local_get(dst_ptr); i32_load8_u(0); i32_const(47); i32_eq;
                    if_empty;
                      local_get(dst_ptr); i32_const(1); i32_add; local_set(dst_ptr);
                      local_get(dst_len); i32_const(1); i32_sub; local_set(dst_len);
                    end;
                });

                // path_rename(old_fd=3, old_path, old_len, new_fd=3, new_path, new_len)
                wasm!(self.func, {
                    i32_const(3);
                    local_get(src_ptr); local_get(src_len);
                    i32_const(3);
                    local_get(dst_ptr); local_get(dst_len);
                    call(self.emitter.rt.path_rename);
                    local_set(errno);
                });

                wasm!(self.func, {
                    local_get(errno); i32_eqz;
                    if_i32;
                });
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(0); i32_store(0);
                    local_get(result_ptr); i32_const(0); i32_store(4);
                    local_get(result_ptr);
                    else_;
                });
                let err_msg = self.emitter.intern_string("failed to rename");
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(1); i32_store(0);
                    local_get(result_ptr); i32_const(err_msg as i32); i32_store(4);
                    local_get(result_ptr);
                    end;
                });

                self.scratch.free_i32(errno);
                self.scratch.free_i32(result_ptr);
                self.scratch.free_i32(dst_len);
                self.scratch.free_i32(dst_ptr);
                self.scratch.free_i32(dst_str);
                self.scratch.free_i32(src_len);
                self.scratch.free_i32(src_ptr);
                self.scratch.free_i32(src_str);
            }
            "remove" => {
                // fs.remove(path) -> Result[Unit, String]
                let path_str = self.scratch.alloc_i32();
                let path_ptr = self.scratch.alloc_i32();
                let path_len = self.scratch.alloc_i32();
                let result_ptr = self.scratch.alloc_i32();
                let errno = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(path_str);
                    local_get(path_str); i32_const(4); i32_add; local_set(path_ptr);
                    local_get(path_str); i32_load(0); local_set(path_len);
                });

                wasm!(self.func, {
                    local_get(path_ptr); i32_load8_u(0); i32_const(47); i32_eq;
                    if_empty;
                      local_get(path_ptr); i32_const(1); i32_add; local_set(path_ptr);
                      local_get(path_len); i32_const(1); i32_sub; local_set(path_len);
                    end;
                });

                wasm!(self.func, {
                    i32_const(3);
                    local_get(path_ptr); local_get(path_len);
                    call(self.emitter.rt.path_unlink_file);
                    local_set(errno);
                });

                wasm!(self.func, {
                    local_get(errno); i32_eqz;
                    if_i32;
                });
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(0); i32_store(0);
                    local_get(result_ptr); i32_const(0); i32_store(4);
                    local_get(result_ptr);
                    else_;
                });
                let err_msg = self.emitter.intern_string("failed to remove file");
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(1); i32_store(0);
                    local_get(result_ptr); i32_const(err_msg as i32); i32_store(4);
                    local_get(result_ptr);
                    end;
                });

                self.scratch.free_i32(errno);
                self.scratch.free_i32(result_ptr);
                self.scratch.free_i32(path_len);
                self.scratch.free_i32(path_ptr);
                self.scratch.free_i32(path_str);
            }
            "remove_all" => {
                // fs.remove_all(path) -> Result[Unit, String]
                // Try unlink (file), then rmdir (empty dir)
                let path_str = self.scratch.alloc_i32();
                let path_ptr = self.scratch.alloc_i32();
                let path_len = self.scratch.alloc_i32();
                let result_ptr = self.scratch.alloc_i32();
                let errno = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(path_str);
                    local_get(path_str); i32_const(4); i32_add; local_set(path_ptr);
                    local_get(path_str); i32_load(0); local_set(path_len);
                });

                wasm!(self.func, {
                    local_get(path_ptr); i32_load8_u(0); i32_const(47); i32_eq;
                    if_empty;
                      local_get(path_ptr); i32_const(1); i32_add; local_set(path_ptr);
                      local_get(path_len); i32_const(1); i32_sub; local_set(path_len);
                    end;
                });

                // Try path_unlink_file first
                wasm!(self.func, {
                    i32_const(3);
                    local_get(path_ptr); local_get(path_len);
                    call(self.emitter.rt.path_unlink_file);
                    local_set(errno);
                });

                wasm!(self.func, {
                    local_get(errno); i32_eqz;
                    if_i32;
                });
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(0); i32_store(0);
                    local_get(result_ptr); i32_const(0); i32_store(4);
                    local_get(result_ptr);
                    else_;
                });

                // Try path_remove_directory
                wasm!(self.func, {
                    i32_const(3);
                    local_get(path_ptr); local_get(path_len);
                    call(self.emitter.rt.path_remove_directory);
                    local_set(errno);
                });

                wasm!(self.func, {
                    local_get(errno); i32_eqz;
                    if_i32;
                });
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(0); i32_store(0);
                    local_get(result_ptr); i32_const(0); i32_store(4);
                    local_get(result_ptr);
                    else_;
                });
                let err_msg = self.emitter.intern_string("failed to remove path");
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(1); i32_store(0);
                    local_get(result_ptr); i32_const(err_msg as i32); i32_store(4);
                    local_get(result_ptr);
                    end;
                    end;
                });

                self.scratch.free_i32(errno);
                self.scratch.free_i32(result_ptr);
                self.scratch.free_i32(path_len);
                self.scratch.free_i32(path_ptr);
                self.scratch.free_i32(path_str);
            }
            "file_size" => {
                // fs.file_size(path) -> Result[Int, String]
                let path_str = self.scratch.alloc_i32();
                let path_ptr = self.scratch.alloc_i32();
                let path_len = self.scratch.alloc_i32();
                let stat_buf = self.scratch.alloc_i32();
                let result_ptr = self.scratch.alloc_i32();
                let errno = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(path_str);
                    local_get(path_str); i32_const(4); i32_add; local_set(path_ptr);
                    local_get(path_str); i32_load(0); local_set(path_len);
                });

                wasm!(self.func, {
                    local_get(path_ptr); i32_load8_u(0); i32_const(47); i32_eq;
                    if_empty;
                      local_get(path_ptr); i32_const(1); i32_add; local_set(path_ptr);
                      local_get(path_len); i32_const(1); i32_sub; local_set(path_len);
                    end;
                });

                wasm!(self.func, {
                    i32_const(64); call(self.emitter.rt.alloc); local_set(stat_buf);
                    i32_const(3); i32_const(1);
                    local_get(path_ptr); local_get(path_len);
                    local_get(stat_buf);
                    call(self.emitter.rt.path_filestat_get);
                    local_set(errno);
                });

                wasm!(self.func, {
                    local_get(errno); i32_eqz;
                    if_i32;
                });
                // ok: file size at offset 32 as i64
                // Result[Int, String] = [tag:i32][padding:i32][i64] = 16 bytes
                wasm!(self.func, {
                    i32_const(16); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(0); i32_store(0);
                    local_get(result_ptr); i32_const(8); i32_add;
                    local_get(stat_buf); i32_const(32); i32_add; i64_load(0);
                    i64_store(0);
                    local_get(result_ptr);
                    else_;
                });
                let err_msg = self.emitter.intern_string("file not found");
                wasm!(self.func, {
                    i32_const(16); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(1); i32_store(0);
                    local_get(result_ptr); i32_const(8); i32_add;
                    i32_const(err_msg as i32); i64_extend_i32_u; i64_store(0);
                    local_get(result_ptr);
                    end;
                });

                self.scratch.free_i32(errno);
                self.scratch.free_i32(result_ptr);
                self.scratch.free_i32(stat_buf);
                self.scratch.free_i32(path_len);
                self.scratch.free_i32(path_ptr);
                self.scratch.free_i32(path_str);
            }
            "modified_at" => {
                // fs.modified_at(path) -> Result[Int, String]
                // mtim at offset 40 (u64, nanoseconds) -> seconds
                let path_str = self.scratch.alloc_i32();
                let path_ptr = self.scratch.alloc_i32();
                let path_len = self.scratch.alloc_i32();
                let stat_buf = self.scratch.alloc_i32();
                let result_ptr = self.scratch.alloc_i32();
                let errno = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(path_str);
                    local_get(path_str); i32_const(4); i32_add; local_set(path_ptr);
                    local_get(path_str); i32_load(0); local_set(path_len);
                });

                wasm!(self.func, {
                    local_get(path_ptr); i32_load8_u(0); i32_const(47); i32_eq;
                    if_empty;
                      local_get(path_ptr); i32_const(1); i32_add; local_set(path_ptr);
                      local_get(path_len); i32_const(1); i32_sub; local_set(path_len);
                    end;
                });

                wasm!(self.func, {
                    i32_const(64); call(self.emitter.rt.alloc); local_set(stat_buf);
                    i32_const(3); i32_const(1);
                    local_get(path_ptr); local_get(path_len);
                    local_get(stat_buf);
                    call(self.emitter.rt.path_filestat_get);
                    local_set(errno);
                });

                wasm!(self.func, {
                    local_get(errno); i32_eqz;
                    if_i32;
                });
                wasm!(self.func, {
                    i32_const(16); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(0); i32_store(0);
                    local_get(result_ptr); i32_const(8); i32_add;
                    local_get(stat_buf); i32_const(40); i32_add; i64_load(0);
                    i64_const(1000000000); i64_div_u;
                    i64_store(0);
                    local_get(result_ptr);
                    else_;
                });
                let err_msg = self.emitter.intern_string("file not found");
                wasm!(self.func, {
                    i32_const(16); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(1); i32_store(0);
                    local_get(result_ptr); i32_const(8); i32_add;
                    i32_const(err_msg as i32); i64_extend_i32_u; i64_store(0);
                    local_get(result_ptr);
                    end;
                });

                self.scratch.free_i32(errno);
                self.scratch.free_i32(result_ptr);
                self.scratch.free_i32(stat_buf);
                self.scratch.free_i32(path_len);
                self.scratch.free_i32(path_ptr);
                self.scratch.free_i32(path_str);
            }
            "stat" => {
                // fs.stat(path) -> Result[{size: Int, is_dir: Bool, is_file: Bool, modified: Int}, String]
                let path_str = self.scratch.alloc_i32();
                let path_ptr = self.scratch.alloc_i32();
                let path_len = self.scratch.alloc_i32();
                let stat_buf = self.scratch.alloc_i32();
                let result_ptr = self.scratch.alloc_i32();
                let rec_ptr = self.scratch.alloc_i32();
                let errno = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(path_str);
                    local_get(path_str); i32_const(4); i32_add; local_set(path_ptr);
                    local_get(path_str); i32_load(0); local_set(path_len);
                });

                wasm!(self.func, {
                    local_get(path_ptr); i32_load8_u(0); i32_const(47); i32_eq;
                    if_empty;
                      local_get(path_ptr); i32_const(1); i32_add; local_set(path_ptr);
                      local_get(path_len); i32_const(1); i32_sub; local_set(path_len);
                    end;
                });

                wasm!(self.func, {
                    i32_const(64); call(self.emitter.rt.alloc); local_set(stat_buf);
                    i32_const(3); i32_const(1);
                    local_get(path_ptr); local_get(path_len);
                    local_get(stat_buf);
                    call(self.emitter.rt.path_filestat_get);
                    local_set(errno);
                });

                wasm!(self.func, {
                    local_get(errno); i32_eqz;
                    if_i32;
                });

                // Record: [size:i64(8)][is_dir:i32(4)][is_file:i32(4)][modified:i64(8)] = 24 bytes
                wasm!(self.func, {
                    i32_const(24); call(self.emitter.rt.alloc); local_set(rec_ptr);
                    // size at stat offset 32
                    local_get(rec_ptr);
                    local_get(stat_buf); i32_const(32); i32_add; i64_load(0);
                    i64_store(0);
                    // is_dir: filetype at offset 16 == 3
                    local_get(rec_ptr); i32_const(8); i32_add;
                    local_get(stat_buf); i32_const(16); i32_add; i32_load8_u(0);
                    i32_const(3); i32_eq;
                    i32_store(0);
                    // is_file: filetype at offset 16 == 4
                    local_get(rec_ptr); i32_const(12); i32_add;
                    local_get(stat_buf); i32_const(16); i32_add; i32_load8_u(0);
                    i32_const(4); i32_eq;
                    i32_store(0);
                    // modified: mtim at stat offset 40, nanoseconds -> seconds
                    local_get(rec_ptr); i32_const(16); i32_add;
                    local_get(stat_buf); i32_const(40); i32_add; i64_load(0);
                    i64_const(1000000000); i64_div_u;
                    i64_store(0);
                });

                // ok(rec_ptr)
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(0); i32_store(0);
                    local_get(result_ptr); local_get(rec_ptr); i32_store(4);
                    local_get(result_ptr);
                    else_;
                });
                let err_msg = self.emitter.intern_string("file not found");
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(1); i32_store(0);
                    local_get(result_ptr); i32_const(err_msg as i32); i32_store(4);
                    local_get(result_ptr);
                    end;
                });

                self.scratch.free_i32(errno);
                self.scratch.free_i32(rec_ptr);
                self.scratch.free_i32(result_ptr);
                self.scratch.free_i32(stat_buf);
                self.scratch.free_i32(path_len);
                self.scratch.free_i32(path_ptr);
                self.scratch.free_i32(path_str);
            }
            "walk" | "glob" | "create_temp_file" | "create_temp_dir" => {
                // These require recursive dir traversal (walk), glob pattern matching (glob),
                // or OS temp dir + random naming (create_temp_*) which are infeasible in pure WASI.
                for arg in args { self.emit_expr(arg); if super::values::ty_to_valtype(&arg.ty).is_some() { wasm!(self.func, { drop; }); } }
                let result_ptr = self.scratch.alloc_i32();
                let err_msg = self.emitter.intern_string("not supported in WASM");
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
                    local_get(result_ptr); i32_const(1); i32_store(0);
                    local_get(result_ptr); i32_const(err_msg as i32); i32_store(4);
                    local_get(result_ptr);
                });
                self.scratch.free_i32(result_ptr);
            }
            "temp_dir" => {
                // fs.temp_dir() -> String: return "/tmp"
                let s = self.emitter.intern_string("/tmp");
                wasm!(self.func, { i32_const(s as i32); });
            }
            _ => {
                self.emit_stub_call(args);
            }
        }
    }

    /// Helper: check path filetype against expected value. Used by is_dir, is_file.
    fn emit_fs_filetype_check(&mut self, args: &[IrExpr], expected_filetype: i32) {
        let path_str = self.scratch.alloc_i32();
        let path_ptr = self.scratch.alloc_i32();
        let path_len = self.scratch.alloc_i32();
        let stat_buf = self.scratch.alloc_i32();
        let errno = self.scratch.alloc_i32();

        self.emit_expr(&args[0]);
        wasm!(self.func, {
            local_set(path_str);
            local_get(path_str); i32_const(4); i32_add; local_set(path_ptr);
            local_get(path_str); i32_load(0); local_set(path_len);
        });

        // Strip leading '/'
        wasm!(self.func, {
            local_get(path_ptr); i32_load8_u(0); i32_const(47); i32_eq;
            if_empty;
              local_get(path_ptr); i32_const(1); i32_add; local_set(path_ptr);
              local_get(path_len); i32_const(1); i32_sub; local_set(path_len);
            end;
        });

        wasm!(self.func, {
            i32_const(64); call(self.emitter.rt.alloc); local_set(stat_buf);
            // flags=1 (follow symlinks) for is_dir/is_file
            i32_const(3); i32_const(1);
            local_get(path_ptr); local_get(path_len);
            local_get(stat_buf);
            call(self.emitter.rt.path_filestat_get);
            local_set(errno);
            local_get(errno); i32_const(0); i32_ne;
            if_i32;
              i32_const(0);
            else_;
              // filetype at stat offset 16
              local_get(stat_buf); i32_const(16); i32_add; i32_load8_u(0);
              i32_const(expected_filetype);
              i32_eq;
            end;
        });

        self.scratch.free_i32(errno);
        self.scratch.free_i32(stat_buf);
        self.scratch.free_i32(path_len);
        self.scratch.free_i32(path_ptr);
        self.scratch.free_i32(path_str);
    }

    /// Helper for list_dir: build a string entry from dirent name and store into list.
    fn emit_fs_list_dir_build_entry(
        &mut self,
        copy_i: u32, entry_name_len: u32, str_ptr: u32,
        dir_buf: u32, offset: u32, list_ptr: u32, counter: u32,
    ) {
        wasm!(self.func, {
            local_get(entry_name_len); i32_const(4); i32_add;
            call(self.emitter.rt.alloc); local_set(str_ptr);
            local_get(str_ptr); local_get(entry_name_len); i32_store(0);
            // Copy name bytes
            i32_const(0); local_set(copy_i);
            block_empty; loop_empty;
            local_get(copy_i); local_get(entry_name_len); i32_ge_u; br_if(1);
            local_get(str_ptr); i32_const(4); i32_add; local_get(copy_i); i32_add;
            local_get(dir_buf); local_get(offset); i32_add; i32_const(24); i32_add;
            local_get(copy_i); i32_add; i32_load8_u(0);
            i32_store8(0);
            local_get(copy_i); i32_const(1); i32_add; local_set(copy_i);
            br(0);
            end; end;
            // Store in list
            local_get(list_ptr); i32_const(4); i32_add;
            local_get(counter); i32_const(4); i32_mul; i32_add;
            local_get(str_ptr); i32_store(0);
            local_get(counter); i32_const(1); i32_add; local_set(counter);
        });
    }

    /// Helper: emit read_text logic, leaving Result[String, String] on stack.
    fn emit_fs_call_inner_read_text(&mut self, args: &[IrExpr]) {
        let path_str = self.scratch.alloc_i32();
        let path_ptr = self.scratch.alloc_i32();
        let path_len = self.scratch.alloc_i32();
        let fd_out_ptr = self.scratch.alloc_i32();
        let opened_fd = self.scratch.alloc_i32();
        let stat_buf = self.scratch.alloc_i32();
        let file_size = self.scratch.alloc_i32();
        let data_buf = self.scratch.alloc_i32();
        let iov_ptr = self.scratch.alloc_i32();
        let nread_ptr = self.scratch.alloc_i32();
        let result_ptr = self.scratch.alloc_i32();
        let str_ptr = self.scratch.alloc_i32();
        let errno = self.scratch.alloc_i32();

        self.emit_expr(&args[0]);
        wasm!(self.func, {
            local_set(path_str);
            local_get(path_str); i32_const(4); i32_add; local_set(path_ptr);
            local_get(path_str); i32_load(0); local_set(path_len);
        });

        wasm!(self.func, {
            i32_const(4); call(self.emitter.rt.alloc); local_set(fd_out_ptr);
        });

        wasm!(self.func, {
            local_get(path_ptr); i32_load8_u(0); i32_const(47); i32_eq;
            if_empty;
              local_get(path_ptr); i32_const(1); i32_add; local_set(path_ptr);
              local_get(path_len); i32_const(1); i32_sub; local_set(path_len);
            end;
        });

        wasm!(self.func, {
            i32_const(3); i32_const(0);
            local_get(path_ptr); local_get(path_len);
            i32_const(0); i64_const(6); i64_const(0); i32_const(0);
            local_get(fd_out_ptr);
            call(self.emitter.rt.path_open);
            local_set(errno);
        });

        wasm!(self.func, {
            local_get(errno); i32_const(0); i32_ne;
            if_i32;
        });
        let err_msg = self.emitter.intern_string("file not found");
        wasm!(self.func, {
            i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
            local_get(result_ptr); i32_const(1); i32_store(0);
            local_get(result_ptr); i32_const(err_msg as i32); i32_store(4);
            local_get(result_ptr);
            else_;
        });

        wasm!(self.func, {
            local_get(fd_out_ptr); i32_load(0); local_set(opened_fd);
            i32_const(64); call(self.emitter.rt.alloc); local_set(stat_buf);
            local_get(opened_fd); local_get(stat_buf);
            call(self.emitter.rt.fd_filestat_get); drop;
            local_get(stat_buf); i32_const(32); i32_add; i32_load(0); local_set(file_size);
            local_get(file_size); call(self.emitter.rt.alloc); local_set(data_buf);
            i32_const(8); call(self.emitter.rt.alloc); local_set(iov_ptr);
            local_get(iov_ptr); local_get(data_buf); i32_store(0);
            local_get(iov_ptr); local_get(file_size); i32_store(4);
            i32_const(4); call(self.emitter.rt.alloc); local_set(nread_ptr);
            local_get(opened_fd); local_get(iov_ptr); i32_const(1); local_get(nread_ptr);
            call(self.emitter.rt.fd_read); drop;
            local_get(opened_fd); call(self.emitter.rt.fd_close); drop;
            local_get(nread_ptr); i32_load(0); local_set(file_size);
        });

        wasm!(self.func, {
            local_get(file_size); i32_const(4); i32_add;
            call(self.emitter.rt.alloc); local_set(str_ptr);
            local_get(str_ptr); local_get(file_size); i32_store(0);
        });

        let counter = self.scratch.alloc_i32();
        wasm!(self.func, {
            i32_const(0); local_set(counter);
            block_empty; loop_empty;
            local_get(counter); local_get(file_size); i32_ge_u; br_if(1);
            local_get(str_ptr); i32_const(4); i32_add; local_get(counter); i32_add;
            local_get(data_buf); local_get(counter); i32_add;
            i32_load8_u(0);
            i32_store8(0);
            local_get(counter); i32_const(1); i32_add; local_set(counter);
            br(0);
            end; end;
        });
        self.scratch.free_i32(counter);

        wasm!(self.func, {
            i32_const(8); call(self.emitter.rt.alloc); local_set(result_ptr);
            local_get(result_ptr); i32_const(0); i32_store(0);
            local_get(result_ptr); local_get(str_ptr); i32_store(4);
            local_get(result_ptr);
            end;
        });

        self.scratch.free_i32(errno);
        self.scratch.free_i32(str_ptr);
        self.scratch.free_i32(result_ptr);
        self.scratch.free_i32(nread_ptr);
        self.scratch.free_i32(iov_ptr);
        self.scratch.free_i32(data_buf);
        self.scratch.free_i32(file_size);
        self.scratch.free_i32(stat_buf);
        self.scratch.free_i32(opened_fd);
        self.scratch.free_i32(fd_out_ptr);
        self.scratch.free_i32(path_len);
        self.scratch.free_i32(path_ptr);
        self.scratch.free_i32(path_str);
    }

    /// io module: print, read_line, read_all
    fn emit_io_call(&mut self, func: &str, args: &[IrExpr]) {
        match func {
            "print" => {
                // io.print(s: String) -> Unit
                // Same as println but WITHOUT the trailing newline.
                let s = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(s);
                    // iov[0].buf = s + 4 (skip length prefix)
                    i32_const(0);
                    local_get(s); i32_const(4); i32_add;
                    i32_store(0);
                    // iov[0].len = *s (load length)
                    i32_const(4);
                    local_get(s); i32_load(0);
                    i32_store(0);
                    // fd_write(stdout=1, iovs=0, iovs_len=1, nwritten=8)
                    i32_const(1); i32_const(0); i32_const(1); i32_const(8);
                    call(self.emitter.rt.fd_write);
                    drop;
                });
                self.scratch.free_i32(s);
            }
            "read_line" => {
                // io.read_line() -> String
                // Read one byte at a time from stdin (fd=0) until '\n' or EOF.
                // Accumulate into a heap buffer, then build an Almide string.
                let buf = self.scratch.alloc_i32();       // growing buffer ptr
                let capacity = self.scratch.alloc_i32();  // current capacity
                let len = self.scratch.alloc_i32();       // bytes read so far
                let iov_ptr = self.scratch.alloc_i32();   // iov struct for fd_read
                let nread_ptr = self.scratch.alloc_i32(); // nread output
                let byte_buf = self.scratch.alloc_i32();  // 1-byte read target
                let nread_val = self.scratch.alloc_i32(); // loaded nread value
                let byte_val = self.scratch.alloc_i32();  // loaded byte value
                let new_buf = self.scratch.alloc_i32();   // for realloc copy
                let copy_i = self.scratch.alloc_i32();    // copy loop counter
                let result = self.scratch.alloc_i32();    // final string ptr

                // Initial capacity = 256
                wasm!(self.func, {
                    i32_const(256); call(self.emitter.rt.alloc); local_set(buf);
                    i32_const(256); local_set(capacity);
                    i32_const(0); local_set(len);
                    // Allocate iov (8 bytes) and nread (4 bytes) and byte_buf (1 byte)
                    i32_const(8); call(self.emitter.rt.alloc); local_set(iov_ptr);
                    i32_const(4); call(self.emitter.rt.alloc); local_set(nread_ptr);
                    i32_const(1); call(self.emitter.rt.alloc); local_set(byte_buf);
                });

                // Main read loop
                wasm!(self.func, {
                    block_empty; loop_empty;
                });

                // Grow buffer if full: len >= capacity
                wasm!(self.func, {
                    local_get(len); local_get(capacity); i32_ge_u;
                    if_empty;
                      // Double capacity
                      local_get(capacity); i32_const(2); i32_mul; local_set(capacity);
                      local_get(capacity); call(self.emitter.rt.alloc); local_set(new_buf);
                      // Copy old data
                      i32_const(0); local_set(copy_i);
                      block_empty; loop_empty;
                        local_get(copy_i); local_get(len); i32_ge_u; br_if(1);
                        local_get(new_buf); local_get(copy_i); i32_add;
                        local_get(buf); local_get(copy_i); i32_add; i32_load8_u(0);
                        i32_store8(0);
                        local_get(copy_i); i32_const(1); i32_add; local_set(copy_i);
                        br(0);
                      end; end;
                      local_get(new_buf); local_set(buf);
                    end;
                });

                // Set up iov to read 1 byte into byte_buf
                wasm!(self.func, {
                    local_get(iov_ptr); local_get(byte_buf); i32_store(0);
                    local_get(iov_ptr); i32_const(1); i32_store(4);
                    // fd_read(stdin=0, iov_ptr, 1, nread_ptr)
                    i32_const(0);
                    local_get(iov_ptr);
                    i32_const(1);
                    local_get(nread_ptr);
                    call(self.emitter.rt.fd_read);
                    drop;
                });

                // Check nread: if 0, EOF → break
                wasm!(self.func, {
                    local_get(nread_ptr); i32_load(0); local_set(nread_val);
                    local_get(nread_val); i32_eqz;
                    br_if(1); // break outer block
                });

                // Load byte, check for '\n'
                wasm!(self.func, {
                    local_get(byte_buf); i32_load8_u(0); local_set(byte_val);
                    local_get(byte_val); i32_const(10); i32_eq; // '\n'
                    br_if(1); // break outer block (don't include '\n' in result)
                });

                // Append byte to buffer
                wasm!(self.func, {
                    local_get(buf); local_get(len); i32_add;
                    local_get(byte_val);
                    i32_store8(0);
                    local_get(len); i32_const(1); i32_add; local_set(len);
                    br(0); // continue loop
                    end; end; // end loop, end block
                });

                // Build Almide string [len:i32][data:u8...]
                wasm!(self.func, {
                    local_get(len); i32_const(4); i32_add;
                    call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); local_get(len); i32_store(0);
                    // Copy buf[0..len] to result+4
                    i32_const(0); local_set(copy_i);
                    block_empty; loop_empty;
                      local_get(copy_i); local_get(len); i32_ge_u; br_if(1);
                      local_get(result); i32_const(4); i32_add; local_get(copy_i); i32_add;
                      local_get(buf); local_get(copy_i); i32_add; i32_load8_u(0);
                      i32_store8(0);
                      local_get(copy_i); i32_const(1); i32_add; local_set(copy_i);
                      br(0);
                    end; end;
                    local_get(result);
                });

                self.scratch.free_i32(result);
                self.scratch.free_i32(copy_i);
                self.scratch.free_i32(new_buf);
                self.scratch.free_i32(byte_val);
                self.scratch.free_i32(nread_val);
                self.scratch.free_i32(byte_buf);
                self.scratch.free_i32(nread_ptr);
                self.scratch.free_i32(iov_ptr);
                self.scratch.free_i32(len);
                self.scratch.free_i32(capacity);
                self.scratch.free_i32(buf);
            }
            "read_all" => {
                // io.read_all() -> String
                // Read all bytes from stdin (fd=0) until EOF.
                // Strategy: read in chunks of 4096 bytes, grow buffer as needed.
                let buf = self.scratch.alloc_i32();
                let capacity = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let iov_ptr = self.scratch.alloc_i32();
                let nread_ptr = self.scratch.alloc_i32();
                let nread_val = self.scratch.alloc_i32();
                let new_buf = self.scratch.alloc_i32();
                let copy_i = self.scratch.alloc_i32();
                let chunk_buf = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();

                // Initial capacity = 4096
                wasm!(self.func, {
                    i32_const(4096); call(self.emitter.rt.alloc); local_set(buf);
                    i32_const(4096); local_set(capacity);
                    i32_const(0); local_set(len);
                    i32_const(8); call(self.emitter.rt.alloc); local_set(iov_ptr);
                    i32_const(4); call(self.emitter.rt.alloc); local_set(nread_ptr);
                });

                // Read loop
                wasm!(self.func, {
                    block_empty; loop_empty;
                });

                // Ensure we have room for at least 4096 bytes
                wasm!(self.func, {
                    local_get(capacity); local_get(len); i32_sub;
                    i32_const(4096); i32_lt_u;
                    if_empty;
                      // Double capacity
                      local_get(capacity); i32_const(2); i32_mul; local_set(capacity);
                      local_get(capacity); call(self.emitter.rt.alloc); local_set(new_buf);
                      // Copy old data
                      i32_const(0); local_set(copy_i);
                      block_empty; loop_empty;
                        local_get(copy_i); local_get(len); i32_ge_u; br_if(1);
                        local_get(new_buf); local_get(copy_i); i32_add;
                        local_get(buf); local_get(copy_i); i32_add; i32_load8_u(0);
                        i32_store8(0);
                        local_get(copy_i); i32_const(1); i32_add; local_set(copy_i);
                        br(0);
                      end; end;
                      local_get(new_buf); local_set(buf);
                    end;
                });

                // Read chunk into buf+len, up to (capacity - len) bytes
                wasm!(self.func, {
                    local_get(iov_ptr); local_get(buf); local_get(len); i32_add; i32_store(0);
                    local_get(iov_ptr); local_get(capacity); local_get(len); i32_sub; i32_store(4);
                    // fd_read(stdin=0, iov_ptr, 1, nread_ptr)
                    i32_const(0);
                    local_get(iov_ptr);
                    i32_const(1);
                    local_get(nread_ptr);
                    call(self.emitter.rt.fd_read);
                    drop;
                });

                // Check nread: if 0, EOF → break
                wasm!(self.func, {
                    local_get(nread_ptr); i32_load(0); local_set(nread_val);
                    local_get(nread_val); i32_eqz;
                    br_if(1);
                    // Advance len
                    local_get(len); local_get(nread_val); i32_add; local_set(len);
                    br(0);
                    end; end; // end loop, end block
                });

                // Build Almide string [len:i32][data:u8...]
                wasm!(self.func, {
                    local_get(len); i32_const(4); i32_add;
                    call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); local_get(len); i32_store(0);
                    // Copy buf[0..len] to result+4
                    i32_const(0); local_set(copy_i);
                    block_empty; loop_empty;
                      local_get(copy_i); local_get(len); i32_ge_u; br_if(1);
                      local_get(result); i32_const(4); i32_add; local_get(copy_i); i32_add;
                      local_get(buf); local_get(copy_i); i32_add; i32_load8_u(0);
                      i32_store8(0);
                      local_get(copy_i); i32_const(1); i32_add; local_set(copy_i);
                      br(0);
                    end; end;
                    local_get(result);
                });

                self.scratch.free_i32(result);
                self.scratch.free_i32(chunk_buf);
                self.scratch.free_i32(copy_i);
                self.scratch.free_i32(new_buf);
                self.scratch.free_i32(nread_val);
                self.scratch.free_i32(nread_ptr);
                self.scratch.free_i32(iov_ptr);
                self.scratch.free_i32(len);
                self.scratch.free_i32(capacity);
                self.scratch.free_i32(buf);
            }
            "write" | "write_bytes" => {
                // io.write(data: Bytes) — layout [len:i32][u8 data...], same as print
                // io.write_bytes(data: List[Int]) — layout [len:i32][i64 elements...]
                if func == "write" {
                    let s = self.scratch.alloc_i32();
                    self.emit_expr(&args[0]);
                    wasm!(self.func, {
                        local_set(s);
                        i32_const(0);
                        local_get(s); i32_const(4); i32_add;
                        i32_store(0);
                        i32_const(4);
                        local_get(s); i32_load(0);
                        i32_store(0);
                        i32_const(1); i32_const(0); i32_const(1); i32_const(8);
                        call(self.emitter.rt.fd_write);
                        drop;
                    });
                    self.scratch.free_i32(s);
                } else {
                    // write_bytes: List[Int] → convert i64 to u8 then write
                    let list_ptr = self.scratch.alloc_i32();
                    let len = self.scratch.alloc_i32();
                    let tmp_buf = self.scratch.alloc_i32();
                    let i = self.scratch.alloc_i32();
                    self.emit_expr(&args[0]);
                    wasm!(self.func, {
                        local_set(list_ptr);
                        local_get(list_ptr); i32_load(0); local_set(len);
                        local_get(len); call(self.emitter.rt.alloc); local_set(tmp_buf);
                        i32_const(0); local_set(i);
                        block_empty; loop_empty;
                          local_get(i); local_get(len); i32_ge_u; br_if(1);
                          local_get(tmp_buf); local_get(i); i32_add;
                          local_get(list_ptr); i32_const(4); i32_add;
                          local_get(i); i32_const(8); i32_mul; i32_add;
                          i64_load(0); i32_wrap_i64;
                          i32_store8(0);
                          local_get(i); i32_const(1); i32_add; local_set(i);
                          br(0);
                        end; end;
                        i32_const(0); local_get(tmp_buf); i32_store(0);
                        i32_const(4); local_get(len); i32_store(0);
                        i32_const(1); i32_const(0); i32_const(1); i32_const(8);
                        call(self.emitter.rt.fd_write);
                        drop;
                    });
                    self.scratch.free_i32(i);
                    self.scratch.free_i32(tmp_buf);
                    self.scratch.free_i32(len);
                    self.scratch.free_i32(list_ptr);
                }
            }
            _ => {
                self.emit_stub_call(args);
            }
        }
    }

    /// process module: exit, stdin_lines
    fn emit_process_call(&mut self, func: &str, args: &[IrExpr]) {
        match func {
            "exit" => {
                // process.exit(code: Int) -> Unit
                // Emit code arg (i64), wrap to i32, call proc_exit
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    i32_wrap_i64;
                    call(self.emitter.rt.proc_exit);
                });
            }
            "stdin_lines" => {
                // process.stdin_lines() -> List[String]
                // Strategy: read all stdin, then split by '\n'.
                // 1. Read all stdin into a raw buffer (same logic as io.read_all)
                // 2. Split by '\n', building a list of Almide strings
                let buf = self.scratch.alloc_i32();
                let capacity = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let iov_ptr = self.scratch.alloc_i32();
                let nread_ptr = self.scratch.alloc_i32();
                let nread_val = self.scratch.alloc_i32();
                let new_buf = self.scratch.alloc_i32();
                let copy_i = self.scratch.alloc_i32();
                let scan_i = self.scratch.alloc_i32();
                let line_start = self.scratch.alloc_i32();
                let line_len = self.scratch.alloc_i32();
                let line_ptr = self.scratch.alloc_i32();
                let list_ptr = self.scratch.alloc_i32();
                let list_cap = self.scratch.alloc_i32();
                let list_count = self.scratch.alloc_i32();
                let new_list = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();

                // --- Phase 1: read all stdin ---
                wasm!(self.func, {
                    i32_const(4096); call(self.emitter.rt.alloc); local_set(buf);
                    i32_const(4096); local_set(capacity);
                    i32_const(0); local_set(len);
                    i32_const(8); call(self.emitter.rt.alloc); local_set(iov_ptr);
                    i32_const(4); call(self.emitter.rt.alloc); local_set(nread_ptr);
                });

                wasm!(self.func, {
                    block_empty; loop_empty;
                });

                // Grow if needed
                wasm!(self.func, {
                    local_get(capacity); local_get(len); i32_sub;
                    i32_const(4096); i32_lt_u;
                    if_empty;
                      local_get(capacity); i32_const(2); i32_mul; local_set(capacity);
                      local_get(capacity); call(self.emitter.rt.alloc); local_set(new_buf);
                      i32_const(0); local_set(copy_i);
                      block_empty; loop_empty;
                        local_get(copy_i); local_get(len); i32_ge_u; br_if(1);
                        local_get(new_buf); local_get(copy_i); i32_add;
                        local_get(buf); local_get(copy_i); i32_add; i32_load8_u(0);
                        i32_store8(0);
                        local_get(copy_i); i32_const(1); i32_add; local_set(copy_i);
                        br(0);
                      end; end;
                      local_get(new_buf); local_set(buf);
                    end;
                });

                // Read chunk
                wasm!(self.func, {
                    local_get(iov_ptr); local_get(buf); local_get(len); i32_add; i32_store(0);
                    local_get(iov_ptr); local_get(capacity); local_get(len); i32_sub; i32_store(4);
                    i32_const(0);
                    local_get(iov_ptr);
                    i32_const(1);
                    local_get(nread_ptr);
                    call(self.emitter.rt.fd_read);
                    drop;
                });

                wasm!(self.func, {
                    local_get(nread_ptr); i32_load(0); local_set(nread_val);
                    local_get(nread_val); i32_eqz;
                    br_if(1);
                    local_get(len); local_get(nread_val); i32_add; local_set(len);
                    br(0);
                    end; end;
                });

                // --- Phase 2: split buf[0..len] by '\n' into List[String] ---
                // List layout: [count:i32][elem0:i32][elem1:i32]...
                // Each elem is a ptr to Almide String [len:i32][data:u8...]
                // We'll build with a growable array of i32 pointers.
                wasm!(self.func, {
                    // Initial list capacity: 64 elements (i32 ptrs)
                    i32_const(64); local_set(list_cap);
                    local_get(list_cap); i32_const(4); i32_mul;
                    call(self.emitter.rt.alloc); local_set(list_ptr);
                    i32_const(0); local_set(list_count);
                    i32_const(0); local_set(scan_i);
                    i32_const(0); local_set(line_start);
                });

                // Scan loop: iterate through buf looking for '\n'
                wasm!(self.func, {
                    block_empty; loop_empty;
                      local_get(scan_i); local_get(len); i32_ge_u;
                      br_if(1);
                });

                // Check if buf[scan_i] == '\n'
                wasm!(self.func, {
                      local_get(buf); local_get(scan_i); i32_add; i32_load8_u(0);
                      i32_const(10); i32_eq;
                      if_empty;
                });

                // Found '\n': build string from line_start..scan_i
                wasm!(self.func, {
                        local_get(scan_i); local_get(line_start); i32_sub; local_set(line_len);
                        // Allocate Almide string
                        local_get(line_len); i32_const(4); i32_add;
                        call(self.emitter.rt.alloc); local_set(line_ptr);
                        local_get(line_ptr); local_get(line_len); i32_store(0);
                        // Copy line data
                        i32_const(0); local_set(copy_i);
                        block_empty; loop_empty;
                          local_get(copy_i); local_get(line_len); i32_ge_u; br_if(1);
                          local_get(line_ptr); i32_const(4); i32_add; local_get(copy_i); i32_add;
                          local_get(buf); local_get(line_start); i32_add; local_get(copy_i); i32_add;
                          i32_load8_u(0);
                          i32_store8(0);
                          local_get(copy_i); i32_const(1); i32_add; local_set(copy_i);
                          br(0);
                        end; end;
                });

                // Grow list if needed
                wasm!(self.func, {
                        local_get(list_count); local_get(list_cap); i32_ge_u;
                        if_empty;
                          local_get(list_cap); i32_const(2); i32_mul; local_set(list_cap);
                          local_get(list_cap); i32_const(4); i32_mul;
                          call(self.emitter.rt.alloc); local_set(new_list);
                          // Copy old list ptrs
                          i32_const(0); local_set(copy_i);
                          block_empty; loop_empty;
                            local_get(copy_i); local_get(list_count); i32_ge_u; br_if(1);
                            local_get(new_list); local_get(copy_i); i32_const(4); i32_mul; i32_add;
                            local_get(list_ptr); local_get(copy_i); i32_const(4); i32_mul; i32_add;
                            i32_load(0);
                            i32_store(0);
                            local_get(copy_i); i32_const(1); i32_add; local_set(copy_i);
                            br(0);
                          end; end;
                          local_get(new_list); local_set(list_ptr);
                        end;
                });

                // Append line_ptr to list
                wasm!(self.func, {
                        local_get(list_ptr); local_get(list_count); i32_const(4); i32_mul; i32_add;
                        local_get(line_ptr); i32_store(0);
                        local_get(list_count); i32_const(1); i32_add; local_set(list_count);
                        // line_start = scan_i + 1
                        local_get(scan_i); i32_const(1); i32_add; local_set(line_start);
                      end; // end if '\n'
                });

                // Advance scan_i
                wasm!(self.func, {
                      local_get(scan_i); i32_const(1); i32_add; local_set(scan_i);
                      br(0);
                    end; end; // end loop, end block
                });

                // Handle last line (if no trailing '\n')
                wasm!(self.func, {
                    local_get(line_start); local_get(len); i32_lt_u;
                    if_empty;
                      local_get(len); local_get(line_start); i32_sub; local_set(line_len);
                      local_get(line_len); i32_const(4); i32_add;
                      call(self.emitter.rt.alloc); local_set(line_ptr);
                      local_get(line_ptr); local_get(line_len); i32_store(0);
                      i32_const(0); local_set(copy_i);
                      block_empty; loop_empty;
                        local_get(copy_i); local_get(line_len); i32_ge_u; br_if(1);
                        local_get(line_ptr); i32_const(4); i32_add; local_get(copy_i); i32_add;
                        local_get(buf); local_get(line_start); i32_add; local_get(copy_i); i32_add;
                        i32_load8_u(0);
                        i32_store8(0);
                        local_get(copy_i); i32_const(1); i32_add; local_set(copy_i);
                        br(0);
                      end; end;
                });

                // Grow list if needed for last line
                wasm!(self.func, {
                      local_get(list_count); local_get(list_cap); i32_ge_u;
                      if_empty;
                        local_get(list_cap); i32_const(2); i32_mul; local_set(list_cap);
                        local_get(list_cap); i32_const(4); i32_mul;
                        call(self.emitter.rt.alloc); local_set(new_list);
                        i32_const(0); local_set(copy_i);
                        block_empty; loop_empty;
                          local_get(copy_i); local_get(list_count); i32_ge_u; br_if(1);
                          local_get(new_list); local_get(copy_i); i32_const(4); i32_mul; i32_add;
                          local_get(list_ptr); local_get(copy_i); i32_const(4); i32_mul; i32_add;
                          i32_load(0);
                          i32_store(0);
                          local_get(copy_i); i32_const(1); i32_add; local_set(copy_i);
                          br(0);
                        end; end;
                        local_get(new_list); local_set(list_ptr);
                      end;
                      // Append last line
                      local_get(list_ptr); local_get(list_count); i32_const(4); i32_mul; i32_add;
                      local_get(line_ptr); i32_store(0);
                      local_get(list_count); i32_const(1); i32_add; local_set(list_count);
                    end; // end if line_start < len
                });

                // Build final Almide List: [count:i32][elem0:i32][elem1:i32]...
                // elem_size = 4 (i32 pointer)
                wasm!(self.func, {
                    local_get(list_count); i32_const(4); i32_mul; i32_const(4); i32_add;
                    call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); local_get(list_count); i32_store(0);
                    // Copy list_ptr[0..list_count] to result+4
                    i32_const(0); local_set(copy_i);
                    block_empty; loop_empty;
                      local_get(copy_i); local_get(list_count); i32_ge_u; br_if(1);
                      local_get(result); i32_const(4); i32_add;
                      local_get(copy_i); i32_const(4); i32_mul; i32_add;
                      local_get(list_ptr); local_get(copy_i); i32_const(4); i32_mul; i32_add;
                      i32_load(0);
                      i32_store(0);
                      local_get(copy_i); i32_const(1); i32_add; local_set(copy_i);
                      br(0);
                    end; end;
                    local_get(result);
                });

                self.scratch.free_i32(result);
                self.scratch.free_i32(new_list);
                self.scratch.free_i32(list_count);
                self.scratch.free_i32(list_cap);
                self.scratch.free_i32(list_ptr);
                self.scratch.free_i32(line_ptr);
                self.scratch.free_i32(line_len);
                self.scratch.free_i32(line_start);
                self.scratch.free_i32(scan_i);
                self.scratch.free_i32(copy_i);
                self.scratch.free_i32(new_buf);
                self.scratch.free_i32(nread_val);
                self.scratch.free_i32(nread_ptr);
                self.scratch.free_i32(iov_ptr);
                self.scratch.free_i32(len);
                self.scratch.free_i32(capacity);
                self.scratch.free_i32(buf);
            }
            _ => {
                self.emit_stub_call(args);
            }
        }
    }
}
