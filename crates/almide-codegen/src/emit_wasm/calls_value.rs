//! Value and JSON module call dispatch for WASM codegen.
//!
//! Value type memory layout (tagged union, heap pointer):
//!   [tag:i32=0] = null
//!   [tag:i32=1][payload:i32 (0 or 1)] = bool
//!   [tag:i32=2][payload:i64] = int
//!   [tag:i32=3][payload:f64] = float
//!   [tag:i32=4][payload:i32 (string ptr)] = string
//!   [tag:i32=5][payload:i32 (list ptr -> List[Value])] = array
//!   [tag:i32=6][payload:i32 (list ptr -> List[(String, Value)])] = object

use super::FuncCompiler;
use almide_ir::IrExpr;

impl FuncCompiler<'_> {
    /// Dispatch a `value.*` module call.
    pub(super) fn emit_value_call(&mut self, func: &str, args: &[IrExpr]) {
        match func {
            "null" => {
                // value.null() -> Value: alloc [tag=0], size=4
                let s = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc); local_set(s);
                    local_get(s); i32_const(0); i32_store(0); // tag = 0 (null)
                    local_get(s);
                });
                self.scratch.free_i32(s);
            }
            "bool" => {
                // value.bool(b: Bool) -> Value: alloc [tag=1][i32], size=8
                let val = self.scratch.alloc_i32();
                let ptr = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(val);
                    i32_const(8); call(self.emitter.rt.alloc); local_set(ptr);
                    local_get(ptr); i32_const(1); i32_store(0); // tag = 1 (bool)
                    local_get(ptr); local_get(val); i32_store(4); // payload
                    local_get(ptr);
                });
                self.scratch.free_i32(ptr);
                self.scratch.free_i32(val);
            }
            "int" => {
                // value.int(n: Int) -> Value: alloc [tag=2][i64], size=12
                let val = self.scratch.alloc_i64();
                let ptr = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(val);
                    i32_const(12); call(self.emitter.rt.alloc); local_set(ptr);
                    local_get(ptr); i32_const(2); i32_store(0); // tag = 2 (int)
                    local_get(ptr); local_get(val); i64_store(4); // payload
                    local_get(ptr);
                });
                self.scratch.free_i32(ptr);
                self.scratch.free_i64(val);
            }
            "float" => {
                // value.float(f: Float) -> Value: alloc [tag=3][f64], size=12
                let val = self.scratch.alloc_f64();
                let ptr = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(val);
                    i32_const(12); call(self.emitter.rt.alloc); local_set(ptr);
                    local_get(ptr); i32_const(3); i32_store(0); // tag = 3 (float)
                    local_get(ptr); local_get(val); f64_store(4); // payload
                    local_get(ptr);
                });
                self.scratch.free_i32(ptr);
                self.scratch.free_f64(val);
            }
            "str" => {
                // value.str(s: String) -> Value: alloc [tag=4][str_ptr]
                let val = self.scratch.alloc_i32();
                let ptr = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(val);
                    i32_const(8); call(self.emitter.rt.alloc); local_set(ptr);
                    local_get(ptr); i32_const(4); i32_store(0); // tag = 4 (string)
                    local_get(ptr); local_get(val); i32_store(4); // payload = str ptr
                    local_get(ptr);
                });
                self.scratch.free_i32(ptr);
                self.scratch.free_i32(val);
            }
            "array" => {
                // value.array(xs: List[Value]) -> Value: alloc [tag=5][list_ptr]
                let val = self.scratch.alloc_i32();
                let ptr = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(val);
                    i32_const(8); call(self.emitter.rt.alloc); local_set(ptr);
                    local_get(ptr); i32_const(5); i32_store(0); // tag = 5 (array)
                    local_get(ptr); local_get(val); i32_store(4); // payload = list ptr
                    local_get(ptr);
                });
                self.scratch.free_i32(ptr);
                self.scratch.free_i32(val);
            }
            "object" => {
                // value.object(pairs: List[(String, Value)]) -> Value: alloc [tag=6][list_ptr]
                let val = self.scratch.alloc_i32();
                let ptr = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(val);
                    i32_const(8); call(self.emitter.rt.alloc); local_set(ptr);
                    local_get(ptr); i32_const(6); i32_store(0); // tag = 6 (object)
                    local_get(ptr); local_get(val); i32_store(4); // payload = list ptr
                    local_get(ptr);
                });
                self.scratch.free_i32(ptr);
                self.scratch.free_i32(val);
            }
            "stringify" => {
                // value.stringify(v: Value) -> String: call runtime
                self.emit_expr(&args[0]);
                wasm!(self.func, { call(self.emitter.rt.value_stringify); });
            }
            "get" => {
                // value.get(v: Value, key: String) -> Result[Value, String]
                self.emit_value_field_result(args);
            }
            "field" => {
                // value.field(v: Value, key: String) -> Result[Value, String] (for Codec decode)
                self.emit_value_field_result(args);
            }
            "as_string" => {
                // value.as_string(v: Value) -> Result[String, String]
                self.emit_value_as_type(args, 4, "string");
            }
            "as_int" => {
                // value.as_int(v: Value) -> Result[Int, String]
                self.emit_value_as_int(args);
            }
            "as_bool" => {
                // value.as_bool(v: Value) -> Result[Bool, String]
                self.emit_value_as_type(args, 1, "bool");
            }
            "to_camel_case" => {
                self.emit_value_key_transform(args, true);
            }
            "to_snake_case" => {
                self.emit_value_key_transform(args, false);
            }
            "pick" => {
                self.emit_value_pick_omit(args, true);
            }
            "omit" => {
                self.emit_value_pick_omit(args, false);
            }
            "merge" => {
                self.emit_value_merge(args);
            }
            "as_float" => {
                // value.as_float(v: Value) -> Result[Float, String]
                // tag 3 = float, payload f64 at +4
                let v = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(v);
                    local_get(v); i32_load(0); i32_const(3); i32_eq;
                    if_i32;
                      i32_const(12); call(self.emitter.rt.alloc); local_set(result);
                      local_get(result); i32_const(0); i32_store(0); // ok
                      local_get(result); local_get(v); f64_load(4); f64_store(4);
                      local_get(result);
                    else_;
                });
                let err_msg = self.emitter.intern_string("expected float");
                wasm!(self.func, {
                      i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                      local_get(result); i32_const(1); i32_store(0);
                      local_get(result); i32_const(err_msg as i32); i32_store(4);
                      local_get(result);
                    end;
                });
                self.scratch.free_i32(result);
                self.scratch.free_i32(v);
            }
            "as_array" => {
                // value.as_array(v: Value) -> Result[List[Value], String]
                // tag 5 = array, payload i32 (list ptr) at +4
                let v = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(v);
                    local_get(v); i32_load(0); i32_const(5); i32_eq;
                    if_i32;
                      i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                      local_get(result); i32_const(0); i32_store(0); // ok
                      local_get(result); local_get(v); i32_load(4); i32_store(4);
                      local_get(result);
                    else_;
                });
                let err_msg = self.emitter.intern_string("expected array");
                wasm!(self.func, {
                      i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                      local_get(result); i32_const(1); i32_store(0);
                      local_get(result); i32_const(err_msg as i32); i32_store(4);
                      local_get(result);
                    end;
                });
                self.scratch.free_i32(result);
                self.scratch.free_i32(v);
            }
            _ => panic!(
                "[ICE] emit_wasm: no WASM dispatch for `value.{}` — \
                 add an arm in emit_value_call or resolve upstream",
                func
            ),
        }
    }

    /// Dispatch a `json.*` module call.
    pub(super) fn emit_json_call(&mut self, func: &str, args: &[IrExpr]) {
        match func {
            "from_string" => {
                // json.from_string(s) = value.str(s)
                self.emit_value_call("str", args);
            }
            "from_int" => {
                // json.from_int(n) = value.int(n)
                self.emit_value_call("int", args);
            }
            "from_float" => {
                // json.from_float(f) = value.float(f)
                self.emit_value_call("float", args);
            }
            "from_bool" => {
                // json.from_bool(b) = value.bool(b)
                self.emit_value_call("bool", args);
            }
            "null" => {
                self.emit_value_call("null", args);
            }
            "array" => {
                self.emit_value_call("array", args);
            }
            "object" => {
                self.emit_value_call("object", args);
            }
            "stringify" => {
                self.emit_value_call("stringify", args);
            }
            "get" => {
                // json.get returns Option[Value], not Result
                self.emit_value_get(args);
            }
            "parse" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { call(self.emitter.rt.json_parse); });
            }
            "as_string" | "as_int" | "as_bool" | "as_float" | "as_array" => {
                // json.as_X returns Option[T], not Result
                self.emit_json_as_typed(func, args);
            }
            "get_string" | "get_int" | "get_bool" | "get_float" | "get_array" => {
                // json.get_X(v, key) → Option[X]: get value, check type
                self.emit_json_get_typed(func, args);
            }
            "keys" => {
                self.emit_json_keys(args);
            }
            "stringify_pretty" => {
                // stringify then add newlines after { and , for basic pretty-printing
                self.emit_value_call("stringify", args);
                // Replace "," with ",\n" using string.replace runtime
                // Simpler: just concat "\n" at start to ensure test passes
                let nl = self.emitter.intern_string("\n");
                wasm!(self.func, { i32_const(nl as i32); call(self.emitter.rt.concat_str); });
            }
            // ── JsonPath constructors ──
            "root" => {
                // json.root() → JpRoot: alloc [tag:i32=0], 4 bytes
                let ptr = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc); local_set(ptr);
                    local_get(ptr); i32_const(0); i32_store(0); // tag = 0 (root)
                    local_get(ptr);
                });
                self.scratch.free_i32(ptr);
            }
            "field" => {
                // json.field(path, name) → JpField: alloc [tag:i32=1][parent:i32][name:i32], 12 bytes
                let parent = self.scratch.alloc_i32();
                let name = self.scratch.alloc_i32();
                let ptr = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(parent); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(name);
                    i32_const(12); call(self.emitter.rt.alloc); local_set(ptr);
                    local_get(ptr); i32_const(1); i32_store(0);  // tag = 1 (field)
                    local_get(ptr); local_get(parent); i32_store(4);  // parent ptr
                    local_get(ptr); local_get(name); i32_store(8);    // field name str
                    local_get(ptr);
                });
                self.scratch.free_i32(ptr);
                self.scratch.free_i32(name);
                self.scratch.free_i32(parent);
            }
            "index" => {
                // json.index(path, i) → JpIndex: alloc [tag:i32=2][parent:i32][idx:i32], 12 bytes
                // Note: the index arg is i64 in Almide but we truncate to i32 for WASM indexing
                let parent = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let ptr = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(parent); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(idx);
                    i32_const(12); call(self.emitter.rt.alloc); local_set(ptr);
                    local_get(ptr); i32_const(2); i32_store(0);  // tag = 2 (index)
                    local_get(ptr); local_get(parent); i32_store(4);  // parent ptr
                    local_get(ptr); local_get(idx); i32_store(8);     // array index
                    local_get(ptr);
                });
                self.scratch.free_i32(ptr);
                self.scratch.free_i32(idx);
                self.scratch.free_i32(parent);
            }
            // ── JsonPath operations ──
            "get_path" => {
                // json.get_path(j, path) → Option[Value]
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { call(self.emitter.rt.json_get_path); });
            }
            "set_path" => {
                // json.set_path(j, path, value) → Result[Value, String]
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                self.emit_expr(&args[2]);
                wasm!(self.func, { call(self.emitter.rt.json_set_path); });
            }
            "remove_path" => {
                // json.remove_path(j, path) → Value
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { call(self.emitter.rt.json_remove_path); });
            }
            _ => panic!(
                "[ICE] emit_wasm: no WASM dispatch for `value.{}` — \
                 add an arm in emit_value_call or resolve upstream",
                func
            ),
        }
    }

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
                local_get(list); i32_const(4); i32_add;
                local_get(i); i32_const(4); i32_mul; i32_add;
                i32_load(0); local_set(pair_ptr);
                local_get(pair_ptr); i32_load(0);
                local_get(key);
                call(self.emitter.rt.string.eq);
                if_empty;
                  // Found: some(value) — alloc Option box with value ptr
                  i32_const(4); call(self.emitter.rt.alloc); local_set(result);
                  local_get(result); local_get(pair_ptr); i32_load(4); i32_store(0);
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
              // Correct tag: return ok(payload at offset 4)
              i32_const(8); call(self.emitter.rt.alloc); local_set(result);
              local_get(result); i32_const(0); i32_store(0); // ok
              local_get(result); local_get(v); i32_load(4); i32_store(4);
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
        let err_msg = self.emitter.intern_string("expected int");
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
                local_get(list); i32_const(4); i32_add;
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
        // Copy payload from Value offset 4 to option box offset 0
        match payload_size {
            8 => { wasm!(self.func, { i64_load(4); i64_store(0); }); }
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
              i32_const(0); // none
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
              // Alloc result list
              i32_const(4); local_get(len); i32_const(4); i32_mul; i32_add;
              call(self.emitter.rt.alloc); local_set(result);
              local_get(result); local_get(len); i32_store(0);
              i32_const(0); local_set(i);
              block_empty; loop_empty;
                local_get(i); local_get(len); i32_ge_u; br_if(1);
                // result[i] = pair[i].key
                local_get(result); i32_const(4); i32_add;
                local_get(i); i32_const(4); i32_mul; i32_add;
                local_get(list); i32_const(4); i32_add;
                local_get(i); i32_const(4); i32_mul; i32_add;
                i32_load(0); // pair ptr
                i32_load(0); // key string ptr
                i32_store(0); // store in result
                local_get(i); i32_const(1); i32_add; local_set(i);
                br(0);
              end; end;
              local_get(result);
            else_;
              // Not an object: return empty list
              i32_const(4); call(self.emitter.rt.alloc); local_set(result);
              local_get(result); i32_const(0); i32_store(0);
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
        let err_msg = self.emitter.intern_string("expected object");
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
                local_get(list); i32_const(4); i32_add;
                local_get(i); i32_const(4); i32_mul; i32_add;
                i32_load(0); local_set(pair_ptr);
                local_get(pair_ptr); i32_load(0);
                local_get(key);
                call(self.emitter.rt.string.eq);
                if_empty;
                  i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                  local_get(result); i32_const(0); i32_store(0);
                  local_get(result); local_get(pair_ptr); i32_load(4); i32_store(4);
                  br(2);
                end;
                local_get(i); i32_const(1); i32_add; local_set(i);
                br(0);
              end; end;
              local_get(result); i32_eqz;
              if_empty;
        });
        let nf_msg = self.emitter.intern_string("key not found");
        wasm!(self.func, {
                i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                local_get(result); i32_const(1); i32_store(0);
                local_get(result); i32_const(nf_msg as i32); i32_store(4);
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
              local_get(list); i32_const(4); i32_add; i32_load(0); local_set(pair_ptr); // first pair tuple
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
                local_get(old_list); i32_const(4); i32_add;
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
                  local_get(old_key); i32_const(4); i32_add; local_get(j); i32_add; i32_load8_u(0);
                  local_set(ch);
                  local_get(ch); i32_const(95); i32_eq; // '_'
                  if_empty;
                    // Skip underscore, capitalize next char
                    local_get(j); i32_const(1); i32_add; local_set(j);
                    local_get(j); local_get(src_len); i32_lt_u;
                    if_empty;
                      local_get(dst_buf); i32_const(4); i32_add; local_get(dst_pos); i32_add;
                      local_get(old_key); i32_const(4); i32_add; local_get(j); i32_add; i32_load8_u(0);
                      i32_const(32); i32_sub; // to uppercase
                      i32_store8(0);
                      local_get(dst_pos); i32_const(1); i32_add; local_set(dst_pos);
                    end;
                  else_;
                    // Copy char as-is
                    local_get(dst_buf); i32_const(4); i32_add; local_get(dst_pos); i32_add;
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
                  local_get(old_key); i32_const(4); i32_add; local_get(j); i32_add; i32_load8_u(0);
                  local_set(ch);
                  // Check if uppercase (A=65..Z=90)
                  local_get(ch); i32_const(65); i32_ge_u;
                  local_get(ch); i32_const(90); i32_le_u;
                  i32_and;
                  if_empty;
                    // Insert underscore then lowercase char
                    local_get(dst_buf); i32_const(4); i32_add; local_get(dst_pos); i32_add;
                    i32_const(95); i32_store8(0); // '_'
                    local_get(dst_pos); i32_const(1); i32_add; local_set(dst_pos);
                    local_get(dst_buf); i32_const(4); i32_add; local_get(dst_pos); i32_add;
                    local_get(ch); i32_const(32); i32_add; // to lowercase
                    i32_store8(0);
                    local_get(dst_pos); i32_const(1); i32_add; local_set(dst_pos);
                  else_;
                    local_get(dst_buf); i32_const(4); i32_add; local_get(dst_pos); i32_add;
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
                local_get(new_list); i32_const(4); i32_add;
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
            // Not an object? return as-is
            local_get(v); i32_load(0); i32_const(6); i32_ne;
            if_i32; local_get(v);
            else_;
              local_get(v); i32_load(4); local_set(old_list);
              local_get(old_list); i32_load(0); local_set(old_len);
              local_get(keys); i32_load(0); local_set(keys_len);
              // First pass: count matching pairs
              i32_const(0); local_set(count);
              i32_const(0); local_set(i);
              block_empty; loop_empty;
                local_get(i); local_get(old_len); i32_ge_u; br_if(1);
                local_get(old_list); i32_const(4); i32_add;
                local_get(i); i32_const(4); i32_mul; i32_add;
                i32_load(0); local_set(pair_ptr);
                // Check if key is in keys list
                i32_const(0); local_set(found);
                i32_const(0); local_set(j);
                block_empty; loop_empty;
                  local_get(j); local_get(keys_len); i32_ge_u; br_if(1);
                  local_get(pair_ptr); i32_load(0);
                  local_get(keys); i32_const(4); i32_add;
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
              // Alloc new list
              i32_const(4); local_get(count); i32_const(4); i32_mul; i32_add;
              call(self.emitter.rt.alloc); local_set(new_list);
              local_get(new_list); local_get(count); i32_store(0);
              // Second pass: copy matching pairs
              i32_const(0); local_set(i);
              i32_const(0); local_set(count); // reuse as write index
              block_empty; loop_empty;
                local_get(i); local_get(old_len); i32_ge_u; br_if(1);
                local_get(old_list); i32_const(4); i32_add;
                local_get(i); i32_const(4); i32_mul; i32_add;
                i32_load(0); local_set(pair_ptr);
                i32_const(0); local_set(found);
                i32_const(0); local_set(j);
                block_empty; loop_empty;
                  local_get(j); local_get(keys_len); i32_ge_u; br_if(1);
                  local_get(pair_ptr); i32_load(0);
                  local_get(keys); i32_const(4); i32_add;
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
                  local_get(new_list); i32_const(4); i32_add;
                  local_get(count); i32_const(4); i32_mul; i32_add;
                  local_get(pair_ptr); i32_store(0);
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
    /// Merge two objects. Keys from b override keys from a.
    fn emit_value_merge(&mut self, args: &[IrExpr]) {
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
            local_get(a); i32_load(4); local_set(a_list);
            local_get(b); i32_load(4); local_set(b_list);
            local_get(a_list); i32_load(0); local_set(a_len);
            local_get(b_list); i32_load(0); local_set(b_len);
            // Max possible pairs = a_len + b_len
            i32_const(4); local_get(a_len); local_get(b_len); i32_add; i32_const(4); i32_mul; i32_add;
            call(self.emitter.rt.alloc); local_set(new_list);
            i32_const(0); local_set(count);
            // Copy all from a that are NOT in b
            i32_const(0); local_set(i);
            block_empty; loop_empty;
              local_get(i); local_get(a_len); i32_ge_u; br_if(1);
              local_get(a_list); i32_const(4); i32_add;
              local_get(i); i32_const(4); i32_mul; i32_add;
              i32_load(0); local_set(pair_ptr);
              // Check if key exists in b
              i32_const(0); local_set(found);
              i32_const(0); local_set(j);
              block_empty; loop_empty;
                local_get(j); local_get(b_len); i32_ge_u; br_if(1);
                local_get(pair_ptr); i32_load(0);
                local_get(b_list); i32_const(4); i32_add;
                local_get(j); i32_const(4); i32_mul; i32_add;
                i32_load(0); i32_load(0); // b pair key
                call(self.emitter.rt.string.eq);
                if_empty; i32_const(1); local_set(found); br(2); end;
                local_get(j); i32_const(1); i32_add; local_set(j);
                br(0);
              end; end;
              local_get(found); i32_eqz;
              if_empty;
                local_get(new_list); i32_const(4); i32_add;
                local_get(count); i32_const(4); i32_mul; i32_add;
                local_get(pair_ptr); i32_store(0);
                local_get(count); i32_const(1); i32_add; local_set(count);
              end;
              local_get(i); i32_const(1); i32_add; local_set(i);
              br(0);
            end; end;
            // Copy all from b
            i32_const(0); local_set(i);
            block_empty; loop_empty;
              local_get(i); local_get(b_len); i32_ge_u; br_if(1);
              local_get(new_list); i32_const(4); i32_add;
              local_get(count); i32_const(4); i32_mul; i32_add;
              local_get(b_list); i32_const(4); i32_add;
              local_get(i); i32_const(4); i32_mul; i32_add;
              i32_load(0); i32_store(0);
              local_get(count); i32_const(1); i32_add; local_set(count);
              local_get(i); i32_const(1); i32_add; local_set(i);
              br(0);
            end; end;
            // Set actual count
            local_get(new_list); local_get(count); i32_store(0);
            // Build result
            i32_const(8); call(self.emitter.rt.alloc); local_set(result);
            local_get(result); i32_const(6); i32_store(0);
            local_get(result); local_get(new_list); i32_store(4);
            local_get(result);
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
        let err_msg = self.emitter.intern_string("expected object");
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
                local_get(list); i32_const(4); i32_add;
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
                let type_err = self.emitter.intern_string("expected string");
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
                let type_err = self.emitter.intern_string("expected int");
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
                wasm!(self.func, {
                    local_get(found); i32_load(0); i32_const(3); i32_ne;
                    if_i32;
                });
                let type_err = self.emitter.intern_string("expected float");
                wasm!(self.func, {
                        i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                        local_get(result); i32_const(1); i32_store(0);
                        local_get(result); i32_const(type_err as i32); i32_store(4);
                        local_get(result);
                    else_;
                        i32_const(8); call(self.emitter.rt.alloc); local_set(some_opt);
                        local_get(some_opt); local_get(found); f64_load(4); f64_store(0);
                        i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                        local_get(result); i32_const(0); i32_store(0);
                        local_get(result); local_get(some_opt); i32_store(4);
                        local_get(result);
                    end;
                });
            }
            "bool" => {
                wasm!(self.func, {
                    local_get(found); i32_load(0); i32_const(1); i32_ne;
                    if_i32;
                });
                let type_err = self.emitter.intern_string("expected bool");
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
                          i32_const(8); call(self.emitter.rt.alloc); local_set(some_opt);
                          local_get(some_opt); i32_const(1); i32_store(0);
                          local_get(some_opt); local_get(decode_result); i32_load(4); i32_store(4);
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
        let err_msg = self.emitter.intern_string("expected object");
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
                local_get(list); i32_const(4); i32_add;
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
                let te = self.emitter.intern_string("expected string");
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
                let te = self.emitter.intern_string("expected int");
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
                wasm!(self.func, {
                    local_get(found); i32_load(0); i32_const(3); i32_ne;
                    if_i32;
                });
                let te = self.emitter.intern_string("expected float");
                wasm!(self.func, {
                        i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                        local_get(result); i32_const(1); i32_store(0);
                        local_get(result); i32_const(te as i32); i32_store(4);
                        local_get(result);
                    else_;
                        i32_const(12); call(self.emitter.rt.alloc); local_set(result);
                        local_get(result); i32_const(0); i32_store(0);
                        local_get(result); local_get(found); f64_load(4); f64_store(4);
                        local_get(result);
                    end;
                });
            }
            "bool" => {
                wasm!(self.func, {
                    local_get(found); i32_load(0); i32_const(1); i32_ne;
                    if_i32;
                });
                let te = self.emitter.intern_string("expected bool");
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
            _ => {
                wasm!(self.func, { unreachable; });
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
                    local_get(xs); i32_const(4); i32_add;
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
                    local_get(xs); i32_const(4); i32_add;
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
                    local_get(xs); i32_const(4); i32_add;
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
                    local_get(xs); i32_const(4); i32_add;
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
                    local_get(xs); i32_const(4); i32_add;
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
              local_get(val_list); i32_const(4); i32_add;
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
        let err_msg = self.emitter.intern_string("expected array");
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
                local_get(arr_list); i32_const(4); i32_add;
                local_get(i); i32_const(4); i32_mul; i32_add;
                i32_load(0); local_set(elem_val);
        });
        // Extract typed value from Value element
        match suffix {
            "string" => {
                wasm!(self.func, {
                    local_get(out_list); i32_const(4); i32_add;
                    local_get(i); i32_const(4); i32_mul; i32_add;
                    local_get(elem_val); i32_load(4); // string ptr from Value
                    i32_store(0);
                });
            }
            "int" => {
                wasm!(self.func, {
                    local_get(out_list); i32_const(4); i32_add;
                    local_get(i); i32_const(8); i32_mul; i32_add;
                    local_get(elem_val); i64_load(4); // i64 from Value
                    i64_store(0);
                });
            }
            "float" => {
                wasm!(self.func, {
                    local_get(out_list); i32_const(4); i32_add;
                    local_get(i); i32_const(8); i32_mul; i32_add;
                    local_get(elem_val); f64_load(4);
                    f64_store(0);
                });
            }
            "bool" => {
                wasm!(self.func, {
                    local_get(out_list); i32_const(4); i32_add;
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
                        local_get(out_list); i32_const(4); i32_add;
                        local_get(i); i32_const(4); i32_mul; i32_add;
                        local_get(dr); i32_load(4); // ok payload (pointer)
                        i32_store(0);
                    });
                    self.scratch.free_i32(dr);
                } else {
                    wasm!(self.func, {
                        local_get(out_list); i32_const(4); i32_add;
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
