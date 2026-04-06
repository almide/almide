//! String stdlib call dispatch for WASM codegen.
//!
//! All `("string", _)` method call handlers live here.

use super::FuncCompiler;
use almide_ir::{IrExpr, IrStringPart};
use almide_lang::types::Ty;

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
                // UTF-8 char count, matching Rust `s.chars().count()` semantics.
                self.emit_expr(&args[0]);
                wasm!(self.func, { call(self.emitter.rt.string.char_count); });
            }
            "contains" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { call(self.emitter.rt.string.contains); });
            }
            "starts_with" => {
                let s0 = self.scratch.alloc_i32();
                let s1 = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(s0); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(s1);
                    local_get(s1); i32_load(0); // prefix.len
                    local_get(s0); i32_load(0); // s.len
                    i32_gt_u;
                    if_i32; i32_const(0);
                    else_;
                      local_get(s0); i32_const(4); i32_add;
                      local_get(s1); i32_const(4); i32_add;
                      local_get(s1); i32_load(0);
                      call(self.emitter.rt.mem_eq);
                    end;
                });
                self.scratch.free_i32(s1);
                self.scratch.free_i32(s0);
            }
            "ends_with" => {
                let s0 = self.scratch.alloc_i32();
                let s1 = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(s0); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(s1);
                    local_get(s1); i32_load(0);
                    local_get(s0); i32_load(0);
                    i32_gt_u;
                    if_i32; i32_const(0);
                    else_;
                      local_get(s0); i32_const(4); i32_add;
                      local_get(s0); i32_load(0); i32_add;
                      local_get(s1); i32_load(0); i32_sub;
                      local_get(s1); i32_const(4); i32_add;
                      local_get(s1); i32_load(0);
                      call(self.emitter.rt.mem_eq);
                    end;
                });
                self.scratch.free_i32(s1);
                self.scratch.free_i32(s0);
            }
            "get" => {
                // get(s, i) → Option[String]
                // OOB → none(0), else → some(1-char string)
                let s = self.scratch.alloc_i32();
                let s1 = self.scratch.alloc_i32();
                let s2 = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(s); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(s1);
                    // bounds check
                    local_get(s1); i32_const(0); i32_lt_s;
                    local_get(s1); local_get(s); i32_load(0); i32_ge_u;
                    i32_or;
                    if_i32;
                      i32_const(0); // none
                    else_;
                      // Build 1-char string
                      i32_const(5); call(self.emitter.rt.alloc); local_set(s2);
                      local_get(s2); i32_const(1); i32_store(0);
                      local_get(s2);
                      local_get(s); i32_const(4); i32_add;
                      local_get(s1); i32_add; i32_load8_u(0);
                      i32_store8(4);
                      // Wrap in some: alloc ptr, store string ptr
                      i32_const(4); call(self.emitter.rt.alloc); local_set(s1);
                      local_get(s1); local_get(s2); i32_store(0);
                      local_get(s1);
                    end;
                });
                self.scratch.free_i32(s2);
                self.scratch.free_i32(s1);
                self.scratch.free_i32(s);
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
                let s64 = self.scratch.alloc_i64();
                let s32 = self.scratch.alloc_i32();
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
                self.scratch.free_i32(s32);
                self.scratch.free_i64(s64);
            }
            "last_index_of" => {
                let s64 = self.scratch.alloc_i64();
                let s32 = self.scratch.alloc_i32();
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
                self.scratch.free_i32(s32);
                self.scratch.free_i64(s64);
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
                let s = self.scratch.alloc_i32();
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
                self.scratch.free_i32(s);
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
                let s = self.scratch.alloc_i32();
                let s1 = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(s);
                    local_get(s); i32_load(0); i32_eqz;
                    if_i32; i32_const(0);
                    else_;
                      i32_const(8); call(self.emitter.rt.alloc); local_set(s1);
                      local_get(s1);
                      local_get(s); i32_load8_u(4); i64_extend_i32_u;
                      i64_store(0);
                      local_get(s1);
                    end;
                });
                self.scratch.free_i32(s1);
                self.scratch.free_i32(s);
            }
            "from_codepoint" => {
                let s = self.scratch.alloc_i32();
                let s1 = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(s);
                    i32_const(5); call(self.emitter.rt.alloc); local_set(s1);
                    local_get(s1); i32_const(1); i32_store(0);
                    local_get(s1); local_get(s); i32_store8(4);
                    local_get(s1);
                });
                self.scratch.free_i32(s1);
                self.scratch.free_i32(s);
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
            "first" => {
                // first(s) → Option[String]: get(s, 0)
                let s = self.scratch.alloc_i32();
                let s1 = self.scratch.alloc_i32();
                let s2 = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(s);
                    local_get(s); i32_load(0); i32_eqz; // empty?
                    if_i32; i32_const(0); // none
                    else_;
                      // alloc 1-char string
                      i32_const(5); call(self.emitter.rt.alloc); local_set(s1);
                      local_get(s1); i32_const(1); i32_store(0);
                      local_get(s1); local_get(s); i32_load8_u(4); i32_store8(4);
                      // wrap in some: alloc ptr
                      i32_const(4); call(self.emitter.rt.alloc); local_set(s2);
                      local_get(s2); local_get(s1); i32_store(0);
                      local_get(s2);
                    end;
                });
                self.scratch.free_i32(s2);
                self.scratch.free_i32(s1);
                self.scratch.free_i32(s);
            }
            "last" => {
                let s = self.scratch.alloc_i32();
                let s1 = self.scratch.alloc_i32();
                let s2 = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(s);
                    local_get(s); i32_load(0); i32_eqz;
                    if_i32; i32_const(0);
                    else_;
                      i32_const(5); call(self.emitter.rt.alloc); local_set(s1);
                      local_get(s1); i32_const(1); i32_store(0);
                      local_get(s1);
                      local_get(s); i32_const(4); i32_add;
                      local_get(s); i32_load(0); i32_const(1); i32_sub; i32_add;
                      i32_load8_u(0); i32_store8(4);
                      i32_const(4); call(self.emitter.rt.alloc); local_set(s2);
                      local_get(s2); local_get(s1); i32_store(0);
                      local_get(s2);
                    end;
                });
                self.scratch.free_i32(s2);
                self.scratch.free_i32(s1);
                self.scratch.free_i32(s);
            }
            "take_end" => {
                // take_end(s, n) = slice(s, max(0, len-n), len)
                let s = self.scratch.alloc_i32();
                let s1 = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(s); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(s1);
                    local_get(s); i32_load(0); local_get(s1); i32_sub;
                    local_set(s1);
                    local_get(s1); i32_const(0); i32_lt_s;
                    if_empty; i32_const(0); local_set(s1); end;
                    local_get(s); local_get(s1); local_get(s); i32_load(0);
                    call(self.emitter.rt.string.slice);
                });
                self.scratch.free_i32(s1);
                self.scratch.free_i32(s);
            }
            "drop_end" => {
                // drop_end(s, n) = slice(s, 0, max(0, len-n))
                let s = self.scratch.alloc_i32();
                let s1 = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(s); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(s1);
                    local_get(s); i32_load(0); local_get(s1); i32_sub;
                    local_set(s1);
                    local_get(s1); i32_const(0); i32_lt_s;
                    if_empty; i32_const(0); local_set(s1); end;
                    local_get(s); i32_const(0); local_get(s1);
                    call(self.emitter.rt.string.slice);
                });
                self.scratch.free_i32(s1);
                self.scratch.free_i32(s);
            }
            "take" => {
                // take(s, n) = slice(s, 0, min(n, len))
                let s = self.scratch.alloc_i32();
                let s1 = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(s); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(s1); // n
                    // min(n, len)
                    local_get(s1); local_get(s); i32_load(0); i32_lt_u;
                    if_i32; local_get(s1); else_; local_get(s); i32_load(0); end;
                    local_set(s1);
                    local_get(s); i32_const(0); local_get(s1);
                    call(self.emitter.rt.string.slice);
                });
                self.scratch.free_i32(s1);
                self.scratch.free_i32(s);
            }
            "drop" => {
                // drop(s, n) = slice(s, min(n, len), len)
                let s = self.scratch.alloc_i32();
                let s1 = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(s); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(s1);
                    local_get(s1); local_get(s); i32_load(0); i32_lt_u;
                    if_i32; local_get(s1); else_; local_get(s); i32_load(0); end;
                    local_set(s1);
                    local_get(s); local_get(s1); local_get(s); i32_load(0);
                    call(self.emitter.rt.string.slice);
                });
                self.scratch.free_i32(s1);
                self.scratch.free_i32(s);
            }
            _ => return false,
        }
        true
    }

    /// Concatenate two strings on the heap via __concat_str runtime.
    pub(super) fn emit_concat_str(&mut self, left: &IrExpr, right: &IrExpr) {
        self.emit_expr(left);
        self.emit_expr(right);
        wasm!(self.func, { call(self.emitter.rt.concat_str); });
    }

    /// String interpolation: convert each part to string, then concat.
    pub(super) fn emit_string_interp(&mut self, parts: &[IrStringPart]) {
        if parts.is_empty() {
            let empty = self.emitter.intern_string("");
            wasm!(self.func, { i32_const(empty as i32); });
            return;
        }

        // Single part: no scratch buffer needed
        if parts.len() == 1 {
            self.emit_string_part(&parts[0]);
            return;
        }

        // Multi-part: use memory 1 scratch buffer.
        // Each part → string ptr → scratch_write_str (copies bytes to mem1).
        // After all parts: scratch_finalize → alloc on mem0 heap + copy from mem1.
        // Result: zero intermediate heap allocations.
        for part in parts {
            self.emit_string_part(part);
            wasm!(self.func, { call(self.emitter.rt.scratch_write_str); });
        }
        wasm!(self.func, { call(self.emitter.rt.scratch_finalize); });
    }

    /// Emit a single string interpolation part as a string (i32 pointer).
    pub(super) fn emit_string_part(&mut self, part: &IrStringPart) {
        match part {
            IrStringPart::Lit { value } => {
                let offset = self.emitter.intern_string(value);
                wasm!(self.func, { i32_const(offset as i32); });
            }
            IrStringPart::Expr { expr } => {
                match &expr.ty {
                    Ty::String => self.emit_expr(expr),
                    Ty::Int => {
                        self.emit_expr(expr);
                        wasm!(self.func, { call(self.emitter.rt.int_to_string); });
                    }
                    Ty::Bool => {
                        self.emit_expr(expr);
                        let t = self.emitter.intern_string("true");
                        let f = self.emitter.intern_string("false");
                        wasm!(self.func, {
                            if_i32;
                            i32_const(t as i32);
                            else_;
                            i32_const(f as i32);
                            end;
                        });
                    }
                    Ty::Float => {
                        self.emit_expr(expr);
                        wasm!(self.func, {
                            call(self.emitter.rt.float_to_string);
                        });
                    }
                    _ => {
                        // Fallback: emit the expression (already a string pointer or unsupported)
                        self.emit_expr(expr);
                    }
                }
            }
        }
    }

    /// ASCII case conversion. Expects string ptr on stack. Returns new string ptr.
    pub(super) fn emit_str_case_convert(&mut self, is_upper: bool) {
        // String ptr is on stack. Store to scratch local.
        let src = self.scratch.alloc_i32();
        let dst = self.scratch.alloc_i32();
        let s = self.scratch.alloc_i32();
        let s1 = self.scratch.alloc_i32();
        wasm!(self.func, {
            local_set(src);
            // Alloc dst with same len
            i32_const(4);
            local_get(src);
            i32_load(0);
            i32_add;
            call(self.emitter.rt.alloc);
            local_set(dst);
            // Store len in dst
            local_get(dst);
            local_get(src);
            i32_load(0);
            i32_store(0);
        });
        // Loop: convert each byte
        wasm!(self.func, {
            i32_const(0);
            local_set(s);
            block_empty;
            loop_empty;
        });
        let depth_guard = self.depth_push_n(2);
        wasm!(self.func, {
            local_get(s);
            local_get(src);
            i32_load(0);
            i32_ge_u;
            br_if(1);
            // dst addr
            local_get(dst);
            i32_const(4);
            i32_add;
            local_get(s);
            i32_add;
            // src byte
            local_get(src);
            i32_const(4);
            i32_add;
            local_get(s);
            i32_add;
            i32_load8_u(0);
            // Convert
            local_set(s1);
        });
        if is_upper {
            wasm!(self.func, {
                local_get(s1);
                i32_const(97);
                i32_ge_u;
                local_get(s1);
                i32_const(122);
                i32_le_u;
                i32_and;
                if_i32;
                local_get(s1);
                i32_const(32);
                i32_sub;
                else_;
                local_get(s1);
                end;
            });
        } else {
            wasm!(self.func, {
                local_get(s1);
                i32_const(65);
                i32_ge_u;
                local_get(s1);
                i32_const(90);
                i32_le_u;
                i32_and;
                if_i32;
                local_get(s1);
                i32_const(32);
                i32_add;
                else_;
                local_get(s1);
                end;
            });
        }
        wasm!(self.func, {
            i32_store8(0);
            local_get(s);
            i32_const(1);
            i32_add;
            local_set(s);
            br(0);
        });
        self.depth_pop(depth_guard);
        wasm!(self.func, {
            end;
            end;
            // Return dst
            local_get(dst);
        });
        self.scratch.free_i32(s1);
        self.scratch.free_i32(s);
        self.scratch.free_i32(dst);
        self.scratch.free_i32(src);
    }
}
