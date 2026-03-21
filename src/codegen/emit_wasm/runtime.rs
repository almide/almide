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

    // __float_to_string(f: f64) -> i32
    let ftoa_ty = emitter.register_type(vec![ValType::F64], vec![ValType::I32]);
    emitter.rt.float_to_string = emitter.register_func("__float_to_string", ftoa_ty);

    // __println_int(n: i64) -> ()
    let println_int_ty = emitter.register_type(vec![ValType::I64], vec![]);
    emitter.rt.println_int = emitter.register_func("__println_int", println_int_ty);

    // __concat_str(left: i32, right: i32) -> i32
    let concat_ty = emitter.register_type(vec![ValType::I32, ValType::I32], vec![ValType::I32]);
    emitter.rt.concat_str = emitter.register_func("__concat_str", concat_ty);

    // __option_eq_i64(a: i32, b: i32) -> i32
    let opt_eq_i64_ty = emitter.register_type(vec![ValType::I32, ValType::I32], vec![ValType::I32]);
    emitter.rt.option_eq_i64 = emitter.register_func("__option_eq_i64", opt_eq_i64_ty);
    // __option_eq_str(a: i32, b: i32) -> i32
    emitter.rt.option_eq_str = emitter.register_func("__option_eq_str", opt_eq_i64_ty);
    // __result_eq_i64_str(a: i32, b: i32) -> i32
    emitter.rt.result_eq_i64_str = emitter.register_func("__result_eq_i64_str", opt_eq_i64_ty);

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

    // __int_parse(s: i32) -> i32 (Result[Int, String])
    let int_parse_ty = emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
    emitter.rt.int_parse = emitter.register_func("__int_parse", int_parse_ty);

    // __int_from_hex(s: i32) -> i32 (Result[Int, String])
    let int_from_hex_ty = emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
    emitter.rt.int_from_hex = emitter.register_func("__int_from_hex", int_from_hex_ty);

    // __float_parse(s: i32) -> i32 (Result[Float, String]: [tag:i32][f64 or str_ptr:i32] = 12 bytes)
    let float_parse_ty = emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
    emitter.rt.float_parse = emitter.register_func("__float_parse", float_parse_ty);

    // __float_to_fixed(f: f64, decimals: i64) -> i32 (String ptr)
    let float_to_fixed_ty = emitter.register_type(vec![ValType::F64, ValType::I64], vec![ValType::I32]);
    emitter.rt.float_to_fixed = emitter.register_func("__float_to_fixed", float_to_fixed_ty);

    // __float_pow(base: f64, exp: f64) -> f64
    let float_pow_ty = emitter.register_type(vec![ValType::F64, ValType::F64], vec![ValType::F64]);
    emitter.rt.float_pow = emitter.register_func("__float_pow", float_pow_ty);

    // __math_sin(x: f64) -> f64
    let f64_f64_ty = emitter.register_type(vec![ValType::F64], vec![ValType::F64]);
    emitter.rt.math_sin = emitter.register_func("__math_sin", f64_f64_ty);
    // __math_cos(x: f64) -> f64
    emitter.rt.math_cos = emitter.register_func("__math_cos", f64_f64_ty);
    // __math_tan(x: f64) -> f64
    emitter.rt.math_tan = emitter.register_func("__math_tan", f64_f64_ty);
    // __math_log(x: f64) -> f64  (natural logarithm)
    emitter.rt.math_log = emitter.register_func("__math_log", f64_f64_ty);
    // __math_exp(x: f64) -> f64  (e^x)
    emitter.rt.math_exp = emitter.register_func("__math_exp", f64_f64_ty);

    // String stdlib runtime (delegated to rt_string module)
    super::rt_string::register(emitter);

    // Global: __heap_ptr (mutable i32, initialized at assembly time)
    emitter.heap_ptr_global = 0; // first and only global
}

/// Compile all runtime function bodies.
pub fn compile_runtime(emitter: &mut WasmEmitter) {
    compile_alloc(emitter);
    compile_println_str(emitter);
    compile_int_to_string(emitter);
    compile_float_to_string(emitter);
    compile_println_int(emitter);
    compile_concat_str(emitter);
    super::runtime_eq::compile_option_eq_i64(emitter);
    super::runtime_eq::compile_option_eq_str(emitter);
    super::runtime_eq::compile_result_eq_i64_str(emitter);
    super::runtime_eq::compile_mem_eq(emitter);
    super::runtime_eq::compile_list_eq(emitter);
    super::runtime_eq::compile_concat_list(emitter);
    super::runtime_eq::compile_int_parse(emitter);
    super::rt_numeric::compile_int_from_hex(emitter);
    super::rt_numeric::compile_float_parse(emitter);
    super::rt_numeric::compile_float_to_fixed(emitter);
    super::rt_numeric::compile_float_pow(emitter);
    super::rt_numeric::compile_math_sin(emitter);
    super::rt_numeric::compile_math_cos(emitter);
    super::rt_numeric::compile_math_tan(emitter);
    super::rt_numeric::compile_math_log(emitter);
    super::rt_numeric::compile_math_exp(emitter);
    // String stdlib runtime (delegated)
    super::rt_string::compile(emitter);
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

/// __float_to_string(f: f64) -> i32
/// Multi-digit decimal: integer_part + "." + decimal_digits (up to 15, trailing zeros trimmed)
fn compile_float_to_string(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.float_to_string];
    // locals: 0=f64 input | 1=i32 int_str, 2=i32 result, 3=f64 frac, 4=i32 buf, 5=i32 count, 6=i32 digit
    let mut f = Function::new([
        (1, ValType::I32), (1, ValType::I32), (1, ValType::F64),
        (1, ValType::I32), (1, ValType::I32), (1, ValType::I32),
    ]);

    // int_str = int_to_string(trunc(f))
    wasm!(f, {
        local_get(0);
        i64_trunc_f64_s;
        call(emitter.rt.int_to_string);
        local_set(1);
        // frac = abs(f) - abs(trunc(f))
        local_get(0); f64_abs;
        local_get(0); i64_trunc_f64_s; f64_convert_i64_s; f64_abs;
        f64_sub;
        local_set(3);
        // Alloc scratch buffer for decimal digits (max 20)
        i32_const(20); call(emitter.rt.alloc); local_set(4);
        i32_const(0); local_set(5); // count = 0
    });
    // Loop: extract digits while frac > 0 and count < 15
    wasm!(f, {
        block_empty; loop_empty;
          local_get(5); i32_const(15); i32_ge_u; br_if(1);
          // digit = trunc(frac * 10)
          local_get(3); f64_const(10.0); f64_mul; local_set(3);
          local_get(3); i64_trunc_f64_s; i32_wrap_i64; local_set(6);
          // buf[count] = '0' + digit
          local_get(4); local_get(5); i32_add;
          local_get(6); i32_const(48); i32_add;
          i32_store8(0);
          // frac = frac - digit
          local_get(3); local_get(6); i64_extend_i32_u; f64_convert_i64_s; f64_sub; local_set(3);
          local_get(5); i32_const(1); i32_add; local_set(5);
          // Stop if frac is essentially 0
          local_get(3); f64_const(0.000000000000001); f64_lt;
          br_if(1);
          br(0);
        end; end;
    });
    // Ensure at least 1 digit (for "X.0")
    wasm!(f, {
        local_get(5); i32_eqz;
        if_empty;
          local_get(4); i32_const(48); i32_store8(0); // '0'
          i32_const(1); local_set(5);
        end;
    });
    // Trim trailing zeros (but keep at least 1 digit)
    wasm!(f, {
        block_empty; loop_empty;
          local_get(5); i32_const(1); i32_le_u; br_if(1);
          local_get(4); local_get(5); i32_const(1); i32_sub; i32_add;
          i32_load8_u(0);
          i32_const(48); // '0'
          i32_ne; br_if(1);
          local_get(5); i32_const(1); i32_sub; local_set(5);
          br(0);
        end; end;
    });
    // Build frac string from buf[0..count]
    wasm!(f, {
        i32_const(4); local_get(5); i32_add;
        call(emitter.rt.alloc); local_set(2);
        local_get(2); local_get(5); i32_store(0);
        // Copy digits
        i32_const(0); local_set(6);
        block_empty; loop_empty;
          local_get(6); local_get(5); i32_ge_u; br_if(1);
          local_get(2); i32_const(4); i32_add; local_get(6); i32_add;
          local_get(4); local_get(6); i32_add; i32_load8_u(0);
          i32_store8(0);
          local_get(6); i32_const(1); i32_add; local_set(6);
          br(0);
        end; end;
    });
    // Result: int_str + "." + frac_str
    let dot = emitter.intern_string(".");
    wasm!(f, {
        local_get(1);
        i32_const(dot as i32);
        call(emitter.rt.concat_str);
        local_get(2);
        call(emitter.rt.concat_str);
        end;
    });

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
pub(super) fn emit_memcpy_loop(f: &mut Function, dst: u32, src: u32, len: u32, counter: u32, dst_off: u32, src_off: u32) {
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

