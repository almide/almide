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
use wasm_encoder::{ValType};
use super::TrackedFunction as Function;
use super::rt_string::{string_data_off, string_hdr, string_cap_off};

/// ASCII '=' — Base64 padding byte (RFC 4648).
const B64_PAD: i32 = 61;

// ── Encoding constants ────────────────────────────────────────────────────────

/// Number of raw bytes in one base64 group (3 bytes → 4 chars).
const B64_GROUP_BYTES: i32 = 3;
/// Number of base64 characters per group (3 bytes → 4 chars).
const B64_GROUP_CHARS: i32 = 4;
/// Bit shift to extract the uppermost 6-bit sextet: `b >> 2`.
const B64_UPPER_SHIFT: i32 = 2;
/// Bit shift for the cross-byte middle join in base64 packing: 4-bit boundary.
const B64_MID_SHIFT: i32 = 4;
/// Bit shift for the lower 6-bit sextet of a 3-byte group: `b >> 6`.
const B64_LOWER_SHIFT: i32 = 6;
/// Bitmask for the lower 2 bits of a byte (`0b11`), used in base64 bit extraction.
const B64_LOW2_MASK: i32 = 3;
/// Bitmask for the lower 4 bits of a byte (`0xF`), used in base64 bit extraction.
const B64_LOW4_MASK: i32 = 15;
/// Bitmask for a full 6-bit sextet (`0x3F`), applied to the last byte of a group.
const B64_6BIT_MASK: i32 = 63;
/// Alphabet index where the numeric digits ('0'..'9') begin in the base64 table.
const B64_DIGIT_START_IDX: i32 = 52;
/// Arithmetic offset for digit indices 52..61: `'0' - 52 = -4`.
const B64_DIGIT_OFFSET: i32 = -4;
/// Number of uppercase letters A-Z represented in the base64 alphabet.
const B64_UPPERCASE_COUNT: i32 = 26;
/// Alphabet index of the '+' / '-' character (std / url-safe base64).
const B64_IDX_PLUS_OR_DASH: i32 = 62;
/// Offset used when encoding/decoding lowercase base64 letters: `'a' - 26 = 71`.
const B64_LOWER_ALPHA_OFFSET: i32 = 71;
/// Sentinel value returned by the char-to-value helpers for an invalid character.
const INVALID_CHAR_SENTINEL: i32 = 255;
/// Byte size of a Result cell: `[tag:i32][value:i32]` = 8 bytes.
const RESULT_CELL_BYTES: i32 = 8;

// ── ASCII character codes ─────────────────────────────────────────────────────

/// ASCII code of 'A' (0x41 = 65).
const ASCII_UPPER_A: i32 = 65;
/// ASCII code of 'Z' (0x5A = 90).
const ASCII_UPPER_Z: i32 = 90;
/// ASCII code of 'F' (0x46 = 70), upper bound of uppercase hex digits.
const ASCII_UPPER_F: i32 = 70;
/// ASCII code of 'a' (0x61 = 97).
const ASCII_LOWER_A: i32 = 97;
/// ASCII code of 'z' (0x7A = 122).
const ASCII_LOWER_Z: i32 = 122;
/// ASCII code of 'f' (0x66 = 102), upper bound of lowercase hex digits.
const ASCII_LOWER_F: i32 = 102;
/// ASCII code of '0' (0x30 = 48).
const ASCII_ZERO: i32 = 48;
/// ASCII code of '9' (0x39 = 57).
const ASCII_NINE: i32 = 57;
/// ASCII code of '+' (0x2B = 43), standard base64 char at index 62.
const ASCII_PLUS: i32 = 43;
/// ASCII code of '-' (0x2D = 45), url-safe base64 char at index 62.
const ASCII_MINUS: i32 = 45;
/// ASCII code of '/' (0x2F = 47), standard base64 char at index 63.
const ASCII_SLASH: i32 = 47;
/// ASCII code of '_' (0x5F = 95), url-safe base64 char at index 63.
const ASCII_UNDERSCORE: i32 = 95;

// ── Hex encoding constants ────────────────────────────────────────────────────

/// Number of hex characters per encoded byte (2 nibbles).
const HEX_CHARS_PER_BYTE: i32 = 2;
/// Bit shift to isolate the high nibble of a byte: `b >> 4`.
const HEX_NIBBLE_SHIFT: i32 = 4;
/// Bitmask to isolate the low nibble of a byte (`0xF`).
const HEX_LOW_NIBBLE_MASK: i32 = 15;
/// Nibble values 0-9 map to ASCII digits; 10-15 use a letter offset.
const HEX_DIGIT_THRESHOLD: i32 = 10;
/// Offset for lowercase hex letter encoding: `'a' - 10 = 87`.
const HEX_LOWER_A_OFFSET: i32 = 87;
/// Offset for uppercase hex letter encoding: `'A' - 10 = 55`.
const HEX_UPPER_A_OFFSET: i32 = 55;

/// __base64_encode(buf_ptr) -> string_ptr.
///
/// Mirrors the native oracle `runtime/rs/src/base64.rs::encode_with`: both the
/// standard and URL-safe alphabets emit the CANONICAL padded form (output is
/// always a multiple of 4, with '=' padding). `url_safe` only swaps the 62/63
/// symbols ('+'/'/' → '-'/'_'); padding is identical.
pub(super) fn compile_base64_encode(emitter: &mut WasmEmitter, url_safe: bool) {
    let func_id = if url_safe { emitter.rt.base64_encode_url } else { emitter.rt.base64_encode };
    let type_idx = emitter.func_type_indices[&func_id];
    // Output character N of a 4-char group lands at `out_ptr + data_off + j + N`.
    // The previous code hard-coded `7` for slot 3 (a stale `data_off + 3` that
    // assumed data_off == 4); deriving every slot from data_off keeps the four
    // store offsets correct for any string layout.
    let data_off = string_data_off();
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

    // groups of 3 → 4 chars, padded: out_len = ceil(byte_len / 3) * 4.
    wasm!(f, {
        local_get(1); i32_const(B64_GROUP_BYTES - 1); i32_add;
        i32_const(B64_GROUP_BYTES); i32_div_u;
        i32_const(B64_GROUP_CHARS); i32_mul;
        local_set(2);
    });

    // out_ptr = string_alloc(out_len): writes the len AND cap header fields
    // (the previous code called raw alloc and left len=0 → empty output).
    wasm!(f, {
        local_get(2); call(emitter.rt.string_alloc); local_set(3);
        i32_const(0); local_set(4); // i = 0
        i32_const(0); local_set(5); // j = 0
    });

    // Main loop: process 3 input bytes → 4 output chars
    wasm!(f, {
        block_empty; loop_empty;
            local_get(4); i32_const(B64_GROUP_BYTES); i32_add; local_get(1); i32_gt_u; br_if(1);
            // Load 3 bytes b0, b1, b2
            local_get(0); i32_const(data_off); i32_add; local_get(4); i32_add; i32_load8_u(0); local_set(6);
            local_get(0); i32_const(data_off + 1); i32_add; local_get(4); i32_add; i32_load8_u(0); local_set(7);
            local_get(0); i32_const(data_off + 2); i32_add; local_get(4); i32_add; i32_load8_u(0); local_set(8);
            // c0 = alphabet[b0 >> 2]
            local_get(6); i32_const(B64_UPPER_SHIFT); i32_shr_u; local_set(9);
    });
    emit_b64_alphabet_lookup(&mut f, 9, url_safe);
    wasm!(f, {
            local_set(9);
            local_get(3); i32_const(data_off); i32_add; local_get(5); i32_add;
            local_get(9);
            i32_store8(0);
            // c1 = alphabet[((b0 & 3) << 4) | (b1 >> 4)]
            local_get(6); i32_const(B64_LOW2_MASK); i32_and; i32_const(B64_MID_SHIFT); i32_shl;
            local_get(7); i32_const(B64_MID_SHIFT); i32_shr_u;
            i32_or; local_set(9);
    });
    emit_b64_alphabet_lookup(&mut f, 9, url_safe);
    wasm!(f, {
            local_set(9);
            local_get(3); i32_const(data_off + 1); i32_add; local_get(5); i32_add;
            local_get(9);
            i32_store8(0);
            // c2 = alphabet[((b1 & 0xF) << 2) | (b2 >> 6)]
            local_get(7); i32_const(B64_LOW4_MASK); i32_and; i32_const(B64_UPPER_SHIFT); i32_shl;
            local_get(8); i32_const(B64_LOWER_SHIFT); i32_shr_u;
            i32_or; local_set(9);
    });
    emit_b64_alphabet_lookup(&mut f, 9, url_safe);
    wasm!(f, {
            local_set(9);
            local_get(3); i32_const(data_off + 2); i32_add; local_get(5); i32_add;
            local_get(9);
            i32_store8(0);
            // c3 = alphabet[b2 & 0x3F]
            local_get(8); i32_const(B64_6BIT_MASK); i32_and; local_set(9);
    });
    emit_b64_alphabet_lookup(&mut f, 9, url_safe);
    wasm!(f, {
            local_set(9);
            local_get(3); i32_const(data_off + 3); i32_add; local_get(5); i32_add;
            local_get(9);
            i32_store8(0);
            local_get(4); i32_const(B64_GROUP_BYTES); i32_add; local_set(4);
            local_get(5); i32_const(B64_GROUP_CHARS); i32_add; local_set(5);
            br(0);
        end; end;
    });

    // Tail: handle rem 1 or 2 bytes
    wasm!(f, {
        local_get(1); local_get(4); i32_sub; // rem
        local_set(9); // reuse local 9 as rem
        local_get(9); i32_const(1); i32_eq;
        if_empty;
            // 1 byte: 2 output chars + 2 '=' padding
            local_get(0); i32_const(data_off); i32_add; local_get(4); i32_add; i32_load8_u(0); local_set(6);
            local_get(6); i32_const(B64_UPPER_SHIFT); i32_shr_u; local_set(9);
    });
    emit_b64_alphabet_lookup(&mut f, 9, url_safe);
    wasm!(f, {
            local_set(9);
            local_get(3); i32_const(data_off); i32_add; local_get(5); i32_add;
            local_get(9);
            i32_store8(0);
            local_get(6); i32_const(B64_LOW2_MASK); i32_and; i32_const(B64_MID_SHIFT); i32_shl; local_set(9);
    });
    emit_b64_alphabet_lookup(&mut f, 9, url_safe);
    wasm!(f, {
            local_set(9);
            local_get(3); i32_const(data_off + 1); i32_add; local_get(5); i32_add;
            local_get(9);
            i32_store8(0);
            // padding '=' '='
            local_get(3); i32_const(data_off + 2); i32_add; local_get(5); i32_add;
            i32_const(B64_PAD); i32_store8(0);
            local_get(3); i32_const(data_off + 3); i32_add; local_get(5); i32_add;
            i32_const(B64_PAD); i32_store8(0);
        end;
        local_get(1); local_get(4); i32_sub;
        i32_const(B64_GROUP_BYTES - 1); i32_eq;
        if_empty;
            // 2 bytes: 3 output chars + 1 '=' padding
            local_get(0); i32_const(data_off); i32_add; local_get(4); i32_add; i32_load8_u(0); local_set(6);
            local_get(0); i32_const(data_off + 1); i32_add; local_get(4); i32_add; i32_load8_u(0); local_set(7);
            local_get(6); i32_const(B64_UPPER_SHIFT); i32_shr_u; local_set(9);
    });
    emit_b64_alphabet_lookup(&mut f, 9, url_safe);
    wasm!(f, {
            local_set(9);
            local_get(3); i32_const(data_off); i32_add; local_get(5); i32_add;
            local_get(9);
            i32_store8(0);
            local_get(6); i32_const(B64_LOW2_MASK); i32_and; i32_const(B64_MID_SHIFT); i32_shl;
            local_get(7); i32_const(B64_MID_SHIFT); i32_shr_u;
            i32_or; local_set(9);
    });
    emit_b64_alphabet_lookup(&mut f, 9, url_safe);
    wasm!(f, {
            local_set(9);
            local_get(3); i32_const(data_off + 1); i32_add; local_get(5); i32_add;
            local_get(9);
            i32_store8(0);
            local_get(7); i32_const(B64_LOW4_MASK); i32_and; i32_const(B64_UPPER_SHIFT); i32_shl; local_set(9);
    });
    emit_b64_alphabet_lookup(&mut f, 9, url_safe);
    wasm!(f, {
            local_set(9);
            local_get(3); i32_const(data_off + 2); i32_add; local_get(5); i32_add;
            local_get(9);
            i32_store8(0);
            local_get(3); i32_const(data_off + 3); i32_add; local_get(5); i32_add;
            i32_const(B64_PAD); i32_store8(0);
        end;
        local_get(3);
        end; // end function
    });

    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

/// Emit code that consumes nothing and pushes the alphabet character for
/// `idx` (read from local `idx_local`). Branchy: select range with if/else.
fn emit_b64_alphabet_lookup(f: &mut Function, idx_local: u32, url_safe: bool) {
    let plus_or_dash: i32 = if url_safe { 45 } else { 43 };   // '-' or '+'
    let slash_or_under: i32 = if url_safe { 95 } else { 47 }; // '_' or '/'
    wasm!(f, {
        local_get(idx_local); i32_const(B64_UPPERCASE_COUNT); i32_lt_u;
        if_i32;
            i32_const(ASCII_UPPER_A); local_get(idx_local); i32_add; // 'A'+i
        else_;
            local_get(idx_local); i32_const(B64_DIGIT_START_IDX); i32_lt_u;
            if_i32;
                i32_const(B64_LOWER_ALPHA_OFFSET); local_get(idx_local); i32_add; // 'a'-26 = 71
            else_;
                local_get(idx_local); i32_const(B64_IDX_PLUS_OR_DASH); i32_lt_u;
                if_i32;
                    i32_const(B64_DIGIT_OFFSET); local_get(idx_local); i32_add; // '0'-52 = -4 (256-52=204 mod 256)
                else_;
                    local_get(idx_local); i32_const(B64_IDX_PLUS_OR_DASH); i32_eq;
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
///
/// Mirrors the native oracle `runtime/rs/src/base64.rs::decode_str`:
///   - strip trailing '=' padding,
///   - reject `body_len % 4 == 1` with "invalid base64 length: <orig_len>",
///   - reject any non-alphabet char (a stray '=' in the body included) with
///     the constant "invalid base64 character".
/// Returns ok(Bytes) at offset 4 on success, err(String) otherwise. The
/// Result cell is `[tag:i32][value:i32]` = 8 bytes (tag=0 ok, tag=1 err).
pub(super) fn compile_base64_decode(emitter: &mut WasmEmitter, _url_safe: bool) {
    let func_id = if _url_safe { emitter.rt.base64_decode_url } else { emitter.rt.base64_decode };
    let type_idx = emitter.func_type_indices[&func_id];
    let err_char = emitter.intern_string("invalid base64 character");
    let err_len_prefix = emitter.intern_string("invalid base64 length: ");
    // Input char `i` lives at `str_ptr + data_off + i`; output byte `j` at
    // `out_ptr + data_off + j`. Deriving every load/store from data_off
    // replaces the stale `3`/`7` literals (which assumed data_off == 4).
    let data_off = string_data_off();
    // Sentinel for an invalid char produced by the char→nibble helper.
    let invalid: i32 = 255;
    let itoa = emitter.rt.int_to_string;
    let concat = emitter.rt.concat_str;
    // params: 0 = str_ptr
    // locals:
    //   1 = str_len, 2 = end (body length after '=' strip), 3 = out_ptr,
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

    // Emit an `err(<interned constant>)` return.
    let emit_err_const = |f: &mut Function, err_ptr: u32| {
        wasm!(f, {
            i32_const(RESULT_CELL_BYTES); call(emitter.rt.alloc); local_set(10);
            local_get(10); i32_const(1); i32_store(0);
            local_get(10); i32_const(err_ptr as i32); i32_store(4);
            local_get(10); return_;
        });
    };
    // Emit `if (a|b|... == invalid) return err(char)`.
    let emit_invalid_guard = |f: &mut Function, slots: &[u32]| {
        for (k, &slot) in slots.iter().enumerate() {
            wasm!(f, { local_get(slot); i32_const(invalid); i32_eq; });
            if k > 0 { wasm!(f, { i32_or; }); }
        }
        wasm!(f, { if_empty; });
        emit_err_const(f, err_char);
        wasm!(f, { end; });
    };

    wasm!(f, {
        local_get(0); i32_load(0); local_set(1);
        local_get(1); local_set(2);
        // Strip trailing '=': last body char is at str_ptr + data_off + (end-1).
        block_empty; loop_empty;
            local_get(2); i32_eqz; br_if(1);
            local_get(0); i32_const(data_off); i32_add; local_get(2); i32_add; i32_const(1); i32_sub;
            i32_load8_u(0); i32_const(B64_PAD); i32_ne; br_if(1);
            local_get(2); i32_const(1); i32_sub; local_set(2);
            br(0);
        end; end;
    });

    // body_len % 4 == 1 → "invalid base64 length: <orig_len>"
    wasm!(f, {
        local_get(2); i32_const(B64_GROUP_CHARS - 1); i32_and; i32_const(1); i32_eq;
        if_empty;
          // msg = "invalid base64 length: " + int_to_string(str_len)
          i32_const(err_len_prefix as i32);
          local_get(1); i64_extend_i32_u; call(itoa);
          call(concat); local_set(11);
          i32_const(RESULT_CELL_BYTES); call(emitter.rt.alloc); local_set(10);
          local_get(10); i32_const(1); i32_store(0);
          local_get(10); local_get(11); i32_store(4);
          local_get(10); return_;
        end;
    });

    wasm!(f, {
        // alloc output: max possible = body_len * 3 / 4
        local_get(2); i32_const(B64_GROUP_BYTES); i32_mul; i32_const(B64_GROUP_CHARS); i32_div_u;
        i32_const(string_hdr()); i32_add;
        call(emitter.rt.alloc); local_set(3);
        i32_const(0); local_set(4);
        i32_const(0); local_set(5);
    });

    // Main 4-char loop
    wasm!(f, {
        block_empty; loop_empty;
            local_get(4); i32_const(B64_GROUP_CHARS); i32_add; local_get(2); i32_gt_u; br_if(1);
            local_get(0); i32_const(data_off); i32_add; local_get(4); i32_add; i32_load8_u(0); local_set(6);
            local_get(0); i32_const(data_off + 1); i32_add; local_get(4); i32_add; i32_load8_u(0); local_set(7);
            local_get(0); i32_const(data_off + 2); i32_add; local_get(4); i32_add; i32_load8_u(0); local_set(8);
            local_get(0); i32_const(data_off + 3); i32_add; local_get(4); i32_add; i32_load8_u(0); local_set(9);
            local_get(6);
    });
    emit_b64_decode_char(&mut f);
    wasm!(f, { local_set(6); local_get(7); });
    emit_b64_decode_char(&mut f);
    wasm!(f, { local_set(7); local_get(8); });
    emit_b64_decode_char(&mut f);
    wasm!(f, { local_set(8); local_get(9); });
    emit_b64_decode_char(&mut f);
    wasm!(f, { local_set(9); });
    emit_invalid_guard(&mut f, &[6, 7, 8, 9]);
    wasm!(f, {
            // decode 3 bytes
            local_get(3); i32_const(data_off); i32_add; local_get(5); i32_add;
            local_get(6); i32_const(B64_UPPER_SHIFT); i32_shl; local_get(7); i32_const(B64_MID_SHIFT); i32_shr_u; i32_or;
            i32_store8(0);
            local_get(3); i32_const(data_off + 1); i32_add; local_get(5); i32_add;
            local_get(7); i32_const(B64_LOW4_MASK); i32_and; i32_const(B64_MID_SHIFT); i32_shl;
            local_get(8); i32_const(B64_UPPER_SHIFT); i32_shr_u; i32_or;
            i32_store8(0);
            local_get(3); i32_const(data_off + 2); i32_add; local_get(5); i32_add;
            local_get(8); i32_const(B64_LOW2_MASK); i32_and; i32_const(B64_LOWER_SHIFT); i32_shl;
            local_get(9); i32_or;
            i32_store8(0);
            local_get(4); i32_const(B64_GROUP_CHARS); i32_add; local_set(4);
            local_get(5); i32_const(B64_GROUP_BYTES); i32_add; local_set(5);
            br(0);
        end; end;
    });

    // Tail: rem == 2 → 1 byte, rem == 3 → 2 bytes, rem == 0 → done.
    // (rem == 1 was rejected by the length check above.)
    wasm!(f, {
        local_get(2); local_get(4); i32_sub;
        i32_const(B64_GROUP_BYTES - 1); i32_eq;
        if_empty;
            local_get(0); i32_const(data_off); i32_add; local_get(4); i32_add; i32_load8_u(0);
    });
    emit_b64_decode_char(&mut f);
    wasm!(f, {
            local_set(6);
            local_get(0); i32_const(data_off + 1); i32_add; local_get(4); i32_add; i32_load8_u(0);
    });
    emit_b64_decode_char(&mut f);
    wasm!(f, { local_set(7); });
    emit_invalid_guard(&mut f, &[6, 7]);
    wasm!(f, {
            local_get(3); i32_const(data_off); i32_add; local_get(5); i32_add;
            local_get(6); i32_const(B64_UPPER_SHIFT); i32_shl; local_get(7); i32_const(B64_MID_SHIFT); i32_shr_u; i32_or;
            i32_store8(0);
            local_get(5); i32_const(1); i32_add; local_set(5);
        end;
        local_get(2); local_get(4); i32_sub;
        i32_const(B64_GROUP_BYTES); i32_eq;
        if_empty;
            local_get(0); i32_const(data_off); i32_add; local_get(4); i32_add; i32_load8_u(0);
    });
    emit_b64_decode_char(&mut f);
    wasm!(f, {
            local_set(6);
            local_get(0); i32_const(data_off + 1); i32_add; local_get(4); i32_add; i32_load8_u(0);
    });
    emit_b64_decode_char(&mut f);
    wasm!(f, {
            local_set(7);
            local_get(0); i32_const(data_off + 2); i32_add; local_get(4); i32_add; i32_load8_u(0);
    });
    emit_b64_decode_char(&mut f);
    wasm!(f, { local_set(8); });
    emit_invalid_guard(&mut f, &[6, 7, 8]);
    wasm!(f, {
            local_get(3); i32_const(data_off); i32_add; local_get(5); i32_add;
            local_get(6); i32_const(B64_UPPER_SHIFT); i32_shl; local_get(7); i32_const(B64_MID_SHIFT); i32_shr_u; i32_or;
            i32_store8(0);
            local_get(3); i32_const(data_off + 1); i32_add; local_get(5); i32_add;
            local_get(7); i32_const(B64_LOW4_MASK); i32_and; i32_const(B64_MID_SHIFT); i32_shl;
            local_get(8); i32_const(B64_UPPER_SHIFT); i32_shr_u; i32_or;
            i32_store8(0);
            local_get(5); i32_const(B64_GROUP_BYTES - 1); i32_add; local_set(5);
        end;
        // Set actual output length + cap on the bytes header
        local_get(3); local_get(5); i32_store(0);
        local_get(3); local_get(5); i32_store(string_cap_off() as u32);
        // Wrap in Result::ok
        i32_const(RESULT_CELL_BYTES); call(emitter.rt.alloc); local_set(10);
        local_get(10); i32_const(0); i32_store(0);
        local_get(10); local_get(3); i32_store(4);
        local_get(10);
        end; // end function
    });

    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

/// Emit a base64 character → 0..63 lookup. Pops one i32 (char), pushes one i32
/// (decoded value, or 255 on invalid). Uses local 11 as scratch — the caller
/// must declare local 11 to be free.
fn emit_b64_decode_char(f: &mut Function) {
    wasm!(f, {
        local_tee(11);
        i32_const(ASCII_UPPER_A); i32_ge_u;
        local_get(11); i32_const(ASCII_UPPER_Z); i32_le_u; i32_and;
        if_i32;
            local_get(11); i32_const(ASCII_UPPER_A); i32_sub;
        else_;
            local_get(11); i32_const(ASCII_LOWER_A); i32_ge_u;
            local_get(11); i32_const(ASCII_LOWER_Z); i32_le_u; i32_and;
            if_i32;
                local_get(11); i32_const(B64_LOWER_ALPHA_OFFSET); i32_sub; // 97 - 26 = 71
            else_;
                local_get(11); i32_const(ASCII_ZERO); i32_ge_u;
                local_get(11); i32_const(ASCII_NINE); i32_le_u; i32_and;
                if_i32;
                    // '0'..'9' (48..57) → values 52..61: value = char - 48 + 52 = char + 4.
                    local_get(11); i32_const(B64_MID_SHIFT); i32_add;
                else_;
                    local_get(11); i32_const(ASCII_PLUS); i32_eq;
                    local_get(11); i32_const(ASCII_MINUS); i32_eq; i32_or;
                    if_i32;
                        i32_const(B64_IDX_PLUS_OR_DASH);
                    else_;
                        local_get(11); i32_const(ASCII_SLASH); i32_eq;
                        local_get(11); i32_const(ASCII_UNDERSCORE); i32_eq; i32_or;
                        if_i32;
                            i32_const(B64_6BIT_MASK);
                        else_;
                            i32_const(INVALID_CHAR_SENTINEL);
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
        local_get(1); i32_const(HEX_CHARS_PER_BYTE); i32_mul; local_set(2);
        local_get(2); call(emitter.rt.string_alloc); local_set(3);
        // len+cap already written by string_alloc
        i32_const(0); local_set(4);
        block_empty; loop_empty;
            local_get(4); local_get(1); i32_ge_u; br_if(1);
            local_get(0); i32_const(string_data_off()); i32_add; local_get(4); i32_add; i32_load8_u(0); local_set(5);
            // hi nibble
            local_get(5); i32_const(HEX_NIBBLE_SHIFT); i32_shr_u; local_set(6);
    });
    emit_hex_nibble_to_char(&mut f, 6, alpha_offset);
    wasm!(f, {
            local_set(6); // reuse 6 as char
            local_get(3); i32_const(string_data_off()); i32_add; local_get(4); i32_const(HEX_CHARS_PER_BYTE); i32_mul; i32_add;
            local_get(6);
            i32_store8(0);
            // lo nibble
            local_get(5); i32_const(HEX_LOW_NIBBLE_MASK); i32_and; local_set(6);
    });
    emit_hex_nibble_to_char(&mut f, 6, alpha_offset);
    wasm!(f, {
            local_set(6);
            local_get(3); i32_const(string_data_off() + 1); i32_add; local_get(4); i32_const(HEX_CHARS_PER_BYTE); i32_mul; i32_add;
            local_get(6);
            i32_store8(0);
            local_get(4); i32_const(1); i32_add; local_set(4);
            br(0);
        end; end;
        local_get(3);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

fn emit_hex_nibble_to_char(f: &mut Function, nibble_local: u32, alpha_offset: i32) {
    wasm!(f, {
        local_get(nibble_local); i32_const(HEX_DIGIT_THRESHOLD); i32_lt_u;
        if_i32;
            i32_const(ASCII_ZERO); local_get(nibble_local); i32_add;
        else_;
            i32_const(alpha_offset); local_get(nibble_local); i32_add;
        end;
    });
}

/// __hex_decode(str_ptr) -> Result[Bytes, String] cell ptr.
///
/// Mirrors native `runtime/rs/src/hex.rs::almide_rt_hex_decode`, INCLUDING the
/// positional error detail the wasm impl used to drop:
///   - odd length → "hex string has odd length: <str_len>"
///   - bad nibble → "invalid hex char at <byte_pos>"   (hi at 2*pair, lo at 2*pair+1)
pub(super) fn compile_hex_decode(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.hex_decode];
    let err_odd_prefix = emitter.intern_string("hex string has odd length: ");
    let err_char_prefix = emitter.intern_string("invalid hex char at ");
    let invalid: i32 = 255;
    let data_off = string_data_off();
    let itoa = emitter.rt.int_to_string;
    let concat = emitter.rt.concat_str;
    let mut f = Function::new([
        (1, ValType::I32),  // 1: str_len
        (1, ValType::I32),  // 2: byte_len
        (1, ValType::I32),  // 3: out_ptr
        (1, ValType::I32),  // 4: i (byte-pair index)
        (1, ValType::I32),  // 5: hi
        (1, ValType::I32),  // 6: lo
        (1, ValType::I32),  // 7: result_ptr
        (1, ValType::I32),  // 8: scratch (used by char→nibble helper)
        (1, ValType::I32),  // 9: msg (built error string)
    ]);

    wasm!(f, {
        local_get(0); i32_load(0); local_set(1);
        // odd length → "hex string has odd length: <str_len>"
        local_get(1); i32_const(1); i32_and;
        if_empty;
            i32_const(err_odd_prefix as i32);
            local_get(1); i64_extend_i32_u; call(itoa);
            call(concat); local_set(9);
            i32_const(RESULT_CELL_BYTES); call(emitter.rt.alloc); local_set(7);
            local_get(7); i32_const(1); i32_store(0);
            local_get(7); local_get(9); i32_store(4);
            local_get(7); return_;
        end;
        local_get(1); i32_const(1); i32_shr_u; local_set(2);
        local_get(2); call(emitter.rt.string_alloc); local_set(3);
        // len+cap already written by string_alloc
        i32_const(0); local_set(4);
        block_empty; loop_empty;
            local_get(4); local_get(2); i32_ge_u; br_if(1);
            local_get(0); i32_const(data_off); i32_add; local_get(4); i32_const(HEX_CHARS_PER_BYTE); i32_mul; i32_add;
            i32_load8_u(0);
    });
    emit_hex_char_to_nibble(&mut f);
    wasm!(f, { local_set(5); });
    // hi invalid → "invalid hex char at <2*pair>"
    wasm!(f, {
            local_get(5); i32_const(invalid); i32_eq;
            if_empty;
                i32_const(err_char_prefix as i32);
                local_get(4); i32_const(HEX_CHARS_PER_BYTE); i32_mul; i64_extend_i32_u; call(itoa);
                call(concat); local_set(9);
                i32_const(RESULT_CELL_BYTES); call(emitter.rt.alloc); local_set(7);
                local_get(7); i32_const(1); i32_store(0);
                local_get(7); local_get(9); i32_store(4);
                local_get(7); return_;
            end;
            local_get(0); i32_const(data_off + 1); i32_add; local_get(4); i32_const(HEX_CHARS_PER_BYTE); i32_mul; i32_add;
            i32_load8_u(0);
    });
    emit_hex_char_to_nibble(&mut f);
    wasm!(f, { local_set(6); });
    // lo invalid → "invalid hex char at <2*pair + 1>"
    wasm!(f, {
            local_get(6); i32_const(invalid); i32_eq;
            if_empty;
                i32_const(err_char_prefix as i32);
                local_get(4); i32_const(HEX_CHARS_PER_BYTE); i32_mul; i32_const(1); i32_add; i64_extend_i32_u; call(itoa);
                call(concat); local_set(9);
                i32_const(RESULT_CELL_BYTES); call(emitter.rt.alloc); local_set(7);
                local_get(7); i32_const(1); i32_store(0);
                local_get(7); local_get(9); i32_store(4);
                local_get(7); return_;
            end;
            local_get(3); i32_const(data_off); i32_add; local_get(4); i32_add;
            local_get(5); i32_const(HEX_NIBBLE_SHIFT); i32_shl; local_get(6); i32_or;
            i32_store8(0);
            local_get(4); i32_const(1); i32_add; local_set(4);
            br(0);
        end; end;
        i32_const(RESULT_CELL_BYTES); call(emitter.rt.alloc); local_set(7);
        local_get(7); i32_const(0); i32_store(0);
        local_get(7); local_get(3); i32_store(4);
        local_get(7);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.hex_decode, type_idx, f));
}

fn emit_hex_char_to_nibble(f: &mut Function) {
    // Pops i32 (char), pushes i32 (0..15 or 255). Uses local 8 as scratch.
    wasm!(f, {
        local_tee(8);
        i32_const(ASCII_ZERO); i32_ge_u;
        local_get(8); i32_const(ASCII_NINE); i32_le_u; i32_and;
        if_i32;
            local_get(8); i32_const(ASCII_ZERO); i32_sub;
        else_;
            local_get(8); i32_const(ASCII_LOWER_A); i32_ge_u;
            local_get(8); i32_const(ASCII_LOWER_F); i32_le_u; i32_and;
            if_i32;
                local_get(8); i32_const(HEX_LOWER_A_OFFSET); i32_sub; // 'a' - 10
            else_;
                local_get(8); i32_const(ASCII_UPPER_A); i32_ge_u;
                local_get(8); i32_const(ASCII_UPPER_F); i32_le_u; i32_and;
                if_i32;
                    local_get(8); i32_const(HEX_UPPER_A_OFFSET); i32_sub; // 'A' - 10
                else_;
                    i32_const(INVALID_CHAR_SENTINEL);
                end;
            end;
        end;
    });
}
