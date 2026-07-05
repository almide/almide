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
                        // Codec helper functions — MUST run before the dotted-name
                        // split below: a module-qualified element type makes the
                        // helper name `__decode_list_varlib.Pigment`, and splitting
                        // on the dot inside `varlib.Pigment` would mis-route it to a
                        // bogus Module call and panic (#609). The `__`-prefix is
                        // unambiguous, so route to the codec helper first.
                        if name.starts_with("__encode_option_") || name.starts_with("__decode_option_") || name.starts_with("__decode_default_") || name.starts_with("__encode_list_") || name.starts_with("__decode_list_") {
                            self.emit_codec_helper(name, args);
                            return;
                        }
                        // Module-qualified call: list.fold, map.set, etc.
                        if let Some(dot) = name.find('.') {
                            let module = &name[..dot];
                            let func = &name[dot+1..];
                            let target = CallTarget::Module { module: almide_base::intern::sym(module), func: almide_base::intern::sym(func), def_id: None };
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
                                // Write args — through the stored-field
                                // contract (fresh values move, alias values
                                // dup), the same rule emit_record uses; a
                                // bare emit_expr stored a payload the source
                                // binding still owned and later Dec'd.
                                let mut offset = 4u32;
                                for arg in args {
                                    wasm!(self.func, { local_get(scratch); });
                                    self.emit_stored_field(arg);
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
                        // Resolve: prefer current module's qualified name, then bare,
                        // then try all qualified variants
                        let func_idx = self.current_module_name.as_ref()
                            .and_then(|m| {
                                let qn = format!("{}.{}", m, name.as_str());
                                self.emitter.func_map.get(qn.as_str()).copied()
                            })
                            .or_else(|| self.emitter.func_map.get(name.as_str()).copied())
                            .or_else(|| {
                                // Try qualified: "{module}.{name}" for each known module
                                self.emitter.module_names.iter()
                                    .find_map(|m| {
                                        let qn = format!("{}.{}", m, name.as_str());
                                        self.emitter.func_map.get(qn.as_str()).copied()
                                    })
                            });
                        if let Some(idx) = func_idx {
                            wasm!(self.func, { call(idx); });
                        } else {
                            panic!(
                                "[ICE] emit_wasm: call target `{}` not in func_map \
                                 (tried bare and module-qualified) — resolve upstream",
                                name.as_str()
                            );
                        }
                    }
                }
            }

            CallTarget::Module { module, func, .. } => {
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
                        if !self.emit_int_call(func, args)
                            && !self.try_named_dispatch_fallback(module.as_str(), func.as_str(), args)
                        {
                            panic!(
                                "[ICE] emit_wasm: no WASM dispatch for `{}.{}` — \
                                 add an arm in emit_{}_call or resolve upstream",
                                module.as_str(), func.as_str(), module.as_str()
                            );
                        }
                    }
                    _ if module == "float" => {
                        if !self.emit_float_call(func, args)
                            && !self.try_named_dispatch_fallback(module.as_str(), func.as_str(), args)
                        {
                            panic!(
                                "[ICE] emit_wasm: no WASM dispatch for `{}.{}` — \
                                 add an arm in emit_{}_call or resolve upstream",
                                module.as_str(), func.as_str(), module.as_str()
                            );
                        }
                    }
                    _ if module == "string" => {
                        if !self.emit_string_call(func, args)
                            && !self.try_named_dispatch_fallback(module.as_str(), func.as_str(), args)
                        {
                            panic!(
                                "[ICE] emit_wasm: no WASM dispatch for `{}.{}` — \
                                 add an arm in emit_{}_call or resolve upstream",
                                module.as_str(), func.as_str(), module.as_str()
                            );
                        }
                    }
                    _ if module == "option" => {
                        if !self.emit_option_call(func, args)
                            && !self.try_named_dispatch_fallback(module.as_str(), func.as_str(), args)
                        {
                            panic!(
                                "[ICE] emit_wasm: no WASM dispatch for `{}.{}` — \
                                 add an arm in emit_{}_call or resolve upstream",
                                module.as_str(), func.as_str(), module.as_str()
                            );
                        }
                    }
                    _ if module == "result" => {
                        if !self.emit_result_call(func, args)
                            && !self.try_named_dispatch_fallback(module.as_str(), func.as_str(), args)
                        {
                            panic!(
                                "[ICE] emit_wasm: no WASM dispatch for `{}.{}` — \
                                 add an arm in emit_{}_call or resolve upstream",
                                module.as_str(), func.as_str(), module.as_str()
                            );
                        }
                    }
                    _ if module == "list" => {
                        if !self.emit_list_call(func, args)
                            && !self.try_named_dispatch_fallback(module.as_str(), func.as_str(), args)
                        {
                            // Bundled-Almide fns inside list (e.g. list.split_at,
                            // list.iterate from stdlib/list.almd) are rewritten to
                            // CallTarget::Named { almide_rt_list_<f> } by
                            // pass_resolve_calls — they never reach this Module arm.
                            // Anything that gets here is a TOML stdlib fn whose
                            // dispatch is missing in emit_list_call. Hard ICE so the
                            // gap is fixed at the source, not papered over.
                            panic!(
                                "[ICE] emit_wasm: no WASM dispatch for `{}.{}` — \
                                 add an arm in emit_{}_call or resolve upstream",
                                module.as_str(), func.as_str(), module.as_str()
                            );
                        }
                    }
                    _ if module == "bytes" => {
                        if !self.emit_bytes_call(func, args)
                            && !self.try_named_dispatch_fallback(module.as_str(), func.as_str(), args)
                        {
                            panic!(
                                "[ICE] emit_wasm: no WASM dispatch for `{}.{}` — \
                                 add an arm in emit_{}_call or resolve upstream",
                                module.as_str(), func.as_str(), module.as_str()
                            );
                        }
                    }
                    _ if module == "matrix" => {
                        if !self.emit_matrix_call(func, args)
                            && !self.try_named_dispatch_fallback(module.as_str(), func.as_str(), args)
                        {
                            panic!(
                                "[ICE] emit_wasm: no WASM dispatch for `{}.{}` — \
                                 add an arm in emit_{}_call or resolve upstream",
                                module.as_str(), func.as_str(), module.as_str()
                            );
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
                        } else if !self.try_named_dispatch_fallback(module.as_str(), func.as_str(), args) {
                            panic!(
                                "[ICE] emit_wasm: no WASM dispatch for `{}.{}` — \
                                 add an arm in emit_{}_call or resolve upstream",
                                module.as_str(), func.as_str(), module.as_str()
                            );
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
                        } else if !self.try_named_dispatch_fallback(module.as_str(), func.as_str(), args) {
                            panic!(
                                "[ICE] emit_wasm: no WASM dispatch for `{}.{}` — \
                                 add an arm in emit_{}_call or resolve upstream",
                                module.as_str(), func.as_str(), module.as_str()
                            );
                        }
                    }
                    _ if module == "map" => {
                        if !self.emit_map_call(func, args)
                            && !self.try_named_dispatch_fallback(module.as_str(), func.as_str(), args)
                        {
                            panic!(
                                "[ICE] emit_wasm: no WASM dispatch for `{}.{}` — \
                                 add an arm in emit_{}_call or resolve upstream",
                                module.as_str(), func.as_str(), module.as_str()
                            );
                        }
                    }
                    _ if module == "math" => {
                        if !self.emit_math_call(func, args)
                            && !self.try_named_dispatch_fallback(module.as_str(), func.as_str(), args)
                        {
                            panic!(
                                "[ICE] emit_wasm: no WASM dispatch for `{}.{}` — \
                                 add an arm in emit_{}_call or resolve upstream",
                                module.as_str(), func.as_str(), module.as_str()
                            );
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
                              i32_const(0); call(self.emitter.rt.string_alloc); local_set(s1);
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
                        // err: wrap the message with context. ok: COPY the payload into a
                        // FRESH Result — returning args[0]'s pointer aliases a box the caller
                        // still Decs; Perceus then double-frees the same ok pointer and a
                        // later ??/match reads garbage (#591). A heap ok payload is rc_inc'd.
                        let ok_ty = if let Ty::Applied(_, a) = &args[0].ty {
                            a.first().cloned().unwrap_or(Ty::Int)
                        } else { Ty::Int };
                        let ok_has_val = values::ty_to_valtype(&ok_ty).is_some();
                        let ok_is_heap = crate::pass_perceus::is_heap_type(&ok_ty);
                        let ok_alloc = 4 + values::byte_size(&ok_ty) as i32;
                        let s = self.scratch.alloc_i32();
                        let s1 = self.scratch.alloc_i32();
                        let s2 = self.scratch.alloc_i32();
                        let s3 = self.scratch.alloc_i32();
                        let nw = self.scratch.alloc_i32();
                        self.emit_expr(&args[0]);
                        wasm!(self.func, {
                            local_set(s);
                            local_get(s); i32_load(0); i32_eqz; // tag == 0 (ok)?
                            if_i32;
                              // ok: fresh [tag=0][payload] copy — never alias args[0]
                              i32_const(ok_alloc); call(self.emitter.rt.alloc); local_set(nw);
                              local_get(nw); i32_const(0); i32_store(0);
                        });
                        if ok_has_val {
                            wasm!(self.func, { local_get(nw); local_get(s); });
                            self.emit_load_at(&ok_ty, 4);
                            if ok_is_heap {
                                wasm!(self.func, { call(self.emitter.rt.rc_inc); });
                            }
                            self.emit_store_at(&ok_ty, 4);
                        }
                        wasm!(self.func, {
                              local_get(nw); // return the fresh ok copy
                            else_;
                              // Build new err with context: "msg: original_err"
                              local_get(s); i32_load(4); local_set(s1); // original err string
                        });
                        self.emit_expr(&args[1]); // context msg
                        wasm!(self.func, {
                              local_set(s2);
                              // Build ": " separator [len=2][cap=2][':'][' ']
                              i32_const(2); call(self.emitter.rt.string_alloc); local_set(s3);
                              local_get(s3); i32_const(2); i32_store(0);
                              local_get(s3); i32_const(2); i32_store(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::CAP) as i32 as u32, 0);
                              local_get(s3); i32_const(58); i32_store8(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32 as u32);
                              local_get(s3); i32_const(32); i32_store8(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32 as u32 + 1);
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
                        self.scratch.free_i32(nw);
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
                            i32_const(2); call(self.emitter.rt.string_alloc); local_set(s1);
                            local_get(s1); i32_const(2); i32_store(0);
                            local_get(s1); i32_const(2); i32_store(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::CAP) as i32 as u32, 0);
                            local_get(s1); i32_const(58); i32_store8(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32 as u32); // ':'
                            local_get(s1); i32_const(32); i32_store8(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32 as u32 + 1); // ' '
                            local_get(s); local_get(s1); call(self.emitter.rt.concat_str);
                        });
                        self.emit_expr(&args[1]);
                        wasm!(self.func, { call(self.emitter.rt.concat_str); });
                        self.scratch.free_i32(s1);
                        self.scratch.free_i32(s);
                    }
                    _ if module == "set" => {
                        if !self.emit_set_call(func, args)
                            && !self.try_named_dispatch_fallback(module.as_str(), func.as_str(), args)
                        {
                            panic!(
                                "[ICE] emit_wasm: no WASM dispatch for `{}.{}` — \
                                 add an arm in emit_{}_call or resolve upstream",
                                module.as_str(), func.as_str(), module.as_str()
                            );
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
                    _ if module == "mem" => {
                        match func.as_str() {
                            "save" => { wasm!(self.func, { call(self.emitter.rt.heap_save); i64_extend_i32_u; }); }
                            "restore" => {
                                self.emit_expr(&args[0]);
                                wasm!(self.func, { i32_wrap_i64; call(self.emitter.rt.heap_restore); });
                            }
                            _ => {}
                        }
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
                        // Delegate to the hardcoded Named handler:
                        // `testing.assert_gt` → `assert_gt`. After Stage 3a
                        // the monomorphizer may specialize bundled generic
                        // fns to `assert_some__String`, `assert_ok__Int_String`,
                        // ... The hardcoded Named dispatch above keys on the
                        // *base* name, so strip the mono suffix before
                        // delegating — otherwise the call falls through to
                        // the user-fn fallback and emits `unreachable` for
                        // any typed caller.
                        let fname = func.as_str();
                        let base = fname.split_once("__").map(|(b, _)| b).unwrap_or(fname);
                        let target = CallTarget::Named { name: almide_base::intern::sym(base) };
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
                                } else if let Some(func_idx) = self.resolve_module_method(module.as_str(), func.as_str()) {
                                    // Cross-module derived Codec method (#609): the
                                    // call site carries only `Type.method` (the
                                    // subject type has no module), but the fn is
                                    // registered under its module_origin-qualified
                                    // name `mod.Type.method` (#433 namespacing). A
                                    // unique func_map key ending in `.Type.method`
                                    // is that function.
                                    for arg in args { self.emit_expr(arg); }
                                    wasm!(self.func, { call(func_idx); });
                                } else {
                                    panic!(
                                "[ICE] emit_wasm: no WASM dispatch for `{}.{}` — \
                                 add an arm in emit_{}_call or resolve upstream",
                                module.as_str(), func.as_str(), module.as_str()
                            );
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
                        let target = CallTarget::Module { module: "list".into(), func: "sort".into(), def_id: None };
                        self.emit_call(&target, &fake, _ret_ty);
                    }
                    "reverse" | "list.reverse" if matches!(&object.ty, Ty::Applied(_, _)) => {
                        let fake = [(**object).clone()];
                        let target = CallTarget::Module { module: "list".into(), func: "reverse".into(), def_id: None };
                        self.emit_call(&target, &fake, _ret_ty);
                    }
                    "filter" | "list.filter" if matches!(&object.ty, Ty::Applied(_, _)) => {
                        let fake = [(**object).clone(), args[0].clone()];
                        let target = CallTarget::Module { module: "list".into(), func: "filter".into(), def_id: None };
                        self.emit_call(&target, &fake, _ret_ty);
                    }
                    "fold" | "list.fold" if matches!(&object.ty, Ty::Applied(_, _)) => {
                        let fake = [(**object).clone(), args[0].clone(), args[1].clone()];
                        let target = CallTarget::Module { module: "list".into(), func: "fold".into(), def_id: None };
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
                        wasm!(self.func, { call(self.emitter.rt.string.to_upper); });
                    }
                    "to_lower" | "string.to_lower" if matches!(object.ty, Ty::String) => {
                        self.emit_expr(object);
                        wasm!(self.func, { call(self.emitter.rt.string.to_lower); });
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
                            panic!(
                                "[ICE] emit_wasm: no WASM dispatch for Option method \
                                 `{}` (stripped: `{}`) — add an arm in emit_option_call \
                                 or resolve upstream",
                                method, m
                            );
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
                            def_id: None,
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
                            def_id: None,
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
                        panic!(
                            "[ICE] emit_wasm: unresolved method call `.{}` on \
                             object ty={:?} — expected TypeName.method or func_map \
                             hit, but both lookups missed",
                            method, object.ty
                        );
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
                    panic!(
                        "[ICE] emit_wasm: closure call through a non-Fn type `{:?}` — \
                         the call signature cannot be built; resolve upstream",
                        callee_fn_ty
                    );
                }
                self.scratch.free_i32(scratch);
            }
        }
    }
}
