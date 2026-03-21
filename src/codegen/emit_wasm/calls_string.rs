//! String stdlib call dispatch for WASM codegen.
//!
//! All `("string", _)` method call handlers live here.

use super::FuncCompiler;
use crate::ir::IrExpr;

impl FuncCompiler<'_> {
    /// Dispatch a string stdlib method call. Returns true if handled.
    pub(super) fn emit_string_call(
        &mut self,
        method: &str,
        args: &[IrExpr],
    ) -> bool {
        match method {
            "trim" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { call(self.emitter.rt.string.trim); });
            }
            "len" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_load(0); i64_extend_i32_u; });
            }
            "contains" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { call(self.emitter.rt.string.contains); });
            }
            "starts_with" => {
                let s = self.match_i32_base + self.match_depth;
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); i32_const(4); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0);
                    i32_const(4); i32_load(0); i32_load(0); // prefix.len
                    i32_const(0); i32_load(0); i32_load(0); // s.len
                    i32_gt_u;
                    if_i32; i32_const(0);
                    else_;
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      i32_const(4); i32_load(0); i32_const(4); i32_add;
                      i32_const(4); i32_load(0); i32_load(0);
                      call(self.emitter.rt.mem_eq);
                    end;
                });
            }
            "ends_with" => {
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); i32_const(4); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0);
                    i32_const(4); i32_load(0); i32_load(0);
                    i32_const(0); i32_load(0); i32_load(0);
                    i32_gt_u;
                    if_i32; i32_const(0);
                    else_;
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      i32_const(0); i32_load(0); i32_load(0); i32_add;
                      i32_const(4); i32_load(0); i32_load(0); i32_sub;
                      i32_const(4); i32_load(0); i32_const(4); i32_add;
                      i32_const(4); i32_load(0); i32_load(0);
                      call(self.emitter.rt.mem_eq);
                    end;
                });
            }
            "get" => {
                let s = self.match_i32_base + self.match_depth;
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(s);
                    i32_const(5); call(self.emitter.rt.alloc); local_set(s + 1);
                    local_get(s + 1); i32_const(1); i32_store(0);
                    local_get(s + 1);
                    i32_const(0); i32_load(0);
                    i32_const(4); i32_add; local_get(s); i32_add;
                    i32_load8_u(0); i32_store8(4);
                    local_get(s + 1);
                });
            }
            "reverse" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { call(self.emitter.rt.string.reverse); });
            }
            "repeat" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { i32_wrap_i64; call(self.emitter.rt.string.repeat); });
            }
            "replace" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                self.emit_expr(&args[2]);
                wasm!(self.func, { call(self.emitter.rt.string.replace); });
            }
            "split" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { call(self.emitter.rt.string.split); });
            }
            "join" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { call(self.emitter.rt.string.join); });
            }
            "slice" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { i32_wrap_i64; });
                if args.len() > 2 {
                    self.emit_expr(&args[2]);
                    wasm!(self.func, { i32_wrap_i64; });
                } else {
                    self.emit_expr(&args[0]);
                    wasm!(self.func, { i32_load(0); });
                }
                wasm!(self.func, { call(self.emitter.rt.string.slice); });
            }
            "index_of" => {
                let s64 = self.match_i64_base + self.match_depth;
                let s32 = self.match_i32_base + self.match_depth;
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    call(self.emitter.rt.string.index_of);
                    local_set(s64);
                    local_get(s64); i64_const(-1i64 as i64); i64_eq;
                    if_i32;
                      i32_const(0);
                    else_;
                      i32_const(8); call(self.emitter.rt.alloc); local_set(s32);
                      local_get(s32); local_get(s64); i64_store(0);
                      local_get(s32);
                    end;
                });
            }
            "last_index_of" => {
                let s64 = self.match_i64_base + self.match_depth;
                let s32 = self.match_i32_base + self.match_depth;
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    call(self.emitter.rt.string.last_index_of);
                    local_set(s64);
                    local_get(s64); i64_const(-1i64 as i64); i64_eq;
                    if_i32;
                      i32_const(0);
                    else_;
                      i32_const(8); call(self.emitter.rt.alloc); local_set(s32);
                      local_get(s32); local_get(s64); i64_store(0);
                      local_get(s32);
                    end;
                });
            }
            "count" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { call(self.emitter.rt.string.count); });
            }
            "pad_start" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { i32_wrap_i64; });
                self.emit_expr(&args[2]);
                wasm!(self.func, { call(self.emitter.rt.string.pad_start); });
            }
            "pad_end" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { i32_wrap_i64; });
                self.emit_expr(&args[2]);
                wasm!(self.func, { call(self.emitter.rt.string.pad_end); });
            }
            "trim_start" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { call(self.emitter.rt.string.trim_start); });
            }
            "trim_end" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { call(self.emitter.rt.string.trim_end); });
            }
            "to_upper" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { call(self.emitter.rt.string.to_upper); });
            }
            "to_lower" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { call(self.emitter.rt.string.to_lower); });
            }
            "capitalize" => {
                let s = self.match_i32_base + self.match_depth;
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(s);
                    local_get(s); i32_load(0); i32_eqz;
                    if_i32; local_get(s);
                    else_;
                      local_get(s); i32_const(0); i32_const(1);
                      call(self.emitter.rt.string.slice);
                      call(self.emitter.rt.string.to_upper);
                      local_get(s); i32_const(1); local_get(s); i32_load(0);
                      call(self.emitter.rt.string.slice);
                      call(self.emitter.rt.concat_str);
                    end;
                });
            }
            "chars" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { call(self.emitter.rt.string.chars); });
            }
            "lines" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { call(self.emitter.rt.string.lines); });
            }
            "from_bytes" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { call(self.emitter.rt.string.from_bytes); });
            }
            "to_bytes" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { call(self.emitter.rt.string.to_bytes); });
            }
            "is_empty" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_load(0); i32_eqz; });
            }
            "is_digit" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { call(self.emitter.rt.string.is_digit); });
            }
            "is_alpha" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { call(self.emitter.rt.string.is_alpha); });
            }
            "is_alphanumeric" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { call(self.emitter.rt.string.is_alnum); });
            }
            "is_whitespace" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { call(self.emitter.rt.string.is_whitespace); });
            }
            "is_upper" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { call(self.emitter.rt.string.is_upper); });
            }
            "is_lower" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { call(self.emitter.rt.string.is_lower); });
            }
            "codepoint" => {
                let s = self.match_i32_base + self.match_depth;
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(s);
                    local_get(s); i32_load(0); i32_eqz;
                    if_i32; i32_const(0);
                    else_;
                      i32_const(8); call(self.emitter.rt.alloc); local_set(s + 1);
                      local_get(s + 1);
                      local_get(s); i32_load8_u(4); i64_extend_i32_u;
                      i64_store(0);
                      local_get(s + 1);
                    end;
                });
            }
            "from_codepoint" => {
                let s = self.match_i32_base + self.match_depth;
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(s);
                    i32_const(5); call(self.emitter.rt.alloc); local_set(s + 1);
                    local_get(s + 1); i32_const(1); i32_store(0);
                    local_get(s + 1); local_get(s); i32_store8(4);
                    local_get(s + 1);
                });
            }
            "replace_first" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                self.emit_expr(&args[2]);
                wasm!(self.func, { call(self.emitter.rt.string.replace_first); });
            }
            "strip_prefix" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { call(self.emitter.rt.string.strip_prefix); });
            }
            "strip_suffix" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { call(self.emitter.rt.string.strip_suffix); });
            }
            _ => return false,
        }
        true
    }
}
