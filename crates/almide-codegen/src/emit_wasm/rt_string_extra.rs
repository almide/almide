//! String stdlib WASM runtime: replace/search variants and predicates.
//!
//! Split from rt_string.rs for file size. These are all standalone compile_* functions
//! called from `compile()` in rt_string.rs.

// ── Named constants ──────────────────────────────────────────────────────────
/// Byte width of an i32 / WASM pointer (used when allocating a single-pointer
/// Option wrapper via `alloc`).
use crate::emit_wasm::engine::{Imm32, Imm64, Local};
const I32_BYTES: i32 = 4;
// ─────────────────────────────────────────────────────────────────────────────

use super::{CompiledFunc, WasmEmitter};
use wasm_encoder::{ValType};
use super::TrackedFunction as Function;
use super::engine::layout::{STRING, string as ls};
use super::rt_string::{string_data_off, string_hdr, string_cap_off};

// ── Replace/search variants ──

pub(super) fn compile_replace_first(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.replace_first];
    let mut f = Function::new([
        (1, ValType::I64), (1, ValType::I32), (1, ValType::I32),
        (1, ValType::I32), (1, ValType::I32),
    ]);
    wasm!(f, {
        local_get(Local(0)); local_get(Local(1)); call(emitter.rt.string.index_of); local_set(Local(3));
        local_get(Local(3)); i64_const(Imm64(-1)); i64_eq;
        if_i32; local_get(Local(0));
        else_;
          local_get(Local(3)); i32_wrap_i64; local_set(Local(4));
          local_get(Local(1)); i32_load(0); local_set(Local(5));
          local_get(Local(0)); i32_const(Imm32(0)); local_get(Local(4));
          call(emitter.rt.string.slice); local_set(Local(6));
          local_get(Local(0)); local_get(Local(4)); local_get(Local(5)); i32_add; local_get(Local(0)); i32_load(0);
          call(emitter.rt.string.slice); local_set(Local(7));
          local_get(Local(6)); local_get(Local(2)); call(emitter.rt.concat_str);
          local_get(Local(7)); call(emitter.rt.concat_str);
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.replace_first, type_idx, f));
}

pub(super) fn compile_last_index_of(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.last_index_of];
    let mut f = Function::new([
        (1, ValType::I32), (1, ValType::I32), (1, ValType::I32),
        (1, ValType::I32), (1, ValType::I32), (1, ValType::I64),
    ]);
    wasm!(f, {
        local_get(Local(0)); i32_load(0); local_set(Local(2));
        local_get(Local(1)); i32_load(0); local_set(Local(3));
        i64_const(Imm64(-1)); local_set(Local(7));
        local_get(Local(3)); i32_eqz;
        // Empty pattern: native `s.rfind("")` == Some(s.len()) — the BYTE length
        // (local 2). The Some sentinel is any non-negative i64; None is -1.
        if_i64; local_get(Local(2)); i64_extend_i32_u;
        else_;
          i32_const(Imm32(0)); local_set(Local(4));
          block_empty; loop_empty;
            local_get(Local(4)); local_get(Local(2)); local_get(Local(3)); i32_sub; i32_const(Imm32(1)); i32_add;
            i32_ge_u; br_if(1);
            local_get(Local(0)); i32_const(Imm32(string_data_off())); i32_add; local_get(Local(4)); i32_add;
            local_get(Local(1)); i32_const(Imm32(string_data_off())); i32_add;
            local_get(Local(3));
            call(emitter.rt.mem_eq);
            if_empty;
              local_get(Local(4)); i64_extend_i32_u; local_set(Local(7));
            end;
            local_get(Local(4)); i32_const(Imm32(1)); i32_add; local_set(Local(4));
            br(0);
          end; end;
          local_get(Local(7));
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.last_index_of, type_idx, f));
}

pub(super) fn compile_strip_prefix(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.strip_prefix];
    // params: 0=s, 1=prefix | locals: 2=s_len, 3=p_len, 4=result_str
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I32), (1, ValType::I32)]);
    wasm!(f, {
        local_get(Local(0)); i32_load(0); local_set(Local(2));
        local_get(Local(1)); i32_load(0); local_set(Local(3));
        local_get(Local(3)); local_get(Local(2)); i32_gt_u;
        if_i32; i32_const(Imm32(0)); // none
        else_;
          local_get(Local(0)); i32_const(Imm32(string_data_off())); i32_add;
          local_get(Local(1)); i32_const(Imm32(string_data_off())); i32_add;
          local_get(Local(3));
          call(emitter.rt.mem_eq);
          if_i32;
            // some(slice): wrap string ptr in Option (alloc 4 bytes, store ptr)
            local_get(Local(0)); local_get(Local(3)); local_get(Local(2));
            call(emitter.rt.string.slice); local_set(Local(4));
            i32_const(Imm32(I32_BYTES)); call(emitter.rt.alloc);
            local_tee(Local(3)); // reuse local 3
            local_get(Local(4)); i32_store(0);
            local_get(Local(3));
          else_;
            i32_const(Imm32(0));
          end;
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.strip_prefix, type_idx, f));
}

pub(super) fn compile_strip_suffix(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.strip_suffix];
    // params: 0=s, 1=suffix | locals: 2=s_len, 3=p_len, 4=result_str
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I32), (1, ValType::I32)]);
    wasm!(f, {
        local_get(Local(0)); i32_load(0); local_set(Local(2));
        local_get(Local(1)); i32_load(0); local_set(Local(3));
        local_get(Local(3)); local_get(Local(2)); i32_gt_u;
        if_i32; i32_const(Imm32(0));
        else_;
          local_get(Local(0)); i32_const(Imm32(string_data_off())); i32_add; local_get(Local(2)); i32_add; local_get(Local(3)); i32_sub;
          local_get(Local(1)); i32_const(Imm32(string_data_off())); i32_add;
          local_get(Local(3));
          call(emitter.rt.mem_eq);
          if_i32;
            local_get(Local(0)); i32_const(Imm32(0)); local_get(Local(2)); local_get(Local(3)); i32_sub;
            call(emitter.rt.string.slice); local_set(Local(4));
            i32_const(Imm32(I32_BYTES)); call(emitter.rt.alloc);
            local_tee(Local(3));
            local_get(Local(4)); i32_store(0);
            local_get(Local(3));
          else_;
            i32_const(Imm32(0));
          end;
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.strip_suffix, type_idx, f));
}

// ── Predicates ──

/// Generic byte predicate: checks all bytes in single range [lo..hi].
/// Empty string returns false (not vacuous truth).
pub(super) fn compile_byte_predicate_range(emitter: &mut WasmEmitter, func_idx: u32, lo: i32, hi: i32) {
    let type_idx = emitter.func_type_indices[&func_idx];
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I32)]);
    wasm!(f, {
        local_get(Local(0)); i32_load(0); local_set(Local(1));
        local_get(Local(1)); i32_eqz;
        if_i32; i32_const(Imm32(0));
        else_;
          i32_const(Imm32(0)); local_set(Local(2));
          block_empty; loop_empty;
            local_get(Local(2)); local_get(Local(1)); i32_ge_u; br_if(1);
            local_get(Local(0)); i32_const(Imm32(string_data_off())); i32_add; local_get(Local(2)); i32_add; i32_load8_u(0);
            local_tee(Local(1));
            i32_const(Imm32(lo)); i32_lt_u;
            local_get(Local(1)); i32_const(Imm32(hi)); i32_gt_u;
            i32_or;
            br_if(1);
            local_get(Local(0)); i32_load(0); local_set(Local(1)); // restore len
            local_get(Local(2)); i32_const(Imm32(1)); i32_add; local_set(Local(2));
            br(0);
          end; end;
          local_get(Local(2)); local_get(Local(0)); i32_load(0); i32_eq;
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

pub(super) fn compile_is_digit(emitter: &mut WasmEmitter) {
    compile_byte_predicate_range(emitter, emitter.rt.string.is_digit, 48, 57);
}

// ── Unicode-aware codepoint predicates ──
//
// Native Almide derives `is_alpha`/`is_alphanumeric`/`is_upper`/`is_lower` from
// Rust's full-Unicode `char` methods. The WASM versions below walk CODEPOINTS
// (not bytes), decode each scalar with the shared `utf8_scalar`/`utf8_width`
// helpers (same path `is_whitespace` uses), and binary-search the oracle-derived
// property range tables (see rt_unicode_tables.rs). This makes native and WASM
// equivalent by construction over the whole scalar space.

/// Emit one `(scalar: i32) -> i32` binary-search membership helper per property,
/// in registration order (alpha, alnum, upper, lower). Each searches the range
/// table interned at `table_off`; the table is a sorted array of inclusive
/// `[lo, hi]` little-endian u32 pairs, prefixed by the standard
/// `[len:i32][cap:i32]` interned-blob header. `len` (= the byte length) gives the
/// pair count via `len >> 3`, so no hardcoded range count is baked in.
pub(super) fn compile_prop_membership(emitter: &mut WasmEmitter) {
    use super::rt_unicode_tables::RANGE_ENTRY_BYTES;
    let entries = [
        (emitter.rt.string.prop_alpha, emitter.rt.string.prop_alpha_table),
        (emitter.rt.string.prop_alnum, emitter.rt.string.prop_alnum_table),
        (emitter.rt.string.prop_upper, emitter.rt.string.prop_upper_table),
        (emitter.rt.string.prop_lower, emitter.rt.string.prop_lower_table),
    ];
    // log2(RANGE_ENTRY_BYTES): a pair is 8 bytes, so pair_index = byte_off >> 3.
    let entry_shift = RANGE_ENTRY_BYTES.trailing_zeros() as i32;
    // Offset of `hi` within an entry (it follows the 4-byte `lo`), and the data
    // offset that skips the interned blob's `[len][cap]` header to the pairs.
    let hi_off = (RANGE_ENTRY_BYTES / 2) as i32;
    let data_off = string_data_off();
    for (func_idx, table_off) in entries {
        let type_idx = emitter.func_type_indices[&func_idx];
        // params: 0 = scalar
        // locals: 1 = lo (pair index), 2 = hi (pair index, exclusive),
        //         3 = mid, 4 = mid_ptr (base ptr of mid entry),
        //         5 = range_lo, 6 = data_base (pointer to first pair)
        let mut f = Function::new([(6, ValType::I32)]);
        // IMPORTANT (DCE landmine): only the BARE `table_off` may appear as an
        // i32.const — the dead-data eliminator relocates the table and patches
        // any const equal to a known string offset. `table_off + data_off` would
        // NOT be recognized, so `data_off` is added at RUNTIME below.
        wasm!(f, {
            // data_base = table_off + data_off (pointer to the first [lo,hi] pair)
            i32_const(Imm32(table_off as i32)); i32_const(Imm32(data_off)); i32_add; local_set(Local(6));
            // lo = 0; hi = pair_count = table_len >> entry_shift
            i32_const(Imm32(0)); local_set(Local(1));
            i32_const(Imm32(table_off as i32)); i32_load(0); i32_const(Imm32(entry_shift)); i32_shr_u; local_set(Local(2));
            block_empty; loop_empty;
              // while lo < hi
              local_get(Local(1)); local_get(Local(2)); i32_ge_u; br_if(1);
              // mid = (lo + hi) / 2
              local_get(Local(1)); local_get(Local(2)); i32_add; i32_const(Imm32(1)); i32_shr_u; local_set(Local(3));
              // mid_ptr = data_base + mid * RANGE_ENTRY_BYTES
              local_get(Local(6));
              local_get(Local(3)); i32_const(Imm32(RANGE_ENTRY_BYTES as i32)); i32_mul; i32_add; local_set(Local(4));
              // range_lo = *mid_ptr
              local_get(Local(4)); i32_load(0); local_set(Local(5));
              // if scalar < range_lo → search left (hi = mid)
              local_get(Local(0)); local_get(Local(5)); i32_lt_u;
              if_empty;
                local_get(Local(3)); local_set(Local(2));
              else_;
                // if scalar <= range_hi → hit
                local_get(Local(0)); local_get(Local(4)); i32_load(hi_off); i32_le_u;
                if_empty;
                  i32_const(Imm32(1)); return_;
                else_;
                  // scalar > range_hi → search right (lo = mid + 1)
                  local_get(Local(3)); i32_const(Imm32(1)); i32_add; local_set(Local(1));
                end;
              end;
              br(0);
            end; end;
            i32_const(Imm32(0)); // not found
            end;
        });
        emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
    }
}

/// is_alpha: non-empty AND every codepoint is alphabetic (full Unicode).
/// Native: `!s.is_empty() && s.chars().all(|c| c.is_alphabetic())`.
pub(super) fn compile_is_alpha(emitter: &mut WasmEmitter) {
    compile_all_codepoints_predicate(emitter, emitter.rt.string.is_alpha, emitter.rt.string.prop_alpha);
}

/// is_alnum: non-empty AND every codepoint is alphanumeric (full Unicode).
/// Native: `!s.is_empty() && s.chars().all(|c| c.is_alphanumeric())`.
pub(super) fn compile_is_alnum(emitter: &mut WasmEmitter) {
    compile_all_codepoints_predicate(emitter, emitter.rt.string.is_alnum, emitter.rt.string.prop_alnum);
}

/// Shared body for `is_alpha`/`is_alnum`: empty → false (NOT vacuously true —
/// native guards with `!s.is_empty()`), else every decoded scalar must satisfy
/// the given property membership helper. Walks codepoints via the shared
/// `utf8_scalar`/`utf8_width` helpers, exactly like `is_whitespace`.
fn compile_all_codepoints_predicate(emitter: &mut WasmEmitter, func_idx: u32, prop_idx: u32) {
    let type_idx = emitter.func_type_indices[&func_idx];
    let uw = emitter.rt.string.utf8_width;
    let us = emitter.rt.string.utf8_scalar;
    const S: u32 = 0;     // param: string ptr
    const BLEN: u32 = 1;  // byte length
    const I: u32 = 2;     // byte index
    let mut f = Function::new([(2, ValType::I32)]);
    wasm!(f, {
        local_get(Local(S)); i32_load(0); local_set(Local(BLEN));
        local_get(Local(BLEN)); i32_eqz;
        if_i32; i32_const(Imm32(0));                                   // empty → false
        else_;
          i32_const(Imm32(0)); local_set(Local(I));
          block_empty; loop_empty;
            local_get(Local(I)); local_get(Local(BLEN)); i32_ge_u; br_if(1);  // walked all → true
            // scalar = utf8_scalar(s, i); fail if not a member
            local_get(Local(S)); local_get(Local(I)); call(us); i32_wrap_i64; call(prop_idx); i32_eqz;
            if_empty; i32_const(Imm32(0)); return_; end;
            // i += utf8_width(s, i)
            local_get(Local(S)); local_get(Local(I)); call(uw); local_get(Local(I)); i32_add; local_set(Local(I));
            br(0);
          end; end;
          i32_const(Imm32(1));
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

/// is_whitespace: every codepoint has the Unicode White_Space property (Rust
/// `char::is_whitespace`, via `__is_unicode_ws`). Mirrors native
/// `s.chars().all(|c| c.is_whitespace())`, so the EMPTY string is vacuously TRUE
/// (Rust `.all()` on an empty iterator) — the old byte version wrongly returned
/// false for "" and missed VT/FF and all non-ASCII whitespace.
pub(super) fn compile_is_whitespace(emitter: &mut WasmEmitter) {
    let func_idx = emitter.rt.string.is_whitespace;
    let type_idx = emitter.func_type_indices[&func_idx];
    let uw = emitter.rt.string.utf8_width;
    let us = emitter.rt.string.utf8_scalar;
    let isws = emitter.rt.string.is_unicode_ws;
    const S: u32 = 0; // param: string ptr
    const BLEN: u32 = 1;
    const I: u32 = 2;
    let mut f = Function::new([(2, ValType::I32)]);
    wasm!(f, {
        local_get(Local(S)); i32_load(0); local_set(Local(BLEN));
        i32_const(Imm32(0)); local_set(Local(I));
        block_empty; loop_empty;
          local_get(Local(I)); local_get(Local(BLEN)); i32_ge_u; br_if(1);   // end (incl. empty) → all WS
          local_get(Local(S)); local_get(Local(I)); call(us); i32_wrap_i64; call(isws); i32_eqz;
          if_empty; i32_const(Imm32(0)); return_; end;                // a non-WS codepoint → false
          local_get(Local(S)); local_get(Local(I)); call(uw); local_get(Local(I)); i32_add; local_set(Local(I));
          br(0);
        end; end;
        i32_const(Imm32(1));
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

/// is_upper: non-empty, has >=1 alphabetic codepoint, and every alphabetic
/// codepoint is uppercase. Native:
/// `!s.is_empty() && s.chars().any(is_alphabetic)
///   && s.chars().all(|c| !c.is_alphabetic() || c.is_uppercase())`.
pub(super) fn compile_is_upper(emitter: &mut WasmEmitter) {
    compile_case_predicate(emitter, emitter.rt.string.is_upper, emitter.rt.string.prop_upper);
}

/// is_lower: mirror of is_upper against the lowercase property.
pub(super) fn compile_is_lower(emitter: &mut WasmEmitter) {
    compile_case_predicate(emitter, emitter.rt.string.is_lower, emitter.rt.string.prop_lower);
}

/// Shared body for `is_upper`/`is_lower`. Walks codepoints (via the shared
/// `utf8_scalar`/`utf8_width` helpers) tracking two flags that mirror the native
/// `any`/`all` expression exactly:
///   any_alpha  — set when an alphabetic codepoint is seen
///   all_ok     — cleared when an alphabetic codepoint lacks the case property
/// Result = `any_alpha && all_ok` (empty string short-circuits to false). The
/// `any_alpha` guard is what fixes the old vacuous-true bugs: "123" and any
/// caseless multibyte string now return false, and no string is both.
fn compile_case_predicate(emitter: &mut WasmEmitter, func_idx: u32, case_prop_idx: u32) {
    let type_idx = emitter.func_type_indices[&func_idx];
    let uw = emitter.rt.string.utf8_width;
    let us = emitter.rt.string.utf8_scalar;
    let prop_alpha = emitter.rt.string.prop_alpha;
    const S: u32 = 0;        // param: string ptr
    const BLEN: u32 = 1;     // byte length
    const I: u32 = 2;        // byte index
    const SCALAR: u32 = 3;   // decoded scalar
    const ANY_ALPHA: u32 = 4;
    const ALL_OK: u32 = 5;
    let mut f = Function::new([(5, ValType::I32)]);
    wasm!(f, {
        local_get(Local(S)); i32_load(0); local_set(Local(BLEN));
        local_get(Local(BLEN)); i32_eqz;
        if_i32; i32_const(Imm32(0));                                   // empty → false
        else_;
          i32_const(Imm32(0)); local_set(Local(I));
          i32_const(Imm32(0)); local_set(Local(ANY_ALPHA));                   // any_alpha = false
          i32_const(Imm32(1)); local_set(Local(ALL_OK));                      // all_ok = true
          block_empty; loop_empty;
            local_get(Local(I)); local_get(Local(BLEN)); i32_ge_u; br_if(1);
            local_get(Local(S)); local_get(Local(I)); call(us); i32_wrap_i64; local_set(Local(SCALAR));
            // if is_alphabetic(scalar)
            local_get(Local(SCALAR)); call(prop_alpha);
            if_empty;
              i32_const(Imm32(1)); local_set(Local(ANY_ALPHA));               // any_alpha = true
              // if NOT case_prop(scalar) → all_ok = false
              local_get(Local(SCALAR)); call(case_prop_idx); i32_eqz;
              if_empty;
                i32_const(Imm32(0)); local_set(Local(ALL_OK));
              end;
            end;
            // i += utf8_width(s, i)
            local_get(Local(S)); local_get(Local(I)); call(uw); local_get(Local(I)); i32_add; local_set(Local(I));
            br(0);
          end; end;
          local_get(Local(ANY_ALPHA)); local_get(Local(ALL_OK)); i32_and;
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

/// __str_cmp(a: i32, b: i32) -> i32
/// Lexicographic comparison: negative if a<b, 0 if equal, positive if a>b.
pub(super) fn compile_cmp(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.cmp];
    let mut f = Function::new([
        (1, ValType::I32), (1, ValType::I32),
        (1, ValType::I32), (1, ValType::I32),
    ]);
    use wasm_encoder::Instruction::*;
    let mem0 = wasm_encoder::MemArg { offset: 0, align: 2, memory_index: 0 };
    let mem0_byte = wasm_encoder::MemArg { offset: 0, align: 0, memory_index: 0 };
    // min_len = min(a.len, b.len)
    f.instruction(&LocalGet(0)).instruction(&I32Load(mem0));
    f.instruction(&LocalGet(1)).instruction(&I32Load(mem0));
    f.instruction(&I32LeU);
    f.instruction(&If(wasm_encoder::BlockType::Result(ValType::I32)));
    f.instruction(&LocalGet(0)).instruction(&I32Load(mem0));
    f.instruction(&Else);
    f.instruction(&LocalGet(1)).instruction(&I32Load(mem0));
    f.instruction(&End);
    f.instruction(&LocalSet(2));
    f.instruction(&I32Const(0)).instruction(&LocalSet(3));
    f.instruction(&Block(wasm_encoder::BlockType::Empty));
    f.instruction(&Loop(wasm_encoder::BlockType::Empty));
    f.instruction(&LocalGet(3)).instruction(&LocalGet(2)).instruction(&I32GeU);
    f.instruction(&BrIf(1));
    f.instruction(&LocalGet(0)).instruction(&I32Const(string_data_off())).instruction(&I32Add);
    f.instruction(&LocalGet(3)).instruction(&I32Add);
    f.instruction(&I32Load8U(mem0_byte));
    f.instruction(&LocalSet(4));
    f.instruction(&LocalGet(1)).instruction(&I32Const(string_data_off())).instruction(&I32Add);
    f.instruction(&LocalGet(3)).instruction(&I32Add);
    f.instruction(&I32Load8U(mem0_byte));
    f.instruction(&LocalSet(5));
    f.instruction(&LocalGet(4)).instruction(&LocalGet(5)).instruction(&I32Ne);
    f.instruction(&If(wasm_encoder::BlockType::Empty));
    f.instruction(&LocalGet(4)).instruction(&LocalGet(5)).instruction(&I32Sub);
    f.instruction(&Return);
    f.instruction(&End);
    f.instruction(&LocalGet(3)).instruction(&I32Const(1)).instruction(&I32Add).instruction(&LocalSet(3));
    f.instruction(&Br(0));
    f.instruction(&End).instruction(&End);
    f.instruction(&LocalGet(0)).instruction(&I32Load(mem0));
    f.instruction(&LocalGet(1)).instruction(&I32Load(mem0));
    f.instruction(&I32Sub);
    f.instruction(&End);
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.cmp, type_idx, f));
}
