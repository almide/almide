/// __json_parse(s: i32) -> i32 (Result[Value, String])
fn compile_json_parse(emitter: &mut WasmEmitter) {
    let parse_at_fn = emitter.rt.json_parse_at;

    let type_idx = emitter.func_type_indices[&emitter.rt.json_parse];
    let alloc = emitter.rt.alloc;

    // param 0 = s, local 1 = parse_result, local 2 = result
    let mut f = Function::new([(2, ValType::I32)]);

    wasm!(f, {
        local_get(0); i32_const(0); call(parse_at_fn); local_set(1);
        local_get(1); i32_load(8);
        if_i32;
          i32_const(8); call(alloc); local_set(2);
          local_get(2); i32_const(1); i32_store(0);
          local_get(2); local_get(1); i32_load(0); i32_store(4);
          local_get(2);
        else_;
          i32_const(8); call(alloc); local_set(2);
          local_get(2); i32_const(0); i32_store(0);
          local_get(2); local_get(1); i32_load(0); i32_store(4);
          local_get(2);
        end;
        end;
    });

    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.json_parse, type_idx, f));

    compile_json_parse_at(emitter);
}

/// __json_parse_at(str_ptr: i32, pos: i32) -> i32
/// Returns ptr to [value_or_err: i32][new_pos: i32][err_flag: i32]
fn compile_json_parse_at(emitter: &mut WasmEmitter) {
    let parse_at_fn = emitter.rt.json_parse_at;
    let type_idx = emitter.func_type_indices[&parse_at_fn];
    let alloc = emitter.rt.alloc;
    let string_alloc = emitter.rt.string_alloc;
    let _concat = emitter.rt.concat_str;
    let _str_eq = emitter.rt.string.eq;
    // The number parser hands a float token to the SAME correctly-rounded parser
    // float.parse uses (__dec2flt), via a slice of the source (#663 / #667).
    let str_slice = emitter.rt.string.slice;
    let float_parse = emitter.rt.float_parse;

    let err_msg = emitter.intern_string("unexpected character in JSON");
    let err_eof = emitter.intern_string("unexpected end of input");

    // Locals:
    // param 0 = str_ptr, param 1 = pos
    // 2=result_ptr, 3=str_len, 4=ch, 5=start, 6=value_ptr, 7=tmp
    // 8=list_ptr, 9=count, 10=sub_result, 11=sign
    // 12=num_val(i64), 13=divisor(f64)
    // 14=capacity, 15=old_buf_save (for growable array/object parsing)
    let mut f = Function::new([
        (10, ValType::I32),
        (1, ValType::I64),
        (1, ValType::F64),
        (2, ValType::I32), // local 14 = capacity, local 15 = old_buf_save
    ]);

    // Allocate result struct (12 bytes)
    wasm!(f, {
        i32_const(12); call(alloc); local_set(2);
        local_get(0); i32_load(0); local_set(3);
    });

    // Skip whitespace
    emit_skip_ws(&mut f);

    // Check EOF
    wasm!(f, {
        local_get(1); local_get(3); i32_ge_u;
        if_empty;
          local_get(2); i32_const(err_eof as i32); i32_store(0);
          local_get(2); i32_const(0); i32_store(4);
          local_get(2); i32_const(1); i32_store(8);
          local_get(2); return_;
        end;
    });

    // Load current char
    wasm!(f, {
        local_get(0); i32_const(string_data_off()); i32_add; local_get(1); i32_add;
        i32_load8_u(0); local_set(4);
    });

    // ── null: check n,u,l,l ──
    wasm!(f, {
        local_get(4); i32_const(110); i32_eq; // 'n'
        if_empty;
          // Validate remaining chars: u(117), l(108), l(108)
          local_get(1); i32_const(3); i32_add; local_get(3); i32_lt_u; // need 3 more chars
          local_get(0); i32_const(string_data_off() + 1); i32_add; local_get(1); i32_add; i32_load8_u(0); i32_const(117); i32_eq;
          i32_and;
          local_get(0); i32_const(string_data_off() + 2); i32_add; local_get(1); i32_add; i32_load8_u(0); i32_const(108); i32_eq;
          i32_and;
          local_get(0); i32_const(string_data_off() + 3); i32_add; local_get(1); i32_add; i32_load8_u(0); i32_const(108); i32_eq;
          i32_and;
    });
    wasm!(f, {
          if_empty;
            local_get(1); i32_const(4); i32_add; local_set(1);
            i32_const(4); call(alloc); local_set(6);
            local_get(6); i32_const(0); i32_store(0);
            local_get(2); local_get(6); i32_store(0);
            local_get(2); local_get(1); i32_store(4);
            local_get(2); i32_const(0); i32_store(8);
            local_get(2); return_;
          end;
        end;
    });

    // ── true: check t,r,u,e ──
    wasm!(f, {
        local_get(4); i32_const(116); i32_eq; // 't'
        if_empty;
          local_get(1); i32_const(3); i32_add; local_get(3); i32_lt_u;
          local_get(0); i32_const(string_data_off() + 1); i32_add; local_get(1); i32_add; i32_load8_u(0); i32_const(114); i32_eq;
          i32_and;
          local_get(0); i32_const(string_data_off() + 2); i32_add; local_get(1); i32_add; i32_load8_u(0); i32_const(117); i32_eq;
          i32_and;
          local_get(0); i32_const(string_data_off() + 3); i32_add; local_get(1); i32_add; i32_load8_u(0); i32_const(101); i32_eq;
          i32_and;
    });
    wasm!(f, {
          if_empty;
            local_get(1); i32_const(4); i32_add; local_set(1);
            i32_const(8); call(alloc); local_set(6);
            local_get(6); i32_const(1); i32_store(0);
            local_get(6); i32_const(1); i32_store(4);
            local_get(2); local_get(6); i32_store(0);
            local_get(2); local_get(1); i32_store(4);
            local_get(2); i32_const(0); i32_store(8);
            local_get(2); return_;
          end;
        end;
    });

    // ── false: check f,a,l,s,e ──
    wasm!(f, {
        local_get(4); i32_const(102); i32_eq; // 'f'
        if_empty;
          local_get(1); i32_const(4); i32_add; local_get(3); i32_lt_u;
          local_get(0); i32_const(string_data_off() + 1); i32_add; local_get(1); i32_add; i32_load8_u(0); i32_const(97); i32_eq;
          i32_and;
          local_get(0); i32_const(string_data_off() + 2); i32_add; local_get(1); i32_add; i32_load8_u(0); i32_const(108); i32_eq;
          i32_and;
          local_get(0); i32_const(string_data_off() + 3); i32_add; local_get(1); i32_add; i32_load8_u(0); i32_const(115); i32_eq;
          i32_and;
          local_get(0); i32_const(string_data_off() + 4); i32_add; local_get(1); i32_add; i32_load8_u(0); i32_const(101); i32_eq;
          i32_and;
    });
    wasm!(f, {
          if_empty;
            local_get(1); i32_const(5); i32_add; local_set(1);
            i32_const(8); call(alloc); local_set(6);
            local_get(6); i32_const(1); i32_store(0);
            local_get(6); i32_const(0); i32_store(4);
            local_get(2); local_get(6); i32_store(0);
            local_get(2); local_get(1); i32_store(4);
            local_get(2); i32_const(0); i32_store(8);
            local_get(2); return_;
          end;
        end;
    });

    // ── String ──
    emit_parse_string(&mut f, alloc, string_alloc);

    // ── Number ──
    emit_parse_number(&mut f, alloc, str_slice, float_parse);

    // ── Array ──
    emit_parse_array(&mut f, alloc, parse_at_fn);

    // ── Object ──
    emit_parse_object(&mut f, alloc, parse_at_fn);

    // Fallback: error
    wasm!(f, {
        local_get(2); i32_const(err_msg as i32); i32_store(0);
        local_get(2); i32_const(0); i32_store(4);
        local_get(2); i32_const(1); i32_store(8);
        local_get(2);
        end;
    });

    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

/// Parse optional JSON exponent (e/E followed by optional +/- and digits).
/// Pushes f64 multiplier onto the stack: 10^exp (or 1.0 if no exponent).
/// Uses locals: 0=str_ptr, 1=pos, 3=str_len, 4=ch, 7=tmp, 12=num_val(i64), 13=divisor(f64)
fn emit_parse_exponent(f: &mut Function) {
    // Default multiplier = 1.0 (no exponent)
    wasm!(f, { f64_const(1.0); local_set(13); });

    // Check if we have e/E
    wasm!(f, {
        local_get(1); local_get(3); i32_lt_u;
        if_empty;
          local_get(0); i32_const(string_data_off()); i32_add; local_get(1); i32_add;
          i32_load8_u(0); local_set(4);
          local_get(4); i32_const(101); i32_eq; // 'e'
          local_get(4); i32_const(69); i32_eq;  // 'E'
          i32_or;
          if_empty;
            local_get(1); i32_const(1); i32_add; local_set(1);
            // exp_sign: check +/-
            i32_const(1); local_set(7); // exp_sign = 1 (positive)
            local_get(1); local_get(3); i32_lt_u;
            if_empty;
              local_get(0); i32_const(string_data_off()); i32_add; local_get(1); i32_add;
              i32_load8_u(0); local_set(4);
              local_get(4); i32_const(45); i32_eq; // '-'
              if_empty;
                i32_const(-1); local_set(7);
                local_get(1); i32_const(1); i32_add; local_set(1);
              else_;
                local_get(4); i32_const(43); i32_eq; // '+'
                if_empty;
                  local_get(1); i32_const(1); i32_add; local_set(1);
                end;
              end;
            end;
            // Parse exponent digits
            i64_const(0); local_set(12);
    });
    wasm!(f, {
            block_empty; loop_empty;
              local_get(1); local_get(3); i32_ge_u; br_if(1);
              local_get(0); i32_const(string_data_off()); i32_add; local_get(1); i32_add;
              i32_load8_u(0); local_set(4);
              local_get(4); i32_const(48); i32_lt_u; br_if(1);
              local_get(4); i32_const(57); i32_gt_u; br_if(1);
              local_get(12); i64_const(10); i64_mul;
              local_get(4); i32_const(48); i32_sub; i64_extend_i32_u;
              i64_add; local_set(12);
              local_get(1); i32_const(1); i32_add; local_set(1);
              br(0);
            end; end;
    });
    // Compute multiplier = 10^exp via loop, store in local 13
    wasm!(f, {
            f64_const(1.0); local_set(13);
            block_empty; loop_empty;
              local_get(12); i64_eqz; br_if(1);
              local_get(7); i32_const(0); i32_lt_s;
              if_empty;
                local_get(13); f64_const(0.1); f64_mul; local_set(13);
              else_;
                local_get(13); f64_const(10.0); f64_mul; local_set(13);
              end;
              local_get(12); i64_const(1); i64_sub; local_set(12);
              br(0);
            end; end;
          end; // if e/E
        end; // if pos < len
    });

    // Push multiplier onto stack
    wasm!(f, { local_get(13); });
}

/// Emit whitespace-skipping loop.
/// Uses locals: 0=str_ptr, 1=pos, 3=str_len, 4=ch
fn emit_skip_ws(f: &mut Function) {
    wasm!(f, {
        block_empty; loop_empty;
          local_get(1); local_get(3); i32_ge_u; br_if(1);
          local_get(0); i32_const(string_data_off()); i32_add; local_get(1); i32_add;
          i32_load8_u(0); local_set(4);
          local_get(4); i32_const(32); i32_eq;
          local_get(4); i32_const(9); i32_eq; i32_or;
          local_get(4); i32_const(10); i32_eq; i32_or;
          local_get(4); i32_const(13); i32_eq; i32_or;
          i32_eqz; br_if(1);
          local_get(1); i32_const(1); i32_add; local_set(1);
          br(0);
        end; end;
    });
}

/// Decode 4 hex digits at `[str_ptr + data + start(5) + read_off(8) + off0 .. +4)`
/// into `target` (an i32 local). Uppercase and lowercase A–F both decode. Used
/// for `\uXXXX` escapes and their low-surrogate continuation (#651). Local 11 is
/// scratch.
fn emit_parse_hex4(f: &mut Function, off0: i32, target: u32) {
    wasm!(f, { i32_const(0); local_set(target); });
    for k in off0..off0 + 4 {
        wasm!(f, {
            local_get(0); i32_const(string_data_off()); i32_add; local_get(5); i32_add; local_get(8); i32_add; i32_const(k); i32_add;
            i32_load8_u(0); local_set(11);
            // digit = byte < ':' ? byte - '0' : (byte | 0x20) - ('a' - 10)
            local_get(11); i32_const(58); i32_lt_u;
            if_i32;
              local_get(11); i32_const(48); i32_sub;
            else_;
              local_get(11); i32_const(32); i32_or; i32_const(87); i32_sub;
            end;
            local_get(target); i32_const(4); i32_shl; i32_add; local_set(target);
        });
    }
}

/// Emit `out[write_off(9)] = (cp(10) >> shift & mask) | high; write_off += 1`.
/// One UTF-8 byte derived from codepoint local 10. For a continuation byte:
/// `high=0x80, mask=0x3F`; for a lead byte the high prefix and a wider mask. The
/// 1-byte (ASCII) case uses `shift=0, mask=0x7F, high=0`. `out` base is local 6.
fn emit_utf8_byte(f: &mut Function, shift: i32, mask: i32, high: i32) {
    wasm!(f, {
        local_get(6); i32_const(string_data_off()); i32_add; local_get(9); i32_add;
        local_get(10); i32_const(shift); i32_shr_u; i32_const(mask); i32_and; i32_const(high); i32_or;
        i32_store8(0);
        local_get(9); i32_const(1); i32_add; local_set(9);
    });
}

/// `\uXXXX` (plus a high+low surrogate pair) → UTF-8 bytes, then continue the
/// copy loop. Assumes the escape char is in local 4, read_off (8) points at the
/// `u`, and we are nested inside `loop → backslash-if` so `br(2)` re-enters the
/// loop — bypassing the single-byte write that handles the simple escapes. The
/// simple-escape rewrites that follow this call run only when the char is not
/// `u`. Mirrors serde_json's `\u` decoding. #651.
fn emit_unicode_escape_branch(f: &mut Function) {
    wasm!(f, {
        local_get(4); i32_const(117); i32_eq; // 'u'
        if_empty;
    });
    // codepoint (local 10) from the 4 hex following the `u`.
    emit_parse_hex4(f, 1, 10);
    wasm!(f, { local_get(8); i32_const(5); i32_add; local_set(8); }); // consumed `u` + 4 hex
    // Surrogate pair: a high surrogate (D800..DBFF) immediately followed by a
    // "\uYYYY" low surrogate (DC00..DFFF) forms one astral codepoint.
    wasm!(f, {
        local_get(10); i32_const(0xD800); i32_ge_u;
        local_get(10); i32_const(0xDBFF); i32_le_u;
        i32_and;
        if_empty;
          local_get(0); i32_const(string_data_off()); i32_add; local_get(5); i32_add; local_get(8); i32_add; i32_load8_u(0); i32_const(92); i32_eq;
          local_get(0); i32_const(string_data_off()); i32_add; local_get(5); i32_add; local_get(8); i32_add; i32_const(1); i32_add; i32_load8_u(0); i32_const(117); i32_eq;
          i32_and;
          if_empty;
    });
    emit_parse_hex4(f, 2, 14); // low surrogate → local 14
    wasm!(f, {
            local_get(14); i32_const(0xDC00); i32_ge_u;
            local_get(14); i32_const(0xDFFF); i32_le_u;
            i32_and;
            if_empty;
              local_get(10); i32_const(0xD800); i32_sub; i32_const(10); i32_shl;
              local_get(14); i32_const(0xDC00); i32_sub; i32_add;
              i32_const(0x10000); i32_add; local_set(10);
              local_get(8); i32_const(6); i32_add; local_set(8); // consumed "\uYYYY"
            end;
          end;
        end;
    });
    // UTF-8 encode cp(10): 1 byte (<0x80), 2 (<0x800), 3 (<0x10000), else 4.
    wasm!(f, { local_get(10); i32_const(0x80); i32_lt_u; if_empty; });
    emit_utf8_byte(f, 0, 0x7F, 0x00);
    wasm!(f, { else_; local_get(10); i32_const(0x800); i32_lt_u; if_empty; });
    emit_utf8_byte(f, 6, 0x1F, 0xC0);
    emit_utf8_byte(f, 0, 0x3F, 0x80);
    wasm!(f, { else_; local_get(10); i32_const(0x10000); i32_lt_u; if_empty; });
    emit_utf8_byte(f, 12, 0x0F, 0xE0);
    emit_utf8_byte(f, 6, 0x3F, 0x80);
    emit_utf8_byte(f, 0, 0x3F, 0x80);
    wasm!(f, { else_; });
    emit_utf8_byte(f, 18, 0x07, 0xF0);
    emit_utf8_byte(f, 12, 0x3F, 0x80);
    emit_utf8_byte(f, 6, 0x3F, 0x80);
    emit_utf8_byte(f, 0, 0x3F, 0x80);
    wasm!(f, {
            end; end; end; // close the <0x80 / <0x800 / <0x10000 chain
        br(2);             // continue the copy loop; read_off already advanced
        end;               // close the `u`-if
    });
}

/// Parse JSON string starting at current pos (ch=='"').
/// Uses locals: 0=str_ptr, 1=pos, 2=result_ptr, 3=str_len, 4=ch, 5=start, 6=value_ptr, 7=tmp, 9=count
fn emit_parse_string(f: &mut Function, alloc: u32, string_alloc: u32) {
    wasm!(f, {
        local_get(4); i32_const(34); i32_eq;
        if_empty;
          local_get(1); i32_const(1); i32_add; local_set(1);
          local_get(1); local_set(5);
    });
    // Find closing quote
    wasm!(f, {
          block_empty; loop_empty;
            local_get(1); local_get(3); i32_ge_u; br_if(1);
            local_get(0); i32_const(string_data_off()); i32_add; local_get(1); i32_add;
            i32_load8_u(0); local_set(7);
            local_get(7); i32_const(34); i32_eq; br_if(1);
            local_get(7); i32_const(92); i32_eq;
            if_empty;
              local_get(1); i32_const(1); i32_add; local_set(1);
            end;
            local_get(1); i32_const(1); i32_add; local_set(1);
            br(0);
          end; end;
    });
    // Build string
    wasm!(f, {
          local_get(1); local_get(5); i32_sub; local_set(7);
          local_get(7); call(string_alloc); local_set(6);
          i32_const(0); local_set(9);
    });
    // Copy bytes loop with JSON escape decoding.
    // Locals: 8 = read offset, 9 = write offset, 4 = current byte (reused).
    // Handles \n \t \r \" \\ \/ \b \f. \uXXXX is not yet supported (passes through).
    wasm!(f, {
          i32_const(0); local_set(8);
          i32_const(0); local_set(9);
          block_empty; loop_empty;
            local_get(8); local_get(7); i32_ge_u; br_if(1);
            // byte = in[in_base + read_off]
            local_get(0); i32_const(string_data_off()); i32_add; local_get(5); i32_add; local_get(8); i32_add;
            i32_load8_u(0); local_set(4);
            // if byte == 0x5C (backslash): decode next byte
            local_get(4); i32_const(92); i32_eq;
            if_empty;
              // advance read past backslash
              local_get(8); i32_const(1); i32_add; local_set(8);
              local_get(8); local_get(7); i32_ge_u; br_if(2);
              // load next byte
              local_get(0); i32_const(string_data_off()); i32_add; local_get(5); i32_add; local_get(8); i32_add;
              i32_load8_u(0); local_set(4);
    });
    // \uXXXX decodes to UTF-8 and continues the loop (multi-byte); the simple
    // single-byte escapes below run only when the char is not `u`. #651.
    emit_unicode_escape_branch(f);
    wasm!(f, {
              // Decode escape: overwrite local 4 in-place via if/else chain.
              // Decoded values (8,9,10,12,13) and idempotent ones (34,47,92) don't
              // collide with the source codes for other escapes (110,116,114,98,102),
              // so a sequential pass is safe.
              local_get(4); i32_const(110); i32_eq; if_empty; i32_const(10); local_set(4); end;  // n
              local_get(4); i32_const(116); i32_eq; if_empty; i32_const(9);  local_set(4); end;  // t
              local_get(4); i32_const(114); i32_eq; if_empty; i32_const(13); local_set(4); end;  // r
              local_get(4); i32_const(98);  i32_eq; if_empty; i32_const(8);  local_set(4); end;  // b
              local_get(4); i32_const(102); i32_eq; if_empty; i32_const(12); local_set(4); end;  // f
              // ", \, /  decode to themselves — no rewrite needed.
            end;
            // out[out_base + write_off] = byte
            local_get(6); i32_const(string_data_off()); i32_add; local_get(9); i32_add;
            local_get(4);
            i32_store8(0);
            local_get(9); i32_const(1); i32_add; local_set(9);
            local_get(8); i32_const(1); i32_add; local_set(8);
            br(0);
          end; end;
    });
    // Build Value and return — write actual decoded length (write_off)
    wasm!(f, {
          local_get(6); local_get(9); i32_store(0);
          local_get(1); i32_const(1); i32_add; local_set(1);
          i32_const(8); call(alloc); local_set(7);
          local_get(7); i32_const(4); i32_store(0);
          local_get(7); local_get(6); i32_store(4);
          local_get(2); local_get(7); i32_store(0);
          local_get(2); local_get(1); i32_store(4);
          local_get(2); i32_const(0); i32_store(8);
          local_get(2); return_;
        end;
    });
}

/// Parse JSON number. Uses: 0=str_ptr, 1=pos, 2=result_ptr, 3=str_len, 4=ch,
/// 5=start, 6=value_ptr, 11=sign, 12=num_val(i64), 13=divisor(f64)
fn emit_parse_number(f: &mut Function, alloc: u32, str_slice: u32, float_parse: u32) {
    // Check if number
    wasm!(f, {
        local_get(4); i32_const(45); i32_eq;
        local_get(4); i32_const(48); i32_ge_u;
        local_get(4); i32_const(57); i32_le_u;
        i32_and; i32_or;
        if_empty;
          // Save the token start (incl. a leading '-') so a float token can be
          // re-parsed by the correctly-rounded float.parse (#663 / #667).
          local_get(1); local_set(5);
          i32_const(1); local_set(11);
          i64_const(0); local_set(12);
          local_get(4); i32_const(45); i32_eq;
          if_empty;
            i32_const(-1); local_set(11);
            local_get(1); i32_const(1); i32_add; local_set(1);
          end;
    });
    // Parse integer digits
    wasm!(f, {
          block_empty; loop_empty;
            local_get(1); local_get(3); i32_ge_u; br_if(1);
            local_get(0); i32_const(string_data_off()); i32_add; local_get(1); i32_add;
            i32_load8_u(0); local_set(4);
            local_get(4); i32_const(48); i32_lt_u; br_if(1);
            local_get(4); i32_const(57); i32_gt_u; br_if(1);
            local_get(12); i64_const(10); i64_mul;
            local_get(4); i32_const(48); i32_sub; i64_extend_i32_u;
            i64_add; local_set(12);
            local_get(1); i32_const(1); i32_add; local_set(1);
            br(0);
          end; end;
    });
    // Check for decimal point
    wasm!(f, {
          local_get(1); local_get(3); i32_lt_u;
          if_empty;
            local_get(0); i32_const(string_data_off()); i32_add; local_get(1); i32_add;
            i32_load8_u(0); i32_const(46); i32_eq;
            if_empty;
              local_get(1); i32_const(1); i32_add; local_set(1);
              f64_const(1.0); local_set(13);
    });
    // Parse decimal digits
    wasm!(f, {
              block_empty; loop_empty;
                local_get(1); local_get(3); i32_ge_u; br_if(1);
                local_get(0); i32_const(string_data_off()); i32_add; local_get(1); i32_add;
                i32_load8_u(0); local_set(4);
                local_get(4); i32_const(48); i32_lt_u; br_if(1);
                local_get(4); i32_const(57); i32_gt_u; br_if(1);
                local_get(12); i64_const(10); i64_mul;
                local_get(4); i32_const(48); i32_sub; i64_extend_i32_u;
                i64_add; local_set(12);
                local_get(13); f64_const(10.0); f64_mul; local_set(13);
                local_get(1); i32_const(1); i32_add; local_set(1);
                br(0);
              end; end;
    });
    // Build float Value. The number token [start, pos) is handed to the SAME
    // correctly-rounded parser float.parse uses (__dec2flt) instead of an ad-hoc
    // `digits/divisor * 10^exp` that dropped the -0.0 sign (#663) and rounded
    // exponent forms off by a ULP (#667). emit_parse_exponent only advances pos
    // past the e/E digits here; its ad-hoc multiplier is dropped.
    emit_parse_exponent(f);
    wasm!(f, {
              drop;
              local_get(0); local_get(5); local_get(1); call(str_slice);
              call(float_parse); local_set(7); // Result[Float, String] ptr; valid JSON ⇒ ok
              i32_const(12); call(alloc); local_set(6);
              local_get(6); i32_const(3); i32_store(0);
              local_get(6); local_get(7); f64_load(4); f64_store(4);
              local_get(2); local_get(6); i32_store(0);
              local_get(2); local_get(1); i32_store(4);
              local_get(2); i32_const(0); i32_store(8);
              local_get(2); return_;
            end;
          end;
    });
    // Integer path: check for exponent → becomes float
    wasm!(f, {
          // Check for e/E after integer
          local_get(1); local_get(3); i32_lt_u;
          if_empty;
            local_get(0); i32_const(string_data_off()); i32_add; local_get(1); i32_add;
            i32_load8_u(0); local_set(4);
            local_get(4); i32_const(101); i32_eq; // 'e'
            local_get(4); i32_const(69); i32_eq;  // 'E'
            i32_or;
            if_empty;
              // Integer mantissa with an exponent suffix → a float. Re-parse the
              // whole token with the correctly-rounded float.parse (#667).
    });
    emit_parse_exponent(f);
    wasm!(f, {
              drop;
              local_get(0); local_get(5); local_get(1); call(str_slice);
              call(float_parse); local_set(7);
              i32_const(12); call(alloc); local_set(6);
              local_get(6); i32_const(3); i32_store(0);
              local_get(6); local_get(7); f64_load(4); f64_store(4);
              local_get(2); local_get(6); i32_store(0);
              local_get(2); local_get(1); i32_store(4);
              local_get(2); i32_const(0); i32_store(8);
              local_get(2); return_;
            end;
          end;
    });
    // Build int Value (no exponent)
    wasm!(f, {
          i32_const(12); call(alloc); local_set(6);
          local_get(6); i32_const(2); i32_store(0);
          local_get(6);
          local_get(11); i64_extend_i32_s; local_get(12); i64_mul;
          i64_store(4);
          local_get(2); local_get(6); i32_store(0);
          local_get(2); local_get(1); i32_store(4);
          local_get(2); i32_const(0); i32_store(8);
          local_get(2); return_;
        end;
    });
}

/// Parse JSON array.
fn emit_parse_array(f: &mut Function, alloc: u32, parse_at_fn: u32) {
    wasm!(f, {
        local_get(4); i32_const(91); i32_eq;
        if_empty;
          local_get(1); i32_const(1); i32_add; local_set(1);
    });
    // Skip whitespace (inline)
    wasm!(f, {
          block_empty; loop_empty;
            local_get(1); local_get(3); i32_ge_u; br_if(1);
            local_get(0); i32_const(string_data_off()); i32_add; local_get(1); i32_add;
            i32_load8_u(0); local_set(4);
            local_get(4); i32_const(32); i32_eq;
            local_get(4); i32_const(9); i32_eq; i32_or;
            local_get(4); i32_const(10); i32_eq; i32_or;
            local_get(4); i32_const(13); i32_eq; i32_or;
            i32_eqz; br_if(1);
            local_get(1); i32_const(1); i32_add; local_set(1);
            br(0);
          end; end;
    });
    // Check empty array
    wasm!(f, {
          local_get(1); local_get(3); i32_lt_u;
          if_empty;
            local_get(0); i32_const(string_data_off()); i32_add; local_get(1); i32_add;
            i32_load8_u(0); i32_const(93); i32_eq;
            if_empty;
              local_get(1); i32_const(1); i32_add; local_set(1);
              i32_const(list_hdr()); call(alloc); local_set(8);
              local_get(8); i32_const(0); i32_store(0);
              i32_const(8); call(alloc); local_set(6);
              local_get(6); i32_const(5); i32_store(0);
              local_get(6); local_get(8); i32_store(4);
              local_get(2); local_get(6); i32_store(0);
              local_get(2); local_get(1); i32_store(4);
              local_get(2); i32_const(0); i32_store(8);
              local_get(2); return_;
            end;
          end;
    });
    // Parse elements — growable buffer (local 14 = capacity)
    wasm!(f, {
          i32_const(64); local_set(14); // initial capacity
          i32_const(264); call(alloc); local_set(8); // 8 + 64*4
          i32_const(0); local_set(9); // count = 0
          block_empty; loop_empty;
            local_get(0); local_get(1);
            call(parse_at_fn); local_set(10);
            local_get(10); i32_load(8);
            if_empty;
              local_get(2); local_get(10); i32_load(0); i32_store(0);
              local_get(2); i32_const(0); i32_store(4);
              local_get(2); i32_const(1); i32_store(8);
              local_get(2); return_;
            end;
    });
    // Grow buffer if count >= capacity (uses only locals 14, 15)
    wasm!(f, {
            local_get(9); local_get(14); i32_ge_u;
            if_empty;
              local_get(8); local_set(15); // save old buf
              local_get(14); i32_const(1); i32_shl; local_set(14); // cap *= 2
              i32_const(list_hdr()); local_get(14); i32_const(4); i32_mul; i32_add;
              call(alloc); local_set(8); // new buf → local 8
              local_get(8); local_get(15);
              i32_const(list_hdr()); local_get(9); i32_const(4); i32_mul; i32_add;
              memory_copy;
            end;
    });
    wasm!(f, {
            local_get(8); i32_const(list_data_off()); i32_add;
            local_get(9); i32_const(4); i32_mul; i32_add;
            local_get(10); i32_load(0); i32_store(0);
            local_get(10); i32_load(4); local_set(1);
            local_get(9); i32_const(1); i32_add; local_set(9);
    });
    // Skip whitespace after element
    wasm!(f, {
            block_empty; loop_empty;
              local_get(1); local_get(3); i32_ge_u; br_if(1);
              local_get(0); i32_const(string_data_off()); i32_add; local_get(1); i32_add;
              i32_load8_u(0); local_set(4);
              local_get(4); i32_const(32); i32_eq;
              local_get(4); i32_const(9); i32_eq; i32_or;
              local_get(4); i32_const(10); i32_eq; i32_or;
              local_get(4); i32_const(13); i32_eq; i32_or;
              i32_eqz; br_if(1);
              local_get(1); i32_const(1); i32_add; local_set(1);
              br(0);
            end; end;
    });
    // Check ] or ,
    wasm!(f, {
            local_get(1); local_get(3); i32_ge_u; br_if(1);
            local_get(0); i32_const(string_data_off()); i32_add; local_get(1); i32_add;
            i32_load8_u(0); local_set(4);
            local_get(4); i32_const(93); i32_eq;
            if_empty;
              local_get(1); i32_const(1); i32_add; local_set(1);
              br(2);
            end;
            local_get(4); i32_const(44); i32_eq;
            if_empty;
              local_get(1); i32_const(1); i32_add; local_set(1);
            end;
            br(0);
          end; end;
    });
    // Build result
    wasm!(f, {
          local_get(8); local_get(9); i32_store(0);
          i32_const(8); call(alloc); local_set(6);
          local_get(6); i32_const(5); i32_store(0);
          local_get(6); local_get(8); i32_store(4);
          local_get(2); local_get(6); i32_store(0);
          local_get(2); local_get(1); i32_store(4);
          local_get(2); i32_const(0); i32_store(8);
          local_get(2); return_;
        end;
    });
}

/// Parse JSON object.
fn emit_parse_object(f: &mut Function, alloc: u32, parse_at_fn: u32) {
    wasm!(f, {
        local_get(4); i32_const(123); i32_eq;
        if_empty;
          local_get(1); i32_const(1); i32_add; local_set(1);
    });
    // Skip whitespace
    wasm!(f, {
          block_empty; loop_empty;
            local_get(1); local_get(3); i32_ge_u; br_if(1);
            local_get(0); i32_const(string_data_off()); i32_add; local_get(1); i32_add;
            i32_load8_u(0); local_set(4);
            local_get(4); i32_const(32); i32_eq;
            local_get(4); i32_const(9); i32_eq; i32_or;
            local_get(4); i32_const(10); i32_eq; i32_or;
            local_get(4); i32_const(13); i32_eq; i32_or;
            i32_eqz; br_if(1);
            local_get(1); i32_const(1); i32_add; local_set(1);
            br(0);
          end; end;
    });
    // Check empty object
    wasm!(f, {
          local_get(1); local_get(3); i32_lt_u;
          if_empty;
            local_get(0); i32_const(string_data_off()); i32_add; local_get(1); i32_add;
            i32_load8_u(0); i32_const(125); i32_eq;
            if_empty;
              local_get(1); i32_const(1); i32_add; local_set(1);
              i32_const(list_hdr()); call(alloc); local_set(8);
              local_get(8); i32_const(0); i32_store(0);
              i32_const(8); call(alloc); local_set(6);
              local_get(6); i32_const(6); i32_store(0);
              local_get(6); local_get(8); i32_store(4);
              local_get(2); local_get(6); i32_store(0);
              local_get(2); local_get(1); i32_store(4);
              local_get(2); i32_const(0); i32_store(8);
              local_get(2); return_;
            end;
          end;
    });
    // Parse key-value pairs — growable buffer (local 14 = capacity)
    wasm!(f, {
          i32_const(64); local_set(14); // initial capacity
          i32_const(264); call(alloc); local_set(8); // 8 + 64*4
          i32_const(0); local_set(9);
          block_empty; loop_empty;
    });
    // Skip whitespace before key
    wasm!(f, {
            block_empty; loop_empty;
              local_get(1); local_get(3); i32_ge_u; br_if(1);
              local_get(0); i32_const(string_data_off()); i32_add; local_get(1); i32_add;
              i32_load8_u(0); local_set(4);
              local_get(4); i32_const(32); i32_eq;
              local_get(4); i32_const(9); i32_eq; i32_or;
              local_get(4); i32_const(10); i32_eq; i32_or;
              local_get(4); i32_const(13); i32_eq; i32_or;
              i32_eqz; br_if(1);
              local_get(1); i32_const(1); i32_add; local_set(1);
              br(0);
            end; end;
    });
    // Parse key
    wasm!(f, {
            local_get(0); local_get(1);
            call(parse_at_fn); local_set(10);
            local_get(10); i32_load(8);
            if_empty;
              local_get(2); local_get(10); i32_load(0); i32_store(0);
              local_get(2); i32_const(0); i32_store(4);
              local_get(2); i32_const(1); i32_store(8);
              local_get(2); return_;
            end;
            local_get(10); i32_load(0); i32_load(4); local_set(7);
            local_get(10); i32_load(4); local_set(1);
    });
    // Skip whitespace and colon
    wasm!(f, {
            block_empty; loop_empty;
              local_get(1); local_get(3); i32_ge_u; br_if(1);
              local_get(0); i32_const(string_data_off()); i32_add; local_get(1); i32_add;
              i32_load8_u(0); local_set(4);
              local_get(4); i32_const(32); i32_eq;
              local_get(4); i32_const(9); i32_eq; i32_or;
              local_get(4); i32_const(10); i32_eq; i32_or;
              local_get(4); i32_const(13); i32_eq; i32_or;
              local_get(4); i32_const(58); i32_eq; i32_or;
              i32_eqz; br_if(1);
              local_get(1); i32_const(1); i32_add; local_set(1);
              br(0);
            end; end;
    });
    // Parse value
    wasm!(f, {
            local_get(0); local_get(1);
            call(parse_at_fn); local_set(10);
            local_get(10); i32_load(8);
            if_empty;
              local_get(2); local_get(10); i32_load(0); i32_store(0);
              local_get(2); i32_const(0); i32_store(4);
              local_get(2); i32_const(1); i32_store(8);
              local_get(2); return_;
            end;
    });
    // Grow object list buffer if count >= capacity (uses only locals 14, 15)
    wasm!(f, {
            local_get(9); local_get(14); i32_ge_u;
            if_empty;
              local_get(8); local_set(15); // save old buf
              local_get(14); i32_const(1); i32_shl; local_set(14); // cap *= 2
              i32_const(list_hdr()); local_get(14); i32_const(4); i32_mul; i32_add;
              call(alloc); local_set(8); // new buf
              local_get(8); local_get(15);
              i32_const(list_hdr()); local_get(9); i32_const(4); i32_mul; i32_add;
              memory_copy;
            end;
    });
    // Allocate tuple (key_str_ptr, value_ptr) and store pointer in list
    wasm!(f, {
            // Allocate 8-byte tuple: [key_str_ptr: i32][value_ptr: i32]
            i32_const(8); call(alloc); local_set(5); // reuse local 5 as tuple_ptr
            local_get(5); local_get(7); i32_store(0); // key
            local_get(5); local_get(10); i32_load(0); i32_store(4); // value
            // Store tuple pointer in list at position count
            local_get(8); i32_const(list_data_off()); i32_add;
            local_get(9); i32_const(4); i32_mul; i32_add;
            local_get(5); i32_store(0);
            local_get(10); i32_load(4); local_set(1);
            local_get(9); i32_const(1); i32_add; local_set(9);
    });
    // Skip whitespace after value
    wasm!(f, {
            block_empty; loop_empty;
              local_get(1); local_get(3); i32_ge_u; br_if(1);
              local_get(0); i32_const(string_data_off()); i32_add; local_get(1); i32_add;
              i32_load8_u(0); local_set(4);
              local_get(4); i32_const(32); i32_eq;
              local_get(4); i32_const(9); i32_eq; i32_or;
              local_get(4); i32_const(10); i32_eq; i32_or;
              local_get(4); i32_const(13); i32_eq; i32_or;
              i32_eqz; br_if(1);
              local_get(1); i32_const(1); i32_add; local_set(1);
              br(0);
            end; end;
    });
    // Check } or ,
    wasm!(f, {
            local_get(1); local_get(3); i32_ge_u; br_if(1);
            local_get(0); i32_const(string_data_off()); i32_add; local_get(1); i32_add;
            i32_load8_u(0); local_set(4);
            local_get(4); i32_const(125); i32_eq;
            if_empty;
              local_get(1); i32_const(1); i32_add; local_set(1);
              br(2);
            end;
            local_get(4); i32_const(44); i32_eq;
            if_empty;
              local_get(1); i32_const(1); i32_add; local_set(1);
            end;
            br(0);
          end; end;
    });
    // Build object result
    wasm!(f, {
          local_get(8); local_get(9); i32_store(0);
          i32_const(8); call(alloc); local_set(6);
          local_get(6); i32_const(6); i32_store(0);
          local_get(6); local_get(8); i32_store(4);
          local_get(2); local_get(6); i32_store(0);
          local_get(2); local_get(1); i32_store(4);
          local_get(2); i32_const(0); i32_store(8);
          local_get(2); return_;
        end;
    });
}

