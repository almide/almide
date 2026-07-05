/// __concat_str(left: i32, right: i32) -> i32
/// Concatenates two strings. Each is [len:i32][data:u8...].
fn compile_concat_str(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.concat_str];
    // params: 0=$left, 1=$right
    // locals: 2=$left_len, 3=$right_len, 4=$new_len, 5=$result, 6=$i
    let mut f = Function::new([
        (1, ValType::I32), // 2: $left_len
        (1, ValType::I32), // 3: $right_len
        (1, ValType::I32), // 4: $new_len
        (1, ValType::I32), // 5: $result
        (1, ValType::I32), // 6: $i
    ]);

    wasm!(f, {
        local_get(0);
        i32_load(0);        // left.len
        local_set(2);
        local_get(1);
        i32_load(0);        // right.len
        local_set(3);
        local_get(2);
        local_get(3);
        i32_add;
        local_set(4);       // new_len = left_len + right_len
        local_get(4);
        i32_const(string_hdr());
        i32_add;
        call(emitter.rt.alloc);
        local_set(5);
        local_get(5);
        local_get(4);
        i32_store(0);       // result.len = new_len
        local_get(5);
        local_get(4);
        i32_store(string_cap_off() as u32); // result.cap = new_len
    });

    // Copy left data: dst=result+DATA_OFFSET, src=left+DATA_OFFSET
    emit_memcpy_loop(&mut f, 5, 0, 2, 6,
        string_data_off() as u32, string_data_off() as u32);

    // Copy right data: dst=result+DATA_OFFSET+left_len, src=right+DATA_OFFSET
    wasm!(f, {
        i32_const(0);
        local_set(6);
        block_empty;
        loop_empty;
        local_get(6);
        local_get(3);
        i32_ge_u;
        br_if(1);
        local_get(5);
        i32_const(string_data_off());
        i32_add;
        local_get(2);
        i32_add;
        local_get(6);
        i32_add;
        local_get(1);
        i32_const(string_data_off());
        i32_add;
        local_get(6);
        i32_add;
        i32_load8_u(0);
        i32_store8(0);
        local_get(6);
        i32_const(1);
        i32_add;
        local_set(6);
        br(0);
        end;
        end;
    });

    wasm!(f, { local_get(5); end; });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.concat_str, type_idx, f));
}

/// __string_alloc(data_len: i32) -> ptr
/// Allocate a string buffer with header properly initialized:
///   ptr[0] = data_len (len field)
///   ptr[cap_off] = data_len (cap field)
/// Returns pointer to the string header.
/// This eliminates the entire class of "cap not written" bugs.
fn compile_string_alloc(emitter: &mut WasmEmitter) {
    use super::engine::{WasmBuilder, layout::*};
    let type_idx = emitter.func_type_indices[&emitter.rt.string_alloc];
    let hdr = emitter.layout_reg.header_size(STRING) as i32;
    let cap_off = emitter.layout_reg.fixed_offset(STRING, string::CAP);
    let cap_ty = emitter.layout_reg.field(STRING, string::CAP).ty;
    let alloc_fn = emitter.rt.alloc;

    // param 0 = data_len, local 1 = ptr
    let mut f = Function::new([(1, ValType::I32)]);
    {
        let w = &mut WasmBuilder::new(&mut f, &emitter.layout_reg);
        // ptr = alloc(hdr + data_len)
        w.get(0).i32c(hdr).add().call(alloc_fn).set(1);
        // ptr.len = data_len
        w.get(1).get(0).emit_store(0, MemType::I32);
        // ptr.cap = data_len
        w.get(1).get(0).emit_store(cap_off, cap_ty);
        // return ptr
        w.get(1);
    }
    f.instruction(&wasm_encoder::Instruction::End);
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string_alloc, type_idx, f));
}

/// __div_trap(msg_ptr: i32)
/// Integer div/mod abort: write the interned message string at `msg_ptr` — already
/// the full `Error: <msg>\n` text, [len:i32][cap:i32][data@DATA] layout — to stderr
/// via WASI fd_write, then `proc_exit(1)`. The shared trap keeps the div-by-zero and
/// signed-overflow paths a single function call at every emit site.
fn compile_div_trap(emitter: &mut WasmEmitter) {
    // WASI fd for stderr (matches native `eprintln!` → fd 2).
    const STDERR_FD: i32 = 2;
    // Exit code on an aborting integer op (matches native `std::process::exit(1)`).
    const ABORT_EXIT_CODE: i32 = 1;
    // Scratch layout for the single fd_write iovec: [buf:i32@0][len:i32@4], with the
    // returned byte count written at [8]. Mirrors `compile_println_str`.
    const IOV_BUF_OFF: i32 = 0;
    const IOV_LEN_OFF: i32 = 4;
    const NWRITTEN_OFF: i32 = 8;
    const IOV_BASE: i32 = 0;
    const IOV_COUNT: i32 = 1;

    let type_idx = emitter.func_type_indices[&emitter.rt.div_trap];
    let data_off = string_data_off();
    let fd_write = emitter.rt.fd_write;
    let proc_exit = emitter.rt.proc_exit;

    // param 0 = msg_ptr (interned `Error: <msg>\n` string)
    let mut f = Function::new([]);
    // iov[0].buf = msg_ptr + DATA  (skip the len+cap header)
    wasm!(f, {
        i32_const(IOV_BUF_OFF);
        local_get(0);
        i32_const(data_off);
        i32_add;
        i32_store(0);
    });
    // iov[0].len = *msg_ptr  (the byte length, which already includes the newline)
    wasm!(f, {
        i32_const(IOV_LEN_OFF);
        local_get(0);
        i32_load(0);
        i32_store(0);
    });
    // fd_write(stderr, iovs=IOV_BASE, iovs_len=IOV_COUNT, nwritten=NWRITTEN_OFF)
    wasm!(f, {
        i32_const(STDERR_FD);
        i32_const(IOV_BASE);
        i32_const(IOV_COUNT);
        i32_const(NWRITTEN_OFF);
        call(fd_write);
        drop;
    });
    // proc_exit(1) — diverges; never returns.
    wasm!(f, {
        i32_const(ABORT_EXIT_CODE);
        call(proc_exit);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.div_trap, type_idx, f));
}

/// Capacity-aware string append: if left has room, append in-place; else grow 2x.
fn compile_string_append(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string_append];
    // params: 0=$left, 1=$right
    // locals: 2=$left_len, 3=$right_len, 4=$new_len, 5=$left_cap, 6=$result, 7=$i
    let mut f = Function::new([
        (1, ValType::I32), // 2: $left_len
        (1, ValType::I32), // 3: $right_len
        (1, ValType::I32), // 4: $new_len
        (1, ValType::I32), // 5: $left_cap
        (1, ValType::I32), // 6: $result
        (1, ValType::I32), // 7: $i (counter)
    ]);

    wasm!(f, {
        local_get(0); i32_load(0); local_set(2);               // left_len
        local_get(1); i32_load(0); local_set(3);               // right_len
        local_get(2); local_get(3); i32_add; local_set(4);     // new_len
        local_get(0); i32_load(string_cap_off() as u32); local_set(5); // left_cap

        // if left_cap >= new_len: append in-place
        local_get(5); local_get(4); i32_ge_u;
        if_i32;
          // In-place: memory_copy right data after left data
          local_get(0); i32_const(string_data_off()); i32_add; local_get(2); i32_add;
          local_get(1); i32_const(string_data_off()); i32_add;
          local_get(3);
          memory_copy;
          // Update left.len
          local_get(0); local_get(4); i32_store(0);
          local_get(0);  // return left (same pointer)
        else_;
          // Grow: alloc new buffer with cap = max(left_cap*2, new_len)
          local_get(5); i32_const(2); i32_mul; local_set(5); // cap *= 2
          local_get(5); local_get(4); i32_lt_u;
          if_empty; local_get(4); local_set(5); end;          // cap = max(cap*2, new_len)
          // Alloc
          local_get(5); i32_const(string_data_off()); i32_add;
          call(emitter.rt.alloc); local_set(6);
          local_get(6); local_get(4); i32_store(0);           // result.len = new_len
          local_get(6); local_get(5); i32_store(string_cap_off() as u32); // result.cap
          // Copy left data
          local_get(6); i32_const(string_data_off()); i32_add;
          local_get(0); i32_const(string_data_off()); i32_add;
          local_get(2);
          memory_copy;
          // Copy right data
          local_get(6); i32_const(string_data_off()); i32_add; local_get(2); i32_add;
          local_get(1); i32_const(string_data_off()); i32_add;
          local_get(3);
          memory_copy;
          local_get(6);  // return new pointer
        end;
    });
    wasm!(f, { end; });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string_append, type_idx, f));
}

/// Emit a byte-by-byte copy loop: dst[dst_off+i] = src[src_off+i], 0..len
/// Uses local `counter` as loop variable.
pub(super) fn emit_memcpy_loop(f: &mut Function, dst: u32, src: u32, len: u32, counter: u32, dst_off: u32, src_off: u32) {
    wasm!(f, {
        i32_const(0);
        local_set(counter);
        block_empty;
        loop_empty;
        local_get(counter);
        local_get(len);
        i32_ge_u;
        br_if(1);
        local_get(dst);
        i32_const(dst_off as i32);
        i32_add;
        local_get(counter);
        i32_add;
        local_get(src);
        i32_const(src_off as i32);
        i32_add;
        local_get(counter);
        i32_add;
        i32_load8_u(0);
        i32_store8(0);
        local_get(counter);
        i32_const(1);
        i32_add;
        local_set(counter);
        br(0);
        end;
        end;
    });
}

// String builder scratch is gone: `emit_string_interp` now builds the result
// inline (see `calls_string::emit_string_interp`). No runtime helpers and no
// reserved memory region — each interpolation does one heap bump for the
// result and a handful of `memory.copy`s.

/// __init_preopen_dirs() → ()
/// Discovers preopened directories via fd_prestat_get/fd_prestat_dir_name.
/// Builds a heap table: [fd:i32, path_ptr:i32, path_len:i32] per entry.
/// Sets globals: preopen_table (ptr), preopen_count (count).
fn compile_init_preopen_dirs(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.init_preopen_dirs];
    // locals: 0=$fd, 1=$buf(8 bytes for prestat), 2=$errno, 3=$path_len,
    //         4=$count, 5=$table_ptr, 6=$path_buf
    let mut f = Function::new([
        (1, ValType::I32), // 0: $fd
        (1, ValType::I32), // 1: $buf (prestat result: [tag:u8, padding:3, name_len:u32] = 8 bytes)
        (1, ValType::I32), // 2: $errno
        (1, ValType::I32), // 3: $path_len
        (1, ValType::I32), // 4: $count
        (1, ValType::I32), // 5: $table_ptr
        (1, ValType::I32), // 6: $path_buf
    ]);

    wasm!(f, {
        // Allocate prestat buf (8 bytes) and table (max 16 entries × 12 bytes = 192)
        i32_const(8); call(emitter.rt.alloc_pinned); local_set(1);
        i32_const(192); call(emitter.rt.alloc_pinned); local_set(5);

        // Start from fd=3 (first possible preopened dir)
        i32_const(3); local_set(0);
        i32_const(0); local_set(4);

        // Loop: try fd_prestat_get for each fd until it fails
        block_empty; loop_empty;
        // fd_prestat_get(fd, buf) -> errno
        local_get(0); local_get(1);
        call(emitter.rt.fd_prestat_get);
        local_set(2);

        // If errno != 0, we're done (EBADF = no more preopened dirs)
        local_get(2); i32_const(0); i32_ne;
        br_if(1);

        // Read path_len from prestat buf: offset 4 (after tag byte + padding)
        local_get(1); i32_load(4); local_set(3);

        // Allocate path buffer and get dir name
        local_get(3); i32_const(1); i32_add; call(emitter.rt.alloc_pinned); local_set(6);
        local_get(0); local_get(6); local_get(3);
        call(emitter.rt.fd_prestat_dir_name);
        drop;

        // Store entry in table: [fd, path_ptr, path_len]
        local_get(5); local_get(4); i32_const(12); i32_mul; i32_add;
        local_get(0); i32_store(0);
        local_get(5); local_get(4); i32_const(12); i32_mul; i32_add;
        local_get(6); i32_store(4);
        local_get(5); local_get(4); i32_const(12); i32_mul; i32_add;
        local_get(3); i32_store(8);

        // count++, fd++
        local_get(4); i32_const(1); i32_add; local_set(4);
        local_get(0); i32_const(1); i32_add; local_set(0);

        // Max 16 entries
        local_get(4); i32_const(16); i32_ge_u; br_if(1);
        br(0);
        end; end;

        // Set globals
        local_get(5); global_set(emitter.preopen_table_global);
        local_get(4); global_set(emitter.preopen_count_global);

        end;
    });

    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.init_preopen_dirs, type_idx, f));
}

/// __resolve_path(path_ptr: i32, path_len: i32) → i32 (result_ptr)
/// Result: [fd:i32, rel_path_ptr:i32, rel_path_len:i32] on heap.
/// Finds longest matching preopened dir prefix. Falls back to fd=3 with stripped leading '/'.
fn compile_resolve_path(emitter: &mut WasmEmitter) {
    // Intern "." so we can use its data pointer for exact-match paths
    let dot_str = emitter.intern_string(".");
    let dot_ptr = dot_str + 4; // skip the 4-byte length prefix to get raw '.' byte
    let type_idx = emitter.func_type_indices[&emitter.rt.resolve_path];
    // params: 0=$path_ptr, 1=$path_len
    // locals: 2=$result, 3=$i, 4=$best_fd, 5=$best_match_len,
    //         6=$entry_ptr, 7=$entry_fd, 8=$entry_path_ptr, 9=$entry_path_len,
    //         10=$j, 11=$match
    let mut f = Function::new([
        (1, ValType::I32), // 2: $result
        (1, ValType::I32), // 3: $i
        (1, ValType::I32), // 4: $best_fd
        (1, ValType::I32), // 5: $best_match_len
        (1, ValType::I32), // 6: $entry_ptr
        (1, ValType::I32), // 7: $entry_fd
        (1, ValType::I32), // 8: $entry_path_ptr
        (1, ValType::I32), // 9: $entry_path_len
        (1, ValType::I32), // 10: $j
        (1, ValType::I32), // 11: $match
    ]);

    wasm!(f, {
        // Allocate result: [fd, rel_path_ptr, rel_path_len]
        i32_const(12); call(emitter.rt.alloc_pinned); local_set(2);

        // Default: fd=3, no prefix match
        i32_const(3); local_set(4);
        i32_const(0); local_set(5);

        // Loop over preopened dirs to find longest prefix match
        i32_const(0); local_set(3);
        block_empty; loop_empty;
        local_get(3); global_get(emitter.preopen_count_global); i32_ge_u; br_if(1);

        // Load entry [fd, path_ptr, path_len]
        global_get(emitter.preopen_table_global);
        local_get(3); i32_const(12); i32_mul; i32_add;
        local_set(6);
        local_get(6); i32_load(0); local_set(7);
        local_get(6); i32_load(4); local_set(8);
        local_get(6); i32_load(8); local_set(9);

        // Skip if entry_path_len > path_len or entry_path_len <= best_match_len
        local_get(9); local_get(1); i32_gt_u;
        local_get(9); local_get(5); i32_le_u;
        i32_or;
        if_empty;
        else_;

        // Check prefix match: compare entry path bytes with input path bytes
        i32_const(1); local_set(11);
        i32_const(0); local_set(10);
        block_empty; loop_empty;
        local_get(10); local_get(9); i32_ge_u; br_if(1);
        local_get(0); local_get(10); i32_add; i32_load8_u(0);
        local_get(8); local_get(10); i32_add; i32_load8_u(0);
        i32_ne;
        if_empty;
          i32_const(0); local_set(11);
          br(2);
        end;
        local_get(10); i32_const(1); i32_add; local_set(10);
        br(0);
        end; end;

        // If matched, update best
        local_get(11);
        if_empty;
          local_get(7); local_set(4);
          local_get(9); local_set(5);
        end;

        end;

        local_get(3); i32_const(1); i32_add; local_set(3);
        br(0);
        end; end;

        // Build result
        local_get(5); i32_const(0); i32_gt_u;
        if_empty;
          // Prefix match found: strip prefix + optional '/' separator
          local_get(2); local_get(4); i32_store(0);
          local_get(1); local_get(5); i32_sub; i32_const(0); i32_gt_u;
          if_empty;
            local_get(0); local_get(5); i32_add; i32_load8_u(0);
            i32_const(47); i32_eq;
            if_empty;
              local_get(2); local_get(0); local_get(5); i32_add; i32_const(1); i32_add; i32_store(4);
              local_get(2); local_get(1); local_get(5); i32_sub; i32_const(1); i32_sub; i32_store(8);
            else_;
              local_get(2); local_get(0); local_get(5); i32_add; i32_store(4);
              local_get(2); local_get(1); local_get(5); i32_sub; i32_store(8);
            end;
          else_;
            // Exact match (e.g., path="/tmp", preopen="/tmp"): use "." as relative path
            local_get(2); i32_const(dot_ptr as i32); i32_store(4);
            local_get(2); i32_const(1); i32_store(8);
          end;
        else_;
          // No prefix match. For relative paths, find "." preopened dir. For absolute, strip '/'.
          local_get(0); i32_load8_u(0); i32_const(47); i32_eq;
          if_empty;
            // Absolute path with no match: strip '/' and use fd=3
            local_get(2); i32_const(3); i32_store(0);
            local_get(2); local_get(0); i32_const(1); i32_add; i32_store(4);
            local_get(2); local_get(1); i32_const(1); i32_sub; i32_store(8);
          else_;
            // Relative path: find "." in preopened dirs, fallback to fd=3
            local_get(2); i32_const(3); i32_store(0); // default fd=3
            i32_const(0); local_set(3);
            block_empty; loop_empty;
            local_get(3); global_get(emitter.preopen_count_global); i32_ge_u; br_if(1);
            global_get(emitter.preopen_table_global);
            local_get(3); i32_const(12); i32_mul; i32_add;
            local_set(6);
            // Check if entry path is "." (len==1 && byte[0]=='.')
            local_get(6); i32_load(8); i32_const(1); i32_eq;
            if_empty;
              local_get(6); i32_load(4); i32_load8_u(0); i32_const(46); i32_eq;
              if_empty;
                local_get(2); local_get(6); i32_load(0); i32_store(0); // use this fd
                br(3); // break out of search loop
              end;
            end;
            local_get(3); i32_const(1); i32_add; local_set(3);
            br(0);
            end; end;
            // Pass relative path as-is
            local_get(2); local_get(0); i32_store(4);
            local_get(2); local_get(1); i32_store(8);
          end;
        end;

        local_get(2);
        end;
    });

    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.resolve_path, type_idx, f));
}


/// __bytes_f16_to_f64(bits: i32) -> f64
///
/// IEEE-754 half-precision expansion. Computes:
///   sign = (bits >> 15) & 1
///   exp  = (bits >> 10) & 0x1f
///   mant = bits & 0x3ff
///   if exp == 0:  sign * mant * 2^-24           (subnormal / zero)
///   if exp == 31: sign * inf  (mant==0) or NaN  (mant!=0)
///   else:         sign * (1 + mant/1024) * 2^(exp-15)
///
/// Implemented with plain WASM math ops — no external float-pow call needed
/// because we can build 2^n with integer shifts into f64 exponent bits.
pub(super) fn compile_bytes_f16_to_f64(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.bytes_f16_to_f64];
    let mut f = Function::new(vec![
        (4, ValType::I32), // locals 1..=4 i32: sign, exp, mant, tmp
        (2, ValType::F64), // locals 5..=6 f64: sign_f, result
    ]);
    wasm!(f, {
        // sign = bits >> 15
        local_get(0); i32_const(15); i32_shr_u; local_set(1);
        // exp = (bits >> 10) & 0x1f
        local_get(0); i32_const(10); i32_shr_u; i32_const(31); i32_and; local_set(2);
        // mant = bits & 0x3ff
        local_get(0); i32_const(1023); i32_and; local_set(3);
        // sign_f = sign ? -1.0 : 1.0
        local_get(1);
        if_f64; f64_const(-1.0);
        else_; f64_const(1.0); end;
        local_set(5);

        // Branch on exp
        local_get(2); i32_eqz;
        if_f64;
            // subnormal: sign_f * mant * 2^-24
            local_get(5);
            local_get(3); f64_convert_i32_u;
            f64_mul;
            f64_const(5.960464477539063e-8); // 2^-24
            f64_mul;
        else_;
            local_get(2); i32_const(31); i32_eq;
            if_f64;
                // exp all-ones: mant==0 → ±inf (sign-preserving), mant!=0 → NaN.
                // Mirrors native f16_bits_to_f64 (runtime/rs/src/bytes.rs): the
                // previous `sign * f32::MAX` was finite and diverged.
                local_get(3); i32_eqz;
                if_f64;
                    local_get(5); f64_const(f64::INFINITY); f64_mul; // ±inf
                else_;
                    f64_const(f64::NAN);
                end;
            else_;
                // normal: sign_f * (1 + mant/1024) * 2^(exp-15)
                // 2^(exp-15) computed as f64 bit pattern:
                //   f64 exponent bias = 1023, so exp_f64 = exp - 15 + 1023 = exp + 1008
                //   bits = (exp_f64) << 52
                local_get(5);
                f64_const(1.0);
                local_get(3); f64_convert_i32_u;
                f64_const(1024.0); f64_div;
                f64_add;
                f64_mul;
                // Multiply by 2^(exp - 15): construct that power via i64 bit tricks.
                local_get(2); i32_const(1008); i32_add; i64_extend_i32_u;
                i64_const(52); i64_shl;
                f64_reinterpret_i64;
                f64_mul;
            end;
        end;
        end;  // close function body
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.bytes_f16_to_f64, type_idx, f));
}
