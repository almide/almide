//! Function call emission — emit_call and related helpers.

use almide_ir::{CallTarget, IrExpr};
use almide_lang::types::Ty;
use wasm_encoder::ValType;

use super::FuncCompiler;
use super::values;

/// Signed / unsigned / float kind for sized numeric WASM conversion
/// dispatch. See `emit_sized_conv_call`.
#[derive(Copy, Clone, PartialEq, Eq)]
enum SizedKind { Int, UInt, Float }

/// Map a sized numeric `Ty` to its module name (`int32`, `uint8`, ...).
/// Mirrors `resolve_module_from_ty` / `builtin_module_for_type` so the
/// WASM dispatcher can route a `CallTarget::Method` on a sized
/// receiver into the same module dispatch path as `CallTarget::Module`.
fn sized_ty_module(ty: &Ty) -> Option<&'static str> {
    Some(match ty {
        Ty::Int => "int",
        Ty::Float => "float",
        Ty::Int8 => "int8",
        Ty::Int16 => "int16",
        Ty::Int32 => "int32",
        Ty::UInt8 => "uint8",
        Ty::UInt16 => "uint16",
        Ty::UInt32 => "uint32",
        Ty::UInt64 => "uint64",
        Ty::Float32 => "float32",
        _ => return None,
    })
}

/// Parse a sized-type module name into (kind, bit-width). Accepts the
/// canonical `int` / `float` (treated as 64-bit) plus every sized
/// variant. Returns `None` for anything else so the dispatcher falls
/// through to legacy TOML / bundled routing.
fn sized_type_info(name: &str) -> Option<(SizedKind, u32)> {
    Some(match name {
        "int" | "int64" => (SizedKind::Int, 64),
        "int32" => (SizedKind::Int, 32),
        "int16" => (SizedKind::Int, 16),
        "int8" => (SizedKind::Int, 8),
        "uint64" => (SizedKind::UInt, 64),
        "uint32" => (SizedKind::UInt, 32),
        "uint16" => (SizedKind::UInt, 16),
        "uint8" => (SizedKind::UInt, 8),
        "float" | "float64" => (SizedKind::Float, 64),
        "float32" => (SizedKind::Float, 32),
        _ => return None,
    })
}
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
                // Sized numeric conversion modules (`int8`, ..., `uint64`,
                // `float32`) live in bundled `.almd` + `@inline_rust` only.
                // On WASM they surface as `CallTarget::Module` here and get
                // lowered to the matching WASM conversion instruction
                // (`i64.extend_i32_s`, `f32.convert_i64_u`, ...) by
                // `emit_sized_conv_call`. The canonical `int` / `float`
                // modules also host `.to_intN()` / `.to_floatN()` methods
                // via the same path, so we consult this dispatcher before
                // their TOML-driven `emit_int_call` / `emit_float_call`.
                if matches!(
                    module.as_str(),
                    "int" | "float" | "int8" | "int16" | "int32"
                        | "uint8" | "uint16" | "uint32" | "uint64" | "float32"
                ) {
                    if self.emit_sized_conv_call(module.as_str(), func.as_str(), args) {
                        return;
                    }
                }
                match (module.as_str(), func.as_str()) {
                    _ if module == "int" => {
                        if !self.emit_int_call(func, args) {
                            self.emit_stub_call_named(module.as_str(), func.as_str(), args);
                        }
                    }
                    _ if module == "float" => {
                        if !self.emit_float_call(func, args) {
                            self.emit_stub_call_named(module.as_str(), func.as_str(), args);
                        }
                    }
                    _ if module == "string" => {
                        if !self.emit_string_call(func, args) {
                            self.emit_stub_call_named(module.as_str(), func.as_str(), args);
                        }
                    }
                    _ if module == "option" => {
                        if !self.emit_option_call(func, args) {
                            self.emit_stub_call_named(module.as_str(), func.as_str(), args);
                        }
                    }
                    _ if module == "result" => {
                        if !self.emit_result_call(func, args) {
                            self.emit_stub_call_named(module.as_str(), func.as_str(), args);
                        }
                    }
                    _ if module == "list" => {
                        if !self.emit_list_call(func, args) {
                            // Bundled-Almide fns inside list (e.g. list.split_at,
                            // list.iterate from stdlib/list.almd) are rewritten to
                            // CallTarget::Named { almide_rt_list_<f> } by
                            // pass_resolve_calls — they never reach this Module arm.
                            // Anything that gets here is a TOML stdlib fn whose
                            // dispatch is missing in emit_list_call. Hard ICE so the
                            // gap is fixed at the source, not papered over.
                            self.emit_stub_call_named(module.as_str(), func.as_str(), args);
                        }
                    }
                    _ if module == "bytes" => {
                        if !self.emit_bytes_call(func, args) {
                            self.emit_stub_call_named(module.as_str(), func.as_str(), args);
                        }
                    }
                    _ if module == "matrix" => {
                        if !self.emit_matrix_call(func, args) {
                            self.emit_stub_call_named(module.as_str(), func.as_str(), args);
                        }
                    }
                    _ if module == "base64" => {
                        let rt_fn = match func.as_str() {
                            "encode" => Some(self.emitter.rt.base64_encode),
                            "decode" => Some(self.emitter.rt.base64_decode),
                            "encode_url" => Some(self.emitter.rt.base64_encode_url),
                            "decode_url" => Some(self.emitter.rt.base64_decode_url),
                            _ => None,
                        };
                        if let Some(rt) = rt_fn {
                            self.emit_expr(&args[0]);
                            wasm!(self.func, { call(rt); });
                        } else {
                            self.emit_stub_call_named(module.as_str(), func.as_str(), args);
                        }
                    }
                    _ if module == "hex" => {
                        let rt_fn = match func.as_str() {
                            "encode" => Some(self.emitter.rt.hex_encode),
                            "encode_upper" => Some(self.emitter.rt.hex_encode_upper),
                            "decode" => Some(self.emitter.rt.hex_decode),
                            _ => None,
                        };
                        if let Some(rt) = rt_fn {
                            self.emit_expr(&args[0]);
                            wasm!(self.func, { call(rt); });
                        } else {
                            self.emit_stub_call_named(module.as_str(), func.as_str(), args);
                        }
                    }
                    _ if module == "map" => {
                        if !self.emit_map_call(func, args) {
                            self.emit_stub_call_named(module.as_str(), func.as_str(), args);
                        }
                    }
                    _ if module == "math" => {
                        if !self.emit_math_call(func, args) {
                            self.emit_stub_call_named(module.as_str(), func.as_str(), args);
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
                            self.emit_stub_call_named(module.as_str(), func.as_str(), args);
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
                                // Last resort: bare func name (for cross-module calls where
                                // module name differs from canonical)
                                if let Some(&func_idx) = self.emitter.func_map.get(func.as_str()) {
                                    for arg in args { self.emit_expr(arg); }
                                    wasm!(self.func, { call(func_idx); });
                                } else {
                                    self.emit_stub_call_named(module.as_str(), func.as_str(), args);
                                }
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
                    // Sized numeric UFCS conversion on `Ty::Int` / `Ty::Float`
                    // (e.g. `n.to_int32()` where `n: Int`). Must precede the
                    // generic `Ty::Int` / `Ty::Float` dispatch below because
                    // those arms route through `emit_int_call` /
                    // `emit_float_call` which don't know about sized
                    // conversion templates.
                    _ if matches!(&object.ty, Ty::Int | Ty::Float)
                        && {
                            let m = method.split('.').last().unwrap_or(method);
                            m.starts_with("to_int") || m.starts_with("to_uint") || m.starts_with("to_float")
                        }
                        => {
                        let src_module = sized_ty_module(&object.ty).unwrap();
                        let bare_method = method.split('.').last().unwrap_or(method);
                        let fake_args = vec![(**object).clone()];
                        let target = CallTarget::Module {
                            module: almide_base::intern::sym(src_module),
                            func: almide_base::intern::sym(bare_method),
                        };
                        self.emit_call(&target, &fake_args, _ret_ty);
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
                    _ if sized_ty_module(&object.ty).is_some()
                        && {
                            let m = method.split('.').last().unwrap_or(method);
                            m.starts_with("to_int") || m.starts_with("to_uint") || m.starts_with("to_float")
                        }
                        => {
                        // Sized numeric UFCS conversion (Stage 3 of the
                        // sized-numeric-types arc). Route through the
                        // Module dispatcher so `emit_sized_conv_call`
                        // picks the right WASM conversion instruction.
                        let src_module = sized_ty_module(&object.ty).unwrap();
                        let bare_method = method.split('.').last().unwrap_or(method);
                        let fake_args = vec![(**object).clone()];
                        let target = CallTarget::Module {
                            module: almide_base::intern::sym(src_module),
                            func: almide_base::intern::sym(bare_method),
                        };
                        self.emit_call(&target, &fake_args, _ret_ty);
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

    /// Emit a tail call (WASM `return_call` / `return_call_indirect`).
    /// Falls back to normal call for targets that don't resolve to a known function.
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
                    wasm!(self.func, { unreachable; });
                }
                self.scratch.free_i32(scratch);
            }
            // Module/Method calls in tail position — fall back to normal call
            _ => {
                self.emit_call(target, args, ret_ty);
            }
        }
    }

    /// S3 (v0.14.7-phase3.2): the WASM dispatcher used to route unknown
    /// stdlib calls to `emit_stub_call`, which deferred the failure to a
    /// runtime `unreachable` trap. spec/ + nn (v0.14.6 stub-panic sweep)
    /// proved every reachable code path resolves before reaching here, so
    /// reaching the stub now is a compile-time ICE — there is no runtime
    /// trap to debug. If you hit this panic, it means a `module.func` call
    /// survived `pass_resolve_calls` without a TOML or bundled IR target;
    /// add the missing dispatch arm or fix the resolver, do not relax this
    /// panic.
    pub(super) fn emit_stub_call_named(&mut self, module: &str, func: &str, _args: &[IrExpr]) -> ! {
        panic!(
            "[ICE] WASM emit reached emit_stub_call_named for `{}.{}`. \
             No runtime stub remains; resolve the call (TOML / bundled IR) \
             or add a dispatch arm in emit_wasm/calls_*.rs.",
            module, func
        );
    }

    pub(super) fn emit_stub_call(&mut self, _args: &[IrExpr]) -> ! {
        panic!(
            "[ICE] WASM emit reached emit_stub_call (no module/func context). \
             A stdlib dispatcher returned false without going through \
             emit_stub_call_named — fix the caller."
        );
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

    /// Stage 3 of the sized-numeric-types arc: emit WASM conversion
    /// instructions for the `to_intN` / `to_uintN` / `to_floatN`
    /// UFCS methods on sized numeric modules. Returns `true` if the
    /// (module, func) pair was handled.
    ///
    /// The scheme mirrors Rust's `as` semantics (wrapping on narrow
    /// int downcasts, saturating trunc on float→int). WASM-native
    /// opcodes cover every combination: `i32.wrap_i64`,
    /// `i64.extend_i32_s/_u`, `f32.convert_i64_s/_u`,
    /// `i32.trunc_sat_f64_s`, `f32.demote_f64`, `f64.promote_f32`,
    /// and no-ops for same-width / same-kind conversions.
    fn emit_sized_conv_call(&mut self, module: &str, func: &str, args: &[IrExpr]) -> bool {
        use wasm_encoder::Instruction;
        // All conversion fns take one positional arg (the source value).
        if args.len() != 1 { return false; }
        // Determine source / destination kind+width purely from names so
        // this dispatcher is closed over the entire sized-type matrix
        // regardless of which .almd module hosts the fn.
        let src = sized_type_info(module);
        let dst = func.strip_prefix("to_").and_then(sized_type_info);
        let (Some((src_kind, src_bits)), Some((dst_kind, dst_bits))) = (src, dst) else {
            return false;
        };
        // to_string is NOT handled here; its @inline_rust uses format!()
        // which is Rust-only. On WASM we already have string dispatch via
        // the respective int/float module, so fall through.
        if func == "to_string" { return false; }

        self.emit_expr(&args[0]);

        let src_u = matches!(src_kind, SizedKind::UInt);
        match (src_kind, dst_kind) {
            (SizedKind::Int | SizedKind::UInt, SizedKind::Int | SizedKind::UInt) => {
                // Integer → integer. WASM valtype buckets: narrow
                // (<=32 bits) → i32; 64-bit → i64. Sign behavior at
                // extend vs wrap:
                //   src i32 bucket → dst i64 bucket: i64.extend_i32_s/_u
                //   src i64 bucket → dst i32 bucket: i32.wrap_i64
                //   same bucket: no-op for width ≥ dst; mask-to-width
                //     for narrow dst so `256 as u8 == 0` matches Rust.
                //
                // Masking is applied on narrowing INTO any sized int
                // less than 32 bits so the representation in the i32
                // bucket is canonical (zero-extended for UInt, sign-
                // extended for Int via extend_8_s / extend_16_s).
                // This keeps subsequent `i64.extend_i32_{s,u}` correct.
                let src_64 = src_bits == 64;
                let dst_64 = dst_bits == 64;
                if src_64 && !dst_64 {
                    self.func.instruction(&Instruction::I32WrapI64);
                } else if !src_64 && dst_64 {
                    if src_u {
                        self.func.instruction(&Instruction::I64ExtendI32U);
                    } else {
                        self.func.instruction(&Instruction::I64ExtendI32S);
                    }
                    // narrowing happens below when dst is <= 32 bits;
                    // extend is only for reaching the i64 bucket.
                }
                // Normalize the narrow representation: UInt* zero-pads,
                // Int* sign-extends from the stored width.
                if dst_bits < 32 {
                    let dst_u = matches!(dst_kind, SizedKind::UInt);
                    if dst_u {
                        let mask = ((1u64 << dst_bits) - 1) as i32;
                        self.func.instruction(&Instruction::I32Const(mask));
                        self.func.instruction(&Instruction::I32And);
                    } else {
                        let instr = if dst_bits == 8 { Instruction::I32Extend8S }
                                    else { Instruction::I32Extend16S };
                        self.func.instruction(&instr);
                    }
                }
            }
            (SizedKind::Int | SizedKind::UInt, SizedKind::Float) => {
                let dst_f32 = dst_bits == 32;
                let src_64 = src_bits == 64;
                let instr = match (dst_f32, src_64, src_u) {
                    (true, true, true) => Instruction::F32ConvertI64U,
                    (true, true, false) => Instruction::F32ConvertI64S,
                    (true, false, true) => Instruction::F32ConvertI32U,
                    (true, false, false) => Instruction::F32ConvertI32S,
                    (false, true, true) => Instruction::F64ConvertI64U,
                    (false, true, false) => Instruction::F64ConvertI64S,
                    (false, false, true) => Instruction::F64ConvertI32U,
                    (false, false, false) => Instruction::F64ConvertI32S,
                };
                self.func.instruction(&instr);
            }
            (SizedKind::Float, SizedKind::Int | SizedKind::UInt) => {
                // Float → int. `_sat_` variants mirror Rust's `as`
                // semantics: NaN → 0, overflow saturates to the
                // target's min/max. The signed/unsigned variant is
                // picked from the DESTINATION kind because that's what
                // determines the integer encoding.
                let src_f32 = src_bits == 32;
                let dst_64 = dst_bits == 64;
                let dst_u = matches!(dst_kind, SizedKind::UInt);
                let instr = match (dst_64, src_f32, dst_u) {
                    (true, true, true) => Instruction::I64TruncSatF32U,
                    (true, true, false) => Instruction::I64TruncSatF32S,
                    (true, false, true) => Instruction::I64TruncSatF64U,
                    (true, false, false) => Instruction::I64TruncSatF64S,
                    (false, true, true) => Instruction::I32TruncSatF32U,
                    (false, true, false) => Instruction::I32TruncSatF32S,
                    (false, false, true) => Instruction::I32TruncSatF64U,
                    (false, false, false) => Instruction::I32TruncSatF64S,
                };
                self.func.instruction(&instr);
            }
            (SizedKind::Float, SizedKind::Float) => {
                if src_bits == 64 && dst_bits == 32 {
                    self.func.instruction(&Instruction::F32DemoteF64);
                } else if src_bits == 32 && dst_bits == 64 {
                    self.func.instruction(&Instruction::F64PromoteF32);
                }
                // Same-width float: no-op.
            }
        }
        true
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
                let elem_ty = self.resolve_list_elem(&args[0], None);
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

}

