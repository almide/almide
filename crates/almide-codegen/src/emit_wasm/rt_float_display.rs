//! WASM runtime for the `Display`-style float text used in string interpolation.
//!
//! `float.to_string(x)` (the Dragon4 driver, `__float_to_string`) appends a `.0`
//! to every integer-valued float so a `Float` always reads as a float — that is
//! the stdlib contract and is byte-identical native↔WASM.
//!
//! Bare/contained string INTERPOLATION (`"${0.0}"`, `"${[1.0]}"`) is different:
//! the native oracle routes a float through Rust's `Display` (`format!("{}", x)`),
//! which renders an integer-valued float WITHOUT the trailing `.0` (`0.0` → `0`,
//! `-0.0` → `-0`, `1.0` → `1`). Every non-integral value (`0.1`, `1.5`, the huge
//! fixed-notation strings for `1e300` / `1e-300`) is already identical between
//! the two formats. So `Display(x)` is exactly `to_string(x)` with a single
//! trailing `.0` removed when (and only when) it is present.
//!
//! `__float_display(f) -> ptr` therefore calls the Dragon4 driver and, if the
//! result ends in `.0`, returns a copy truncated by those two bytes; otherwise
//! it returns the driver's string pointer unchanged. Output uses the standard
//! `[len:i32][cap:i32][data:u8...]` string layout.

use super::{CompiledFunc, WasmEmitter};
use wasm_encoder::ValType;
use super::TrackedFunction as Function;
use super::rt_string::{string_data_off, string_hdr, string_cap_off};

// The 2-byte `.0` suffix the Dragon4 driver adds to integer-valued floats; the
// only difference between `float.to_string` and the `Display` form.
const BYTE_DOT: i32 = 0x2E; // '.'
const BYTE_ZERO: i32 = 0x30; // '0'
const DOT_ZERO_LEN: i32 = 2;

pub(super) fn register(emitter: &mut WasmEmitter) {
    // __float_display(f: f64) -> i32 (String ptr)
    let ty = emitter.register_type(vec![ValType::F64], vec![ValType::I32]);
    emitter.rt.float_display = emitter.register_func("__float_display", ty);
}

/// __float_display(f) -> ptr: `__float_to_string(f)` with a trailing `.0` removed.
pub(super) fn compile(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.float_display];
    let to_string = emitter.rt.float_to_string;
    let alloc = emitter.rt.alloc;
    let data_off = string_data_off();
    let hdr = string_hdr();
    let cap_off = string_cap_off() as u32; // string header: [len@0][cap@cap_off]

    // param 0 = f (the float)
    // locals:
    //   1 = src      (Dragon4 string ptr)
    //   2 = len      (src byte length)
    //   3 = out      (truncated result ptr)
    //   4 = out_len  (len - 2)
    //   5 = i        (copy index)
    let mut f = Function::new([
        (1, ValType::I32), // 1: src
        (1, ValType::I32), // 2: len
        (1, ValType::I32), // 3: out
        (1, ValType::I32), // 4: out_len
        (1, ValType::I32), // 5: i
    ]);
    const F: u32 = 0;
    const SRC: u32 = 1;
    const LEN: u32 = 2;
    const OUT: u32 = 3;
    const OUT_LEN: u32 = 4;
    const I: u32 = 5;

    wasm!(f, {
        local_get(F); call(to_string); local_set(SRC);
        local_get(SRC); i32_load(0); local_set(LEN);
        // Keep the driver's string verbatim unless it ends in exactly `.0`:
        //   len >= 2  &&  data[len-2] == '.'  &&  data[len-1] == '0'
        local_get(LEN); i32_const(DOT_ZERO_LEN); i32_ge_s;
        local_get(SRC); i32_const(data_off); i32_add;
          local_get(LEN); i32_add; i32_const(DOT_ZERO_LEN); i32_sub;
          i32_load8_u(0); i32_const(BYTE_DOT); i32_eq;
        i32_and;
        local_get(SRC); i32_const(data_off); i32_add;
          local_get(LEN); i32_add; i32_const(1); i32_sub;
          i32_load8_u(0); i32_const(BYTE_ZERO); i32_eq;
        i32_and;
        if_i32;
          // out_len = len - 2; allocate header + out_len, copy the prefix bytes.
          local_get(LEN); i32_const(DOT_ZERO_LEN); i32_sub; local_set(OUT_LEN);
          local_get(OUT_LEN); i32_const(hdr); i32_add; call(alloc); local_set(OUT);
          local_get(OUT); local_get(OUT_LEN); i32_store(0);
          local_get(OUT); local_get(OUT_LEN); i32_store(cap_off, 0);
          i32_const(0); local_set(I);
          block_empty; loop_empty;
            local_get(I); local_get(OUT_LEN); i32_ge_u; br_if(1);
            local_get(OUT); i32_const(data_off); i32_add; local_get(I); i32_add;
            local_get(SRC); i32_const(data_off); i32_add; local_get(I); i32_add;
            i32_load8_u(0); i32_store8(0);
            local_get(I); i32_const(1); i32_add; local_set(I);
            br(0);
          end; end;
          local_get(OUT);
        else_;
          local_get(SRC);
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}
