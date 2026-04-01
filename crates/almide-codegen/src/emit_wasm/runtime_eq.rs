//! WASM runtime: equality, comparison, list ops, and int_parse.
//!
//! Split from runtime.rs for file size. These are all standalone compile_* functions
//! called from `compile_runtime()` in runtime.rs.

use super::{CompiledFunc, WasmEmitter};
use wasm_encoder::{Function, ValType};

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

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
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

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
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

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
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
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
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
        i32_const(4);
        i32_add;
        local_get(5);
        i32_add;
        i32_load8_u(0);
        local_get(1);
        i32_const(4);
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
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// __concat_list(a: i32, b: i32, elem_size: i32) -> i32
/// Concatenate two lists. Layout: [len:i32][data...]. Generic over elem_size.
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
        i32_const(4);
        local_get(7);
        i32_add;
        local_get(8);
        i32_add;
        call(emitter.rt.alloc);
        local_set(6);
        local_get(6);
        local_get(5);
        i32_store(0);
    });

    // Copy a's data
    super::runtime::emit_memcpy_loop(&mut f, 6, 0, 7, 9, 4, 4);

    // Copy b's data: dst=$result+4+$bytes_a, src=$b+4
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
        i32_const(4);
        i32_add;
        local_get(7);
        i32_add;
        local_get(9);
        i32_add;
        local_get(1);
        i32_const(4);
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
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// __int_parse(s: i32) -> i32 (Result[Int, String])
/// Parse string to i64. Returns Result: [tag:i32][value:i64] on heap.
/// tag=0 ok, tag=1 err.
pub(super) fn compile_int_parse(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.int_parse];
    // params: 0=$s
    // locals: 1=$len, 2=$i, 3=$result(i64), 4=$is_neg, 5=$byte, 6=$alloc_ptr
    let mut f = Function::new([
        (1, ValType::I32),  // 1: len
        (1, ValType::I32),  // 2: i
        (1, ValType::I64),  // 3: result
        (1, ValType::I32),  // 4: is_neg
        (1, ValType::I32),  // 5: byte
        (1, ValType::I32),  // 6: alloc_ptr
    ]);

    // len = s.len
    wasm!(f, {
        local_get(0);
        i32_load(0);
        local_set(1);
    });

    // Empty string → err
    wasm!(f, {
        local_get(1);
        i32_eqz;
        if_empty;
    });
    // Return err("empty string")
    let err_str = emitter.intern_string("invalid number");
    wasm!(f, {
        i32_const(12);
        call(emitter.rt.alloc);
        local_set(6);
        local_get(6);
        i32_const(1);
        i32_store(0);
        local_get(6);
        i32_const(err_str as i32);
        i32_store(4);
        local_get(6);
        return_;
        end;
    });

    // i = 0, result = 0, is_neg = 0
    wasm!(f, {
        i32_const(0);
        local_set(2);
        i64_const(0);
        local_set(3);
        i32_const(0);
        local_set(4);
    });

    // Check leading '-'
    wasm!(f, {
        local_get(0);
        i32_load8_u(4);
        i32_const(45);
        i32_eq;
        if_empty;
        i32_const(1);
        local_set(4);
        i32_const(1);
        local_set(2);
        end;
    });

    // Check leading '+'
    wasm!(f, {
        local_get(0);
        i32_load8_u(4);
        i32_const(43);
        i32_eq;
        local_get(4);
        i32_eqz;
        i32_and;
        if_empty;
        i32_const(1);
        local_set(2);
        end;
    });

    // Loop: while i < len
    wasm!(f, {
        block_empty;
        loop_empty;
        local_get(2);
        local_get(1);
        i32_ge_u;
        br_if(1);
    });

    // byte = s[4+i]
    wasm!(f, {
        local_get(0);
        i32_const(4);
        i32_add;
        local_get(2);
        i32_add;
        i32_load8_u(0);
        local_set(5);
    });

    // if byte < '0' || byte > '9' → err
    wasm!(f, {
        local_get(5);
        i32_const(48);
        i32_lt_u;
        local_get(5);
        i32_const(57);
        i32_gt_u;
        i32_or;
        if_empty;
    });
    wasm!(f, {
        i32_const(12);
        call(emitter.rt.alloc);
        local_set(6);
        local_get(6);
        i32_const(1);
        i32_store(0);
        local_get(6);
        i32_const(err_str as i32);
        i32_store(4);
        local_get(6);
        return_;
        end;
    });

    // result = result * 10 + (byte - '0')
    wasm!(f, {
        local_get(3);
        i64_const(10);
        i64_mul;
        local_get(5);
        i32_const(48);
        i32_sub;
        i64_extend_i32_u;
        i64_add;
        local_set(3);
    });

    // i++
    wasm!(f, {
        local_get(2);
        i32_const(1);
        i32_add;
        local_set(2);
        br(0);
        end;
        end;
    });

    // if is_neg: result = -result
    wasm!(f, {
        local_get(4);
        if_empty;
        i64_const(0);
        local_get(3);
        i64_sub;
        local_set(3);
        end;
    });

    // Return ok(result): alloc [tag=0, value=result]
    wasm!(f, {
        i32_const(12);
        call(emitter.rt.alloc);
        local_set(6);
        local_get(6);
        i32_const(0);
        i32_store(0);
        local_get(6);
        local_get(3);
        i64_store(4);
        local_get(6);
        end;
    });

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}
