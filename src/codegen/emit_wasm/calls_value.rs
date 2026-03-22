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
use crate::ir::IrExpr;

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
                self.emit_value_get(args);
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
            _ => {
                self.emit_stub_call(args);
            }
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
                self.emit_value_call("get", args);
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
                // For now, same as stringify (no indentation)
                self.emit_value_call("stringify", args);
            }
            _ => {
                self.emit_stub_call(args);
            }
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
              i32_const(16); call(self.emitter.rt.alloc); local_set(result);
              local_get(result); i32_const(0); i32_store(0); // ok
              local_get(result); local_get(v); i64_load(4); i64_store(8); // i64 payload at offset 8
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
}
