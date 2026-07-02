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
                self.emit_stored_field(&args[0]);
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
                self.emit_stored_field(&args[0]);
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
                self.emit_stored_field(&args[0]);
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
                // Error variant name mirrors the native Value::Str oracle (#657).
                self.emit_value_as_type(args, 4, "Str");
            }
            "as_int" => {
                // value.as_int(v: Value) -> Result[Int, String]
                self.emit_value_as_int(args);
            }
            "as_bool" => {
                // value.as_bool(v: Value) -> Result[Bool, String]
                self.emit_value_as_type(args, 1, "Bool");
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
                      // #658: a JSON integer is a valid Float — widen Int→f64 so
                      // Codec roundtrips stay total (mirrors native as_float).
                      local_get(v); i32_load(0); i32_const(2); i32_eq;
                      if_i32;
                        i32_const(12); call(self.emitter.rt.alloc); local_set(result);
                        local_get(result); i32_const(0); i32_store(0); // ok
                        local_get(result); local_get(v); i64_load(4); f64_convert_i64_s; f64_store(4);
                        local_get(result);
                      else_;
                });
                let err_msg = self.emitter.intern_string("expected Float");
                wasm!(self.func, {
                      i32_const(8); call(self.emitter.rt.alloc); local_set(result);
                      local_get(result); i32_const(1); i32_store(0);
                      local_get(result); i32_const(err_msg as i32); i32_store(4);
                      local_get(result);
                      end;
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
                      // SHARE: the payload list stays owned by the Value — the
                      // Result box must own its own +1 (#668 class), like
                      // emit_value_as_type's pointer-tag rule.
                      local_get(result); local_get(v); i32_load(4);
                      call(self.emitter.rt.rc_inc); i32_store(4);
                      local_get(result);
                    else_;
                });
                let err_msg = self.emitter.intern_string("expected Array");
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
            // value.keys shares json.keys' object-key extraction. Its native
            // intrinsic moved to the value runtime (`almide_rt_value_keys`) so a
            // value-only program links, which routed it here instead of the
            // RuntimeCall path — so dispatch it explicitly (#416).
            "keys" => self.emit_json_keys(args),
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
                // Real recursive pretty-printer mirroring the native oracle
                // (runtime/rs/src/json.rs stringify_value): 2-space indent per
                // depth, starting at depth 0. No trailing newline — println adds
                // exactly one, matching native.
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    i32_const(0);
                    call(self.emitter.rt.json_stringify_pretty);
                });
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
                // json.set_path(j, path, value) → Result[Value, String].
                // new_val is MOVE-stored into the rebuilt tree — stored-field
                // contract so an alias argument carries its own reference.
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                self.emit_stored_field(&args[2]);
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
}

include!("calls_value_p2.rs");
include!("calls_value_p3.rs");
include!("calls_value_p4.rs");
