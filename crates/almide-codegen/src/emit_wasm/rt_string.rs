//! String stdlib WASM runtime functions.
//!
//! All `__str_*` runtime function registration and compilation lives here.

use super::{CompiledFunc, WasmEmitter};
use wasm_encoder::{Function, ValType};

/// Register all string runtime function signatures.
pub fn register(emitter: &mut WasmEmitter) {
    // Reusable type signatures
    let ty_i32x3_i32 = emitter.register_type(vec![ValType::I32, ValType::I32, ValType::I32], vec![ValType::I32]);
    let ty_i32x2_i32 = emitter.register_type(vec![ValType::I32, ValType::I32], vec![ValType::I32]);
    let ty_i32_i32 = emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
    let ty_i32x2_i64 = emitter.register_type(vec![ValType::I32, ValType::I32], vec![ValType::I64]);
    let ty_i32_i64 = emitter.register_type(vec![ValType::I32], vec![ValType::I64]);

    let s = &mut emitter.rt.string;
    // Will be set after register_func calls — need to go through emitter
    let _ = s;

    let rt = &mut emitter.rt;
    // Can't borrow emitter and rt.string simultaneously. Use a different approach.
    let _ = rt;

    // Core string ops
    emitter.rt.string.eq = emitter.register_func("__str_eq", ty_i32x2_i32);
    emitter.rt.string.contains = emitter.register_func("__str_contains", ty_i32x2_i32);
    emitter.rt.string.trim = emitter.register_func("__str_trim", ty_i32_i32);

    // Slice / transform
    emitter.rt.string.slice = emitter.register_func("__str_slice", ty_i32x3_i32);
    emitter.rt.string.reverse = emitter.register_func("__str_reverse", ty_i32_i32);
    emitter.rt.string.repeat = emitter.register_func("__str_repeat", ty_i32x2_i32);
    emitter.rt.string.index_of = emitter.register_func("__str_index_of", ty_i32x2_i64);
    emitter.rt.string.replace = emitter.register_func("__str_replace", ty_i32x3_i32);
    emitter.rt.string.split = emitter.register_func("__str_split", ty_i32x2_i32);
    emitter.rt.string.join = emitter.register_func("__str_join", ty_i32x2_i32);
    emitter.rt.string.count = emitter.register_func("__str_count", ty_i32x2_i64);

    // Padding / trimming
    emitter.rt.string.pad_start = emitter.register_func("__str_pad_start", ty_i32x3_i32);
    emitter.rt.string.pad_end = emitter.register_func("__str_pad_end", ty_i32x3_i32);
    emitter.rt.string.trim_start = emitter.register_func("__str_trim_start", ty_i32_i32);
    emitter.rt.string.trim_end = emitter.register_func("__str_trim_end", ty_i32_i32);

    // Case transform
    emitter.rt.string.to_upper = emitter.register_func("__str_to_upper", ty_i32_i32);
    emitter.rt.string.to_lower = emitter.register_func("__str_to_lower", ty_i32_i32);

    // Decompose
    emitter.rt.string.chars = emitter.register_func("__str_chars", ty_i32_i32);
    emitter.rt.string.lines = emitter.register_func("__str_lines", ty_i32_i32);
    emitter.rt.string.from_bytes = emitter.register_func("__str_from_bytes", ty_i32_i32);
    emitter.rt.string.to_bytes = emitter.register_func("__str_to_bytes", ty_i32_i32);

    // Replace / search variants
    emitter.rt.string.replace_first = emitter.register_func("__str_replace_first", ty_i32x3_i32);
    emitter.rt.string.last_index_of = emitter.register_func("__str_last_index_of", ty_i32x2_i64);
    emitter.rt.string.strip_prefix = emitter.register_func("__str_strip_prefix", ty_i32x2_i32);
    emitter.rt.string.strip_suffix = emitter.register_func("__str_strip_suffix", ty_i32x2_i32);

    // Predicates — return i32 (Bool is i32 in WASM)
    emitter.rt.string.is_digit = emitter.register_func("__str_is_digit", ty_i32_i32);
    emitter.rt.string.is_alpha = emitter.register_func("__str_is_alpha", ty_i32_i32);
    emitter.rt.string.is_alnum = emitter.register_func("__str_is_alnum", ty_i32_i32);
    emitter.rt.string.is_whitespace = emitter.register_func("__str_is_whitespace", ty_i32_i32);
    emitter.rt.string.is_upper = emitter.register_func("__str_is_upper", ty_i32_i32);
    emitter.rt.string.is_lower = emitter.register_func("__str_is_lower", ty_i32_i32);

    // String comparison: (a: i32, b: i32) -> i32 (negative/0/positive)
    emitter.rt.string.cmp = emitter.register_func("__str_cmp", ty_i32x2_i32);

    // UTF-8 char count: (s: i32) -> i64 — counts code points, not bytes.
    // Distinct from `string.len` which reads the byte-count header and is used
    // for sizing; `char_count` walks the data section and skips UTF-8
    // continuation bytes (bytes whose top two bits are `10`).
    emitter.rt.string.char_count = emitter.register_func("__str_char_count", ty_i32_i64);
}

/// Compile all string runtime function bodies.
pub fn compile(emitter: &mut WasmEmitter) {
    compile_eq(emitter);
    compile_contains(emitter);
    compile_trim(emitter);
    compile_slice(emitter);
    compile_reverse(emitter);
    compile_repeat(emitter);
    compile_index_of(emitter);
    compile_replace(emitter);
    compile_split(emitter);
    compile_join(emitter);
    compile_count(emitter);
    compile_pad_start(emitter);
    compile_pad_end(emitter);
    compile_trim_start(emitter);
    compile_trim_end(emitter);
    compile_to_upper(emitter);
    compile_to_lower(emitter);
    compile_chars(emitter);
    compile_lines(emitter);
    compile_from_bytes(emitter);
    compile_to_bytes(emitter);
    super::rt_string_extra::compile_replace_first(emitter);
    super::rt_string_extra::compile_last_index_of(emitter);
    super::rt_string_extra::compile_strip_prefix(emitter);
    super::rt_string_extra::compile_strip_suffix(emitter);
    super::rt_string_extra::compile_is_digit(emitter);
    super::rt_string_extra::compile_is_alpha(emitter);
    super::rt_string_extra::compile_is_alnum(emitter);
    super::rt_string_extra::compile_is_whitespace(emitter);
    super::rt_string_extra::compile_is_upper(emitter);
    super::rt_string_extra::compile_is_lower(emitter);
    super::rt_string_extra::compile_cmp(emitter);
    compile_char_count(emitter);
}

/// Count UTF-8 code points in a string (pointer to `[len:i32][bytes...]`).
/// Iterates the byte payload and skips continuation bytes (top bits `10xxxxxx`).
fn compile_char_count(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.char_count];
    // Locals: 1=byte_len, 2=i (byte index), 3=count
    let mut f = Function::new([(3, ValType::I32)]);
    wasm!(f, {
        // byte_len = *s
        local_get(0); i32_load(0); local_set(1);
        // i = 0, count = 0
        i32_const(0); local_set(2);
        i32_const(0); local_set(3);
        block_empty; loop_empty;
          local_get(2); local_get(1); i32_ge_u; br_if(1);
          // byte = s[4 + i]
          local_get(0); i32_const(4); i32_add;
          local_get(2); i32_add; i32_load8_u(0);
          // if (byte & 0xC0) != 0x80 then count++
          i32_const(0xC0); i32_and;
          i32_const(0x80); i32_ne;
          if_empty;
            local_get(3); i32_const(1); i32_add; local_set(3);
          end;
          local_get(2); i32_const(1); i32_add; local_set(2);
          br(0);
        end; end;
        local_get(3); i64_extend_i32_u;
        end;
    });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

// ── Core ──

fn compile_eq(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.eq];
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I32)]);
    // Same pointer → 1
    wasm!(f, {
        local_get(0); local_get(1); i32_eq;
        if_empty; i32_const(1); return_; end;
    });
    // Length mismatch → 0
    wasm!(f, {
        local_get(0); i32_load(0); local_set(2);
        local_get(2); local_get(1); i32_load(0); i32_ne;
        if_empty; i32_const(0); return_; end;
    });
    // Byte-by-byte compare
    wasm!(f, {
        i32_const(0); local_set(3);
        block_empty; loop_empty;
          local_get(3); local_get(2); i32_ge_u;
          if_empty; i32_const(1); return_; end;
          local_get(0); i32_const(4); i32_add; local_get(3); i32_add; i32_load8_u(0);
          local_get(1); i32_const(4); i32_add; local_get(3); i32_add; i32_load8_u(0);
          i32_ne;
          if_empty; i32_const(0); return_; end;
          local_get(3); i32_const(1); i32_add; local_set(3);
          br(0);
        end; end;
    });
    wasm!(f, { i32_const(0); end; });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

fn compile_contains(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.contains];
    let mut f = Function::new([
        (1, ValType::I32), (1, ValType::I32), (1, ValType::I32),
        (1, ValType::I32), (1, ValType::I32),
    ]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(2); // h_len
        local_get(1); i32_load(0); local_set(3); // n_len
        // empty needle → true
        local_get(3); i32_eqz;
        if_empty; i32_const(1); return_; end;
        // n_len > h_len → false
        local_get(3); local_get(2); i32_gt_u;
        if_empty; i32_const(0); return_; end;
        i32_const(0); local_set(4); // i=0
        block_empty; loop_empty;
          local_get(4); local_get(2); local_get(3); i32_sub; i32_const(1); i32_add;
          i32_ge_u; br_if(1);
          // compare h[4+i..] with n[4..n_len]
          local_get(0); i32_const(4); i32_add; local_get(4); i32_add;
          local_get(1); i32_const(4); i32_add;
          local_get(3);
          call(emitter.rt.mem_eq);
          if_empty; i32_const(1); return_; end;
          local_get(4); i32_const(1); i32_add; local_set(4);
          br(0);
        end; end;
        i32_const(0); end;
    });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

fn compile_trim(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.trim];
    let mut f = Function::new([
        (1, ValType::I32), (1, ValType::I32), (1, ValType::I32), (1, ValType::I32),
    ]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(1);
        i32_const(0); local_set(2); // start
        local_get(1); local_set(3); // end = len
    });
    // Find start (skip whitespace)
    wasm!(f, {
        block_empty; loop_empty;
          local_get(2); local_get(3); i32_ge_u; br_if(1);
          local_get(0); i32_const(4); i32_add; local_get(2); i32_add; i32_load8_u(0);
          local_set(1);
          local_get(1); i32_const(32); i32_eq;
          local_get(1); i32_const(9); i32_eq; i32_or;
          local_get(1); i32_const(10); i32_eq; i32_or;
          local_get(1); i32_const(13); i32_eq; i32_or;
          i32_eqz; br_if(1);
          local_get(2); i32_const(1); i32_add; local_set(2);
          br(0);
        end; end;
    });
    // Find end (skip trailing whitespace)
    wasm!(f, {
        block_empty; loop_empty;
          local_get(3); local_get(2); i32_le_u; br_if(1);
          local_get(0); i32_const(4); i32_add;
          local_get(3); i32_const(1); i32_sub; i32_add; i32_load8_u(0);
          local_set(1);
          local_get(1); i32_const(32); i32_eq;
          local_get(1); i32_const(9); i32_eq; i32_or;
          local_get(1); i32_const(10); i32_eq; i32_or;
          local_get(1); i32_const(13); i32_eq; i32_or;
          i32_eqz; br_if(1);
          local_get(3); i32_const(1); i32_sub; local_set(3);
          br(0);
        end; end;
    });
    // slice(s, start, end)
    wasm!(f, {
        local_get(0); local_get(2); local_get(3);
        call(emitter.rt.string.slice);
        end;
    });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

// ── Slice / transform ──

fn compile_slice(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.slice];
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I32)]);
    wasm!(f, {
        i32_const(4); local_get(2); local_get(1); i32_sub; i32_add;
        call(emitter.rt.alloc); local_set(3);
        local_get(3); local_get(2); local_get(1); i32_sub; i32_store(0);
        i32_const(0); local_set(4);
        block_empty; loop_empty;
          local_get(4); local_get(2); local_get(1); i32_sub; i32_ge_u; br_if(1);
          local_get(3); i32_const(4); i32_add; local_get(4); i32_add;
          local_get(0); i32_const(4); i32_add; local_get(1); i32_add; local_get(4); i32_add;
          i32_load8_u(0); i32_store8(0);
          local_get(4); i32_const(1); i32_add; local_set(4);
          br(0);
        end; end;
        local_get(3); end;
    });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

fn compile_reverse(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.reverse];
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I32), (1, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(1);
        i32_const(4); local_get(1); i32_add; call(emitter.rt.alloc); local_set(2);
        local_get(2); local_get(1); i32_store(0);
        i32_const(0); local_set(3);
        block_empty; loop_empty;
          local_get(3); local_get(1); i32_ge_u; br_if(1);
          local_get(2); i32_const(4); i32_add; local_get(3); i32_add;
          local_get(0); i32_const(4); i32_add;
          local_get(1); i32_const(1); i32_sub; local_get(3); i32_sub; i32_add;
          i32_load8_u(0); i32_store8(0);
          local_get(3); i32_const(1); i32_add; local_set(3);
          br(0);
        end; end;
        local_get(2); end;
    });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

fn compile_repeat(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.repeat];
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_load(0); local_get(1); i32_mul; local_set(2);
        i32_const(4); local_get(2); i32_add; call(emitter.rt.alloc); local_set(3);
        local_get(3); local_get(2); i32_store(0);
        i32_const(0); local_set(2); // reuse as offset
        block_empty; loop_empty;
          local_get(2); local_get(0); i32_load(0); local_get(1); i32_mul; i32_ge_u; br_if(1);
          local_get(3); i32_const(4); i32_add; local_get(2); i32_add;
          local_get(0); i32_const(4); i32_add;
          local_get(2); local_get(0); i32_load(0); i32_rem_u;
          i32_add; i32_load8_u(0); i32_store8(0);
          local_get(2); i32_const(1); i32_add; local_set(2);
          br(0);
        end; end;
        local_get(3); end;
    });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
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
          local_get(0); i32_const(4); i32_add; local_get(4); i32_add;
          local_get(1); i32_const(4); i32_add;
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
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

fn compile_replace(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.replace];
    let mut f = Function::new([
        (1, ValType::I64), (1, ValType::I32), (1, ValType::I32),
        (1, ValType::I32), (1, ValType::I32),
    ]);
    wasm!(f, {
        local_get(0); local_get(1); call(emitter.rt.string.index_of); local_set(3);
        local_get(3); i64_const(-1); i64_eq;
        if_i32; local_get(0);
        else_;
          local_get(3); i32_wrap_i64; local_set(4);
          local_get(1); i32_load(0); local_set(5);
          local_get(0); i32_const(0); local_get(4);
          call(emitter.rt.string.slice); local_set(6);
          local_get(0); local_get(4); local_get(5); i32_add; local_get(0); i32_load(0);
          call(emitter.rt.string.slice); local_set(7);
          local_get(7); local_get(1); local_get(2);
          call(emitter.rt.string.replace); local_set(7);
          local_get(6); local_get(2); call(emitter.rt.concat_str);
          local_get(7); call(emitter.rt.concat_str);
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// Recursive split using index_of. Supports multi-char delimiter.
/// Strategy: find first delimiter → [before] ++ split(rest, delim)
/// Base case: no delimiter found → [s]
fn compile_split(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.split];
    // params: 0=s, 1=delim | locals: 2=idx(i64), 3=d_len, 4=before, 5=rest, 6=rest_list, 7=result
    let mut f = Function::new([
        (1, ValType::I64), (1, ValType::I32), (1, ValType::I32),
        (1, ValType::I32), (1, ValType::I32), (1, ValType::I32),
    ]);
    wasm!(f, {
        local_get(1); i32_load(0); local_set(3); // d_len
        // Empty delimiter: return [s] to avoid infinite recursion on index_of("x", "") == 0.
        local_get(3); i32_eqz;
        if_empty;
          i32_const(8); call(emitter.rt.alloc); local_set(7);
          local_get(7); i32_const(1); i32_store(0);
          local_get(7); local_get(0); i32_store(4);
          local_get(7); return_;
        end;
        local_get(0); local_get(1); call(emitter.rt.string.index_of); local_set(2);
        local_get(2); i64_const(-1); i64_eq;
        if_i32;
          // No match: return [s]
          i32_const(8); call(emitter.rt.alloc); local_set(7);
          local_get(7); i32_const(1); i32_store(0);
          local_get(7); local_get(0); i32_store(4);
          local_get(7);
        else_;
          // before = slice(s, 0, idx)
          local_get(0); i32_const(0); local_get(2); i32_wrap_i64;
          call(emitter.rt.string.slice); local_set(4);
          // rest = slice(s, idx + d_len, s_len)
          local_get(0);
          local_get(2); i32_wrap_i64; local_get(3); i32_add;
          local_get(0); i32_load(0);
          call(emitter.rt.string.slice); local_set(5);
          // rest_list = split(rest, delim) — recursive
          local_get(5); local_get(1);
          call(emitter.rt.string.split); local_set(6);
          // result = [before] ++ rest_list
          // Alloc: 4 + (1 + rest_list.len) * 4
          i32_const(4);
          local_get(6); i32_load(0); i32_const(1); i32_add;
          i32_const(4); i32_mul; i32_add;
          call(emitter.rt.alloc); local_set(7);
          local_get(7);
          local_get(6); i32_load(0); i32_const(1); i32_add;
          i32_store(0); // result.len
          // result[0] = before
          local_get(7); local_get(4); i32_store(4);
    });
    // Copy rest_list elements to result[1..]
    wasm!(f, {
          i32_const(0); local_set(3); // reuse as i
          block_empty; loop_empty;
            local_get(3); local_get(6); i32_load(0); i32_ge_u; br_if(1);
            local_get(7); i32_const(8); i32_add; // &result[1]
            local_get(3); i32_const(4); i32_mul; i32_add;
            local_get(6); i32_const(4); i32_add;
            local_get(3); i32_const(4); i32_mul; i32_add;
            i32_load(0); i32_store(0);
            local_get(3); i32_const(1); i32_add; local_set(3);
            br(0);
          end; end;
          local_get(7);
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

fn compile_join(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.join];
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I32), (1, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(2); // len
        local_get(2); i32_eqz;
        if_i32;
          // empty list → empty string
          i32_const(4); call(emitter.rt.alloc); local_tee(4);
          i32_const(0); i32_store(0);
          local_get(4);
        else_;
          // result = list[0]
          local_get(0); i32_const(4); i32_add; i32_load(0); local_set(4);
          i32_const(1); local_set(3); // i=1
          block_empty; loop_empty;
            local_get(3); local_get(2); i32_ge_u; br_if(1);
            // result = concat(result, sep)
            local_get(4); local_get(1); call(emitter.rt.concat_str); local_set(4);
            // result = concat(result, list[i])
            local_get(4);
            local_get(0); i32_const(4); i32_add;
            local_get(3); i32_const(4); i32_mul; i32_add; i32_load(0);
            call(emitter.rt.concat_str); local_set(4);
            local_get(3); i32_const(1); i32_add; local_set(3);
            br(0);
          end; end;
          local_get(4);
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
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
        if_i64; i64_const(0);
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
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

// ── Padding / trimming ──

fn compile_pad_start(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.pad_start];
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I32), (1, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(3);
        local_get(3); local_get(1); i32_ge_u;
        if_i32; local_get(0);
        else_;
          local_get(1); local_get(3); i32_sub; local_set(4);
          local_get(2); local_get(4); call(emitter.rt.string.repeat); local_set(5);
          local_get(5); local_get(0); call(emitter.rt.concat_str);
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

fn compile_pad_end(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.pad_end];
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I32), (1, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(3);
        local_get(3); local_get(1); i32_ge_u;
        if_i32; local_get(0);
        else_;
          local_get(1); local_get(3); i32_sub; local_set(4);
          local_get(2); local_get(4); call(emitter.rt.string.repeat); local_set(5);
          local_get(0); local_get(5); call(emitter.rt.concat_str);
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

fn compile_trim_start(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.trim_start];
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(1);
        i32_const(0); local_set(2);
        block_empty; loop_empty;
          local_get(2); local_get(1); i32_ge_u; br_if(1);
          local_get(0); i32_const(4); i32_add; local_get(2); i32_add; i32_load8_u(0);
          local_tee(1);
          i32_const(32); i32_eq;
          local_get(1); i32_const(9); i32_eq; i32_or;
          local_get(1); i32_const(10); i32_eq; i32_or;
          local_get(1); i32_const(13); i32_eq; i32_or;
          i32_eqz; br_if(1);
          local_get(2); i32_const(1); i32_add; local_set(2);
          local_get(0); i32_load(0); local_set(1);
          br(0);
        end; end;
        local_get(0); i32_load(0); local_set(1);
        local_get(0); local_get(2); local_get(1);
        call(emitter.rt.string.slice);
        end;
    });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

fn compile_trim_end(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.trim_end];
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(1);
        block_empty; loop_empty;
          local_get(1); i32_eqz; br_if(1);
          local_get(0); i32_const(4); i32_add;
          local_get(1); i32_const(1); i32_sub; i32_add;
          i32_load8_u(0); local_set(2);
          local_get(2); i32_const(32); i32_eq;
          local_get(2); i32_const(9); i32_eq; i32_or;
          local_get(2); i32_const(10); i32_eq; i32_or;
          local_get(2); i32_const(13); i32_eq; i32_or;
          i32_eqz; br_if(1);
          local_get(1); i32_const(1); i32_sub; local_set(1);
          br(0);
        end; end;
        local_get(0); i32_const(0); local_get(1);
        call(emitter.rt.string.slice);
        end;
    });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

// ── Case transform ──

fn compile_to_upper(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.to_upper];
    compile_case_transform(emitter, type_idx, 97, 122, -32); // a-z → A-Z
}

fn compile_to_lower(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.to_lower];
    compile_case_transform(emitter, type_idx, 65, 90, 32); // A-Z → a-z
}

fn compile_case_transform(emitter: &mut WasmEmitter, type_idx: u32, lo: i32, hi: i32, delta: i32) {
    let mut f = Function::new([
        (1, ValType::I32), (1, ValType::I32), (1, ValType::I32), (1, ValType::I32),
    ]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(1);
        i32_const(4); local_get(1); i32_add;
        call(emitter.rt.alloc); local_set(2);
        local_get(2); local_get(1); i32_store(0);
        i32_const(0); local_set(3);
        block_empty; loop_empty;
          local_get(3); local_get(1); i32_ge_u; br_if(1);
          local_get(0); i32_const(4); i32_add; local_get(3); i32_add;
          i32_load8_u(0); local_set(4);
          local_get(4); i32_const(lo); i32_ge_u;
          local_get(4); i32_const(hi); i32_le_u;
          i32_and;
          if_empty;
            local_get(4); i32_const(delta); i32_add; local_set(4);
          end;
          local_get(2); i32_const(4); i32_add; local_get(3); i32_add;
          local_get(4); i32_store8(0);
          local_get(3); i32_const(1); i32_add; local_set(3);
          br(0);
        end; end;
        local_get(2); end;
    });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

// ── Decompose ──

fn compile_chars(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.chars];
    let mut f = Function::new([
        (1, ValType::I32), (1, ValType::I32), (1, ValType::I32), (1, ValType::I32),
    ]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(1);
        i32_const(4); local_get(1); i32_const(4); i32_mul; i32_add;
        call(emitter.rt.alloc); local_set(2);
        local_get(2); local_get(1); i32_store(0);
        i32_const(0); local_set(3);
        block_empty; loop_empty;
          local_get(3); local_get(1); i32_ge_u; br_if(1);
          i32_const(5); call(emitter.rt.alloc); local_set(4);
          local_get(4); i32_const(1); i32_store(0);
          local_get(4);
          local_get(0); i32_const(4); i32_add; local_get(3); i32_add; i32_load8_u(0);
          i32_store8(4);
          local_get(2); i32_const(4); i32_add; local_get(3); i32_const(4); i32_mul; i32_add;
          local_get(4); i32_store(0);
          local_get(3); i32_const(1); i32_add; local_set(3);
          br(0);
        end; end;
        local_get(2); end;
    });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

fn compile_lines(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.lines];
    let mut f = Function::new([(1, ValType::I32)]);
    // If input string is empty, return empty list (alloc 4 bytes, len=0)
    wasm!(f, {
        local_get(0); i32_load(0); i32_eqz;
        if_i32;
          i32_const(4); call(emitter.rt.alloc); local_set(1);
          local_get(1); i32_const(0); i32_store(0);
          local_get(1);
        else_;
          i32_const(5); call(emitter.rt.alloc); local_set(1);
          local_get(1); i32_const(1); i32_store(0);
          local_get(1); i32_const(10); i32_store8(4);
          local_get(0); local_get(1); call(emitter.rt.string.split);
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

fn compile_from_bytes(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.from_bytes];
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I32), (1, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(1);
        i32_const(4); local_get(1); i32_add;
        call(emitter.rt.alloc); local_set(2);
        local_get(2); local_get(1); i32_store(0);
        i32_const(0); local_set(3);
        block_empty; loop_empty;
          local_get(3); local_get(1); i32_ge_u; br_if(1);
          local_get(2); i32_const(4); i32_add; local_get(3); i32_add;
          local_get(0); i32_const(4); i32_add; local_get(3); i32_const(8); i32_mul; i32_add;
          i64_load(0); i32_wrap_i64; i32_store8(0);
          local_get(3); i32_const(1); i32_add; local_set(3);
          br(0);
        end; end;
        local_get(2); end;
    });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

fn compile_to_bytes(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.to_bytes];
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I32), (1, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(1);
        i32_const(4); local_get(1); i32_const(8); i32_mul; i32_add;
        call(emitter.rt.alloc); local_set(2);
        local_get(2); local_get(1); i32_store(0);
        i32_const(0); local_set(3);
        block_empty; loop_empty;
          local_get(3); local_get(1); i32_ge_u; br_if(1);
          local_get(2); i32_const(4); i32_add; local_get(3); i32_const(8); i32_mul; i32_add;
          local_get(0); i32_const(4); i32_add; local_get(3); i32_add;
          i32_load8_u(0); i64_extend_i32_u; i64_store(0);
          local_get(3); i32_const(1); i32_add; local_set(3);
          br(0);
        end; end;
        local_get(2); end;
    });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

