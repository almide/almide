//! WASM runtime: base64 and hex encoding/decoding.
//!
//! All helpers operate on the standard `[len:i32][data:u8...]` layout shared
//! by `Bytes` and `String`. Decoders return a `Result[Bytes, String]` cell
//! (`[tag:i32][value:i32]` = 8 bytes; tag=0 ok with Bytes ptr, tag=1 err with
//! String ptr).
//!
//! Avoiding heap segments means we compute alphabet characters arithmetically:
//!   base64 alphabet (std):     0..25 → 'A'+i, 26..51 → 'a'+(i-26),
//!                              52..61 → '0'+(i-52), 62 → '+' (or '-'), 63 → '/' (or '_')
//!   hex alphabet (lower):      0..9 → '0'+i, 10..15 → 'a'+(i-10)
//!   hex alphabet (upper):      0..9 → '0'+i, 10..15 → 'A'+(i-10)

use super::{CompiledFunc, WasmEmitter};
use wasm_encoder::{Function, ValType};

/// __base64_encode(buf_ptr) -> string_ptr.
/// `url_safe = true` swaps '+'/'/' for '-'/'_' and omits '=' padding.
pub(super) fn compile_base64_encode(emitter: &mut WasmEmitter, url_safe: bool) {
    let func_id = if url_safe { emitter.rt.base64_encode_url } else { emitter.rt.base64_encode };
    let type_idx = emitter.func_type_indices[&func_id];
    // params: 0 = buf_ptr (i32)
    // locals:
    //   1 = byte_len, 2 = out_len, 3 = out_ptr, 4 = i (input idx),
    //   5 = j (output idx), 6 = b0, 7 = b1, 8 = b2, 9 = idx (alphabet index)
    let mut f = Function::new([
        (1, ValType::I32),  // 1: byte_len
        (1, ValType::I32),  // 2: out_len
        (1, ValType::I32),  // 3: out_ptr
        (1, ValType::I32),  // 4: i
        (1, ValType::I32),  // 5: j
        (1, ValType::I32),  // 6: b0
        (1, ValType::I32),  // 7: b1
        (1, ValType::I32),  // 8: b2
        (1, ValType::I32),  // 9: idx
    ]);

    // byte_len = buf_ptr[0]
    wasm!(f, {
        local_get(0); i32_load(0); local_set(1);
    });

    // groups of 3 → 4 chars; remainder padded (or not).
    // out_len = ((byte_len + 2) / 3) * 4 with std (padded), or
    //           ((byte_len * 4) + 2) / 3 with url_safe (no padding) — equivalent
    if url_safe {
        // ceil(byte_len * 4 / 3)
        wasm!(f, {
            local_get(1); i32_const(4); i32_mul;
            i32_const(2); i32_add;
            i32_const(3); i32_div_u;
            local_set(2);
        });
    } else {
        wasm!(f, {
            local_get(1); i32_const(2); i32_add;
            i32_const(3); i32_div_u;
            i32_const(4); i32_mul;
            local_set(2);
        });
    }

    // out_ptr = alloc(4 + out_len); out_ptr[0] = out_len
    wasm!(f, {
        local_get(2); i32_const(4); i32_add;
        call(emitter.rt.alloc); local_set(3);
        local_get(3); local_get(2); i32_store(0);
        i32_const(0); local_set(4); // i = 0
        i32_const(0); local_set(5); // j = 0
    });

    // Main loop: process 3 input bytes → 4 output chars
    wasm!(f, {
        block_empty; loop_empty;
            local_get(4); i32_const(3); i32_add; local_get(1); i32_gt_u; br_if(1);
            // Load 3 bytes b0, b1, b2
            local_get(0); i32_const(4); i32_add; local_get(4); i32_add; i32_load8_u(0); local_set(6);
            local_get(0); i32_const(5); i32_add; local_get(4); i32_add; i32_load8_u(0); local_set(7);
            local_get(0); i32_const(6); i32_add; local_get(4); i32_add; i32_load8_u(0); local_set(8);
            // c0 = alphabet[b0 >> 2]
            local_get(6); i32_const(2); i32_shr_u; local_set(9);
    });
    emit_b64_alphabet_lookup(&mut f, 9, url_safe);
    wasm!(f, {
            local_set(9);
            local_get(3); i32_const(4); i32_add; local_get(5); i32_add;
            local_get(9);
            i32_store8(0);
            // c1 = alphabet[((b0 & 3) << 4) | (b1 >> 4)]
            local_get(6); i32_const(3); i32_and; i32_const(4); i32_shl;
            local_get(7); i32_const(4); i32_shr_u;
            i32_or; local_set(9);
    });
    emit_b64_alphabet_lookup(&mut f, 9, url_safe);
    wasm!(f, {
            local_set(9);
            local_get(3); i32_const(5); i32_add; local_get(5); i32_add;
            local_get(9);
            i32_store8(0);
            // c2 = alphabet[((b1 & 0xF) << 2) | (b2 >> 6)]
            local_get(7); i32_const(15); i32_and; i32_const(2); i32_shl;
            local_get(8); i32_const(6); i32_shr_u;
            i32_or; local_set(9);
    });
    emit_b64_alphabet_lookup(&mut f, 9, url_safe);
    wasm!(f, {
            local_set(9);
            local_get(3); i32_const(6); i32_add; local_get(5); i32_add;
            local_get(9);
            i32_store8(0);
            // c3 = alphabet[b2 & 0x3F]
            local_get(8); i32_const(63); i32_and; local_set(9);
    });
    emit_b64_alphabet_lookup(&mut f, 9, url_safe);
    wasm!(f, {
            local_set(9);
            local_get(3); i32_const(7); i32_add; local_get(5); i32_add;
            local_get(9);
            i32_store8(0);
            local_get(4); i32_const(3); i32_add; local_set(4);
            local_get(5); i32_const(4); i32_add; local_set(5);
            br(0);
        end; end;
    });

    // Tail: handle rem 1 or 2 bytes
    wasm!(f, {
        local_get(1); local_get(4); i32_sub; // rem
        local_set(9); // reuse local 9 as rem
        local_get(9); i32_const(1); i32_eq;
        if_empty;
            // 1 byte: 2 output chars + (== padding if std)
            local_get(0); i32_const(4); i32_add; local_get(4); i32_add; i32_load8_u(0); local_set(6);
            local_get(6); i32_const(2); i32_shr_u; local_set(9);
    });
    emit_b64_alphabet_lookup(&mut f, 9, url_safe);
    wasm!(f, {
            local_set(9);
            local_get(3); i32_const(4); i32_add; local_get(5); i32_add;
            local_get(9);
            i32_store8(0);
            local_get(6); i32_const(3); i32_and; i32_const(4); i32_shl; local_set(9);
    });
    emit_b64_alphabet_lookup(&mut f, 9, url_safe);
    wasm!(f, {
            local_set(9);
            local_get(3); i32_const(5); i32_add; local_get(5); i32_add;
            local_get(9);
            i32_store8(0);
    });
    if !url_safe {
        wasm!(f, {
            // padding '=' '='
            local_get(3); i32_const(6); i32_add; local_get(5); i32_add;
            i32_const(61); i32_store8(0);
            local_get(3); i32_const(7); i32_add; local_get(5); i32_add;
            i32_const(61); i32_store8(0);
        });
    }
    wasm!(f, {
        end;
        local_get(1); local_get(4); i32_sub;
        i32_const(2); i32_eq;
        if_empty;
            // 2 bytes: 3 output chars + 1 padding (if std)
            local_get(0); i32_const(4); i32_add; local_get(4); i32_add; i32_load8_u(0); local_set(6);
            local_get(0); i32_const(5); i32_add; local_get(4); i32_add; i32_load8_u(0); local_set(7);
            local_get(6); i32_const(2); i32_shr_u; local_set(9);
    });
    emit_b64_alphabet_lookup(&mut f, 9, url_safe);
    wasm!(f, {
            local_set(9);
            local_get(3); i32_const(4); i32_add; local_get(5); i32_add;
            local_get(9);
            i32_store8(0);
            local_get(6); i32_const(3); i32_and; i32_const(4); i32_shl;
            local_get(7); i32_const(4); i32_shr_u;
            i32_or; local_set(9);
    });
    emit_b64_alphabet_lookup(&mut f, 9, url_safe);
    wasm!(f, {
            local_set(9);
            local_get(3); i32_const(5); i32_add; local_get(5); i32_add;
            local_get(9);
            i32_store8(0);
            local_get(7); i32_const(15); i32_and; i32_const(2); i32_shl; local_set(9);
    });
    emit_b64_alphabet_lookup(&mut f, 9, url_safe);
    wasm!(f, {
            local_set(9);
            local_get(3); i32_const(6); i32_add; local_get(5); i32_add;
            local_get(9);
            i32_store8(0);
    });
    if !url_safe {
        wasm!(f, {
            local_get(3); i32_const(7); i32_add; local_get(5); i32_add;
            i32_const(61); i32_store8(0);
        });
    }
    wasm!(f, {
        end;
        local_get(3);
        end; // end function
    });

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// Emit code that consumes nothing and pushes the alphabet character for
/// `idx` (read from local `idx_local`). Branchy: select range with if/else.
fn emit_b64_alphabet_lookup(f: &mut Function, idx_local: u32, url_safe: bool) {
    let plus_or_dash: i32 = if url_safe { 45 } else { 43 };   // '-' or '+'
    let slash_or_under: i32 = if url_safe { 95 } else { 47 }; // '_' or '/'
    wasm!(f, {
        local_get(idx_local); i32_const(26); i32_lt_u;
        if_i32;
            i32_const(65); local_get(idx_local); i32_add; // 'A'+i
        else_;
            local_get(idx_local); i32_const(52); i32_lt_u;
            if_i32;
                i32_const(71); local_get(idx_local); i32_add; // 'a'-26 = 71
            else_;
                local_get(idx_local); i32_const(62); i32_lt_u;
                if_i32;
                    i32_const(-4); local_get(idx_local); i32_add; // '0'-52 = -4 (256-52=204 mod 256)
                else_;
                    local_get(idx_local); i32_const(62); i32_eq;
                    if_i32;
                        i32_const(plus_or_dash);
                    else_;
                        i32_const(slash_or_under);
                    end;
                end;
            end;
        end;
    });
}

/// __base64_decode(str_ptr) -> Result[Bytes, String] cell ptr.
/// Returns ok with Bytes ptr at offset 4 on success, err with err-string otherwise.
pub(super) fn compile_base64_decode(emitter: &mut WasmEmitter, _url_safe: bool) {
    let func_id = if _url_safe { emitter.rt.base64_decode_url } else { emitter.rt.base64_decode };
    let type_idx = emitter.func_type_indices[&func_id];
    let err_str = emitter.intern_string("invalid base64");
    // params: 0 = str_ptr
    // locals:
    //   1 = str_len, 2 = end (last non-pad index), 3 = out_ptr,
    //   4 = i, 5 = j (out idx), 6 = a, 7 = b, 8 = c, 9 = d, 10 = result_ptr
    let mut f = Function::new([
        (1, ValType::I32),  // 1: str_len
        (1, ValType::I32),  // 2: end
        (1, ValType::I32),  // 3: out_ptr
        (1, ValType::I32),  // 4: i
        (1, ValType::I32),  // 5: j
        (1, ValType::I32),  // 6: a
        (1, ValType::I32),  // 7: b
        (1, ValType::I32),  // 8: c
        (1, ValType::I32),  // 9: d
        (1, ValType::I32),  // 10: result_ptr
        (1, ValType::I32),  // 11: scratch (used by char→nibble helper)
    ]);

    wasm!(f, {
        local_get(0); i32_load(0); local_set(1);
        local_get(1); local_set(2);
        // Strip trailing '='
        block_empty; loop_empty;
            local_get(2); i32_eqz; br_if(1);
            local_get(0); i32_const(3); i32_add; local_get(2); i32_add;
            i32_load8_u(0); i32_const(61); i32_ne; br_if(1);
            local_get(2); i32_const(1); i32_sub; local_set(2);
            br(0);
        end; end;
        // alloc output: max possible = end * 3 / 4
        local_get(2); i32_const(3); i32_mul; i32_const(4); i32_div_u;
        i32_const(4); i32_add;
        call(emitter.rt.alloc); local_set(3);
        i32_const(0); local_set(4);
        i32_const(0); local_set(5);
    });

    // Main 4-char loop
    wasm!(f, {
        block_empty; loop_empty;
            local_get(4); i32_const(4); i32_add; local_get(2); i32_gt_u; br_if(1);
            local_get(0); i32_const(4); i32_add; local_get(4); i32_add; i32_load8_u(0); local_set(6);
            local_get(0); i32_const(5); i32_add; local_get(4); i32_add; i32_load8_u(0); local_set(7);
            local_get(0); i32_const(6); i32_add; local_get(4); i32_add; i32_load8_u(0); local_set(8);
            local_get(0); i32_const(7); i32_add; local_get(4); i32_add; i32_load8_u(0); local_set(9);
            local_get(6);
    });
    emit_b64_decode_char(&mut f);
    wasm!(f, {
            local_set(6);
            local_get(7);
    });
    emit_b64_decode_char(&mut f);
    wasm!(f, {
            local_set(7);
            local_get(8);
    });
    emit_b64_decode_char(&mut f);
    wasm!(f, {
            local_set(8);
            local_get(9);
    });
    emit_b64_decode_char(&mut f);
    wasm!(f, {
            local_set(9);
            // any of a,b,c,d == 255 → invalid
            local_get(6); i32_const(255); i32_eq;
            local_get(7); i32_const(255); i32_eq; i32_or;
            local_get(8); i32_const(255); i32_eq; i32_or;
            local_get(9); i32_const(255); i32_eq; i32_or;
            if_empty;
              // err
              i32_const(8); call(emitter.rt.alloc); local_set(10);
              local_get(10); i32_const(1); i32_store(0);
              local_get(10); i32_const(err_str as i32); i32_store(4);
              local_get(10); return_;
            end;
            // decode 3 bytes
            local_get(3); i32_const(4); i32_add; local_get(5); i32_add;
            local_get(6); i32_const(2); i32_shl; local_get(7); i32_const(4); i32_shr_u; i32_or;
            i32_store8(0);
            local_get(3); i32_const(5); i32_add; local_get(5); i32_add;
            local_get(7); i32_const(15); i32_and; i32_const(4); i32_shl;
            local_get(8); i32_const(2); i32_shr_u; i32_or;
            i32_store8(0);
            local_get(3); i32_const(6); i32_add; local_get(5); i32_add;
            local_get(8); i32_const(3); i32_and; i32_const(6); i32_shl;
            local_get(9); i32_or;
            i32_store8(0);
            local_get(4); i32_const(4); i32_add; local_set(4);
            local_get(5); i32_const(3); i32_add; local_set(5);
            br(0);
        end; end;
    });

    // Tail: rem == 2 → 1 byte, rem == 3 → 2 bytes, rem == 0 → done, rem == 1 → invalid
    wasm!(f, {
        local_get(2); local_get(4); i32_sub;
        i32_const(2); i32_eq;
        if_empty;
            local_get(0); i32_const(4); i32_add; local_get(4); i32_add; i32_load8_u(0);
    });
    emit_b64_decode_char(&mut f);
    wasm!(f, {
            local_set(6);
            local_get(0); i32_const(5); i32_add; local_get(4); i32_add; i32_load8_u(0);
    });
    emit_b64_decode_char(&mut f);
    wasm!(f, {
            local_set(7);
            local_get(6); i32_const(255); i32_eq;
            local_get(7); i32_const(255); i32_eq; i32_or;
            if_empty;
              i32_const(8); call(emitter.rt.alloc); local_set(10);
              local_get(10); i32_const(1); i32_store(0);
              local_get(10); i32_const(err_str as i32); i32_store(4);
              local_get(10); return_;
            end;
            local_get(3); i32_const(4); i32_add; local_get(5); i32_add;
            local_get(6); i32_const(2); i32_shl; local_get(7); i32_const(4); i32_shr_u; i32_or;
            i32_store8(0);
            local_get(5); i32_const(1); i32_add; local_set(5);
        end;
        local_get(2); local_get(4); i32_sub;
        i32_const(3); i32_eq;
        if_empty;
            local_get(0); i32_const(4); i32_add; local_get(4); i32_add; i32_load8_u(0);
    });
    emit_b64_decode_char(&mut f);
    wasm!(f, {
            local_set(6);
            local_get(0); i32_const(5); i32_add; local_get(4); i32_add; i32_load8_u(0);
    });
    emit_b64_decode_char(&mut f);
    wasm!(f, {
            local_set(7);
            local_get(0); i32_const(6); i32_add; local_get(4); i32_add; i32_load8_u(0);
    });
    emit_b64_decode_char(&mut f);
    wasm!(f, {
            local_set(8);
            local_get(6); i32_const(255); i32_eq;
            local_get(7); i32_const(255); i32_eq; i32_or;
            local_get(8); i32_const(255); i32_eq; i32_or;
            if_empty;
              i32_const(8); call(emitter.rt.alloc); local_set(10);
              local_get(10); i32_const(1); i32_store(0);
              local_get(10); i32_const(err_str as i32); i32_store(4);
              local_get(10); return_;
            end;
            local_get(3); i32_const(4); i32_add; local_get(5); i32_add;
            local_get(6); i32_const(2); i32_shl; local_get(7); i32_const(4); i32_shr_u; i32_or;
            i32_store8(0);
            local_get(3); i32_const(5); i32_add; local_get(5); i32_add;
            local_get(7); i32_const(15); i32_and; i32_const(4); i32_shl;
            local_get(8); i32_const(2); i32_shr_u; i32_or;
            i32_store8(0);
            local_get(5); i32_const(2); i32_add; local_set(5);
        end;
        // Set actual output length on the bytes header
        local_get(3); local_get(5); i32_store(0);
        // Wrap in Result::ok
        i32_const(8); call(emitter.rt.alloc); local_set(10);
        local_get(10); i32_const(0); i32_store(0);
        local_get(10); local_get(3); i32_store(4);
        local_get(10);
        end; // end function
    });

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// Emit a base64 character → 0..63 lookup. Pops one i32 (char), pushes one i32
/// (decoded value, or 255 on invalid). Uses local 11 as scratch — the caller
/// must declare local 11 to be free.
fn emit_b64_decode_char(f: &mut Function) {
    wasm!(f, {
        local_tee(11);
        i32_const(65); i32_ge_u;
        local_get(11); i32_const(90); i32_le_u; i32_and;
        if_i32;
            local_get(11); i32_const(65); i32_sub;
        else_;
            local_get(11); i32_const(97); i32_ge_u;
            local_get(11); i32_const(122); i32_le_u; i32_and;
            if_i32;
                local_get(11); i32_const(71); i32_sub; // 97 - 26 = 71
            else_;
                local_get(11); i32_const(48); i32_ge_u;
                local_get(11); i32_const(57); i32_le_u; i32_and;
                if_i32;
                    local_get(11); i32_const(-4); i32_add; // 48 - 52 = -4
                else_;
                    local_get(11); i32_const(43); i32_eq;
                    local_get(11); i32_const(45); i32_eq; i32_or;
                    if_i32;
                        i32_const(62);
                    else_;
                        local_get(11); i32_const(47); i32_eq;
                        local_get(11); i32_const(95); i32_eq; i32_or;
                        if_i32;
                            i32_const(63);
                        else_;
                            i32_const(255);
                        end;
                    end;
                end;
            end;
        end;
    });
}

/// __hex_encode(buf_ptr) -> string_ptr.
pub(super) fn compile_hex_encode(emitter: &mut WasmEmitter, upper: bool) {
    let func_id = if upper { emitter.rt.hex_encode_upper } else { emitter.rt.hex_encode };
    let type_idx = emitter.func_type_indices[&func_id];
    let alpha_offset: i32 = if upper { 55 } else { 87 }; // 'A'-10 = 55, 'a'-10 = 87
    let mut f = Function::new([
        (1, ValType::I32),  // 1: byte_len
        (1, ValType::I32),  // 2: out_len
        (1, ValType::I32),  // 3: out_ptr
        (1, ValType::I32),  // 4: i
        (1, ValType::I32),  // 5: b
        (1, ValType::I32),  // 6: nibble
    ]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(1);
        local_get(1); i32_const(2); i32_mul; local_set(2);
        local_get(2); i32_const(4); i32_add; call(emitter.rt.alloc); local_set(3);
        local_get(3); local_get(2); i32_store(0);
        i32_const(0); local_set(4);
        block_empty; loop_empty;
            local_get(4); local_get(1); i32_ge_u; br_if(1);
            local_get(0); i32_const(4); i32_add; local_get(4); i32_add; i32_load8_u(0); local_set(5);
            // hi nibble
            local_get(5); i32_const(4); i32_shr_u; local_set(6);
    });
    emit_hex_nibble_to_char(&mut f, 6, alpha_offset);
    wasm!(f, {
            local_set(6); // reuse 6 as char
            local_get(3); i32_const(4); i32_add; local_get(4); i32_const(2); i32_mul; i32_add;
            local_get(6);
            i32_store8(0);
            // lo nibble
            local_get(5); i32_const(15); i32_and; local_set(6);
    });
    emit_hex_nibble_to_char(&mut f, 6, alpha_offset);
    wasm!(f, {
            local_set(6);
            local_get(3); i32_const(5); i32_add; local_get(4); i32_const(2); i32_mul; i32_add;
            local_get(6);
            i32_store8(0);
            local_get(4); i32_const(1); i32_add; local_set(4);
            br(0);
        end; end;
        local_get(3);
        end;
    });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

fn emit_hex_nibble_to_char(f: &mut Function, nibble_local: u32, alpha_offset: i32) {
    wasm!(f, {
        local_get(nibble_local); i32_const(10); i32_lt_u;
        if_i32;
            i32_const(48); local_get(nibble_local); i32_add;
        else_;
            i32_const(alpha_offset); local_get(nibble_local); i32_add;
        end;
    });
}

/// __hex_decode(str_ptr) -> Result[Bytes, String] cell ptr.
pub(super) fn compile_hex_decode(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.hex_decode];
    let err_odd = emitter.intern_string("hex string has odd length");
    let err_char = emitter.intern_string("invalid hex char");
    let mut f = Function::new([
        (1, ValType::I32),  // 1: str_len
        (1, ValType::I32),  // 2: byte_len
        (1, ValType::I32),  // 3: out_ptr
        (1, ValType::I32),  // 4: i
        (1, ValType::I32),  // 5: hi
        (1, ValType::I32),  // 6: lo
        (1, ValType::I32),  // 7: result_ptr
        (1, ValType::I32),  // 8: scratch (used by char→nibble helper)
    ]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(1);
        // odd length → err
        local_get(1); i32_const(1); i32_and;
        if_empty;
            i32_const(8); call(emitter.rt.alloc); local_set(7);
            local_get(7); i32_const(1); i32_store(0);
            local_get(7); i32_const(err_odd as i32); i32_store(4);
            local_get(7); return_;
        end;
        local_get(1); i32_const(1); i32_shr_u; local_set(2);
        local_get(2); i32_const(4); i32_add; call(emitter.rt.alloc); local_set(3);
        local_get(3); local_get(2); i32_store(0);
        i32_const(0); local_set(4);
        block_empty; loop_empty;
            local_get(4); local_get(2); i32_ge_u; br_if(1);
            local_get(0); i32_const(4); i32_add; local_get(4); i32_const(2); i32_mul; i32_add;
            i32_load8_u(0);
    });
    emit_hex_char_to_nibble(&mut f);
    wasm!(f, {
            local_set(5);
            local_get(0); i32_const(5); i32_add; local_get(4); i32_const(2); i32_mul; i32_add;
            i32_load8_u(0);
    });
    emit_hex_char_to_nibble(&mut f);
    wasm!(f, {
            local_set(6);
            local_get(5); i32_const(255); i32_eq;
            local_get(6); i32_const(255); i32_eq; i32_or;
            if_empty;
                i32_const(8); call(emitter.rt.alloc); local_set(7);
                local_get(7); i32_const(1); i32_store(0);
                local_get(7); i32_const(err_char as i32); i32_store(4);
                local_get(7); return_;
            end;
            local_get(3); i32_const(4); i32_add; local_get(4); i32_add;
            local_get(5); i32_const(4); i32_shl; local_get(6); i32_or;
            i32_store8(0);
            local_get(4); i32_const(1); i32_add; local_set(4);
            br(0);
        end; end;
        i32_const(8); call(emitter.rt.alloc); local_set(7);
        local_get(7); i32_const(0); i32_store(0);
        local_get(7); local_get(3); i32_store(4);
        local_get(7);
        end;
    });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

fn emit_hex_char_to_nibble(f: &mut Function) {
    // Pops i32 (char), pushes i32 (0..15 or 255). Uses local 8 as scratch.
    wasm!(f, {
        local_tee(8);
        i32_const(48); i32_ge_u;
        local_get(8); i32_const(57); i32_le_u; i32_and;
        if_i32;
            local_get(8); i32_const(48); i32_sub;
        else_;
            local_get(8); i32_const(97); i32_ge_u;
            local_get(8); i32_const(102); i32_le_u; i32_and;
            if_i32;
                local_get(8); i32_const(87); i32_sub; // 'a' - 10
            else_;
                local_get(8); i32_const(65); i32_ge_u;
                local_get(8); i32_const(70); i32_le_u; i32_and;
                if_i32;
                    local_get(8); i32_const(55); i32_sub; // 'A' - 10
                else_;
                    i32_const(255);
                end;
            end;
        end;
    });
}
