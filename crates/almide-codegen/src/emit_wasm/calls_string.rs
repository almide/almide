//! String stdlib call dispatch for WASM codegen.
//!
//! All `("string", _)` method call handlers live here.

use super::FuncCompiler;
use almide_ir::{IrExpr, IrStringPart};
use almide_lang::types::Ty;

/// Saturation ceiling for an i64 codepoint COUNT in `take`/`drop`/`take_end`/
/// `drop_end` (C-054). Any count `>= count(s)` — including a negative i64
/// (huge as `usize`) or any value `>= 2^32` — means "the whole string" / "drop
/// everything". `i32::MAX` is larger than any materializable codepoint count
/// (a string that long cannot exist in linear memory), and the downstream
/// `utf8_byte_of_cp` / `cp = count - n` logic floors it to the string's actual
/// length, so this sentinel reproduces native's unsigned `n as usize` behavior
/// with a lossless `i32_wrap_i64`. Named so the relationship to the i32 ceiling
/// is explicit instead of a bare `2147483647` literal.
const STRING_COUNT_HUGE: i64 = i32::MAX as i64;

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
                // Codepoint count (#419) — the documented contract is "number
                // of characters", and the whole position API speaks ONE unit.
                // The O(1) byte length stays internal (header load).
                self.emit_expr(&args[0]);
                wasm!(self.func, { call(self.emitter.rt.string.char_count); });
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
                // get(s, i) → Option[String]: `i` is a CODEPOINT index (#419).
                // Negative or ≥ char_count → none, else the whole codepoint.
                let s = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();   // cp index → byte offset → some box
                let cp = self.scratch.alloc_i32();  // result string ptr
                let i64v = self.scratch.alloc_i64();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(s); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(i64v);
                    local_get(i64v); i64_const(0); i64_lt_s;
                    if_i32;
                      i32_const(0); // none
                    else_;
                      // saturate the cp index to i32::MAX; utf8_byte_of_cp maps
                      // any count at/past the end to byte_len → none below.
                      local_get(i64v); i64_const(0x7FFF_FFFF); i64_gt_s;
                      if_i32;
                        i32_const(0x7FFF_FFFF);
                      else_;
                        local_get(i64v); i32_wrap_i64;
                      end;
                      local_set(i);
                      local_get(s); local_get(i); call(self.emitter.rt.string.utf8_byte_of_cp); local_set(i);
                      local_get(i); local_get(s); i32_load(0); i32_ge_u;
                      if_i32;
                        i32_const(0); // none
                      else_;
                        // cp = slice(s, i, i + width(s, i))
                        local_get(s); local_get(i);
                        local_get(i); local_get(s); local_get(i); call(self.emitter.rt.string.utf8_width); i32_add;
                        call(self.emitter.rt.string.slice); local_set(cp);
                        // wrap in some: alloc ptr, store string ptr
                        i32_const(4); call(self.emitter.rt.alloc); local_set(i);
                        local_get(i); local_get(cp); i32_store(0);
                        local_get(i);
                      end;
                    end;
                });
                self.scratch.free_i64(i64v);
                self.scratch.free_i32(cp);
                self.scratch.free_i32(i);
                self.scratch.free_i32(s);
            }
            "repeat" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                // Unsigned-saturate the i64 count to [0, i32::MAX] before
                // narrowing (C-054): a bare i32_wrap_i64 turned `2^32` into 0
                // (empty) and `2^32+k` into a small in-range count, silently
                // producing the wrong string. A count past i32::MAX cannot be
                // materialized on either target (native `s.repeat(n as usize)`
                // aborts on the multi-GB request, wasm `memory.grow` fails), so
                // saturating to the sentinel keeps the wrap lossless and both
                // targets both-fail. Native `n as usize` is UNSIGNED, so a
                // NEGATIVE count is huge (also both-fail), NOT empty.
                const STRING_REPEAT_MAX_COUNT: i64 = i32::MAX as i64;
                self.emit_clamp_count_to_i32(super::calls_list_helpers::ClampHi::Const(STRING_REPEAT_MAX_COUNT));
                wasm!(self.func, { call(self.emitter.rt.string.repeat); });
            }
            "slice" => {
                // slice(s, start, end) — CODEPOINT indices (#419), clamped to
                // [0, char_count]. `utf8_byte_of_cp` maps a count at/past the
                // end to the byte length, so after the signed i64→i32
                // saturation below the `end: Int = i64::MAX` default degrades
                // to "to the end" and a negative index to 0 (native clamps
                // SIGNED: `(start.max(0) as usize).min(count)`, C-054).
                let s_ptr = self.scratch.alloc_i32();
                let start_b = self.scratch.alloc_i32();
                let end_b = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(s_ptr); });
                self.emit_expr(&args[1]);
                self.emit_clamp_count_signed_i32(super::calls_list_helpers::ClampHi::Const(STRING_COUNT_HUGE));
                wasm!(self.func, {
                    local_set(start_b);
                    local_get(s_ptr); local_get(start_b);
                    call(self.emitter.rt.string.utf8_byte_of_cp); local_set(start_b);
                });
                if args.len() > 2 {
                    self.emit_expr(&args[2]);
                    self.emit_clamp_count_signed_i32(super::calls_list_helpers::ClampHi::Const(STRING_COUNT_HUGE));
                    wasm!(self.func, {
                        local_set(end_b);
                        local_get(s_ptr); local_get(end_b);
                        call(self.emitter.rt.string.utf8_byte_of_cp); local_set(end_b);
                    });
                } else {
                    wasm!(self.func, { local_get(s_ptr); i32_load(0); local_set(end_b); });
                }
                wasm!(self.func, {
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
                // The runtime returns a BYTE offset (str::find oracle); the
                // user-facing index is the CODEPOINT index (#419), so convert
                // through cp_of_byte before boxing the some(Int).
                let s64 = self.scratch.alloc_i64();
                let s32 = self.scratch.alloc_i32();
                let s_ptr = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(s_ptr); local_get(s_ptr); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    call(self.emitter.rt.string.index_of);
                    local_set(s64);
                    local_get(s64); i64_const(-1i64 as i64); i64_eq;
                    if_i32;
                      i32_const(0);
                    else_;
                      local_get(s_ptr); local_get(s64); i32_wrap_i64;
                      call(self.emitter.rt.string.cp_of_byte); local_set(s64);
                      i32_const(8); call(self.emitter.rt.alloc); local_set(s32);
                      local_get(s32); local_get(s64); i64_store(0);
                      local_get(s32);
                    end;
                });
                self.scratch.free_i32(s_ptr);
                self.scratch.free_i32(s32);
                self.scratch.free_i64(s64);
            }
            "last_index_of" => {
                // Same byte→codepoint conversion as index_of (#419).
                let s64 = self.scratch.alloc_i64();
                let s32 = self.scratch.alloc_i32();
                let s_ptr = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(s_ptr); local_get(s_ptr); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    call(self.emitter.rt.string.last_index_of);
                    local_set(s64);
                    local_get(s64); i64_const(-1i64 as i64); i64_eq;
                    if_i32;
                      i32_const(0);
                    else_;
                      local_get(s_ptr); local_get(s64); i32_wrap_i64;
                      call(self.emitter.rt.string.cp_of_byte); local_set(s64);
                      i32_const(8); call(self.emitter.rt.alloc); local_set(s32);
                      local_get(s32); local_get(s64); i64_store(0);
                      local_get(s32);
                    end;
                });
                self.scratch.free_i32(s_ptr);
                self.scratch.free_i32(s32);
                self.scratch.free_i64(s64);
            }
            "pad_start" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                // Unsigned-saturate the i64 WIDTH (a target codepoint COUNT)
                // before narrowing (C-054): native `width as usize` is UNSIGNED,
                // so a NEGATIVE or `>= 2^32` width is enormous (`len >= w` false →
                // pads toward a multi-GB string that aborts). A bare i32_wrap_i64
                // turned `2^32+k` into a small in-range width → silently the
                // WRONG (short) result. Clamping to the i32::MAX sentinel keeps
                // small widths exact and makes huge widths both-fail (wasm
                // memory.grow / native abort) instead of wasm-wrong-short.
                self.emit_clamp_count_to_i32(super::calls_list_helpers::ClampHi::Const(STRING_COUNT_HUGE));
                self.emit_expr(&args[2]);
                wasm!(self.func, { call(self.emitter.rt.string.pad_start); });
            }
            "pad_end" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                // Unsigned-saturate the i64 WIDTH before narrowing — same rule as
                // pad_start (C-054); see that comment.
                self.emit_clamp_count_to_i32(super::calls_list_helpers::ClampHi::Const(STRING_COUNT_HUGE));
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
                // Unsigned saturate the i64 COUNT (C-054): native take_end uses
                // `n as usize >= count → start 0` (whole string). A negative `n`
                // OR any `n >= 2^32` means "the whole string"; both map to the
                // huge sentinel here, so `cp = count - n` underflows to <0 and is
                // floored to start 0 below. The old bare wrap + sign-check missed
                // `2^32`/`2^32+k` (they wrap to small non-negative counts).
                self.emit_clamp_count_to_i32(super::calls_list_helpers::ClampHi::Const(STRING_COUNT_HUGE));
                wasm!(self.func, {
                    local_set(n);
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
                // Unsigned saturate the i64 COUNT (C-054): native drop_end uses
                // `n as usize >= count → end 0` (empty). A negative `n` OR any
                // `n >= 2^32` means "drop everything"; both map to the huge
                // sentinel here, so `cp = count - n` underflows to <0 → end 0.
                self.emit_clamp_count_to_i32(super::calls_list_helpers::ClampHi::Const(STRING_COUNT_HUGE));
                wasm!(self.func, {
                    local_set(n);
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
                // Saturate the i64 codepoint COUNT to [0, i32::MAX] BEFORE the
                // wrap (C-054). Native `take(n as usize)` is UNSIGNED, so a
                // negative `n` (huge as usize) AND any `n >= 2^32` mean "the
                // whole string". A bare `i32_wrap_i64` + sign-check missed the
                // `2^32`/`2^32+k` cases (they wrap to 0 / a small in-range count
                // — NON-negative — so the sign-check never fired and the result
                // was an empty / wrong prefix). The unsigned clamp maps all of
                // them to the STRING_COUNT_HUGE sentinel, and `byte_of_cp` then
                // clamps to byte_len → whole string.
                self.emit_clamp_count_to_i32(super::calls_list_helpers::ClampHi::Const(STRING_COUNT_HUGE));
                wasm!(self.func, {
                    local_set(n);
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
                // Unsigned saturate the i64 COUNT (C-054): native
                // `drop(n as usize)` treats a negative `n` and any `n >= 2^32`
                // as "drop everything" (empty). The clamp maps them to the huge
                // sentinel; `byte_of_cp` then clamps the start to byte_len.
                self.emit_clamp_count_to_i32(super::calls_list_helpers::ClampHi::Const(STRING_COUNT_HUGE));
                wasm!(self.func, {
                    local_set(n);
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
