//! WASM runtime functions: bump allocator, println, int_to_string.
//!
//! These are emitted as regular WASM functions, not imports.
//! Only fd_write is imported from WASI.

use super::{CompiledFunc, WasmEmitter, SCRATCH_ITOA, NEWLINE_OFFSET};
use wasm_encoder::{Function, ValType};

/// Register WASI host imports only. Must be called before any register_func.
/// After this, callers may register additional imports (e.g. @extern(wasm))
/// before calling `register_runtime_functions`.
pub fn register_runtime_imports(emitter: &mut WasmEmitter) {
    // fd_write(fd: i32, iovs: i32, iovs_len: i32, nwritten: i32) -> i32
    let fd_write_ty = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.rt.fd_write = emitter.register_import(fd_write_ty);

    // clock_time_get(id: i32, precision: i64, time_ptr: i32) -> i32
    let clock_ty = emitter.register_type(
        vec![ValType::I32, ValType::I64, ValType::I32],
        vec![ValType::I32],
    );
    emitter.rt.clock_time_get = emitter.register_import(clock_ty);

    // proc_exit(code: i32) -> !
    let proc_exit_ty = emitter.register_type(vec![ValType::I32], vec![]);
    emitter.rt.proc_exit = emitter.register_import(proc_exit_ty);

    // random_get(buf: i32, len: i32) -> i32
    let random_get_ty = emitter.register_type(
        vec![ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.rt.random_get = emitter.register_import(random_get_ty);

    // path_open(fd, dirflags, path, path_len, oflags, fs_rights_base, fs_rights_inheriting, fdflags, fd_out) -> errno
    let path_open_ty = emitter.register_type(
        vec![
            ValType::I32, ValType::I32, ValType::I32, ValType::I32,
            ValType::I32, ValType::I64, ValType::I64, ValType::I32,
            ValType::I32,
        ],
        vec![ValType::I32],
    );
    emitter.rt.path_open = emitter.register_import(path_open_ty);

    // fd_read(fd, iovs, iovs_len, nread) -> errno
    let fd_read_ty = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.rt.fd_read = emitter.register_import(fd_read_ty);

    // fd_close(fd) -> errno
    let fd_close_ty = emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
    emitter.rt.fd_close = emitter.register_import(fd_close_ty);

    // fd_seek(fd, offset_i64, whence, new_offset_ptr) -> errno
    let fd_seek_ty = emitter.register_type(
        vec![ValType::I32, ValType::I64, ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.rt.fd_seek = emitter.register_import(fd_seek_ty);

    // fd_filestat_get(fd, buf) -> errno
    let fd_filestat_get_ty = emitter.register_type(
        vec![ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.rt.fd_filestat_get = emitter.register_import(fd_filestat_get_ty);

    // path_filestat_get(fd, flags, path, path_len, buf) -> errno
    let path_filestat_get_ty = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.rt.path_filestat_get = emitter.register_import(path_filestat_get_ty);

    // path_create_directory(fd, path, path_len) -> errno
    let path_create_directory_ty = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.rt.path_create_directory = emitter.register_import(path_create_directory_ty);

    // path_rename(old_fd, old_path, old_path_len, new_fd, new_path, new_path_len) -> errno
    let path_rename_ty = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.rt.path_rename = emitter.register_import(path_rename_ty);

    // path_unlink_file(fd, path, path_len) -> errno
    let path_unlink_file_ty = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.rt.path_unlink_file = emitter.register_import(path_unlink_file_ty);

    // path_remove_directory(fd, path, path_len) -> errno
    let path_remove_directory_ty = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.rt.path_remove_directory = emitter.register_import(path_remove_directory_ty);

    // fd_prestat_get(fd, buf) -> errno
    let fd_prestat_get_ty = emitter.register_type(
        vec![ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.rt.fd_prestat_get = emitter.register_import(fd_prestat_get_ty);

    // fd_prestat_dir_name(fd, path, path_len) -> errno
    let fd_prestat_dir_name_ty = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.rt.fd_prestat_dir_name = emitter.register_import(fd_prestat_dir_name_ty);

    // fd_readdir(fd, buf, buf_len, cookie, bufused_ptr) -> errno
    let fd_readdir_ty = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32, ValType::I64, ValType::I32],
        vec![ValType::I32],
    );
    emitter.rt.fd_readdir = emitter.register_import(fd_readdir_ty);
}

/// Register runtime defined function signatures.
/// Must be called after all imports (WASI + @extern) are registered.
pub fn register_runtime_functions(emitter: &mut WasmEmitter) {
    // __alloc(size: i32) -> i32
    let alloc_ty = emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
    emitter.rt.alloc = emitter.register_func("__alloc", alloc_ty);

    // __heap_save() -> i32   — return current heap pointer
    let heap_save_ty = emitter.register_type(vec![], vec![ValType::I32]);
    emitter.rt.heap_save = emitter.register_func("__heap_save", heap_save_ty);

    // __heap_restore(ptr: i32) -> () — reset heap pointer (frees alloc above ptr)
    let heap_restore_ty = emitter.register_type(vec![ValType::I32], vec![]);
    emitter.rt.heap_restore = emitter.register_func("__heap_restore", heap_restore_ty);

    // __println_str(ptr: i32) -> ()
    let println_ty = emitter.register_type(vec![ValType::I32], vec![]);
    emitter.rt.println_str = emitter.register_func("__println_str", println_ty);

    // __int_to_string(n: i64) -> i32
    let itoa_ty = emitter.register_type(vec![ValType::I64], vec![ValType::I32]);
    // Register under the Rust runtime-fn name so `@intrinsic(...)` lookups
    // resolve directly (Phase 1e-3). Rust side already uses the same
    // symbol, so this is the single-name contract.
    emitter.rt.int_to_string = emitter.register_func("almide_rt_int_to_string", itoa_ty);

    // __float_to_string(f: f64) -> i32
    let ftoa_ty = emitter.register_type(vec![ValType::F64], vec![ValType::I32]);
    emitter.rt.float_to_string = emitter.register_func("__float_to_string", ftoa_ty);

    // __println_int(n: i64) -> ()
    let println_int_ty = emitter.register_type(vec![ValType::I64], vec![]);
    emitter.rt.println_int = emitter.register_func("__println_int", println_int_ty);

    // __concat_str(left: i32, right: i32) -> i32
    let concat_ty = emitter.register_type(vec![ValType::I32, ValType::I32], vec![ValType::I32]);
    emitter.rt.concat_str = emitter.register_func("__concat_str", concat_ty);

    // Note: string interpolation is now emitted inline at the call site
    // (see `calls_string::emit_string_interp`). No scratch runtime helpers.

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

    // __init_preopen_dirs() -> ()
    let init_preopen_ty = emitter.register_type(vec![], vec![]);
    emitter.rt.init_preopen_dirs = emitter.register_func("__init_preopen_dirs", init_preopen_ty);

    // __resolve_path(path_ptr: i32, path_len: i32) -> i32 (result ptr: [fd:i32, rel_ptr:i32, rel_len:i32])
    let resolve_path_ty = emitter.register_type(
        vec![ValType::I32, ValType::I32], vec![ValType::I32],
    );
    emitter.rt.resolve_path = emitter.register_func("__resolve_path", resolve_path_ty);

    // __list_eq(a: i32, b: i32, elem_size: i32) -> i32
    let list_eq_ty = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32], vec![ValType::I32],
    );
    emitter.rt.list_eq = emitter.register_func("__list_eq", list_eq_ty);

    // __list_list_str_cmp(a: i32, b: i32) -> i32 (lexicographic compare of
    // List[String] lists; returns negative / 0 / positive like memcmp).
    let llcmp_ty = emitter.register_type(
        vec![ValType::I32, ValType::I32], vec![ValType::I32],
    );
    emitter.rt.list_list_str_cmp = emitter.register_func("__list_list_str_cmp", llcmp_ty);

    // __concat_list(a: i32, b: i32, elem_size: i32) -> i32
    let concat_list_ty = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32], vec![ValType::I32],
    );
    emitter.rt.concat_list = emitter.register_func("__concat_list", concat_list_ty);

    // __int_parse(s: i32) -> i32 (Result[Int, String])
    let int_parse_ty = emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
    emitter.rt.int_parse = emitter.register_func("almide_rt_int_parse", int_parse_ty);

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
    // __math_log10(x: f64) -> f64  (common logarithm)
    emitter.rt.math_log10 = emitter.register_func("__math_log10", f64_f64_ty);
    // __math_log2(x: f64) -> f64  (binary logarithm)
    emitter.rt.math_log2 = emitter.register_func("__math_log2", f64_f64_ty);
    // __math_exp(x: f64) -> f64  (e^x)
    emitter.rt.math_exp = emitter.register_func("__math_exp", f64_f64_ty);

    // __bytes_f16_to_f64(bits: i32) -> f64  (IEEE-754 half-precision expand)
    let i32_f64_ty = emitter.register_type(vec![ValType::I32], vec![ValType::F64]);
    emitter.rt.bytes_f16_to_f64 = emitter.register_func("__bytes_f16_to_f64", i32_f64_ty);

    // base64 / hex runtime helpers. All take (bytes_or_str_ptr: i32) -> ptr: i32.
    let i32_i32_ty = emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
    emitter.rt.base64_encode = emitter.register_func("__base64_encode", i32_i32_ty);
    emitter.rt.base64_decode = emitter.register_func("__base64_decode", i32_i32_ty);
    emitter.rt.base64_encode_url = emitter.register_func("__base64_encode_url", i32_i32_ty);
    emitter.rt.base64_decode_url = emitter.register_func("__base64_decode_url", i32_i32_ty);
    emitter.rt.hex_encode = emitter.register_func("__hex_encode", i32_i32_ty);
    emitter.rt.hex_encode_upper = emitter.register_func("__hex_encode_upper", i32_i32_ty);
    emitter.rt.hex_decode = emitter.register_func("__hex_decode", i32_i32_ty);

    // String stdlib runtime (delegated to rt_string module)
    super::rt_string::register(emitter);

    // Value/JSON runtime
    super::rt_value::register(emitter);

    // Regex runtime
    super::rt_regex::register(emitter);

    // Global 0: __heap_ptr (memory 0 bump allocator)
    emitter.heap_ptr_global = 0;
    // Global 1: __preopen_table (ptr to heap-allocated table of [fd, path_ptr, path_len] entries)
    emitter.preopen_table_global = 1;
    // Global 2: __preopen_count (number of preopened dirs)
    emitter.preopen_count_global = 2;
}

/// Compile all runtime function bodies.
pub fn compile_runtime(emitter: &mut WasmEmitter) {
    compile_alloc(emitter);
    compile_heap_save(emitter);
    compile_heap_restore(emitter);
    compile_println_str(emitter);
    compile_int_to_string(emitter);
    compile_float_to_string(emitter);
    compile_println_int(emitter);
    compile_concat_str(emitter);
    super::runtime_eq::compile_option_eq_i64(emitter);
    super::runtime_eq::compile_option_eq_str(emitter);
    super::runtime_eq::compile_result_eq_i64_str(emitter);
    super::runtime_eq::compile_mem_eq(emitter);
    compile_init_preopen_dirs(emitter);
    compile_resolve_path(emitter);
    super::runtime_eq::compile_list_eq(emitter);
    super::runtime_eq::compile_list_list_str_cmp(emitter);
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
    super::rt_numeric::compile_math_log10(emitter);
    super::rt_numeric::compile_math_log2(emitter);
    super::rt_numeric::compile_math_exp(emitter);
    compile_bytes_f16_to_f64(emitter);
    // Compile order MUST match registration order in `register_runtime`.
    super::rt_encoding::compile_base64_encode(emitter, /*url_safe=*/false);
    super::rt_encoding::compile_base64_decode(emitter, /*url_safe=*/false);
    super::rt_encoding::compile_base64_encode(emitter, /*url_safe=*/true);
    super::rt_encoding::compile_base64_decode(emitter, /*url_safe=*/true);
    super::rt_encoding::compile_hex_encode(emitter, /*upper=*/false);
    super::rt_encoding::compile_hex_encode(emitter, /*upper=*/true);
    super::rt_encoding::compile_hex_decode(emitter);
    // String stdlib runtime (delegated)
    super::rt_string::compile(emitter);
    // Value/JSON runtime
    super::rt_value::compile(emitter);
    // Regex runtime
    super::rt_regex::compile(emitter);
}

/// __alloc(size: i32) -> i32
/// Bump allocator: returns current heap_ptr (8-byte aligned), then advances by size.
/// All returned pointers are guaranteed to be 8-byte aligned, matching wasi-libc
/// and Emscripten conventions. This ensures i64 loads/stores never trap on alignment.
fn compile_alloc(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.alloc];
    let mut f = Function::new([(1, ValType::I32)]); // local 1: $ptr

    wasm!(f, {
        // Align heap_ptr up to 8-byte boundary: ptr = (heap_ptr + 7) & ~7
        global_get(emitter.heap_ptr_global);
        i32_const(7); i32_add; i32_const(-8); i32_and;
        local_set(1);
        // Advance heap_ptr past the allocation: heap_ptr = ptr + size
        local_get(1);
        local_get(0);
        i32_add;
        global_set(emitter.heap_ptr_global);
        // Grow memory if needed: while heap_ptr > memory.size * 64KB
        block_empty; loop_empty;
          global_get(emitter.heap_ptr_global);
          memory_size(0);
          i32_const(65536); i32_mul;
          i32_le_u;
          br_if(1);
          // Grow by 16 pages (1MB)
          i32_const(16);
          memory_grow(0);
          // If grow failed (-1), trap
          i32_const(-1); i32_eq;
          if_empty; unreachable; end;
          br(0);
        end; end;
        local_get(1);
        end;
    });

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

// __heap_save() -> i32
// Returns the current heap_ptr. Pair with __heap_restore for arena-style
// scoped allocation: save before a sequence of __alloc calls, restore after
// to free everything allocated since the save.
fn compile_heap_save(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.heap_save];
    let mut f = Function::new([]);
    f.instruction(&wasm_encoder::Instruction::GlobalGet(emitter.heap_ptr_global));
    f.instruction(&wasm_encoder::Instruction::End);
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

// __heap_restore(ptr: i32) -> ()
// Resets heap_ptr to the given checkpoint. Pointers allocated above this
// checkpoint become invalid; any view over them must be discarded by the
// caller before invoking restore.
fn compile_heap_restore(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.heap_restore];
    let mut f = Function::new([]);
    f.instruction(&wasm_encoder::Instruction::LocalGet(0));
    f.instruction(&wasm_encoder::Instruction::GlobalSet(emitter.heap_ptr_global));
    f.instruction(&wasm_encoder::Instruction::End);
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

// String builder scratch is gone: `emit_string_interp` now builds the result
// inline (see `calls_string::emit_string_interp`). No runtime helpers and no
// reserved memory region — each interpolation does one heap bump for the
// result and a handful of `memory.copy`s.

/// __init_preopen_dirs() → ()
/// Discovers preopened directories via fd_prestat_get/fd_prestat_dir_name.
/// Builds a heap table: [fd:i32, path_ptr:i32, path_len:i32] per entry.
/// Sets globals: preopen_table (ptr), preopen_count (count).
fn compile_init_preopen_dirs(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.init_preopen_dirs];
    // locals: 0=$fd, 1=$buf(8 bytes for prestat), 2=$errno, 3=$path_len,
    //         4=$count, 5=$table_ptr, 6=$path_buf
    let mut f = Function::new([
        (1, ValType::I32), // 0: $fd
        (1, ValType::I32), // 1: $buf (prestat result: [tag:u8, padding:3, name_len:u32] = 8 bytes)
        (1, ValType::I32), // 2: $errno
        (1, ValType::I32), // 3: $path_len
        (1, ValType::I32), // 4: $count
        (1, ValType::I32), // 5: $table_ptr
        (1, ValType::I32), // 6: $path_buf
    ]);

    wasm!(f, {
        // Allocate prestat buf (8 bytes) and table (max 16 entries × 12 bytes = 192)
        i32_const(8); call(emitter.rt.alloc); local_set(1);
        i32_const(192); call(emitter.rt.alloc); local_set(5);

        // Start from fd=3 (first possible preopened dir)
        i32_const(3); local_set(0);
        i32_const(0); local_set(4);

        // Loop: try fd_prestat_get for each fd until it fails
        block_empty; loop_empty;
        // fd_prestat_get(fd, buf) -> errno
        local_get(0); local_get(1);
        call(emitter.rt.fd_prestat_get);
        local_set(2);

        // If errno != 0, we're done (EBADF = no more preopened dirs)
        local_get(2); i32_const(0); i32_ne;
        br_if(1);

        // Read path_len from prestat buf: offset 4 (after tag byte + padding)
        local_get(1); i32_load(4); local_set(3);

        // Allocate path buffer and get dir name
        local_get(3); i32_const(1); i32_add; call(emitter.rt.alloc); local_set(6);
        local_get(0); local_get(6); local_get(3);
        call(emitter.rt.fd_prestat_dir_name);
        drop;

        // Store entry in table: [fd, path_ptr, path_len]
        local_get(5); local_get(4); i32_const(12); i32_mul; i32_add;
        local_get(0); i32_store(0);
        local_get(5); local_get(4); i32_const(12); i32_mul; i32_add;
        local_get(6); i32_store(4);
        local_get(5); local_get(4); i32_const(12); i32_mul; i32_add;
        local_get(3); i32_store(8);

        // count++, fd++
        local_get(4); i32_const(1); i32_add; local_set(4);
        local_get(0); i32_const(1); i32_add; local_set(0);

        // Max 16 entries
        local_get(4); i32_const(16); i32_ge_u; br_if(1);
        br(0);
        end; end;

        // Set globals
        local_get(5); global_set(emitter.preopen_table_global);
        local_get(4); global_set(emitter.preopen_count_global);

        end;
    });

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// __resolve_path(path_ptr: i32, path_len: i32) → i32 (result_ptr)
/// Result: [fd:i32, rel_path_ptr:i32, rel_path_len:i32] on heap.
/// Finds longest matching preopened dir prefix. Falls back to fd=3 with stripped leading '/'.
fn compile_resolve_path(emitter: &mut WasmEmitter) {
    // Intern "." so we can use its data pointer for exact-match paths
    let dot_str = emitter.intern_string(".");
    let dot_ptr = dot_str + 4; // skip the 4-byte length prefix to get raw '.' byte
    let type_idx = emitter.func_type_indices[&emitter.rt.resolve_path];
    // params: 0=$path_ptr, 1=$path_len
    // locals: 2=$result, 3=$i, 4=$best_fd, 5=$best_match_len,
    //         6=$entry_ptr, 7=$entry_fd, 8=$entry_path_ptr, 9=$entry_path_len,
    //         10=$j, 11=$match
    let mut f = Function::new([
        (1, ValType::I32), // 2: $result
        (1, ValType::I32), // 3: $i
        (1, ValType::I32), // 4: $best_fd
        (1, ValType::I32), // 5: $best_match_len
        (1, ValType::I32), // 6: $entry_ptr
        (1, ValType::I32), // 7: $entry_fd
        (1, ValType::I32), // 8: $entry_path_ptr
        (1, ValType::I32), // 9: $entry_path_len
        (1, ValType::I32), // 10: $j
        (1, ValType::I32), // 11: $match
    ]);

    wasm!(f, {
        // Allocate result: [fd, rel_path_ptr, rel_path_len]
        i32_const(12); call(emitter.rt.alloc); local_set(2);

        // Default: fd=3, no prefix match
        i32_const(3); local_set(4);
        i32_const(0); local_set(5);

        // Loop over preopened dirs to find longest prefix match
        i32_const(0); local_set(3);
        block_empty; loop_empty;
        local_get(3); global_get(emitter.preopen_count_global); i32_ge_u; br_if(1);

        // Load entry [fd, path_ptr, path_len]
        global_get(emitter.preopen_table_global);
        local_get(3); i32_const(12); i32_mul; i32_add;
        local_set(6);
        local_get(6); i32_load(0); local_set(7);
        local_get(6); i32_load(4); local_set(8);
        local_get(6); i32_load(8); local_set(9);

        // Skip if entry_path_len > path_len or entry_path_len <= best_match_len
        local_get(9); local_get(1); i32_gt_u;
        local_get(9); local_get(5); i32_le_u;
        i32_or;
        if_empty;
        else_;

        // Check prefix match: compare entry path bytes with input path bytes
        i32_const(1); local_set(11);
        i32_const(0); local_set(10);
        block_empty; loop_empty;
        local_get(10); local_get(9); i32_ge_u; br_if(1);
        local_get(0); local_get(10); i32_add; i32_load8_u(0);
        local_get(8); local_get(10); i32_add; i32_load8_u(0);
        i32_ne;
        if_empty;
          i32_const(0); local_set(11);
          br(2);
        end;
        local_get(10); i32_const(1); i32_add; local_set(10);
        br(0);
        end; end;

        // If matched, update best
        local_get(11);
        if_empty;
          local_get(7); local_set(4);
          local_get(9); local_set(5);
        end;

        end;

        local_get(3); i32_const(1); i32_add; local_set(3);
        br(0);
        end; end;

        // Build result
        local_get(5); i32_const(0); i32_gt_u;
        if_empty;
          // Prefix match found: strip prefix + optional '/' separator
          local_get(2); local_get(4); i32_store(0);
          local_get(1); local_get(5); i32_sub; i32_const(0); i32_gt_u;
          if_empty;
            local_get(0); local_get(5); i32_add; i32_load8_u(0);
            i32_const(47); i32_eq;
            if_empty;
              local_get(2); local_get(0); local_get(5); i32_add; i32_const(1); i32_add; i32_store(4);
              local_get(2); local_get(1); local_get(5); i32_sub; i32_const(1); i32_sub; i32_store(8);
            else_;
              local_get(2); local_get(0); local_get(5); i32_add; i32_store(4);
              local_get(2); local_get(1); local_get(5); i32_sub; i32_store(8);
            end;
          else_;
            // Exact match (e.g., path="/tmp", preopen="/tmp"): use "." as relative path
            local_get(2); i32_const(dot_ptr as i32); i32_store(4);
            local_get(2); i32_const(1); i32_store(8);
          end;
        else_;
          // No prefix match. For relative paths, find "." preopened dir. For absolute, strip '/'.
          local_get(0); i32_load8_u(0); i32_const(47); i32_eq;
          if_empty;
            // Absolute path with no match: strip '/' and use fd=3
            local_get(2); i32_const(3); i32_store(0);
            local_get(2); local_get(0); i32_const(1); i32_add; i32_store(4);
            local_get(2); local_get(1); i32_const(1); i32_sub; i32_store(8);
          else_;
            // Relative path: find "." in preopened dirs, fallback to fd=3
            local_get(2); i32_const(3); i32_store(0); // default fd=3
            i32_const(0); local_set(3);
            block_empty; loop_empty;
            local_get(3); global_get(emitter.preopen_count_global); i32_ge_u; br_if(1);
            global_get(emitter.preopen_table_global);
            local_get(3); i32_const(12); i32_mul; i32_add;
            local_set(6);
            // Check if entry path is "." (len==1 && byte[0]=='.')
            local_get(6); i32_load(8); i32_const(1); i32_eq;
            if_empty;
              local_get(6); i32_load(4); i32_load8_u(0); i32_const(46); i32_eq;
              if_empty;
                local_get(2); local_get(6); i32_load(0); i32_store(0); // use this fd
                br(3); // break out of search loop
              end;
            end;
            local_get(3); i32_const(1); i32_add; local_set(3);
            br(0);
            end; end;
            // Pass relative path as-is
            local_get(2); local_get(0); i32_store(4);
            local_get(2); local_get(1); i32_store(8);
          end;
        end;

        local_get(2);
        end;
    });

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}


/// __bytes_f16_to_f64(bits: i32) -> f64
///
/// IEEE-754 half-precision expansion. Computes:
///   sign = (bits >> 15) & 1
///   exp  = (bits >> 10) & 0x1f
///   mant = bits & 0x3ff
///   if exp == 0:  sign * mant * 2^-24           (subnormal / zero)
///   if exp == 31: sign * inf  (mant==0) or NaN  (mant!=0)
///   else:         sign * (1 + mant/1024) * 2^(exp-15)
///
/// Implemented with plain WASM math ops — no external float-pow call needed
/// because we can build 2^n with integer shifts into f64 exponent bits.
pub(super) fn compile_bytes_f16_to_f64(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.bytes_f16_to_f64];
    let mut f = Function::new(vec![
        (4, ValType::I32), // locals 1..=4 i32: sign, exp, mant, tmp
        (2, ValType::F64), // locals 5..=6 f64: sign_f, result
    ]);
    wasm!(f, {
        // sign = bits >> 15
        local_get(0); i32_const(15); i32_shr_u; local_set(1);
        // exp = (bits >> 10) & 0x1f
        local_get(0); i32_const(10); i32_shr_u; i32_const(31); i32_and; local_set(2);
        // mant = bits & 0x3ff
        local_get(0); i32_const(1023); i32_and; local_set(3);
        // sign_f = sign ? -1.0 : 1.0
        local_get(1);
        if_f64; f64_const(-1.0);
        else_; f64_const(1.0); end;
        local_set(5);

        // Branch on exp
        local_get(2); i32_eqz;
        if_f64;
            // subnormal: sign_f * mant * 2^-24
            local_get(5);
            local_get(3); f64_convert_i32_u;
            f64_mul;
            f64_const(5.960464477539063e-8); // 2^-24
            f64_mul;
        else_;
            local_get(2); i32_const(31); i32_eq;
            if_f64;
                // inf/nan: return sign-preserving large value for simplicity
                local_get(5); f64_const(3.4028235e38); f64_mul;
            else_;
                // normal: sign_f * (1 + mant/1024) * 2^(exp-15)
                // 2^(exp-15) computed as f64 bit pattern:
                //   f64 exponent bias = 1023, so exp_f64 = exp - 15 + 1023 = exp + 1008
                //   bits = (exp_f64) << 52
                local_get(5);
                f64_const(1.0);
                local_get(3); f64_convert_i32_u;
                f64_const(1024.0); f64_div;
                f64_add;
                f64_mul;
                // Multiply by 2^(exp - 15): construct that power via i64 bit tricks.
                local_get(2); i32_const(1008); i32_add; i64_extend_i32_u;
                i64_const(52); i64_shl;
                f64_reinterpret_i64;
                f64_mul;
            end;
        end;
        end;  // close function body
    });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}
