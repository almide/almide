//! Regex module call dispatch for WASM codegen.
//!
//! Functions:
//!   regex.is_match(pattern, text) → Bool
//!   regex.full_match(pattern, text) → Bool
//!   regex.find(pattern, text) → Option[String]
//!   regex.find_all(pattern, text) → List[String]
//!   regex.replace(pattern, text, replacement) → String
//!   regex.replace_first(pattern, text, replacement) → String
//!   regex.split(pattern, text) → List[String]
//!   regex.captures(pattern, text) → Option[List[String]]

use super::FuncCompiler;
use crate::ir::IrExpr;

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
            _ => self.emit_stub_call(args),
        }
    }

    /// regex.is_match(pattern, text) → Bool (i32: 0 or 1)
    fn emit_regex_is_match(&mut self, args: &[IrExpr]) {
        let pat = self.scratch.alloc_i32();
        let text = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();

        self.emit_expr(&args[0]);
        wasm!(self.func, { local_set(pat); });
        self.emit_expr(&args[1]);
        wasm!(self.func, {
            local_set(text);
            local_get(pat);
            local_get(text);
            i32_const(0);
            call(self.emitter.rt.regex.match_search);
            i32_const(-1);
            i32_ne;
        });

        self.scratch.free_i32(result);
        self.scratch.free_i32(text);
        self.scratch.free_i32(pat);
    }

    /// regex.full_match(pattern, text) → Bool
    fn emit_regex_full_match(&mut self, args: &[IrExpr]) {
        let pat = self.scratch.alloc_i32();
        let text = self.scratch.alloc_i32();
        let end_pos = self.scratch.alloc_i32();
        let text_len = self.scratch.alloc_i32();

        self.emit_expr(&args[0]);
        wasm!(self.func, { local_set(pat); });
        self.emit_expr(&args[1]);
        wasm!(self.func, {
            local_set(text);
            local_get(text); i32_load(0); local_set(text_len);
            // Try anchored match from position 0
            local_get(pat); local_get(text);
            i32_const(0); // pat_pos
            i32_const(0); // text_pos
            call(self.emitter.rt.regex.match_anchored);
            local_set(end_pos);
            // full_match = end_pos == text_len
            local_get(end_pos); local_get(text_len); i32_eq;
        });

        self.scratch.free_i32(text_len);
        self.scratch.free_i32(end_pos);
        self.scratch.free_i32(text);
        self.scratch.free_i32(pat);
    }

    /// regex.find(pattern, text) → Option[String]
    fn emit_regex_find(&mut self, args: &[IrExpr]) {
        let pat = self.scratch.alloc_i32();
        let text = self.scratch.alloc_i32();
        let end_pos = self.scratch.alloc_i32();
        let match_start = self.scratch.alloc_i32();
        let str_ptr = self.scratch.alloc_i32();
        let opt_ptr = self.scratch.alloc_i32();

        self.emit_expr(&args[0]);
        wasm!(self.func, { local_set(pat); });
        self.emit_expr(&args[1]);
        wasm!(self.func, {
            local_set(text);
            local_get(pat); local_get(text); i32_const(0);
            call(self.emitter.rt.regex.match_search);
            local_set(end_pos);
            local_get(end_pos); i32_const(-1); i32_eq;
            if_i32;
                i32_const(0); // none
            else_;
                // Get match_start from global
                global_get(self.emitter.rt.regex.match_start_global);
                local_set(match_start);
                // slice text[match_start..end_pos]
                local_get(text);
                local_get(match_start);
                local_get(end_pos);
                call(self.emitter.rt.string.slice);
                local_set(str_ptr);
                // Wrap in option some: alloc 4 bytes, store str_ptr
                i32_const(4); call(self.emitter.rt.alloc); local_set(opt_ptr);
                local_get(opt_ptr); local_get(str_ptr); i32_store(0);
                local_get(opt_ptr);
            end;
        });

        self.scratch.free_i32(opt_ptr);
        self.scratch.free_i32(str_ptr);
        self.scratch.free_i32(match_start);
        self.scratch.free_i32(end_pos);
        self.scratch.free_i32(text);
        self.scratch.free_i32(pat);
    }

    /// regex.find_all(pattern, text) → List[String]
    fn emit_regex_find_all(&mut self, args: &[IrExpr]) {
        let pat = self.scratch.alloc_i32();
        let text = self.scratch.alloc_i32();
        let pos = self.scratch.alloc_i32();
        let end_pos = self.scratch.alloc_i32();
        let match_start = self.scratch.alloc_i32();
        let text_len = self.scratch.alloc_i32();
        let count = self.scratch.alloc_i32();
        let buf = self.scratch.alloc_i32();
        let str_ptr = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();

        self.emit_expr(&args[0]);
        wasm!(self.func, { local_set(pat); });
        self.emit_expr(&args[1]);
        wasm!(self.func, {
            local_set(text);
            local_get(text); i32_load(0); local_set(text_len);

            // Allocate temp buffer for up to 256 matches (ptrs)
            i32_const(1024); call(self.emitter.rt.alloc); local_set(buf);
            i32_const(0); local_set(count);
            i32_const(0); local_set(pos);

            // Find all matches
            block_empty; loop_empty;
                local_get(pos); local_get(text_len); i32_gt_u; br_if(1);
                local_get(count); i32_const(255); i32_ge_u; br_if(1);

                local_get(pat); local_get(text); local_get(pos);
                call(self.emitter.rt.regex.match_search);
                local_set(end_pos);
                local_get(end_pos); i32_const(-1); i32_eq; br_if(1);

                global_get(self.emitter.rt.regex.match_start_global);
                local_set(match_start);

                // Slice the match
                local_get(text); local_get(match_start); local_get(end_pos);
                call(self.emitter.rt.string.slice);
                local_set(str_ptr);

                // Store in buffer
                local_get(buf); local_get(count); i32_const(4); i32_mul; i32_add;
                local_get(str_ptr); i32_store(0);
                local_get(count); i32_const(1); i32_add; local_set(count);

                // Advance: if match was empty, advance by 1
                local_get(end_pos); local_get(match_start); i32_eq;
                if_empty;
                    local_get(end_pos); i32_const(1); i32_add; local_set(pos);
                else_;
                    local_get(end_pos); local_set(pos);
                end;
                br(0);
            end; end;

            // Build result list: [len:i32][ptr0..ptrN]
            local_get(count); i32_const(4); i32_mul; i32_const(4); i32_add;
            call(self.emitter.rt.alloc);
            local_set(result);
            local_get(result); local_get(count); i32_store(0);

            // Copy ptrs from buf to result
            i32_const(0); local_set(pos);
            block_empty; loop_empty;
                local_get(pos); local_get(count); i32_ge_u; br_if(1);
                local_get(result); i32_const(4); i32_add;
                local_get(pos); i32_const(4); i32_mul; i32_add;
                local_get(buf); local_get(pos); i32_const(4); i32_mul; i32_add; i32_load(0);
                i32_store(0);
                local_get(pos); i32_const(1); i32_add; local_set(pos);
                br(0);
            end; end;

            local_get(result);
        });

        self.scratch.free_i32(result);
        self.scratch.free_i32(str_ptr);
        self.scratch.free_i32(buf);
        self.scratch.free_i32(count);
        self.scratch.free_i32(text_len);
        self.scratch.free_i32(match_start);
        self.scratch.free_i32(end_pos);
        self.scratch.free_i32(pos);
        self.scratch.free_i32(text);
        self.scratch.free_i32(pat);
    }

    /// regex.replace(pattern, text, replacement) → String
    fn emit_regex_replace(&mut self, args: &[IrExpr]) {
        let pat = self.scratch.alloc_i32();
        let text = self.scratch.alloc_i32();
        let repl = self.scratch.alloc_i32();
        let pos = self.scratch.alloc_i32();
        let end_pos = self.scratch.alloc_i32();
        let match_start = self.scratch.alloc_i32();
        let text_len = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();
        let segment = self.scratch.alloc_i32();

        let empty_str = self.emitter.intern_string("");

        self.emit_expr(&args[0]);
        wasm!(self.func, { local_set(pat); });
        self.emit_expr(&args[1]);
        wasm!(self.func, { local_set(text); });
        self.emit_expr(&args[2]);
        wasm!(self.func, {
            local_set(repl);
            local_get(text); i32_load(0); local_set(text_len);
            i32_const(empty_str as i32); local_set(result); // start with empty string
            i32_const(0); local_set(pos);

            block_empty; loop_empty;
                local_get(pos); local_get(text_len); i32_gt_u; br_if(1);

                local_get(pat); local_get(text); local_get(pos);
                call(self.emitter.rt.regex.match_search);
                local_set(end_pos);
                local_get(end_pos); i32_const(-1); i32_eq;
                if_empty;
                    // No more matches: append rest of text
                    local_get(text); local_get(pos); local_get(text_len);
                    call(self.emitter.rt.string.slice);
                    local_set(segment);
                    local_get(result); local_get(segment);
                    call(self.emitter.rt.concat_str);
                    local_set(result);
                    br(1);
                end;

                global_get(self.emitter.rt.regex.match_start_global);
                local_set(match_start);

                // Append text before match
                local_get(text); local_get(pos); local_get(match_start);
                call(self.emitter.rt.string.slice);
                local_set(segment);
                local_get(result); local_get(segment);
                call(self.emitter.rt.concat_str);
                local_set(result);

                // Append replacement
                local_get(result); local_get(repl);
                call(self.emitter.rt.concat_str);
                local_set(result);

                // Advance past match
                local_get(end_pos); local_get(match_start); i32_eq;
                if_empty;
                    local_get(end_pos); i32_const(1); i32_add; local_set(pos);
                else_;
                    local_get(end_pos); local_set(pos);
                end;
                br(0);
            end; end;

            local_get(result);
        });

        self.scratch.free_i32(segment);
        self.scratch.free_i32(result);
        self.scratch.free_i32(text_len);
        self.scratch.free_i32(match_start);
        self.scratch.free_i32(end_pos);
        self.scratch.free_i32(pos);
        self.scratch.free_i32(repl);
        self.scratch.free_i32(text);
        self.scratch.free_i32(pat);
    }

    /// regex.replace_first(pattern, text, replacement) → String
    fn emit_regex_replace_first(&mut self, args: &[IrExpr]) {
        let pat = self.scratch.alloc_i32();
        let text = self.scratch.alloc_i32();
        let repl = self.scratch.alloc_i32();
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
        wasm!(self.func, {
            local_set(repl);
            local_get(text); i32_load(0); local_set(text_len);

            local_get(pat); local_get(text); i32_const(0);
            call(self.emitter.rt.regex.match_search);
            local_set(end_pos);
            local_get(end_pos); i32_const(-1); i32_eq;
            if_i32;
                // No match: return text as-is
                local_get(text);
            else_;
                global_get(self.emitter.rt.regex.match_start_global);
                local_set(match_start);
                // before + replacement + after
                local_get(text); i32_const(0); local_get(match_start);
                call(self.emitter.rt.string.slice);
                local_set(before);
                local_get(text); local_get(end_pos); local_get(text_len);
                call(self.emitter.rt.string.slice);
                local_set(after);
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
        self.scratch.free_i32(repl);
        self.scratch.free_i32(text);
        self.scratch.free_i32(pat);
    }

    /// regex.split(pattern, text) → List[String]
    fn emit_regex_split(&mut self, args: &[IrExpr]) {
        let pat = self.scratch.alloc_i32();
        let text = self.scratch.alloc_i32();
        let pos = self.scratch.alloc_i32();
        let end_pos = self.scratch.alloc_i32();
        let match_start = self.scratch.alloc_i32();
        let text_len = self.scratch.alloc_i32();
        let count = self.scratch.alloc_i32();
        let buf = self.scratch.alloc_i32();
        let segment = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();

        self.emit_expr(&args[0]);
        wasm!(self.func, { local_set(pat); });
        self.emit_expr(&args[1]);
        wasm!(self.func, {
            local_set(text);
            local_get(text); i32_load(0); local_set(text_len);

            // Allocate temp buffer for up to 256 segments
            i32_const(1024); call(self.emitter.rt.alloc); local_set(buf);
            i32_const(0); local_set(count);
            i32_const(0); local_set(pos);

            block_empty; loop_empty;
                local_get(pos); local_get(text_len); i32_gt_u; br_if(1);
                local_get(count); i32_const(255); i32_ge_u; br_if(1);

                local_get(pat); local_get(text); local_get(pos);
                call(self.emitter.rt.regex.match_search);
                local_set(end_pos);
                local_get(end_pos); i32_const(-1); i32_eq;
                if_empty;
                    // No more matches: add rest
                    local_get(text); local_get(pos); local_get(text_len);
                    call(self.emitter.rt.string.slice);
                    local_set(segment);
                    local_get(buf); local_get(count); i32_const(4); i32_mul; i32_add;
                    local_get(segment); i32_store(0);
                    local_get(count); i32_const(1); i32_add; local_set(count);
                    br(1);
                end;

                global_get(self.emitter.rt.regex.match_start_global);
                local_set(match_start);
        });
        // Add segment before match
        wasm!(self.func, {
                local_get(text); local_get(pos); local_get(match_start);
                call(self.emitter.rt.string.slice);
                local_set(segment);
                local_get(buf); local_get(count); i32_const(4); i32_mul; i32_add;
                local_get(segment); i32_store(0);
                local_get(count); i32_const(1); i32_add; local_set(count);

                // Advance past match
                local_get(end_pos); local_get(match_start); i32_eq;
                if_empty;
                    local_get(end_pos); i32_const(1); i32_add; local_set(pos);
                else_;
                    local_get(end_pos); local_set(pos);
                end;
                br(0);
            end; end;
        });
        // If no segments, add full text
        wasm!(self.func, {
            local_get(count); i32_eqz;
            if_empty;
                local_get(text); local_set(segment);
                local_get(buf); local_get(count); i32_const(4); i32_mul; i32_add;
                local_get(segment); i32_store(0);
                local_get(count); i32_const(1); i32_add; local_set(count);
            end;
        });
        // Build result list
        wasm!(self.func, {
            local_get(count); i32_const(4); i32_mul; i32_const(4); i32_add;
            call(self.emitter.rt.alloc);
            local_set(result);
            local_get(result); local_get(count); i32_store(0);
            i32_const(0); local_set(pos);
            block_empty; loop_empty;
                local_get(pos); local_get(count); i32_ge_u; br_if(1);
                local_get(result); i32_const(4); i32_add;
                local_get(pos); i32_const(4); i32_mul; i32_add;
                local_get(buf); local_get(pos); i32_const(4); i32_mul; i32_add; i32_load(0);
                i32_store(0);
                local_get(pos); i32_const(1); i32_add; local_set(pos);
                br(0);
            end; end;
            local_get(result);
        });

        self.scratch.free_i32(result);
        self.scratch.free_i32(segment);
        self.scratch.free_i32(buf);
        self.scratch.free_i32(count);
        self.scratch.free_i32(text_len);
        self.scratch.free_i32(match_start);
        self.scratch.free_i32(end_pos);
        self.scratch.free_i32(pos);
        self.scratch.free_i32(text);
        self.scratch.free_i32(pat);
    }

    /// regex.captures(pattern, text) → Option[List[String]]
    fn emit_regex_captures(&mut self, args: &[IrExpr]) {
        self.emit_expr(&args[0]);
        let pat = self.scratch.alloc_i32();
        wasm!(self.func, { local_set(pat); });
        self.emit_expr(&args[1]);
        let text = self.scratch.alloc_i32();
        wasm!(self.func, {
            local_set(text);
            local_get(pat); local_get(text); i32_const(0);
            call(self.emitter.rt.regex.captures_search);
            // Returns Option[List[String]]: ptr or 0
        });
        self.scratch.free_i32(text);
        self.scratch.free_i32(pat);
    }
}
