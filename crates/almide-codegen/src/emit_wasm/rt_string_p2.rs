// ── Slice / transform ──

fn compile_slice(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.slice];
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I32)]);
    wasm!(f, {
        local_get(2); local_get(1); i32_sub;
        call(emitter.rt.string_alloc); local_set(3);
        i32_const(0); local_set(4);
        block_empty; loop_empty;
          local_get(4); local_get(2); local_get(1); i32_sub; i32_ge_u; br_if(1);
          local_get(3); i32_const(string_data_off()); i32_add; local_get(4); i32_add;
          local_get(0); i32_const(string_data_off()); i32_add; local_get(1); i32_add; local_get(4); i32_add;
          i32_load8_u(0); i32_store8(0);
          local_get(4); i32_const(1); i32_add; local_set(4);
          br(0);
        end; end;
        local_get(3); end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.slice, type_idx, f));
}

/// reverse(s): reverse by CODEPOINT. Each codepoint's bytes are copied in
/// forward order, but whole codepoints are placed from the end of the output
/// toward the start — so multibyte sequences stay valid UTF-8 (native parity).
fn compile_reverse(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.reverse];
    // params: 0=s | locals: 1=blen, 2=result, 3=in_off, 4=out_off, 5=width, 6=k
    let mut f = Function::new([(6, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(1);                 // blen
        local_get(1); call(emitter.rt.string_alloc); local_set(2);
        i32_const(0); local_set(3);                              // in_off = 0
        local_get(1); local_set(4);                              // out_off = blen (write end-first)
        block_empty; loop_empty;
          local_get(3); local_get(1); i32_ge_u; br_if(1);
          // width of codepoint at in_off
          local_get(0); local_get(3); call(emitter.rt.string.utf8_width); local_set(5);
          // out_off -= width (start of this codepoint in the output)
          local_get(4); local_get(5); i32_sub; local_set(4);
          // copy width bytes forward: out[out_off + k] = in[in_off + k]
          i32_const(0); local_set(6);
          block_empty; loop_empty;
            local_get(6); local_get(5); i32_ge_u; br_if(1);
            local_get(2); i32_const(string_data_off()); i32_add; local_get(4); i32_add; local_get(6); i32_add;
            local_get(0); i32_const(string_data_off()); i32_add; local_get(3); i32_add; local_get(6); i32_add;
            i32_load8_u(0); i32_store8(0);
            local_get(6); i32_const(1); i32_add; local_set(6);
            br(0);
          end; end;
          local_get(3); local_get(5); i32_add; local_set(3);     // in_off += width
          br(0);
        end; end;
        local_get(2); end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.reverse, type_idx, f));
}

fn compile_repeat(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.repeat];
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_load(0); local_get(1); i32_mul; local_set(2);
        local_get(2); call(emitter.rt.string_alloc); local_set(3);
        i32_const(0); local_set(2); // reuse as offset
        block_empty; loop_empty;
          local_get(2); local_get(0); i32_load(0); local_get(1); i32_mul; i32_ge_u; br_if(1);
          local_get(3); i32_const(string_data_off()); i32_add; local_get(2); i32_add;
          local_get(0); i32_const(string_data_off()); i32_add;
          local_get(2); local_get(0); i32_load(0); i32_rem_u;
          i32_add; i32_load8_u(0); i32_store8(0);
          local_get(2); i32_const(1); i32_add; local_set(2);
          br(0);
        end; end;
        local_get(3); end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.repeat, type_idx, f));
}

fn compile_index_of(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.index_of];
    // params: 0=s, 1=needle | locals: 2=s_len, 3=n_len, 4=i, 5=result(i64)
    let mut f = Function::new([
        (1, ValType::I32), (1, ValType::I32), (1, ValType::I32), (1, ValType::I64),
    ]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(2);
        local_get(1); i32_load(0); local_set(3);
        i64_const(-1); local_set(5); // result = -1 (not found)
        // empty needle → 0
        local_get(3); i32_eqz;
        if_empty; i64_const(0); local_set(5); i64_const(0); return_; end;
        // n_len > s_len → -1
        local_get(3); local_get(2); i32_gt_u;
        if_empty; i64_const(-1); return_; end;
        // Scan
        i32_const(0); local_set(4);
        block_empty; loop_empty;
          local_get(4); local_get(2); local_get(3); i32_sub; i32_const(1); i32_add;
          i32_ge_u; br_if(1);
          local_get(0); i32_const(string_data_off()); i32_add; local_get(4); i32_add;
          local_get(1); i32_const(string_data_off()); i32_add;
          local_get(3);
          call(emitter.rt.mem_eq);
          if_empty;
            local_get(4); i64_extend_i32_u; return_;
          end;
          local_get(4); i32_const(1); i32_add; local_set(4);
          br(0);
        end; end;
        i64_const(-1); end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.index_of, type_idx, f));
}

/// Iterative replace via a count-then-build forward scan. Mirrors the native
/// oracle `s.replace(from, to)`. De-recursed (#634): the old recursion was one
/// frame per occurrence (and INFINITE on an empty `from`, since `index_of`
/// returns 0 forever), exhausting the wasm call stack. An empty `from` inserts
/// `to` at every codepoint boundary: `"abc".replace("","X") == "XaXbXcX"`.
fn compile_replace(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.replace];
    // params: 0=s, 1=from, 2=to
    // locals: 3=blen, 4=fl(from len), 5=tl(to len), 6=i(scan), 7=cnt,
    //         8=result, 9=out(write off), 10=width
    let mut f = Function::new([
        (1, ValType::I32), (1, ValType::I32), (1, ValType::I32),
        (1, ValType::I32), (1, ValType::I32), (1, ValType::I32),
        (1, ValType::I32), (1, ValType::I32),
    ]);
    let dat = string_data_off();
    wasm!(f, {
        local_get(0); i32_load(0); local_set(3); // blen
        local_get(1); i32_load(0); local_set(4); // fl
        local_get(2); i32_load(0); local_set(5); // tl
        // Empty `from`: insert `to` before every codepoint plus one at the end.
        // result_len = blen + (char_count + 1) * tl. Count codepoints first.
        local_get(4); i32_eqz;
        if_empty;
          // cnt = codepoint count (scan widths)
          i32_const(0); local_set(6); // i = 0 (byte offset)
          i32_const(0); local_set(7); // cnt = 0 (codepoints)
          block_empty; loop_empty;
            local_get(6); local_get(3); i32_ge_u; br_if(1);
            local_get(0); local_get(6); call(emitter.rt.string.utf8_width);
            local_get(6); i32_add; local_set(6);
            local_get(7); i32_const(1); i32_add; local_set(7);
            br(0);
          end; end;
          // result = string_alloc(blen + (cnt + 1) * tl)
          local_get(3); local_get(7); i32_const(1); i32_add; local_get(5); i32_mul; i32_add;
          call(emitter.rt.string_alloc); local_set(8);
          i32_const(0); local_set(9); // out = 0
          // leading `to`: result[out..] = to
          local_get(8); i32_const(dat); i32_add; local_get(9); i32_add;
          local_get(2); i32_const(dat); i32_add; local_get(5); memory_copy;
          local_get(9); local_get(5); i32_add; local_set(9);
          i32_const(0); local_set(6); // i = 0
          block_empty; loop_empty;
            local_get(6); local_get(3); i32_ge_u; br_if(1);
            local_get(0); local_get(6); call(emitter.rt.string.utf8_width); local_set(10); // width
            // copy one codepoint: result[out..] = s[i .. i+width]
            local_get(8); i32_const(dat); i32_add; local_get(9); i32_add;
            local_get(0); i32_const(dat); i32_add; local_get(6); i32_add;
            local_get(10); memory_copy;
            local_get(9); local_get(10); i32_add; local_set(9); // out += width
            local_get(6); local_get(10); i32_add; local_set(6); // i += width
            // `to` after this codepoint
            local_get(8); i32_const(dat); i32_add; local_get(9); i32_add;
            local_get(2); i32_const(dat); i32_add; local_get(5); memory_copy;
            local_get(9); local_get(5); i32_add; local_set(9);
            br(0);
          end; end;
          local_get(8); return_;
        end;
        // Non-empty `from`. Pass 1: count occurrences.
        i32_const(0); local_set(6); // i = 0
        i32_const(0); local_set(7); // cnt = 0
        block_empty; loop_empty;
          local_get(6); local_get(4); i32_add; local_get(3); i32_gt_u; br_if(1);
          local_get(0); i32_const(dat); i32_add; local_get(6); i32_add;
          local_get(1); i32_const(dat); i32_add;
          local_get(4); call(emitter.rt.mem_eq);
          if_empty;
            local_get(7); i32_const(1); i32_add; local_set(7); // cnt += 1
            local_get(6); local_get(4); i32_add; local_set(6); // i += fl
          else_;
            local_get(6); i32_const(1); i32_add; local_set(6); // i += 1
          end;
          br(0);
        end; end;
        // No occurrences → return s unchanged.
        // SHARE: this hands back the INPUT string, so it must own a +1 — the
        // caller drops the result as a fresh value AND drops `s` at scope end,
        // so an un-shared pass-through double-frees (#666/#668 class; the svg
        // `escape_attr` no-match pipe chain trapped __rc_dec on wasm).
        local_get(7); i32_eqz;
        if_empty; local_get(0); call(emitter.rt.rc_inc); return_; end;
        // result = string_alloc(blen + cnt * (tl - fl))
        local_get(3); local_get(7); local_get(5); local_get(4); i32_sub; i32_mul; i32_add;
        call(emitter.rt.string_alloc); local_set(8);
        // Pass 2: build result.
        i32_const(0); local_set(6); // i = 0 (read off into s)
        i32_const(0); local_set(9); // out = 0 (write off into result)
        block_empty; loop_empty;
          local_get(6); local_get(3); i32_ge_u; br_if(1);
          // match at i (only when a full `from` still fits)?
          local_get(6); local_get(4); i32_add; local_get(3); i32_le_u;
          if_empty;
            local_get(0); i32_const(dat); i32_add; local_get(6); i32_add;
            local_get(1); i32_const(dat); i32_add;
            local_get(4); call(emitter.rt.mem_eq);
            if_empty;
              // copy `to`, advance i by fl, out by tl
              local_get(8); i32_const(dat); i32_add; local_get(9); i32_add;
              local_get(2); i32_const(dat); i32_add; local_get(5); memory_copy;
              local_get(9); local_get(5); i32_add; local_set(9);
              local_get(6); local_get(4); i32_add; local_set(6);
              br(2); // continue outer loop
            end;
          end;
          // copy one byte verbatim
          local_get(8); i32_const(dat); i32_add; local_get(9); i32_add;
          local_get(0); i32_const(dat); i32_add; local_get(6); i32_add;
          i32_load8_u(0); i32_store8(0);
          local_get(9); i32_const(1); i32_add; local_set(9);
          local_get(6); i32_const(1); i32_add; local_set(6);
          br(0);
        end; end;
        local_get(8); end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.replace, type_idx, f));
}

/// Iterative split using a single forward byte-scan. Supports multi-char
/// delimiters. Mirrors the native oracle `s.split(sep)` (a non-empty `sep`
/// yields one segment per gap between matches, including a trailing empty
/// segment when `s` ends with `sep`). De-recursed (#634): the old recursion
/// was one frame per segment and exhausted the wasm call stack at ~4700
/// segments — same precedent as `compile_lines`'s byte-scan rewrite.
fn compile_split(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.split];
    // params: 0=s, 1=delim
    // locals: 2=d_len, 3=blen, 4=seg_start, 5=i(scan), 6=slot, 7=result
    //   empty-delim branch reuses: 8=in_off, 9=width
    let mut f = Function::new([
        (1, ValType::I32), (1, ValType::I32), (1, ValType::I32),
        (1, ValType::I32), (1, ValType::I32), (1, ValType::I32),
        (2, ValType::I32),
    ]);
    wasm!(f, {
        local_get(1); i32_load(0); local_set(2); // d_len
        local_get(0); i32_load(0); local_set(3); // blen
        // Empty delimiter: split per CODEPOINT with a leading + trailing empty
        // string — native `s.split("")` yields ["", c0, c1, …, ""] (and ["", ""]
        // for ""). Slots = char_count + 2.
        local_get(2); i32_eqz;
        if_empty;
          // result list: [len = char_count + 2][slot ptrs…]. Worst case (all
          // ASCII) char_count == blen, so blen + 2 slots is always enough.
          i32_const(list_hdr()); local_get(3); i32_const(2); i32_add; i32_const(4); i32_mul; i32_add;
          call(emitter.rt.alloc); local_set(7);
          // slot[0] = "" (leading empty)
          local_get(7); i32_const(list_data_off()); i32_add;
          i32_const(0); call(emitter.rt.string_alloc); i32_store(0);
          i32_const(0); local_set(8);                              // in_off = 0
          i32_const(1); local_set(6);                              // slot = 1 (after leading "")
          block_empty; loop_empty;
            local_get(8); local_get(3); i32_ge_u; br_if(1);
            local_get(0); local_get(8); call(emitter.rt.string.utf8_width); local_set(9); // width
            // slot[slot] = slice(s, in_off, in_off + width)  (one codepoint)
            local_get(7); i32_const(list_data_off()); i32_add; local_get(6); i32_const(4); i32_mul; i32_add;
            local_get(0); local_get(8); local_get(8); local_get(9); i32_add;
            call(emitter.rt.string.slice); i32_store(0);
            local_get(8); local_get(9); i32_add; local_set(8);     // in_off += width
            local_get(6); i32_const(1); i32_add; local_set(6);     // slot += 1
            br(0);
          end; end;
          // slot[slot] = "" (trailing empty)
          local_get(7); i32_const(list_data_off()); i32_add; local_get(6); i32_const(4); i32_mul; i32_add;
          i32_const(0); call(emitter.rt.string_alloc); i32_store(0);
          // result.len = slot + 1  (== char_count + 2)
          local_get(7); local_get(6); i32_const(1); i32_add; i32_store(0);
          local_get(7); return_;
        end;
        // Non-empty delimiter. A delimiter of length d_len>=1 can match at most
        // blen times, so blen + 1 segments is the upper bound on the slot count.
        i32_const(list_hdr()); local_get(3); i32_const(1); i32_add; i32_const(4); i32_mul; i32_add;
        call(emitter.rt.alloc); local_set(7);
        i32_const(0); local_set(4); // seg_start = 0
        i32_const(0); local_set(5); // i = 0 (scan position)
        i32_const(0); local_set(6); // slot = 0
        block_empty; loop_empty;
          // stop scanning once a full delimiter can no longer fit: i > blen - d_len.
          local_get(5); local_get(2); i32_add; local_get(3); i32_gt_u; br_if(1);
          // if mem_eq(s_data + i, delim_data, d_len)
          local_get(0); i32_const(string_data_off()); i32_add; local_get(5); i32_add;
          local_get(1); i32_const(string_data_off()); i32_add;
          local_get(2);
          call(emitter.rt.mem_eq);
          if_empty;
            // emit segment slice(s, seg_start, i)
            local_get(7); i32_const(list_data_off()); i32_add; local_get(6); i32_const(4); i32_mul; i32_add;
            local_get(0); local_get(4); local_get(5); call(emitter.rt.string.slice); i32_store(0);
            local_get(6); i32_const(1); i32_add; local_set(6);     // slot += 1
            local_get(5); local_get(2); i32_add; local_set(5);     // i += d_len
            local_get(5); local_set(4);                            // seg_start = i
          else_;
            local_get(5); i32_const(1); i32_add; local_set(5);     // i += 1
          end;
          br(0);
        end; end;
        // trailing segment slice(s, seg_start, blen)
        local_get(7); i32_const(list_data_off()); i32_add; local_get(6); i32_const(4); i32_mul; i32_add;
        local_get(0); local_get(4); local_get(3); call(emitter.rt.string.slice); i32_store(0);
        local_get(6); i32_const(1); i32_add; local_set(6);         // slot += 1
        local_get(7); local_get(6); i32_store(0);                  // result.len = slot
        local_get(7); end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.split, type_idx, f));
}

fn compile_join(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.join];
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I32), (1, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(2); // len
        local_get(2); i32_eqz;
        if_i32;
          // empty list → empty string
          i32_const(0); call(emitter.rt.string_alloc);
        else_;
          // result = list[0]
          local_get(0); i32_const(list_data_off()); i32_add; i32_load(0); local_set(4);
          // Singleton SHARE dup: for len==1 the loop never runs and the
          // ELEMENT POINTER itself is returned — an alias into a list the
          // caller still owns and will deep-Dec. Inc it (no-op for
          // data-section strings; len>=2 results are fresh via concat, an
          // unconditional inc would leak elem0 once per join).
          local_get(2); i32_const(1); i32_eq;
          if_empty;
            local_get(4); call(emitter.rt.rc_inc); drop;
          end;
          i32_const(1); local_set(3); // i=1
          block_empty; loop_empty;
            local_get(3); local_get(2); i32_ge_u; br_if(1);
            // result = concat(result, sep)
            local_get(4); local_get(1); call(emitter.rt.concat_str); local_set(4);
            // result = concat(result, list[i])
            local_get(4);
            local_get(0); i32_const(list_data_off()); i32_add;
            local_get(3); i32_const(4); i32_mul; i32_add; i32_load(0);
            call(emitter.rt.concat_str); local_set(4);
            local_get(3); i32_const(1); i32_add; local_set(3);
            br(0);
          end; end;
          local_get(4);
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.join, type_idx, f));
}

fn compile_count(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.count];
    let mut f = Function::new([
        (1, ValType::I64), (1, ValType::I64), (1, ValType::I32),
        (1, ValType::I32), (1, ValType::I32),
    ]);
    wasm!(f, {
        i64_const(0); local_set(2); // count
        i32_const(0); local_set(4); // pos
        local_get(1); i32_load(0); local_set(5); // sub_len
        local_get(5); i32_eqz;
        // Empty pattern: native `s.matches("").count()` == s.chars().count() + 1
        // (one empty match at every char boundary, including the end).
        if_i64; local_get(0); call(emitter.rt.string.char_count); i64_const(1); i64_add;
        else_;
          block_empty; loop_empty;
            local_get(0); local_get(4); local_get(0); i32_load(0);
            call(emitter.rt.string.slice); local_set(6);
            local_get(6); local_get(1); call(emitter.rt.string.index_of); local_set(3);
            local_get(3); i64_const(-1); i64_eq; br_if(1);
            local_get(2); i64_const(1); i64_add; local_set(2);
            local_get(4); local_get(3); i32_wrap_i64; i32_add;
            local_get(5); i32_add; local_set(4);
            br(0);
          end; end;
          local_get(2);
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.count, type_idx, f));
}

// ── Padding / trimming ──

/// Build a 1-codepoint String holding the FIRST codepoint of `pad`. Empty
/// `pad` degenerates to a width-0 string (native uses `' '`, but pad is never
/// empty in practice for the padding ops; an empty pad simply pads with
/// nothing on both targets — kept consistent here).
fn emit_pad_first_cp(emitter: &mut WasmEmitter, f: &mut Function, pad_local: u32, out_local: u32) {
    // width of first codepoint (0 if pad empty)
    wasm!(*f, {
        local_get(pad_local); i32_load(0); i32_eqz;
        if_i32;
          i32_const(0); call(emitter.rt.string_alloc);
        else_;
          // unit = slice(pad, 0, width(pad, 0))
          local_get(pad_local); i32_const(0);
          local_get(pad_local); i32_const(0); call(emitter.rt.string.utf8_width);
          call(emitter.rt.string.slice);
        end;
        local_set(out_local);
    });
}

/// pad_start(s, width, pad): width measured in CODEPOINTS; pad unit = first
/// codepoint of `pad`, repeated (width - char_count(s)) times, prepended.
fn compile_pad_start(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.pad_start];
    // params: 0=s, 1=width, 2=pad | locals: 3=count, 4=n, 5=unit, 6=fill
    let mut f = Function::new([(4, ValType::I32)]);
    wasm!(f, {
        local_get(0); call(emitter.rt.string.char_count); i32_wrap_i64; local_set(3);
        local_get(3); local_get(1); i32_ge_u;
        // SHARE: width satisfied → returns the INPUT string; own a +1 or the
        // caller's result-drop + input-drop double-free it (#668 class).
        if_i32; local_get(0); call(emitter.rt.rc_inc);
        else_;
          local_get(1); local_get(3); i32_sub; local_set(4);    // n = width - count
    });
    emit_pad_first_cp(emitter, &mut f, 2, 5);                    // unit (local 5)
    wasm!(f, {
          local_get(5); local_get(4); call(emitter.rt.string.repeat); local_set(6);
          local_get(6); local_get(0); call(emitter.rt.concat_str);
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.pad_start, type_idx, f));
}

/// pad_end(s, width, pad): like pad_start but appended.
fn compile_pad_end(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.pad_end];
    let mut f = Function::new([(4, ValType::I32)]);
    wasm!(f, {
        local_get(0); call(emitter.rt.string.char_count); i32_wrap_i64; local_set(3);
        local_get(3); local_get(1); i32_ge_u;
        // SHARE: width satisfied → returns the INPUT string; own a +1 (#668 class).
        if_i32; local_get(0); call(emitter.rt.rc_inc);
        else_;
          local_get(1); local_get(3); i32_sub; local_set(4);
    });
    emit_pad_first_cp(emitter, &mut f, 2, 5);
    wasm!(f, {
          local_get(5); local_get(4); call(emitter.rt.string.repeat); local_set(6);
          local_get(0); local_get(6); call(emitter.rt.concat_str);
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.pad_end, type_idx, f));
}

fn compile_trim_start(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.trim_start];
    const S: u32 = 0; // param
    const LEN: u32 = 1;
    const START: u32 = 2;
    let mut f = Function::new([(2, ValType::I32)]);
    wasm!(f, {
        local_get(S); i32_load(0); local_set(LEN);
        i32_const(0); local_set(START);
    });
    emit_trim_forward(&mut f, emitter, START, LEN);
    wasm!(f, {
        local_get(S); local_get(START); local_get(LEN);
        call(emitter.rt.string.slice);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.trim_start, type_idx, f));
}

fn compile_trim_end(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.trim_end];
    const S: u32 = 0; // param
    const END: u32 = 1;
    const Q: u32 = 2; // scratch for the backward walk
    const FLOOR: u32 = 3; // 0 — never trim below the start
    let mut f = Function::new([(3, ValType::I32)]);
    wasm!(f, {
        local_get(S); i32_load(0); local_set(END);
        i32_const(0); local_set(FLOOR);
    });
    emit_trim_backward(&mut f, emitter, END, FLOOR, Q);
    wasm!(f, {
        local_get(S); i32_const(0); local_get(END);
        call(emitter.rt.string.slice);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.trim_end, type_idx, f));
}

// ── Case transform ──
//
// Full-Unicode, byte-identical to native `str::to_uppercase()`/`to_lowercase()`.
// `to_upper`/`to_lower` are thin wrappers over the unified `__str_case_map`
// driver; the real work (oracle-derived table lookup, Final_Sigma scan, two-pass
// exact-size allocation) lives in the case-folding functions at the end of this
// file. The old ASCII-only ±32 byte loop (`compile_case_transform`) is gone.

fn compile_to_upper(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.to_upper];
    let map = emitter.rt.string.str_case_map;
    let mut f = Function::new([]);
    wasm!(f, { local_get(0); i32_const(1); call(map); end; });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.to_upper, type_idx, f));
}

fn compile_to_lower(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.to_lower];
    let map = emitter.rt.string.str_case_map;
    let mut f = Function::new([]);
    wasm!(f, { local_get(0); i32_const(0); call(map); end; });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.to_lower, type_idx, f));
}

// ── Decompose ──

/// chars(s): one element per CODEPOINT, each a String holding that codepoint's
/// 1-4 UTF-8 bytes. The list length is the codepoint count (worst case = byte
/// length, so we size the list buffer by byte length and only fill `j` slots).
fn compile_chars(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.chars];
    // params: 0=s | locals: 1=blen, 2=result, 3=in_off, 4=str, 5=width, 6=j, 7=k
    let mut f = Function::new([(7, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(1);                 // blen
        // worst-case slots = blen (all-ASCII); fewer codepoints just leave gaps
        i32_const(list_hdr()); local_get(1); i32_const(4); i32_mul; i32_add;
        call(emitter.rt.alloc); local_set(2);
        i32_const(0); local_set(3);                              // in_off = 0
        i32_const(0); local_set(6);                              // j = 0 (codepoint index)
        block_empty; loop_empty;
          local_get(3); local_get(1); i32_ge_u; br_if(1);
          local_get(0); local_get(3); call(emitter.rt.string.utf8_width); local_set(5);
          // str = alloc(width); copy width bytes
          local_get(5); call(emitter.rt.string_alloc); local_set(4);
          i32_const(0); local_set(7);
          block_empty; loop_empty;
            local_get(7); local_get(5); i32_ge_u; br_if(1);
            local_get(4); i32_const(string_data_off()); i32_add; local_get(7); i32_add;
            local_get(0); i32_const(string_data_off()); i32_add; local_get(3); i32_add; local_get(7); i32_add;
            i32_load8_u(0); i32_store8(0);
            local_get(7); i32_const(1); i32_add; local_set(7);
            br(0);
          end; end;
          // result.data[j] = str
          local_get(2); i32_const(list_data_off()); i32_add; local_get(6); i32_const(4); i32_mul; i32_add;
          local_get(4); i32_store(0);
          local_get(3); local_get(5); i32_add; local_set(3);     // in_off += width
          local_get(6); i32_const(1); i32_add; local_set(6);     // j += 1
          br(0);
        end; end;
        local_get(2); local_get(6); i32_store(0);                // result.len = j
        local_get(2); end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.chars, type_idx, f));
}

/// run_length_encode(s) -> List[(String, Int)].
/// Two passes over the byte payload: first count maximal runs of equal CODEPOINTS
/// to size the list exactly, then build a String (the whole codepoint, not one
/// byte) + i64 count tuple per run. Each list slot holds a pointer to a 12-byte
/// tuple `[str_ptr:i32 @0][cnt:i64 @4]` (tuple fields are laid out sequentially
/// with no padding — see values::byte_size). Codepoint-granular to match native
/// `s.chars()` grouping — multibyte `ﬀ`/`İ` now agree (Cluster-2 finding #6);
/// runs are compared by Unicode scalar (`utf8_scalar`) and advanced by
/// `utf8_width`.
fn compile_run_length_encode(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.run_length_encode];
    // locals: 1=blen 2=nr 3=i(byte off) 4=cur(i64 scalar) 5=result 6=j 7=cnt
    //         8=strp 9=tup 10=run_start(byte off) 11=width(i32)
    let mut f = Function::new([
        (3, ValType::I32),  // 1=blen 2=nr 3=i
        (1, ValType::I64),  // 4=cur (Unicode scalar)
        (5, ValType::I32),  // 5=result 6=j 7=cnt 8=strp 9=tup
        (2, ValType::I32),  // 10=run_start 11=width
    ]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(1);                 // blen = *s
        // ── Pass 1: count maximal runs (by codepoint scalar) into nr ──
        i32_const(0); local_set(2);                              // nr = 0
        i32_const(0); local_set(3);                              // i = 0 (byte offset)
        block_empty; loop_empty;
          local_get(3); local_get(1); i32_ge_u; br_if(1);
          local_get(0); local_get(3); call(emitter.rt.string.utf8_scalar); local_set(4); // cur scalar
          local_get(2); i32_const(1); i32_add; local_set(2);     // nr += 1
          local_get(0); local_get(3); call(emitter.rt.string.utf8_width); local_get(3); i32_add; local_set(3); // i += width
          block_empty; loop_empty;                               // skip equal codepoints
            local_get(3); local_get(1); i32_ge_u; br_if(1);
            local_get(0); local_get(3); call(emitter.rt.string.utf8_scalar);
            local_get(4); i64_ne; br_if(1);
            local_get(0); local_get(3); call(emitter.rt.string.utf8_width); local_get(3); i32_add; local_set(3);
            br(0);
          end; end;
          br(0);
        end; end;
        // ── Allocate the result list: [len=nr][nr * ptr] ──
        i32_const(list_hdr()); local_get(2); i32_const(4); i32_mul; i32_add;
        call(emitter.rt.alloc); local_set(5);
        local_get(5); local_get(2); i32_store(0);
        // ── Pass 2: emit one (codepoint-string, count) tuple per run ──
        i32_const(0); local_set(3);                              // i = 0
        i32_const(0); local_set(6);                              // j = 0
        block_empty; loop_empty;
          local_get(3); local_get(1); i32_ge_u; br_if(1);
          local_get(3); local_set(10);                           // run_start = i
          local_get(0); local_get(3); call(emitter.rt.string.utf8_scalar); local_set(4); // cur scalar
          local_get(0); local_get(3); call(emitter.rt.string.utf8_width); local_set(11); // width
          i32_const(1); local_set(7);                            // cnt = 1
          local_get(3); local_get(11); i32_add; local_set(3);    // i += width
          block_empty; loop_empty;
            local_get(3); local_get(1); i32_ge_u; br_if(1);
            local_get(0); local_get(3); call(emitter.rt.string.utf8_scalar);
            local_get(4); i64_ne; br_if(1);
            local_get(7); i32_const(1); i32_add; local_set(7);   // cnt += 1
            local_get(0); local_get(3); call(emitter.rt.string.utf8_width); local_get(3); i32_add; local_set(3); // i += width
            br(0);
          end; end;
          // strp = slice(s, run_start, run_start + width): the whole codepoint
          local_get(0); local_get(10); local_get(10); local_get(11); i32_add;
          call(emitter.rt.string.slice); local_set(8);
          // tup = [strp @0][cnt:i64 @4]
          i32_const(12); call(emitter.rt.alloc); local_set(9);
          local_get(9); local_get(8); i32_store(0);
          local_get(9); local_get(7); i64_extend_i32_u; i64_store(4);
          // result.data[j] = tup
          local_get(5); i32_const(list_data_off()); i32_add; local_get(6); i32_const(4); i32_mul; i32_add;
          local_get(9); i32_store(0);
          local_get(6); i32_const(1); i32_add; local_set(6);     // j += 1
          br(0);
        end; end;
        local_get(5); end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.run_length_encode, type_idx, f));
}

fn compile_lines(emitter: &mut WasmEmitter) {
    // #601: true `str::lines()` — NOT split-on-\n. Two semantics split must
    // not have: (1) a final line terminator does NOT yield a trailing empty
    // line ("a\nb\n" -> [a, b], not [a, b, ""]); (2) a "\r\n" line drops the
    // trailing "\r". Byte-scan loop mirroring the native oracle
    // `runtime/rs/src/string.rs::almide_rt_string_lines = s.lines()`.
    let type_idx = emitter.func_type_indices[&emitter.rt.string.lines];
    // locals: 1=blen 2=result 3=cur 4=i 5=slot 6=line_end
    let mut f = Function::new([(6, ValType::I32)]);
    let dat = string_data_off() as i32;
    wasm!(f, {
        local_get(0); i32_load(0); local_set(1); // blen
        // (empty input falls through naturally: the loop body never runs, the
        // trailing-line guard is false, and result.len stays 0 -> empty list.)
        // result: header + (blen + 1) slots (upper bound on the line count).
        i32_const(list_hdr()); local_get(1); i32_const(1); i32_add; i32_const(4); i32_mul; i32_add;
        call(emitter.rt.alloc); local_set(2);
        i32_const(0); local_set(3); // cur
        i32_const(0); local_set(4); // i
        i32_const(0); local_set(5); // slot
        block_empty; loop_empty;
          local_get(4); local_get(1); i32_ge_u; br_if(1); // i >= blen -> done scanning
          // if byte[i] == '\n'
          local_get(0); i32_const(dat); i32_add; local_get(4); i32_add; i32_load8_u(0);
          i32_const(10); i32_eq;
          if_empty;
            // line_end = i; strip a trailing '\r' (byte[i-1] == 13 when i > cur)
            local_get(4); local_set(6);
            local_get(4); local_get(3); i32_gt_u;
            if_empty;
              local_get(0); i32_const(dat); i32_add; local_get(4); i32_const(1); i32_sub; i32_add; i32_load8_u(0);
              i32_const(13); i32_eq;
              if_empty;
                local_get(4); i32_const(1); i32_sub; local_set(6);
              end;
            end;
            // slot[slot] = slice(s, cur, line_end)
            local_get(2); i32_const(list_data_off()); i32_add; local_get(5); i32_const(4); i32_mul; i32_add;
            local_get(0); local_get(3); local_get(6); call(emitter.rt.string.slice);
            i32_store(0);
            local_get(5); i32_const(1); i32_add; local_set(5); // slot++
            local_get(4); i32_const(1); i32_add; local_set(3); // cur = i + 1
          end;
          local_get(4); i32_const(1); i32_add; local_set(4); // i++
          br(0);
        end; end;
        // trailing non-empty line (input did NOT end at a '\n')
        local_get(3); local_get(1); i32_lt_u;
        if_empty;
          local_get(2); i32_const(list_data_off()); i32_add; local_get(5); i32_const(4); i32_mul; i32_add;
          local_get(0); local_get(3); local_get(1); call(emitter.rt.string.slice);
          i32_store(0);
          local_get(5); i32_const(1); i32_add; local_set(5);
        end;
        local_get(2); local_get(5); i32_store(0); // result.len = slot
        local_get(2);
        end; // close the function body
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.lines, type_idx, f));
}
