//! WASM runtime: equality, comparison, list ops, and int_parse.
//!
//! Split from runtime.rs for file size. These are all standalone compile_* functions
//! called from `compile_runtime()` in runtime.rs.

use super::{CompiledFunc, WasmEmitter};
use wasm_encoder::{ValType};
use super::TrackedFunction as Function;

pub(super) fn compile_option_eq_i64(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.option_eq_i64];
    let mut f = Function::new([]);

    // Both none → 1
    wasm!(f, {
        local_get(0);
        i32_eqz;
        local_get(1);
        i32_eqz;
        i32_and;
        if_empty;
        i32_const(1);
        return_;
        end;
    });
    // One none → 0
    wasm!(f, {
        local_get(0);
        i32_eqz;
        local_get(1);
        i32_eqz;
        i32_or;
        if_empty;
        i32_const(0);
        return_;
        end;
    });
    // Both some: compare i64 values
    wasm!(f, {
        local_get(0);
        i64_load(0);
        local_get(1);
        i64_load(0);
        i64_eq;
        end;
    });

    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.option_eq_i64, type_idx, f));
}

/// __option_eq_str(a: i32, b: i32) -> i32
pub(super) fn compile_option_eq_str(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.option_eq_str];
    let mut f = Function::new([]);

    wasm!(f, {
        local_get(0);
        i32_eqz;
        local_get(1);
        i32_eqz;
        i32_and;
        if_empty;
        i32_const(1);
        return_;
        end;
        local_get(0);
        i32_eqz;
        local_get(1);
        i32_eqz;
        i32_or;
        if_empty;
        i32_const(0);
        return_;
        end;
    });
    // Both some: load string ptrs and call str_eq
    wasm!(f, {
        local_get(0);
        i32_load(0);
        local_get(1);
        i32_load(0);
        call(emitter.rt.string.eq);
        end;
    });

    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.option_eq_str, type_idx, f));
}

/// __result_eq_i64_str(a: i32, b: i32) -> i32
/// Result[Int, String] equality: compare tags, then ok(i64) or err(str).
pub(super) fn compile_result_eq_i64_str(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.result_eq_i64_str];
    let mut f = Function::new([]);

    // Compare tags
    wasm!(f, {
        local_get(0);
        i32_load(0);
        local_get(1);
        i32_load(0);
        i32_ne;
        if_empty;
        i32_const(0);
        return_;
        end;
    });
    // Same tag. If tag == 0 (ok): compare i64 at offset 4
    wasm!(f, {
        local_get(0);
        i32_load(0);
        i32_eqz;
        if_empty;
        local_get(0);
        i64_load(4);
        local_get(1);
        i64_load(4);
        i64_eq;
        return_;
        end;
    });
    // tag == 1 (err): compare strings at offset 4
    wasm!(f, {
        local_get(0);
        i32_load(4);
        local_get(1);
        i32_load(4);
        call(emitter.rt.string.eq);
        end;
    });

    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.result_eq_i64_str, type_idx, f));
}

/// __str_contains(haystack: i32, needle: i32) -> i32 (bool)
/// O(n*m) substring search.

pub(super) fn compile_mem_eq(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.mem_eq];
    let mut f = Function::new([(1, ValType::I32)]); // 3: $i

    // Same pointer → equal
    wasm!(f, {
        local_get(0);
        local_get(1);
        i32_eq;
        if_empty;
        i32_const(1);
        return_;
        end;
    });

    // Compare bytes
    wasm!(f, {
        i32_const(0);
        local_set(3);
        block_empty;
        loop_empty;
        local_get(3);
        local_get(2);
        i32_ge_u;
        if_empty;
        i32_const(1);
        return_;
        end;
        local_get(0);
        local_get(3);
        i32_add;
        i32_load8_u(0);
        local_get(1);
        local_get(3);
        i32_add;
        i32_load8_u(0);
        i32_ne;
        if_empty;
        i32_const(0);
        return_;
        end;
        local_get(3);
        i32_const(1);
        i32_add;
        local_set(3);
        br(0);
        end;
        end;
    });

    wasm!(f, { i32_const(0); end; });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.mem_eq, type_idx, f));
}

/// __list_eq(a: i32, b: i32, elem_size: i32) -> i32
/// Compare two lists byte-by-byte. Returns 1 if equal.
pub(super) fn compile_list_eq(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.list_eq];
    let mut f = Function::new([
        (1, ValType::I32), // 3: $len
        (1, ValType::I32), // 4: $total_bytes
        (1, ValType::I32), // 5: $i
    ]);

    // Same pointer → equal
    wasm!(f, {
        local_get(0);
        local_get(1);
        i32_eq;
        if_empty;
        i32_const(1);
        return_;
        end;
    });

    // Compare lengths
    wasm!(f, {
        local_get(0);
        i32_load(0);
        local_set(3);
        local_get(3);
        local_get(1);
        i32_load(0);
        i32_ne;
        if_empty;
        i32_const(0);
        return_;
        end;
    });

    // $total_bytes = $len * $elem_size
    wasm!(f, {
        local_get(3);
        local_get(2);
        i32_mul;
        local_set(4);
    });

    // Compare data bytes
    wasm!(f, {
        i32_const(0);
        local_set(5);
        block_empty;
        loop_empty;
        local_get(5);
        local_get(4);
        i32_ge_u;
        if_empty;
        i32_const(1);
        return_;
        end;
        local_get(0);
        i32_const(emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32);
        i32_add;
        local_get(5);
        i32_add;
        i32_load8_u(0);
        local_get(1);
        i32_const(emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32);
        i32_add;
        local_get(5);
        i32_add;
        i32_load8_u(0);
        i32_ne;
        if_empty;
        i32_const(0);
        return_;
        end;
        local_get(5);
        i32_const(1);
        i32_add;
        local_set(5);
        br(0);
        end;
        end;
    });

    wasm!(f, { i32_const(0); end; });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.list_eq, type_idx, f));
}

/// __list_list_str_cmp(a: i32, b: i32) -> i32
/// Lexicographic comparison of two `List[String]` values. Returns negative if
/// a < b, 0 if equal, positive if a > b (matching `memcmp`/`strcmp`).
pub(super) fn compile_list_list_str_cmp(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.list_list_str_cmp];
    let str_cmp = emitter.rt.string.cmp;
    let mut f = Function::new([
        (1, ValType::I32), // 2: a_len
        (1, ValType::I32), // 3: b_len
        (1, ValType::I32), // 4: min_len
        (1, ValType::I32), // 5: i
        (1, ValType::I32), // 6: c (per-element cmp)
    ]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(2);
        local_get(1); i32_load(0); local_set(3);
        // min_len = min(a_len, b_len)
        local_get(2); local_get(3); i32_lt_u;
        if_i32; local_get(2); else_; local_get(3); end;
        local_set(4);
        // Compare element-by-element (elements are 4-byte string pointers).
        i32_const(0); local_set(5);
        block_empty; loop_empty;
          local_get(5); local_get(4); i32_ge_u; br_if(1);
          local_get(0); i32_const(emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
          local_get(5); i32_const(4); i32_mul; i32_add; i32_load(0);
          local_get(1); i32_const(emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
          local_get(5); i32_const(4); i32_mul; i32_add; i32_load(0);
          call(str_cmp); local_set(6);
          local_get(6); i32_const(0); i32_ne;
          if_empty; local_get(6); return_; end;
          local_get(5); i32_const(1); i32_add; local_set(5);
          br(0);
        end; end;
        // Tie on common prefix: shorter list sorts first.
        local_get(2); local_get(3); i32_sub;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.list_list_str_cmp, type_idx, f));
}

/// __concat_list(a: i32, b: i32, elem_size: i32) -> i32
/// Concatenate two lists. Layout: [len:i32][cap:i32][data...]. Generic over elem_size.
pub(super) fn compile_concat_list(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.concat_list];
    let mut f = Function::new([
        (1, ValType::I32), // 3: $len_a
        (1, ValType::I32), // 4: $len_b
        (1, ValType::I32), // 5: $new_len
        (1, ValType::I32), // 6: $result
        (1, ValType::I32), // 7: $bytes_a
        (1, ValType::I32), // 8: $bytes_b
        (1, ValType::I32), // 9: $i
    ]);

    wasm!(f, {
        local_get(0);
        i32_load(0);
        local_set(3);
        local_get(1);
        i32_load(0);
        local_set(4);
        local_get(3);
        local_get(4);
        i32_add;
        local_set(5);
        local_get(3);
        local_get(2);
        i32_mul;
        local_set(7);
        local_get(4);
        local_get(2);
        i32_mul;
        local_set(8);
        i32_const(emitter.layout_reg.header_size(super::engine::layout::LIST) as i32);
        local_get(7);
        i32_add;
        local_get(8);
        i32_add;
        call(emitter.rt.alloc);
        local_set(6);
        // Store len
        local_get(6);
        local_get(5);
        i32_store(0);
        // Store cap = new_len
        local_get(6);
        local_get(5);
        i32_store(4);
    });

    // Copy a's data
    super::runtime::emit_memcpy_loop(&mut f, 6, 0, 7, 9, 8, 8);

    // Copy b's data: dst=$result+DATA_OFFSET+$bytes_a, src=$b+DATA_OFFSET
    wasm!(f, {
        i32_const(0);
        local_set(9);
        block_empty;
        loop_empty;
        local_get(9);
        local_get(8);
        i32_ge_u;
        br_if(1);
        local_get(6);
        i32_const(emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32);
        i32_add;
        local_get(7);
        i32_add;
        local_get(9);
        i32_add;
        local_get(1);
        i32_const(emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32);
        i32_add;
        local_get(9);
        i32_add;
        i32_load8_u(0);
        i32_store8(0);
        local_get(9);
        i32_const(1);
        i32_add;
        local_set(9);
        br(0);
        end;
        end;
    });

    wasm!(f, { local_get(6); end; });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.concat_list, type_idx, f));
}

/// __int_parse(s: i32) -> i32 (Result[Int, String])
/// Parse string to i64. Returns Result: [tag:i32][value:i64] on heap.
/// tag=0 ok, tag=1 err.
pub(super) fn compile_int_parse(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.int_parse];
    // Byte-for-byte mirror of the native oracle
    //   `s.trim().parse::<i64>().map_err(|e| e.to_string())`
    // (runtime/rs/src/int.rs). The cross-target diff gate compares the Err
    // payload STRING, so the four std error messages must be reproduced exactly:
    //   - empty after trim   → "cannot parse integer from empty string"
    //   - lone sign / nondigit → "invalid digit found in string"
    //   - > i64::MAX          → "number too large to fit in target type"
    //   - < i64::MIN          → "number too small to fit in target type"
    //
    // params: 0=$s (string ptr: [len:i32][cap:i32][data:u8...])
    // locals:
    //   1=len, 2=i (cursor), 3=end (exclusive, after trailing trim),
    //   4=is_neg, 5=byte, 6=alloc_ptr, 7=acc (i64 magnitude, u64 semantics),
    //   8=digit_count, 9=limit (i64, max magnitude for the sign, u64 semantics),
    //   10=tmp (i64 scratch for overflow math)
    let mut f = Function::new([
        (1, ValType::I32),  // 1: len
        (1, ValType::I32),  // 2: i
        (1, ValType::I32),  // 3: end
        (1, ValType::I32),  // 4: is_neg
        (1, ValType::I32),  // 5: byte
        (1, ValType::I32),  // 6: alloc_ptr
        (1, ValType::I64),  // 7: acc (magnitude)
        (1, ValType::I32),  // 8: digit_count
        (1, ValType::I64),  // 9: limit
        (1, ValType::I64),  // 10: tmp
    ]);

    let data_off = emitter.layout_reg.fixed_offset(
        super::engine::layout::STRING, super::engine::layout::string::DATA) as i32;

    // The four distinct std error strings (deduped by intern_string).
    let err_empty = emitter.intern_string("cannot parse integer from empty string");
    let err_digit = emitter.intern_string("invalid digit found in string");
    let err_large = emitter.intern_string("number too large to fit in target type");
    let err_small = emitter.intern_string("number too small to fit in target type");
    let alloc = emitter.rt.alloc;

    // Emit an `err(<interned string>)` return: alloc [tag=1][str_ptr] and return.
    let emit_err = |f: &mut Function, err_str: u32| {
        wasm!(f, {
            i32_const(12);
            call(alloc);
            local_set(6);
            local_get(6); i32_const(1); i32_store(0);              // tag = 1 (err)
            local_get(6); i32_const(err_str as i32); i32_store(4); // err string ptr
            local_get(6);
            return_;
        });
    };

    // Emit `byte = s[data_off + idx_local]`.
    let load_byte = |f: &mut Function, idx_local: u32, dst: u32| {
        wasm!(f, {
            local_get(0); i32_const(data_off); i32_add;
            local_get(idx_local); i32_add;
            i32_load8_u(0); local_set(dst);
        });
    };

    // len = s.len  (byte length lives at the LEN field, offset 0)
    wasm!(f, {
        local_get(0); i32_load(0); local_set(1);
    });

    // Trim leading + trailing Unicode whitespace (matches native s.trim().parse),
    // codepoint-aware via the shared __is_unicode_ws helpers. i=cursor(2), end=3,
    // string ptr=0, scratch q=5.
    wasm!(f, { i32_const(0); local_set(2); });
    super::rt_string::emit_trim_forward(&mut f, emitter, 2, 1);
    wasm!(f, { local_get(1); local_set(3); });
    super::rt_string::emit_trim_backward(&mut f, emitter, 3, 2, 5);

    // ── Empty after trim → "cannot parse integer from empty string" ──
    wasm!(f, { local_get(2); local_get(3); i32_ge_u; if_empty; });
    emit_err(&mut f, err_empty);
    wasm!(f, { end; });

    // ── Optional single leading sign ──
    wasm!(f, { i32_const(0); local_set(4); }); // is_neg = 0
    load_byte(&mut f, 2, 5);
    wasm!(f, {
        local_get(5); i32_const(45); i32_eq; // '-'
        if_empty;
          i32_const(1); local_set(4);
          local_get(2); i32_const(1); i32_add; local_set(2);
        else_;
          local_get(5); i32_const(43); i32_eq; // '+'
          if_empty;
            local_get(2); i32_const(1); i32_add; local_set(2);
          end;
        end;
    });

    // ── No digits after sign → "invalid digit found in string" ──
    wasm!(f, { local_get(2); local_get(3); i32_ge_u; if_empty; });
    emit_err(&mut f, err_digit);
    wasm!(f, { end; });

    // ── limit = is_neg ? 0x8000000000000000 (|i64::MIN|) : i64::MAX ──
    wasm!(f, {
        local_get(4);
        if_empty;
          i64_const(i64::MIN); // bit pattern 0x8000000000000000 == 2^63 unsigned
          local_set(9);
        else_;
          i64_const(i64::MAX);
          local_set(9);
        end;
    });

    // acc = 0, digit_count = 0
    wasm!(f, {
        i64_const(0); local_set(7);
        i32_const(0); local_set(8);
    });

    // ── Main parse loop: while i < end ──
    wasm!(f, { block_empty; loop_empty;
        local_get(2); local_get(3); i32_ge_u; br_if(1);
    });

    // byte = s[i]
    load_byte(&mut f, 2, 5);

    // Non-digit → "invalid digit found in string"
    wasm!(f, {
        local_get(5); i32_const(48); i32_lt_u; // < '0'
        local_get(5); i32_const(57); i32_gt_u; // > '9'
        i32_or;
        if_empty;
    });
    emit_err(&mut f, err_digit);
    wasm!(f, { end; });

    // d = (byte - '0') as i64  →  tmp
    wasm!(f, {
        local_get(5); i32_const(48); i32_sub;
        i64_extend_i32_u; local_set(10);
    });

    // ── Overflow step 1: if acc > limit/10 → overflow (acc*10 already exceeds limit) ──
    // Unsigned `a > b` ≡ !(b >= a), expressed via i64_ge_u + i32_eqz.
    wasm!(f, {
        local_get(9); i64_const(10); i64_div_u; // limit/10
        local_get(7);                            // acc
        i64_ge_u;                                // (limit/10) >= acc  ≡  acc <= limit/10
        i32_eqz;                                 // → acc > limit/10
        if_empty;
    });
    wasm!(f, { local_get(4); if_empty; });
    emit_err(&mut f, err_small);
    wasm!(f, { else_; });
    emit_err(&mut f, err_large);
    wasm!(f, { end; end; });

    // acc = acc * 10  (safe: acc <= limit/10, so acc*10 <= limit < u64::MAX)
    wasm!(f, {
        local_get(7); i64_const(10); i64_mul; local_set(7);
    });

    // ── Overflow step 2: if acc > limit - d → overflow (adding d would exceed) ──
    wasm!(f, {
        local_get(9); local_get(10); i64_sub; // limit - d
        local_get(7);                          // acc
        i64_ge_u;                              // (limit - d) >= acc
        i32_eqz;                               // → acc > limit - d
        if_empty;
    });
    wasm!(f, { local_get(4); if_empty; });
    emit_err(&mut f, err_small);
    wasm!(f, { else_; });
    emit_err(&mut f, err_large);
    wasm!(f, { end; end; });

    // acc = acc + d
    wasm!(f, {
        local_get(7); local_get(10); i64_add; local_set(7);
    });

    // i++, continue
    wasm!(f, {
        local_get(2); i32_const(1); i32_add; local_set(2);
        br(0);
        end; end;
    });

    // ── Materialize signed i64 value ──
    // Negative: value = 0 - acc (two's-complement; wraps exactly to i64::MIN when
    // acc == 2^63). Positive: value = acc (acc <= i64::MAX, fits).
    wasm!(f, {
        local_get(4);
        if_empty;
          i64_const(0); local_get(7); i64_sub; local_set(7);
        end;
    });

    // Return ok(value): alloc [tag=0][value:i64]
    wasm!(f, {
        i32_const(12); call(alloc); local_set(6);
        local_get(6); i32_const(0); i32_store(0); // tag = 0 (ok)
        local_get(6); local_get(7); i64_store(4); // value
        local_get(6);
        end;
    });

    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.int_parse, type_idx, f));
}
