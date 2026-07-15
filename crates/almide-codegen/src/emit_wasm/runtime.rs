//! WASM runtime functions: bump allocator, println, int_to_string.
//!
//! These are emitted as regular WASM functions, not imports.
//! Only fd_write is imported from WASI.

use super::{CompiledFunc, WasmEmitter, SCRATCH_ITOA, NEWLINE_OFFSET};
use super::rt_string::{string_data_off, string_hdr, string_cap_off, list_data_off, list_hdr};
use wasm_encoder::{ValType};
use super::TrackedFunction as Function;

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

    // args_sizes_get(argc_ptr: i32, argv_buf_size_ptr: i32) -> errno
    let args_sizes_get_ty = emitter.register_type(
        vec![ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.rt.args_sizes_get = emitter.register_import(args_sizes_get_ty);

    // args_get(argv_ptr: i32, argv_buf_ptr: i32) -> errno
    let args_get_ty = emitter.register_type(
        vec![ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.rt.args_get = emitter.register_import(args_get_ty);

    // environ_sizes_get(count_ptr: i32, buf_size_ptr: i32) -> errno
    let environ_sizes_get_ty = emitter.register_type(
        vec![ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.rt.environ_sizes_get = emitter.register_import(environ_sizes_get_ty);

    // environ_get(environ_ptr: i32, environ_buf_ptr: i32) -> errno
    let environ_get_ty = emitter.register_type(
        vec![ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.rt.environ_get = emitter.register_import(environ_get_ty);
}

/// Register runtime defined function signatures.
/// Must be called after all imports (WASI + @extern) are registered.
pub fn register_runtime_functions(emitter: &mut WasmEmitter) {
    // __alloc(size: i32) -> i32
    let alloc_ty = emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
    emitter.rt.alloc = emitter.register_func("__alloc", alloc_ty);

    // __rc_inc(ptr: i32) -> i32 — increment refcount at ptr-4, return ptr
    emitter.rt.rc_inc = emitter.register_func("__rc_inc", alloc_ty);

    // __rc_dec(ptr: i32) → () — decrement RC; if 0, push to free list
    let rc_dec_ty = emitter.register_type(vec![ValType::I32], vec![]);
    emitter.rt.rc_dec = emitter.register_func("__rc_dec", rc_dec_ty);

    // __cow_check(ptr: i32) -> i32 — copy-on-write guard for in-place mutation.
    // If rc<=1 (unique owner) returns ptr unchanged; if rc>1 (a live alias exists)
    // allocs a fresh block, memcpys the data, decrements the old rc, and returns the
    // new ptr. The copy length is read from the alloc header's SIZE field (set by
    // __alloc) — so no per-call-site byte-size argument is needed (every collection
    // header records its own data size). This realizes Almide value semantics: a
    // mutation through one binding never reaches another binding aliasing the value.
    let cow_check_ty = emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
    emitter.rt.cow_check = emitter.register_func("__cow_check", cow_check_ty);

    // __heap_save() -> i32   — return current heap pointer
    let heap_save_ty = emitter.register_type(vec![], vec![ValType::I32]);
    emitter.rt.heap_save = emitter.register_func("__heap_save", heap_save_ty);

    // __heap_restore(ptr: i32) -> () — reset heap pointer (frees alloc above ptr)
    let heap_restore_ty = emitter.register_type(vec![ValType::I32], vec![]);
    emitter.rt.heap_restore = emitter.register_func("__heap_restore", heap_restore_ty);

    // __alloc_pinned(size: i32) -> i32 — alloc stamped PINNED_RC (immortal)
    let alloc_pinned_ty = emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
    emitter.rt.alloc_pinned = emitter.register_func("__alloc_pinned", alloc_pinned_ty);

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

    // __string_append(left: i32, right: i32) -> i32
    // Capacity-aware: if left has room, append in-place; else realloc 2x.
    emitter.rt.string_append = emitter.register_func("__string_append", concat_ty);

    // __string_alloc(len) -> ptr: alloc string with len AND cap written.
    let str_alloc_ty = emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
    emitter.rt.string_alloc = emitter.register_func("__string_alloc", str_alloc_ty);

    // __div_trap(msg_ptr: i32) -> () — integer div/mod abort: write the interned
    // `Error: <msg>\n` string to stderr (fd 2) and proc_exit(1).
    let div_trap_ty = emitter.register_type(vec![ValType::I32], vec![]);
    emitter.rt.div_trap = emitter.register_func("__div_trap", div_trap_ty);

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
    emitter.rt.int_from_hex = emitter.register_func("almide_rt_int_parse_hex", int_from_hex_ty);

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

    // Vendored-libm trig helpers (floor/scalbn/rem_pio2[_large]/k_sin/k_cos/k_tan).
    // Registered here so the __math_sin/cos/tan bodies (compiled by rt_numeric in
    // the slots just above) can reference these indices. Compile order below must
    // mirror this registration order (see `compile_runtime`).
    super::rt_libm::register(emitter);

    // __bytes_f16_to_f64(bits: i32) -> f64  (IEEE-754 half-precision expand)
    let i32_f64_ty = emitter.register_type(vec![ValType::I32], vec![ValType::F64]);
    emitter.rt.bytes_f16_to_f64 = emitter.register_func("__bytes_f16_to_f64", i32_f64_ty);

    // base64 / hex runtime helpers. All take (bytes_or_str_ptr: i32) -> ptr: i32.
    let i32_i32_ty = emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
    emitter.rt.base64_encode = emitter.register_func("almide_rt_base64_encode", i32_i32_ty);
    emitter.rt.base64_decode = emitter.register_func("almide_rt_base64_decode", i32_i32_ty);
    emitter.rt.base64_encode_url = emitter.register_func("almide_rt_base64_encode_url", i32_i32_ty);
    emitter.rt.base64_decode_url = emitter.register_func("almide_rt_base64_decode_url", i32_i32_ty);
    emitter.rt.hex_encode = emitter.register_func("almide_rt_hex_encode", i32_i32_ty);
    emitter.rt.hex_encode_upper = emitter.register_func("almide_rt_hex_encode_upper", i32_i32_ty);
    emitter.rt.hex_decode = emitter.register_func("almide_rt_hex_decode", i32_i32_ty);

    // String stdlib runtime (delegated to rt_string module)
    super::rt_string::register(emitter);

    // Value/JSON runtime
    super::rt_value::register(emitter);

    // Regex runtime
    super::rt_regex::register(emitter);

    // Dragon4 big-integer helpers for float.to_string.
    super::rt_dragon::register(emitter);
    // Correctly-rounded decimal→f64 for float.parse (reuses the Dragon4 bignum).
    super::rt_dec2flt::register(emitter);
    // Compound-repr string escape helper (registered last → compiled last so the
    // func-index order matches; see the compile-order note in `compile_runtime`).
    super::rt_repr::register(emitter);
    // Display-form float text for string interpolation (reuses the Dragon4
    // driver). Registered right after rt_repr so the compile order below matches.
    super::rt_float_display::register(emitter);

    // Global index layout — MUST match the emitted global section
    // (0=heap_ptr, 1=free_list, 2=preopen_table, 3=preopen_count,
    // 4=heap_start) and the struct initializers in mod.rs. This block held
    // the PRE-FREES numbering (table=1, count=2) long after free_list took
    // index 1: __init_preopen_dirs wrote the TABLE POINTER into the FREE
    // LIST head, so for every fs-using program the (unconditional) __alloc
    // walk treated the preopen table as a free node from boot — the entire
    // C-042 "op-count ceiling" class (exists×2 OOB, multi-op traps), live
    // in releases for months and self-consistent enough to pass two-op
    // fixtures because __resolve_path read the table back through the SAME
    // stale index.
    emitter.heap_ptr_global = HEAP_PTR_GLOBAL_IDX;
    emitter.preopen_table_global = PREOPEN_TABLE_GLOBAL_IDX;
    emitter.preopen_count_global = PREOPEN_COUNT_GLOBAL_IDX;
}

/// Global index of the immutable `__heap_start` low-bound (see `next_global`
/// layout note: 0=heap_ptr, 1=free_list, 2=preopen_table, 3=preopen_count,
/// 4=heap_start). Named so the header-guard runtime fns and the assemble-time
/// global declaration share one source of truth.
///
/// NOTE: this is the CORRECT low bound. The legacy `emitter.rt.heap_start_global`
/// field is only assigned (to this value) in `assemble`, AFTER `compile_runtime` —
/// so at runtime-fn compile time it is still 0 (= the moving heap_ptr global).
/// Every runtime guard that needs the TRUE heap floor (rc_inc/rc_dec since the
/// frees flip, `compile_cow_check`) uses THIS constant instead of that legacy
/// field.
/// THE wasm global index layout — the single source of truth. The emitted
/// global section, the emitter struct initializers, and every runtime body
/// derive from these. (The preopen pair was previously RE-DEFINED in three
/// places; inserting FREE_LIST at index 1 updated two of them and the third
/// wrote the preopen table pointer into the free-list head for months.)
pub const HEAP_PTR_GLOBAL_IDX: u32 = 0;
pub const FREE_LIST_GLOBAL_IDX: u32 = 1;
pub const PREOPEN_TABLE_GLOBAL_IDX: u32 = 2;
pub const PREOPEN_COUNT_GLOBAL_IDX: u32 = 3;
pub const HEAP_START_GLOBAL_IDX: u32 = 4;

/// Refcount sentinel for PINNED (immortal) blocks — host-written scratch the
/// allocator must never reclaim. A full value, not a bit: unreachable by real
/// counting, distinct from the freed sentinel 0, and both rc_inc and rc_dec
/// early-out on it so it can never increment toward wrap or hit the free path.
pub(super) const PINNED_RC: i32 = i32::MAX;

/// Compile all runtime function bodies.
pub fn compile_runtime(emitter: &mut WasmEmitter) {
    compile_alloc(emitter);
    compile_rc_inc(emitter);
    compile_rc_dec(emitter);
    compile_cow_check(emitter);
    compile_heap_save(emitter);
    compile_heap_restore(emitter);
    compile_alloc_pinned(emitter);
    compile_println_str(emitter);
    compile_int_to_string(emitter);
    // float.to_string driver (Dragon4 shortest-decimal). Registered early, so
    // its body is emitted here; the bignum helpers (registered late) are
    // compiled at the end of this function via `compile_helpers`.
    super::rt_dragon::compile_driver(emitter);
    compile_println_int(emitter);
    compile_concat_str(emitter);
    compile_string_append(emitter);
    compile_string_alloc(emitter);
    compile_div_trap(emitter);
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
    // Vendored-libm trig helper bodies. Compile order MUST match the registration
    // order in `register_runtime` (right after __math_exp).
    super::rt_libm::compile_helpers(emitter);
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
    // Dragon4 bignum helpers (registered last in register_runtime_functions,
    // so their bodies must be emitted last to keep func-index order).
    super::rt_dragon::compile_helpers(emitter);
    // decimal→f64 parser bodies (registered right after the Dragon4 helpers).
    super::rt_dec2flt::compile_helpers(emitter);
    // Compound-repr string escape (registered last in register_runtime, so its
    // body is emitted last to keep func-index order).
    super::rt_repr::compile(emitter);
    // Display-form float text (registered right after rt_repr → compiled here).
    super::rt_float_display::compile(emitter);
}

include!("runtime_p2.rs");
include!("runtime_p3.rs");
