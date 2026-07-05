//! Regex module call dispatch for WASM codegen.
//!
//! Each public op compiles the pattern ONCE via `__rx_compile` (mirroring
//! native's per-call `rx_compile`), reads the group count from `ncap_global`,
//! allocates a `caps` buffer (ncap slots × [start,end] BYTE offsets), then
//! drives the node-graph matcher in `rt_regex.rs`. Positions are BYTE offsets
//! throughout so `string.slice` (byte-indexed) stays correct; `__rx_find_at`
//! advances by scalar width.
//!
//! Return shapes mirror native exactly:
//!   regex.is_match(pattern, text)      → Bool
//!   regex.full_match(pattern, text)    → Bool
//!   regex.find(pattern, text)          → Option[String]
//!   regex.find_all(pattern, text)      → List[String]
//!   regex.replace(pattern, text, rep)  → String   (rep inserted VERBATIM)
//!   regex.replace_first(pattern, text, rep) → String
//!   regex.split(pattern, text)         → List[String]
//!   regex.captures(pattern, text)      → Option[List[String]]  (None when ncap==0)

use super::FuncCompiler;
use almide_ir::IrExpr;

/// Bytes per `caps` slot: `[start:i32, end:i32]` (matches `RX_CAP_BYTES`).
const RX_CAP_BYTES: i32 = 8;
/// Bytes per list/buffer pointer slot.
const PTR_BYTES: i32 = 4;

impl FuncCompiler<'_> {
    /// Dispatch a `regex.*` module call.
    pub(super) fn emit_regex_call(&mut self, func: &str, args: &[IrExpr]) {
        match func {
            "is_match" => self.emit_regex_is_match(args),
            "full_match" => self.emit_regex_full_match(args),
            "find" => self.emit_regex_find(args),
            "find_all" => self.emit_regex_find_all(args),
            "replace" => self.emit_regex_replace(args),
            "replace_first" => self.emit_regex_replace_first(args),
            "split" => self.emit_regex_split(args),
            "captures" => self.emit_regex_captures(args),
            _ => panic!(
                "[ICE] emit_wasm: no WASM dispatch for `regex.{}` — \
                 add an arm in emit_regex_call or resolve upstream",
                func
            ),
        }
    }

    /// Emit: compile `pat` (already in local `pat`) → alts_ptr in `alts`, and
    /// allocate a caps buffer (ncap slots) into `caps`. Leaves both locals set.
    fn emit_compile_pattern(&mut self, pat: u32, alts: u32, caps: u32, ncap: u32) {
        let compile = self.emitter.rt.regex.compile;
        let ncap_global = self.emitter.rt.regex.ncap_global;
        let alloc = self.emitter.rt.alloc;
        wasm!(self.func, {
            local_get(pat); call(compile); local_set(alts);
            global_get(ncap_global); local_set(ncap);
            // caps = alloc(ncap * RX_CAP_BYTES)  (ncap may be 0 → 0-byte alloc, fine)
            local_get(ncap); i32_const(RX_CAP_BYTES); i32_mul; call(alloc); local_set(caps);
        });
    }

    /// regex.is_match(pattern, text) → Bool
    fn emit_regex_is_match(&mut self, args: &[IrExpr]) {
        let pat = self.scratch.alloc_i32();
        let text = self.scratch.alloc_i32();
        let alts = self.scratch.alloc_i32();
        let caps = self.scratch.alloc_i32();
        let ncap = self.scratch.alloc_i32();

        self.emit_expr(&args[0]);
        wasm!(self.func, { local_set(pat); });
        self.emit_expr(&args[1]);
        wasm!(self.func, { local_set(text); });
        self.emit_compile_pattern(pat, alts, caps, ncap);
        wasm!(self.func, {
            local_get(alts); local_get(text); i32_const(0); local_get(caps); local_get(ncap);
            call(self.emitter.rt.regex.find_at);
            i32_const(-1); i32_ne;
        });

        self.scratch.free_i32(ncap);
        self.scratch.free_i32(caps);
        self.scratch.free_i32(alts);
        self.scratch.free_i32(text);
        self.scratch.free_i32(pat);
    }

    /// regex.full_match(pattern, text) → Bool
    /// Native: rx_match_alts at p=0, success AND end == byte_len.
    fn emit_regex_full_match(&mut self, args: &[IrExpr]) {
        let pat = self.scratch.alloc_i32();
        let text = self.scratch.alloc_i32();
        let alts = self.scratch.alloc_i32();
        let caps = self.scratch.alloc_i32();
        let ncap = self.scratch.alloc_i32();
        let end_pos = self.scratch.alloc_i32();

        self.emit_expr(&args[0]);
        wasm!(self.func, { local_set(pat); });
        self.emit_expr(&args[1]);
        wasm!(self.func, { local_set(text); });
        self.emit_compile_pattern(pat, alts, caps, ncap);
        wasm!(self.func, {
            local_get(alts); local_get(text); i32_const(0); local_get(caps); local_get(ncap);
            call(self.emitter.rt.regex.match_alts);
            local_set(end_pos);
            // full_match = end_pos != -1 && end_pos == byte_len
            local_get(end_pos); i32_const(-1); i32_ne;
            local_get(end_pos); local_get(text); i32_load(0); i32_eq;
            i32_and;
        });

        self.scratch.free_i32(end_pos);
        self.scratch.free_i32(ncap);
        self.scratch.free_i32(caps);
        self.scratch.free_i32(alts);
        self.scratch.free_i32(text);
        self.scratch.free_i32(pat);
    }

    /// regex.find(pattern, text) → Option[String]
    fn emit_regex_find(&mut self, args: &[IrExpr]) {
        let pat = self.scratch.alloc_i32();
        let text = self.scratch.alloc_i32();
        let alts = self.scratch.alloc_i32();
        let caps = self.scratch.alloc_i32();
        let ncap = self.scratch.alloc_i32();
        let end_pos = self.scratch.alloc_i32();
        let match_start = self.scratch.alloc_i32();
        let str_ptr = self.scratch.alloc_i32();
        let opt_ptr = self.scratch.alloc_i32();

        self.emit_expr(&args[0]);
        wasm!(self.func, { local_set(pat); });
        self.emit_expr(&args[1]);
        wasm!(self.func, { local_set(text); });
        self.emit_compile_pattern(pat, alts, caps, ncap);
        wasm!(self.func, {
            local_get(alts); local_get(text); i32_const(0); local_get(caps); local_get(ncap);
            call(self.emitter.rt.regex.find_at);
            local_set(end_pos);
            local_get(end_pos); i32_const(-1); i32_eq;
            if_i32;
                i32_const(0); // none
            else_;
                global_get(self.emitter.rt.regex.match_start_global); local_set(match_start);
                local_get(text); local_get(match_start); local_get(end_pos);
                call(self.emitter.rt.string.slice);
                local_set(str_ptr);
                i32_const(4); call(self.emitter.rt.alloc); local_set(opt_ptr);
                local_get(opt_ptr); local_get(str_ptr); i32_store(0);
                local_get(opt_ptr);
            end;
        });

        self.scratch.free_i32(opt_ptr);
        self.scratch.free_i32(str_ptr);
        self.scratch.free_i32(match_start);
        self.scratch.free_i32(end_pos);
        self.scratch.free_i32(ncap);
        self.scratch.free_i32(caps);
        self.scratch.free_i32(alts);
        self.scratch.free_i32(text);
        self.scratch.free_i32(pat);
    }

    /// regex.find_all(pattern, text) → List[String]
    /// Zero-width matches advance by one scalar width (native: +1 char). The
    /// temp buffer is sized to the real match upper bound (byte_len+1 matches),
    /// not a fixed cap.
    fn emit_regex_find_all(&mut self, args: &[IrExpr]) {
        let pat = self.scratch.alloc_i32();
        let text = self.scratch.alloc_i32();
        let alts = self.scratch.alloc_i32();
        let caps = self.scratch.alloc_i32();
        let ncap = self.scratch.alloc_i32();
        let pos = self.scratch.alloc_i32();
        let end_pos = self.scratch.alloc_i32();
        let match_start = self.scratch.alloc_i32();
        let text_len = self.scratch.alloc_i32();
        let count = self.scratch.alloc_i32();
        let buf = self.scratch.alloc_i32();
        let str_ptr = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();
        let width = self.scratch.alloc_i32();

        let list_hdr = self.emitter.layout_reg.header_size(super::engine::layout::LIST) as i32;
        let list_data = self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32;

        self.emit_expr(&args[0]);
        wasm!(self.func, { local_set(pat); });
        self.emit_expr(&args[1]);
        wasm!(self.func, { local_set(text); });
        self.emit_compile_pattern(pat, alts, caps, ncap);
        wasm!(self.func, {
            local_get(text); i32_load(0); local_set(text_len);
            // upper bound: at most text_len+1 matches → buffer of (text_len+1) ptrs
            local_get(text_len); i32_const(1); i32_add; i32_const(PTR_BYTES); i32_mul;
            call(self.emitter.rt.alloc); local_set(buf);
            i32_const(0); local_set(count);
            i32_const(0); local_set(pos);

            block_empty; loop_empty;
                local_get(pos); local_get(text_len); i32_gt_u; br_if(1);
                local_get(alts); local_get(text); local_get(pos); local_get(caps); local_get(ncap);
                call(self.emitter.rt.regex.find_at);
                local_set(end_pos);
                local_get(end_pos); i32_const(-1); i32_eq; br_if(1);
                global_get(self.emitter.rt.regex.match_start_global); local_set(match_start);
                // slice the match
                local_get(text); local_get(match_start); local_get(end_pos);
                call(self.emitter.rt.string.slice);
                local_set(str_ptr);
                local_get(buf); local_get(count); i32_const(PTR_BYTES); i32_mul; i32_add;
                local_get(str_ptr); i32_store(0);
                local_get(count); i32_const(1); i32_add; local_set(count);
                // advance: zero-width → +1 scalar width; else → end
                local_get(end_pos); local_get(match_start); i32_eq;
                if_empty;
                    // zero-width: if at end, stop; else advance one scalar
                    local_get(end_pos); local_get(text_len); i32_ge_u;
                    if_empty; br(3); end; // break loop
                    local_get(text); local_get(end_pos);
                    call(self.emitter.rt.string.utf8_width); local_set(width);
                    local_get(end_pos); local_get(width); i32_add; local_set(pos);
                else_;
                    local_get(end_pos); local_set(pos);
                end;
                br(0);
            end; end;

            // build result list
            local_get(count); i32_const(PTR_BYTES); i32_mul; i32_const(list_hdr); i32_add;
            call(self.emitter.rt.alloc); local_set(result);
            local_get(result); local_get(count); i32_store(0);
            i32_const(0); local_set(pos);
            block_empty; loop_empty;
                local_get(pos); local_get(count); i32_ge_u; br_if(1);
                local_get(result); i32_const(list_data); i32_add;
                local_get(pos); i32_const(PTR_BYTES); i32_mul; i32_add;
                local_get(buf); local_get(pos); i32_const(PTR_BYTES); i32_mul; i32_add; i32_load(0);
                i32_store(0);
                local_get(pos); i32_const(1); i32_add; local_set(pos);
                br(0);
            end; end;
            local_get(result);
        });

        self.scratch.free_i32(width);
        self.scratch.free_i32(result);
        self.scratch.free_i32(str_ptr);
        self.scratch.free_i32(buf);
        self.scratch.free_i32(count);
        self.scratch.free_i32(text_len);
        self.scratch.free_i32(match_start);
        self.scratch.free_i32(end_pos);
        self.scratch.free_i32(pos);
        self.scratch.free_i32(ncap);
        self.scratch.free_i32(caps);
        self.scratch.free_i32(alts);
        self.scratch.free_i32(text);
        self.scratch.free_i32(pat);
    }

    /// regex.replace(pattern, text, replacement) → String (verbatim rep, replace ALL)
    /// Zero-width: emit chars[end]'s scalar then advance one scalar width
    /// (native: result.push(chars[end]); pos = end+1).
    fn emit_regex_replace(&mut self, args: &[IrExpr]) {
        let pat = self.scratch.alloc_i32();
        let text = self.scratch.alloc_i32();
        let repl = self.scratch.alloc_i32();
        let alts = self.scratch.alloc_i32();
        let caps = self.scratch.alloc_i32();
        let ncap = self.scratch.alloc_i32();
        let pos = self.scratch.alloc_i32();
        let end_pos = self.scratch.alloc_i32();
        let match_start = self.scratch.alloc_i32();
        let text_len = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();
        let segment = self.scratch.alloc_i32();
        let width = self.scratch.alloc_i32();

        let empty_str = self.emitter.intern_string("");

        self.emit_expr(&args[0]);
        wasm!(self.func, { local_set(pat); });
        self.emit_expr(&args[1]);
        wasm!(self.func, { local_set(text); });
        self.emit_expr(&args[2]);
        wasm!(self.func, { local_set(repl); });
        self.emit_compile_pattern(pat, alts, caps, ncap);
        wasm!(self.func, {
            local_get(text); i32_load(0); local_set(text_len);
            i32_const(empty_str as i32); local_set(result);
            i32_const(0); local_set(pos);

            block_empty; loop_empty;
                // pos > text_len → done
                local_get(pos); local_get(text_len); i32_gt_u;
                if_empty; br(2); end;

                local_get(alts); local_get(text); local_get(pos); local_get(caps); local_get(ncap);
                call(self.emitter.rt.regex.find_at);
                local_set(end_pos);
                local_get(end_pos); i32_const(-1); i32_eq;
                if_empty;
                    // no more matches: append rest of text and break
                    local_get(text); local_get(pos); local_get(text_len);
                    call(self.emitter.rt.string.slice); local_set(segment);
                    local_get(result); local_get(segment);
                    call(self.emitter.rt.concat_str); local_set(result);
                    br(2);
                end;

                global_get(self.emitter.rt.regex.match_start_global); local_set(match_start);
                // append text[pos..match_start]
                local_get(text); local_get(pos); local_get(match_start);
                call(self.emitter.rt.string.slice); local_set(segment);
                local_get(result); local_get(segment);
                call(self.emitter.rt.concat_str); local_set(result);
                // append replacement (verbatim)
                local_get(result); local_get(repl);
                call(self.emitter.rt.concat_str); local_set(result);

                // advance: zero-width → emit one scalar of text then +width
                local_get(end_pos); local_get(match_start); i32_eq;
                if_empty;
                    // if end < text_len, append the scalar at `end`
                    local_get(end_pos); local_get(text_len); i32_lt_u;
                    if_empty;
                        local_get(text); local_get(end_pos);
                        call(self.emitter.rt.string.utf8_width); local_set(width);
                        local_get(text); local_get(end_pos);
                        local_get(end_pos); local_get(width); i32_add;
                        call(self.emitter.rt.string.slice); local_set(segment);
                        local_get(result); local_get(segment);
                        call(self.emitter.rt.concat_str); local_set(result);
                        local_get(end_pos); local_get(width); i32_add; local_set(pos);
                    else_;
                        // end == text_len: advance past end to terminate
                        local_get(end_pos); i32_const(1); i32_add; local_set(pos);
                    end;
                else_;
                    local_get(end_pos); local_set(pos);
                end;
                br(0);
            end; end;

            local_get(result);
        });

        self.scratch.free_i32(width);
        self.scratch.free_i32(segment);
        self.scratch.free_i32(result);
        self.scratch.free_i32(text_len);
        self.scratch.free_i32(match_start);
        self.scratch.free_i32(end_pos);
        self.scratch.free_i32(pos);
        self.scratch.free_i32(ncap);
        self.scratch.free_i32(caps);
        self.scratch.free_i32(alts);
        self.scratch.free_i32(repl);
        self.scratch.free_i32(text);
        self.scratch.free_i32(pat);
    }

    /// regex.replace_first(pattern, text, replacement) → String (verbatim rep)
    fn emit_regex_replace_first(&mut self, args: &[IrExpr]) {
        let pat = self.scratch.alloc_i32();
        let text = self.scratch.alloc_i32();
        let repl = self.scratch.alloc_i32();
        let alts = self.scratch.alloc_i32();
        let caps = self.scratch.alloc_i32();
        let ncap = self.scratch.alloc_i32();
        let end_pos = self.scratch.alloc_i32();
        let match_start = self.scratch.alloc_i32();
        let text_len = self.scratch.alloc_i32();
        let before = self.scratch.alloc_i32();
        let after = self.scratch.alloc_i32();

        self.emit_expr(&args[0]);
        wasm!(self.func, { local_set(pat); });
        self.emit_expr(&args[1]);
        wasm!(self.func, { local_set(text); });
        self.emit_expr(&args[2]);
        wasm!(self.func, { local_set(repl); });
        self.emit_compile_pattern(pat, alts, caps, ncap);
        wasm!(self.func, {
            local_get(text); i32_load(0); local_set(text_len);
            local_get(alts); local_get(text); i32_const(0); local_get(caps); local_get(ncap);
            call(self.emitter.rt.regex.find_at);
            local_set(end_pos);
            local_get(end_pos); i32_const(-1); i32_eq;
            if_i32;
                local_get(text); // no match → text as-is
            else_;
                global_get(self.emitter.rt.regex.match_start_global); local_set(match_start);
                local_get(text); i32_const(0); local_get(match_start);
                call(self.emitter.rt.string.slice); local_set(before);
                local_get(text); local_get(end_pos); local_get(text_len);
                call(self.emitter.rt.string.slice); local_set(after);
                local_get(before); local_get(repl);
                call(self.emitter.rt.concat_str);
                local_get(after);
                call(self.emitter.rt.concat_str);
            end;
        });

        self.scratch.free_i32(after);
        self.scratch.free_i32(before);
        self.scratch.free_i32(text_len);
        self.scratch.free_i32(match_start);
        self.scratch.free_i32(end_pos);
        self.scratch.free_i32(ncap);
        self.scratch.free_i32(caps);
        self.scratch.free_i32(alts);
        self.scratch.free_i32(repl);
        self.scratch.free_i32(text);
        self.scratch.free_i32(pat);
    }

    /// regex.split(pattern, text) → List[String]
    /// Mirrors native: zero-width match at current pos takes one scalar; the
    /// trailing segment is always pushed. Buffer sized to the real upper bound.
    fn emit_regex_split(&mut self, args: &[IrExpr]) {
        let pat = self.scratch.alloc_i32();
        let text = self.scratch.alloc_i32();
        let alts = self.scratch.alloc_i32();
        let caps = self.scratch.alloc_i32();
        let ncap = self.scratch.alloc_i32();
        let pos = self.scratch.alloc_i32();
        let end_pos = self.scratch.alloc_i32();
        let match_start = self.scratch.alloc_i32();
        let text_len = self.scratch.alloc_i32();
        let count = self.scratch.alloc_i32();
        let buf = self.scratch.alloc_i32();
        let segment = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();
        let width = self.scratch.alloc_i32();

        let list_hdr = self.emitter.layout_reg.header_size(super::engine::layout::LIST) as i32;
        let list_data = self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32;

        self.emit_expr(&args[0]);
        wasm!(self.func, { local_set(pat); });
        self.emit_expr(&args[1]);
        wasm!(self.func, { local_set(text); });
        self.emit_compile_pattern(pat, alts, caps, ncap);
        wasm!(self.func, {
            local_get(text); i32_load(0); local_set(text_len);
            // upper bound on segments: text_len+2
            local_get(text_len); i32_const(2); i32_add; i32_const(PTR_BYTES); i32_mul;
            call(self.emitter.rt.alloc); local_set(buf);
            i32_const(0); local_set(count);
            i32_const(0); local_set(pos);

            block_empty; loop_empty;
                local_get(pos); local_get(text_len); i32_gt_u; br_if(1);

                local_get(alts); local_get(text); local_get(pos); local_get(caps); local_get(ncap);
                call(self.emitter.rt.regex.find_at);
                local_set(end_pos);
                local_get(end_pos); i32_const(-1); i32_eq;
                if_empty;
                    // no more matches: push rest and break
                    local_get(text); local_get(pos); local_get(text_len);
                    call(self.emitter.rt.string.slice); local_set(segment);
                    local_get(buf); local_get(count); i32_const(PTR_BYTES); i32_mul; i32_add;
                    local_get(segment); i32_store(0);
                    local_get(count); i32_const(1); i32_add; local_set(count);
                    br(2);
                end;

                global_get(self.emitter.rt.regex.match_start_global); local_set(match_start);

                // zero-width at current pos: take one scalar, move on
                local_get(end_pos); local_get(match_start); i32_eq;
                local_get(match_start); local_get(pos); i32_eq;
                i32_and;
                if_empty;
                    local_get(pos); local_get(text_len); i32_lt_u;
                    if_empty;
                        local_get(text); local_get(pos);
                        call(self.emitter.rt.string.utf8_width); local_set(width);
                        local_get(text); local_get(pos);
                        local_get(pos); local_get(width); i32_add;
                        call(self.emitter.rt.string.slice); local_set(segment);
                        local_get(buf); local_get(count); i32_const(PTR_BYTES); i32_mul; i32_add;
                        local_get(segment); i32_store(0);
                        local_get(count); i32_const(1); i32_add; local_set(count);
                        local_get(pos); local_get(width); i32_add; local_set(pos);
                        // continue loop: 2 `if`s deep (if(zerowidth)/if(pos<len)),
                        // loop is the 3rd level → br(2).
                        br(2);
                    else_;
                        // break out to after the block (pos == text_len) → br(3).
                        br(3);
                    end;
                end;

                // normal: push text[pos..match_start], advance to end
                local_get(text); local_get(pos); local_get(match_start);
                call(self.emitter.rt.string.slice); local_set(segment);
                local_get(buf); local_get(count); i32_const(PTR_BYTES); i32_mul; i32_add;
                local_get(segment); i32_store(0);
                local_get(count); i32_const(1); i32_add; local_set(count);
                local_get(end_pos); local_set(pos);
                br(0);
            end; end;

            // build result list
            local_get(count); i32_const(PTR_BYTES); i32_mul; i32_const(list_hdr); i32_add;
            call(self.emitter.rt.alloc); local_set(result);
            local_get(result); local_get(count); i32_store(0);
            i32_const(0); local_set(pos);
            block_empty; loop_empty;
                local_get(pos); local_get(count); i32_ge_u; br_if(1);
                local_get(result); i32_const(list_data); i32_add;
                local_get(pos); i32_const(PTR_BYTES); i32_mul; i32_add;
                local_get(buf); local_get(pos); i32_const(PTR_BYTES); i32_mul; i32_add; i32_load(0);
                i32_store(0);
                local_get(pos); i32_const(1); i32_add; local_set(pos);
                br(0);
            end; end;
            local_get(result);
        });

        self.scratch.free_i32(width);
        self.scratch.free_i32(result);
        self.scratch.free_i32(segment);
        self.scratch.free_i32(buf);
        self.scratch.free_i32(count);
        self.scratch.free_i32(text_len);
        self.scratch.free_i32(match_start);
        self.scratch.free_i32(end_pos);
        self.scratch.free_i32(pos);
        self.scratch.free_i32(ncap);
        self.scratch.free_i32(caps);
        self.scratch.free_i32(alts);
        self.scratch.free_i32(text);
        self.scratch.free_i32(pat);
    }

    /// regex.captures(pattern, text) → Option[List[String]]
    /// None when ncap==0 (native returns None even on match if no groups).
    /// Each slot maps to a substring, or "" for an unset (optional-miss) slot.
    fn emit_regex_captures(&mut self, args: &[IrExpr]) {
        let pat = self.scratch.alloc_i32();
        let text = self.scratch.alloc_i32();
        let alts = self.scratch.alloc_i32();
        let caps = self.scratch.alloc_i32();
        let ncap = self.scratch.alloc_i32();
        let end_pos = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let grp_start = self.scratch.alloc_i32();
        let grp_end = self.scratch.alloc_i32();
        let str_ptr = self.scratch.alloc_i32();
        let list_ptr = self.scratch.alloc_i32();
        let opt_ptr = self.scratch.alloc_i32();

        let list_hdr = self.emitter.layout_reg.header_size(super::engine::layout::LIST) as i32;
        let list_data = self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32;
        let empty_str = self.emitter.intern_string("");

        self.emit_expr(&args[0]);
        wasm!(self.func, { local_set(pat); });
        self.emit_expr(&args[1]);
        wasm!(self.func, { local_set(text); });
        self.emit_compile_pattern(pat, alts, caps, ncap);
        wasm!(self.func, {
            // ncap == 0 → None
            local_get(ncap); i32_eqz;
            if_i32;
                i32_const(0);
            else_;
                local_get(alts); local_get(text); i32_const(0); local_get(caps); local_get(ncap);
                call(self.emitter.rt.regex.find_at);
                local_set(end_pos);
                local_get(end_pos); i32_const(-1); i32_eq;
                if_i32;
                    i32_const(0); // None
                else_;
                    // build list of ncap strings
                    local_get(ncap); i32_const(PTR_BYTES); i32_mul; i32_const(list_hdr); i32_add;
                    call(self.emitter.rt.alloc); local_set(list_ptr);
                    local_get(list_ptr); local_get(ncap); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                        local_get(i); local_get(ncap); i32_ge_u; br_if(1);
                        local_get(caps); local_get(i); i32_const(RX_CAP_BYTES); i32_mul; i32_add; i32_load(0);
                        local_set(grp_start);
                        local_get(caps); local_get(i); i32_const(RX_CAP_BYTES); i32_mul; i32_add; i32_load(4);
                        local_set(grp_end);
                        // unset (-1) → empty string
                        local_get(grp_start); i32_const(-1); i32_eq;
                        if_i32;
                            i32_const(empty_str as i32);
                        else_;
                            local_get(text); local_get(grp_start); local_get(grp_end);
                            call(self.emitter.rt.string.slice);
                        end;
                        local_set(str_ptr);
                        local_get(list_ptr); i32_const(list_data); i32_add;
                        local_get(i); i32_const(PTR_BYTES); i32_mul; i32_add;
                        local_get(str_ptr); i32_store(0);
                        local_get(i); i32_const(1); i32_add; local_set(i);
                        br(0);
                    end; end;
                    // wrap in Option some
                    i32_const(4); call(self.emitter.rt.alloc); local_set(opt_ptr);
                    local_get(opt_ptr); local_get(list_ptr); i32_store(0);
                    local_get(opt_ptr);
                end;
            end;
        });

        self.scratch.free_i32(opt_ptr);
        self.scratch.free_i32(list_ptr);
        self.scratch.free_i32(str_ptr);
        self.scratch.free_i32(grp_end);
        self.scratch.free_i32(grp_start);
        self.scratch.free_i32(i);
        self.scratch.free_i32(end_pos);
        self.scratch.free_i32(ncap);
        self.scratch.free_i32(caps);
        self.scratch.free_i32(alts);
        self.scratch.free_i32(text);
        self.scratch.free_i32(pat);
    }
}
