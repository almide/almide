impl FuncCompiler<'_> {
    /// `bytes.reverse(b) -> Bytes`. Allocates a fresh buffer.
    pub(super) fn emit_bytes_reverse(&mut self, args: &[IrExpr]) {
        let buf = self.scratch.alloc_i32();
        let len = self.scratch.alloc_i32();
        let dst = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        self.emit_expr(&args[0]);
        wasm!(self.func, {
            local_set(buf);
            local_get(buf); i32_load(0); local_set(len);
            local_get(len); call(self.emitter.rt.string_alloc); local_set(dst);
            local_get(dst); local_get(len); i32_store(0);
            i32_const(0); local_set(i);
            block_empty; loop_empty;
                local_get(i); local_get(len); i32_ge_u; br_if(1);
                // dst[4 + i] = buf[4 + (len - 1 - i)]
                local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(i); i32_add;
                local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
                local_get(len); i32_const(1); i32_sub; local_get(i); i32_sub; i32_add;
                i32_load8_u(0);
                i32_store8(0);
                local_get(i); i32_const(1); i32_add; local_set(i);
                br(0);
            end; end;
            local_get(dst);
        });
        self.scratch.free_i32(i);
        self.scratch.free_i32(dst);
        self.scratch.free_i32(len);
        self.scratch.free_i32(buf);
    }

    /// `bytes.fill(b, val)` — overwrite all bytes in place.
    pub(super) fn emit_bytes_fill(&mut self, args: &[IrExpr]) {
        let buf = self.scratch.alloc_i32();
        let val = self.scratch.alloc_i32();
        let len = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        self.emit_expr(&args[0]);
        wasm!(self.func, { local_set(buf); });
        self.emit_expr(&args[1]);
        wasm!(self.func, {
            i32_wrap_i64; local_set(val);
            local_get(buf); i32_load(0); local_set(len);
            i32_const(0); local_set(i);
            block_empty; loop_empty;
                local_get(i); local_get(len); i32_ge_u; br_if(1);
                local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(i); i32_add;
                local_get(val); i32_store8(0);
                local_get(i); i32_const(1); i32_add; local_set(i);
                br(0);
            end; end;
        });
        self.scratch.free_i32(i);
        self.scratch.free_i32(len);
        self.scratch.free_i32(val);
        self.scratch.free_i32(buf);
    }

    /// `bytes.insert(b, pos, val) -> Bytes`. Returns a fresh buffer of length
    /// `len(b) + 1`. `pos` clamps to `[0, len(b)]`.
    pub(super) fn emit_bytes_insert(&mut self, args: &[IrExpr]) {
        let buf = self.scratch.alloc_i32();
        let pos = self.scratch.alloc_i32();
        let val = self.scratch.alloc_i32();
        let len = self.scratch.alloc_i32();
        let dst = self.scratch.alloc_i32();
        self.emit_expr(&args[0]); wasm!(self.func, { local_set(buf); });
        self.emit_expr(&args[1]); wasm!(self.func, { i32_wrap_i64; local_set(pos); });
        self.emit_expr(&args[2]); wasm!(self.func, {
            i32_wrap_i64; local_set(val);
            local_get(buf); i32_load(0); local_set(len);
            // Clamp pos to [0, len]
            local_get(pos); i32_const(0); i32_lt_s;
            if_empty; i32_const(0); local_set(pos); end;
            local_get(pos); local_get(len); i32_gt_u;
            if_empty; local_get(len); local_set(pos); end;
            // alloc len + 5
            local_get(len); i32_const(1); i32_add; call(self.emitter.rt.string_alloc); local_set(dst);
            // memcpy [0, pos)
            local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
            local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
            local_get(pos);
            memory_copy;
            // store val at dst+data_off+pos
            local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(pos); i32_add;
            local_get(val); i32_store8(0);
            // memcpy [pos, len) → dst+data_off+pos+1
            local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32 + 1); i32_add; local_get(pos); i32_add;
            local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(pos); i32_add;
            local_get(len); local_get(pos); i32_sub;
            memory_copy;
            local_get(dst);
        });
        self.scratch.free_i32(dst);
        self.scratch.free_i32(len);
        self.scratch.free_i32(val);
        self.scratch.free_i32(pos);
        self.scratch.free_i32(buf);
    }

    /// `bytes.remove_at(b, pos) -> Bytes`. Out-of-range returns clone.
    pub(super) fn emit_bytes_remove_at(&mut self, args: &[IrExpr]) {
        let buf = self.scratch.alloc_i32();
        let pos = self.scratch.alloc_i32();
        let len = self.scratch.alloc_i32();
        let dst = self.scratch.alloc_i32();
        self.emit_expr(&args[0]); wasm!(self.func, { local_set(buf); });
        self.emit_expr(&args[1]); wasm!(self.func, {
            i32_wrap_i64; local_set(pos);
            local_get(buf); i32_load(0); local_set(len);
            // If pos out of range → clone len+4 bytes
            local_get(pos); local_get(len); i32_ge_u;
            if_i32;
                local_get(len); call(self.emitter.rt.string_alloc); local_set(dst);
                local_get(dst); local_get(buf); local_get(len); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; memory_copy;
                local_get(dst);
            else_;
                local_get(len); i32_const(1); i32_sub; call(self.emitter.rt.string_alloc); local_set(dst);
                // memcpy [0, pos)
                local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
                local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
                local_get(pos);
                memory_copy;
                // memcpy [pos+1, len)
                local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(pos); i32_add;
                local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32 + 1); i32_add; local_get(pos); i32_add;
                local_get(len); local_get(pos); i32_sub; i32_const(1); i32_sub;
                memory_copy;
                local_get(dst);
            end;
        });
        self.scratch.free_i32(dst);
        self.scratch.free_i32(len);
        self.scratch.free_i32(pos);
        self.scratch.free_i32(buf);
    }

    /// `bytes.chunks(b, size) -> List[Bytes]`. Builds a fresh List with one
    /// fresh Bytes per chunk (last may be shorter).
    pub(super) fn emit_bytes_chunks(&mut self, args: &[IrExpr]) {
        let buf = self.scratch.alloc_i32();
        let size = self.scratch.alloc_i32();
        let len = self.scratch.alloc_i32();
        let n_chunks = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let off = self.scratch.alloc_i32();
        let chunk_len = self.scratch.alloc_i32();
        let chunk = self.scratch.alloc_i32();
        self.emit_expr(&args[0]); wasm!(self.func, { local_set(buf); });
        self.emit_expr(&args[1]); wasm!(self.func, {
            i32_wrap_i64; local_set(size);
            local_get(buf); i32_load(0); local_set(len);
            // n_chunks = ceil(len / size); if size == 0 → 0
            local_get(size); i32_eqz;
            if_i32; i32_const(0);
            else_;
                local_get(len); local_get(size); i32_add; i32_const(1); i32_sub;
                local_get(size); i32_div_u;
            end;
            local_set(n_chunks);
            // alloc List header: 4 + n_chunks*4
            local_get(n_chunks); i32_const(4); i32_mul; i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
            call(self.emitter.rt.alloc); local_set(result);
            local_get(result); local_get(n_chunks); i32_store(0);
            i32_const(0); local_set(i);
            i32_const(0); local_set(off);
            block_empty; loop_empty;
                local_get(i); local_get(n_chunks); i32_ge_u; br_if(1);
                // chunk_len = min(size, len - off)
                local_get(len); local_get(off); i32_sub;
                local_get(size); local_get(len); local_get(off); i32_sub; i32_lt_u;
                if_i32; local_get(size); else_; local_get(len); local_get(off); i32_sub; end;
                local_set(chunk_len);
                // alloc chunk: 4 + chunk_len
                local_get(chunk_len); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
                call(self.emitter.rt.alloc); local_set(chunk);
                local_get(chunk); local_get(chunk_len); i32_store(0);
                local_get(chunk); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
                local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(off); i32_add;
                local_get(chunk_len);
                memory_copy;
                // result.elems[i] = chunk
                local_get(result); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(i); i32_const(4); i32_mul; i32_add;
                local_get(chunk); i32_store(0);
                local_get(off); local_get(size); i32_add; local_set(off);
                local_get(i); i32_const(1); i32_add; local_set(i);
                br(0);
            end; end;
            local_get(result);
        });
        self.scratch.free_i32(chunk);
        self.scratch.free_i32(chunk_len);
        self.scratch.free_i32(off);
        self.scratch.free_i32(i);
        self.scratch.free_i32(result);
        self.scratch.free_i32(n_chunks);
        self.scratch.free_i32(len);
        self.scratch.free_i32(size);
        self.scratch.free_i32(buf);
    }

    /// `bytes.split(b, sep) -> List[Bytes]` and `bytes.lines(b) -> List[Bytes]`.
    /// Two-pass implementation: first count parts, then alloc List + chunks.
    /// `lf=true` uses a hardcoded `'\n'` separator (and ignores `sep` arg).
    pub(super) fn emit_bytes_split(&mut self, args: &[IrExpr], _single_byte: bool, lf: bool) {
        let buf = self.scratch.alloc_i32();
        let sep = self.scratch.alloc_i32();
        let blen = self.scratch.alloc_i32();
        let plen = self.scratch.alloc_i32();
        let count = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let j = self.scratch.alloc_i32();
        let start = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();
        let chunk = self.scratch.alloc_i32();
        let chunk_len = self.scratch.alloc_i32();
        let out_idx = self.scratch.alloc_i32();
        self.emit_expr(&args[0]); wasm!(self.func, { local_set(buf); });
        if lf {
            // sep is implicit "\n" — alloc a 1-byte sep buffer at runtime.
            wasm!(self.func, {
                i32_const(1); call(self.emitter.rt.string_alloc); local_set(sep);
                local_get(sep); i32_const(1); i32_store(0);
                local_get(sep); i32_const(1); i32_store(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::CAP) as i32 as u32, 0);
                local_get(sep); i32_const(10); i32_store8(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32 as u32);
            });
        } else {
            self.emit_expr(&args[1]);
            wasm!(self.func, { local_set(sep); });
        }
        wasm!(self.func, {
            local_get(buf); i32_load(0); local_set(blen);
            local_get(sep); i32_load(0); local_set(plen);
        });
        if lf {
            // For lines: count = number of '\n' bytes; trailing '\n' adds nothing.
            wasm!(self.func, {
                i32_const(0); local_set(count);
                i32_const(0); local_set(i);
                block_empty; loop_empty;
                    local_get(i); local_get(blen); i32_ge_u; br_if(1);
                    local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(i); i32_add;
                    i32_load8_u(0); i32_const(10); i32_eq;
                    if_empty; local_get(count); i32_const(1); i32_add; local_set(count); end;
                    local_get(i); i32_const(1); i32_add; local_set(i);
                    br(0);
                end; end;
                // If buffer doesn't end with newline, add 1 for the final line.
                local_get(blen); i32_eqz;
                if_empty;
                    // empty buffer → count stays 0
                else_;
                    local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(blen); i32_const(1); i32_sub; i32_add;
                    i32_load8_u(0); i32_const(10); i32_ne;
                    if_empty; local_get(count); i32_const(1); i32_add; local_set(count); end;
                end;
            });
        } else {
            // Generic split. Empty sep → 1 part (whole buffer).
            wasm!(self.func, {
                i32_const(1); local_set(count);
                local_get(plen); i32_eqz;
                if_empty;
                    // empty sep — count stays 1, skip scan
                else_;
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                        local_get(i); local_get(plen); i32_add; local_get(blen); i32_gt_u; br_if(1);
                        // compare buf[i..i+plen] == sep[0..plen]; out_idx = match flag
                        i32_const(0); local_set(j);
                        i32_const(1); local_set(out_idx);
                        block_empty; loop_empty;
                            local_get(j); local_get(plen); i32_ge_u; br_if(1);
                            local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(i); i32_add; local_get(j); i32_add;
                            i32_load8_u(0);
                            local_get(sep); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(j); i32_add;
                            i32_load8_u(0);
                            i32_ne;
                            if_empty;
                                i32_const(0); local_set(out_idx); br(2);
                            end;
                            local_get(j); i32_const(1); i32_add; local_set(j);
                            br(0);
                        end; end;
                        local_get(out_idx);
                        if_empty;
                            local_get(count); i32_const(1); i32_add; local_set(count);
                            local_get(i); local_get(plen); i32_add; local_set(i);
                        else_;
                            local_get(i); i32_const(1); i32_add; local_set(i);
                        end;
                        br(0);
                    end; end;
                end;
            });
        }
        // Second pass: build the actual list using count chunks.
        wasm!(self.func, {
            local_get(count); i32_const(4); i32_mul; i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
            call(self.emitter.rt.alloc); local_set(result);
            local_get(result); local_get(count); i32_store(0);
            i32_const(0); local_set(start);
            i32_const(0); local_set(out_idx);
            i32_const(0); local_set(i);
            block_empty; loop_empty;
                local_get(out_idx); local_get(count); i32_ge_u; br_if(1);
                // Find next sep starting at i (or end).
                block_empty; loop_empty;
                    local_get(i); local_get(plen); i32_add; local_get(blen); i32_gt_u; br_if(1);
                    // compare buf[i..i+plen] == sep
                    i32_const(0); local_set(j);
                    i32_const(1); local_set(chunk_len); // reuse: match flag
                    block_empty; loop_empty;
                        local_get(j); local_get(plen); i32_ge_u; br_if(1);
                        local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(i); i32_add; local_get(j); i32_add;
                        i32_load8_u(0);
                        local_get(sep); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(j); i32_add;
                        i32_load8_u(0);
                        i32_ne; if_empty; i32_const(0); local_set(chunk_len); br(2); end;
                        local_get(j); i32_const(1); i32_add; local_set(j);
                        br(0);
                    end; end;
                    local_get(chunk_len); br_if(1); // matched → break inner
                    local_get(i); i32_const(1); i32_add; local_set(i);
                    br(0);
                end; end;
                // chunk = buf[start..i] (or buf[start..blen] when no further match)
                local_get(i); local_get(plen); i32_add; local_get(blen); i32_gt_u;
                if_empty; local_get(blen); local_set(i); end;
                local_get(i); local_get(start); i32_sub; local_set(chunk_len);
                local_get(chunk_len); call(self.emitter.rt.string_alloc); local_set(chunk);
                local_get(chunk); local_get(chunk_len); i32_store(0);
                local_get(chunk); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
                local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(start); i32_add;
                local_get(chunk_len);
                memory_copy;
                local_get(result); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(out_idx); i32_const(4); i32_mul; i32_add;
                local_get(chunk); i32_store(0);
                local_get(out_idx); i32_const(1); i32_add; local_set(out_idx);
                local_get(i); local_get(plen); i32_add; local_set(i);
                local_get(i); local_set(start);
                br(0);
            end; end;
            local_get(result);
        });
        self.scratch.free_i32(out_idx);
        self.scratch.free_i32(chunk_len);
        self.scratch.free_i32(chunk);
        self.scratch.free_i32(result);
        self.scratch.free_i32(start);
        self.scratch.free_i32(j);
        self.scratch.free_i32(i);
        self.scratch.free_i32(count);
        self.scratch.free_i32(plen);
        self.scratch.free_i32(blen);
        self.scratch.free_i32(sep);
        self.scratch.free_i32(buf);
    }

    /// `bytes.starts_with` / `bytes.ends_with`. Both compare `pat` against a
    /// fixed-position window in `b` and return Bool. Result accumulated into
    /// a local to avoid block-result-type bookkeeping.
    pub(super) fn emit_bytes_prefix_match(&mut self, args: &[IrExpr], at_end: bool) {
        let b = self.scratch.alloc_i32();
        let pat = self.scratch.alloc_i32();
        let blen = self.scratch.alloc_i32();
        let plen = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let off = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();
        self.emit_expr(&args[0]); wasm!(self.func, { local_set(b); });
        self.emit_expr(&args[1]); wasm!(self.func, {
            local_set(pat);
            local_get(b); i32_load(0); local_set(blen);
            local_get(pat); i32_load(0); local_set(plen);
            i32_const(1); local_set(result);
            // If pat longer than b → false and skip loop.
            local_get(plen); local_get(blen); i32_gt_u;
            if_empty;
                i32_const(0); local_set(result);
            else_;
        });
        if at_end {
            wasm!(self.func, {
                local_get(blen); local_get(plen); i32_sub; local_set(off);
            });
        } else {
            wasm!(self.func, { i32_const(0); local_set(off); });
        }
        wasm!(self.func, {
                i32_const(0); local_set(i);
                block_empty; loop_empty;
                    local_get(i); local_get(plen); i32_ge_u; br_if(1);
                    local_get(b); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(off); i32_add; local_get(i); i32_add;
                    i32_load8_u(0);
                    local_get(pat); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(i); i32_add;
                    i32_load8_u(0);
                    i32_ne;
                    if_empty;
                        i32_const(0); local_set(result); br(2);
                    end;
                    local_get(i); i32_const(1); i32_add; local_set(i);
                    br(0);
                end; end;
            end;
            local_get(result);
        });
        self.scratch.free_i32(result);
        self.scratch.free_i32(off);
        self.scratch.free_i32(i);
        self.scratch.free_i32(plen);
        self.scratch.free_i32(blen);
        self.scratch.free_i32(pat);
        self.scratch.free_i32(b);
    }

    /// Shared core for `contains` / `index_of`: returns i64 position
    /// (or -1 sentinel if not found) on the WASM stack.
    pub(super) fn emit_bytes_index_of_inner(&mut self, args: &[IrExpr]) {
        let b = self.scratch.alloc_i32();
        let pat = self.scratch.alloc_i32();
        let blen = self.scratch.alloc_i32();
        let plen = self.scratch.alloc_i32();
        let limit = self.scratch.alloc_i32(); // last valid start = blen - plen
        let i = self.scratch.alloc_i32();
        let j = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();
        self.emit_expr(&args[0]); wasm!(self.func, { local_set(b); });
        self.emit_expr(&args[1]); wasm!(self.func, {
            local_set(pat);
            local_get(b); i32_load(0); local_set(blen);
            local_get(pat); i32_load(0); local_set(plen);
            i32_const(-1); local_set(result);
            // Empty pattern → 0
            local_get(plen); i32_eqz;
            if_empty;
                i32_const(0); local_set(result);
            else_;
                local_get(plen); local_get(blen); i32_gt_u;
                if_empty;
                    // result stays -1
                else_;
                    local_get(blen); local_get(plen); i32_sub; local_set(limit);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                        local_get(i); local_get(limit); i32_gt_u; br_if(1);
                        i32_const(0); local_set(j);
                        block_empty; loop_empty;
                            local_get(j); local_get(plen); i32_ge_u; br_if(1);
                            local_get(b); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(i); i32_add; local_get(j); i32_add;
                            i32_load8_u(0);
                            local_get(pat); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(j); i32_add;
                            i32_load8_u(0);
                            i32_ne; br_if(1);
                            local_get(j); i32_const(1); i32_add; local_set(j);
                            br(0);
                        end; end;
                        // If we reached j == plen, full match.
                        local_get(j); local_get(plen); i32_eq;
                        if_empty;
                            local_get(i); local_set(result);
                            br(3);
                        end;
                        local_get(i); i32_const(1); i32_add; local_set(i);
                        br(0);
                    end; end;
                end;
            end;
            local_get(result); i64_extend_i32_s;
        });
        self.scratch.free_i32(result);
        self.scratch.free_i32(j);
        self.scratch.free_i32(i);
        self.scratch.free_i32(limit);
        self.scratch.free_i32(plen);
        self.scratch.free_i32(blen);
        self.scratch.free_i32(pat);
        self.scratch.free_i32(b);
    }

    /// `bytes.cmp(a, b) -> Int` — byte-wise lexicographic comparison.
    pub(super) fn emit_bytes_cmp(&mut self, args: &[IrExpr]) {
        let a = self.scratch.alloc_i32();
        let b = self.scratch.alloc_i32();
        let alen = self.scratch.alloc_i32();
        let blen = self.scratch.alloc_i32();
        let minlen = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let av = self.scratch.alloc_i32();
        let bv = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();
        self.emit_expr(&args[0]); wasm!(self.func, { local_set(a); });
        self.emit_expr(&args[1]); wasm!(self.func, {
            local_set(b);
            local_get(a); i32_load(0); local_set(alen);
            local_get(b); i32_load(0); local_set(blen);
            // minlen = min(alen, blen)
            local_get(alen); local_get(blen); i32_lt_u;
            if_i32; local_get(alen); else_; local_get(blen); end;
            local_set(minlen);
            i32_const(0); local_set(result);
            i32_const(0); local_set(i);
            block_empty; loop_empty;
                local_get(i); local_get(minlen); i32_ge_u; br_if(1);
                local_get(a); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(i); i32_add; i32_load8_u(0); local_set(av);
                local_get(b); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(i); i32_add; i32_load8_u(0); local_set(bv);
                local_get(av); local_get(bv); i32_lt_u;
                if_empty; i32_const(-1); local_set(result); br(2); end;
                local_get(av); local_get(bv); i32_gt_u;
                if_empty; i32_const(1); local_set(result); br(2); end;
                local_get(i); i32_const(1); i32_add; local_set(i);
                br(0);
            end; end;
            // All shared bytes equal → shorter is less.
            local_get(result); i32_eqz;
            if_empty;
                local_get(alen); local_get(blen); i32_lt_u;
                if_empty; i32_const(-1); local_set(result); end;
                local_get(alen); local_get(blen); i32_gt_u;
                if_empty; i32_const(1); local_set(result); end;
            end;
            local_get(result); i64_extend_i32_s;
        });
        self.scratch.free_i32(result);
        self.scratch.free_i32(bv);
        self.scratch.free_i32(av);
        self.scratch.free_i32(i);
        self.scratch.free_i32(minlen);
        self.scratch.free_i32(blen);
        self.scratch.free_i32(alen);
        self.scratch.free_i32(b);
        self.scratch.free_i32(a);
    }

    /// `bytes.is_valid_utf8(b) -> Bool`. Shape-validates UTF-8 (catches invalid
    /// lead/follow bytes and short sequences; does not flag overlong forms or
    /// surrogates). Sufficient to reject obvious garbage like `0xFF` and to
    /// accept all well-formed Unicode strings.
    pub(super) fn emit_bytes_is_valid_utf8(&mut self, args: &[IrExpr]) {
        let buf = self.scratch.alloc_i32();
        let len = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let b = self.scratch.alloc_i32();
        let need = self.scratch.alloc_i32();
        let valid = self.scratch.alloc_i32();
        let k = self.scratch.alloc_i32();
        let fb = self.scratch.alloc_i32();
        self.emit_expr(&args[0]);
        wasm!(self.func, {
            local_set(buf);
            local_get(buf); i32_load(0); local_set(len);
            i32_const(0); local_set(i);
            i32_const(1); local_set(valid);
            block_empty; loop_empty;
                local_get(i); local_get(len); i32_ge_u; br_if(1);
                local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(i); i32_add; i32_load8_u(0); local_set(b);
                // ASCII fast path
                local_get(b); i32_const(128); i32_lt_u;
                if_empty;
                    local_get(i); i32_const(1); i32_add; local_set(i);
                    br(2); // continue outer loop
                end;
                // Determine number of follow-bytes
                local_get(b); i32_const(0xC2); i32_lt_u;
                if_empty;
                    i32_const(0); local_set(valid); br(2);
                end;
                local_get(b); i32_const(0xE0); i32_lt_u;
                if_i32; i32_const(1); else_;
                  local_get(b); i32_const(0xF0); i32_lt_u;
                  if_i32; i32_const(2); else_;
                    local_get(b); i32_const(0xF5); i32_lt_u;
                    if_i32; i32_const(3); else_; i32_const(-1); end;
                  end;
                end;
                local_set(need);
                // need == -1 → invalid
                local_get(need); i32_const(-1); i32_eq;
                if_empty;
                    i32_const(0); local_set(valid); br(2);
                end;
                // Bounds: i + 1 + need > len → invalid
                local_get(i); i32_const(1); i32_add; local_get(need); i32_add; local_get(len); i32_gt_u;
                if_empty;
                    i32_const(0); local_set(valid); br(2);
                end;
                // Walk follow-bytes
                i32_const(0); local_set(k);
                block_empty; loop_empty;
                    local_get(k); local_get(need); i32_ge_u; br_if(1);
                    local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32 + 1); i32_add;
                    local_get(i); i32_add; local_get(k); i32_add;
                    i32_load8_u(0); local_set(fb);
                    local_get(fb); i32_const(0x80); i32_lt_u;
                    local_get(fb); i32_const(0xC0); i32_ge_u; i32_or;
                    if_empty;
                        i32_const(0); local_set(valid); br(4);
                    end;
                    local_get(k); i32_const(1); i32_add; local_set(k);
                    br(0);
                end; end;
                local_get(i); i32_const(1); i32_add; local_get(need); i32_add; local_set(i);
                br(0);
            end; end;
            local_get(valid);
        });
        self.scratch.free_i32(fb);
        self.scratch.free_i32(k);
        self.scratch.free_i32(valid);
        self.scratch.free_i32(need);
        self.scratch.free_i32(b);
        self.scratch.free_i32(i);
        self.scratch.free_i32(len);
        self.scratch.free_i32(buf);
    }

}
