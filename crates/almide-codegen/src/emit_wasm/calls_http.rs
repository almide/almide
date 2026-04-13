//! http module — WASM codegen dispatch.

use super::FuncCompiler;
use almide_ir::IrExpr;
use almide_lang::types::Ty;
use super::values;
use wasm_encoder::Instruction;

impl FuncCompiler<'_> {
    pub(super) fn emit_http_call(&mut self, func: &str, args: &[IrExpr]) {
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
                //
                // Inline emit: use `br` to exit the search loop, NOT `return_`.
                let resp = self.scratch.alloc_i32();
                let key = self.scratch.alloc_i32();
                let hdrs = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let pair_ptr = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(resp); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(key);
                    i32_const(0); local_set(result); // default: none
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
                        local_get(pair_ptr); i32_load(4); local_set(result);
                        br(2);
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result);
                });
                self.scratch.free_i32(result);
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

}
