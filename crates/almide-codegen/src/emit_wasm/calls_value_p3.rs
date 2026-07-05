impl FuncCompiler<'_> {
    /// value.pick / value.omit: filter object keys
    /// pick=true: keep only keys in list, pick=false: remove keys in list
    fn emit_value_pick_omit(&mut self, args: &[IrExpr], is_pick: bool) {
        let v = self.scratch.alloc_i32();
        let keys = self.scratch.alloc_i32();
        let old_list = self.scratch.alloc_i32();
        let old_len = self.scratch.alloc_i32();
        let keys_len = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let j = self.scratch.alloc_i32();
        let pair_ptr = self.scratch.alloc_i32();
        let found = self.scratch.alloc_i32();
        let new_list = self.scratch.alloc_i32();
        let count = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();

        self.emit_expr(&args[0]); // v: Value (object)
        wasm!(self.func, { local_set(v); });
        self.emit_expr(&args[1]); // keys: List[String]
        wasm!(self.func, {
            local_set(keys);
            // Not an object? return as-is.
            // SHARE: hands back the INPUT value — own a +1 (#668 class).
            local_get(v); i32_load(0); i32_const(6); i32_ne;
            if_i32; local_get(v); call(self.emitter.rt.rc_inc);
            else_;
              local_get(v); i32_load(4); local_set(old_list);
              local_get(old_list); i32_load(0); local_set(old_len);
              local_get(keys); i32_load(0); local_set(keys_len);
              // First pass: count matching pairs
              i32_const(0); local_set(count);
              i32_const(0); local_set(i);
              block_empty; loop_empty;
                local_get(i); local_get(old_len); i32_ge_u; br_if(1);
                local_get(old_list); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::SWISS_MAP, super::engine::layout::map::TAGS) as i32); i32_add;
                local_get(i); i32_const(4); i32_mul; i32_add;
                i32_load(0); local_set(pair_ptr);
                // Check if key is in keys list
                i32_const(0); local_set(found);
                i32_const(0); local_set(j);
                block_empty; loop_empty;
                  local_get(j); local_get(keys_len); i32_ge_u; br_if(1);
                  local_get(pair_ptr); i32_load(0);
                  local_get(keys); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::SWISS_MAP, super::engine::layout::map::TAGS) as i32); i32_add;
                  local_get(j); i32_const(4); i32_mul; i32_add;
                  i32_load(0);
                  call(self.emitter.rt.string.eq);
                  if_empty;
                    i32_const(1); local_set(found);
                    br(2);
                  end;
                  local_get(j); i32_const(1); i32_add; local_set(j);
                  br(0);
                end; end;
        });
        // pick: include if found, omit: include if NOT found
        if is_pick {
            wasm!(self.func, {
                local_get(found);
                if_empty; local_get(count); i32_const(1); i32_add; local_set(count); end;
            });
        } else {
            wasm!(self.func, {
                local_get(found); i32_eqz;
                if_empty; local_get(count); i32_const(1); i32_add; local_set(count); end;
            });
        }
        wasm!(self.func, {
                local_get(i); i32_const(1); i32_add; local_set(i);
                br(0);
              end; end;
              // Alloc new pair list — FULL list layout [len][cap][data]: the old
              // `4 + n*4` alloc had no cap word, so every element landed 4 bytes
              // past its slot and the LAST write ran past the allocation
              // (silent heap clobber; value.merge/pick read back ""/null).
              i32_const(8); local_get(count); i32_const(4); i32_mul; i32_add;
              call(self.emitter.rt.alloc); local_set(new_list);
              local_get(new_list); local_get(count); i32_store(0);
              local_get(new_list); local_get(count); i32_store(4); // cap = len
              // Second pass: copy matching pairs
              i32_const(0); local_set(i);
              i32_const(0); local_set(count); // reuse as write index
              block_empty; loop_empty;
                local_get(i); local_get(old_len); i32_ge_u; br_if(1);
                local_get(old_list); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::SWISS_MAP, super::engine::layout::map::TAGS) as i32); i32_add;
                local_get(i); i32_const(4); i32_mul; i32_add;
                i32_load(0); local_set(pair_ptr);
                i32_const(0); local_set(found);
                i32_const(0); local_set(j);
                block_empty; loop_empty;
                  local_get(j); local_get(keys_len); i32_ge_u; br_if(1);
                  local_get(pair_ptr); i32_load(0);
                  local_get(keys); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::SWISS_MAP, super::engine::layout::map::TAGS) as i32); i32_add;
                  local_get(j); i32_const(4); i32_mul; i32_add;
                  i32_load(0);
                  call(self.emitter.rt.string.eq);
                  if_empty;
                    i32_const(1); local_set(found);
                    br(2);
                  end;
                  local_get(j); i32_const(1); i32_add; local_set(j);
                  br(0);
                end; end;
        });
        if is_pick {
            wasm!(self.func, { local_get(found); });
        } else {
            wasm!(self.func, { local_get(found); i32_eqz; });
        }
        wasm!(self.func, {
                if_empty;
                  local_get(new_list); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::SWISS_MAP, super::engine::layout::map::TAGS) as i32); i32_add;
                  local_get(count); i32_const(4); i32_mul; i32_add;
                  // SHARE: the copied pair is now reachable from the source
                  // object AND the result — own a +1 per copy (#668 class).
                  local_get(pair_ptr); call(self.emitter.rt.rc_inc); i32_store(0);
                  local_get(count); i32_const(1); i32_add; local_set(count);
                end;
                local_get(i); i32_const(1); i32_add; local_set(i);
                br(0);
              end; end;
              // Build result object
              i32_const(8); call(self.emitter.rt.alloc); local_set(result);
              local_get(result); i32_const(6); i32_store(0);
              local_get(result); local_get(new_list); i32_store(4);
              local_get(result);
            end;
        });

        self.scratch.free_i32(result);
        self.scratch.free_i32(new_list);
        self.scratch.free_i32(count);
        self.scratch.free_i32(found);
        self.scratch.free_i32(pair_ptr);
        self.scratch.free_i32(j);
        self.scratch.free_i32(i);
        self.scratch.free_i32(keys_len);
        self.scratch.free_i32(old_len);
        self.scratch.free_i32(old_list);
        self.scratch.free_i32(keys);
        self.scratch.free_i32(v);
    }

    /// value.merge(a: Value, b: Value) -> Value
    ///
    /// Mirrors the native oracle `almide_rt_value_merge` exactly: for two
    /// Objects, a's keys keep a's POSITIONS (a key overridden by b keeps its
    /// slot with b's value), then b's NEW keys append in b's order; when either
    /// side is not an Object, the result is `b` (native `(_, b) => b.clone()`).
    /// Every shared key/value/pair owns a +1 (#668 class), and the pair list
    /// uses the full `[len][cap][data]` layout.
    fn emit_value_merge(&mut self, args: &[IrExpr]) {
        let dat = self.emitter.layout_reg.fixed_offset(super::engine::layout::SWISS_MAP, super::engine::layout::map::TAGS) as i32;
        let a = self.scratch.alloc_i32();
        let b = self.scratch.alloc_i32();
        let a_list = self.scratch.alloc_i32();
        let b_list = self.scratch.alloc_i32();
        let a_len = self.scratch.alloc_i32();
        let b_len = self.scratch.alloc_i32();
        let new_list = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let j = self.scratch.alloc_i32();
        let count = self.scratch.alloc_i32();
        let pair_ptr = self.scratch.alloc_i32();
        let found = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();

        self.emit_expr(&args[0]);
        wasm!(self.func, { local_set(a); });
        self.emit_expr(&args[1]);
        wasm!(self.func, {
            local_set(b);
            // Non-object on either side → b (native `(_, b) => b.clone()`).
            // SHARE: hands back the INPUT value — own a +1 (#668 class).
            local_get(a); i32_load(0); i32_const(6); i32_ne;
            local_get(b); i32_load(0); i32_const(6); i32_ne;
            i32_or;
            if_i32;
              local_get(b); call(self.emitter.rt.rc_inc);
            else_;
              local_get(a); i32_load(4); local_set(a_list);
              local_get(b); i32_load(4); local_set(b_list);
              local_get(a_list); i32_load(0); local_set(a_len);
              local_get(b_list); i32_load(0); local_set(b_len);
              // Pass 1: count = a_len + (b keys NOT in a).
              local_get(a_len); local_set(count);
              i32_const(0); local_set(j);
              block_empty; loop_empty;
                local_get(j); local_get(b_len); i32_ge_u; br_if(1);
                local_get(b_list); i32_const(dat); i32_add;
                local_get(j); i32_const(4); i32_mul; i32_add;
                i32_load(0); local_set(pair_ptr); // b pair
                i32_const(0); local_set(found);
                i32_const(0); local_set(i);
                block_empty; loop_empty;
                  local_get(i); local_get(a_len); i32_ge_u; br_if(1);
                  local_get(pair_ptr); i32_load(0);
                  local_get(a_list); i32_const(dat); i32_add;
                  local_get(i); i32_const(4); i32_mul; i32_add;
                  i32_load(0); i32_load(0); // a pair key
                  call(self.emitter.rt.string.eq);
                  if_empty; i32_const(1); local_set(found); br(2); end;
                  local_get(i); i32_const(1); i32_add; local_set(i);
                  br(0);
                end; end;
                local_get(found); i32_eqz;
                if_empty; local_get(count); i32_const(1); i32_add; local_set(count); end;
                local_get(j); i32_const(1); i32_add; local_set(j);
                br(0);
              end; end;
              // Alloc the result pair list — full [len][cap][data] layout.
              i32_const(8); local_get(count); i32_const(4); i32_mul; i32_add;
              call(self.emitter.rt.alloc); local_set(new_list);
              local_get(new_list); local_get(count); i32_store(0);
              local_get(new_list); local_get(count); i32_store(4);
              // Pass 2: a's keys in a's positions; a key present in b takes a
              // FRESH pair (a's key, b's value), each half owning a +1.
              i32_const(0); local_set(i);
              block_empty; loop_empty;
                local_get(i); local_get(a_len); i32_ge_u; br_if(1);
                local_get(a_list); i32_const(dat); i32_add;
                local_get(i); i32_const(4); i32_mul; i32_add;
                i32_load(0); local_set(pair_ptr); // a pair
                // Scan b for the same key.
                i32_const(0); local_set(found);
                i32_const(0); local_set(j);
                block_empty; loop_empty;
                  local_get(j); local_get(b_len); i32_ge_u; br_if(1);
                  local_get(pair_ptr); i32_load(0);
                  local_get(b_list); i32_const(dat); i32_add;
                  local_get(j); i32_const(4); i32_mul; i32_add;
                  i32_load(0); i32_load(0); // b pair key
                  call(self.emitter.rt.string.eq);
                  if_empty; i32_const(1); local_set(found); br(2); end;
                  local_get(j); i32_const(1); i32_add; local_set(j);
                  br(0);
                end; end;
                local_get(new_list); i32_const(dat); i32_add;
                local_get(i); i32_const(4); i32_mul; i32_add;
                local_get(found);
                if_i32;
                  // Fresh pair [a.key +1][b[j].value +1] — j stopped at the hit.
                  i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                  local_get(result);
                  local_get(pair_ptr); i32_load(0); call(self.emitter.rt.rc_inc);
                  i32_store(0);
                  local_get(result);
                  local_get(b_list); i32_const(dat); i32_add;
                  local_get(j); i32_const(4); i32_mul; i32_add;
                  i32_load(0); i32_load(4); call(self.emitter.rt.rc_inc);
                  i32_store(4);
                  local_get(result);
                else_;
                  // SHARE: the pair stays owned by `a` too — own a +1.
                  local_get(pair_ptr); call(self.emitter.rt.rc_inc);
                end;
                i32_store(0);
                local_get(i); i32_const(1); i32_add; local_set(i);
                br(0);
              end; end;
              // Pass 3: b's NEW keys append in b's order.
              local_get(a_len); local_set(count);
              i32_const(0); local_set(j);
              block_empty; loop_empty;
                local_get(j); local_get(b_len); i32_ge_u; br_if(1);
                local_get(b_list); i32_const(dat); i32_add;
                local_get(j); i32_const(4); i32_mul; i32_add;
                i32_load(0); local_set(pair_ptr); // b pair
                i32_const(0); local_set(found);
                i32_const(0); local_set(i);
                block_empty; loop_empty;
                  local_get(i); local_get(a_len); i32_ge_u; br_if(1);
                  local_get(pair_ptr); i32_load(0);
                  local_get(a_list); i32_const(dat); i32_add;
                  local_get(i); i32_const(4); i32_mul; i32_add;
                  i32_load(0); i32_load(0);
                  call(self.emitter.rt.string.eq);
                  if_empty; i32_const(1); local_set(found); br(2); end;
                  local_get(i); i32_const(1); i32_add; local_set(i);
                  br(0);
                end; end;
                local_get(found); i32_eqz;
                if_empty;
                  local_get(new_list); i32_const(dat); i32_add;
                  local_get(count); i32_const(4); i32_mul; i32_add;
                  // SHARE: the appended pair stays owned by `b` too.
                  local_get(pair_ptr); call(self.emitter.rt.rc_inc); i32_store(0);
                  local_get(count); i32_const(1); i32_add; local_set(count);
                end;
                local_get(j); i32_const(1); i32_add; local_set(j);
                br(0);
              end; end;
              // Build result box.
              i32_const(8); call(self.emitter.rt.alloc); local_set(result);
              local_get(result); i32_const(6); i32_store(0);
              local_get(result); local_get(new_list); i32_store(4);
              local_get(result);
            end;
        });

        self.scratch.free_i32(result);
        self.scratch.free_i32(found);
        self.scratch.free_i32(pair_ptr);
        self.scratch.free_i32(count);
        self.scratch.free_i32(j);
        self.scratch.free_i32(i);
        self.scratch.free_i32(new_list);
        self.scratch.free_i32(b_len);
        self.scratch.free_i32(a_len);
        self.scratch.free_i32(b_list);
        self.scratch.free_i32(a_list);
        self.scratch.free_i32(b);
        self.scratch.free_i32(a);
    }

    // ── Codec helper functions ──────────────────────────────────────

    /// Dispatch __encode_option_*, __decode_option_*, __decode_default_*, __encode_list_*, __decode_list_*
    pub(super) fn emit_codec_helper(&mut self, name: &str, args: &[IrExpr]) {
        if let Some(suffix) = name.strip_prefix("__encode_option_") {
            self.emit_encode_option(args, suffix);
        } else if let Some(suffix) = name.strip_prefix("__decode_option_") {
            self.emit_decode_option(args, suffix);
        } else if let Some(suffix) = name.strip_prefix("__decode_default_") {
            self.emit_decode_default(args, suffix);
        } else if let Some(suffix) = name.strip_prefix("__encode_list_") {
            self.emit_encode_list(args, suffix);
        } else if let Some(suffix) = name.strip_prefix("__decode_list_") {
            self.emit_decode_list(args, suffix);
        } else {
            for arg in args { self.emit_expr(arg); }
            wasm!(self.func, { unreachable; });
        }
    }

    /// __encode_option_T(opt: Option[T]) -> Value
    /// None -> value.null, Some(v) -> value.T(v)
    fn emit_encode_option(&mut self, args: &[IrExpr], suffix: &str) {
        let opt = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();
        self.emit_expr(&args[0]);
        wasm!(self.func, {
            local_set(opt);
            // Option layout: ptr==0 → None, ptr!=0 → Some (payload at offset 0, no tag)
            local_get(opt); i32_const(0); i32_ne;
            if_i32;
        });
        // Some: wrap payload as Value
        match suffix {
            "string" => {
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); i32_const(4); i32_store(0); // tag = string
                    local_get(result); local_get(opt); i32_load(0); i32_store(4); // payload at offset 0
                    local_get(result);
                });
            }
            "int" => {
                wasm!(self.func, {
                    i32_const(12); call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); i32_const(2); i32_store(0); // tag = int
                    local_get(result); local_get(opt); i64_load(0); i64_store(4); // payload at offset 0
                    local_get(result);
                });
            }
            "float" => {
                wasm!(self.func, {
                    i32_const(12); call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); i32_const(3); i32_store(0); // tag = float
                    local_get(result); local_get(opt); f64_load(0); f64_store(4); // payload at offset 0
                    local_get(result);
                });
            }
            "bool" => {
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); i32_const(1); i32_store(0); // tag = bool
                    local_get(result); local_get(opt); i32_load(0); i32_store(4); // payload at offset 0
                    local_get(result);
                });
            }
            _ => {
                // Named type: payload is a pointer at offset 0
                wasm!(self.func, {
                    local_get(opt); i32_load(0);
                });
                let encode_name = format!("{}.encode", suffix);
                if let Some(&func_idx) = self.emitter.func_map.get(encode_name.as_str()) {
                    wasm!(self.func, { call(func_idx); });
                } else {
                    wasm!(self.func, { drop; i32_const(4); call(self.emitter.rt.alloc); local_set(result); local_get(result); i32_const(0); i32_store(0); local_get(result); });
                }
            }
        }
        wasm!(self.func, {
            else_;
              // None: return value.null
              i32_const(4); call(self.emitter.rt.alloc); local_set(result);
              local_get(result); i32_const(0); i32_store(0); // tag = null
              local_get(result);
            end;
        });
        self.scratch.free_i32(result);
        self.scratch.free_i32(opt);
    }

    /// __decode_option_T(v: Value, key: String) -> Result[Option[T], String]
    /// Missing/null → ok(None), present → ok(Some(as_T(field)?))
    fn emit_decode_option(&mut self, args: &[IrExpr], suffix: &str) {
        // Step 1: look up key in object — re-implement value.field inline
        // but treat missing/null as ok(None) instead of err
        let v = self.scratch.alloc_i32();
        let key = self.scratch.alloc_i32();
        let list = self.scratch.alloc_i32();
        let len = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let pair_ptr = self.scratch.alloc_i32();
        let found = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();

        self.emit_expr(&args[0]); // v: Value
        wasm!(self.func, { local_set(v); });
        self.emit_expr(&args[1]); // key: String
        wasm!(self.func, {
            local_set(key);
            // v must be object
            local_get(v); i32_load(0); i32_const(6); i32_ne;
            if_i32;
        });
        let err_msg = self.emitter.intern_string("expected Object");
        wasm!(self.func, {
              i32_const(8); call(self.emitter.rt.alloc); local_set(result);
              local_get(result); i32_const(1); i32_store(0); // err
              local_get(result); i32_const(err_msg as i32); i32_store(4);
              local_get(result);
            else_;
              local_get(v); i32_load(4); local_set(list);
              local_get(list); i32_load(0); local_set(len);
              i32_const(0); local_set(i);
              i32_const(0); local_set(found); // 0 = not found
              block_empty; loop_empty;
                local_get(i); local_get(len); i32_ge_u; br_if(1);
                local_get(list); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                local_get(i); i32_const(4); i32_mul; i32_add;
                i32_load(0); local_set(pair_ptr);
                local_get(pair_ptr); i32_load(0);
                local_get(key);
                call(self.emitter.rt.string.eq);
                if_empty;
                  local_get(pair_ptr); i32_load(4); local_set(found); // found = value ptr
                  br(2);
                end;
                local_get(i); i32_const(1); i32_add; local_set(i);
                br(0);
              end; end;
              // found == 0 → missing → ok(None)
              // found != 0 && tag == 0 (null) → ok(None)
              // found != 0 && tag != 0 → ok(Some(as_T(found)?))
              // Use nested if to avoid loading from null pointer
              local_get(found); i32_eqz;
              if_i32;
                i32_const(1); // use_none = true
              else_;
                local_get(found); i32_load(0); i32_eqz; // tag == null?
              end;
              if_i32;
                // Missing or null → ok(None)
                // Option None = [tag=0], wrapped in Result ok: [tag=0][option_ptr]
        });
        // Build None Option (= null pointer 0) wrapped in ok Result
        let none_opt = self.scratch.alloc_i32();
        wasm!(self.func, {
                // Option None = pointer 0
                // Wrap in ok Result: [tag=0][payload=0]
                i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                local_get(result); i32_const(0); i32_store(0); // Result ok
                local_get(result); i32_const(0); i32_store(4); // Option None = 0
                local_get(result);
              else_;
                // Found value with non-null type → extract and wrap in Some
        });
        // Type-specific extraction: check tag, get payload
        let some_opt = self.scratch.alloc_i32();
        match suffix {
            "string" => {
                // Value tag 4 = string, payload i32 at +4
                // Option[String] Some = heap ptr → [string_ptr:i32] (no tag, ptr != 0 = Some)
                wasm!(self.func, {
                    local_get(found); i32_load(0); i32_const(4); i32_ne;
                    if_i32;
                });
                let type_err = self.emitter.intern_string("expected Str");
                wasm!(self.func, {
                        i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                        local_get(result); i32_const(1); i32_store(0);
                        local_get(result); i32_const(type_err as i32); i32_store(4);
                        local_get(result);
                    else_;
                        // Some: alloc payload only (no tag)
                        i32_const(4); call(self.emitter.rt.alloc); local_set(some_opt);
                        local_get(some_opt); local_get(found); i32_load(4); i32_store(0); // string ptr
                        i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                        local_get(result); i32_const(0); i32_store(0); // ok
                        local_get(result); local_get(some_opt); i32_store(4);
                        local_get(result);
                    end;
                });
            }
            "int" => {
                wasm!(self.func, {
                    local_get(found); i32_load(0); i32_const(2); i32_ne;
                    if_i32;
                });
                let type_err = self.emitter.intern_string("expected Int");
                wasm!(self.func, {
                        i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                        local_get(result); i32_const(1); i32_store(0);
                        local_get(result); i32_const(type_err as i32); i32_store(4);
                        local_get(result);
                    else_;
                        i32_const(8); call(self.emitter.rt.alloc); local_set(some_opt);
                        local_get(some_opt); local_get(found); i64_load(4); i64_store(0); // i64 payload
                        i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                        local_get(result); i32_const(0); i32_store(0);
                        local_get(result); local_get(some_opt); i32_store(4);
                        local_get(result);
                    end;
                });
            }
            "float" => {
                // #658: accept both Float (tag 3) and Int (tag 2, widened to f64).
                let type_err = self.emitter.intern_string("expected Float");
                wasm!(self.func, {
                    local_get(found); i32_load(0); i32_const(3); i32_eq;
                    if_i32;
                        i32_const(8); call(self.emitter.rt.alloc); local_set(some_opt);
                        local_get(some_opt); local_get(found); f64_load(4); f64_store(0);
                        i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                        local_get(result); i32_const(0); i32_store(0);
                        local_get(result); local_get(some_opt); i32_store(4);
                        local_get(result);
                    else_;
                      local_get(found); i32_load(0); i32_const(2); i32_eq;
                      if_i32;
                        i32_const(8); call(self.emitter.rt.alloc); local_set(some_opt);
                        local_get(some_opt); local_get(found); i64_load(4); f64_convert_i64_s; f64_store(0);
                        i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                        local_get(result); i32_const(0); i32_store(0);
                        local_get(result); local_get(some_opt); i32_store(4);
                        local_get(result);
                      else_;
                        i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                        local_get(result); i32_const(1); i32_store(0);
                        local_get(result); i32_const(type_err as i32); i32_store(4);
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
                let type_err = self.emitter.intern_string("expected Bool");
                wasm!(self.func, {
                        i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                        local_get(result); i32_const(1); i32_store(0);
                        local_get(result); i32_const(type_err as i32); i32_store(4);
                        local_get(result);
                    else_;
                        i32_const(4); call(self.emitter.rt.alloc); local_set(some_opt);
                        local_get(some_opt); local_get(found); i32_load(4); i32_store(0);
                        i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                        local_get(result); i32_const(0); i32_store(0);
                        local_get(result); local_get(some_opt); i32_store(4);
                        local_get(result);
                    end;
                });
            }
            _ => {
                // Named type: call Type.decode(found_value)
                wasm!(self.func, { local_get(found); });
                let decode_name = format!("{}.decode", suffix);
                if let Some(&func_idx) = self.emitter.func_map.get(decode_name.as_str()) {
                    // Type.decode returns Result[T, String]
                    // On ok, wrap in Some; on err, propagate
                    let decode_result = self.scratch.alloc_i32();
                    wasm!(self.func, {
                        call(func_idx); local_set(decode_result);
                        local_get(decode_result); i32_load(0); i32_const(0); i32_eq;
                        if_i32;
                          // Option Some = a block whose offset 0 IS the payload ptr
                          // (no tag — matches the encode/primitive layout). Storing a
                          // spurious tag here made the re-encode read `1` as the
                          // payload pointer → garbage (新②).
                          i32_const(4); call(self.emitter.rt.alloc); local_set(some_opt);
                          local_get(some_opt); local_get(decode_result); i32_load(4); i32_store(0);
                          i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                          local_get(result); i32_const(0); i32_store(0);
                          local_get(result); local_get(some_opt); i32_store(4);
                          local_get(result);
                        else_;
                          local_get(decode_result); // propagate err
                        end;
                    });
                    self.scratch.free_i32(decode_result);
                } else {
                    wasm!(self.func, { drop; unreachable; });
                }
            }
        }
        wasm!(self.func, {
              end;
            end;
        });

        self.scratch.free_i32(some_opt);
        self.scratch.free_i32(none_opt);
        self.scratch.free_i32(result);
        self.scratch.free_i32(found);
        self.scratch.free_i32(pair_ptr);
        self.scratch.free_i32(i);
        self.scratch.free_i32(len);
        self.scratch.free_i32(list);
        self.scratch.free_i32(key);
        self.scratch.free_i32(v);
    }
}
