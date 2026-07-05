//! WASM runtime for the Almide-literal repr of a string inside a container.
//!
//! Compound string interpolation (`"${[1, 2]}"`, `"${["a": 1]}"`, …) renders a
//! value back to its Almide-literal form. The structural walks (list / map / set
//! / tuple / option / result) are emitted INLINE at each interpolation site,
//! driven by the static `Ty` (see `calls_string::emit_repr_value`). The only
//! part that needs a real runtime function is escaping a string: it is a byte
//! loop whose output length is data-dependent, so it cannot be a fixed inline
//! sequence.
//!
//! `__repr_str(s) -> ptr` double-quotes a string and escapes exactly the set
//! used by the native `almide_repr_str` / `almide_rt_value_stringify`:
//!   `\\ \" \n \r \t`  (every other byte is copied verbatim).
//! Output (and input) use the standard `[len:i32][cap:i32][data:u8...]` layout.

use super::{CompiledFunc, WasmEmitter};
use wasm_encoder::ValType;
use super::TrackedFunction as Function;
use super::rt_string::{string_data_off, string_hdr, string_cap_off};

// Escapable input bytes and the character that follows the backslash in the
// 2-byte output sequence. Named so the two passes stay in lock-step (the count
// pass and the write pass must agree on exactly which bytes expand to 2 bytes).
const BYTE_BACKSLASH: i32 = 0x5C; // \  → \\
const BYTE_QUOTE: i32 = 0x22;     // "  → \"
const BYTE_NL: i32 = 0x0A;        // \n → \ n
const BYTE_CR: i32 = 0x0D;        // \r → \ r
const BYTE_TAB: i32 = 0x09;       // \t → \ t
const CHAR_N: i32 = b'n' as i32;
const CHAR_R: i32 = b'r' as i32;
const CHAR_T: i32 = b't' as i32;
const QUOTE_LEN: i32 = 2; // the two surrounding `"` characters

pub(super) fn register(emitter: &mut WasmEmitter) {
    // __repr_str(s: i32) -> i32
    let ty = emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
    emitter.rt.repr_str = emitter.register_func("__repr_str", ty);
}

/// __repr_str(s) -> ptr: `"` + escaped(s) + `"`.
pub(super) fn compile(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.repr_str];
    let alloc = emitter.rt.alloc;
    let data_off = string_data_off();
    let hdr = string_hdr();
    let cap_off = string_cap_off() as u32; // string header: [len@0][cap@cap_off]

    // param 0 = s (string ptr)
    // locals:
    //   1 = in_len   (input byte length)
    //   2 = out_len  (escaped byte length, excl. quotes during pass 1)
    //   3 = i        (input index)
    //   4 = b        (current byte)
    //   5 = out      (result string ptr)
    //   6 = w        (write cursor, absolute address into result data)
    //   7 = esc      (the post-backslash char for the current byte, 0 = none)
    let mut f = Function::new([
        (1, ValType::I32), // 1: in_len
        (1, ValType::I32), // 2: out_len
        (1, ValType::I32), // 3: i
        (1, ValType::I32), // 4: b
        (1, ValType::I32), // 5: out
        (1, ValType::I32), // 6: w
        (1, ValType::I32), // 7: esc
    ]);
    const S: u32 = 0;
    const IN_LEN: u32 = 1;
    const OUT_LEN: u32 = 2;
    const I: u32 = 3;
    const B: u32 = 4;
    const OUT: u32 = 5;
    const W: u32 = 6;
    const ESC: u32 = 7;

    wasm!(f, {
        local_get(S); i32_load(0); local_set(IN_LEN);
        // ── Pass 1: out_len = sum over bytes (escapable → 2, else → 1) ──
        i32_const(0); local_set(OUT_LEN);
        i32_const(0); local_set(I);
        block_empty; loop_empty;
          local_get(I); local_get(IN_LEN); i32_ge_u; br_if(1);
          local_get(S); i32_const(data_off); i32_add; local_get(I); i32_add;
          i32_load8_u(0); local_set(B);
          // escapable = b==\\ || b=='"' || b=='\n' || b=='\r' || b=='\t'
          local_get(OUT_LEN);
          local_get(B); i32_const(BYTE_BACKSLASH); i32_eq;
          local_get(B); i32_const(BYTE_QUOTE); i32_eq; i32_or;
          local_get(B); i32_const(BYTE_NL); i32_eq; i32_or;
          local_get(B); i32_const(BYTE_CR); i32_eq; i32_or;
          local_get(B); i32_const(BYTE_TAB); i32_eq; i32_or;
          // escapable ? 2 : 1  →  1 + escapable
          i32_const(1); i32_add;
          i32_add; local_set(OUT_LEN);
          local_get(I); i32_const(1); i32_add; local_set(I);
          br(0);
        end; end;
        // ── Allocate result: header + quotes + escaped bytes ──
        local_get(OUT_LEN); i32_const(QUOTE_LEN); i32_add; i32_const(hdr); i32_add;
        call(alloc); local_set(OUT);
        // len = cap = out_len + 2  (write both header words like __string_alloc)
        local_get(OUT); local_get(OUT_LEN); i32_const(QUOTE_LEN); i32_add; i32_store(0);
        local_get(OUT); local_get(OUT_LEN); i32_const(QUOTE_LEN); i32_add; i32_store(cap_off, 0);
        // w = out + DATA; write opening quote.
        local_get(OUT); i32_const(data_off); i32_add; local_set(W);
        local_get(W); i32_const(BYTE_QUOTE); i32_store8(0);
        local_get(W); i32_const(1); i32_add; local_set(W);
        // ── Pass 2: copy/escape each byte ──
        i32_const(0); local_set(I);
        block_empty; loop_empty;
          local_get(I); local_get(IN_LEN); i32_ge_u; br_if(1);
          local_get(S); i32_const(data_off); i32_add; local_get(I); i32_add;
          i32_load8_u(0); local_set(B);
          // esc = post-backslash char, or 0 if this byte is copied verbatim.
          //   \\ , "  → the literal byte itself (a backslash precedes it)
          //   \n      → 'n' , \r → 'r' , \t → 't'
          i32_const(0); local_set(ESC);
          local_get(B); i32_const(BYTE_BACKSLASH); i32_eq;
          local_get(B); i32_const(BYTE_QUOTE); i32_eq; i32_or;
          if_empty;
            local_get(B); local_set(ESC);          // self-quoting: \\ , \"
          else_;
            local_get(B); i32_const(BYTE_NL); i32_eq;
            if_empty; i32_const(CHAR_N); local_set(ESC); end;
            local_get(B); i32_const(BYTE_CR); i32_eq;
            if_empty; i32_const(CHAR_R); local_set(ESC); end;
            local_get(B); i32_const(BYTE_TAB); i32_eq;
            if_empty; i32_const(CHAR_T); local_set(ESC); end;
          end;
          // esc == 0 → copy byte verbatim; esc != 0 → write '\\' then esc.
          local_get(ESC); i32_eqz;
          if_empty;
            // verbatim byte (esc == 0)
            local_get(W); local_get(B); i32_store8(0);
            local_get(W); i32_const(1); i32_add; local_set(W);
          else_;
            // escaped pair: backslash + esc char
            local_get(W); i32_const(BYTE_BACKSLASH); i32_store8(0);
            local_get(W); i32_const(1); i32_add; local_get(ESC); i32_store8(0);
            local_get(W); i32_const(QUOTE_LEN); i32_add; local_set(W);
          end;
          local_get(I); i32_const(1); i32_add; local_set(I);
          br(0);
        end; end;
        // closing quote
        local_get(W); i32_const(BYTE_QUOTE); i32_store8(0);
        local_get(OUT);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}
