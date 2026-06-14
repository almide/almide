//! http module — WASM codegen dispatch.

use crate::emit_wasm::engine::{Imm32, Imm64, Local};
use super::FuncCompiler;
use almide_ir::IrExpr;
use almide_lang::types::Ty;
use super::values;
use wasm_encoder::Instruction;

// Named constants for WASM immediate operands used in the HTTP emitter.
mod imm {
    /// Byte size of one i32 value / pointer (4 bytes).
    /// Also used as the stride when indexing pointer-arrays in list data.
    pub const I32_BYTES: i32 = 4;
    /// Byte size of an (i32, i32) tuple — two adjacent pointer fields.
    pub const TUPLE2_BYTES: i32 = 8;
    /// Byte size of the Response struct: status:i64 (8) + body:i32 (4) + headers:i32 (4).
    pub const RESP_BYTES: i32 = 16;
    /// HTTP 302 Found / Redirect status code.
    pub const HTTP_REDIRECT: i64 = 302;
    /// HTTP 404 Not Found status code.
    pub const HTTP_NOT_FOUND: i64 = 404;
}
use imm::*;

impl FuncCompiler<'_> {
    pub(super) fn emit_http_call(&mut self, func: &str, args: &[IrExpr]) {
        match func {
            "response" => {
                // http.response(status: Int, body: String) → Response
                let s = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(Imm32(RESP_BYTES)); call(self.emitter.rt.alloc); local_set(Local(s));
                    local_get(Local(s));
                });
                self.emit_expr(&args[0]); // status: i64
                wasm!(self.func, { i64_store(0); local_get(Local(s)); });
                self.emit_expr(&args[1]); // body: i32 str
                wasm!(self.func, {
                    i32_store(8);
                    // Empty headers list
                    local_get(Local(s));
                    i32_const(Imm32(I32_BYTES)); call(self.emitter.rt.alloc);
                });
                let empty_list = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_set(Local(empty_list));
                    local_get(Local(empty_list)); i32_const(Imm32(0)); i32_store(0);
                    local_get(Local(empty_list));
                    i32_store(12);
                    local_get(Local(s));
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
                    i32_const(Imm32(RESP_BYTES)); call(self.emitter.rt.alloc); local_set(Local(s));
                    local_get(Local(s));
                });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i64_store(0); local_get(Local(s)); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { i32_store(8); });
                // Build headers list with Content-Type: application/json
                let ct_key = self.emitter.intern_string("Content-Type");
                let ct_val = self.emitter.intern_string("application/json");
                wasm!(self.func, {
                    i32_const(Imm32(TUPLE2_BYTES)); call(self.emitter.rt.alloc); local_set(Local(tuple_ptr));
                    local_get(Local(tuple_ptr)); i32_const(Imm32(ct_key as i32)); i32_store(0);
                    local_get(Local(tuple_ptr)); i32_const(Imm32(ct_val as i32)); i32_store(4);
                    i32_const(Imm32(self.emitter.layout_reg.header_size(super::engine::layout::LIST) as i32 + I32_BYTES)); call(self.emitter.rt.alloc); local_set(Local(hdr_list));
                    local_get(Local(hdr_list)); i32_const(Imm32(1)); i32_store(0); // len = 1
                    local_get(Local(hdr_list)); local_get(Local(tuple_ptr)); i32_store(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32 as u32); // data[0] = tuple_ptr
                    local_get(Local(s)); local_get(Local(hdr_list)); i32_store(12);
                    local_get(Local(s));
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
                wasm!(self.func, { local_set(Local(resp)); });
                wasm!(self.func, {
                    i32_const(Imm32(RESP_BYTES)); call(self.emitter.rt.alloc); local_set(Local(new_resp));
                    local_get(Local(new_resp));
                });
                self.emit_expr(&args[1]); // new status: i64
                wasm!(self.func, {
                    i64_store(0);
                    local_get(Local(new_resp)); local_get(Local(resp)); i32_load(8); i32_store(8);
                    local_get(Local(new_resp)); local_get(Local(resp)); i32_load(12); i32_store(12);
                    local_get(Local(new_resp));
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
                // Option[String] layout convention: a pointer to an i32 cell
                // holding the string pointer (0 = None). We must allocate
                // and copy to match what `option.unwrap_or` / match arms
                // expect — a raw string pointer is NOT a valid Option.
                let resp = self.scratch.alloc_i32();
                let key = self.scratch.alloc_i32();
                let hdrs = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let pair_ptr = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let some_ptr = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(Local(resp)); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(Local(key));
                    i32_const(Imm32(0)); local_set(Local(result)); // default: none
                    local_get(Local(resp)); i32_load(12); local_set(Local(hdrs));
                    local_get(Local(hdrs)); i32_load(0); local_set(Local(len));
                    i32_const(Imm32(0)); local_set(Local(i));
                    block_empty; loop_empty;
                      local_get(Local(i)); local_get(Local(len)); i32_ge_u; br_if(1);
                      local_get(Local(hdrs)); i32_const(Imm32(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32)); i32_add;
                      local_get(Local(i)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                      i32_load(0); local_set(Local(pair_ptr));
                      local_get(Local(pair_ptr)); i32_load(0);
                      local_get(Local(key));
                      call(self.emitter.rt.string.eq);
                      if_empty;
                        // Found: build Some(string_ptr) by allocating a 4-byte
                        // cell and storing the value pointer inside.
                        i32_const(Imm32(I32_BYTES)); call(self.emitter.rt.alloc); local_set(Local(some_ptr));
                        local_get(Local(some_ptr)); local_get(Local(pair_ptr)); i32_load(4); i32_store(0);
                        local_get(Local(some_ptr)); local_set(Local(result));
                        br(2);
                      end;
                      local_get(Local(i)); i32_const(Imm32(1)); i32_add; local_set(Local(i));
                      br(0);
                    end; end;
                    local_get(Local(result));
                });
                self.scratch.free_i32(some_ptr);
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
                wasm!(self.func, { local_set(Local(resp)); });
                // Build new tuple (key, value)
                let key_scratch = self.scratch.alloc_i32();
                let val_scratch = self.scratch.alloc_i32();
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(Local(key_scratch)); });
                self.emit_expr(&args[2]);
                wasm!(self.func, {
                    local_set(Local(val_scratch));
                    i32_const(Imm32(TUPLE2_BYTES)); call(self.emitter.rt.alloc); local_set(Local(tuple_ptr));
                    local_get(Local(tuple_ptr)); local_get(Local(key_scratch)); i32_store(0);
                    local_get(Local(tuple_ptr)); local_get(Local(val_scratch)); i32_store(4);
                    // Copy old headers + append new
                    local_get(Local(resp)); i32_load(12); local_set(Local(old_hdrs));
                    local_get(Local(old_hdrs)); i32_load(0); i32_const(Imm32(1)); i32_add;
                    local_set(Local(val_scratch)); // reuse as new_len
                    i32_const(Imm32(self.emitter.layout_reg.header_size(super::engine::layout::LIST) as i32)); local_get(Local(val_scratch)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(Local(new_hdrs));
                    local_get(Local(new_hdrs)); local_get(Local(val_scratch)); i32_store(0);
                    // Copy old header ptrs
                    i32_const(Imm32(0)); local_set(Local(key_scratch)); // reuse as i
                    block_empty; loop_empty;
                      local_get(Local(key_scratch)); local_get(Local(old_hdrs)); i32_load(0); i32_ge_u; br_if(1);
                      local_get(Local(new_hdrs)); i32_const(Imm32(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32)); i32_add;
                      local_get(Local(key_scratch)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                      local_get(Local(old_hdrs)); i32_const(Imm32(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32)); i32_add;
                      local_get(Local(key_scratch)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                      i32_load(0); i32_store(0);
                      local_get(Local(key_scratch)); i32_const(Imm32(1)); i32_add; local_set(Local(key_scratch));
                      br(0);
                    end; end;
                    // Append new tuple at end
                    local_get(Local(new_hdrs)); i32_const(Imm32(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32)); i32_add;
                    local_get(Local(old_hdrs)); i32_load(0); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                    local_get(Local(tuple_ptr)); i32_store(0);
                    // Build new response
                    i32_const(Imm32(RESP_BYTES)); call(self.emitter.rt.alloc); local_set(Local(new_resp));
                    local_get(Local(new_resp)); local_get(Local(resp)); i64_load(0); i64_store(0);
                    local_get(Local(new_resp)); local_get(Local(resp)); i32_load(8); i32_store(8);
                    local_get(Local(new_resp)); local_get(Local(new_hdrs)); i32_store(12);
                    local_get(Local(new_resp));
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
                wasm!(self.func, { local_set(Local(resp)); });
                self.emit_expr(&args[1]);
                let eq_str = self.emitter.intern_string("=");
                wasm!(self.func, { i32_const(Imm32(eq_str as i32)); call(self.emitter.rt.concat_str); });
                self.emit_expr(&args[2]);
                wasm!(self.func, { call(self.emitter.rt.concat_str); local_set(Local(cookie_val)); });
                let cookie_key = self.emitter.intern_string("Set-Cookie");
                let tuple_ptr = self.scratch.alloc_i32();
                let new_hdrs = self.scratch.alloc_i32();
                let old_hdrs = self.scratch.alloc_i32();
                let new_resp = self.scratch.alloc_i32();
                let new_len = self.scratch.alloc_i32();
                let ci = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(Imm32(TUPLE2_BYTES)); call(self.emitter.rt.alloc); local_set(Local(tuple_ptr));
                    local_get(Local(tuple_ptr)); i32_const(Imm32(cookie_key as i32)); i32_store(0);
                    local_get(Local(tuple_ptr)); local_get(Local(cookie_val)); i32_store(4);
                    local_get(Local(resp)); i32_load(12); local_set(Local(old_hdrs));
                    local_get(Local(old_hdrs)); i32_load(0); i32_const(Imm32(1)); i32_add; local_set(Local(new_len));
                    i32_const(Imm32(self.emitter.layout_reg.header_size(super::engine::layout::LIST) as i32)); local_get(Local(new_len)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(Local(new_hdrs));
                    local_get(Local(new_hdrs)); local_get(Local(new_len)); i32_store(0);
                    i32_const(Imm32(0)); local_set(Local(ci));
                    block_empty; loop_empty;
                      local_get(Local(ci)); local_get(Local(old_hdrs)); i32_load(0); i32_ge_u; br_if(1);
                      local_get(Local(new_hdrs)); i32_const(Imm32(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32)); i32_add; local_get(Local(ci)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                      local_get(Local(old_hdrs)); i32_const(Imm32(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32)); i32_add; local_get(Local(ci)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                      i32_load(0); i32_store(0);
                      local_get(Local(ci)); i32_const(Imm32(1)); i32_add; local_set(Local(ci));
                      br(0);
                    end; end;
                    local_get(Local(new_hdrs)); i32_const(Imm32(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32)); i32_add;
                    local_get(Local(old_hdrs)); i32_load(0); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                    local_get(Local(tuple_ptr)); i32_store(0);
                    i32_const(Imm32(RESP_BYTES)); call(self.emitter.rt.alloc); local_set(Local(new_resp));
                    local_get(Local(new_resp)); local_get(Local(resp)); i64_load(0); i64_store(0);
                    local_get(Local(new_resp)); local_get(Local(resp)); i32_load(8); i32_store(8);
                    local_get(Local(new_resp)); local_get(Local(new_hdrs)); i32_store(12);
                    local_get(Local(new_resp));
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
                wasm!(self.func, { i32_const(Imm32(RESP_BYTES)); call(self.emitter.rt.alloc); local_set(Local(s)); local_get(Local(s)); });
                self.emit_expr(&args[0]); // status
                wasm!(self.func, { i64_store(0); local_get(Local(s)); });
                self.emit_expr(&args[1]); // body
                wasm!(self.func, { i32_store(8); });
                self.emit_expr(&args[2]); // headers map
                wasm!(self.func, {
                    local_set(Local(hdr_map));
                    local_get(Local(hdr_map)); i32_load(0); local_set(Local(len));
                    // Build headers list from Swiss Table map entries
                    i32_const(Imm32(self.emitter.layout_reg.header_size(super::engine::layout::LIST) as i32)); local_get(Local(len)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(Local(hdr_list));
                    local_get(Local(hdr_list)); local_get(Local(len)); i32_store(0);
                });
                // Dense COD walk: emit entries[0..len] in insertion order (output index == i).
                let (ks, vs) = self.map_kv_sizes(&args[2].ty);
                let es = ks + vs;
                let list_data_off = self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32;
                let map_cap = self.scratch.alloc_i32();
                let map_eb = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_get(Local(hdr_map)); i32_load(self.emitter.layout_reg.fixed_offset(super::engine::layout::SWISS_MAP, super::engine::layout::map::CAP)); local_set(Local(map_cap));
                });
                self.emit_dict_entries_base(hdr_map, map_cap);
                wasm!(self.func, {
                    local_set(Local(map_eb));
                    i32_const(Imm32(0)); local_set(Local(i));
                    block_empty; loop_empty;
                      local_get(Local(i)); local_get(Local(len)); i32_ge_u; br_if(1);
                      // tuple = copy of entries[i] (key@0, val@ks — es bytes contiguous)
                      i32_const(Imm32(es as i32)); call(self.emitter.rt.alloc); local_set(Local(tuple));
                      local_get(Local(tuple));
                      local_get(Local(map_eb)); local_get(Local(i)); i32_const(Imm32(es as i32)); i32_mul; i32_add;
                      i32_const(Imm32(es as i32)); memory_copy;
                      // hdr_list[i] = tuple
                      local_get(Local(hdr_list)); i32_const(Imm32(list_data_off)); i32_add;
                      local_get(Local(i)); i32_const(Imm32(I32_BYTES)); i32_mul; i32_add;
                      local_get(Local(tuple)); i32_store(0);
                      local_get(Local(i)); i32_const(Imm32(1)); i32_add; local_set(Local(i));
                      br(0);
                    end; end;
                    local_get(Local(s)); local_get(Local(hdr_list)); i32_store(12);
                    local_get(Local(s));
                });
                self.scratch.free_i32(map_eb);
                self.scratch.free_i32(map_cap);
                self.scratch.free_i32(tuple);
                self.scratch.free_i32(i);
                self.scratch.free_i32(len);
                self.scratch.free_i32(hdr_list);
                self.scratch.free_i32(hdr_map);
                self.scratch.free_i32(s);
            }
            "not_found" => {
                let s = self.scratch.alloc_i32();
                wasm!(self.func, { i32_const(Imm32(RESP_BYTES)); call(self.emitter.rt.alloc); local_set(Local(s)); local_get(Local(s)); i64_const(Imm64(HTTP_NOT_FOUND)); i64_store(0); local_get(Local(s)); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(8); });
                let empty = self.scratch.alloc_i32();
                wasm!(self.func, { i32_const(Imm32(I32_BYTES)); call(self.emitter.rt.alloc); local_set(Local(empty)); local_get(Local(empty)); i32_const(Imm32(0)); i32_store(0); local_get(Local(s)); local_get(Local(empty)); i32_store(12); local_get(Local(s)); });
                self.scratch.free_i32(empty);
                self.scratch.free_i32(s);
            }
            "redirect" => {
                let s = self.scratch.alloc_i32();
                let empty_body = self.emitter.intern_string("");
                wasm!(self.func, { i32_const(Imm32(RESP_BYTES)); call(self.emitter.rt.alloc); local_set(Local(s)); local_get(Local(s)); i64_const(Imm64(HTTP_REDIRECT)); i64_store(0); local_get(Local(s)); i32_const(Imm32(empty_body as i32)); i32_store(8); });
                let loc_key = self.emitter.intern_string("Location");
                let tuple = self.scratch.alloc_i32();
                let hdrs = self.scratch.alloc_i32();
                wasm!(self.func, { i32_const(Imm32(TUPLE2_BYTES)); call(self.emitter.rt.alloc); local_set(Local(tuple)); local_get(Local(tuple)); i32_const(Imm32(loc_key as i32)); i32_store(0); local_get(Local(tuple)); });
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    i32_store(4);
                    i32_const(Imm32(self.emitter.layout_reg.header_size(super::engine::layout::LIST) as i32 + I32_BYTES)); call(self.emitter.rt.alloc); local_set(Local(hdrs));
                    local_get(Local(hdrs)); i32_const(Imm32(1)); i32_store(0);
                    local_get(Local(hdrs)); local_get(Local(tuple)); i32_store(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32 as u32);
                    local_get(Local(s)); local_get(Local(hdrs)); i32_store(12); local_get(Local(s));
                });
                self.scratch.free_i32(hdrs);
                self.scratch.free_i32(tuple);
                self.scratch.free_i32(s);
            }
            _ => panic!(
                "[ICE] emit_wasm: no WASM dispatch for `http.{}` — \
                 add an arm in emit_http_call or resolve upstream",
                func
            ),
        }
    }

}
