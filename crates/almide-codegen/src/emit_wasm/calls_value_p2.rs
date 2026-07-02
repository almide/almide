impl FuncCompiler<'_> {
    /// value.get(v, key) -> Result[Value, String]
    /// Check tag==6 (object), iterate pairs list for matching key.
    /// value.get(v, key) -> Option[Value]
    /// Check tag==6 (object), iterate pairs list for matching key.
    /// Returns some(value_ptr) or none(0).
    fn emit_value_get(&mut self, args: &[IrExpr]) {
        let v = self.scratch.alloc_i32();
        let key = self.scratch.alloc_i32();
        let list = self.scratch.alloc_i32();
        let len = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let pair_ptr = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();

        self.emit_expr(&args[0]);
        wasm!(self.func, { local_set(v); });
        self.emit_expr(&args[1]);
        wasm!(self.func, {
            local_set(key);
            i32_const(0); local_set(result); // none by default
            // Check tag == 6 (object)
            local_get(v); i32_load(0); i32_const(6); i32_eq;
            if_empty;
              // It's an object: iterate pairs
              local_get(v); i32_load(4); local_set(list);
              local_get(list); i32_load(0); local_set(len);
              i32_const(0); local_set(i);
              block_empty; loop_empty;
                local_get(i); local_get(len); i32_ge_u; br_if(1);
                local_get(list); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                local_get(i); i32_const(4); i32_mul; i32_add;
                i32_load(0); local_set(pair_ptr);
                local_get(pair_ptr); i32_load(0);
                local_get(key);
                call(self.emitter.rt.string.eq);
                if_empty;
                  // Found: some(value) — alloc Option box with value ptr.
                  // SHARE: the boxed pointer is an interior reference into a
                  // Value tree the caller still owns — dup it.
                  i32_const(4); call(self.emitter.rt.alloc); local_set(result);
                  local_get(result);
                  local_get(pair_ptr); i32_load(4); call(self.emitter.rt.rc_inc);
                  i32_store(0);
                  br(2);
                end;
                local_get(i); i32_const(1); i32_add; local_set(i);
                br(0);
              end; end;
            end;
            local_get(result); // 0 = none, or some ptr
        });

        self.scratch.free_i32(result);
        self.scratch.free_i32(pair_ptr);
        self.scratch.free_i32(i);
        self.scratch.free_i32(len);
        self.scratch.free_i32(list);
        self.scratch.free_i32(key);
        self.scratch.free_i32(v);
    }

    /// value.as_string / value.as_bool: check tag, return ok(payload) or err.
    /// For tag=4 (string) payload is i32 at offset 4.
    /// For tag=1 (bool) payload is i32 at offset 4.
    fn emit_value_as_type(&mut self, args: &[IrExpr], expected_tag: i32, type_name: &str) {
        let v = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();

        self.emit_expr(&args[0]);
        wasm!(self.func, {
            local_set(v);
            local_get(v); i32_load(0); i32_const(expected_tag); i32_eq;
            if_i32;
              // Correct tag: return ok(payload at offset 4). The STRING
              // payload is an interior pointer into the surviving Value —
              // dup it (scalar tags copy by value).
              i32_const(8); call(self.emitter.rt.alloc); local_set(result);
              local_get(result); i32_const(0); i32_store(0); // ok
              local_get(result);
              local_get(v); i32_load(4);
        });
        if expected_tag == 4 || expected_tag == 5 {
            // tags 4 (string) and 5 (array) carry POINTER payloads; bool
            // shares the 4-byte size but is a scalar — key on the TAG.
            wasm!(self.func, { call(self.emitter.rt.rc_inc); });
        }
        wasm!(self.func, {
              i32_store(4);
              local_get(result);
            else_;
              // Wrong tag: return err("expected <type>")
        });
        let err_msg = self.emitter.intern_string(&format!("expected {}", type_name));
        wasm!(self.func, {
              i32_const(8); call(self.emitter.rt.alloc); local_set(result);
              local_get(result); i32_const(1); i32_store(0); // err
              local_get(result); i32_const(err_msg as i32); i32_store(4);
              local_get(result);
            end;
        });

        self.scratch.free_i32(result);
        self.scratch.free_i32(v);
    }

    /// value.as_int: check tag==2, return ok(i64) or err.
    /// Result layout for Int: [tag:i32][padding:4][i64:8] = 16 bytes
    fn emit_value_as_int(&mut self, args: &[IrExpr]) {
        let v = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();

        self.emit_expr(&args[0]);
        wasm!(self.func, {
            local_set(v);
            local_get(v); i32_load(0); i32_const(2); i32_eq;
            if_i32;
              // tag==2 (int): payload is i64 at offset 4
              i32_const(12); call(self.emitter.rt.alloc); local_set(result);
              local_get(result); i32_const(0); i32_store(0); // ok tag
              local_get(result); local_get(v); i64_load(4); i64_store(4); // i64 payload at offset 4
              local_get(result);
            else_;
        });
        let err_msg = self.emitter.intern_string("expected Int");
        wasm!(self.func, {
              i32_const(8); call(self.emitter.rt.alloc); local_set(result);
              local_get(result); i32_const(1); i32_store(0); // err
              local_get(result); i32_const(err_msg as i32); i32_store(4);
              local_get(result);
            end;
        });

        self.scratch.free_i32(result);
        self.scratch.free_i32(v);
    }

    /// json.get_string / get_int / get_bool / get_float / get_array
    /// Returns Option[T]: get value by key, check type tag, unwrap payload.
    fn emit_json_get_typed(&mut self, func: &str, args: &[IrExpr]) {
        let expected_tag: i32 = match func {
            "get_string" => 4,
            "get_int" => 2,
            "get_float" => 3,
            "get_bool" => 1,
            "get_array" => 5,
            _ => 4,
        };

        // First do json.get(v, key) → Option[Value]
        // Then check the Value's tag matches expected
        let v = self.scratch.alloc_i32();
        let key = self.scratch.alloc_i32();
        let list = self.scratch.alloc_i32();
        let len = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let pair_ptr = self.scratch.alloc_i32();
        let found_val = self.scratch.alloc_i32();

        self.emit_expr(&args[0]);
        wasm!(self.func, { local_set(v); });
        self.emit_expr(&args[1]);
        wasm!(self.func, {
            local_set(key);
            i32_const(0); local_set(found_val);
            local_get(v); i32_load(0); i32_const(6); i32_eq;
            if_empty;
              local_get(v); i32_load(4); local_set(list);
              local_get(list); i32_load(0); local_set(len);
              i32_const(0); local_set(i);
              block_empty; loop_empty;
                local_get(i); local_get(len); i32_ge_u; br_if(1);
                local_get(list); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                local_get(i); i32_const(4); i32_mul; i32_add;
                i32_load(0); local_set(pair_ptr);
                local_get(pair_ptr); i32_load(0);
                local_get(key);
                call(self.emitter.rt.string.eq);
                if_empty;
                  // Found key. Check tag.
                  local_get(pair_ptr); i32_load(4); local_set(found_val); // value ptr
                  local_get(found_val); i32_load(0); i32_const(expected_tag); i32_ne;
                  if_empty;
                    i32_const(0); local_set(found_val); // wrong type → none
                  end;
                  br(2);
                end;
                local_get(i); i32_const(1); i32_add; local_set(i);
                br(0);
              end; end;
            end;
        });

        // found_val is Value ptr (with matching tag) or 0.
        // Return Option[T]: some = alloc box with payload, none = 0.
        // For all types, we alloc a box and copy the payload.
        let payload_size: u32 = match func {
            "get_int" => 8,    // i64
            "get_float" => 8,  // f64
            _ => 4,            // i32 (string ptr, bool, list ptr)
        };
        let option_box = self.scratch.alloc_i32();
        wasm!(self.func, {
            local_get(found_val); i32_eqz;
            if_i32; i32_const(0); // none
            else_;
              i32_const(payload_size as i32); call(self.emitter.rt.alloc); local_set(option_box);
              local_get(option_box);
              local_get(found_val);
        });
        // Copy payload from Value offset 4 to option box offset 0.
        // get_string/get_array box an INTERIOR POINTER into the surviving
        // Value tree → dup (SHARE). get_int/get_float are 8-byte scalars;
        // get_bool is a 4-byte scalar — key on the FUNC, not the size.
        let payload_is_ptr = matches!(func, "get_string" | "get_array");
        match payload_size {
            8 => { wasm!(self.func, { i64_load(4); i64_store(0); }); }
            _ if payload_is_ptr => { wasm!(self.func, { i32_load(4); call(self.emitter.rt.rc_inc); i32_store(0); }); }
            _ => { wasm!(self.func, { i32_load(4); i32_store(0); }); }
        }
        wasm!(self.func, {
              local_get(option_box);
            end;
        });
        self.scratch.free_i32(option_box);

        self.scratch.free_i32(found_val);
        self.scratch.free_i32(pair_ptr);
        self.scratch.free_i32(i);
        self.scratch.free_i32(len);
        self.scratch.free_i32(list);
        self.scratch.free_i32(key);
        self.scratch.free_i32(v);
    }

    /// json.as_string / as_int / as_bool / as_float / as_array → Option[T]
    /// (json module returns Option, value module returns Result — handled separately)
    fn emit_json_as_typed(&mut self, func: &str, args: &[IrExpr]) {
        let expected_tag: i32 = match func {
            "as_string" => 4,
            "as_int" => 2,
            "as_float" => 3,
            "as_bool" => 1,
            "as_array" => 5,
            _ => 4,
        };
        let payload_size: u32 = match func {
            "as_int" => 8,
            "as_float" => 8,
            _ => 4,
        };
        // #658: json.as_float widens a JSON Int to Float, matching the native
        // almide_json_as_float oracle (a JSON number is type-agnostic).
        let coerce_int = func == "as_float";
        let v = self.scratch.alloc_i32();
        let option_box = self.scratch.alloc_i32();
        self.emit_expr(&args[0]);
        wasm!(self.func, {
            local_set(v);
            local_get(v); i32_load(0); i32_const(expected_tag); i32_eq;
            if_i32;
              // Matching tag: alloc option box, copy payload
              i32_const(payload_size as i32); call(self.emitter.rt.alloc); local_set(option_box);
              local_get(option_box);
              local_get(v);
        });
        match payload_size {
            8 => { wasm!(self.func, { i64_load(4); i64_store(0); }); }
            _ => { wasm!(self.func, { i32_load(4); i32_store(0); }); }
        }
        wasm!(self.func, {
              local_get(option_box);
            else_;
        });
        if coerce_int {
            wasm!(self.func, {
              local_get(v); i32_load(0); i32_const(2); i32_eq;
              if_i32;
                i32_const(8); call(self.emitter.rt.alloc); local_set(option_box);
                local_get(option_box); local_get(v); i64_load(4); f64_convert_i64_s; f64_store(0);
                local_get(option_box);
              else_;
                i32_const(0); // none
              end;
            });
        } else {
            wasm!(self.func, { i32_const(0); }); // none
        }
        wasm!(self.func, {
            end;
        });
        self.scratch.free_i32(option_box);
        self.scratch.free_i32(v);
    }

    /// json.keys(v: Value) → List[String]: extract keys from object
    fn emit_json_keys(&mut self, args: &[IrExpr]) {
        let v = self.scratch.alloc_i32();
        let list = self.scratch.alloc_i32();
        let len = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();

        self.emit_expr(&args[0]);
        wasm!(self.func, {
            local_set(v);
            local_get(v); i32_load(0); i32_const(6); i32_eq;
            if_i32;
              local_get(v); i32_load(4); local_set(list);
              local_get(list); i32_load(0); local_set(len);
              // Alloc result list — FULL [len][cap][data] layout (the old
              // `4 + n*4` alloc had no cap word: elements landed one slot late
              // and the last write ran past the allocation).
              i32_const(8); local_get(len); i32_const(4); i32_mul; i32_add;
              call(self.emitter.rt.alloc); local_set(result);
              local_get(result); local_get(len); i32_store(0);
              local_get(result); local_get(len); i32_store(4); // cap = len
              i32_const(0); local_set(i);
              block_empty; loop_empty;
                local_get(i); local_get(len); i32_ge_u; br_if(1);
                // result[i] = pair[i].key
                local_get(result); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::SWISS_MAP, super::engine::layout::map::TAGS) as i32); i32_add;
                local_get(i); i32_const(4); i32_mul; i32_add;
                local_get(list); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::SWISS_MAP, super::engine::layout::map::TAGS) as i32); i32_add;
                local_get(i); i32_const(4); i32_mul; i32_add;
                i32_load(0); // pair ptr
                i32_load(0); // key string ptr
                // SHARE: the key string stays owned by the object's pair — the
                // returned list must own its own +1, or binding an element
                // (`list.get(json.keys(v), 0) ?? d`) frees the object's live
                // key (#668 class).
                call(self.emitter.rt.rc_inc);
                i32_store(0); // store in result
                local_get(i); i32_const(1); i32_add; local_set(i);
                br(0);
              end; end;
              local_get(result);
            else_;
              // Not an object: return empty list ([len][cap] header)
              i32_const(8); call(self.emitter.rt.alloc); local_set(result);
              local_get(result); i32_const(0); i32_store(0);
              local_get(result); i32_const(0); i32_store(4);
              local_get(result);
            end;
        });

        self.scratch.free_i32(i);
        self.scratch.free_i32(result);
        self.scratch.free_i32(len);
        self.scratch.free_i32(list);
        self.scratch.free_i32(v);
    }

    /// value.field(v, key) -> Result[Value, String]
    fn emit_value_field_result(&mut self, args: &[IrExpr]) {
        let v = self.scratch.alloc_i32();
        let key = self.scratch.alloc_i32();
        let list = self.scratch.alloc_i32();
        let len = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let pair_ptr = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();

        self.emit_expr(&args[0]);
        wasm!(self.func, { local_set(v); });
        self.emit_expr(&args[1]);
        wasm!(self.func, {
            local_set(key);
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
              i32_const(0); local_set(result);
              block_empty; loop_empty;
                local_get(i); local_get(len); i32_ge_u; br_if(1);
                local_get(list); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                local_get(i); i32_const(4); i32_mul; i32_add;
                i32_load(0); local_set(pair_ptr);
                local_get(pair_ptr); i32_load(0);
                local_get(key);
                call(self.emitter.rt.string.eq);
                if_empty;
                  i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                  local_get(result); i32_const(0); i32_store(0);
                  // SHARE: the ok payload is the pair's value, still owned by
                  // the object — the Result box must own its own +1 (#668
                  // class; unwrapping `value.get(v, k)` freed the object's
                  // live field value).
                  local_get(result); local_get(pair_ptr); i32_load(4);
                  call(self.emitter.rt.rc_inc); i32_store(4);
                  br(2);
                end;
                local_get(i); i32_const(1); i32_add; local_set(i);
                br(0);
              end; end;
              local_get(result); i32_eqz;
              if_empty;
        });
        // Mirror the native oracle's `format!("missing field '{}'", key)` exactly
        // (runtime/rs/src/value.rs::almide_rt_value_field) so Codec decode errors
        // are byte-identical across targets (#657). The key is a runtime string,
        // so build the message with two __concat_str calls.
        let mf_prefix = self.emitter.intern_string("missing field '");
        let mf_suffix = self.emitter.intern_string("'");
        wasm!(self.func, {
                i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                local_get(result); i32_const(1); i32_store(0);
                local_get(result);
                  i32_const(mf_prefix as i32); local_get(key); call(self.emitter.rt.concat_str);
                  i32_const(mf_suffix as i32); call(self.emitter.rt.concat_str);
                i32_store(4);
              end;
              local_get(result);
            end;
        });

        self.scratch.free_i32(result);
        self.scratch.free_i32(pair_ptr);
        self.scratch.free_i32(i);
        self.scratch.free_i32(len);
        self.scratch.free_i32(list);
        self.scratch.free_i32(key);
        self.scratch.free_i32(v);
    }

    /// almide_rt_value_tagged_variant(v: Value) → Result[(String, Value), String]
    /// Extract variant tag name and payload from encoded Value object.
    /// Format: {"CaseName": payload_value} — first key is the tag, its value is payload.
    pub(super) fn emit_value_tagged_variant(&mut self, args: &[IrExpr]) {
        let v = self.scratch.alloc_i32();
        let list = self.scratch.alloc_i32();
        let pair_ptr = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();

        self.emit_expr(&args[0]);
        wasm!(self.func, {
            local_set(v);
            // Must be object (tag 6) with at least 1 pair
            local_get(v); i32_load(0); i32_const(6); i32_ne;
            if_i32;
        });
        let err_msg = self.emitter.intern_string("not a tagged variant");
        wasm!(self.func, {
              i32_const(8); call(self.emitter.rt.alloc); local_set(result);
              local_get(result); i32_const(1); i32_store(0);
              local_get(result); i32_const(err_msg as i32); i32_store(4);
              local_get(result);
            else_;
              // Get first pair: key = tag name, value = payload
              local_get(v); i32_load(4); local_set(list); // pairs list
              local_get(list); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::SWISS_MAP, super::engine::layout::map::TAGS) as i32); i32_add; i32_load(0); local_set(pair_ptr); // first pair tuple
              // Build tuple (tag_name: String, payload: Value)
              i32_const(8); call(self.emitter.rt.alloc); local_set(result);
              local_get(result); local_get(pair_ptr); i32_load(0); i32_store(0); // tag name string
              local_get(result); local_get(pair_ptr); i32_load(4); i32_store(4); // payload value
              // Build ok(tuple)
              local_get(result); local_set(pair_ptr); // save tuple ptr
              i32_const(8); call(self.emitter.rt.alloc); local_set(result);
              local_get(result); i32_const(0); i32_store(0); // tag = ok
              local_get(result); local_get(pair_ptr); i32_store(4);
              local_get(result);
            end;
        });

        self.scratch.free_i32(result);
        self.scratch.free_i32(pair_ptr);
        self.scratch.free_i32(list);
        self.scratch.free_i32(v);
    }

    /// value.to_camel_case / to_snake_case: transform object keys
    /// to_camel = true: snake_case → camelCase
    /// to_camel = false: camelCase → snake_case
    fn emit_value_key_transform(&mut self, args: &[IrExpr], to_camel: bool) {
        // If not an object, return as-is
        let v = self.scratch.alloc_i32();
        let old_list = self.scratch.alloc_i32();
        let new_list = self.scratch.alloc_i32();
        let len = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let old_pair = self.scratch.alloc_i32();
        let new_pair = self.scratch.alloc_i32();
        let old_key = self.scratch.alloc_i32();
        let new_key = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();
        // String transform locals
        let src_len = self.scratch.alloc_i32();
        let j = self.scratch.alloc_i32();
        let dst_pos = self.scratch.alloc_i32();
        let ch = self.scratch.alloc_i32();
        let dst_buf = self.scratch.alloc_i32();

        self.emit_expr(&args[0]);
        wasm!(self.func, {
            local_set(v);
            local_get(v); i32_load(0); i32_const(6); i32_ne;
            if_i32; local_get(v); // not object → return as-is
            else_;
              local_get(v); i32_load(4); local_set(old_list);
              local_get(old_list); i32_load(0); local_set(len);
              // Alloc new pairs list
              i32_const(4); local_get(len); i32_const(4); i32_mul; i32_add;
              call(self.emitter.rt.alloc); local_set(new_list);
              local_get(new_list); local_get(len); i32_store(0);
              i32_const(0); local_set(i);
              block_empty; loop_empty;
                local_get(i); local_get(len); i32_ge_u; br_if(1);
                // Get old pair
                local_get(old_list); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::SWISS_MAP, super::engine::layout::map::TAGS) as i32); i32_add;
                local_get(i); i32_const(4); i32_mul; i32_add;
                i32_load(0); local_set(old_pair);
                local_get(old_pair); i32_load(0); local_set(old_key);
                // Transform key
                local_get(old_key); i32_load(0); local_set(src_len); // key string len
                // Alloc dst buffer (max 2x src_len for snake_case expansion)
                i32_const(4); local_get(src_len); i32_const(2); i32_mul; i32_add;
                call(self.emitter.rt.alloc); local_set(dst_buf);
                i32_const(0); local_set(j); // src index
                i32_const(0); local_set(dst_pos); // dst index
        });

        if to_camel {
            // snake_case → camelCase: skip '_', capitalize next
            wasm!(self.func, {
                block_empty; loop_empty;
                  local_get(j); local_get(src_len); i32_ge_u; br_if(1);
                  local_get(old_key); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(j); i32_add; i32_load8_u(0);
                  local_set(ch);
                  local_get(ch); i32_const(95); i32_eq; // '_'
                  if_empty;
                    // Skip underscore, capitalize next char
                    local_get(j); i32_const(1); i32_add; local_set(j);
                    local_get(j); local_get(src_len); i32_lt_u;
                    if_empty;
                      local_get(dst_buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(dst_pos); i32_add;
                      local_get(old_key); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(j); i32_add; i32_load8_u(0);
                      i32_const(32); i32_sub; // to uppercase
                      i32_store8(0);
                      local_get(dst_pos); i32_const(1); i32_add; local_set(dst_pos);
                    end;
                  else_;
                    // Copy char as-is
                    local_get(dst_buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(dst_pos); i32_add;
                    local_get(ch);
                    i32_store8(0);
                    local_get(dst_pos); i32_const(1); i32_add; local_set(dst_pos);
                  end;
                  local_get(j); i32_const(1); i32_add; local_set(j);
                  br(0);
                end; end;
            });
        } else {
            // camelCase → snake_case: insert '_' before uppercase, lowercase
            wasm!(self.func, {
                block_empty; loop_empty;
                  local_get(j); local_get(src_len); i32_ge_u; br_if(1);
                  local_get(old_key); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(j); i32_add; i32_load8_u(0);
                  local_set(ch);
                  // Check if uppercase (A=65..Z=90)
                  local_get(ch); i32_const(65); i32_ge_u;
                  local_get(ch); i32_const(90); i32_le_u;
                  i32_and;
                  if_empty;
                    // Insert underscore then lowercase char
                    local_get(dst_buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(dst_pos); i32_add;
                    i32_const(95); i32_store8(0); // '_'
                    local_get(dst_pos); i32_const(1); i32_add; local_set(dst_pos);
                    local_get(dst_buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(dst_pos); i32_add;
                    local_get(ch); i32_const(32); i32_add; // to lowercase
                    i32_store8(0);
                    local_get(dst_pos); i32_const(1); i32_add; local_set(dst_pos);
                  else_;
                    local_get(dst_buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(dst_pos); i32_add;
                    local_get(ch); i32_store8(0);
                    local_get(dst_pos); i32_const(1); i32_add; local_set(dst_pos);
                  end;
                  local_get(j); i32_const(1); i32_add; local_set(j);
                  br(0);
                end; end;
            });
        }

        wasm!(self.func, {
                // Set dst string length
                local_get(dst_buf); local_get(dst_pos); i32_store(0);
                local_get(dst_buf); local_set(new_key);
                // Build new pair (new_key, old_value)
                i32_const(8); call(self.emitter.rt.alloc); local_set(new_pair);
                local_get(new_pair); local_get(new_key); i32_store(0);
                local_get(new_pair); local_get(old_pair); i32_load(4); i32_store(4);
                // Store in new list
                local_get(new_list); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::SWISS_MAP, super::engine::layout::map::TAGS) as i32); i32_add;
                local_get(i); i32_const(4); i32_mul; i32_add;
                local_get(new_pair); i32_store(0);
                local_get(i); i32_const(1); i32_add; local_set(i);
                br(0);
              end; end;
              // Build new Value object
              i32_const(8); call(self.emitter.rt.alloc); local_set(result);
              local_get(result); i32_const(6); i32_store(0); // tag = object
              local_get(result); local_get(new_list); i32_store(4);
              local_get(result);
            end;
        });

        self.scratch.free_i32(dst_buf);
        self.scratch.free_i32(ch);
        self.scratch.free_i32(dst_pos);
        self.scratch.free_i32(j);
        self.scratch.free_i32(src_len);
        self.scratch.free_i32(result);
        self.scratch.free_i32(new_key);
        self.scratch.free_i32(old_key);
        self.scratch.free_i32(new_pair);
        self.scratch.free_i32(old_pair);
        self.scratch.free_i32(i);
        self.scratch.free_i32(len);
        self.scratch.free_i32(new_list);
        self.scratch.free_i32(old_list);
        self.scratch.free_i32(v);
    }
}
