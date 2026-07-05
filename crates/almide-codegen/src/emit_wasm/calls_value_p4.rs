impl FuncCompiler<'_> {
    /// __decode_default_T(v: Value, key: String, default: T) -> Result[T, String]
    /// Missing/null → ok(default), present → ok(as_T(field)?)
    fn emit_decode_default(&mut self, args: &[IrExpr], suffix: &str) {
        let v = self.scratch.alloc_i32();
        let key = self.scratch.alloc_i32();
        let list = self.scratch.alloc_i32();
        let len = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let pair_ptr = self.scratch.alloc_i32();
        let found = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();
        // Save default value based on type
        let default_local = match suffix {
            "int" => self.scratch.alloc_i64(),
            "float" => self.scratch.alloc_f64(),
            _ => self.scratch.alloc_i32(), // string, bool
        };

        self.emit_expr(&args[0]); // v
        wasm!(self.func, { local_set(v); });
        self.emit_expr(&args[1]); // key
        wasm!(self.func, { local_set(key); });
        self.emit_expr(&args[2]); // default value
        wasm!(self.func, {
            local_set(default_local);
            local_get(v); i32_load(0); i32_const(6); i32_ne;
            if_i32;
        });
        let err_msg = self.emitter.intern_string("expected Object");
        wasm!(self.func, {
              i32_const(8); call(self.emitter.rt.alloc); local_set(result);
              local_get(result); i32_const(1); i32_store(0);
              local_get(result); i32_const(err_msg as i32); i32_store(4);
              local_get(result);
            else_;
              local_get(v); i32_load(4); local_set(list);
              local_get(list); i32_load(0); local_set(len);
              i32_const(0); local_set(i);
              i32_const(0); local_set(found);
              block_empty; loop_empty;
                local_get(i); local_get(len); i32_ge_u; br_if(1);
                local_get(list); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                local_get(i); i32_const(4); i32_mul; i32_add;
                i32_load(0); local_set(pair_ptr);
                local_get(pair_ptr); i32_load(0);
                local_get(key);
                call(self.emitter.rt.string.eq);
                if_empty;
                  local_get(pair_ptr); i32_load(4); local_set(found);
                  br(2);
                end;
                local_get(i); i32_const(1); i32_add; local_set(i);
                br(0);
              end; end;
              // found==0 or null → use default
              local_get(found); i32_eqz;
              if_i32;
                i32_const(1);
              else_;
                local_get(found); i32_load(0); i32_eqz;
              end;
              if_i32;
        });
        // Return ok(default)
        match suffix {
            "string" | "bool" => {
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); i32_const(0); i32_store(0);
                    local_get(result); local_get(default_local); i32_store(4);
                    local_get(result);
                });
            }
            "int" => {
                wasm!(self.func, {
                    i32_const(12); call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); i32_const(0); i32_store(0);
                    local_get(result); local_get(default_local); i64_store(4);
                    local_get(result);
                });
            }
            "float" => {
                wasm!(self.func, {
                    i32_const(12); call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); i32_const(0); i32_store(0);
                    local_get(result); local_get(default_local); f64_store(4);
                    local_get(result);
                });
            }
            _ => {
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); i32_const(0); i32_store(0);
                    local_get(result); local_get(default_local); i32_store(4);
                    local_get(result);
                });
            }
        }
        wasm!(self.func, {
              else_;
        });
        // Extract value by type
        match suffix {
            "string" => {
                wasm!(self.func, {
                    local_get(found); i32_load(0); i32_const(4); i32_ne;
                    if_i32;
                });
                let te = self.emitter.intern_string("expected Str");
                wasm!(self.func, {
                        i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                        local_get(result); i32_const(1); i32_store(0);
                        local_get(result); i32_const(te as i32); i32_store(4);
                        local_get(result);
                    else_;
                        i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                        local_get(result); i32_const(0); i32_store(0);
                        local_get(result); local_get(found); i32_load(4); i32_store(4);
                        local_get(result);
                    end;
                });
            }
            "int" => {
                wasm!(self.func, {
                    local_get(found); i32_load(0); i32_const(2); i32_ne;
                    if_i32;
                });
                let te = self.emitter.intern_string("expected Int");
                wasm!(self.func, {
                        i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                        local_get(result); i32_const(1); i32_store(0);
                        local_get(result); i32_const(te as i32); i32_store(4);
                        local_get(result);
                    else_;
                        i32_const(12); call(self.emitter.rt.alloc); local_set(result);
                        local_get(result); i32_const(0); i32_store(0);
                        local_get(result); local_get(found); i64_load(4); i64_store(4);
                        local_get(result);
                    end;
                });
            }
            "float" => {
                // #658: widen a JSON Int (tag 2) to Float, like native as_float.
                let te = self.emitter.intern_string("expected Float");
                wasm!(self.func, {
                    local_get(found); i32_load(0); i32_const(3); i32_eq;
                    if_i32;
                        i32_const(12); call(self.emitter.rt.alloc); local_set(result);
                        local_get(result); i32_const(0); i32_store(0);
                        local_get(result); local_get(found); f64_load(4); f64_store(4);
                        local_get(result);
                    else_;
                      local_get(found); i32_load(0); i32_const(2); i32_eq;
                      if_i32;
                        i32_const(12); call(self.emitter.rt.alloc); local_set(result);
                        local_get(result); i32_const(0); i32_store(0);
                        local_get(result); local_get(found); i64_load(4); f64_convert_i64_s; f64_store(4);
                        local_get(result);
                      else_;
                        i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                        local_get(result); i32_const(1); i32_store(0);
                        local_get(result); i32_const(te as i32); i32_store(4);
                        local_get(result);
                      end;
                    end;
                });
            }
            "bool" => {
                wasm!(self.func, {
                    local_get(found); i32_load(0); i32_const(1); i32_ne;
                    if_i32;
                });
                let te = self.emitter.intern_string("expected Bool");
                wasm!(self.func, {
                        i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                        local_get(result); i32_const(1); i32_store(0);
                        local_get(result); i32_const(te as i32); i32_store(4);
                        local_get(result);
                    else_;
                        i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                        local_get(result); i32_const(0); i32_store(0);
                        local_get(result); local_get(found); i32_load(4); i32_store(4);
                        local_get(result);
                    end;
                });
            }
            other => {
                panic!(
                    "[ICE] emit_wasm: no WASM dispatch for `value.{}` — \
                     add an arm in emit_value_call or resolve upstream",
                    other
                );
            }
        }
        wasm!(self.func, {
              end;
            end;
        });

        match suffix {
            "int" => self.scratch.free_i64(default_local),
            "float" => self.scratch.free_f64(default_local),
            _ => self.scratch.free_i32(default_local),
        }
        self.scratch.free_i32(result);
        self.scratch.free_i32(found);
        self.scratch.free_i32(pair_ptr);
        self.scratch.free_i32(i);
        self.scratch.free_i32(len);
        self.scratch.free_i32(list);
        self.scratch.free_i32(key);
        self.scratch.free_i32(v);
    }

    /// __encode_list_T(xs: List[T]) -> Value (array of Value)
    fn emit_encode_list(&mut self, args: &[IrExpr], suffix: &str) {
        let xs = self.scratch.alloc_i32();
        let len = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let val_list = self.scratch.alloc_i32();
        let elem = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();

        self.emit_expr(&args[0]);
        wasm!(self.func, {
            local_set(xs);
            local_get(xs); i32_load(0); local_set(len);
            // Alloc value list: [len:i32][ptr0][ptr1]...
            i32_const(4); local_get(len); i32_const(4); i32_mul; i32_add;
            call(self.emitter.rt.alloc); local_set(val_list);
            local_get(val_list); local_get(len); i32_store(0);
            i32_const(0); local_set(i);
            block_empty; loop_empty;
              local_get(i); local_get(len); i32_ge_u; br_if(1);
        });
        // Read element from source list
        let _elem_stride = match suffix {
            "int" => 8, "float" => 8, _ => 4,
        };
        match suffix {
            "string" => {
                wasm!(self.func, {
                    local_get(xs); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::SWISS_MAP, super::engine::layout::map::TAGS) as i32); i32_add;
                    local_get(i); i32_const(4); i32_mul; i32_add;
                    i32_load(0); local_set(elem);
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); i32_const(4); i32_store(0); // string tag
                    local_get(result); local_get(elem); i32_store(4);
                });
            }
            "int" => {
                let elem64 = self.scratch.alloc_i64();
                wasm!(self.func, {
                    local_get(xs); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::SWISS_MAP, super::engine::layout::map::TAGS) as i32); i32_add;
                    local_get(i); i32_const(8); i32_mul; i32_add;
                    i64_load(0); local_set(elem64);
                    i32_const(12); call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); i32_const(2); i32_store(0); // int tag
                    local_get(result); local_get(elem64); i64_store(4);
                });
                self.scratch.free_i64(elem64);
            }
            "float" => {
                let elem_f = self.scratch.alloc_f64();
                wasm!(self.func, {
                    local_get(xs); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::SWISS_MAP, super::engine::layout::map::TAGS) as i32); i32_add;
                    local_get(i); i32_const(8); i32_mul; i32_add;
                    f64_load(0); local_set(elem_f);
                    i32_const(12); call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); i32_const(3); i32_store(0); // float tag
                    local_get(result); local_get(elem_f); f64_store(4);
                });
                self.scratch.free_f64(elem_f);
            }
            "bool" => {
                wasm!(self.func, {
                    local_get(xs); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::SWISS_MAP, super::engine::layout::map::TAGS) as i32); i32_add;
                    local_get(i); i32_const(4); i32_mul; i32_add;
                    i32_load(0); local_set(elem);
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); i32_const(1); i32_store(0); // bool tag
                    local_get(result); local_get(elem); i32_store(4);
                });
            }
            _ => {
                // Named type: call Type.encode(elem)
                wasm!(self.func, {
                    local_get(xs); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::SWISS_MAP, super::engine::layout::map::TAGS) as i32); i32_add;
                    local_get(i); i32_const(4); i32_mul; i32_add;
                    i32_load(0);
                });
                let encode_name = format!("{}.encode", suffix);
                if let Some(&func_idx) = self.emitter.func_map.get(encode_name.as_str()) {
                    wasm!(self.func, { call(func_idx); local_set(result); });
                } else {
                    wasm!(self.func, { local_set(result); }); // passthrough ptr
                }
            }
        }
        wasm!(self.func, {
              // Store value in val_list
              local_get(val_list); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::SWISS_MAP, super::engine::layout::map::TAGS) as i32); i32_add;
              local_get(i); i32_const(4); i32_mul; i32_add;
              local_get(result); i32_store(0);
              local_get(i); i32_const(1); i32_add; local_set(i);
              br(0);
            end; end;
            // Wrap in Value array: [tag=5][list_ptr]
            i32_const(8); call(self.emitter.rt.alloc); local_set(result);
            local_get(result); i32_const(5); i32_store(0); // array tag
            local_get(result); local_get(val_list); i32_store(4);
            local_get(result);
        });

        self.scratch.free_i32(result);
        self.scratch.free_i32(elem);
        self.scratch.free_i32(val_list);
        self.scratch.free_i32(i);
        self.scratch.free_i32(len);
        self.scratch.free_i32(xs);
    }

    /// __decode_list_T(v: Value) -> Result[List[T], String]
    fn emit_decode_list(&mut self, args: &[IrExpr], suffix: &str) {
        let v = self.scratch.alloc_i32();
        let arr_list = self.scratch.alloc_i32();
        let len = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let out_list = self.scratch.alloc_i32();
        let elem_val = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();

        self.emit_expr(&args[0]);
        wasm!(self.func, {
            local_set(v);
            // Must be array (tag 5)
            local_get(v); i32_load(0); i32_const(5); i32_ne;
            if_i32;
        });
        let err_msg = self.emitter.intern_string("expected Array");
        let elem_size: u32 = match suffix {
            "int" | "float" => 8, _ => 4,
        };
        wasm!(self.func, {
              i32_const(8); call(self.emitter.rt.alloc); local_set(result);
              local_get(result); i32_const(1); i32_store(0);
              local_get(result); i32_const(err_msg as i32); i32_store(4);
              local_get(result);
            else_;
              local_get(v); i32_load(4); local_set(arr_list);
              local_get(arr_list); i32_load(0); local_set(len);
              // Alloc output list
              i32_const(4); local_get(len); i32_const(elem_size as i32); i32_mul; i32_add;
              call(self.emitter.rt.alloc); local_set(out_list);
              local_get(out_list); local_get(len); i32_store(0);
              i32_const(0); local_set(i);
              block_empty; loop_empty;
                local_get(i); local_get(len); i32_ge_u; br_if(1);
                // Get Value element from array
                local_get(arr_list); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::SWISS_MAP, super::engine::layout::map::TAGS) as i32); i32_add;
                local_get(i); i32_const(4); i32_mul; i32_add;
                i32_load(0); local_set(elem_val);
        });
        // Extract typed value from Value element
        match suffix {
            "string" => {
                wasm!(self.func, {
                    local_get(out_list); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::SWISS_MAP, super::engine::layout::map::TAGS) as i32); i32_add;
                    local_get(i); i32_const(4); i32_mul; i32_add;
                    local_get(elem_val); i32_load(4); // string ptr from Value
                    i32_store(0);
                });
            }
            "int" => {
                wasm!(self.func, {
                    local_get(out_list); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::SWISS_MAP, super::engine::layout::map::TAGS) as i32); i32_add;
                    local_get(i); i32_const(8); i32_mul; i32_add;
                    local_get(elem_val); i64_load(4); // i64 from Value
                    i64_store(0);
                });
            }
            "float" => {
                wasm!(self.func, {
                    local_get(out_list); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::SWISS_MAP, super::engine::layout::map::TAGS) as i32); i32_add;
                    local_get(i); i32_const(8); i32_mul; i32_add;
                    local_get(elem_val); f64_load(4);
                    f64_store(0);
                });
            }
            "bool" => {
                wasm!(self.func, {
                    local_get(out_list); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::SWISS_MAP, super::engine::layout::map::TAGS) as i32); i32_add;
                    local_get(i); i32_const(4); i32_mul; i32_add;
                    local_get(elem_val); i32_load(4); // bool (0/1)
                    i32_store(0);
                });
            }
            _ => {
                // Named type: call Type.decode(elem_val), unwrap ok
                let decode_name = format!("{}.decode", suffix);
                if let Some(&func_idx) = self.emitter.func_map.get(decode_name.as_str()) {
                    let dr = self.scratch.alloc_i32();
                    wasm!(self.func, {
                        local_get(elem_val); call(func_idx); local_set(dr);
                        local_get(out_list); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::SWISS_MAP, super::engine::layout::map::TAGS) as i32); i32_add;
                        local_get(i); i32_const(4); i32_mul; i32_add;
                        local_get(dr); i32_load(4); // ok payload (pointer)
                        i32_store(0);
                    });
                    self.scratch.free_i32(dr);
                } else {
                    wasm!(self.func, {
                        local_get(out_list); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::SWISS_MAP, super::engine::layout::map::TAGS) as i32); i32_add;
                        local_get(i); i32_const(4); i32_mul; i32_add;
                        local_get(elem_val);
                        i32_store(0);
                    });
                }
            }
        }
        wasm!(self.func, {
                local_get(i); i32_const(1); i32_add; local_set(i);
                br(0);
              end; end;
              // Wrap in ok Result
              i32_const(8); call(self.emitter.rt.alloc); local_set(result);
              local_get(result); i32_const(0); i32_store(0);
              local_get(result); local_get(out_list); i32_store(4);
              local_get(result);
            end;
        });

        self.scratch.free_i32(result);
        self.scratch.free_i32(elem_val);
        self.scratch.free_i32(out_list);
        self.scratch.free_i32(i);
        self.scratch.free_i32(len);
        self.scratch.free_i32(arr_list);
        self.scratch.free_i32(v);
    }
}
