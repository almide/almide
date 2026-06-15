//! WASM runtime functions: bump allocator, println, int_to_string.
//!
//! These are emitted as regular WASM functions, not imports.
//! Only fd_write is imported from WASI.

use crate::emit_wasm::engine::{Imm32, Imm64, Local};
use super::{CompiledFunc, WasmEmitter, SCRATCH_ITOA, NEWLINE_OFFSET};
use super::rt_string::{string_data_off, string_hdr, string_cap_off, list_data_off, list_hdr};
use wasm_encoder::{ValType};
use super::TrackedFunction as Function;

// ---------------------------------------------------------------------------
// Named immediate constants — every non-obvious literal that appears as an
// argument to a WASM const-emit call is named here.  The grouping follows the
// subsystem that owns the meaning; sharing a numeric value across subsystems is
// intentional only when the field/role is truly the same.
// ---------------------------------------------------------------------------

// WASM memory-page arithmetic (used in the bump-allocator grow check).
/// WASM page size in bytes minus one; used to round a byte address up to the
/// next page boundary: `(addr + PAGE_SIZE_MINUS_1) >> PAGE_SHIFT`.
const WASM_PAGE_SIZE_MINUS_1: i64 = 65535;
/// Log₂ of the WASM page size (65536 = 2¹⁶); right-shifting by this converts
/// a byte count to a page count.
const WASM_PAGE_SHIFT: i64 = 16;

// IOV (iovec) scratch layout used by WASI fd_write calls.
// The scratch area at address 0 holds one iovec: [buf:i32@0][len:i32@4],
// and the fd_write nwritten output is written at byte 8.
/// Byte offset of the `len` field inside one iovec record (4 bytes past `buf`).
const IOV_LEN_OFF: i32 = 4;
/// Byte offset at which fd_write writes its `nwritten` result (just past one iovec).
const NWRITTEN_OFF: i32 = 8;

// WASI file-descriptor numbers.
/// First preopened directory fd assigned by the WASI host (fds 0–2 are stdio).
const WASI_FIRST_PREOPEN_FD: i32 = 3;

// Preopened-directory table layout.
/// Size in bytes of one entry in the preopen table: [fd:i32, path_ptr:i32, path_len:i32].
const PREOPEN_ENTRY_SIZE: i32 = 12;
/// Maximum number of preopened directory entries we scan and record.
const PREOPEN_MAX_ENTRIES: i32 = 16;
/// Total byte size of the preallocated preopen table (PREOPEN_MAX_ENTRIES × PREOPEN_ENTRY_SIZE).
const PREOPEN_TABLE_SIZE: i32 = 192; // 16 × 12
/// Size in bytes of the WASI prestat_t buffer used in fd_prestat_get calls.
const PRESTAT_BUF_SIZE: i32 = 8;

// ASCII character codes (used in itoa and path resolution).
/// ASCII code for the digit '0'; used to convert a decimal digit (0..9) to a character.
const ASCII_ZERO: i32 = 48;
/// ASCII code for the minus sign '-'; written as the sign byte in negative integers.
const ASCII_MINUS: i32 = 45;
/// ASCII code for the forward slash '/'; used to detect and strip absolute path prefixes.
const ASCII_SLASH: i32 = 47;
/// ASCII code for the period '.'; used to detect the WASI "." preopened directory.
const ASCII_DOT: i32 = 46;

// Integer-to-decimal conversion.
/// Decimal radix; used for both the remainder and the division in the itoa digit loop.
const DECIMAL_BASE: i64 = 10;

// String capacity growth factor.
/// Multiplier applied to the current capacity when a string buffer must be grown.
const STRING_GROW_FACTOR: i32 = 2;

// IEEE-754 half-precision (f16) bit-field constants.
/// Bit position of the sign bit in a 16-bit f16 word (bit 15).
const F16_SIGN_SHIFT: i32 = 15;
/// Bit position of the low exponent bit in a 16-bit f16 word (bits 14..10).
const F16_EXP_SHIFT: i32 = 10;
/// The 5-bit exponent field value that signals NaN or infinity in f16.
const F16_EXP_ALL_ONES: i32 = 31;
/// 10-bit mantissa mask for f16 (0x3FF = 1023).
const F16_MANTISSA_MASK: i32 = 1023;
/// Bias adjustment to convert an f16 biased exponent (bias 15) to an f64 biased
/// exponent (bias 1023): 1023 − 15 = 1008.  Applied as `exp_f16 + F16_TO_F64_EXP_BIAS_ADJ`
/// before shifting into the f64 exponent field.
const F16_TO_F64_EXP_BIAS_ADJ: i32 = 1008;
/// Number of mantissa bits in an IEEE-754 double (f64); the biased exponent is placed
/// at bit 52 of the 64-bit representation.
const F64_MANTISSA_BITS: i64 = 52;

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

    // __alloc_nozero(size: i32) -> i32 — like __alloc, no zeroing on free-list reuse
    emitter.rt.alloc_nozero = emitter.register_func("__alloc_nozero", alloc_ty);

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
    compile_alloc_nozero(emitter);
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
    let idx = emitter.rt.alloc;
    compile_alloc_variant(emitter, idx, true);
}
/// `__alloc_nozero`: identical to `__alloc` but does NOT zero a reused free-list
/// block. For callers that immediately overwrite the entire data area (matrix
/// transpose / matmul / zeros), the reuse-zeroing in `__alloc` is pure waste —
/// at 512×512 f64 it was ~70% of a transpose's wall time (a fresh 2 MB fill per
/// call). Bump-path blocks come from fresh wasm pages (already zero), so only the
/// reuse path differs.
fn compile_alloc_nozero(emitter: &mut WasmEmitter) {
    let idx = emitter.rt.alloc_nozero;
    compile_alloc_variant(emitter, idx, false);
}
/// Shared body for the `__alloc` family. `zero_on_reuse` controls whether a
/// recycled free-list block has its data area cleared before return.
fn compile_alloc_variant(emitter: &mut WasmEmitter, func_idx: u32, zero_on_reuse: bool) {
    use super::engine::{WasmBuilder, layout::*};
    let type_idx = emitter.func_type_indices[&func_idx];
    let hdr = emitter.layout_reg.header_size(ALLOC_HEADER) as i32;
    let rc_off = emitter.layout_reg.fixed_offset(ALLOC_HEADER, alloc::RC);
    let size_off = emitter.layout_reg.fixed_offset(ALLOC_HEADER, alloc::SIZE);
    let rc_ty = emitter.layout_reg.field(ALLOC_HEADER, alloc::RC).ty;
    let size_ty = emitter.layout_reg.field(ALLOC_HEADER, alloc::SIZE).ty;
    let free_list = emitter.free_list_global;
    let heap_ptr = emitter.heap_ptr_global;

    // locals: 0=request_size, 1=ptr, 2=grow_pages, 3=prev, 4=cur, 5=steps
    let mut f = Function::new([
        (1, ValType::I32), (1, ValType::I32),
        (1, ValType::I32), (1, ValType::I32),
        (1, ValType::I32),
    ]);
    {
        let w = &mut WasmBuilder::new(&mut f, &emitter.layout_reg);

        // --- Free list walk ---
        // The walk carries two tripwires (free when the list is empty, i.e.
        // whenever frees are off): a STEP BOUND that traps on a cycle (a
        // double-free that slipped past the rc sentinel pushes a block twice
        // and links the list to itself — without the bound this loop spins
        // forever, the hang that forced the first activation revert), and a
        // SIZE SANITY check that traps when a node's header was clobbered
        // (e.g. a host-written buffer freed and overwritten — the fs scratch
        // poison class). No free list can have more nodes than 8-byte blocks
        // in the heap, so heap_ptr >> 3 bounds any acyclic walk.
        w.i32c(Imm32(0)).set(Local(3));                       // prev = null
        w.gget(free_list).set(Local(4));               // cur = free_list_head
        w.i32c(Imm32(0)).set(Local(5));                       // steps = 0
        w.block(|w| { w.loop_(|w| {
            w.get(Local(4)).eqz().br_if(1);            // cur == null → bump
            // steps++; steps > cap → cycle → trap. The cap is ABSOLUTE:
            // a heap-derived bound (heap_ptr >> 3) lets a multi-hundred-MB
            // heap walk tens of millions of steps PER ALLOC before tripping —
            // a corrupted cycle then spins the host at 100% CPU for hours
            // instead of trapping (observed killing the dev machine). No sane
            // free list approaches a million nodes in this runtime.
            const FREE_LIST_WALK_CAP: i32 = 1 << 20;
            w.get(Local(5)).i32c(Imm32(1)).add().tee(Local(5));
            w.i32c(Imm32(FREE_LIST_WALK_CAP));
            w.gt_u();
            w.if_void(|w| { w.unreachable_(); }, |_| {});
            // NOTE: a size-sanity bound (cur+hdr+size <= heap_ptr) was tried
            // here and removed: it false-positived on legitimate freed nodes
            // (first churn loop), and the corruption classes it aimed at are
            // covered by the step cap above (cycles), the rc==0 sentinel in
            // rc_dec (double-free), and the rc==0 trap in rc_inc
            // (resurrection). Host-clobbered headers (the fs scratch class)
            // are addressed by construction via pinned allocations.
            w.get(Local(4)).emit_load(size_off, size_ty); // cur.size
            w.get(Local(0)).ge_u();                     // >= request_size?
            w.if_void(|w| {
                // Found: unlink
                w.get(Local(3)).eqz();
                w.if_void(|w| {
                    // prev == null → cur is head: head = cur.next
                    w.get(Local(4)).i32c(Imm32(hdr)).add().emit_load(0, MemType::I32);
                    w.gset(free_list);
                }, |w| {
                    // prev.next = cur.next
                    w.get(Local(3)).i32c(Imm32(hdr)).add();
                    w.get(Local(4)).i32c(Imm32(hdr)).add().emit_load(0, MemType::I32);
                    w.emit_store(0, MemType::I32);
                });
                // RC = 1
                w.get(Local(4)).i32c(Imm32(1)).emit_store(rc_off, rc_ty);
                if zero_on_reuse {
                    // Zero-fill reused block's data area to prevent stale data
                    // (critical for Swiss Table tag arrays). Skipped by
                    // __alloc_nozero, whose callers overwrite the whole block.
                    w.get(Local(4)).i32c(Imm32(hdr)).add();  // data_ptr
                    w.i32c(Imm32(0));                 // fill value
                    w.get(Local(4)).emit_load(size_off, size_ty); // size
                    w.raw(wasm_encoder::Instruction::MemoryFill(0));
                }
                // Return data ptr
                w.get(Local(4)).i32c(Imm32(hdr)).add().ret();
            }, |_| {});
            // Advance: prev = cur, cur = cur.next
            w.get(Local(4)).set(Local(3));
            w.get(Local(4)).i32c(Imm32(hdr)).add().emit_load(0, MemType::I32).set(Local(4));
            w.br(0);
        }); });

        // --- Bump path ---
        // Align heap_ptr to header boundary
        let align_mask = hdr - 1;       // hdr is power of 2 (8) → mask = 7
        w.gget(heap_ptr).i32c(Imm32(align_mask)).add().i32c(Imm32(-hdr)).and().set(Local(1));
        // Advance: ptr + size + header
        w.get(Local(1)).get(Local(0)).add().i32c(Imm32(hdr)).add().gset(heap_ptr);
        // Grow memory if needed
        w.gget(heap_ptr);
        w.raw(wasm_encoder::Instruction::I64ExtendI32U);
        w.i64c(Imm64(WASM_PAGE_SIZE_MINUS_1));
        w.raw(wasm_encoder::Instruction::I64Add);
        w.i64c(Imm64(WASM_PAGE_SHIFT));
        w.raw(wasm_encoder::Instruction::I64ShrU);
        w.raw(wasm_encoder::Instruction::I32WrapI64);
        w.memory_size().sub().tee(Local(2));
        w.i32c(Imm32(0));
        w.raw(wasm_encoder::Instruction::I32GtS);
        w.if_void(|w| {
            w.memory_size().get(Local(2));
            w.memory_size().get(Local(2));
            w.gt_u();
            w.raw(wasm_encoder::Instruction::Select);
            w.memory_grow();
            w.i32c(Imm32(-1)).eq();
            w.if_void(|w| { w.unreachable_(); }, |_| {});
        }, |_| {});
        // Write header
        w.get(Local(1)).get(Local(0)).emit_store(size_off, size_ty);
        w.get(Local(1)).i32c(Imm32(1)).emit_store(rc_off, rc_ty);
        // Return data ptr
        w.get(Local(1)).i32c(Imm32(hdr)).add();
    }
    f.instruction(&wasm_encoder::Instruction::End);
    emitter.add_compiled(CompiledFunc::tracked_for(func_idx, type_idx, f));
}

/// Whether this emission activates real reference-count frees — the DEFAULT
/// since 0.27.0 (the true-Perceus flip: quadruple bar green ×3 — native
/// corpus + wasm corpus both modes + byte gate + churn; see
/// docs/roadmap/active/wasm-frees-ownership-discipline.md and contract
/// C-066). `ALMIDE_WASM_FREES=0` is the opt-out escape hatch back to the
/// bump-allocate-and-leak model. Env-conditional emission is DECLARED
/// behavior: the host-determinism gates pin the environment, and the same
/// env must always produce identical bytes.
pub(super) fn wasm_frees_enabled() -> bool {
    std::env::var_os("ALMIDE_WASM_FREES").is_none_or(|v| v != "0")
}

fn compile_rc_inc(emitter: &mut WasmEmitter) {
    use super::engine::{WasmBuilder, layout::*};
    let type_idx = emitter.func_type_indices[&emitter.rt.rc_inc];

    if !wasm_frees_enabled() {
        // Bump-and-leak model (default): true no-op, return the pointer
        // untouched. The old header-guard `ptr < global0(heap_ptr)` returned
        // early for every VALID heap pointer, so an increment here could only
        // ever execute on a GARBAGE pointer (#470) — touching memory is pure
        // downside while frees are off.
        let mut f = Function::new([]);
        {
            let w = &mut WasmBuilder::new(&mut f, &emitter.layout_reg);
            w.get(Local(0));
        }
        f.instruction(&wasm_encoder::Instruction::End);
        emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.rc_inc, type_idx, f));
        return;
    }

    // ALMIDE_WASM_FREES=1: real reference counting. The guard uses the
    // IMMUTABLE heap_start low bound (HEAP_START_GLOBAL_IDX) — the legacy
    // `emitter.rt.heap_start_global` field is still 0 (= the moving heap_ptr)
    // at compile_runtime time, which is exactly what baked the old body into
    // a no-op for years.
    let rc_neg = emitter.layout_reg.alloc_header_neg_offset(alloc::RC) as i32;
    let rc_ty = emitter.layout_reg.field(ALLOC_HEADER, alloc::RC).ty;
    let heap_start = HEAP_START_GLOBAL_IDX;

    let heap_ptr = emitter.heap_ptr_global;
    let mut f = Function::new([(1, ValType::I32)]); // local 1: $rc
    {
        let w = &mut WasmBuilder::new(&mut f, &emitter.layout_reg);
        // Data-section constants have no header: pass through.
        w.get(Local(0)).gget(heap_start).lt_u();
        w.if_void(|w| { w.get(Local(0)).ret(); }, |_| {});
        // Dead-zone guard: after __heap_restore moved the frontier DOWN, a
        // stale pointer at/above heap_ptr has no live header — touching it
        // would corrupt whatever gets bump-allocated there next. Skip (the
        // leak direction is the safe one).
        w.get(Local(0)).gget(heap_ptr).ge_u();
        w.if_void(|w| { w.get(Local(0)).ret(); }, |_| {});
        // Resurrection tripwire: Inc of a FREED block (rc==0 sentinel) is
        // always a compiler bug — without this trap it silently revives a
        // block already on the free list and the next alloc hands out live
        // memory (observed as silent value corruption, not a crash).
        w.get(Local(0)).i32c(Imm32(rc_neg)).sub().emit_load(0, rc_ty).tee(Local(1));
        w.eqz();
        w.if_void(|w| { w.unreachable_(); }, |_| {});
        // PINNED blocks are immortal: pass through untouched (a +1 would
        // creep the sentinel toward wrap/unpin).
        w.get(Local(1)).i32c(Imm32(PINNED_RC)).eq();
        w.if_void(|w| { w.get(Local(0)).ret(); }, |_| {});
        // *(ptr - rc_neg) = rc + 1
        w.get(Local(0)).i32c(Imm32(rc_neg)).sub();
        w.get(Local(1)).i32c(Imm32(1)).add();
        w.emit_store(0, rc_ty);
        w.get(Local(0)); // return ptr
    }
    f.instruction(&wasm_encoder::Instruction::End);
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.rc_inc, type_idx, f));
}

fn compile_rc_dec(emitter: &mut WasmEmitter) {
    use super::engine::{WasmBuilder, layout::*};
    let type_idx = emitter.func_type_indices[&emitter.rt.rc_dec];

    if !wasm_frees_enabled() {
        // Bump-and-leak model (default): true no-op (see compile_rc_inc).
        let mut f = Function::new([]);
        f.instruction(&wasm_encoder::Instruction::End);
        emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.rc_dec, type_idx, f));
        return;
    }

    // ALMIDE_WASM_FREES=1: real decrement + free-list push, with the
    // DOUBLE-FREE SENTINEL: a freed block is stamped rc=0; a Dec that sees
    // rc==0 traps `unreachable` LOUDLY instead of pushing the block onto the
    // free list a second time — a second push forms a cycle that spins
    // __alloc's walk forever (the silent hang that forced the first revert).
    // __alloc restores rc=1 on reuse, so the sentinel only marks dead blocks.
    let rc_neg = emitter.layout_reg.alloc_header_neg_offset(alloc::RC) as i32;
    let rc_ty = emitter.layout_reg.field(ALLOC_HEADER, alloc::RC).ty;
    let hdr = emitter.layout_reg.header_size(ALLOC_HEADER) as i32;
    let heap_start = HEAP_START_GLOBAL_IDX;
    let free_list = emitter.free_list_global;
    let heap_ptr_g = emitter.heap_ptr_global;

    let mut f = Function::new([(1, ValType::I32)]); // local 1: $rc
    {
        let w = &mut WasmBuilder::new(&mut f, &emitter.layout_reg);
        w.get(Local(0)).gget(heap_start).lt_u();
        w.if_void(|w| { w.ret(); }, |_| {});
        // Dead-zone guard (see compile_rc_inc): a stale pointer at/above the
        // restored bump frontier has no header — freeing it would re-poison
        // the just-reset free list. Skip = bounded leak.
        w.get(Local(0)).gget(heap_ptr_g).ge_u();
        w.if_void(|w| { w.ret(); }, |_| {});
        // rc = *(ptr - rc_neg)
        w.get(Local(0)).i32c(Imm32(rc_neg)).sub().emit_load(0, rc_ty).tee(Local(1));
        // PINNED blocks never free (host-written scratch; see __alloc_pinned).
        w.i32c(Imm32(PINNED_RC)).eq();
        w.if_void(|w| { w.ret(); }, |_| {});
        w.get(Local(1));
        w.i32c(Imm32(1)).gt_u();
        w.if_void(|w| {
            // rc > 1: decrement
            w.get(Local(0)).i32c(Imm32(rc_neg)).sub();
            w.get(Local(1)).i32c(Imm32(1)).sub();
            w.emit_store(0, rc_ty);
        }, |w| {
            // rc <= 1: about to free. Sentinel: rc==0 = already freed → trap.
            w.get(Local(1)).eqz();
            w.if_void(|w| { w.unreachable_(); }, |_| {});
            // Push to free list for reuse (next ptr lives at data[0]).
            w.get(Local(0)).gget(free_list).emit_store(0, MemType::I32);
            w.get(Local(0)).i32c(Imm32(hdr)).sub().gset(free_list);
            // Stamp rc=0 (the sentinel).
            w.get(Local(0)).i32c(Imm32(rc_neg)).sub();
            w.i32c(Imm32(0));
            w.emit_store(0, rc_ty);
        });
    }
    f.instruction(&wasm_encoder::Instruction::End);
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.rc_dec, type_idx, f));
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
        w.get(Local(0)).gget(HEAP_START_GLOBAL_IDX).lt_u();
        w.if_void(|w| { w.get(Local(0)).ret(); }, |_| {});
        // size = header.SIZE; new = alloc(size); memcpy(new, ptr, size); return new.
        w.get(Local(0)).i32c(Imm32(size_neg)).sub().emit_load(0, size_ty).set(Local(1));
        w.get(Local(1)).call(alloc_fn).set(Local(2));
        w.get(Local(2)).get(Local(0)).get(Local(1)).memory_copy();
        w.get(Local(2)); // return the fresh clone
    }
    f.instruction(&wasm_encoder::Instruction::End);
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.cow_check, type_idx, f));
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
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.heap_save, type_idx, f));
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
        local_get(Local(0));
        global_set(emitter.heap_ptr_global);
        // Forget the free list wholesale: nodes above the restored frontier
        // are dead (the walk's size-sanity tripwire traps on them); nodes
        // below are merely un-remembered — optimization loss, never
        // corruption. Unconditional: a no-op while frees are off, so the
        // emitted bytes stay env-independent here.
        i32_const(Imm32(0));
        global_set(emitter.free_list_global);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.heap_restore, type_idx, f));
}

/// `__alloc_pinned(size) -> ptr` — `__alloc` + stamp the rc header with
/// `PINNED_RC`. Used for every buffer a WASI host call writes into
/// (fd_out/stat/iov/nread/data scratch in the fs ops, the preopen tables):
/// such a block on the FREE LIST gets its `next` field overwritten by the
/// host (the field lives in the data area) → poisoned walk → OOB. Pinning
/// removes the entire class by construction; the cost is a bounded,
/// deliberate leak of small per-op scratch. Unconditional stamp — in
/// leak-mode (`ALMIDE_WASM_FREES=0`) rc is inert anyway, and keeping the
/// bytes env-independent here preserves the host-determinism story.
fn compile_alloc_pinned(emitter: &mut WasmEmitter) {
    use super::engine::{WasmBuilder, layout::*};
    let type_idx = emitter.func_type_indices[&emitter.rt.alloc_pinned];
    let rc_neg = emitter.layout_reg.alloc_header_neg_offset(alloc::RC) as i32;
    let rc_ty = emitter.layout_reg.field(ALLOC_HEADER, alloc::RC).ty;
    let mut f = Function::new([(1, ValType::I32)]); // local 1: $ptr
    {
        let w = &mut WasmBuilder::new(&mut f, &emitter.layout_reg);
        w.get(Local(0)).call(emitter.rt.alloc).set(Local(1));
        w.get(Local(1)).i32c(Imm32(rc_neg)).sub();
        w.i32c(Imm32(PINNED_RC));
        w.emit_store(0, rc_ty);
        w.get(Local(1));
    }
    f.instruction(&wasm_encoder::Instruction::End);
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.alloc_pinned, type_idx, f));
}

/// __println_str(ptr: i32)
/// Prints string at ptr ([len:i32][cap:i32][data@8]) followed by newline via WASI fd_write.
fn compile_println_str(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.println_str];
    let mut f = Function::new([]);

    // --- Write the string ---
    // iov[0].buf = ptr + string_data_off()  (skip len+cap header)
    wasm!(f, {
        i32_const(Imm32(0));
        local_get(Local(0));
        i32_const(Imm32(emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32));
        i32_add;
        i32_store(0);
    });
    // iov[0].len = *ptr  (load length)
    wasm!(f, {
        i32_const(Imm32(IOV_LEN_OFF));
        local_get(Local(0));
        i32_load(0);
        i32_store(0);
    });
    // fd_write(stdout=1, iovs=0, iovs_len=1, nwritten=NWRITTEN_OFF)
    wasm!(f, {
        i32_const(Imm32(1));
        i32_const(Imm32(0));
        i32_const(Imm32(1));
        i32_const(Imm32(NWRITTEN_OFF));
        call(emitter.rt.fd_write);
        drop;
    });

    // --- Write newline ---
    wasm!(f, {
        i32_const(Imm32(0));
        i32_const(Imm32(NEWLINE_OFFSET as i32));
        i32_store(0);
        i32_const(Imm32(IOV_LEN_OFF));
        i32_const(Imm32(1));
        i32_store(0);
        i32_const(Imm32(1));
        i32_const(Imm32(0));
        i32_const(Imm32(1));
        i32_const(Imm32(NWRITTEN_OFF));
        call(emitter.rt.fd_write);
        drop;
        end;
    });

    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.println_str, type_idx, f));
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
        i32_const(Imm32(scratch_end as i32));
        local_set(Local(1));
    });

    // $is_neg = $n < 0
    wasm!(f, { local_get(Local(0)); });
    wasm!(f, { i64_const(Imm64(0)); });
    f.instruction(&wasm_encoder::Instruction::I64LtS);
    wasm!(f, { local_set(Local(2)); });

    // $abs_n = if $is_neg then -$n else $n
    wasm!(f, {
        local_get(Local(2));
        if_i64;
        i64_const(Imm64(0));
        local_get(Local(0));
        i64_sub;
        else_;
        local_get(Local(0));
        end;
        local_set(Local(3));
    });

    // if $abs_n == 0: write '0'
    wasm!(f, {
        local_get(Local(3));
        i64_eqz;
        if_empty;
        local_get(Local(1));
        i32_const(Imm32(ASCII_ZERO));
        i32_store8(0);
        local_get(Local(1));
        i32_const(Imm32(1));
        i32_sub;
        local_set(Local(1));
        else_;
    });
    // while $abs_n > 0: write digits backwards
    wasm!(f, {
        block_empty;
        loop_empty;
        local_get(Local(3));
        i64_eqz;
        br_if(1);
    });
    // mem[$pos] = ($abs_n % 10) + '0'
    wasm!(f, { local_get(Local(1)); });
    wasm!(f, { local_get(Local(3)); });
    wasm!(f, { i64_const(Imm64(DECIMAL_BASE)); });
    // UNSIGNED rem: `abs_n = 0 - n` produces the correct unsigned magnitude bits
    // even for i64::MIN (0x8000…0 = 2^63), but a SIGNED rem would read those bits
    // as negative and emit bytes below '0'. Unsigned keeps MIN's digits correct.
    f.instruction(&wasm_encoder::Instruction::I64RemU);
    wasm!(f, {
        i32_wrap_i64;
        i32_const(Imm32(ASCII_ZERO));
        i32_add;
        i32_store8(0);
    });
    // $pos -= 1
    wasm!(f, {
        local_get(Local(1));
        i32_const(Imm32(1));
        i32_sub;
        local_set(Local(1));
    });
    // $abs_n /= 10  (UNSIGNED — see the rem note above; keeps i64::MIN correct)
    wasm!(f, {
        local_get(Local(3));
        i64_const(Imm64(DECIMAL_BASE));
        i64_div_u;
        local_set(Local(3));
        br(0);
        end;
        end;
        end;
    });

    // if $is_neg: write '-'
    wasm!(f, {
        local_get(Local(2));
        if_empty;
        local_get(Local(1));
        i32_const(Imm32(ASCII_MINUS));
        i32_store8(0);
        local_get(Local(1));
        i32_const(Imm32(1));
        i32_sub;
        local_set(Local(1));
        end;
    });

    // $start = $pos + 1
    wasm!(f, {
        local_get(Local(1));
        i32_const(Imm32(1));
        i32_add;
        local_set(Local(4));
    });

    // $len = scratch_end - $pos
    wasm!(f, {
        i32_const(Imm32(scratch_end as i32));
        local_get(Local(1));
        i32_sub;
        local_set(Local(5));
    });

    // $result = __alloc(string_hdr() + $len)
    // String layout: [len:i32][cap:i32][data@8]
    wasm!(f, {
        local_get(Local(5));
        i32_const(Imm32(emitter.layout_reg.header_size(super::engine::layout::STRING) as i32));
        i32_add;
        call(emitter.rt.alloc);
        local_set(Local(6));
    });

    // mem32[$result+0] = $len, mem32[$result+4] = $len (cap = len)
    wasm!(f, {
        local_get(Local(6));
        local_get(Local(5));
        i32_store(0);
        local_get(Local(6));
        local_get(Local(5));
        i32_store(emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::CAP) as i32 as u32, 0);
    });

    // memcpy: copy $len bytes from $start to $result+string_data_off()
    wasm!(f, {
        i32_const(Imm32(0));
        local_set(Local(7));
        block_empty;
        loop_empty;
        local_get(Local(7));
        local_get(Local(5));
        i32_ge_u;
        br_if(1);
    });
    // mem[$result + string_data_off() + $i] = mem[$start + $i]
    wasm!(f, {
        local_get(Local(6));
        i32_const(Imm32(emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32));
        i32_add;
        local_get(Local(7));
        i32_add;
        local_get(Local(4));
        local_get(Local(7));
        i32_add;
        i32_load8_u(0);
        i32_store8(0);
        local_get(Local(7));
        i32_const(Imm32(1));
        i32_add;
        local_set(Local(7));
        br(0);
        end;
        end;
    });

    // return $result
    wasm!(f, { local_get(Local(6)); end; });

    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.int_to_string, type_idx, f));
}

/// __println_int(n: i64)
/// Convenience: int_to_string then println_str.
fn compile_println_int(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.println_int];
    let mut f = Function::new([]);

    wasm!(f, {
        local_get(Local(0));
        call(emitter.rt.int_to_string);
        call(emitter.rt.println_str);
        end;
    });

    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.println_int, type_idx, f));
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
        local_get(Local(0));
        i32_load(0);        // left.len
        local_set(Local(2));
        local_get(Local(1));
        i32_load(0);        // right.len
        local_set(Local(3));
        local_get(Local(2));
        local_get(Local(3));
        i32_add;
        local_set(Local(4));       // new_len = left_len + right_len
        local_get(Local(4));
        i32_const(Imm32(string_hdr()));
        i32_add;
        call(emitter.rt.alloc);
        local_set(Local(5));
        local_get(Local(5));
        local_get(Local(4));
        i32_store(0);       // result.len = new_len
        local_get(Local(5));
        local_get(Local(4));
        i32_store(string_cap_off() as u32); // result.cap = new_len
    });

    // Copy left data: dst=result+DATA_OFFSET, src=left+DATA_OFFSET
    emit_memcpy_loop(&mut f, 5, 0, 2, 6,
        string_data_off() as u32, string_data_off() as u32);

    // Copy right data: dst=result+DATA_OFFSET+left_len, src=right+DATA_OFFSET
    wasm!(f, {
        i32_const(Imm32(0));
        local_set(Local(6));
        block_empty;
        loop_empty;
        local_get(Local(6));
        local_get(Local(3));
        i32_ge_u;
        br_if(1);
        local_get(Local(5));
        i32_const(Imm32(string_data_off()));
        i32_add;
        local_get(Local(2));
        i32_add;
        local_get(Local(6));
        i32_add;
        local_get(Local(1));
        i32_const(Imm32(string_data_off()));
        i32_add;
        local_get(Local(6));
        i32_add;
        i32_load8_u(0);
        i32_store8(0);
        local_get(Local(6));
        i32_const(Imm32(1));
        i32_add;
        local_set(Local(6));
        br(0);
        end;
        end;
    });

    wasm!(f, { local_get(Local(5)); end; });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.concat_str, type_idx, f));
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
        w.get(Local(0)).i32c(Imm32(hdr)).add().call(alloc_fn).set(Local(1));
        // ptr.len = data_len
        w.get(Local(1)).get(Local(0)).emit_store(0, MemType::I32);
        // ptr.cap = data_len
        w.get(Local(1)).get(Local(0)).emit_store(cap_off, cap_ty);
        // return ptr
        w.get(Local(1));
    }
    f.instruction(&wasm_encoder::Instruction::End);
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string_alloc, type_idx, f));
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
        i32_const(Imm32(IOV_BUF_OFF));
        local_get(Local(0));
        i32_const(Imm32(data_off));
        i32_add;
        i32_store(0);
    });
    // iov[0].len = *msg_ptr  (the byte length, which already includes the newline)
    wasm!(f, {
        i32_const(Imm32(IOV_LEN_OFF));
        local_get(Local(0));
        i32_load(0);
        i32_store(0);
    });
    // fd_write(stderr, iovs=IOV_BASE, iovs_len=IOV_COUNT, nwritten=NWRITTEN_OFF)
    wasm!(f, {
        i32_const(Imm32(STDERR_FD));
        i32_const(Imm32(IOV_BASE));
        i32_const(Imm32(IOV_COUNT));
        i32_const(Imm32(NWRITTEN_OFF));
        call(fd_write);
        drop;
    });
    // proc_exit(1) — diverges; never returns.
    wasm!(f, {
        i32_const(Imm32(ABORT_EXIT_CODE));
        call(proc_exit);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.div_trap, type_idx, f));
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
        local_get(Local(0)); i32_load(0); local_set(Local(2));               // left_len
        local_get(Local(1)); i32_load(0); local_set(Local(3));               // right_len
        local_get(Local(2)); local_get(Local(3)); i32_add; local_set(Local(4));     // new_len
        local_get(Local(0)); i32_load(string_cap_off() as u32); local_set(Local(5)); // left_cap

        // if left_cap >= new_len: append in-place
        local_get(Local(5)); local_get(Local(4)); i32_ge_u;
        if_i32;
          // In-place: memory_copy right data after left data
          local_get(Local(0)); i32_const(Imm32(string_data_off())); i32_add; local_get(Local(2)); i32_add;
          local_get(Local(1)); i32_const(Imm32(string_data_off())); i32_add;
          local_get(Local(3));
          memory_copy;
          // Update left.len
          local_get(Local(0)); local_get(Local(4)); i32_store(0);
          local_get(Local(0));  // return left (same pointer)
        else_;
          // Grow: alloc new buffer with cap = max(left_cap*2, new_len)
          local_get(Local(5)); i32_const(Imm32(STRING_GROW_FACTOR)); i32_mul; local_set(Local(5)); // cap *= 2
          local_get(Local(5)); local_get(Local(4)); i32_lt_u;
          if_empty; local_get(Local(4)); local_set(Local(5)); end;          // cap = max(cap*2, new_len)
          // Alloc
          local_get(Local(5)); i32_const(Imm32(string_data_off())); i32_add;
          call(emitter.rt.alloc); local_set(Local(6));
          local_get(Local(6)); local_get(Local(4)); i32_store(0);           // result.len = new_len
          local_get(Local(6)); local_get(Local(5)); i32_store(string_cap_off() as u32); // result.cap
          // Copy left data
          local_get(Local(6)); i32_const(Imm32(string_data_off())); i32_add;
          local_get(Local(0)); i32_const(Imm32(string_data_off())); i32_add;
          local_get(Local(2));
          memory_copy;
          // Copy right data
          local_get(Local(6)); i32_const(Imm32(string_data_off())); i32_add; local_get(Local(2)); i32_add;
          local_get(Local(1)); i32_const(Imm32(string_data_off())); i32_add;
          local_get(Local(3));
          memory_copy;
          local_get(Local(6));  // return new pointer
        end;
    });
    wasm!(f, { end; });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string_append, type_idx, f));
}

/// Emit a byte-by-byte copy loop: dst[dst_off+i] = src[src_off+i], 0..len
/// Uses local `counter` as loop variable.
pub(super) fn emit_memcpy_loop(f: &mut Function, dst: u32, src: u32, len: u32, counter: u32, dst_off: u32, src_off: u32) {
    wasm!(f, {
        i32_const(Imm32(0));
        local_set(Local(counter));
        block_empty;
        loop_empty;
        local_get(Local(counter));
        local_get(Local(len));
        i32_ge_u;
        br_if(1);
        local_get(Local(dst));
        i32_const(Imm32(dst_off as i32));
        i32_add;
        local_get(Local(counter));
        i32_add;
        local_get(Local(src));
        i32_const(Imm32(src_off as i32));
        i32_add;
        local_get(Local(counter));
        i32_add;
        i32_load8_u(0);
        i32_store8(0);
        local_get(Local(counter));
        i32_const(Imm32(1));
        i32_add;
        local_set(Local(counter));
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
        // Allocate prestat buf (PRESTAT_BUF_SIZE bytes) and table (PREOPEN_TABLE_SIZE bytes)
        i32_const(Imm32(PRESTAT_BUF_SIZE)); call(emitter.rt.alloc_pinned); local_set(Local(1));
        i32_const(Imm32(PREOPEN_TABLE_SIZE)); call(emitter.rt.alloc_pinned); local_set(Local(5));

        // Start from fd=WASI_FIRST_PREOPEN_FD (first possible preopened dir)
        i32_const(Imm32(WASI_FIRST_PREOPEN_FD)); local_set(Local(0));
        i32_const(Imm32(0)); local_set(Local(4));

        // Loop: try fd_prestat_get for each fd until it fails
        block_empty; loop_empty;
        // fd_prestat_get(fd, buf) -> errno
        local_get(Local(0)); local_get(Local(1));
        call(emitter.rt.fd_prestat_get);
        local_set(Local(2));

        // If errno != 0, we're done (EBADF = no more preopened dirs)
        local_get(Local(2)); i32_const(Imm32(0)); i32_ne;
        br_if(1);

        // Read path_len from prestat buf: offset 4 (after tag byte + padding)
        local_get(Local(1)); i32_load(4); local_set(Local(3));

        // Allocate path buffer and get dir name
        local_get(Local(3)); i32_const(Imm32(1)); i32_add; call(emitter.rt.alloc_pinned); local_set(Local(6));
        local_get(Local(0)); local_get(Local(6)); local_get(Local(3));
        call(emitter.rt.fd_prestat_dir_name);
        drop;

        // Store entry in table: [fd, path_ptr, path_len]
        local_get(Local(5)); local_get(Local(4)); i32_const(Imm32(PREOPEN_ENTRY_SIZE)); i32_mul; i32_add;
        local_get(Local(0)); i32_store(0);
        local_get(Local(5)); local_get(Local(4)); i32_const(Imm32(PREOPEN_ENTRY_SIZE)); i32_mul; i32_add;
        local_get(Local(6)); i32_store(4);
        local_get(Local(5)); local_get(Local(4)); i32_const(Imm32(PREOPEN_ENTRY_SIZE)); i32_mul; i32_add;
        local_get(Local(3)); i32_store(8);

        // count++, fd++
        local_get(Local(4)); i32_const(Imm32(1)); i32_add; local_set(Local(4));
        local_get(Local(0)); i32_const(Imm32(1)); i32_add; local_set(Local(0));

        // Max PREOPEN_MAX_ENTRIES entries
        local_get(Local(4)); i32_const(Imm32(PREOPEN_MAX_ENTRIES)); i32_ge_u; br_if(1);
        br(0);
        end; end;

        // Set globals
        local_get(Local(5)); global_set(emitter.preopen_table_global);
        local_get(Local(4)); global_set(emitter.preopen_count_global);

        end;
    });

    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.init_preopen_dirs, type_idx, f));
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
        i32_const(Imm32(PREOPEN_ENTRY_SIZE)); call(emitter.rt.alloc_pinned); local_set(Local(2));

        // Default: fd=WASI_FIRST_PREOPEN_FD, no prefix match
        i32_const(Imm32(WASI_FIRST_PREOPEN_FD)); local_set(Local(4));
        i32_const(Imm32(0)); local_set(Local(5));

        // Loop over preopened dirs to find longest prefix match
        i32_const(Imm32(0)); local_set(Local(3));
        block_empty; loop_empty;
        local_get(Local(3)); global_get(emitter.preopen_count_global); i32_ge_u; br_if(1);

        // Load entry [fd, path_ptr, path_len]
        global_get(emitter.preopen_table_global);
        local_get(Local(3)); i32_const(Imm32(PREOPEN_ENTRY_SIZE)); i32_mul; i32_add;
        local_set(Local(6));
        local_get(Local(6)); i32_load(0); local_set(Local(7));
        local_get(Local(6)); i32_load(4); local_set(Local(8));
        local_get(Local(6)); i32_load(8); local_set(Local(9));

        // Skip if entry_path_len > path_len or entry_path_len <= best_match_len
        local_get(Local(9)); local_get(Local(1)); i32_gt_u;
        local_get(Local(9)); local_get(Local(5)); i32_le_u;
        i32_or;
        if_empty;
        else_;

        // Check prefix match: compare entry path bytes with input path bytes
        i32_const(Imm32(1)); local_set(Local(11));
        i32_const(Imm32(0)); local_set(Local(10));
        block_empty; loop_empty;
        local_get(Local(10)); local_get(Local(9)); i32_ge_u; br_if(1);
        local_get(Local(0)); local_get(Local(10)); i32_add; i32_load8_u(0);
        local_get(Local(8)); local_get(Local(10)); i32_add; i32_load8_u(0);
        i32_ne;
        if_empty;
          i32_const(Imm32(0)); local_set(Local(11));
          br(2);
        end;
        local_get(Local(10)); i32_const(Imm32(1)); i32_add; local_set(Local(10));
        br(0);
        end; end;

        // If matched, update best
        local_get(Local(11));
        if_empty;
          local_get(Local(7)); local_set(Local(4));
          local_get(Local(9)); local_set(Local(5));
        end;

        end;

        local_get(Local(3)); i32_const(Imm32(1)); i32_add; local_set(Local(3));
        br(0);
        end; end;

        // Build result
        local_get(Local(5)); i32_const(Imm32(0)); i32_gt_u;
        if_empty;
          // Prefix match found: strip prefix + optional '/' separator
          local_get(Local(2)); local_get(Local(4)); i32_store(0);
          local_get(Local(1)); local_get(Local(5)); i32_sub; i32_const(Imm32(0)); i32_gt_u;
          if_empty;
            local_get(Local(0)); local_get(Local(5)); i32_add; i32_load8_u(0);
            i32_const(Imm32(ASCII_SLASH)); i32_eq;
            if_empty;
              local_get(Local(2)); local_get(Local(0)); local_get(Local(5)); i32_add; i32_const(Imm32(1)); i32_add; i32_store(4);
              local_get(Local(2)); local_get(Local(1)); local_get(Local(5)); i32_sub; i32_const(Imm32(1)); i32_sub; i32_store(8);
            else_;
              local_get(Local(2)); local_get(Local(0)); local_get(Local(5)); i32_add; i32_store(4);
              local_get(Local(2)); local_get(Local(1)); local_get(Local(5)); i32_sub; i32_store(8);
            end;
          else_;
            // Exact match (e.g., path="/tmp", preopen="/tmp"): use "." as relative path
            local_get(Local(2)); i32_const(Imm32(dot_ptr as i32)); i32_store(4);
            local_get(Local(2)); i32_const(Imm32(1)); i32_store(8);
          end;
        else_;
          // No prefix match. For relative paths, find "." preopened dir. For absolute, strip '/'.
          local_get(Local(0)); i32_load8_u(0); i32_const(Imm32(ASCII_SLASH)); i32_eq;
          if_empty;
            // Absolute path with no match: strip '/' and use WASI_FIRST_PREOPEN_FD
            local_get(Local(2)); i32_const(Imm32(WASI_FIRST_PREOPEN_FD)); i32_store(0);
            local_get(Local(2)); local_get(Local(0)); i32_const(Imm32(1)); i32_add; i32_store(4);
            local_get(Local(2)); local_get(Local(1)); i32_const(Imm32(1)); i32_sub; i32_store(8);
          else_;
            // Relative path: find "." in preopened dirs, fallback to WASI_FIRST_PREOPEN_FD
            local_get(Local(2)); i32_const(Imm32(WASI_FIRST_PREOPEN_FD)); i32_store(0); // default fd
            i32_const(Imm32(0)); local_set(Local(3));
            block_empty; loop_empty;
            local_get(Local(3)); global_get(emitter.preopen_count_global); i32_ge_u; br_if(1);
            global_get(emitter.preopen_table_global);
            local_get(Local(3)); i32_const(Imm32(PREOPEN_ENTRY_SIZE)); i32_mul; i32_add;
            local_set(Local(6));
            // Check if entry path is "." (len==1 && byte[0]=='.')
            local_get(Local(6)); i32_load(8); i32_const(Imm32(1)); i32_eq;
            if_empty;
              local_get(Local(6)); i32_load(4); i32_load8_u(0); i32_const(Imm32(ASCII_DOT)); i32_eq;
              if_empty;
                local_get(Local(2)); local_get(Local(6)); i32_load(0); i32_store(0); // use this fd
                br(3); // break out of search loop
              end;
            end;
            local_get(Local(3)); i32_const(Imm32(1)); i32_add; local_set(Local(3));
            br(0);
            end; end;
            // Pass relative path as-is
            local_get(Local(2)); local_get(Local(0)); i32_store(4);
            local_get(Local(2)); local_get(Local(1)); i32_store(8);
          end;
        end;

        local_get(Local(2));
        end;
    });

    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.resolve_path, type_idx, f));
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
        // sign = bits >> F16_SIGN_SHIFT
        local_get(Local(0)); i32_const(Imm32(F16_SIGN_SHIFT)); i32_shr_u; local_set(Local(1));
        // exp = (bits >> F16_EXP_SHIFT) & F16_EXP_ALL_ONES
        local_get(Local(0)); i32_const(Imm32(F16_EXP_SHIFT)); i32_shr_u; i32_const(Imm32(F16_EXP_ALL_ONES)); i32_and; local_set(Local(2));
        // mant = bits & F16_MANTISSA_MASK
        local_get(Local(0)); i32_const(Imm32(F16_MANTISSA_MASK)); i32_and; local_set(Local(3));
        // sign_f = sign ? -1.0 : 1.0
        local_get(Local(1));
        if_f64; f64_const(-1.0);
        else_; f64_const(1.0); end;
        local_set(Local(5));

        // Branch on exp
        local_get(Local(2)); i32_eqz;
        if_f64;
            // subnormal: sign_f * mant * 2^-24
            local_get(Local(5));
            local_get(Local(3)); f64_convert_i32_u;
            f64_mul;
            f64_const(5.960464477539063e-8); // 2^-24
            f64_mul;
        else_;
            local_get(Local(2)); i32_const(Imm32(F16_EXP_ALL_ONES)); i32_eq;
            if_f64;
                // exp all-ones: mant==0 → ±inf (sign-preserving), mant!=0 → NaN.
                // Mirrors native f16_bits_to_f64 (runtime/rs/src/bytes.rs): the
                // previous `sign * f32::MAX` was finite and diverged.
                local_get(Local(3)); i32_eqz;
                if_f64;
                    local_get(Local(5)); f64_const(f64::INFINITY); f64_mul; // ±inf
                else_;
                    f64_const(f64::NAN);
                end;
            else_;
                // normal: sign_f * (1 + mant/1024) * 2^(exp-15)
                // 2^(exp-15) computed as f64 bit pattern:
                //   f64 exponent bias = 1023, so exp_f64 = exp - 15 + 1023 = exp + F16_TO_F64_EXP_BIAS_ADJ
                //   bits = (exp_f64) << F64_MANTISSA_BITS
                local_get(Local(5));
                f64_const(1.0);
                local_get(Local(3)); f64_convert_i32_u;
                f64_const(1024.0); f64_div;
                f64_add;
                f64_mul;
                // Multiply by 2^(exp - 15): construct that power via i64 bit tricks.
                local_get(Local(2)); i32_const(Imm32(F16_TO_F64_EXP_BIAS_ADJ)); i32_add; i64_extend_i32_u;
                i64_const(Imm64(F64_MANTISSA_BITS)); i64_shl;
                f64_reinterpret_i64;
                f64_mul;
            end;
        end;
        end;  // close function body
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.bytes_f16_to_f64, type_idx, f));
}
