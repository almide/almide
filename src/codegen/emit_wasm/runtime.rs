//! WASM runtime functions: bump allocator, println, int_to_string.
//!
//! These are emitted as regular WASM functions, not imports.
//! Only fd_write is imported from WASI.

use super::{CompiledFunc, WasmEmitter, SCRATCH_ITOA, NEWLINE_OFFSET};
use wasm_encoder::{Function, ValType};

/// Register WASI imports and runtime function signatures.
pub fn register_runtime(emitter: &mut WasmEmitter) {
    // fd_write(fd: i32, iovs: i32, iovs_len: i32, nwritten: i32) -> i32
    let fd_write_ty = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.rt.fd_write = emitter.register_import(fd_write_ty);

    // __alloc(size: i32) -> i32
    let alloc_ty = emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
    emitter.rt.alloc = emitter.register_func("__alloc", alloc_ty);

    // __println_str(ptr: i32) -> ()
    let println_ty = emitter.register_type(vec![ValType::I32], vec![]);
    emitter.rt.println_str = emitter.register_func("__println_str", println_ty);

    // __int_to_string(n: i64) -> i32
    let itoa_ty = emitter.register_type(vec![ValType::I64], vec![ValType::I32]);
    emitter.rt.int_to_string = emitter.register_func("__int_to_string", itoa_ty);

    // __println_int(n: i64) -> ()
    let println_int_ty = emitter.register_type(vec![ValType::I64], vec![]);
    emitter.rt.println_int = emitter.register_func("__println_int", println_int_ty);

    // __concat_str(left: i32, right: i32) -> i32
    let concat_ty = emitter.register_type(vec![ValType::I32, ValType::I32], vec![ValType::I32]);
    emitter.rt.concat_str = emitter.register_func("__concat_str", concat_ty);

    // __str_eq(a: i32, b: i32) -> i32
    let str_eq_ty = emitter.register_type(vec![ValType::I32, ValType::I32], vec![ValType::I32]);
    emitter.rt.str_eq = emitter.register_func("__str_eq", str_eq_ty);

    // __str_trim(s: i32) -> i32
    let str_trim_ty = emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
    emitter.rt.str_trim = emitter.register_func("__str_trim", str_trim_ty);

    // __option_eq_i64(a: i32, b: i32) -> i32
    let opt_eq_i64_ty = emitter.register_type(vec![ValType::I32, ValType::I32], vec![ValType::I32]);
    emitter.rt.option_eq_i64 = emitter.register_func("__option_eq_i64", opt_eq_i64_ty);
    // __option_eq_str(a: i32, b: i32) -> i32
    emitter.rt.option_eq_str = emitter.register_func("__option_eq_str", opt_eq_i64_ty);
    // __result_eq_i64_str(a: i32, b: i32) -> i32
    emitter.rt.result_eq_i64_str = emitter.register_func("__result_eq_i64_str", opt_eq_i64_ty);

    // __str_contains(haystack: i32, needle: i32) -> i32
    let str_contains_ty = emitter.register_type(vec![ValType::I32, ValType::I32], vec![ValType::I32]);
    emitter.rt.str_contains = emitter.register_func("__str_contains", str_contains_ty);

    // __mem_eq(a: i32, b: i32, size: i32) -> i32
    let mem_eq_ty = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32], vec![ValType::I32],
    );
    emitter.rt.mem_eq = emitter.register_func("__mem_eq", mem_eq_ty);

    // __list_eq(a: i32, b: i32, elem_size: i32) -> i32
    let list_eq_ty = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32], vec![ValType::I32],
    );
    emitter.rt.list_eq = emitter.register_func("__list_eq", list_eq_ty);

    // __concat_list(a: i32, b: i32, elem_size: i32) -> i32
    let concat_list_ty = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32], vec![ValType::I32],
    );
    emitter.rt.concat_list = emitter.register_func("__concat_list", concat_list_ty);

    // Global: __heap_ptr (mutable i32, initialized at assembly time)
    emitter.heap_ptr_global = 0; // first and only global
}

/// Compile all runtime function bodies.
pub fn compile_runtime(emitter: &mut WasmEmitter) {
    compile_alloc(emitter);
    compile_println_str(emitter);
    compile_int_to_string(emitter);
    compile_println_int(emitter);
    compile_concat_str(emitter);
    compile_str_eq(emitter);
    compile_str_trim(emitter);
    compile_option_eq_i64(emitter);
    compile_option_eq_str(emitter);
    compile_result_eq_i64_str(emitter);
    compile_str_contains(emitter);
    compile_mem_eq(emitter);
    compile_list_eq(emitter);
    compile_concat_list(emitter);
}

/// __alloc(size: i32) -> i32
/// Bump allocator: returns current heap_ptr, then advances it by size.
fn compile_alloc(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.alloc];
    let mut f = Function::new([(1, ValType::I32)]); // local 1: $ptr

    wasm!(f, {
        global_get(emitter.heap_ptr_global);
        local_set(1);
        global_get(emitter.heap_ptr_global);
        local_get(0);
        i32_add;
        global_set(emitter.heap_ptr_global);
        local_get(1);
        end;
    });

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// __println_str(ptr: i32)
/// Prints string at ptr ([len:i32][data:u8...]) followed by newline via WASI fd_write.
fn compile_println_str(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.println_str];
    let mut f = Function::new([]);

    // --- Write the string ---
    // iov[0].buf = ptr + 4  (skip length prefix)
    wasm!(f, {
        i32_const(0);
        local_get(0);
        i32_const(4);
        i32_add;
        i32_store(0);
    });
    // iov[0].len = *ptr  (load length)
    wasm!(f, {
        i32_const(4);
        local_get(0);
        i32_load(0);
        i32_store(0);
    });
    // fd_write(stdout=1, iovs=0, iovs_len=1, nwritten=8)
    wasm!(f, {
        i32_const(1);
        i32_const(0);
        i32_const(1);
        i32_const(8);
        call(emitter.rt.fd_write);
        drop;
    });

    // --- Write newline ---
    wasm!(f, {
        i32_const(0);
        i32_const(NEWLINE_OFFSET as i32);
        i32_store(0);
        i32_const(4);
        i32_const(1);
        i32_store(0);
        i32_const(1);
        i32_const(0);
        i32_const(1);
        i32_const(8);
        call(emitter.rt.fd_write);
        drop;
        end;
    });

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// __int_to_string(n: i64) -> i32
/// Converts an i64 to a decimal string on the heap.
/// Uses scratch area [SCRATCH_ITOA..SCRATCH_ITOA+32) for digit buffer.
fn compile_int_to_string(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.int_to_string];
    // Locals: 0=$n (param), 1=$pos, 2=$is_neg, 3=$abs_n(i64), 4=$start, 5=$len, 6=$result, 7=$i
    let mut f = Function::new([
        (1, ValType::I32),  // 1: $pos
        (1, ValType::I32),  // 2: $is_neg
        (1, ValType::I64),  // 3: $abs_n
        (1, ValType::I32),  // 4: $start
        (1, ValType::I32),  // 5: $len
        (1, ValType::I32),  // 6: $result
        (1, ValType::I32),  // 7: $i
    ]);

    let scratch_end = SCRATCH_ITOA + 31;

    // $pos = scratch_end (write backwards from end of scratch buffer)
    wasm!(f, {
        i32_const(scratch_end as i32);
        local_set(1);
    });

    // $is_neg = $n < 0
    f.instruction(&wasm_encoder::Instruction::LocalGet(0));
    f.instruction(&wasm_encoder::Instruction::I64Const(0));
    f.instruction(&wasm_encoder::Instruction::I64LtS);
    wasm!(f, { local_set(2); });

    // $abs_n = if $is_neg then -$n else $n
    wasm!(f, {
        local_get(2);
        if_i64;
        i64_const(0);
        local_get(0);
        i64_sub;
        else_;
        local_get(0);
        end;
        local_set(3);
    });

    // if $abs_n == 0: write '0'
    wasm!(f, {
        local_get(3);
        i64_eqz;
        if_empty;
        local_get(1);
        i32_const(48);
        i32_store8(0);
        local_get(1);
        i32_const(1);
        i32_sub;
        local_set(1);
        else_;
    });
    // while $abs_n > 0: write digits backwards
    wasm!(f, {
        block_empty;
        loop_empty;
        local_get(3);
        i64_eqz;
        br_if(1);
    });
    // mem[$pos] = ($abs_n % 10) + '0'
    wasm!(f, { local_get(1); });
    f.instruction(&wasm_encoder::Instruction::LocalGet(3));
    f.instruction(&wasm_encoder::Instruction::I64Const(10));
    f.instruction(&wasm_encoder::Instruction::I64RemS);
    wasm!(f, {
        i32_wrap_i64;
        i32_const(48);
        i32_add;
        i32_store8(0);
    });
    // $pos -= 1
    wasm!(f, {
        local_get(1);
        i32_const(1);
        i32_sub;
        local_set(1);
    });
    // $abs_n /= 10
    wasm!(f, {
        local_get(3);
        i64_const(10);
        i64_div_s;
        local_set(3);
        br(0);
        end;
        end;
        end;
    });

    // if $is_neg: write '-'
    wasm!(f, {
        local_get(2);
        if_empty;
        local_get(1);
        i32_const(45);
        i32_store8(0);
        local_get(1);
        i32_const(1);
        i32_sub;
        local_set(1);
        end;
    });

    // $start = $pos + 1
    wasm!(f, {
        local_get(1);
        i32_const(1);
        i32_add;
        local_set(4);
    });

    // $len = scratch_end - $pos
    wasm!(f, {
        i32_const(scratch_end as i32);
        local_get(1);
        i32_sub;
        local_set(5);
    });

    // $result = __alloc(4 + $len)
    wasm!(f, {
        local_get(5);
        i32_const(4);
        i32_add;
        call(emitter.rt.alloc);
        local_set(6);
    });

    // mem32[$result] = $len
    wasm!(f, {
        local_get(6);
        local_get(5);
        i32_store(0);
    });

    // memcpy: copy $len bytes from $start to $result+4
    wasm!(f, {
        i32_const(0);
        local_set(7);
        block_empty;
        loop_empty;
        local_get(7);
        local_get(5);
        i32_ge_u;
        br_if(1);
    });
    // mem[$result + 4 + $i] = mem[$start + $i]
    wasm!(f, {
        local_get(6);
        i32_const(4);
        i32_add;
        local_get(7);
        i32_add;
        local_get(4);
        local_get(7);
        i32_add;
        i32_load8_u(0);
        i32_store8(0);
        local_get(7);
        i32_const(1);
        i32_add;
        local_set(7);
        br(0);
        end;
        end;
    });

    // return $result
    wasm!(f, { local_get(6); end; });

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// __println_int(n: i64)
/// Convenience: int_to_string then println_str.
fn compile_println_int(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.println_int];
    let mut f = Function::new([]);

    wasm!(f, {
        local_get(0);
        call(emitter.rt.int_to_string);
        call(emitter.rt.println_str);
        end;
    });

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

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
        i32_load(0);
        local_set(2);
        local_get(1);
        i32_load(0);
        local_set(3);
        local_get(2);
        local_get(3);
        i32_add;
        local_set(4);
        local_get(4);
        i32_const(4);
        i32_add;
        call(emitter.rt.alloc);
        local_set(5);
        local_get(5);
        local_get(4);
        i32_store(0);
    });

    // Copy left data
    emit_memcpy_loop(&mut f, 5, 0, 2, 6, 4, 4);

    // Copy right data: dst=$result+4+$left_len, src=$right+4
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
        i32_const(4);
        i32_add;
        local_get(2);
        i32_add;
        local_get(6);
        i32_add;
        local_get(1);
        i32_const(4);
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
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// Emit a byte-by-byte copy loop: dst[dst_off+i] = src[src_off+i], 0..len
/// Uses local `counter` as loop variable.
fn emit_memcpy_loop(f: &mut Function, dst: u32, src: u32, len: u32, counter: u32, dst_off: u32, src_off: u32) {
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

/// __str_eq(a: i32, b: i32) -> i32
/// Deep string equality: compare lengths then bytes. Returns 1 if equal.
fn compile_str_eq(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.str_eq];
    // params: 0=$a, 1=$b. locals: 2=$len_a, 3=$i
    let mut f = Function::new([
        (1, ValType::I32), // 2: $len_a
        (1, ValType::I32), // 3: $i
    ]);

    // If same pointer, return 1
    wasm!(f, {
        local_get(0);
        local_get(1);
        i32_eq;
        if_empty;
        i32_const(1);
        return_;
        end;
    });

    // Load a.len; if lengths differ return 0
    wasm!(f, {
        local_get(0);
        i32_load(0);
        local_set(2);
        local_get(2);
        local_get(1);
        i32_load(0);
        i32_ne;
        if_empty;
        i32_const(0);
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
    });
    // if a[4+i] != b[4+i] → return 0
    wasm!(f, {
        local_get(0);
        i32_const(4);
        i32_add;
        local_get(3);
        i32_add;
        i32_load8_u(0);
        local_get(1);
        i32_const(4);
        i32_add;
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

/// __str_trim(s: i32) -> i32
/// Strip leading and trailing whitespace (space, tab, newline, CR).
fn compile_str_trim(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.str_trim];
    // params: 0=$s. locals: 1=$len, 2=$start, 3=$end
    let mut f = Function::new([
        (1, ValType::I32), // 1: len
        (1, ValType::I32), // 2: start
        (1, ValType::I32), // 3: end
    ]);

    wasm!(f, {
        local_get(0);
        i32_load(0);
        local_set(1);
        i32_const(0);
        local_set(2);
    });

    // Skip leading whitespace: while start < len && byte < 33
    wasm!(f, {
        block_empty;
        loop_empty;
        local_get(2);
        local_get(1);
        i32_ge_u;
        br_if(1);
        local_get(0);
        i32_const(4);
        i32_add;
        local_get(2);
        i32_add;
        i32_load8_u(0);
        i32_const(33);
        i32_lt_u;
        i32_eqz;
        br_if(1);
        local_get(2);
        i32_const(1);
        i32_add;
        local_set(2);
        br(0);
        end;
        end;
    });

    // end = len
    wasm!(f, {
        local_get(1);
        local_set(3);
    });

    // Skip trailing whitespace
    wasm!(f, {
        block_empty;
        loop_empty;
        local_get(3);
        local_get(2);
        i32_le_u;
        br_if(1);
        local_get(0);
        i32_const(4);
        i32_add;
        local_get(3);
        i32_const(1);
        i32_sub;
        i32_add;
        i32_load8_u(0);
        i32_const(33);
        i32_lt_u;
        i32_eqz;
        br_if(1);
        local_get(3);
        i32_const(1);
        i32_sub;
        local_set(3);
        br(0);
        end;
        end;
    });

    // new_len = end - start. Allocate and copy.
    wasm!(f, {
        local_get(3);
        local_get(2);
        i32_sub;
        i32_const(4);
        i32_add;
        call(emitter.rt.alloc);
        local_set(1);
    });

    // Store new_len at result[0]
    wasm!(f, {
        local_get(1);
        local_get(3);
        local_get(2);
        i32_sub;
        i32_store(0);
    });

    // Copy bytes: i=0; while i < new_len
    wasm!(f, {
        i32_const(0);
        local_set(3);
        block_empty;
        loop_empty;
        local_get(3);
        local_get(1);
        i32_load(0);
        i32_ge_u;
        br_if(1);
        local_get(1);
        i32_const(4);
        i32_add;
        local_get(3);
        i32_add;
        local_get(0);
        i32_const(4);
        i32_add;
        local_get(2);
        i32_add;
        local_get(3);
        i32_add;
        i32_load8_u(0);
        i32_store8(0);
        local_get(3);
        i32_const(1);
        i32_add;
        local_set(3);
        br(0);
        end;
        end;
    });

    wasm!(f, { local_get(1); end; });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// __option_eq_i64(a: i32, b: i32) -> i32
/// Option[Int] equality: none=0, some=ptr to i64.
fn compile_option_eq_i64(emitter: &mut WasmEmitter) {
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
fn compile_option_eq_str(emitter: &mut WasmEmitter) {
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
        call(emitter.rt.str_eq);
        end;
    });

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// __result_eq_i64_str(a: i32, b: i32) -> i32
/// Result[Int, String] equality: compare tags, then ok(i64) or err(str).
fn compile_result_eq_i64_str(emitter: &mut WasmEmitter) {
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
        call(emitter.rt.str_eq);
        end;
    });

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// __str_contains(haystack: i32, needle: i32) -> i32 (bool)
/// O(n*m) substring search.
fn compile_str_contains(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.str_contains];
    let mut f = Function::new([
        (1, ValType::I32), // 2: h_len
        (1, ValType::I32), // 3: n_len
        (1, ValType::I32), // 4: i
        (1, ValType::I32), // 5: j
        (1, ValType::I32), // 6: match flag
    ]);

    wasm!(f, {
        local_get(0);
        i32_load(0);
        local_set(2);
        local_get(1);
        i32_load(0);
        local_set(3);
    });

    // Empty needle → always contains
    wasm!(f, {
        local_get(3);
        i32_eqz;
        if_empty;
        i32_const(1);
        return_;
        end;
    });

    // If needle longer than haystack → false
    wasm!(f, {
        local_get(3);
        local_get(2);
        i32_gt_u;
        if_empty;
        i32_const(0);
        return_;
        end;
    });

    // Outer loop
    wasm!(f, {
        i32_const(0);
        local_set(4);
        block_empty;
        loop_empty;
        local_get(4);
        local_get(2);
        local_get(3);
        i32_sub;
        i32_gt_u;
        if_empty;
        i32_const(0);
        return_;
        end;
    });

    // Compare at position i using mem_eq
    wasm!(f, {
        local_get(0);
        i32_const(4);
        i32_add;
        local_get(4);
        i32_add;
        local_get(1);
        i32_const(4);
        i32_add;
        local_get(3);
        call(emitter.rt.mem_eq);
        if_empty;
        i32_const(1);
        return_;
        end;
        local_get(4);
        i32_const(1);
        i32_add;
        local_set(4);
        br(0);
        end;
        end;
    });

    wasm!(f, { i32_const(0); end; });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// __mem_eq(a: i32, b: i32, size: i32) -> i32
/// Byte-by-byte comparison of two memory regions.
fn compile_mem_eq(emitter: &mut WasmEmitter) {
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
fn compile_list_eq(emitter: &mut WasmEmitter) {
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
fn compile_concat_list(emitter: &mut WasmEmitter) {
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
    emit_memcpy_loop(&mut f, 6, 0, 7, 9, 4, 4);

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
