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
        use super::stdlib_dispatch::StdlibOp;

        // ── Declarative table: simple runtime-call patterns ──
        // Each entry maps method name → (arity, runtime function index).
        let rt = &self.emitter.rt.string;
        let op: Option<StdlibOp> = match method {
            "trim"       => Some(StdlibOp::Call1(rt.trim)),
            "trim_start" => Some(StdlibOp::Call1(rt.trim_start)),
            "trim_end"   => Some(StdlibOp::Call1(rt.trim_end)),
            "reverse"    => Some(StdlibOp::Call1(rt.reverse)),
            "len" | "length" => {
                // O(1): read byte-length header directly
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_load(0); i64_extend_i32_u; });
                return true;
            }
            "contains"   => Some(StdlibOp::Call2(rt.contains)),
            "split"      => Some(StdlibOp::Call2(rt.split)),
            "join"       => Some(StdlibOp::Call2(rt.join)),
            "count"      => Some(StdlibOp::Call2(rt.count)),
            "replace"    => Some(StdlibOp::Call3(rt.replace)),
            // pad_start / pad_end need i32_wrap_i64 on the length arg — stay inline.
            _ => None,
        };
        if let Some(op) = op {
            self.emit_stdlib_op(op, args);
            return true;
        }

        match method {
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
                      local_get(s0); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
                      local_get(s1); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
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
                      local_get(s0); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
                      local_get(s0); i32_load(0); i32_add;
                      local_get(s1); i32_load(0); i32_sub;
                      local_get(s1); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
                      local_get(s1); i32_load(0);
                      call(self.emitter.rt.mem_eq);
                    end;
                });
                self.scratch.free_i32(s1);
                self.scratch.free_i32(s0);
            }
            "get" => {
                // get(s, i) → Option[String]. Maps to native `string.char_at`:
                // `i` is a BYTE index; OOB → none, else snap `i` down to a char
                // boundary and return the WHOLE codepoint starting there.
                let s = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();   // byte index (snapped)
                let cp = self.scratch.alloc_i32();  // result string ptr / some box
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(s); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(i);
                    // bounds check: i < 0 || i >= byte_len → none
                    local_get(i); i32_const(0); i32_lt_s;
                    local_get(i); local_get(s); i32_load(0); i32_ge_u;
                    i32_or;
                    if_i32;
                      i32_const(0); // none
                    else_;
                      // snap i down to a char boundary
                      local_get(s); local_get(i); call(self.emitter.rt.string.utf8_snap); local_set(i);
                      // cp = slice(s, i, i + width(s, i))
                      local_get(s); local_get(i);
                      local_get(i); local_get(s); local_get(i); call(self.emitter.rt.string.utf8_width); i32_add;
                      call(self.emitter.rt.string.slice); local_set(cp);
                      // wrap in some: alloc ptr, store string ptr
                      i32_const(4); call(self.emitter.rt.alloc); local_set(i);
                      local_get(i); local_get(cp); i32_store(0);
                      local_get(i);
                    end;
                });
                self.scratch.free_i32(cp);
                self.scratch.free_i32(i);
                self.scratch.free_i32(s);
            }
            "repeat" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { i32_wrap_i64; call(self.emitter.rt.string.repeat); });
            }
            "slice" => {
                // slice(s, start, end) — BYTE indices, clamped to [0, byte_len]
                // and each SNAPPED DOWN to a char boundary so a multibyte
                // codepoint is never split (matches native string.slice).
                //
                // When `end` comes from the `end: Int = i64::MAX` default
                // injection, wrapping to i32 produces 0xFFFFFFFF (-1) which the
                // runtime would read as a huge unsigned → trap. Clamping to the
                // byte length degrades the sentinel to "to the end" safely.
                let s_ptr = self.scratch.alloc_i32();
                let start_b = self.scratch.alloc_i32();
                let end_b = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(s_ptr); });
                // start (clamped to [0, byte_len], then snapped to a boundary)
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(start_b);
                    // clamp negative → 0
                    local_get(start_b); i32_const(0); i32_lt_s;
                    if_empty; i32_const(0); local_set(start_b); end;
                    // clamp > byte_len → byte_len
                    local_get(start_b); local_get(s_ptr); i32_load(0); i32_gt_u;
                    if_empty; local_get(s_ptr); i32_load(0); local_set(start_b); end;
                    // snap down to char boundary
                    local_get(s_ptr); local_get(start_b);
                    call(self.emitter.rt.string.utf8_snap); local_set(start_b);
                });
                // end — clamp the i64 value to [0, byte_len] BEFORE wrapping to i32, so the
                // `end: Int = i64::MAX` default (and any end > byte_len) degrades to byte_len
                // instead of wrapping to a bogus i32 (i64::MAX -> -1 -> clamp 0 -> empty).
                if args.len() > 2 {
                    let end64 = self.scratch.alloc_i64();
                    self.emit_expr(&args[2]);
                    wasm!(self.func, {
                        local_set(end64);
                        local_get(end64); i64_const(0); i64_lt_s;                 // end < 0 ?
                        if_i32;
                            i32_const(0);
                        else_;
                            local_get(end64);
                            local_get(s_ptr); i32_load(0); i64_extend_i32_u;       // byte_len as i64
                            i64_gt_s;                                             // end > byte_len ?
                            if_i32;
                                local_get(s_ptr); i32_load(0);                    // -> byte_len
                            else_;
                                local_get(end64); i32_wrap_i64;                   // fits in i32
                            end;
                        end;
                        local_set(end_b);
                    });
                    self.scratch.free_i64(end64);
                } else {
                    wasm!(self.func, { local_get(s_ptr); i32_load(0); local_set(end_b); });
                }
                wasm!(self.func, {
                    // clamp negative → 0
                    local_get(end_b); i32_const(0); i32_lt_s;
                    if_empty; i32_const(0); local_set(end_b); end;
                    // clamp > byte_len → byte_len
                    local_get(end_b); local_get(s_ptr); i32_load(0); i32_gt_u;
                    if_empty; local_get(s_ptr); i32_load(0); local_set(end_b); end;
                    // snap down to char boundary
                    local_get(s_ptr); local_get(end_b);
                    call(self.emitter.rt.string.utf8_snap); local_set(end_b);
                    // start >= end → empty string (matches native; also guards
                    // __str_slice against an unsigned `end - start` underflow)
                    local_get(start_b); local_get(end_b); i32_ge_u;
                    if_i32;
                        i32_const(0); call(self.emitter.rt.string_alloc);
                    else_;
                        local_get(s_ptr); local_get(start_b); local_get(end_b);
                        call(self.emitter.rt.string.slice);
                    end;
                });
                self.scratch.free_i32(s_ptr);
                self.scratch.free_i32(start_b);
                self.scratch.free_i32(end_b);
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
            "to_upper" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { call(self.emitter.rt.string.to_upper); });
            }
            "to_lower" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { call(self.emitter.rt.string.to_lower); });
            }
            "capitalize" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { call(self.emitter.rt.string.capitalize); });
            }
            "chars" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { call(self.emitter.rt.string.chars); });
            }
            "run_length_encode" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { call(self.emitter.rt.string.run_length_encode); });
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
                // codepoint(s) → Option[Int]: Unicode scalar of the FIRST
                // codepoint (decoded from its UTF-8 bytes), none if empty.
                let s = self.scratch.alloc_i32();
                let some = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(s);
                    local_get(s); i32_load(0); i32_eqz;
                    if_i32; i32_const(0);
                    else_;
                      i32_const(8); call(self.emitter.rt.alloc); local_set(some);
                      local_get(some);
                      local_get(s); i32_const(0); call(self.emitter.rt.string.utf8_scalar);
                      i64_store(0);
                      local_get(some);
                    end;
                });
                self.scratch.free_i32(some);
                self.scratch.free_i32(s);
            }
            "from_codepoint" => {
                // from_codepoint(cp) → UTF-8 ENCODE the scalar (1-4 bytes).
                // Invalid scalar (cp < 0, cp > 0x10FFFF, or surrogate
                // 0xD800..=0xDFFF) → EMPTY string (matches char::from_u32).
                let cp = self.scratch.alloc_i32();
                let r = self.scratch.alloc_i32();   // result string ptr
                let data = self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32 as u32;
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(cp);
                    // invalid? cp < 0 || cp > 0x10FFFF || (cp >= 0xD800 && cp <= 0xDFFF)
                    local_get(cp); i32_const(0); i32_lt_s;
                    local_get(cp); i32_const(0x10FFFF); i32_gt_s; i32_or;
                    local_get(cp); i32_const(0xD800); i32_ge_s;
                    local_get(cp); i32_const(0xDFFF); i32_le_s; i32_and; i32_or;
                    if_i32;
                      // invalid → empty string
                      i32_const(0); call(self.emitter.rt.string_alloc);
                    else_;
                      local_get(cp); i32_const(0x80); i32_lt_s;
                      if_i32;
                        // 1 byte
                        i32_const(1); call(self.emitter.rt.string_alloc); local_set(r);
                        local_get(r); local_get(cp); i32_store8(data);
                        local_get(r);
                      else_;
                        local_get(cp); i32_const(0x800); i32_lt_s;
                        if_i32;
                          // 2 bytes
                          i32_const(2); call(self.emitter.rt.string_alloc); local_set(r);
                          local_get(r);
                          i32_const(0xC0); local_get(cp); i32_const(6); i32_shr_u; i32_or;
                          i32_store8(data);
                          local_get(r);
                          i32_const(0x80); local_get(cp); i32_const(0x3F); i32_and; i32_or;
                          i32_store8(data + 1);
                          local_get(r);
                        else_;
                          local_get(cp); i32_const(0x10000); i32_lt_s;
                          if_i32;
                            // 3 bytes
                            i32_const(3); call(self.emitter.rt.string_alloc); local_set(r);
                            local_get(r);
                            i32_const(0xE0); local_get(cp); i32_const(12); i32_shr_u; i32_or;
                            i32_store8(data);
                            local_get(r);
                            i32_const(0x80); local_get(cp); i32_const(6); i32_shr_u; i32_const(0x3F); i32_and; i32_or;
                            i32_store8(data + 1);
                            local_get(r);
                            i32_const(0x80); local_get(cp); i32_const(0x3F); i32_and; i32_or;
                            i32_store8(data + 2);
                            local_get(r);
                          else_;
                            // 4 bytes
                            i32_const(4); call(self.emitter.rt.string_alloc); local_set(r);
                            local_get(r);
                            i32_const(0xF0); local_get(cp); i32_const(18); i32_shr_u; i32_or;
                            i32_store8(data);
                            local_get(r);
                            i32_const(0x80); local_get(cp); i32_const(12); i32_shr_u; i32_const(0x3F); i32_and; i32_or;
                            i32_store8(data + 1);
                            local_get(r);
                            i32_const(0x80); local_get(cp); i32_const(6); i32_shr_u; i32_const(0x3F); i32_and; i32_or;
                            i32_store8(data + 2);
                            local_get(r);
                            i32_const(0x80); local_get(cp); i32_const(0x3F); i32_and; i32_or;
                            i32_store8(data + 3);
                            local_get(r);
                          end;
                        end;
                      end;
                    end;
                });
                self.scratch.free_i32(r);
                self.scratch.free_i32(cp);
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
                // first(s) → Option[String]: the first whole CODEPOINT.
                let s = self.scratch.alloc_i32();
                let cp = self.scratch.alloc_i32();  // codepoint string
                let some = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(s);
                    local_get(s); i32_load(0); i32_eqz; // empty?
                    if_i32; i32_const(0); // none
                    else_;
                      // cp = slice(s, 0, width(s, 0))
                      local_get(s); i32_const(0);
                      local_get(s); i32_const(0); call(self.emitter.rt.string.utf8_width);
                      call(self.emitter.rt.string.slice); local_set(cp);
                      // wrap in some
                      i32_const(4); call(self.emitter.rt.alloc); local_set(some);
                      local_get(some); local_get(cp); i32_store(0);
                      local_get(some);
                    end;
                });
                self.scratch.free_i32(some);
                self.scratch.free_i32(cp);
                self.scratch.free_i32(s);
            }
            "last" => {
                // last(s) → Option[String]: the last whole CODEPOINT.
                let s = self.scratch.alloc_i32();
                let start = self.scratch.alloc_i32(); // byte offset of last cp
                let cp = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(s);
                    local_get(s); i32_load(0); i32_eqz;
                    if_i32; i32_const(0);
                    else_;
                      // start = byte_of_cp(s, char_count(s) - 1)
                      local_get(s);
                      local_get(s); call(self.emitter.rt.string.char_count); i32_wrap_i64;
                      i32_const(1); i32_sub;
                      call(self.emitter.rt.string.utf8_byte_of_cp); local_set(start);
                      // cp = slice(s, start, byte_len)
                      local_get(s); local_get(start); local_get(s); i32_load(0);
                      call(self.emitter.rt.string.slice); local_set(cp);
                      // wrap in some
                      i32_const(4); call(self.emitter.rt.alloc); local_set(start);
                      local_get(start); local_get(cp); i32_store(0);
                      local_get(start);
                    end;
                });
                self.scratch.free_i32(cp);
                self.scratch.free_i32(start);
                self.scratch.free_i32(s);
            }
            "take_end" => {
                // take_end(s, n) = last n CODEPOINTS.
                // = slice_bytes(s, byte_of_cp(s, count - n), byte_len)
                // (n >= count → byte_of_cp(s, <=0) = 0 → whole string)
                let s = self.scratch.alloc_i32();
                let n = self.scratch.alloc_i32();
                let cp = self.scratch.alloc_i32(); // count - n
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(s); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(n);
                    // negative n → i32::MAX so cp = count - n underflows to <0 → start 0 →
                    // whole string, matching native take_end (n as usize >= count → start 0).
                    local_get(n); i32_const(0); i32_lt_s;
                    if_empty; i32_const(2147483647); local_set(n); end;
                    // cp = char_count(s) - n
                    local_get(s); call(self.emitter.rt.string.char_count); i32_wrap_i64;
                    local_get(n); i32_sub; local_set(cp);
                    // start_byte = byte_of_cp(s, max(0, cp))
                    local_get(cp); i32_const(0); i32_lt_s;
                    if_empty; i32_const(0); local_set(cp); end;
                    local_get(s);
                    local_get(s); local_get(cp); call(self.emitter.rt.string.utf8_byte_of_cp);
                    local_get(s); i32_load(0);
                    call(self.emitter.rt.string.slice);
                });
                self.scratch.free_i32(cp);
                self.scratch.free_i32(n);
                self.scratch.free_i32(s);
            }
            "drop_end" => {
                // drop_end(s, n) = all but last n CODEPOINTS.
                // = slice_bytes(s, 0, byte_of_cp(s, count - n))
                let s = self.scratch.alloc_i32();
                let n = self.scratch.alloc_i32();
                let cp = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(s); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(n);
                    // negative n → i32::MAX so cp = count - n underflows to <0 → end 0 →
                    // empty, matching native drop_end (n as usize >= count → end 0).
                    local_get(n); i32_const(0); i32_lt_s;
                    if_empty; i32_const(2147483647); local_set(n); end;
                    local_get(s); call(self.emitter.rt.string.char_count); i32_wrap_i64;
                    local_get(n); i32_sub; local_set(cp); // cp = count - n
                    local_get(cp); i32_const(0); i32_lt_s;
                    if_empty; i32_const(0); local_set(cp); end;
                    local_get(s); i32_const(0);
                    local_get(s); local_get(cp); call(self.emitter.rt.string.utf8_byte_of_cp);
                    call(self.emitter.rt.string.slice);
                });
                self.scratch.free_i32(cp);
                self.scratch.free_i32(n);
                self.scratch.free_i32(s);
            }
            "take" => {
                // take(s, n) = first n CODEPOINTS = slice_bytes(s, 0, byte_of_cp(s, n)).
                // byte_of_cp clamps n >= count → byte_len, so n>=count → whole string.
                let s = self.scratch.alloc_i32();
                let n = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(s); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(n);
                    // negative n → huge (i32::MAX): byte_of_cp clamps to byte_len → whole
                    // string, matching native `take(n as usize)` (a negative i64 casts to a
                    // huge usize). NOT clamp-to-0.
                    local_get(n); i32_const(0); i32_lt_s;
                    if_empty; i32_const(2147483647); local_set(n); end;
                    local_get(s); i32_const(0);
                    local_get(s); local_get(n); call(self.emitter.rt.string.utf8_byte_of_cp);
                    call(self.emitter.rt.string.slice);
                });
                self.scratch.free_i32(n);
                self.scratch.free_i32(s);
            }
            "drop" => {
                // drop(s, n) = all but first n CODEPOINTS
                // = slice_bytes(s, byte_of_cp(s, n), byte_len).
                let s = self.scratch.alloc_i32();
                let n = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(s); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(n);
                    // negative n → huge (i32::MAX): byte_of_cp clamps to byte_len → drop
                    // everything (empty), matching native `drop(n as usize)`.
                    local_get(n); i32_const(0); i32_lt_s;
                    if_empty; i32_const(2147483647); local_set(n); end;
                    local_get(s);
                    local_get(s); local_get(n); call(self.emitter.rt.string.utf8_byte_of_cp);
                    local_get(s); i32_load(0);
                    call(self.emitter.rt.string.slice);
                });
                self.scratch.free_i32(n);
                self.scratch.free_i32(s);
            }
            // ── In-place mutators (Unit-returning, write back into the var) ──
            // These mirror `list.push` / `list.clear`: the receiver is mutated
            // and (for push) the possibly-reallocated pointer is written back
            // into the var/cell via `emit_mutator_writeback`. They leave nothing
            // on the stack (the call is in Unit context). See
            // `is_inplace_mutator` in pass_closure_conversion.rs for the source
            // of truth on which calls mutate `args[0]`.
            "push" | "push_char" => {
                // push(mut s, suffix) → Unit. `__string_append` appends suffix
                // to s in place when s has spare capacity, else grows 2x and
                // returns a new pointer — exactly the semantics native
                // `s.push_str(suffix)` needs. push_char takes a 1-char String,
                // so it is identical. Both args are `String` (i32 ptr).
                let new_ptr = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    call(self.emitter.rt.string_append);
                    local_set(new_ptr);
                });
                // Write the (possibly reallocated) string ptr back into the
                // local / global / mutable-capture cell.
                self.emit_mutator_writeback(&args[0], new_ptr);
                self.scratch.free_i32(new_ptr);
            }
            "clear" => {
                // clear(mut s) → Unit. Sets the length header to 0 in place.
                // Capacity and buffer are kept (matches `String::clear`), so no
                // writeback is needed — the pointer is unchanged. Mirrors
                // `list.clear`.
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_const(0); i32_store(0); });
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

    /// String interpolation: build the concatenated result inline without any
    /// fixed-size scratch region.
    ///
    /// Strategy:
    ///   1. Evaluate each part and stash the resulting string pointer in a
    ///      scratch i32 local (one local per part).
    ///   2. Sum the part lengths into `total_len`.
    ///   3. Allocate `[len:i32][data...]` on the heap in a single `alloc()`.
    ///   4. `memory.copy` each part's data bytes into the result buffer.
    ///
    /// This replaces the old scheme that wrote into a 256 KB reserved region
    /// via `__scratch_write_str` / `__scratch_finalize`. The inline version
    /// has no static size cap, so pathological multi-part interpolations no
    /// longer trap, and the module no longer reserves memory for a scratch
    /// that goes unused when no interpolation runs.
    pub(super) fn emit_string_interp(&mut self, parts: &[IrStringPart]) {

        if parts.is_empty() {
            let empty = self.emitter.intern_string("");
            wasm!(self.func, { i32_const(empty as i32); });
            return;
        }

        // Single part: the value IS the result. Skip the whole dance.
        if parts.len() == 1 {
            self.emit_string_part(&parts[0]);
            return;
        }

        // Stage 1: stash each part's pointer in a scratch local.
        let mut part_locals: Vec<u32> = Vec::with_capacity(parts.len());
        for part in parts {
            self.emit_string_part(part);
            let local = self.scratch.alloc_i32();
            wasm!(self.func, { local_set(local); });
            part_locals.push(local);
        }

        // Stage 2: total_len = sum(load(part) for each part).
        // String header layout: mem0[ptr + 0] = byte length.
        let total_len_local = self.scratch.alloc_i32();
        wasm!(self.func, { i32_const(0); local_set(total_len_local); });
        for &p in &part_locals {
            wasm!(self.func, {
                local_get(total_len_local);
                local_get(p);
                i32_load(0);
                i32_add;
                local_set(total_len_local);
            });
        }

        // Stage 3: alloc result = [len:i32][cap:i32][data...] with one heap bump.
        let result_local = self.scratch.alloc_i32();
        wasm!(self.func, {
            local_get(total_len_local);
            i32_const(self.emitter.layout_reg.header_size(super::engine::layout::STRING) as i32);
            i32_add;
        });
        let alloc_fn = self.emitter.rt.alloc;
        wasm!(self.func, {
            call(alloc_fn);
            local_set(result_local);
        });
        // Write length prefix.
        wasm!(self.func, {
            local_get(result_local);
            local_get(total_len_local);
            i32_store(0);
        });
        // Write capacity = total_len.
        wasm!(self.func, {
            local_get(result_local);
            local_get(total_len_local);
            i32_store(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::CAP) as i32 as u32);
        });

        // Stage 4: memory.copy each part's data bytes into the result buffer.
        // dst starts at result + self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32 and advances by each part's length.
        let dst_local = self.scratch.alloc_i32();
        wasm!(self.func, {
            local_get(result_local);
            i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32);
            i32_add;
            local_set(dst_local);
        });
        for &p in &part_locals {
            wasm!(self.func, {
                // memory.copy(dst, src=part+self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32, len=mem0[part])
                local_get(dst_local);
                local_get(p);
                i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32);
                i32_add;
                local_get(p);
                i32_load(0);
                memory_copy(0, 0);
                // dst += part length
                local_get(dst_local);
                local_get(p);
                i32_load(0);
                i32_add;
                local_set(dst_local);
            });
        }

        // Leave the result pointer on the stack as the interp value, then
        // release the scratch locals.
        wasm!(self.func, { local_get(result_local); });
        for p in part_locals {
            self.scratch.free_i32(p);
        }
        self.scratch.free_i32(total_len_local);
        self.scratch.free_i32(dst_local);
        self.scratch.free_i32(result_local);
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
                        // Display form: an integer-valued float drops its `.0`
                        // (`${1.0}` → `1`), matching the native Rust `Display`.
                        wasm!(self.func, {
                            call(self.emitter.rt.float_display);
                        });
                    }
                    _ => {
                        // COMPOUND part (List/Map/Set/Tuple/Option/Result): walk it
                        // to its Almide-literal repr, byte-identical with native. The
                        // walk is type-driven (see `emit_repr_value`); `emit_expr`
                        // leaves the value on the stack, then the repr consumes it.
                        // The old fallback treated a compound's HEAP POINTER as a
                        // string pointer → silent garbage. Records/variants are scoped
                        // out (the walker leaves them on the Display path) so they do
                        // not reach here.
                        self.emit_expr(expr);
                        self.emit_repr_value(&expr.ty);
                    }
                }
            }
        }
    }
}
