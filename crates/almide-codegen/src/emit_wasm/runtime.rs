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

    // Global 0: __heap_ptr (memory 0 bump allocator)
    emitter.heap_ptr_global = 0;
    // Global 1: __preopen_table (ptr to heap-allocated table of [fd, path_ptr, path_len] entries)
    emitter.preopen_table_global = 1;
    // Global 2: __preopen_count (number of preopened dirs)
    emitter.preopen_count_global = 2;
}

/// Global index of the immutable `__heap_start` low-bound (see `next_global`
/// layout note: 0=heap_ptr, 1=free_list, 2=preopen_table, 3=preopen_count,
/// 4=heap_start). Named so the header-guard runtime fns and the assemble-time
/// global declaration share one source of truth.
///
/// NOTE: this is the CORRECT low bound. The legacy `emitter.rt.heap_start_global`
/// field is only assigned (to this value) in `assemble`, AFTER `compile_runtime` —
/// so at runtime-fn compile time it is still 0 (= the moving heap_ptr global). The
/// rc_inc/rc_dec header guard therefore bakes `global.get 0`, which makes them
/// no-ops for every heap pointer (a pure bump-allocate-and-leak model — sound, no
/// frees). `compile_cow_check` deliberately uses THIS constant instead, so its
/// data-section guard is correct independent of that legacy field.
pub const HEAP_START_GLOBAL_IDX: u32 = 4;

/// Compile all runtime function bodies.
pub fn compile_runtime(emitter: &mut WasmEmitter) {
    compile_alloc(emitter);
    compile_rc_inc(emitter);
    compile_rc_dec(emitter);
    compile_cow_check(emitter);
    compile_heap_save(emitter);
    compile_heap_restore(emitter);
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

/// __alloc(size: i32) -> i32
/// Bump allocator: returns current heap_ptr (8-byte aligned), then advances by size.
/// All returned pointers are guaranteed to be 8-byte aligned, matching wasi-libc
/// and Emscripten conventions. This ensures i64 loads/stores never trap on alignment.
fn compile_alloc(emitter: &mut WasmEmitter) {
    use super::engine::{WasmBuilder, layout::*};
    let type_idx = emitter.func_type_indices[&emitter.rt.alloc];
    let hdr = emitter.layout_reg.header_size(ALLOC_HEADER) as i32;
    let rc_off = emitter.layout_reg.fixed_offset(ALLOC_HEADER, alloc::RC);
    let size_off = emitter.layout_reg.fixed_offset(ALLOC_HEADER, alloc::SIZE);
    let rc_ty = emitter.layout_reg.field(ALLOC_HEADER, alloc::RC).ty;
    let size_ty = emitter.layout_reg.field(ALLOC_HEADER, alloc::SIZE).ty;
    let free_list = emitter.free_list_global;
    let heap_ptr = emitter.heap_ptr_global;

    // locals: 0=request_size, 1=ptr, 2=grow_pages, 3=prev, 4=cur
    let mut f = Function::new([
        (1, ValType::I32), (1, ValType::I32),
        (1, ValType::I32), (1, ValType::I32),
    ]);
    {
        let w = &mut WasmBuilder::new(&mut f, &emitter.layout_reg);

        // --- Free list walk ---
        w.i32c(0).set(3);                       // prev = null
        w.gget(free_list).set(4);               // cur = free_list_head
        w.block(|w| { w.loop_(|w| {
            w.get(4).eqz().br_if(1);            // cur == null → bump
            w.get(4).emit_load(size_off, size_ty); // cur.size
            w.get(0).ge_u();                     // >= request_size?
            w.if_void(|w| {
                // Found: unlink
                w.get(3).eqz();
                w.if_void(|w| {
                    // prev == null → cur is head: head = cur.next
                    w.get(4).i32c(hdr).add().emit_load(0, MemType::I32);
                    w.gset(free_list);
                }, |w| {
                    // prev.next = cur.next
                    w.get(3).i32c(hdr).add();
                    w.get(4).i32c(hdr).add().emit_load(0, MemType::I32);
                    w.emit_store(0, MemType::I32);
                });
                // RC = 1
                w.get(4).i32c(1).emit_store(rc_off, rc_ty);
                // Zero-fill reused block's data area to prevent stale data
                // (critical for Swiss Table tag arrays)
                w.get(4).i32c(hdr).add();  // data_ptr
                w.i32c(0);                 // fill value
                w.get(4).emit_load(size_off, size_ty); // size
                w.raw(wasm_encoder::Instruction::MemoryFill(0));
                // Return data ptr
                w.get(4).i32c(hdr).add().ret();
            }, |_| {});
            // Advance: prev = cur, cur = cur.next
            w.get(4).set(3);
            w.get(4).i32c(hdr).add().emit_load(0, MemType::I32).set(4);
            w.br(0);
        }); });

        // --- Bump path ---
        // Align heap_ptr to header boundary
        let align_mask = hdr - 1;       // hdr is power of 2 (8) → mask = 7
        w.gget(heap_ptr).i32c(align_mask).add().i32c(-hdr).and().set(1);
        // Advance: ptr + size + header
        w.get(1).get(0).add().i32c(hdr).add().gset(heap_ptr);
        // Grow memory if needed
        w.gget(heap_ptr);
        w.raw(wasm_encoder::Instruction::I64ExtendI32U);
        w.raw(wasm_encoder::Instruction::I64Const(65535));
        w.raw(wasm_encoder::Instruction::I64Add);
        w.raw(wasm_encoder::Instruction::I64Const(16));
        w.raw(wasm_encoder::Instruction::I64ShrU);
        w.raw(wasm_encoder::Instruction::I32WrapI64);
        w.memory_size().sub().tee(2);
        w.i32c(0);
        w.raw(wasm_encoder::Instruction::I32GtS);
        w.if_void(|w| {
            w.memory_size().get(2);
            w.memory_size().get(2);
            w.gt_u();
            w.raw(wasm_encoder::Instruction::Select);
            w.memory_grow();
            w.i32c(-1).eq();
            w.if_void(|w| { w.unreachable_(); }, |_| {});
        }, |_| {});
        // Write header
        w.get(1).get(0).emit_store(size_off, size_ty);
        w.get(1).i32c(1).emit_store(rc_off, rc_ty);
        // Return data ptr
        w.get(1).i32c(hdr).add();
    }
    f.instruction(&wasm_encoder::Instruction::End);
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

fn compile_rc_inc(emitter: &mut WasmEmitter) {
    use super::engine::WasmBuilder;
    let type_idx = emitter.func_type_indices[&emitter.rt.rc_inc];

    // True no-op: return the pointer untouched. The WASM runtime is a
    // bump-allocate-and-leak model (see HEAP_START_GLOBAL_IDX note): the old
    // header-guard `ptr < global0(heap_ptr)` already returned early for every
    // VALID heap pointer, so the increment below it could only ever execute on
    // a GARBAGE pointer (e.g. a mis-shaped drop reading past a box, #470) —
    // and then it WROTE +1 into not-yet-allocated heap or trapped OOB.
    // Touching memory here is pure downside until real frees land.
    let mut f = Function::new([]);
    {
        let w = &mut WasmBuilder::new(&mut f, &emitter.layout_reg);
        w.get(0);
    }
    f.instruction(&wasm_encoder::Instruction::End);
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

fn compile_rc_dec(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.rc_dec];

    // True no-op (see compile_rc_inc). The old body's decrement/free-list push
    // was unreachable for valid heap pointers (header guard) — it only ever
    // ran on GARBAGE pointers, where it either trapped OOB ([ptr-4] past
    // memory) or silently PUSHED the garbage block onto the free list,
    // poisoning the next __alloc free-list walk (#470's second trap site).
    // When real frees are activated, restore the decrement/push together with
    // the heap_start guard fix and the Perceus aliasing prerequisites (see the
    // wasm-frees roadmap).
    let mut f = Function::new([]);
    f.instruction(&wasm_encoder::Instruction::End);
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

/// __cow_check(ptr) -> ptr. See registration comment in `register_runtime`.
///
/// Returns a FRESH, uniquely-owned copy of the heap object so an in-place mutation
/// of the result is invisible through any other binding that aliased `ptr` (Almide
/// value semantics; only emitted at the mutation sites of `AliasCowPass`-marked
/// vars). The data byte length is read from the alloc header's SIZE field, so the
/// body carries no hardcoded element-size — it works uniformly for List/String/
/// Map/Record/Bytes/variant blocks, all of which __alloc stamps with their size.
///
/// This clones UNCONDITIONALLY (a data-section pointer, which has no header, is the
/// only pass-through). It does NOT branch on the refcount: in the current WASM
/// runtime the rc header guard (rc_inc/rc_dec) is a no-op (a bump-allocate-and-leak
/// model — see `HEAP_START_GLOBAL_IDX`), so the rc never reflects aliasing and a
/// `rc>1` test would never fire. Unconditional clone matches the Rust target's
/// eager `.clone()` at the bind: correct, and the extra copy when the alias is
/// already dead is the accepted, conservative cost of `needs_cow` marking. The
/// original block is left untouched (it leaks like every other block today), so no
/// refcount bookkeeping is needed.
fn compile_cow_check(emitter: &mut WasmEmitter) {
    use super::engine::{WasmBuilder, layout::*};
    let type_idx = emitter.func_type_indices[&emitter.rt.cow_check];
    let size_neg = emitter.layout_reg.alloc_header_neg_offset(alloc::SIZE) as i32;
    let size_ty = emitter.layout_reg.field(ALLOC_HEADER, alloc::SIZE).ty;
    let alloc_fn = emitter.rt.alloc;

    // locals: 1 = $size (data byte count), 2 = $new_ptr
    let mut f = Function::new([(2, ValType::I32)]);
    {
        let w = &mut WasmBuilder::new(&mut f, &emitter.layout_reg);
        // A data-section ptr (below heap_start) has no alloc header → not a heap
        // object → nothing to clone, return as-is. Uses the immutable heap_start
        // global directly (the rt field is still 0 at this compile point).
        w.get(0).gget(HEAP_START_GLOBAL_IDX).lt_u();
        w.if_void(|w| { w.get(0).ret(); }, |_| {});
        // size = header.SIZE; new = alloc(size); memcpy(new, ptr, size); return new.
        w.get(0).i32c(size_neg).sub().emit_load(0, size_ty).set(1);
        w.get(1).call(alloc_fn).set(2);
        w.get(2).get(0).get(1).memory_copy();
        w.get(2); // return the fresh clone
    }
    f.instruction(&wasm_encoder::Instruction::End);
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
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
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

// __heap_restore(ptr: i32) -> ()
// Resets heap_ptr to the given checkpoint. Pointers allocated above this
// checkpoint become invalid; any view over them must be discarded by the
// caller before invoking restore.
fn compile_heap_restore(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.heap_restore];
    let mut f = Function::new([]);
    // Reset heap pointer (no zero-fill — alloc writes refcount header,
    // and Swiss Table init zeroes tags via bump allocator's fresh pages).
    wasm!(f, {
        local_get(0);
        global_set(emitter.heap_ptr_global);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

/// __println_str(ptr: i32)
/// Prints string at ptr ([len:i32][cap:i32][data@8]) followed by newline via WASI fd_write.
fn compile_println_str(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.println_str];
    let mut f = Function::new([]);

    // --- Write the string ---
    // iov[0].buf = ptr + string_data_off()  (skip len+cap header)
    wasm!(f, {
        i32_const(0);
        local_get(0);
        i32_const(emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32);
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

    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
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
    // UNSIGNED rem: `abs_n = 0 - n` produces the correct unsigned magnitude bits
    // even for i64::MIN (0x8000…0 = 2^63), but a SIGNED rem would read those bits
    // as negative and emit bytes below '0'. Unsigned keeps MIN's digits correct.
    f.instruction(&wasm_encoder::Instruction::I64RemU);
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
    // $abs_n /= 10  (UNSIGNED — see the rem note above; keeps i64::MIN correct)
    wasm!(f, {
        local_get(3);
        i64_const(10);
        i64_div_u;
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

    // $result = __alloc(string_hdr() + $len)
    // String layout: [len:i32][cap:i32][data@8]
    wasm!(f, {
        local_get(5);
        i32_const(emitter.layout_reg.header_size(super::engine::layout::STRING) as i32);
        i32_add;
        call(emitter.rt.alloc);
        local_set(6);
    });

    // mem32[$result+0] = $len, mem32[$result+4] = $len (cap = len)
    wasm!(f, {
        local_get(6);
        local_get(5);
        i32_store(0);
        local_get(6);
        local_get(5);
        i32_store(emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::CAP) as i32 as u32, 0);
    });

    // memcpy: copy $len bytes from $start to $result+string_data_off()
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
    // mem[$result + string_data_off() + $i] = mem[$start + $i]
    wasm!(f, {
        local_get(6);
        i32_const(emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32);
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

    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
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

    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
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
        i32_load(0);        // left.len
        local_set(2);
        local_get(1);
        i32_load(0);        // right.len
        local_set(3);
        local_get(2);
        local_get(3);
        i32_add;
        local_set(4);       // new_len = left_len + right_len
        local_get(4);
        i32_const(string_hdr());
        i32_add;
        call(emitter.rt.alloc);
        local_set(5);
        local_get(5);
        local_get(4);
        i32_store(0);       // result.len = new_len
        local_get(5);
        local_get(4);
        i32_store(string_cap_off() as u32); // result.cap = new_len
    });

    // Copy left data: dst=result+DATA_OFFSET, src=left+DATA_OFFSET
    emit_memcpy_loop(&mut f, 5, 0, 2, 6,
        string_data_off() as u32, string_data_off() as u32);

    // Copy right data: dst=result+DATA_OFFSET+left_len, src=right+DATA_OFFSET
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
        i32_const(string_data_off());
        i32_add;
        local_get(2);
        i32_add;
        local_get(6);
        i32_add;
        local_get(1);
        i32_const(string_data_off());
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
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

/// __string_alloc(data_len: i32) -> ptr
/// Allocate a string buffer with header properly initialized:
///   ptr[0] = data_len (len field)
///   ptr[cap_off] = data_len (cap field)
/// Returns pointer to the string header.
/// This eliminates the entire class of "cap not written" bugs.
fn compile_string_alloc(emitter: &mut WasmEmitter) {
    use super::engine::{WasmBuilder, layout::*};
    let type_idx = emitter.func_type_indices[&emitter.rt.string_alloc];
    let hdr = emitter.layout_reg.header_size(STRING) as i32;
    let cap_off = emitter.layout_reg.fixed_offset(STRING, string::CAP);
    let cap_ty = emitter.layout_reg.field(STRING, string::CAP).ty;
    let alloc_fn = emitter.rt.alloc;

    // param 0 = data_len, local 1 = ptr
    let mut f = Function::new([(1, ValType::I32)]);
    {
        let w = &mut WasmBuilder::new(&mut f, &emitter.layout_reg);
        // ptr = alloc(hdr + data_len)
        w.get(0).i32c(hdr).add().call(alloc_fn).set(1);
        // ptr.len = data_len
        w.get(1).get(0).emit_store(0, MemType::I32);
        // ptr.cap = data_len
        w.get(1).get(0).emit_store(cap_off, cap_ty);
        // return ptr
        w.get(1);
    }
    f.instruction(&wasm_encoder::Instruction::End);
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

/// __div_trap(msg_ptr: i32)
/// Integer div/mod abort: write the interned message string at `msg_ptr` — already
/// the full `Error: <msg>\n` text, [len:i32][cap:i32][data@DATA] layout — to stderr
/// via WASI fd_write, then `proc_exit(1)`. The shared trap keeps the div-by-zero and
/// signed-overflow paths a single function call at every emit site.
fn compile_div_trap(emitter: &mut WasmEmitter) {
    // WASI fd for stderr (matches native `eprintln!` → fd 2).
    const STDERR_FD: i32 = 2;
    // Exit code on an aborting integer op (matches native `std::process::exit(1)`).
    const ABORT_EXIT_CODE: i32 = 1;
    // Scratch layout for the single fd_write iovec: [buf:i32@0][len:i32@4], with the
    // returned byte count written at [8]. Mirrors `compile_println_str`.
    const IOV_BUF_OFF: i32 = 0;
    const IOV_LEN_OFF: i32 = 4;
    const NWRITTEN_OFF: i32 = 8;
    const IOV_BASE: i32 = 0;
    const IOV_COUNT: i32 = 1;

    let type_idx = emitter.func_type_indices[&emitter.rt.div_trap];
    let data_off = string_data_off();
    let fd_write = emitter.rt.fd_write;
    let proc_exit = emitter.rt.proc_exit;

    // param 0 = msg_ptr (interned `Error: <msg>\n` string)
    let mut f = Function::new([]);
    // iov[0].buf = msg_ptr + DATA  (skip the len+cap header)
    wasm!(f, {
        i32_const(IOV_BUF_OFF);
        local_get(0);
        i32_const(data_off);
        i32_add;
        i32_store(0);
    });
    // iov[0].len = *msg_ptr  (the byte length, which already includes the newline)
    wasm!(f, {
        i32_const(IOV_LEN_OFF);
        local_get(0);
        i32_load(0);
        i32_store(0);
    });
    // fd_write(stderr, iovs=IOV_BASE, iovs_len=IOV_COUNT, nwritten=NWRITTEN_OFF)
    wasm!(f, {
        i32_const(STDERR_FD);
        i32_const(IOV_BASE);
        i32_const(IOV_COUNT);
        i32_const(NWRITTEN_OFF);
        call(fd_write);
        drop;
    });
    // proc_exit(1) — diverges; never returns.
    wasm!(f, {
        i32_const(ABORT_EXIT_CODE);
        call(proc_exit);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

/// Capacity-aware string append: if left has room, append in-place; else grow 2x.
fn compile_string_append(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string_append];
    // params: 0=$left, 1=$right
    // locals: 2=$left_len, 3=$right_len, 4=$new_len, 5=$left_cap, 6=$result, 7=$i
    let mut f = Function::new([
        (1, ValType::I32), // 2: $left_len
        (1, ValType::I32), // 3: $right_len
        (1, ValType::I32), // 4: $new_len
        (1, ValType::I32), // 5: $left_cap
        (1, ValType::I32), // 6: $result
        (1, ValType::I32), // 7: $i (counter)
    ]);

    wasm!(f, {
        local_get(0); i32_load(0); local_set(2);               // left_len
        local_get(1); i32_load(0); local_set(3);               // right_len
        local_get(2); local_get(3); i32_add; local_set(4);     // new_len
        local_get(0); i32_load(string_cap_off() as u32); local_set(5); // left_cap

        // if left_cap >= new_len: append in-place
        local_get(5); local_get(4); i32_ge_u;
        if_i32;
          // In-place: memory_copy right data after left data
          local_get(0); i32_const(string_data_off()); i32_add; local_get(2); i32_add;
          local_get(1); i32_const(string_data_off()); i32_add;
          local_get(3);
          memory_copy;
          // Update left.len
          local_get(0); local_get(4); i32_store(0);
          local_get(0);  // return left (same pointer)
        else_;
          // Grow: alloc new buffer with cap = max(left_cap*2, new_len)
          local_get(5); i32_const(2); i32_mul; local_set(5); // cap *= 2
          local_get(5); local_get(4); i32_lt_u;
          if_empty; local_get(4); local_set(5); end;          // cap = max(cap*2, new_len)
          // Alloc
          local_get(5); i32_const(string_data_off()); i32_add;
          call(emitter.rt.alloc); local_set(6);
          local_get(6); local_get(4); i32_store(0);           // result.len = new_len
          local_get(6); local_get(5); i32_store(string_cap_off() as u32); // result.cap
          // Copy left data
          local_get(6); i32_const(string_data_off()); i32_add;
          local_get(0); i32_const(string_data_off()); i32_add;
          local_get(2);
          memory_copy;
          // Copy right data
          local_get(6); i32_const(string_data_off()); i32_add; local_get(2); i32_add;
          local_get(1); i32_const(string_data_off()); i32_add;
          local_get(3);
          memory_copy;
          local_get(6);  // return new pointer
        end;
    });
    wasm!(f, { end; });
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
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

    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
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

    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
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
                // exp all-ones: mant==0 → ±inf (sign-preserving), mant!=0 → NaN.
                // Mirrors native f16_bits_to_f64 (runtime/rs/src/bytes.rs): the
                // previous `sign * f32::MAX` was finite and diverged.
                local_get(3); i32_eqz;
                if_f64;
                    local_get(5); f64_const(f64::INFINITY); f64_mul; // ±inf
                else_;
                    f64_const(f64::NAN);
                end;
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
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}
