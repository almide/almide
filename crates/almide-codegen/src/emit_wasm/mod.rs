//! Direct WASM emission — Phase 1: IR-driven codegen
//!
//! Emits a standalone WASM binary from IrProgram, targeting WASI preview1.
//!
//! Architecture:
//!   IrProgram → WasmEmitter (register + compile) → wasm_encoder::Module → Vec<u8>
//!
//! Memory layout (1 page = 64KB):
//!   [0..16)     Scratch area (iov struct for WASI fd_write)
//!   [16..48)    int_to_string scratch buffer
//!   [48]        Newline byte (0x0A)
//!   [49..N)     String literal data ([len:i32][data:u8...] per string)
//!   [N..)       Heap (bump allocator, grows upward)

#[macro_use]
mod wasm_macro;

pub mod values;
pub mod list_layout;
pub mod engine;
mod strings;
mod runtime;
mod runtime_eq;
mod rt_string;
mod rt_string_extra;
mod rt_string_case;
mod rt_unicode_tables;
mod rt_numeric;
mod rt_dragon;
mod rt_dec2flt;
mod rt_libm;
mod rt_repr;
mod rt_float_display;
mod calls_string_repr;
mod expressions;
mod stdlib_dispatch;
mod calls;
mod calls_env;
mod calls_random;
mod calls_datetime;
mod calls_http;
mod calls_fs;
mod calls_io;
mod calls_process;
mod calls_string;
mod calls_option;
mod calls_numeric;
mod calls_list;
mod calls_list_helpers;
mod calls_list_closure;
mod calls_list_closure2;
mod calls_lambda;
mod calls_map;
mod calls_map_closure;
mod calls_bytes;
mod calls_matrix;
mod calls_set;
mod calls_value;
mod calls_regex;
mod rt_value;
pub(crate) mod rt_regex;
mod rt_encoding;
mod closures;
mod equality;
mod collections;
mod control;
pub mod statements;
mod functions;
pub mod scratch;
mod dce;

use std::collections::HashMap;
// BTreeMap for record_fields / variant_info: their iteration order reaches
// emitted bytes (field offsets, variant sizes, the Unknown-type member-access
// fallback), and HashMap iteration is host-pointer-width dependent (hashbrown's
// h2 control byte = hash >> (usize_bits-7)), so a wasm32 host (the playground)
// would pick different offsets than x86-64 and trap. BTreeMap is deterministic.
use std::collections::BTreeMap;
use wasm_encoder::{
    CodeSection, DataSection, ElementSection, Elements, ExportSection,
    Function, FunctionSection, GlobalSection, GlobalType, ImportSection,
    MemorySection, MemoryType, Module, NameMap, NameSection, RefType, TableSection,
    TableType, TypeSection, ValType,
};

use almide_ir::IrProgram;
use almide_lang::types::Ty;

// Memory layout constants
const SCRATCH_ITOA: u32 = 16;
/// String pool base address. Must be above ASCII range (0-127) so that
/// data section DCE can distinguish string offsets from character codes.
const NEWLINE_OFFSET: u32 = 4096;

/// Wrapper around `wasm_encoder::Function` that automatically records
/// `call` targets as instructions are emitted. Used by `FuncCompiler`
/// so DCE gets a type-safe call graph without bytecode scanning.
pub struct TrackedFunction {
    pub inner: Function,
    pub call_targets: Vec<u32>,
}

impl TrackedFunction {
    pub fn new<I>(locals: I) -> Self
    where
        I: IntoIterator<Item = (u32, ValType)>,
        I::IntoIter: ExactSizeIterator,
    {
        Self { inner: Function::new(locals), call_targets: Vec::new() }
    }

    /// Emit an instruction, recording call targets automatically.
    pub fn instruction(&mut self, i: &wasm_encoder::Instruction) -> &mut Self {
        match i {
            wasm_encoder::Instruction::Call(idx) => self.call_targets.push(*idx),
            wasm_encoder::Instruction::ReturnCall(idx) => self.call_targets.push(*idx),
            _ => {}
        }
        self.inner.instruction(i);
        self
    }

    /// Consume into a raw body (delegates to inner Function).
    pub fn into_raw_body(self) -> Vec<u8> {
        self.inner.into_raw_body()
    }
}

impl Clone for TrackedFunction {
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone(), call_targets: self.call_targets.clone() }
    }
}

/// A compiled WASM function ready for the code section.
///
/// All functions MUST be constructed via `CompiledFunc::tracked()` to ensure
/// DCE has a complete call graph. Direct construction is impossible — the
/// `call_targets` field is always populated by TrackedFunction.
pub struct CompiledFunc {
    pub type_idx: u32,
    pub func: Function,
    /// Call targets recorded during compilation. DCE uses this directly —
    /// no bytecode scanning needed. Guaranteed non-empty by construction
    /// (TrackedFunction records all `call` instructions automatically).
    pub call_targets: Vec<u32>,
    /// Patched raw body bytes (set by data section DCE). If Some, used
    /// instead of `func` during assembly to reflect compacted data offsets.
    pub patched_body: Option<Vec<u8>>,
}

impl CompiledFunc {
    /// Construct from a TrackedFunction. This is the ONLY constructor —
    /// enforces that call_targets is always populated.
    pub fn tracked(type_idx: u32, tf: TrackedFunction) -> Self {
        Self { type_idx, func: tf.inner, call_targets: tf.call_targets, patched_body: None }
    }
}

/// String stdlib runtime function indices.
pub struct StringRuntime {
    pub eq: u32,
    pub contains: u32,
    pub trim: u32,
    pub slice: u32,
    pub char_count: u32,
    /// `__str_cp_of_byte(s, byte) -> i64`: codepoint index of byte offset `byte`.
    pub cp_of_byte: u32,
    pub reverse: u32,
    pub repeat: u32,
    pub index_of: u32,
    pub replace: u32,
    pub split: u32,
    pub join: u32,
    pub count: u32,
    pub pad_start: u32,
    pub pad_end: u32,
    pub trim_start: u32,
    pub trim_end: u32,
    pub to_upper: u32,
    pub to_lower: u32,
    pub chars: u32,
    pub lines: u32,
    pub from_bytes: u32,
    pub to_bytes: u32,
    pub replace_first: u32,
    pub last_index_of: u32,
    pub strip_prefix: u32,
    pub strip_suffix: u32,
    pub is_digit: u32,
    pub is_alpha: u32,
    pub is_alnum: u32,
    pub is_whitespace: u32,
    /// `is_unicode_ws(scalar) -> i32`: 1 iff `scalar` has the Unicode White_Space
    /// property (Rust `char::is_whitespace`). The single source of truth for all
    /// trim / is_whitespace / parse-trim sites.
    pub is_unicode_ws: u32,
    /// `utf8_classify(buf, i, n) -> i32`: classify the UTF-8 sequence at byte `i`
    /// for `from_utf8_lossy`. Returns `(consumed << 1) | valid` — `valid`=1 means
    /// copy `consumed` bytes; `valid`=0 means a maximal invalid subpart of
    /// `consumed` bytes (emit one U+FFFD).
    pub utf8_classify: u32,
    pub is_upper: u32,
    pub is_lower: u32,
    pub cmp: u32,
    pub run_length_encode: u32,
    /// UTF-8 codepoint helpers (shared by codepoint-aware string ops).
    /// `utf8_width(s, byte_i) -> i32`: byte width (1-4) of the codepoint whose
    /// lead byte sits at data offset `byte_i`. Stray continuation bytes and a
    /// width that would read past the end both clamp to 1.
    pub utf8_width: u32,
    /// `utf8_scalar(s, byte_i) -> i64`: Unicode scalar value decoded from the
    /// codepoint at data offset `byte_i`. Malformed sequences yield the lead
    /// byte (width-1 fallback), never an OOB read.
    pub utf8_scalar: u32,
    /// `utf8_byte_of_cp(s, n) -> i32`: byte offset (within the data section) of
    /// the start of the n-th codepoint. `n >= count` returns the byte length.
    pub utf8_byte_of_cp: u32,
    /// `utf8_snap(s, byte_i) -> i32`: byte_i rounded DOWN to the nearest UTF-8
    /// char boundary (clamped to [0, byte_len]). Mirrors native `slice`'s
    /// boundary-safe byte indexing.
    pub utf8_snap: u32,
    // ── Oracle-derived Unicode property membership (in `rt_unicode_tables`) ──
    /// `(scalar: i32) -> i32` (0/1): binary-search the embedded property range
    /// table for `is_alphabetic` / `is_alphanumeric` / `is_uppercase` /
    /// `is_lowercase`. These make the WASM string predicates full-Unicode and
    /// byte-identical to native's `char` methods. Each searches the table at the
    /// matching `prop_*_table` offset below.
    pub prop_alpha: u32,
    pub prop_alnum: u32,
    pub prop_upper: u32,
    pub prop_lower: u32,
    /// Data-section offsets of the embedded `[len][cap][data]` range-table blobs
    /// (interned via `intern_bytes`, so dead-data elimination may relocate or
    /// drop them). The membership helpers above emit the BARE offset as their
    /// only data `i32.const` so a relocation stays correct.
    pub prop_alpha_table: u32,
    pub prop_alnum_table: u32,
    pub prop_upper_table: u32,
    pub prop_lower_table: u32,
    // ── Full-Unicode case folding (oracle-derived tables in `rt_string_case`) ──
    /// `__utf8_emit_scalar(dst, byte_off, scalar) -> i32`: encode `scalar` as
    /// UTF-8 at `dst`'s data section + byte_off; returns the advanced byte_off.
    pub utf8_emit_scalar: u32,
    /// `__case_map_lookup(map_sel, scalar) -> i32`: binary-search the UPPER(0) /
    /// LOWER(1) map; returns the absolute address of the `[len][bytes]` value
    /// record, or -1 on miss (identity).
    pub case_map_lookup: u32,
    /// `__set_member(set_sel, scalar) -> i32`: 1 iff `scalar` is in the
    /// CASED(0) / CASE_IGNORABLE(1) sorted key array.
    pub set_member: u32,
    /// `__final_sigma(s, byte_off) -> i32`: ς(U+03C2) or σ(U+03C3) for a Σ at
    /// `byte_off`, per the Unicode Final_Sigma context rule.
    pub final_sigma: u32,
    /// `__str_case_map(s, is_upper) -> i32`: the unified two-pass case driver.
    pub str_case_map: u32,
    /// `__str_capitalize(s) -> i32`: first scalar uppercased, rest verbatim.
    pub capitalize: u32,
}

/// Absolute linear-memory addresses + counts of the embedded case tables.
#[derive(Clone, Copy)]
struct CaseTableOffsets {
    upper_keys: u32, upper_offs: u32, upper_n: u32,
    lower_keys: u32, lower_offs: u32, lower_n: u32,
    cased: u32, cased_n: u32,
    ci: u32, ci_n: u32,
}

/// Indices of built-in runtime functions.
pub struct RuntimeFuncs {
    pub fd_write: u32,
    pub alloc: u32,
    pub rc_inc: u32,
    pub rc_dec: u32,
    pub cow_check: u32,
    pub heap_save: u32,
    pub heap_restore: u32,
    /// Global index holding the heap start address (immutable).
    /// Pointers below this are in the data section and must NOT be rc_dec'd.
    pub heap_start_global: u32,
    pub println_str: u32,
    pub int_to_string: u32,
    pub println_int: u32,
    pub concat_str: u32,
    pub string_append: u32,
    /// __div_trap(msg_ptr: i32) -> () — write the interned message string
    /// (already `Error: <msg>\n`) to stderr and `proc_exit(1)`. Shared by the
    /// integer div/mod zero-divisor and signed-overflow abort paths so the failure
    /// matches native byte-for-byte (§13 termination convention).
    pub div_trap: u32,
    /// __string_alloc(len: i32) -> i32
    /// Allocate string with header: writes len AND cap, returns ptr.
    /// Eliminates the class of bugs where cap is forgotten after alloc.
    pub string_alloc: u32,
    pub concat_list: u32,
    pub list_eq: u32,
    pub mem_eq: u32,
    pub list_list_str_cmp: u32,
    pub option_eq_i64: u32,
    pub option_eq_str: u32,
    pub result_eq_i64_str: u32,
    pub int_parse: u32,
    pub int_from_hex: u32,
    pub float_to_string: u32,
    pub float_parse: u32,
    pub float_to_fixed: u32,
    pub float_pow: u32,
    pub math_sin: u32,
    pub math_cos: u32,
    pub math_tan: u32,
    pub math_log: u32,
    pub math_log10: u32,
    pub math_log2: u32,
    pub math_exp: u32,
    /// IEEE-754 half-precision → f64 (for bytes.read_f16_le).
    pub bytes_f16_to_f64: u32,
    /// base64 / hex stdlib runtime helpers.
    pub base64_encode: u32,
    pub base64_decode: u32,
    pub base64_encode_url: u32,
    pub base64_decode_url: u32,
    pub hex_encode: u32,
    pub hex_encode_upper: u32,
    pub hex_decode: u32,
    pub string: StringRuntime,
    pub value_stringify: u32,
    pub json_escape_string: u32,
    pub json_parse: u32,
    pub json_parse_at: u32,
    pub json_get_path: u32,
    pub json_set_path: u32,
    pub json_remove_path: u32,
    pub regex: rt_regex::RegexRuntime,
    pub clock_time_get: u32,
    pub proc_exit: u32,
    pub random_get: u32,
    pub path_open: u32,
    pub fd_read: u32,
    pub fd_close: u32,
    pub fd_seek: u32,
    pub fd_filestat_get: u32,
    pub path_filestat_get: u32,
    pub path_create_directory: u32,
    pub path_rename: u32,
    pub path_unlink_file: u32,
    pub path_remove_directory: u32,
    pub fd_readdir: u32,
    pub fd_prestat_get: u32,
    pub fd_prestat_dir_name: u32,
    /// __resolve_path(path_ptr, path_len) → (fd, rel_path_ptr, rel_path_len)
    /// Resolves absolute/relative paths against preopened directories.
    pub resolve_path: u32,
    /// __init_preopen_dirs() → ()
    /// Called at _start to discover preopened directories.
    pub init_preopen_dirs: u32,
    /// Dragon4 shortest-decimal helper functions (float.to_string).
    pub dragon: rt_dragon::DragonRuntime,
    pub decfloat: rt_dec2flt::DecFloatRuntime,
    /// Vendored musl-libm trig (sin/cos/tan + their kernels/argument reduction).
    /// Mirrors `runtime/rs/src/libm.rs` so trig is bit-identical native↔wasm.
    pub libm: rt_libm::LibmRuntime,
    /// __repr_str(s: i32) -> i32: double-quote + escape a string for the
    /// Almide-literal repr of a string INSIDE a container (compound string
    /// interpolation). Escape set mirrors `almide_rt_value_stringify` and the
    /// native `almide_repr_str`: `\\ \" \n \r \t`.
    pub repr_str: u32,
    /// __float_display(f: f64) -> i32: the `Display`-form float text used by
    /// string interpolation — the Dragon4 `float.to_string` output with a
    /// trailing `.0` removed, matching the native Rust `Display` oracle
    /// (`0.0` → `0`, `1.0` → `1`; non-integral values are unchanged).
    pub float_display: u32,
}

/// Import descriptor for WASM import section.
struct ImportInfo {
    module: String,
    name: String,
    type_idx: u32,
}

/// Central state for WASM binary emission.
pub struct WasmEmitter {
    // Layout registry — single source of truth for all heap object layouts.
    pub layout_reg: engine::LayoutRegistry,

    // Type section (deduplicated function signatures)
    pub(crate) types: Vec<(Vec<ValType>, Vec<ValType>)>,
    type_map: HashMap<(Vec<ValType>, Vec<ValType>), u32>,

    // Imports
    imports: Vec<ImportInfo>,
    num_imports: u32,

    // Function index tracking
    next_func_idx: u32,
    pub func_map: HashMap<String, u32>,
    pub module_names: Vec<String>,
    /// Reverse lookup for `@intrinsic("almide_rt_<m>_<f>")` attributes:
    /// mangled symbol → (stdlib module name, Almide fn name). Populated
    /// once from bundled stdlib sources; used by the WASM `RuntimeCall`
    /// fallback path so that dispatch routes to the correct
    /// `emit_<m>_call` arm even when the runtime symbol name differs
    /// from the Almide fn name (e.g. `map.map` → `almide_rt_map_map_values`).
    pub intrinsic_symbol_to_fn: HashMap<String, (String, String)>,
    // func_idx → type_idx for defined (non-import) functions
    pub func_type_indices: HashMap<u32, u32>,

    // Compiled function bodies (in definition order)
    compiled: Vec<CompiledFunc>,

    // String pool
    strings: HashMap<String, u32>,
    data_bytes: Vec<u8>,

    /// Embedded Unicode case-mapping table offsets (Some only when the program
    /// uses string case ops — see `embed_case_tables` / `program_uses_case_op`).
    case_tables: Option<CaseTableOffsets>,
    /// Total bytes of the protected table region at the FRONT of `data_bytes`
    /// (after the newline byte): Unicode case tables AND the vendored-libm
    /// 2/pi (IPIO2) / PIO2 tables. `eliminate_dead_data` skips this whole region
    /// so the raw table bytes are never misparsed as interned-string entries and
    /// never shift when dead strings are compacted (the lookup functions bake
    /// absolute addresses). 0 when neither case ops nor trig are used.
    pub case_table_bytes: usize,
    /// Absolute byte offsets of the embedded libm trig tables (the `IPIO2`
    /// 2/pi table and `PIO2` extended-precision pi/2 table). `Some` only when
    /// the program uses `math.sin/cos/tan` — see `embed_libm_tables`.
    pub libm_tables: Option<rt_libm::LibmTableOffsets>,

    // Runtime function indices
    pub rt: RuntimeFuncs,

    // Globals
    pub heap_ptr_global: u32,
    /// Free list head for Perceus-style reuse (0 = empty).
    pub free_list_global: u32,
    /// Preopened dir table pointer
    pub preopen_table_global: u32,
    /// Number of preopened directories discovered
    pub preopen_count_global: u32,
    // Top-level let globals: VarId → (global index, ValType)
    pub top_let_globals: HashMap<u32, (u32, ValType)>,
    /// Name-keyed mirror of `top_let_globals`, plus entries for cross-module
    /// `static ALMIDE_RT_<MOD>_<NAME>` so that synthetic Vars in the main
    /// var_table can resolve via name even when their VarId belongs to a
    /// different table.
    pub top_let_globals_by_name: HashMap<String, (u32, ValType)>,
    /// DefId-keyed global mapping. Authoritative for cross-package resolution.
    pub def_globals: HashMap<u32, (u32, ValType)>,
    pub top_let_init: Vec<(u32, ValType, i64)>, // (global_idx, type, const_init_bits) in order
    pub next_global: u32,

    // Function table: func_idx → table_idx (for call_indirect / FnRef)
    pub func_table: Vec<u32>, // list of func_idx in table order
    pub func_to_table_idx: HashMap<u32, u32>, // func_idx → table index

    // User-defined public functions to export: (export_name, internal_name)
    pub user_exports: Vec<(String, String)>,

    // Type info: record/variant name → field list (for field offset computation)
    pub record_fields: BTreeMap<String, Vec<(String, almide_lang::types::Ty)>>,
    // Variant info: variant type name → list of (case_name, tag, fields)
    pub variant_info: BTreeMap<String, Vec<VariantCase>>,
    // Default field values: (type_name, field_name) → default IR expr
    pub default_fields: HashMap<(String, String), almide_ir::IrExpr>,

    // Lambda/closure info: sequential index → LambdaInfo
    pub lambdas: Vec<LambdaInfo>,
    // FnRef wrappers: original func name → wrapper table_idx
    pub fn_ref_wrappers: HashMap<String, u32>,
    // Lambda counter (for matching pre-scan order during emission)
    pub lambda_counter: std::cell::Cell<usize>,
    // Effect functions: their call returns Result but IR may expect unwrapped type
    pub effect_fns: HashSet<String>,
    // Mutable variables captured by closures: these must use heap cells instead of locals
    pub mutable_captures: HashSet<u32>,
    // Copy-aliased, in-place-mutated heap locals (AliasCowPass). At each mutation
    // site of one of these, the emitter inserts __cow_check + write-back so the
    // alias is preserved (value semantics). Empty for non-aliasing programs → the
    // direct mutation path is byte-identical to before.
    pub needs_cow: HashSet<u32>,
    /// §4 Stage 2: synthetic cross-module use-site VarId → declaration VarId
    /// (from `codegen_annotations.global_alias`). The PRIMARY resolution for
    /// module globals — the name-key reconstruction below it is the soak-era
    /// fallback.
    pub global_alias: HashMap<u32, u32>,
    // Deep-equality functions per variant type: type_name → func_idx
    pub eq_funcs: HashMap<String, u32>,
    // Almide-literal repr functions per NAMED record/variant type: type_name →
    // func_idx. A `__repr_<Type>(ptr) -> str_ptr` walks one finite value of that
    // type back to its literal form, recursing into self/mutually-recursive type
    // references as a CALL (terminating at runtime over the finite value) — the
    // same shape as the native `AlmideRepr` trait dispatch. Without this, a
    // recursive ADT (`type Tree = Leaf(Int) | Node(Tree, Tree)`) would expand its
    // type graph forever at compile time. Reserved (sorted, host-deterministic)
    // before compilation, mirroring `eq_funcs`.
    // Keyed by the MANGLED instantiation name (`Tree_Int`, `Tree_String`,
    // `Tree_List_Int`), not the bare type name. A generic recursive ADT
    // (`type Tree[T] = Leaf(T) | Node(Tree[T], Tree[T])`) needs a SEPARATE repr
    // fn per concrete `T`: the fn renders its payload TEXT, which differs by `T`
    // (a `Leaf(Int)` reprs `1`, a `Leaf(String)` reprs `"a"`). A monomorphic
    // by-bare-name fn read the payload as a raw `TypeVar` and printed `T {  }`.
    // Non-generic recursive types mangle to their bare name (backward compatible).
    pub repr_funcs: BTreeMap<String, u32>,
    // The concrete instantiation `Ty::Named(name, args)` behind each mangled
    // `repr_funcs` key, so `compile_repr_funcs` walks the body with the real
    // type args (substituted into each case's payload), not empty args.
    pub repr_func_tys: BTreeMap<String, Ty>,
    // Whether the program uses filesystem operations (fs.read_text, etc.)
    pub needs_fs: bool,
}

/// A single case of a variant type.
#[derive(Clone)]
pub struct VariantCase {
    pub name: String,
    pub tag: u32,
    pub fields: Vec<(String, almide_lang::types::Ty)>,
}

/// Pre-scanned lambda information.
pub struct LambdaInfo {
    pub table_idx: u32,
    pub closure_type_idx: u32,
    pub captures: Vec<(almide_ir::VarId, almide_lang::types::Ty)>,
    pub param_ids: Vec<u32>,
    pub lambda_id: Option<u32>,
}

impl WasmEmitter {
    fn new() -> Self {
        WasmEmitter {
            layout_reg: engine::LayoutRegistry::new(),
            types: Vec::new(),
            type_map: HashMap::new(),
            imports: Vec::new(),
            num_imports: 0,
            next_func_idx: 0,
            func_map: HashMap::new(),
            module_names: Vec::new(),
            intrinsic_symbol_to_fn: HashMap::new(),
            func_type_indices: HashMap::new(),
            compiled: Vec::new(),
            strings: HashMap::new(),
            // First byte is newline at NEWLINE_OFFSET
            data_bytes: vec![0x0A],
            case_tables: None,
            libm_tables: None,
            case_table_bytes: 0,
            rt: RuntimeFuncs {
                fd_write: 0, alloc: 0, rc_inc: 0, rc_dec: 0, cow_check: 0,
                heap_save: 0, heap_restore: 0, heap_start_global: 0,
                println_str: 0, println_int: 0,
                int_to_string: 0, float_to_string: 0,
                float_parse: 0, float_to_fixed: 0, float_pow: 0,
                math_sin: 0, math_cos: 0, math_tan: 0,
                math_log: 0, math_log10: 0, math_log2: 0, math_exp: 0,
                bytes_f16_to_f64: 0,
                base64_encode: 0, base64_decode: 0,
                base64_encode_url: 0, base64_decode_url: 0,
                hex_encode: 0, hex_encode_upper: 0, hex_decode: 0,
                concat_str: 0,
                div_trap: 0,
                string_append: 0,
                string_alloc: 0,
                concat_list: 0,
                list_eq: 0, mem_eq: 0, list_list_str_cmp: 0,
                option_eq_i64: 0, option_eq_str: 0,
                result_eq_i64_str: 0, int_parse: 0, int_from_hex: 0,
                string: StringRuntime {
                    eq: 0, contains: 0, trim: 0,
                    slice: 0, reverse: 0, repeat: 0, index_of: 0,
                    replace: 0, split: 0, join: 0, count: 0,
                    pad_start: 0, pad_end: 0,
                    trim_start: 0, trim_end: 0,
                    to_upper: 0, to_lower: 0,
                    chars: 0, lines: 0,
                    from_bytes: 0, to_bytes: 0,
                    replace_first: 0, last_index_of: 0,
                    strip_prefix: 0, strip_suffix: 0,
                    is_digit: 0, is_alpha: 0, is_alnum: 0,
                    is_whitespace: 0, is_unicode_ws: 0, utf8_classify: 0, is_upper: 0, is_lower: 0,
                    cmp: 0,
                    char_count: 0,
                    cp_of_byte: 0,
                    run_length_encode: 0,
                    utf8_width: 0,
                    utf8_scalar: 0,
                    utf8_byte_of_cp: 0,
                    utf8_snap: 0,
                    prop_alpha: 0, prop_alnum: 0, prop_upper: 0, prop_lower: 0,
                    prop_alpha_table: 0, prop_alnum_table: 0,
                    prop_upper_table: 0, prop_lower_table: 0,
                    utf8_emit_scalar: 0,
                    case_map_lookup: 0,
                    set_member: 0,
                    final_sigma: 0,
                    str_case_map: 0,
                    capitalize: 0,
                },
                value_stringify: 0,
                json_escape_string: 0,
                json_parse: 0,
                json_parse_at: 0,
                json_get_path: 0,
                json_set_path: 0,
                json_remove_path: 0,
                regex: rt_regex::RegexRuntime::default(),
                clock_time_get: 0,
                proc_exit: 0,
                random_get: 0,
                path_open: 0,
                fd_read: 0,
                fd_close: 0,
                fd_seek: 0,
                fd_filestat_get: 0,
                path_filestat_get: 0,
                path_create_directory: 0,
                path_rename: 0,
                path_unlink_file: 0,
                path_remove_directory: 0,
                fd_readdir: 0,
                fd_prestat_get: 0,
                fd_prestat_dir_name: 0,
                resolve_path: 0,
                init_preopen_dirs: 0,
                dragon: rt_dragon::DragonRuntime::default(),
                decfloat: rt_dec2flt::DecFloatRuntime::default(),
                libm: rt_libm::LibmRuntime::default(),
                repr_str: 0,
                float_display: 0,
            },
            heap_ptr_global: 0,
            free_list_global: 1,
            preopen_table_global: 2,
            preopen_count_global: 3,
            top_let_globals: HashMap::new(),
            def_globals: HashMap::new(),
            top_let_globals_by_name: HashMap::new(),
            top_let_init: Vec::new(),
            next_global: 5, // 0=heap_ptr, 1=free_list, 2=preopen_table, 3=preopen_count, 4=heap_start
            func_table: Vec::new(),
            func_to_table_idx: HashMap::new(),
            record_fields: BTreeMap::new(),
            variant_info: BTreeMap::new(),
            default_fields: HashMap::new(),
            lambdas: Vec::new(),
            fn_ref_wrappers: HashMap::new(),
            lambda_counter: std::cell::Cell::new(0),
            effect_fns: HashSet::new(),
            mutable_captures: HashSet::new(),
            needs_cow: HashSet::new(),
            global_alias: HashMap::new(),
            eq_funcs: HashMap::new(),
            repr_funcs: BTreeMap::new(),
            repr_func_tys: BTreeMap::new(),
            user_exports: Vec::new(),
            needs_fs: false,
        }
    }

    /// Register a function type, returning its (deduplicated) type index.
    pub fn register_type(&mut self, params: Vec<ValType>, results: Vec<ValType>) -> u32 {
        let key = (params.clone(), results.clone());
        if let Some(&idx) = self.type_map.get(&key) {
            return idx;
        }
        let idx = self.types.len() as u32;
        self.types.push((params, results));
        self.type_map.insert(key, idx);
        idx
    }

    /// Register a WASI import function, returning its function index.
    pub fn register_import(&mut self, _type_idx: u32) -> u32 {
        let idx = self.next_func_idx;
        self.next_func_idx += 1;
        self.num_imports += 1;
        idx
    }

    /// Register a defined function by name, returning its function index.
    pub fn register_func(&mut self, name: &str, type_idx: u32) -> u32 {
        let idx = self.next_func_idx;
        self.next_func_idx += 1;
        self.func_map.insert(name.to_string(), idx);
        self.func_type_indices.insert(idx, type_idx);
        idx
    }

    /// Add a compiled function body.
    pub fn add_compiled(&mut self, compiled: CompiledFunc) {
        self.compiled.push(compiled);
    }

    /// Embed the oracle-derived Unicode case tables at the FRONT of `data_bytes`
    /// (immediately after the newline byte), recording their absolute addresses.
    ///
    /// Placement at the front is MANDATORY: it sits at a fixed low offset that
    /// never moves when interned string literals (appended later, during function
    /// compilation) are compacted by `eliminate_dead_data`. Must be called once,
    /// before any string is interned (asserted), so the baked `i32_const` offsets
    /// in the case runtime functions stay valid for the life of the module.
    fn embed_case_tables(&mut self) {
        debug_assert_eq!(
            self.data_bytes.len(), 1,
            "case tables must be embedded before any string interning"
        );
        let t = rt_string_case::generate_case_tables();

        fn pad4(db: &mut Vec<u8>) {
            while db.len() % 4 != 0 { db.push(0); }
        }
        fn push_u32s(db: &mut Vec<u8>, arr: &[u32]) -> u32 {
            pad4(db);
            let base = NEWLINE_OFFSET + db.len() as u32;
            for &x in arr { db.extend_from_slice(&x.to_le_bytes()); }
            base
        }
        fn push_bytes(db: &mut Vec<u8>, bytes: &[u8]) -> u32 {
            let base = NEWLINE_OFFSET + db.len() as u32;
            db.extend_from_slice(bytes);
            base
        }

        // For each map: place VALS first so its base is known, then bake the OFFS
        // array as ABSOLUTE addresses into VALS, then the KEYS search array.
        let db = &mut self.data_bytes;
        let upper_vals = push_bytes(db, &t.upper.vals);
        let upper_keys = push_u32s(db, &t.upper.keys);
        let upper_offs_abs: Vec<u32> = t.upper.val_offsets.iter().map(|o| upper_vals + o).collect();
        let upper_offs = push_u32s(db, &upper_offs_abs);

        let lower_vals = push_bytes(db, &t.lower.vals);
        let lower_keys = push_u32s(db, &t.lower.keys);
        let lower_offs_abs: Vec<u32> = t.lower.val_offsets.iter().map(|o| lower_vals + o).collect();
        let lower_offs = push_u32s(db, &lower_offs_abs);

        let cased = push_u32s(db, &t.cased);
        let ci = push_u32s(db, &t.case_ignorable);

        self.case_table_bytes = self.data_bytes.len() - 1;
        self.case_tables = Some(CaseTableOffsets {
            upper_keys, upper_offs, upper_n: t.upper.keys.len() as u32,
            lower_keys, lower_offs, lower_n: t.lower.keys.len() as u32,
            cased, cased_n: t.cased.len() as u32,
            ci, ci_n: t.case_ignorable.len() as u32,
        });
    }

    /// Embed the vendored-libm 2/pi (`IPIO2`) and `PIO2` constant tables into the
    /// FRONT protected region of `data_bytes`, immediately after the case tables
    /// (if any) and before any string is interned. Their absolute addresses are
    /// recorded and baked as `i32_const` into the `rt_libm` runtime, so — like the
    /// case tables — they must sit at a fixed low offset that `eliminate_dead_data`
    /// never moves. Adds to the protected `case_table_bytes` prefix.
    fn embed_libm_tables(&mut self) {
        debug_assert!(
            self.data_bytes.len() == 1 + self.case_table_bytes,
            "libm tables must be embedded right after case tables, before string interning"
        );
        fn pad8(db: &mut Vec<u8>) {
            while db.len() % 8 != 0 { db.push(0); }
        }
        // IPIO2: 690 × i32. PIO2: 8 × f64 (8-byte aligned for f64.load).
        let db = &mut self.data_bytes;
        let ipio2_base = NEWLINE_OFFSET + db.len() as u32;
        for &x in rt_libm::IPIO2.iter() {
            db.extend_from_slice(&x.to_le_bytes());
        }
        pad8(db);
        let pio2_base = NEWLINE_OFFSET + db.len() as u32;
        for &x in rt_libm::PIO2.iter() {
            db.extend_from_slice(&x.to_le_bytes());
        }
        self.case_table_bytes = self.data_bytes.len() - 1;
        self.libm_tables = Some(rt_libm::LibmTableOffsets { ipio2_base, pio2_base });
    }
}

/// Label tracking for break/continue in loops.
pub struct LoopLabels {
    pub break_depth: u32,
    pub continue_depth: u32,
}

/// RAII guard for WASM block nesting depth.
/// Created by `depth_push`/`depth_push_n`, consumed by `depth_pop`.
/// `#[must_use]` ensures the guard is not silently dropped.
#[must_use = "call depth_pop() to restore depth"]
pub struct DepthGuard(u32);

impl DepthGuard {
    /// The depth value at the point this guard was created (before push).
    pub fn saved(&self) -> u32 { self.0 }
}

/// Per-function compilation state.
pub struct FuncCompiler<'a> {
    pub emitter: &'a mut WasmEmitter,
    pub func: TrackedFunction,
    pub var_map: HashMap<u32, u32>,
    pub depth: u32,
    pub loop_stack: Vec<LoopLabels>,
    // Scratch local allocator
    pub scratch: scratch::ScratchAllocator,
    // Variable table for name lookups (pattern matching)
    pub var_table: &'a almide_ir::VarTable,
    // Return type for stub calls (set by emit_call before delegating to handlers)
    pub stub_ret_ty: Ty,
    // Module name of the function being compiled (for intra-module call resolution)
    pub current_module_name: Option<String>,
}

impl FuncCompiler<'_> {
    /// Push depth by 1. Returns a guard that must be passed to `depth_pop`.
    pub fn depth_push(&mut self) -> DepthGuard {
        let g = DepthGuard(self.depth);
        self.depth += 1;
        g
    }

    /// Push depth by N. Returns a guard that restores to the saved depth.
    pub fn depth_push_n(&mut self, n: u32) -> DepthGuard {
        let g = DepthGuard(self.depth);
        self.depth += n;
        g
    }

    /// Restore depth from guard. Debug-asserts that depth hasn't been corrupted.
    pub fn depth_pop(&mut self, guard: DepthGuard) {
        debug_assert!(
            self.depth > guard.0,
            "depth_pop: depth {} should be > saved {}",
            self.depth, guard.0,
        );
        self.depth = guard.0;
    }

    /// Write a freshly-(re)allocated heap object pointer back to the variable an
    /// in-place mutator (`list.push`, `map.insert`, `string.push`, …) operates on.
    ///
    /// Every such op may relocate its target (grow + realloc), so the new pointer
    /// must replace the old binding. There are three storage classes, and getting
    /// any of them wrong is silently wrong (the mutation is lost or, worse, the
    /// next read dereferences a stale pointer):
    ///
    /// - **mutable capture** — the local holds a *cell* pointer, not the object.
    ///   Store the new object pointer *into* the cell (`i32_store(0)`), preserving
    ///   the cell's identity so other closures sharing it observe the update. This
    ///   is the case that the per-op write-backs historically missed (Closure
    ///   Architecture v2, P6): they overwrote the local with the object pointer,
    ///   corrupting the cell so subsequent cell-deref reads returned garbage.
    /// - **local** — overwrite the local with the new pointer.
    /// - **top-level global** — overwrite the global.
    ///
    /// `new_ptr` is a local already holding the new object pointer. No-op when the
    /// target is not a bare `Var` (e.g. `foo().push(x)` has nowhere to write back).
    pub fn emit_mutator_writeback(&mut self, target: &almide_ir::IrExpr, new_ptr: u32) {
        let id = match &target.kind {
            almide_ir::IrExprKind::Var { id } => id.0,
            _ => return,
        };
        if self.emitter.mutable_captures.contains(&id) {
            if let Some(&local_idx) = self.var_map.get(&id) {
                wasm!(self.func, { local_get(local_idx); local_get(new_ptr); i32_store(0); });
            }
        } else if let Some(&local_idx) = self.var_map.get(&id) {
            wasm!(self.func, { local_get(new_ptr); local_set(local_idx); });
        } else if let Some(&(global_idx, _)) = self.emitter.top_let_globals.get(&id) {
            wasm!(self.func, { local_get(new_ptr); global_set(global_idx); });
        }
    }

    /// Copy-on-write guard for the in-place mutation of a heap local. If `id` is a
    /// COW target (copy-aliased + mutated; see AliasCowPass) and stored in a plain
    /// local, load it, call `__cow_check` (clones iff rc>1), and write the returned
    /// pointer back to the local BEFORE the mutation reads it. The alias keeps the
    /// old pointer (whose rc __cow_check decremented), so the mutation can no longer
    /// reach it.
    ///
    /// No-op when `id` is not a COW target → the direct mutation path is unchanged
    /// (byte-identical wasm for non-aliasing programs). Also skipped for shared-cell
    /// captures (deliberately reference-shared) and for vars without a plain local
    /// (globals don't alias at the source level; AliasCowPass excludes them).
    pub fn cow_if_needed(&mut self, id: u32) {
        if !self.emitter.needs_cow.contains(&id) { return; }
        if self.emitter.mutable_captures.contains(&id) { return; }
        let Some(&local_idx) = self.var_map.get(&id) else { return };
        let cow_check = self.emitter.rt.cow_check;
        wasm!(self.func, { local_get(local_idx); call(cow_check); local_set(local_idx); });
    }
}

// ── Public API ──────────────────────────────────────────────────────

/// Emit a WASM binary from a fully-certified IR program (WASI mode).
///
/// AlmidePerceusBelt: the sole public door to WASM emission accepts only a
/// [`Canonical`](super::Canonical) program, which is reachable only by refining
/// a [`Verified`](super::Verified) one (see `Canonical::certify`). So neither
/// RC-unverified nor non-canonical IR can reach emission. `emit` below is
/// `pub(crate)` so this stays the only entry — closing the prior bypass where a
/// caller could invoke `emit` directly and skip the gate.
pub fn emit_certified(canonical: super::Canonical<'_>) -> Vec<u8> {
    emit(canonical.0)
}

pub(crate) fn emit(program: &IrProgram) -> Vec<u8> {
    let mut emitter = WasmEmitter::new();

    // Pre-scan: detect filesystem usage to conditionally include init_preopen_dirs
    emitter.needs_fs = program_uses_fs(program);

    // Copy the COW-target var set (AliasCowPass) onto the emitter as bare u32s,
    // mirroring `mutable_captures`. Read at every in-place mutation emit site.
    emitter.needs_cow = program.codegen_annotations.needs_cow.iter().map(|v| v.0).collect();
    emitter.global_alias = program.codegen_annotations.global_alias.iter()
        .map(|(k, v)| (k.0, v.0)).collect();

    // Phase 0: Collect `@intrinsic(symbol)` → (module, fn_name) from every
    // bundled stdlib source so the `RuntimeCall` fallback path can route
    // dispatch by the Almide fn name rather than by naively decoding the
    // runtime symbol. Needed when the runtime symbol differs from the
    // Almide fn name (e.g. `map.map` → `almide_rt_map_map_values`).
    {
        use almide_lang::ast::{AttrValue, Decl};
        for &mod_name in almide_lang::stdlib_info::BUNDLED_MODULES {
            let Some(source) = almide_lang::stdlib_info::bundled_source(mod_name) else { continue };
            let Some(parsed) = almide_lang::parse_cached(source) else { continue };
            for decl in &parsed.decls {
                let Decl::Fn { name, attrs, .. } = decl else { continue };
                let Some(attr) = attrs.iter().find(|a| a.name.as_str() == "intrinsic") else { continue };
                let Some(first) = attr.args.first() else { continue };
                let AttrValue::String { value: symbol } = &first.value else { continue };
                emitter.intrinsic_symbol_to_fn.insert(
                    symbol.clone(),
                    (mod_name.to_string(), name.to_string()),
                );
            }
        }
    }

    // Embed the Unicode case-mapping tables at the FRONT of the data section
    // (while data_bytes is still just the newline byte) when the program uses any
    // string case op. Gated to keep non-case-folding modules lean (~51KB tables).
    if program_uses_case_op(program) {
        emitter.embed_case_tables();
    }
    // Embed the libm 2/pi / PIO2 tables (front protected region, after case
    // tables, before any string interning) when the program uses trig.
    if program_uses_trig(program) {
        emitter.embed_libm_tables();
    }

    // Phase 1: Register types and function indices
    // Step 1a: WASI imports (must come first — all imports before any defined functions)
    runtime::register_runtime_imports(&mut emitter);

    // Store import info for fd_write
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "fd_write".to_string(),
        type_idx: emitter.types.iter().position(|(p, r)| {
            p == &[ValType::I32, ValType::I32, ValType::I32, ValType::I32]
                && r == &[ValType::I32]
        }).unwrap() as u32,
    });

    // Import clock_time_get: (id: i32, precision: i64, time_ptr: i32) -> i32
    let clock_type_idx = emitter.register_type(
        vec![ValType::I32, ValType::I64, ValType::I32],
        vec![ValType::I32],
    );
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "clock_time_get".to_string(),
        type_idx: clock_type_idx,
    });

    // Import proc_exit: (code: i32) -> ()
    let proc_exit_type_idx = emitter.register_type(vec![ValType::I32], vec![]);
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "proc_exit".to_string(),
        type_idx: proc_exit_type_idx,
    });

    // Import random_get: (buf: i32, len: i32) -> i32
    let random_get_type_idx = emitter.register_type(
        vec![ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "random_get".to_string(),
        type_idx: random_get_type_idx,
    });

    // Import path_open
    let path_open_type_idx = emitter.register_type(
        vec![
            ValType::I32, ValType::I32, ValType::I32, ValType::I32,
            ValType::I32, ValType::I64, ValType::I64, ValType::I32,
            ValType::I32,
        ],
        vec![ValType::I32],
    );
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "path_open".to_string(),
        type_idx: path_open_type_idx,
    });

    // Import fd_read
    let fd_read_type_idx = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "fd_read".to_string(),
        type_idx: fd_read_type_idx,
    });

    // Import fd_close
    let fd_close_type_idx = emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "fd_close".to_string(),
        type_idx: fd_close_type_idx,
    });

    // Import fd_seek
    let fd_seek_type_idx = emitter.register_type(
        vec![ValType::I32, ValType::I64, ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "fd_seek".to_string(),
        type_idx: fd_seek_type_idx,
    });

    // Import fd_filestat_get
    let fd_filestat_get_type_idx = emitter.register_type(
        vec![ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "fd_filestat_get".to_string(),
        type_idx: fd_filestat_get_type_idx,
    });

    // Import path_filestat_get
    let path_filestat_get_type_idx = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "path_filestat_get".to_string(),
        type_idx: path_filestat_get_type_idx,
    });

    // Import path_create_directory
    let path_create_directory_type_idx = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "path_create_directory".to_string(),
        type_idx: path_create_directory_type_idx,
    });

    // Import path_rename
    let path_rename_type_idx = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "path_rename".to_string(),
        type_idx: path_rename_type_idx,
    });

    // Import path_unlink_file
    let path_unlink_file_type_idx = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "path_unlink_file".to_string(),
        type_idx: path_unlink_file_type_idx,
    });

    // Import path_remove_directory
    let path_remove_directory_type_idx = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "path_remove_directory".to_string(),
        type_idx: path_remove_directory_type_idx,
    });

    // Import fd_prestat_get
    let fd_prestat_get_type_idx = emitter.register_type(
        vec![ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "fd_prestat_get".to_string(),
        type_idx: fd_prestat_get_type_idx,
    });

    // Import fd_prestat_dir_name
    let fd_prestat_dir_name_type_idx = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "fd_prestat_dir_name".to_string(),
        type_idx: fd_prestat_dir_name_type_idx,
    });

    // Import fd_readdir
    let fd_readdir_type_idx = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32, ValType::I64, ValType::I32],
        vec![ValType::I32],
    );
    emitter.imports.push(ImportInfo {
        module: "wasi_snapshot_preview1".to_string(),
        name: "fd_readdir".to_string(),
        type_idx: fd_readdir_type_idx,
    });

    // Step 1b: @extern(wasm, ...) imports — must be registered before any
    // defined functions so import indices are contiguous at the start.
    // Scan both program.functions and module functions.
    let mut extern_wasm_set: HashSet<usize> = HashSet::new();
    for (i, func) in program.functions.iter().enumerate() {
        if let Some(attr) = func.extern_attrs.iter().find(|a| a.target.as_str() == "wasm") {
            let params: Vec<ValType> = func.params.iter()
                .filter_map(|p| values::ty_to_valtype(&p.ty))
                .collect();
            let results = values::ret_type(&func.ret_ty);
            let type_idx = emitter.register_type(params, results);
            let func_idx = emitter.register_import(type_idx);
            emitter.imports.push(ImportInfo {
                module: attr.module.as_str().to_string(),
                name: attr.function.as_str().to_string(),
                type_idx,
            });
            emitter.func_map.insert(func.name.to_string(), func_idx);
            if func.is_effect {
                emitter.effect_fns.insert(func.name.to_string());
            }
            extern_wasm_set.insert(i);
        }
    }
    // Module @extern(wasm) imports: key = (module_idx, func_idx)
    let mut extern_wasm_module_set: HashSet<(usize, usize)> = HashSet::new();
    for (mi, module) in program.modules.iter().enumerate() {
        emitter.module_names.push(module.name.to_string());
        let mod_ident = module.versioned_name
            .map(|v| v.to_string().replace('.', "_"))
            .unwrap_or_else(|| module.name.to_string().replace('.', "_"));
        for (fi, func) in module.functions.iter().enumerate() {
            if let Some(attr) = func.extern_attrs.iter().find(|a| a.target.as_str() == "wasm") {
                let params: Vec<ValType> = func.params.iter()
                    .filter_map(|p| values::ty_to_valtype(&p.ty))
                    .collect();
                let results = values::ret_type(&func.ret_ty);
                let type_idx = emitter.register_type(params, results);
                let func_idx = emitter.register_import(type_idx);
                emitter.imports.push(ImportInfo {
                    module: attr.module.as_str().to_string(),
                    name: attr.function.as_str().to_string(),
                    type_idx,
                });
                // Register by prefixed, qualified, and bare name for call dispatch
                let func_name_sanitized = func.name.to_string().replace(' ', "_").replace('-', "_").replace('.', "_");
                let prefixed_name = format!("almide_rt_{}_{}", mod_ident, func_name_sanitized);
                emitter.func_map.insert(prefixed_name, func_idx);
                // Qualified name: "{module}.{func}" — preferred for disambiguation
                let module_name = module.name.to_string();
                let qualified_name = format!("{}.{}", module_name, func.name);
                emitter.func_map.insert(qualified_name, func_idx);
                // Bare name: last-write-wins (later modules override earlier ones
                // so intra-module calls resolve to the local function, not an
                // imported module's function with the same name)
                let bare_name = func.name.to_string();
                emitter.func_map.insert(bare_name, func_idx);
                if func.is_effect {
                    let effect_prefixed = format!("almide_rt_{}_{}", mod_ident, func_name_sanitized);
                    emitter.effect_fns.insert(effect_prefixed);
                }
                extern_wasm_module_set.insert((mi, fi));
            }
        }
    }

    // Step 1c: Runtime defined functions (after all imports are registered)
    runtime::register_runtime_functions(&mut emitter);

    // Register type declarations (record and variant field layouts).
    // Include both the main program and all imported modules so nominal
    // types from `import mod` resolve during codegen.
    // Register module type_decls first, then program's own (self) type_decls.
    // This ensures self types win over same-named dependency types in record_fields.
    let all_type_decls = program.modules.iter().flat_map(|m| m.type_decls.iter())
        .chain(program.type_decls.iter());
    for td in all_type_decls {
        match &td.kind {
            almide_ir::IrTypeDeclKind::Record { fields } => {
                let field_list: Vec<(String, almide_lang::types::Ty)> = fields.iter()
                    .map(|f| (f.name.to_string(), f.ty.clone()))
                    .collect();
                emitter.record_fields.insert(td.name.to_string(), field_list);
            }
            almide_ir::IrTypeDeclKind::Variant { cases, .. } => {
                let mut variant_cases = Vec::new();
                for (tag, case) in cases.iter().enumerate() {
                    let fields: Vec<(String, almide_lang::types::Ty)> = match &case.kind {
                        almide_ir::IrVariantKind::Record { fields } => {
                            fields.iter().map(|f| (f.name.to_string(), f.ty.clone())).collect()
                        }
                        almide_ir::IrVariantKind::Tuple { fields } => {
                            fields.iter().enumerate()
                                .map(|(i, ty)| (format!("_{}", i), ty.clone()))
                                .collect()
                        }
                        almide_ir::IrVariantKind::Unit => vec![],
                    };
                    // Also register each case name in record_fields for field access
                    emitter.record_fields.insert(case.name.to_string(), fields.clone());
                    variant_cases.push(VariantCase {
                        name: case.name.to_string(),
                        tag: tag as u32,
                        fields,
                    });
                }
                emitter.variant_info.insert(td.name.to_string(), variant_cases);
            }
            almide_ir::IrTypeDeclKind::Alias { .. } => {
                // Alias types are erased by ConcretizeTypesPass — nothing to register.
            }
        }
    }

    // Stdlib runtime types that aren't declared as Almide records but must
    // resolve for Member access (e.g. `resp.status`). Field offsets must
    // match the layout chosen by the corresponding stdlib emit (see
    // calls_http.rs `response`/`json`).
    use almide_lang::types::Ty as _Ty;
    emitter.record_fields.insert("HttpResponse".to_string(), vec![
        ("status".to_string(),  _Ty::Int),     // i64 @ 0
        ("body".to_string(),    _Ty::String),  // i32 ptr @ 8
        ("headers".to_string(),
            _Ty::Applied(almide_lang::types::TypeConstructorId::List, vec![
                _Ty::Tuple(vec![_Ty::String, _Ty::String]),
            ])),                                // i32 ptr @ 12
    ]);
    emitter.record_fields.insert("HttpRequest".to_string(), vec![
        ("method".to_string(),  _Ty::String),
        ("path".to_string(),    _Ty::String),
        ("body".to_string(),    _Ty::String),
        ("headers".to_string(),
            _Ty::Applied(almide_lang::types::TypeConstructorId::List, vec![
                _Ty::Tuple(vec![_Ty::String, _Ty::String]),
            ])),
    ]);

    // Also register all anonymous record shapes found in the IR under synthetic
    // names so `emit_member`'s Unknown-type fallback (which searches
    // `record_fields` by field name) can resolve Member access on Lambda
    // parameters whose type inference left them as TypeVar/Unknown.
    register_anonymous_records(program, &mut emitter);

    // Build default_fields from type declarations
    for td in &program.type_decls {
        match &td.kind {
            almide_ir::IrTypeDeclKind::Variant { cases, .. } => {
                for case in cases {
                    if let almide_ir::IrVariantKind::Record { fields } = &case.kind {
                        for f in fields {
                            if let Some(def) = &f.default {
                                emitter.default_fields.insert(
                                    (case.name.to_string(), f.name.to_string()), def.clone()
                                );
                            }
                        }
                    }
                }
            }
            almide_ir::IrTypeDeclKind::Record { fields } => {
                for f in fields {
                    if let Some(def) = &f.default {
                        emitter.default_fields.insert(
                            (td.name.to_string(), f.name.to_string()), def.clone()
                        );
                    }
                }
            }
            _ => {}
        }
    }

    // Register top-level let bindings as globals
    for tl in &program.top_lets {
        let global_idx = emitter.next_global;
        emitter.next_global += 1;
        let vt = values::ty_to_valtype(&tl.ty).unwrap_or(ValType::I64);
        // Extract const value for direct initialization (store as i64 bits)
        let const_bits: i64 = match &tl.value.kind {
            almide_ir::IrExprKind::LitInt { value } => *value,
            almide_ir::IrExprKind::LitFloat { value } => value.to_bits() as i64,
            almide_ir::IrExprKind::LitBool { value } => *value as i64,
            _ => 0, // computed values default to 0
        };
        emitter.top_let_globals.insert(tl.var.0, (global_idx, vt));
        let name = program.var_table.get(tl.var).name.to_string();
        emitter.top_let_globals_by_name.insert(name, (global_idx, vt));
        emitter.top_let_init.push((global_idx, vt, const_bits));
    }
    // Also register module top_lets as globals so cross-module access (synthetic
    // Var with `ALMIDE_RT_<MOD>_<NAME>` name) can resolve at WASM emit time.
    for module in &program.modules {
        for tl in &module.top_lets {
            let global_idx = emitter.next_global;
            emitter.next_global += 1;
            let vt = values::ty_to_valtype(&tl.ty).unwrap_or(ValType::I64);
            let const_bits: i64 = match &tl.value.kind {
                almide_ir::IrExprKind::LitInt { value } => *value,
                almide_ir::IrExprKind::LitFloat { value } => value.to_bits() as i64,
                almide_ir::IrExprKind::LitBool { value } => *value as i64,
                _ => 0,
            };
            let name = program.var_table.get(tl.var).name.to_string();
            emitter.top_let_globals_by_name.insert(name.clone(), (global_idx, vt));
            emitter.top_let_globals.insert(tl.var.0, (global_idx, vt));
            emitter.top_let_init.push((global_idx, vt, const_bits));
            // Register by DefId for direct cross-package resolution
            if let Some(def_id) = tl.def_id {
                emitter.def_globals.insert(def_id.0, (global_idx, vt));
            }

            // Also register under the ALMIDE_RT_<MOD>_<NAME> synthetic name
            // that cross-module access creates during lowering. Without this,
            // the name-keyed fallback in expressions.rs can't find the global.
            let mod_name = module.name.as_str();
            if !mod_name.is_empty() {
                // Register ALMIDE_RT_<MOD>_<NAME> under multiple name forms:
                // - Full module path: ALMIDE_RT_SNAIDHM_WEB_GPU_STORAGE
                // - VarTable name as-is (may include _V0_ versioning)
                // - Leaf segment only: ALMIDE_RT_GPU_STORAGE
                let segments: Vec<&str> = mod_name.split('.').collect();
                let leaf = segments.last().copied().unwrap_or(mod_name);
                for alias in [mod_name, leaf] {
                    let synthetic = format!(
                        "ALMIDE_RT_{}_{}",
                        alias.to_uppercase().replace('.', "_"),
                        name.to_uppercase(),
                    );
                    emitter.top_let_globals_by_name.insert(synthetic, (global_idx, vt));
                }
                // Also register the VarTable name itself (handles versioned names like ALMIDE_RT_SNAIDHM_V0_...)
                if name.starts_with("ALMIDE_RT_") {
                    emitter.top_let_globals_by_name.insert(name.clone(), (global_idx, vt));
                    // Strip version suffix: ALMIDE_RT_SNAIDHM_V0_WEB_GPU_STORAGE → ALMIDE_RT_SNAIDHM_WEB_GPU_STORAGE
                    // so that the unversioned lowering synthetic name can also match.
                    let stripped = name.replacen("_V0_", "_", 1);
                    if stripped != name {
                        emitter.top_let_globals_by_name.insert(stripped, (global_idx, vt));
                    }
                }
            }
        }
    }

    // Register function signatures.
    // Library mode (no main): skip test functions so the WASM module
    // can be loaded without a _start entry point.
    let mut user_meta: Vec<u32> = Vec::new();
    let mut user_func_indices: Vec<u32> = Vec::new();
    let mut test_func_indices: Vec<(u32, String)> = Vec::new();
    let has_main = program.functions.iter().any(|f| f.name == "main" && !f.is_test);
    let has_tests = program.functions.iter().any(|f| f.is_test);
    let library_mode = !has_main && !has_tests;

    for (func_enum_idx, func) in program.functions.iter().enumerate() {
        // Skip @extern(wasm) — already registered as imports above
        if extern_wasm_set.contains(&func_enum_idx) {
            continue;
        }
        // Library mode: skip test functions entirely
        if library_mode && func.is_test {
            continue;
        }
        // Resolve param and ret types: Unknown/TypeVar can leak through from
        // lifted lambdas whose outer `Ty::Fn` had unresolved entries. Fall back
        // to VarTable (for params) and expression inspection (for ret).
        let params: Vec<ValType> = func.params.iter()
            .filter_map(|p| {
                if func.name.contains("closure") || func.name.contains("lambda") {
                }
                let pty = if p.ty.is_unresolved_structural() {
                    let vt_ty = &program.var_table.get(p.var).ty;
                    if !vt_ty.is_unresolved_structural() {
                        vt_ty.clone()
                    } else {
                        p.ty.clone()
                    }
                } else {
                    p.ty.clone()
                };
                values::ty_to_valtype(&pty)
            })
            .collect();
        if func.name.contains("closure") || func.name.contains("lambda") {
        }
        // Function return type: use declared ret_ty, fall back to body.ty
        // (concretized by the ConcretizeTypes pass) when declared is Unknown.
        let resolved_ret_ty = if func.ret_ty.is_unresolved() {
            func.body.ty.clone()
        } else {
            func.ret_ty.clone()
        };
        let results = values::ret_type(&resolved_ret_ty);
        let type_idx = emitter.register_type(params, results);
        // Test blocks already carry `TEST_NAME_PREFIX` from lowering so
        // they cannot collide with user fns — use the name as-is.
        let reg_name = func.name.to_string();
        let func_idx = emitter.register_func(&reg_name, type_idx);
        user_meta.push(type_idx);
        user_func_indices.push(func_idx);
        if func.is_test {
            test_func_indices.push((func_idx, func.display_name().to_string()));
        }
        if func.is_effect {
            emitter.effect_fns.insert(func.name.to_string());
        }
    }

    // Register module functions (user packages, not stdlib)
    let mut module_func_meta: Vec<(usize, usize, u32)> = Vec::new(); // (module_idx, func_idx, type_idx)
    for (mi, module) in program.modules.iter().enumerate() {
        let mod_ident = module.versioned_name
            .map(|v| v.to_string().replace('.', "_"))
            .unwrap_or_else(|| module.name.to_string().replace('.', "_"));
        for (fi, func) in module.functions.iter().enumerate() {
            // Skip @extern(wasm) — already registered as imports
            if extern_wasm_module_set.contains(&(mi, fi)) {
                continue;
            }
            // Skip test functions defined in dependency modules: they are
            // only relevant when running tests on that module directly,
            // not when it's imported by another file. Including them would
            // emit extra closures whose function-table layout can conflict
            // with the top-level program's own closures.
            if func.is_test {
                continue;
            }
            // Stdlib Unification Stage 1: `@inline_rust` / `@wasm_intrinsic`
            // bundled fns are dispatch-only declarations. On the WASM
            // target, the call dispatch still goes through
            // `calls_<module>.rs` (TOML-backed intrinsics); the bundled
            // fn's body (typically `_` / Hole) is never needed and would
            // fail to compile. Skip registration + emission.
            if func.attrs.iter().any(|a|
                matches!(a.name.as_str(), "inline_rust" | "wasm_intrinsic" | "intrinsic"))
            {
                continue;
            }
            let func_name_sanitized = func.name.to_string().replace(' ', "_").replace('-', "_").replace('.', "_");
            // Test blocks carry `TEST_NAME_PREFIX` from lowering — no
            // additional conditional prefix needed here.
            let prefixed_name = format!("almide_rt_{}_{}", mod_ident, func_name_sanitized);
            let params: Vec<ValType> = func.params.iter()
                .filter_map(|p| values::ty_to_valtype(&p.ty))
                .collect();
            let results = values::ret_type(&func.ret_ty);
            let type_idx = emitter.register_type(params, results);
            let func_idx = emitter.register_func(&prefixed_name, type_idx);
            // Register qualified name: "{module}.{func}" for intra-module resolution
            let module_name_str = module.name.to_string();
            let qualified_name = format!("{}.{}", module_name_str, func.name);
            emitter.func_map.insert(qualified_name, func_idx);
            // Also register by bare name so lifted closures from this module
            // can call module-local functions. ClosureConversion lifts lambdas
            // from modules to program.functions, but their Named call targets
            // use the unqualified function name. Skip tests — tests must not
            // shadow user functions.
            if !func.is_test {
                let bare_name = func.name.to_string();
                if !emitter.func_map.contains_key(&bare_name) {
                    emitter.func_map.insert(bare_name, func_idx);
                }
            }
            module_func_meta.push((mi, fi, type_idx));
            user_func_indices.push(func_idx);
            if func.is_effect {
                emitter.effect_fns.insert(prefixed_name);
            }
        }
    }

    // Check if any top-level let needs dynamic initialization (non-constant values).
    // LitStr needs init because string pointers are resolved at runtime via data section.
    let is_dyn = |tl: &almide_ir::IrTopLet| !matches!(&tl.value.kind,
        almide_ir::IrExprKind::LitInt { .. } | almide_ir::IrExprKind::LitFloat { .. } |
        almide_ir::IrExprKind::LitBool { .. }
    );
    let needs_init = program.top_lets.iter().any(is_dyn)
        || program.modules.iter().any(|m| m.top_lets.iter().any(is_dyn));
    let init_globals_idx: Option<u32> = if needs_init {
        let void_ty = emitter.register_type(vec![], vec![]);
        let idx = emitter.register_func("__init_globals", void_ty);
        Some(idx)
    } else {
        None
    };

    // If no main but has tests, register a test runner as _start
    let test_runner_idx = if !has_main && !test_func_indices.is_empty() {
        let void_ty = emitter.register_type(vec![], vec![]);
        let idx = emitter.register_func("__test_runner", void_ty);
        Some(idx)
    } else {
        None
    };

    // If `main` exists, wrap it in a void `__main_runner` so the exported
    // `_start` is a clean WASI command `() -> ()`. `main` is an effect fn that
    // returns a Result (an i32 at the wasm boundary); exporting it directly as
    // `_start` leaves the entry non-void, so wasmtime runs it via `--invoke`
    // and prints the return value to stdout — corrupting any observable-output
    // capture (e.g. a cross-target equivalence diff). This occupies exactly the
    // `__test_runner` slot (mutually exclusive with it), inheriting the same
    // proven registration/compile ordering relative to closures and globals.
    let main_runner_idx = if has_main {
        let void_ty = emitter.register_type(vec![], vec![]);
        Some(emitter.register_func("__main_runner", void_ty))
    } else {
        None
    };

    // Pre-scan for lambdas and FnRefs — only these need element table entries.
    // (Previously all user functions were added unconditionally, bloating the
    // element table and preventing DCE from eliminating unused functions.)
    closures::pre_scan_closures(program, &mut emitter);

    // Pre-register variant deep-equality functions (must be before compilation starts)
    register_variant_eq_funcs(&mut emitter);

    // Pre-register per-type Almide-literal repr functions (recursive ADTs walk via
    // call, not inline expansion). Must reserve indices before compilation starts.
    register_repr_funcs(&mut emitter, program);

    // Phase 2: Compile function bodies (order must match registration order)
    runtime::compile_runtime(&mut emitter);

    // User + test functions (skip @extern(wasm) — they are imports, not defined)
    let mut user_idx = 0;
    for (func_enum_idx, func) in program.functions.iter().enumerate() {
        if extern_wasm_set.contains(&func_enum_idx) {
            continue;
        }
        if library_mode && func.is_test {
            continue;
        }
        let type_idx = user_meta[user_idx];
        // Pass init_globals_idx to main function so top-level lets get initialized
        let is_main = func.name == "main" && !func.is_test;
        let init_idx = if is_main { init_globals_idx } else { None };
        let compiled = functions::compile_function_with_init(&mut emitter, func, &program.var_table, type_idx, init_idx);
        emitter.add_compiled(compiled);
        user_idx += 1;
    }

    // Module functions (user packages). VarIds already point into the
    // unified `program.var_table` (see `pass_unify_var_tables`).
    for &(mi, fi, type_idx) in &module_func_meta {
        let module = &program.modules[mi];
        let func = &module.functions[fi];
        let mod_name = module.name.to_string();
        let compiled = functions::compile_module_function(&mut emitter, func, &program.var_table, type_idx, &mod_name);
        emitter.add_compiled(compiled);
    }

    // Init globals (dynamic top-level let initialization, must come before test runner)
    if init_globals_idx.is_some() {
        compile_init_globals(&mut emitter, program);
    }

    // Test runner (if needed)
    if let Some(_runner_idx) = test_runner_idx {
        compile_test_runner(&mut emitter, &test_func_indices, init_globals_idx);
    }

    // Main runner (mirrors the test-runner slot; mutually exclusive with it).
    // `main` already runs `__init_globals` itself (init_idx passed at its
    // compilation), so the runner only calls `main` and drops its Result.
    if main_runner_idx.is_some() {
        let main_idx = *emitter.func_map.get("main")
            .expect("has_main implies a registered `main`");
        let main_func = program.functions.iter().find(|f| f.name == "main" && !f.is_test);
        // Only `effect fn main` returns a `Result` that can carry an unhandled
        // error. A plain `fn main` (`Unit`) cannot fail — never tag-check it, or
        // its `Unit` payload would be misread as an `Err` tag and abort every run.
        let is_effect = main_func.map(|f| f.is_effect).unwrap_or(false);
        let drop_count = main_func
            .map(|f| {
                let ret = if f.ret_ty.is_unresolved() { f.body.ty.clone() } else { f.ret_ty.clone() };
                values::ret_type(&ret).len()
            })
            .unwrap_or(0);
        compile_main_runner(&mut emitter, main_idx, drop_count, is_effect);
    }

    // Lambda bodies and FnRef wrappers
    closures::compile_lambda_bodies(program, &mut emitter);

    // Compile variant deep-equality functions (bodies, after all user code)
    compile_variant_eq_funcs(&mut emitter, &program.var_table);

    // Compile per-type repr function bodies (after eq funcs, same sorted-order
    // index/body contract). Order relative to eq funcs matches the registration
    // order above (eq funcs reserved first, then repr funcs).
    compile_repr_funcs(&mut emitter, &program.var_table);

    // Collect public user functions for WASM export (skip imports) BEFORE DCE.
    // A host-driven export (`render_frame`, `on_pointer_*`, any JS-called `pub fn`)
    // is often unreachable from `main`/`_start`; if DCE runs first it stubs the body
    // to `unreachable`, and the export then traps on the first host call (#457). By
    // populating `user_exports` here, DCE seeds these as roots and keeps their bodies.
    // @export(wasm, "symbol") overrides the export name; otherwise use fn name.
    for (func_enum_idx, func) in program.functions.iter().enumerate() {
        if extern_wasm_set.contains(&func_enum_idx) { continue; }
        if func.is_test { continue; }
        if !matches!(func.visibility, almide_ir::IrVisibility::Public) { continue; }
        if func.generics.as_ref().map_or(false, |g| !g.is_empty()) { continue; }
        if func.name.as_str() == "main" { continue; }
        let internal_name = func.name.to_string();
        let export_name = func.export_attrs.iter()
            .find(|a| a.target.as_str() == "wasm")
            .map(|a| a.symbol.to_string())
            .unwrap_or_else(|| internal_name.clone());
        emitter.user_exports.push((export_name, internal_name));
    }

    // Phase 2.5: Dead Code Elimination (exported `pub fn`s above are roots)
    let dce_count = dce::eliminate_dead_code(&mut emitter);

    // Phase 2.6: Dead Data Elimination — remove unreferenced string constants
    let _data_dce_bytes = dce::eliminate_dead_data(&mut emitter);

    // Phase 3: Assemble (DCE already ran in Phase 2.5: {} functions eliminated)
    let _ = dce_count;
    let bytes = assemble(&mut emitter);

    // Phase 4: Validate — mechanical guarantee of structural correctness.
    // ALWAYS-ON and FATAL (release-parity, completeness §10): this used to be
    // debug-only and print-only, and an invalid module (a Unit tail var
    // pushing a phantom value — caught by the §2 matrix gate) shipped through
    // it for as long as the shape existed, runnable only because wasm-opt
    // happened to repair it on machines that have binaryen installed. The
    // wasmtime-facing artifact must never depend on an optional external
    // sanitizer; validation costs milliseconds at these module sizes.
    if let Err(e) = wasmparser::validate(&bytes) {
        eprintln!("error: [COMPILER BUG] emitted WASM failed structural validation");
        eprintln!("  {e}");
        eprintln!("  The module would be rejected by any spec-compliant runtime. This is a");
        eprintln!("  compiler bug, not an error in your program.");
        eprintln!("  Please report this at https://github.com/almide/almide/issues");
        std::process::exit(1);
    }

    // Phase 5: RC balance verification — mathematical double-free prevention.
    //
    // For each user function, count RcDec statements in the IR and
    // call(rc_dec) instructions in the emitted WASM. If the WASM has MORE
    // rc_dec calls than the IR specifies (accounting for typed child drops),
    // it's a compiler bug that could cause double-free.
    //
    // This is a static, post-emit check — no runtime overhead, no function
    // index perturbation. Combined with PerceusVerifyPass (Lean 4 certified
    // IR-level balance) and Verified<'_> type-state gate, this closes the
    // gap between IR verification and WASM emission. ALWAYS-ON (§10): it is
    // a cheap per-function instruction count, and a violation in the
    // double-free direction must stop a release build too.
    verify_rc_balance(program, &emitter);

    bytes
}

/// Post-emit RC balance verification.
///
/// Counts call(rc_dec) in each compiled function's call_targets and compares
/// with the IR-level RcDec count. Extra rc_dec calls (beyond typed child
/// drops) indicate a compiler bug.
fn verify_rc_balance(program: &IrProgram, emitter: &WasmEmitter) {
    use almide_ir::{IrStmtKind, IrExprKind};
    use almide_ir::visit::{IrVisitor, walk_expr, walk_stmt};

    let rc_dec_fn = emitter.rt.rc_dec;

    // Count IR-level RcDec statements per function
    struct RcDecCounter { count: usize }
    impl IrVisitor for RcDecCounter {
        fn visit_stmt(&mut self, stmt: &almide_ir::IrStmt) {
            if matches!(&stmt.kind, IrStmtKind::RcDec { .. }) {
                self.count += 1;
            }
            walk_stmt(self, stmt);
        }
        fn visit_expr(&mut self, expr: &almide_ir::IrExpr) {
            walk_expr(self, expr);
        }
    }

    for (i, func) in program.functions.iter().enumerate() {
        // Count IR RcDec
        let mut counter = RcDecCounter { count: 0 };
        counter.visit_expr(&func.body);
        let ir_dec_count = counter.count;

        // Find compiled function's call targets
        // User functions start after runtime functions in compiled[]
        // We match by name via func_map
        let func_name = func.name.to_string();
        if let Some(&func_idx) = emitter.func_map.get(&func_name) {
            // Count call(rc_dec) in call_targets
            let compiled_idx = func_idx as usize - emitter.num_imports as usize;
            if compiled_idx < emitter.compiled.len() {
                let wasm_dec_count = emitter.compiled[compiled_idx]
                    .call_targets.iter()
                    .filter(|&&t| t == rc_dec_fn)
                    .count();

                // The WASM may have MORE rc_dec calls than the IR because
                // emit_typed_rc_dec generates child drops. But it should
                // never have FEWER (that would be a leak, caught by
                // PerceusVerifyPass). We log mismatches for debugging.
                if wasm_dec_count < ir_dec_count {
                    // The IR (Verified by PerceusVerifyPass) specifies the
                    // balance; an emission that DROPS a Dec is a leak the
                    // belt already certified against. No warn-mode (§10).
                    eprintln!("error: [COMPILER BUG] WASM emission dropped RC decrements");
                    eprintln!(
                        "  `{}` has {} IR RcDec statement(s) but only {} emitted rc_dec call(s).",
                        func_name, ir_dec_count, wasm_dec_count,
                    );
                    eprintln!("  Please report this at https://github.com/almide/almide/issues");
                    std::process::exit(1);
                }
            }
        }
    }
}

/// Assemble all sections into a final WASM binary.
fn assemble(emitter: &mut WasmEmitter) -> Vec<u8> {
    let mut module = Module::new();

    // ── Type section ──
    let mut types = TypeSection::new();
    for (params, results) in &emitter.types {
        types.ty().function(params.iter().copied(), results.iter().copied());
    }
    module.section(&types);

    // ── Import section ──
    let mut imports = ImportSection::new();
    for info in &emitter.imports {
        imports.import(
            &info.module,
            &info.name,
            wasm_encoder::EntityType::Function(info.type_idx),
        );
    }
    module.section(&imports);

    // ── Function section (type indices for defined functions) ──
    let mut functions = FunctionSection::new();
    for cf in &emitter.compiled {
        functions.function(cf.type_idx);
    }
    module.section(&functions);

    // ── Table section (for call_indirect / FnRef) ──
    if !emitter.func_table.is_empty() {
        let mut tables = TableSection::new();
        tables.table(TableType {
            element_type: RefType::FUNCREF,
            minimum: emitter.func_table.len() as u64,
            maximum: Some(emitter.func_table.len() as u64),
            table64: false,
            shared: false,
        });
        module.section(&tables);
    }

    // ── Memory section ──
    // Single memory layout (iOS-Safari compatible):
    //   [data segment][heap ...]
    // The heap grows upward via `__alloc`. There is no reserved scratch
    // region — string interpolation builds results inline directly on the
    // heap (see `calls_string::emit_string_interp`).
    // Data layout: [data bytes][8-byte alignment][heap...]. The active data
    // segment (newline + embedded case tables + interned string literals) is
    // written into linear memory at instantiation, so the INITIAL memory must
    // already cover it — derive the page count from the heap start (>= data_end)
    // rather than a fixed 2 pages. The ~51KB case tables roughly halve the
    // literal headroom of the old fixed 128KB minimum, so a large-literal
    // case-folding program could otherwise overrun it and fail to instantiate.
    let data_end = NEWLINE_OFFSET + emitter.data_bytes.len() as u32;
    let heap_start_aligned = (data_end + 7) & !7;
    const WASM_PAGE_BYTES: u32 = 65536;
    let min_pages = heap_start_aligned.div_ceil(WASM_PAGE_BYTES).max(2);
    let mut memory = MemorySection::new();
    memory.memory(MemoryType {
        minimum: min_pages as u64,  // covers the full data region; allocator grows from here
        maximum: Some(65536),   // 4GB max (WASM32 hard limit) — explicit so V8 doesn't apply a smaller default
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    module.section(&memory);

    // ── Global section ──
    let mut globals = GlobalSection::new();
    // Global 0: heap pointer (memory 0)
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &wasm_encoder::ConstExpr::i32_const(heap_start_aligned as i32),
    );
    // Global 1: free list head (Perceus reuse, 0 = empty)
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &wasm_encoder::ConstExpr::i32_const(0),
    );
    // Global 2: preopen table pointer
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &wasm_encoder::ConstExpr::i32_const(0),
    );
    // Global 2: preopen count (set by __init_preopen_dirs at startup)
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &wasm_encoder::ConstExpr::i32_const(0),
    );
    // Global 4: heap_start (immutable) — pointers below this are data section, not heap
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: false,
            shared: false,
        },
        &wasm_encoder::ConstExpr::i32_const(heap_start_aligned as i32),
    );
    emitter.rt.heap_start_global = runtime::HEAP_START_GLOBAL_IDX;

    // Top-level let globals
    for &(_, vt, bits) in &emitter.top_let_init {
        let init = match vt {
            ValType::I64 => wasm_encoder::ConstExpr::i64_const(bits),
            ValType::F64 => wasm_encoder::ConstExpr::f64_const(f64::from_bits(bits as u64).into()),
            ValType::I32 => wasm_encoder::ConstExpr::i32_const(bits as i32),
            _ => wasm_encoder::ConstExpr::i32_const(0),
        };
        globals.global(
            GlobalType { val_type: vt, mutable: true, shared: false },
            &init,
        );
    }
    module.section(&globals);

    // ── Export section ──
    let mut exports = ExportSection::new();
    exports.export("memory", wasm_encoder::ExportKind::Memory, 0);
    if let Some(&runner_idx) = emitter.func_map.get("__main_runner") {
        // Void wrapper around `main` — keeps `_start` a clean WASI command.
        exports.export("_start", wasm_encoder::ExportKind::Func, runner_idx);
    } else if let Some(&main_idx) = emitter.func_map.get("main") {
        exports.export("_start", wasm_encoder::ExportKind::Func, main_idx);
    } else if let Some(&runner_idx) = emitter.func_map.get("__test_runner") {
        exports.export("_start", wasm_encoder::ExportKind::Func, runner_idx);
    } else if let Some(&init_idx) = emitter.func_map.get("__init_globals") {
        exports.export("_start", wasm_encoder::ExportKind::Func, init_idx);
    }
    // Export __alloc for FFI callers to allocate WASM linear memory
    if let Some(&alloc_idx) = emitter.func_map.get("__alloc") {
        exports.export("__alloc", wasm_encoder::ExportKind::Func, alloc_idx);
    }
    // Export __heap_save / __heap_restore so JS-side wrappers can implement
    // scoped (arena-style) cleanup after each foreign call. Without these
    // the bump allocator never frees and long-running benchmarks OOM.
    if let Some(&idx) = emitter.func_map.get("__heap_save") {
        exports.export("__heap_save", wasm_encoder::ExportKind::Func, idx);
    }
    if let Some(&idx) = emitter.func_map.get("__heap_restore") {
        exports.export("__heap_restore", wasm_encoder::ExportKind::Func, idx);
    }
    // Export public user functions (collected during emit)
    for (export_name, internal_name) in &emitter.user_exports {
        if let Some(&idx) = emitter.func_map.get(internal_name.as_str()) {
            exports.export(export_name, wasm_encoder::ExportKind::Func, idx);
        }
    }
    module.section(&exports);

    // ── Element section (populate function table, must come before Code) ──
    if !emitter.func_table.is_empty() {
        let mut elements = ElementSection::new();
        elements.active(
            Some(0),
            &wasm_encoder::ConstExpr::i32_const(0),
            Elements::Functions(std::borrow::Cow::Borrowed(&emitter.func_table)),
        );
        module.section(&elements);
    }

    // ── Code section ──
    let mut codes = CodeSection::new();
    for cf in &emitter.compiled {
        if let Some(ref patched) = cf.patched_body {
            codes.raw(patched);
        } else {
            codes.function(&cf.func);
        }
    }
    module.section(&codes);

    // ── Data section ──
    let mut data = DataSection::new();
    // Newline byte + string literals, starting at NEWLINE_OFFSET
    if !emitter.data_bytes.is_empty() {
        data.active(
            0,
            &wasm_encoder::ConstExpr::i32_const(NEWLINE_OFFSET as i32),
            emitter.data_bytes.iter().copied(),
        );
    }
    module.section(&data);

    // ── Custom `name` section ──
    // Attribute functions by name so a trap (e.g. a `RuntimeError: unreachable`
    // surfaced in the browser playground) points at a named function instead of
    // an anonymous `wasm-function[N]`. Built from func_map sorted by index, so it
    // is host-deterministic (same as the rest of the module).
    let mut fn_index_names: Vec<(u32, &str)> =
        emitter.func_map.iter().map(|(name, &idx)| (idx, name.as_str())).collect();
    fn_index_names.sort_by_key(|(idx, _)| *idx);
    fn_index_names.dedup_by_key(|(idx, _)| *idx);
    if !fn_index_names.is_empty() {
        let mut fn_names = NameMap::new();
        for (idx, name) in &fn_index_names {
            fn_names.append(*idx, name);
        }
        let mut names = NameSection::new();
        names.functions(&fn_names);
        module.section(&names);
    }

    module.finish()
}

// ── Test runner ─────────────────────────────────────────────────

/// Compile the __init_globals function.
#[allow(dead_code)] // Will be activated when top-let WASM codegen is wired up
fn compile_init_globals(emitter: &mut WasmEmitter, program: &IrProgram) {
    // C-007 by construction (§4 stage 3): this function's emission order
    // (root top-lets, then per-module) must BE `global_init_order` — the
    // same vector the native main wrapper derives its eager forces from.
    // Asserted rather than re-derived: a future reorder of either side
    // becomes a build failure, not an eager-vs-init cross-target drift.
    {
        let emitted: Vec<almide_ir::VarId> = program.top_lets.iter().map(|tl| tl.var)
            .chain(program.modules.iter().flat_map(|m| m.top_lets.iter().map(|tl| tl.var)))
            .collect();
        assert_eq!(
            emitted, program.codegen_annotations.global_init_order,
            "[COMPILER BUG] __init_globals emission order diverged from global_init_order (C-007)"
        );
    }
    let void_type = emitter.register_type(vec![], vec![]);

    let mut local_decls = Vec::new();
    // ScratchAllocator locals
    // Generous fixed scratch caps — see functions.rs note (#417).
    let scratch_i32_cap = 64usize;
    let scratch_i64_cap = 48usize;
    let scratch_f64_cap = 48usize;
    let scratch_i32_base = local_decls.len() as u32;
    for _ in 0..scratch_i32_cap { local_decls.push((1, ValType::I32)); }
    let scratch_i64_base = local_decls.len() as u32;
    for _ in 0..scratch_i64_cap { local_decls.push((1, ValType::I64)); }
    let scratch_f64_base = local_decls.len() as u32;
    for _ in 0..scratch_f64_cap { local_decls.push((1, ValType::F64)); }
    let scratch_v128_cap = 8usize;
    let scratch_v128_base = local_decls.len() as u32;
    for _ in 0..scratch_v128_cap { local_decls.push((1, ValType::V128)); }

    let wasm_func = TrackedFunction::new(local_decls);
    let compiled_func = {
        let mut scratch_alloc = scratch::ScratchAllocator::new();
        scratch_alloc.set_bases_with_capacity(scratch_i32_base, scratch_i32_cap, scratch_i64_base, scratch_i64_cap, scratch_f64_base, scratch_f64_cap);
        scratch_alloc.set_v128_base(scratch_v128_base);
        let mut compiler = FuncCompiler {
            emitter: &mut *emitter,
            func: wasm_func,
            var_map: HashMap::new(),
            depth: 0,
            loop_stack: Vec::new(),
            scratch: scratch_alloc,
            var_table: &program.var_table,
            stub_ret_ty: Ty::Unit,
            current_module_name: None,
        };

        for tl in &program.top_lets {
            compiler.emit_expr(&tl.value);
            if let Some(&(global_idx, _)) = compiler.emitter.top_let_globals.get(&tl.var.0) {
                compiler.func.instruction(&wasm_encoder::Instruction::GlobalSet(global_idx));
            }
        }
        // Also initialize cross-module top_lets via name lookup (their VarIds
        // belong to per-module var_tables, so id-keyed top_let_globals can't
        // resolve them; we use the prefixed name set up at registration time).
        compiler.func
    };
    // Append module top_let initializers to the same function body. Each
    // module needs its own var_table for the FuncCompiler ctx, so we re-build
    // a compiler per module and append instructions.
    let mut compiled_func = compiled_func;
    for module in &program.modules {
        if module.top_lets.is_empty() { continue; }
        let mut scratch_alloc = scratch::ScratchAllocator::new();
        scratch_alloc.set_bases_with_capacity(scratch_i32_base, scratch_i32_cap, scratch_i64_base, scratch_i64_cap, scratch_f64_base, scratch_f64_cap);
        scratch_alloc.set_v128_base(scratch_v128_base);
        let mut mc = FuncCompiler {
            emitter: &mut *emitter,
            func: compiled_func,
            var_map: HashMap::new(),
            depth: 0,
            loop_stack: Vec::new(),
            scratch: scratch_alloc,
            var_table: &program.var_table,
            stub_ret_ty: Ty::Unit,
            current_module_name: None,
        };
        for tl in &module.top_lets {
            mc.emit_expr(&tl.value);
            // Module top-let VarIds now index into `program.var_table`
            // thanks to `UnifyVarTablesPass`, so the id-keyed map is
            // the primary lookup; the name-keyed mirror is a backup.
            if let Some(&(global_idx, _)) = mc.emitter.top_let_globals.get(&tl.var.0) {
                mc.func.instruction(&wasm_encoder::Instruction::GlobalSet(global_idx));
            } else if let Some(&(global_idx, _)) = mc.emitter.top_let_globals_by_name.get(program.var_table.get(tl.var).name.as_str()) {
                mc.func.instruction(&wasm_encoder::Instruction::GlobalSet(global_idx));
            } else {
                mc.func.instruction(&wasm_encoder::Instruction::Drop);
            }
        }
        compiled_func = mc.func;
    }
    let compiled_func = {
        let mut f = compiled_func;
        f.instruction(&wasm_encoder::Instruction::End);
        f
    };

    emitter.add_compiled(CompiledFunc::tracked(void_type, compiled_func));
}

/// Compile a test runner function that calls each test, printing results.
fn compile_test_runner(emitter: &mut WasmEmitter, tests: &[(u32, String)], init_globals: Option<u32>) {
    let void_type = emitter.register_type(vec![], vec![]);
    let mut f = TrackedFunction::new([]);

    // Initialize globals if needed
    if let Some(init_idx) = init_globals {
        f.instruction(&wasm_encoder::Instruction::Call(init_idx));
    }

    for (func_idx, test_name) in tests {
        // Print test name
        let name_str = emitter.intern_string(&format!("test: {} ... ", test_name));
        f.instruction(&wasm_encoder::Instruction::I32Const(name_str as i32));
        f.instruction(&wasm_encoder::Instruction::Call(emitter.rt.println_str));

        // Call the test function (it will trap on assert_eq failure)
        f.instruction(&wasm_encoder::Instruction::Call(*func_idx));

        // If we get here, test passed
        let pass_str = emitter.intern_string("ok");
        f.instruction(&wasm_encoder::Instruction::I32Const(pass_str as i32));
        f.instruction(&wasm_encoder::Instruction::Call(emitter.rt.println_str));
    }

    f.instruction(&wasm_encoder::Instruction::End);
    emitter.add_compiled(CompiledFunc::tracked(void_type, f));
}

/// Compile `__main_runner`: call `main` and drop its result so the exported
/// `_start` is a void WASI command. `drop_count` is `main`'s wasm result arity
/// (0 for a void `main`, 1 for an effect fn returning a `Result`). `main` runs
/// `__init_globals` itself, so the runner does nothing else.
fn compile_main_runner(emitter: &mut WasmEmitter, main_idx: u32, drop_count: usize, is_effect: bool) {
    use wasm_encoder::Instruction as Ins;
    fn m(offset: u64) -> wasm_encoder::MemArg {
        wasm_encoder::MemArg { offset, align: 2, memory_index: 0 }
    }
    let void_type = emitter.register_type(vec![], vec![]);

    // Non-effect `fn main` (returns `Unit`) cannot fail: call and drop its result.
    if !is_effect {
        let mut f = TrackedFunction::new([]);
        f.instruction(&Ins::Call(main_idx));
        for _ in 0..drop_count {
            f.instruction(&Ins::Drop);
        }
        f.instruction(&Ins::End);
        emitter.add_compiled(CompiledFunc::tracked(void_type, f));
        return;
    }

    // `effect fn main` returns `Result<Unit, String>` (`[tag:i32@0][payload@4]`).
    // On `Err`, write `Error: <msg>\n` to stderr (fd 2) and `proc_exit(1)` so the
    // wasm command's failure (non-zero exit + stderr) matches native byte-for-byte
    // (native's `fn main` wrapper emits the same `Error: <msg>` via Display + exit).
    // On `Ok` (tag 0), fall through and return normally → exit 0.
    let err_prefix = emitter.intern_string("Error: ") as i32;
    let newline = emitter.intern_string("\n") as i32;
    let data_off = emitter.layout_reg.fixed_offset(
        engine::layout::STRING, engine::layout::string::DATA) as i32;
    let concat = emitter.rt.concat_str;
    let fd_write = emitter.rt.fd_write;
    let proc_exit = emitter.rt.proc_exit;

    // locals: 0 = main's Result ptr, 1 = composed "Error: <msg>\n" string ptr
    let mut f = TrackedFunction::new([(2u32, ValType::I32)]);
    f.instruction(&Ins::Call(main_idx));
    f.instruction(&Ins::LocalSet(0));
    f.instruction(&Ins::LocalGet(0));
    f.instruction(&Ins::I32Load(m(0)));                    // tag
    f.instruction(&Ins::If(wasm_encoder::BlockType::Empty));
    //   msg = "Error: " ++ <err String @ payload> ++ "\n"
    f.instruction(&Ins::I32Const(err_prefix));
    f.instruction(&Ins::LocalGet(0));
    f.instruction(&Ins::I32Load(m(4)));                    // err String ptr
    f.instruction(&Ins::Call(concat));
    f.instruction(&Ins::I32Const(newline));
    f.instruction(&Ins::Call(concat));
    f.instruction(&Ins::LocalSet(1));
    //   iov[0] = { buf: msg + DATA, len: *msg } at scratch [0..8); nwritten at 8
    f.instruction(&Ins::I32Const(0));
    f.instruction(&Ins::LocalGet(1));
    f.instruction(&Ins::I32Const(data_off));
    f.instruction(&Ins::I32Add);
    f.instruction(&Ins::I32Store(m(0)));
    f.instruction(&Ins::I32Const(4));
    f.instruction(&Ins::LocalGet(1));
    f.instruction(&Ins::I32Load(m(0)));
    f.instruction(&Ins::I32Store(m(0)));
    //   fd_write(fd=2 stderr, iovs=0, iovs_len=1, nwritten=8)
    f.instruction(&Ins::I32Const(2));
    f.instruction(&Ins::I32Const(0));
    f.instruction(&Ins::I32Const(1));
    f.instruction(&Ins::I32Const(8));
    f.instruction(&Ins::Call(fd_write));
    f.instruction(&Ins::Drop);
    f.instruction(&Ins::I32Const(1));
    f.instruction(&Ins::Call(proc_exit));
    f.instruction(&Ins::End);                              // end if
    f.instruction(&Ins::End);                              // end function
    emitter.add_compiled(CompiledFunc::tracked(void_type, f));
}

/// Pre-register variant deep-equality functions for all variant types with pointer fields.
/// Must be called before Phase 2 (compilation) so func_idx is known at emit time.
fn register_variant_eq_funcs(emitter: &mut WasmEmitter) {
    let type_idx = emitter.register_type(
        vec![ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    // Collect variant names that need deep eq (have pointer fields).
    // Sort: variant_info is a HashMap, and its iteration order (host-dependent —
    // hash seed + usize bucket layout) here determines the func indices these
    // __eq_* functions reserve. Unsorted, a 32-bit host (wasm32) reserves them
    // in a different order than a 64-bit host, shifting every later function's
    // index and producing a divergent, trapping module. Sorting makes index
    // reservation a pure function of the program, and must match the (also
    // sorted) compile order in compile_variant_eq_funcs.
    let mut names: Vec<String> = emitter.variant_info.iter()
        .filter(|(_, cases)| {
            cases.iter().any(|c| c.fields.iter().any(|(_, ft)| {
                !matches!(ft, almide_lang::types::Ty::Int | almide_lang::types::Ty::Float | almide_lang::types::Ty::Bool | almide_lang::types::Ty::Unit)
            }))
        })
        .map(|(name, _)| name.clone())
        .collect();
    names.sort();
    for name in names {
        let func_idx = emitter.register_func(&format!("__eq_{}", name), type_idx);
        emitter.eq_funcs.insert(name, func_idx);
    }
}

/// Compile variant deep-equality function bodies.
/// Each function: (a: i32, b: i32) -> i32 — compares tag then dispatches to per-case field comparison.
fn compile_variant_eq_funcs(emitter: &mut WasmEmitter, var_table: &almide_ir::VarTable) {
    // Collect eq_funcs entries (name → func_idx) and corresponding cases.
    // Sort by name so body-emission order matches the (sorted) index-reservation
    // order in register_variant_eq_funcs. add_compiled pushes bodies positionally
    // (function index = num_imports + push position), so the two orders MUST
    // agree, and both must be host-independent — otherwise a 32-bit host places
    // __eq_Foo's body at __eq_Bar's index and the module traps.
    let mut eq_entries: Vec<(String, u32)> = emitter.eq_funcs.iter()
        .map(|(n, &idx)| (n.clone(), idx))
        .collect();
    eq_entries.sort();

    for (name, _func_idx) in &eq_entries {
        let cases = match emitter.variant_info.get(name.as_str()) {
            Some(c) => c.clone(),
            None => continue,
        };

        let type_idx = emitter.register_type(
            vec![ValType::I32, ValType::I32],
            vec![ValType::I32],
        );

        // Build function body with its own FuncCompiler
        let mut local_decls = Vec::new();
        let scratch_i32_cap = 16usize;
        let scratch_i64_cap = 8usize;
        let scratch_f64_cap = 2usize;
        let scratch_i32_base = 2u32; // after 2 params
        for _ in 0..scratch_i32_cap { local_decls.push((1, ValType::I32)); }
        let scratch_i64_base = scratch_i32_base + scratch_i32_cap as u32;
        for _ in 0..scratch_i64_cap { local_decls.push((1, ValType::I64)); }
        let scratch_f64_base = scratch_i64_base + scratch_i64_cap as u32;
        for _ in 0..scratch_f64_cap { local_decls.push((1, ValType::F64)); }

        let wasm_func = TrackedFunction::new(local_decls);
        let mut scratch_alloc = scratch::ScratchAllocator::new();
        scratch_alloc.set_bases_with_capacity(
            scratch_i32_base, scratch_i32_cap,
            scratch_i64_base, scratch_i64_cap,
            scratch_f64_base, scratch_f64_cap,
        );

        let compiled_func = {
            let mut compiler = FuncCompiler {
                emitter: &mut *emitter,
                func: wasm_func,
                var_map: std::collections::HashMap::new(),
                depth: 0,
                loop_stack: Vec::new(),
                scratch: scratch_alloc,
                var_table,
                stub_ret_ty: almide_lang::types::Ty::Unit,
                current_module_name: None,
            };

            // Compare tags
            wasm!(compiler.func, {
                local_get(0); i32_load(0);
                local_get(1); i32_load(0);
                i32_ne;
                if_empty; i32_const(0); return_; end;
            });

            // Branch on tag for each case
            let non_empty: Vec<_> = cases.iter().filter(|c| !c.fields.is_empty()).collect();
            if non_empty.is_empty() {
                wasm!(compiler.func, { i32_const(1); });
            } else {
                for case in &non_empty {
                    wasm!(compiler.func, {
                        local_get(0); i32_load(0);
                        i32_const(case.tag as i32);
                        i32_eq;
                        if_i32;
                    });
                    // Compare fields (AND results together)
                    let mut offset = 4u32;
                    for (fi, (_, field_ty)) in case.fields.iter().enumerate() {
                        let field_size = values::byte_size(field_ty);
                        wasm!(compiler.func, { local_get(0); });
                        compiler.emit_load_at(field_ty, offset);
                        wasm!(compiler.func, { local_get(1); });
                        compiler.emit_load_at(field_ty, offset);
                        let ft = field_ty.clone();
                        compiler.emit_eq_typed(&ft);
                        if fi > 0 {
                            wasm!(compiler.func, { i32_and; });
                        }
                        offset += field_size;
                    }
                    wasm!(compiler.func, { else_; });
                }
                wasm!(compiler.func, { i32_const(1); }); // default: unit case → equal
                for _ in 0..non_empty.len() {
                    wasm!(compiler.func, { end; });
                }
            }

            compiler.func.instruction(&wasm_encoder::Instruction::End);
            compiler.func
        };

        emitter.add_compiled(CompiledFunc::tracked(type_idx, compiled_func));
    }
}

/// A named record/variant type is repr-backed unless it (transitively, through a
/// container) holds a closure field — a closure has no Almide-literal form, so it
/// never reaches compound interpolation. Mirrors the native `type_has_repr_impl`
/// closure gate so the two targets agree on exactly which types get a repr.
fn ty_field_has_closure(ty: &almide_lang::types::Ty) -> bool {
    matches!(ty, almide_lang::types::Ty::Fn { .. })
        || ty.children().into_iter().any(ty_field_has_closure)
}

/// Collect the names of named types referenced anywhere inside `ty` (directly or
/// nested in a container / tuple / record), into `out`.
fn collect_named_refs(ty: &almide_lang::types::Ty, out: &mut HashSet<String>) {
    if let almide_lang::types::Ty::Named(n, _) = ty {
        out.insert(n.to_string());
    }
    for child in ty.children() {
        collect_named_refs(child, out);
    }
}

/// The set of named record/variant types that lie on a reference CYCLE — i.e. a
/// type that can reach itself through its fields/cases (self-recursion like
/// `Tree = … Node(Tree, Tree)`, or mutual recursion `A → B → A`). Only these need
/// a per-type repr function: a NON-recursive type stays on the inline walk, where
/// the concrete `Ty::Named(_, type_args)` at the interpolation site resolves its
/// fields' types (so a generic non-recursive type like `Box[Int]` reprs its `T`
/// payload correctly). A recursive type's inline walk would instead expand its
/// type graph forever at compile time, so it routes through its repr fn where the
/// self-reference is a runtime CALL.
fn recursive_type_names(emitter: &WasmEmitter) -> HashSet<String> {
    // Edge set: type name → directly-referenced named types (through fields).
    let mut edges: HashMap<String, HashSet<String>> = HashMap::new();
    for (name, cases) in &emitter.variant_info {
        let mut refs = HashSet::new();
        for c in cases {
            for (_, ft) in &c.fields { collect_named_refs(ft, &mut refs); }
        }
        edges.insert(name.clone(), refs);
    }
    let case_names: HashSet<String> = emitter.variant_info.values()
        .flat_map(|cases| cases.iter().map(|c| c.name.clone()))
        .collect();
    for (name, fields) in &emitter.record_fields {
        if name.starts_with("__anon_record_") || case_names.contains(name)
            || emitter.variant_info.contains_key(name) { continue; }
        let mut refs = HashSet::new();
        for (_, ft) in fields { collect_named_refs(ft, &mut refs); }
        edges.insert(name.clone(), refs);
    }

    // A type is recursive iff it can reach itself. Reachability via DFS over the
    // edge set (only edges to nodes that are themselves typed are followed).
    let mut recursive = HashSet::new();
    for start in edges.keys() {
        let mut stack: Vec<String> = edges[start].iter().cloned().collect();
        let mut seen: HashSet<String> = HashSet::new();
        while let Some(n) = stack.pop() {
            if &n == start { recursive.insert(start.clone()); break; }
            if !seen.insert(n.clone()) { continue; }
            if let Some(next) = edges.get(&n) {
                for m in next { stack.push(m.clone()); }
            }
        }
    }
    recursive
}

/// Pre-register one `__repr_<TypeName>(ptr: i32) -> i32` per repr-backed NAMED
/// record/variant type that lies on a reference cycle. Recursion (self / mutual)
/// becomes a CALL into the callee's repr fn, so a finite runtime value terminates
/// exactly like the native trait dispatch — inline expansion of a recursive type
/// graph would loop forever at compile time. NON-recursive types are walked
/// inline (the concrete type args at the interpolation site resolve their fields).
///
/// Reserve indices in SORTED name order so they are a pure function of the
/// program (the same 32-bit/64-bit host-determinism contract as
/// `register_variant_eq_funcs`); the (also sorted) compile order must match.
fn register_repr_funcs(emitter: &mut WasmEmitter, program: &IrProgram) {
    let type_idx = emitter.register_type(vec![ValType::I32], vec![ValType::I32]);

    let recursive = recursive_type_names(emitter);

    // Repr-backed RECURSIVE named types: every recursive variant type, plus every
    // recursive named record. A record case-name is also stored in `record_fields`
    // (for field access), so restrict records to declared record types — not the
    // synthetic `__anon_record_*` shapes (walked inline) and not the variant CASE
    // names (a `Node` value reprs through its variant type's fn, by tag). A type
    // with a closure field is excluded.
    let mut base_names: HashSet<String> = HashSet::new();
    for (name, cases) in &emitter.variant_info {
        let has_closure = cases.iter().any(|c| c.fields.iter().any(|(_, ft)| ty_field_has_closure(ft)));
        if recursive.contains(name) && !has_closure {
            base_names.insert(name.clone());
        }
    }
    // Variant CASE names that are also keyed in record_fields — skip them as
    // standalone record reprs (they are reached via the owning variant type).
    let case_names: HashSet<String> = emitter.variant_info.values()
        .flat_map(|cases| cases.iter().map(|c| c.name.clone()))
        .collect();
    for (name, fields) in &emitter.record_fields {
        let is_anon = name.starts_with("__anon_record_");
        let is_variant_case = case_names.contains(name);
        let is_variant_type = emitter.variant_info.contains_key(name);
        let has_closure = fields.iter().any(|(_, ft)| ty_field_has_closure(ft));
        if recursive.contains(name) && !is_anon && !is_variant_case && !is_variant_type && !has_closure {
            base_names.insert(name.clone());
        }
    }

    // Discover the concrete INSTANTIATIONS of these recursive types used in the
    // program (`Tree[Int]`, `Tree[String]`, `Tree[List[Int]]`). Each needs its
    // own repr fn keyed by the mangled name — a monomorphic by-bare-name fn
    // reads the `T` payload as a raw `TypeVar`. The recursive references inside
    // a fn body resolve to the SAME instantiation (`Node`'s children are
    // `Tree[T]` → `Tree[Int]` for the `Tree[Int]` fn), so the site-level
    // instantiations are the full set — no new ones appear transitively.
    // A non-generic recursive type (`type IntTree = Leaf(Int) | ...`) yields the
    // bare-name key (empty args → mangle is just the name), preserving behavior.
    let mut instantiations: BTreeMap<String, Ty> = BTreeMap::new();
    // Always register the bare-name fn for every recursive type: a recursive type
    // with NO interpolation site still needs its reserved slot iff something else
    // (e.g. a nested non-generic recursive field) routes to it, and the
    // non-generic case is reached via the bare name at dispatch.
    for name in &base_names {
        let bare = Ty::Named(almide_base::intern::sym(name), Vec::new());
        instantiations.insert(name.clone(), bare);
    }
    collect_repr_instantiations(program, &base_names, &mut instantiations);

    // Reserve indices in SORTED key order (host-determinism contract); the
    // compile order in `compile_repr_funcs` must match.
    for (mangled, ty) in instantiations {
        let func_idx = emitter.register_func(&format!("__repr_{}", mangled), type_idx);
        emitter.repr_funcs.insert(mangled.clone(), func_idx);
        emitter.repr_func_tys.insert(mangled, ty);
    }
}

/// Mangle a concrete type into the suffix used to key per-instantiation repr
/// fns (`Tree[Int]` → `Tree_Int`, `Tree[List[Int]]` → `Tree_List_Int`). Mirrors
/// the Rust-walker `mangle_ty_for_mono` convention so the two targets name
/// instantiations identically. A type with no args mangles to its bare name.
pub(super) fn mangle_repr_ty(ty: &Ty) -> String {
    use almide_lang::types::constructor::TypeConstructorId;
    match ty {
        Ty::Int => "Int".into(),
        Ty::Float => "Float".into(),
        Ty::String => "String".into(),
        Ty::Bool => "Bool".into(),
        Ty::Int8 => "Int8".into(),
        Ty::Int16 => "Int16".into(),
        Ty::Int32 => "Int32".into(),
        Ty::UInt8 => "UInt8".into(),
        Ty::UInt16 => "UInt16".into(),
        Ty::UInt32 => "UInt32".into(),
        Ty::UInt64 => "UInt64".into(),
        Ty::Float32 => "Float32".into(),
        Ty::Bytes => "Bytes".into(),
        Ty::Unit => "Unit".into(),
        Ty::Named(name, args) => {
            if args.is_empty() { name.to_string() }
            else { format!("{}_{}", name, args.iter().map(mangle_repr_ty).collect::<Vec<_>>().join("_")) }
        }
        Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 =>
            format!("List_{}", mangle_repr_ty(&args[0])),
        Ty::Applied(id, args) => {
            let name = format!("{:?}", id);
            if args.is_empty() { name } else {
                format!("{}_{}", name, args.iter().map(mangle_repr_ty).collect::<Vec<_>>().join("_"))
            }
        }
        _ => "Unknown".into(),
    }
}

/// Walk every expression type in the program, recording each concrete
/// instantiation `Ty::Named(base, non-empty-args)` of a recursive repr-backed
/// type (`base` ∈ `base_names`), keyed by its mangled name. Nested instantiations
/// (`List[Tree[Int]]` → `Tree[Int]`) are found by scanning each type's subtree.
fn collect_repr_instantiations(
    program: &IrProgram,
    base_names: &HashSet<String>,
    out: &mut BTreeMap<String, Ty>,
) {
    use almide_ir::visit::{IrVisitor, walk_expr};
    struct Collector<'a> {
        base_names: &'a HashSet<String>,
        out: &'a mut BTreeMap<String, Ty>,
    }
    impl<'a> Collector<'a> {
        fn scan_ty(&mut self, ty: &Ty) {
            // A generic recursive type used concretely: record the instantiation.
            if let Ty::Named(name, args) = ty {
                if !args.is_empty()
                    && self.base_names.contains(name.as_str())
                    && !args.iter().any(ty_is_unresolved_repr)
                {
                    out_insert(self.out, ty);
                }
            }
            // Descend into every child type (List/Tuple/Map/Option/Result/Named
            // args, Record fields) so nested instantiations surface too.
            for child in ty.children() {
                self.scan_ty(child);
            }
        }
    }
    fn out_insert(out: &mut BTreeMap<String, Ty>, ty: &Ty) {
        out.insert(mangle_repr_ty(ty), ty.clone());
    }
    impl<'a> IrVisitor for Collector<'a> {
        fn visit_expr(&mut self, expr: &almide_ir::IrExpr) {
            self.scan_ty(&expr.ty);
            walk_expr(self, expr);
        }
    }
    let mut c = Collector { base_names, out };
    for func in &program.functions {
        c.visit_expr(&func.body);
    }
    for module in &program.modules {
        for func in &module.functions {
            c.visit_expr(&func.body);
        }
    }
}

/// A type still carrying an unresolved `TypeVar`/`Unknown` is not a real
/// instantiation — skip it (the corresponding bare-name fn handles the
/// degenerate case, and we must not mangle a `TypeVar` into a fn name).
fn ty_is_unresolved_repr(ty: &Ty) -> bool {
    match ty {
        Ty::TypeVar(_) | Ty::Unknown => true,
        _ => ty.children().iter().any(|c| ty_is_unresolved_repr(c)),
    }
}

/// Compile each `__repr_<TypeName>` body: load the value pointer (param 0) and
/// run the SAME structural walk the inline path uses (`emit_repr_record` /
/// `emit_repr_variant`). Nested named-type fields recurse as a CALL because
/// `emit_repr_value` routes a repr-backed `Ty::Named` through its repr fn (see
/// `calls_string_repr.rs`). Body-emit order is sorted to match the (sorted)
/// index reservation in `register_repr_funcs`.
fn compile_repr_funcs(emitter: &mut WasmEmitter, var_table: &almide_ir::VarTable) {
    let mut entries: Vec<(String, u32)> = emitter.repr_funcs.iter()
        .map(|(n, &idx)| (n.clone(), idx))
        .collect();
    entries.sort();

    for (mangled, _func_idx) in &entries {
        let type_idx = emitter.register_type(vec![ValType::I32], vec![ValType::I32]);

        // One repr fn walks a SINGLE level (children recurse via call), so its
        // scratch demand is bounded by one type's field/case count — the generous
        // caps below mirror the eq-fn setup and never approach the inline-expansion
        // overflow that motivated these functions.
        let mut local_decls = Vec::new();
        let scratch_i32_cap = 32usize;
        let scratch_i64_cap = 8usize;
        let scratch_f64_cap = 2usize;
        let scratch_i32_base = 1u32; // after the single `ptr` param
        for _ in 0..scratch_i32_cap { local_decls.push((1, ValType::I32)); }
        let scratch_i64_base = scratch_i32_base + scratch_i32_cap as u32;
        for _ in 0..scratch_i64_cap { local_decls.push((1, ValType::I64)); }
        let scratch_f64_base = scratch_i64_base + scratch_i64_cap as u32;
        for _ in 0..scratch_f64_cap { local_decls.push((1, ValType::F64)); }

        let wasm_func = TrackedFunction::new(local_decls);
        let mut scratch_alloc = scratch::ScratchAllocator::new();
        scratch_alloc.set_bases_with_capacity(
            scratch_i32_base, scratch_i32_cap,
            scratch_i64_base, scratch_i64_cap,
            scratch_f64_base, scratch_f64_cap,
        );

        // The repr emitters dispatch on the static `Ty` of the value: a variant
        // type → `emit_repr_variant`, otherwise a record → `emit_repr_record`.
        // The instantiation `Ty` (e.g. `Tree[Int]`) carries the concrete type
        // args so the variant/record walk substitutes them into each payload —
        // this is the whole point of keying by instantiation. The dispatch base
        // name is the type's own name (`Tree`), not the mangled key.
        let ty = emitter.repr_func_tys.get(mangled).cloned()
            .unwrap_or_else(|| almide_lang::types::Ty::Named(almide_base::intern::sym(mangled), Vec::new()));
        let base_name = match &ty {
            almide_lang::types::Ty::Named(n, _) => n.to_string(),
            _ => mangled.clone(),
        };
        let is_variant = emitter.variant_info.contains_key(base_name.as_str());

        let compiled_func = {
            let mut compiler = FuncCompiler {
                emitter: &mut *emitter,
                func: wasm_func,
                var_map: std::collections::HashMap::new(),
                depth: 0,
                loop_stack: Vec::new(),
                scratch: scratch_alloc,
                var_table,
                stub_ret_ty: almide_lang::types::Ty::Unit,
                current_module_name: None,
            };

            // Push the value pointer (param 0); the walk consumes it from the
            // stack and leaves the result string pointer (the return value).
            wasm!(compiler.func, { local_get(0); });
            if is_variant {
                compiler.emit_repr_variant(&ty);
            } else {
                let fields = compiler.extract_record_fields(&ty);
                compiler.emit_repr_record(Some(base_name.as_str()), &fields);
            }
            compiler.func.instruction(&wasm_encoder::Instruction::End);
            compiler.func
        };

        emitter.add_compiled(CompiledFunc::tracked(type_idx, compiled_func));
    }
}

use std::collections::HashSet;

/// Content-derived, host-deterministic name for an anonymous record shape.
///
/// The name is a pure function of the field shape — the same
/// `(field_name, Debug(ty))` key used for dedup below — so two structurally
/// identical records get one name and the name is invariant to the IR walk
/// order in which a record happens to be discovered. The previous
/// `__anon_record_{record_fields.len()}` counter coupled the name to walk
/// position, which is deterministic only as long as the entire upstream walk
/// order is; the Determinism Belt prefers names that are a function of content,
/// not provenance.
///
/// FNV-1a/64 is used deliberately instead of `std`'s `DefaultHasher`, whose
/// `RandomState` seed varies per process (it would reintroduce exactly the
/// non-determinism this is meant to remove). See
/// docs/roadmap/active/determinism-belt.md.
fn anon_record_name(key: &[(String, String)]) -> String {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut h = FNV_OFFSET;
    let mut mix = |bytes: &[u8]| {
        for &b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(FNV_PRIME);
        }
        // Field separator so `["ab","c"]` and `["a","bc"]` can't alias.
        h ^= 0xff;
        h = h.wrapping_mul(FNV_PRIME);
    };
    for (n, t) in key {
        mix(n.as_bytes());
        mix(t.as_bytes());
    }
    format!("__anon_record_{h:016x}")
}

/// Walk all IR expressions/statements and collect anonymous record shapes
/// (i.e. `Ty::Record { fields }`). Each unique field-set is registered in
/// `emitter.record_fields` under a content-derived name (see
/// [`anon_record_name`]) so the emit-phase Member access fallback (which
/// iterates record_fields looking for a match by field name) can find them
/// when a lambda param's own type was left as Unknown/TypeVar by inference.
fn register_anonymous_records(program: &IrProgram, emitter: &mut WasmEmitter) {
    use almide_ir::{IrExpr, IrExprKind, IrStmt, IrStmtKind};
    let mut seen: HashSet<Vec<(String, String)>> = HashSet::new();
    // Seed with already-registered records to avoid redundant anonymous entries.
    for fields in emitter.record_fields.values() {
        let key: Vec<(String, String)> = fields.iter().map(|(n, t)| (n.clone(), format!("{:?}", t))).collect();
        seen.insert(key);
    }

    fn walk_ty(
        ty: &Ty,
        seen: &mut HashSet<Vec<(String, String)>>,
        record_fields: &mut BTreeMap<String, Vec<(String, Ty)>>,
    ) {
        match ty {
            Ty::Record { fields } | Ty::OpenRecord { fields } => {
                let field_vec: Vec<(String, Ty)> = fields.iter()
                    .map(|(n, t)| (n.to_string(), t.clone()))
                    .collect();
                let key: Vec<(String, String)> = field_vec.iter()
                    .map(|(n, t)| (n.clone(), format!("{:?}", t)))
                    .collect();
                let name = anon_record_name(&key);
                if seen.insert(key) {
                    record_fields.insert(name, field_vec.clone());
                }
                for (_, fty) in fields.iter() { walk_ty(fty, seen, record_fields); }
            }
            Ty::Applied(_, args) => { for a in args { walk_ty(a, seen, record_fields); } }
            Ty::Tuple(elems) => { for e in elems { walk_ty(e, seen, record_fields); } }
            Ty::Fn { params, ret } => {
                for p in params { walk_ty(p, seen, record_fields); }
                walk_ty(ret, seen, record_fields);
            }
            _ => {}
        }
    }

    fn walk_expr(
        expr: &IrExpr,
        seen: &mut HashSet<Vec<(String, String)>>,
        record_fields: &mut BTreeMap<String, Vec<(String, Ty)>>,
    ) {
        walk_ty(&expr.ty, seen, record_fields);
        match &expr.kind {
            IrExprKind::Block { stmts, expr: tail } => {
                for s in stmts { walk_stmt(s, seen, record_fields); }
                if let Some(t) = tail { walk_expr(t, seen, record_fields); }
            }
            IrExprKind::Call { args, .. } => { for a in args { walk_expr(a, seen, record_fields); } }
            IrExprKind::If { cond, then, else_ } => {
                walk_expr(cond, seen, record_fields);
                walk_expr(then, seen, record_fields);
                walk_expr(else_, seen, record_fields);
            }
            IrExprKind::Match { subject, arms } => {
                walk_expr(subject, seen, record_fields);
                for arm in arms {
                    if let Some(g) = &arm.guard { walk_expr(g, seen, record_fields); }
                    walk_expr(&arm.body, seen, record_fields);
                }
            }
            IrExprKind::Record { fields, .. } => {
                // Build field-type list from the literal's field expressions.
                let field_vec: Vec<(String, Ty)> = fields.iter()
                    .map(|(n, e)| (n.to_string(), e.ty.clone()))
                    .collect();
                let key: Vec<(String, String)> = field_vec.iter()
                    .map(|(n, t)| (n.clone(), format!("{:?}", t)))
                    .collect();
                let name = anon_record_name(&key);
                if field_vec.iter().all(|(_, t)| !t.is_unresolved()) && seen.insert(key) {
                    record_fields.insert(name, field_vec);
                }
                for (_, e) in fields.iter() { walk_expr(e, seen, record_fields); }
            }
            IrExprKind::SpreadRecord { base, fields } => {
                walk_expr(base, seen, record_fields);
                for (_, e) in fields.iter() { walk_expr(e, seen, record_fields); }
            }
            IrExprKind::List { elements } => { for e in elements { walk_expr(e, seen, record_fields); } }
            IrExprKind::Tuple { elements } => { for e in elements { walk_expr(e, seen, record_fields); } }
            IrExprKind::Lambda { body, .. } => { walk_expr(body, seen, record_fields); }
            IrExprKind::ClosureCreate { captures, .. } => {
                for (_, t) in captures { walk_ty(t, seen, record_fields); }
            }
            IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
            | IrExprKind::OptionSome { expr } => walk_expr(expr, seen, record_fields),
            IrExprKind::Member { object, .. } => { walk_expr(object, seen, record_fields); }
            IrExprKind::IndexAccess { object, index } => {
                walk_expr(object, seen, record_fields);
                walk_expr(index, seen, record_fields);
            }
            IrExprKind::BinOp { left, right, .. } => {
                walk_expr(left, seen, record_fields);
                walk_expr(right, seen, record_fields);
            }
            IrExprKind::UnOp { operand, .. } => walk_expr(operand, seen, record_fields),
            IrExprKind::Try { expr } | IrExprKind::Unwrap { expr } => walk_expr(expr, seen, record_fields),
            IrExprKind::ForIn { iterable, body, .. } => {
                walk_expr(iterable, seen, record_fields);
                for s in body { walk_stmt(s, seen, record_fields); }
            }
            IrExprKind::While { cond, body } => {
                walk_expr(cond, seen, record_fields);
                for s in body { walk_stmt(s, seen, record_fields); }
            }
            _ => {}
        }
    }

    fn walk_stmt(
        stmt: &IrStmt,
        seen: &mut HashSet<Vec<(String, String)>>,
        record_fields: &mut BTreeMap<String, Vec<(String, Ty)>>,
    ) {
        match &stmt.kind {
            IrStmtKind::Bind { value, ty, .. } => {
                walk_ty(ty, seen, record_fields);
                walk_expr(value, seen, record_fields);
            }
            IrStmtKind::BindDestructure { value, .. } => walk_expr(value, seen, record_fields),
            IrStmtKind::Assign { value, .. } => walk_expr(value, seen, record_fields),
            IrStmtKind::Expr { expr } => walk_expr(expr, seen, record_fields),
            _ => {}
        }
    }

    for func in &program.functions {
        walk_ty(&func.ret_ty, &mut seen, &mut emitter.record_fields);
        for p in &func.params { walk_ty(&p.ty, &mut seen, &mut emitter.record_fields); }
        walk_expr(&func.body, &mut seen, &mut emitter.record_fields);
    }
    for tl in &program.top_lets {
        walk_expr(&tl.value, &mut seen, &mut emitter.record_fields);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_program_produces_valid_wasm() {
        let program = IrProgram {
            functions: vec![],
            top_lets: vec![],
            type_decls: vec![],
            var_table: almide_ir::VarTable::new(),
            def_table: Default::default(),
            modules: vec![],
            type_registry: Default::default(),
            effect_fn_names: Default::default(),
            effect_map: Default::default(),
            codegen_annotations: Default::default(),
            used_stdlib_modules: Default::default(),
        };
        let bytes = emit(&program);
        assert_eq!(&bytes[0..4], b"\0asm");
        assert_eq!(&bytes[4..8], &[1, 0, 0, 0]);
    }
}

/// Scan IR program for filesystem module calls (fs.read_text, fs.write_text, etc.).
/// True iff the program references `string.to_upper` / `to_lower` / `capitalize`
/// in any form (resolved module call, unresolved method call, or runtime call).
/// Conservative by design: matching every dispatch form the emitter handles means
/// the pre-scan never misses a reachable case op (a miss would leave the always-
/// compiled case lookup functions baking stale offsets — silently wrong).
fn program_uses_case_op(program: &IrProgram) -> bool {
    use almide_ir::{IrExprKind, CallTarget};
    use almide_ir::visit::{IrVisitor, walk_expr, walk_stmt};

    fn is_case_fn(name: &str) -> bool {
        // Accept both the bare method name and a "string."-qualified one: the
        // Module arm sees a bare `func`, but the unresolved Method arm (and the
        // calls.rs UFCS fallback) can carry a dotted "string.to_upper". Missing a
        // form would leave the case tables un-embedded while the runtime fns stay
        // DCE-live — silently wrong, so keep this strictly broader than dispatch.
        let name = name.strip_prefix("string.").unwrap_or(name);
        matches!(name, "to_upper" | "to_lower" | "capitalize")
    }

    struct CaseScanner { found: bool }
    impl IrVisitor for CaseScanner {
        fn visit_expr(&mut self, expr: &almide_ir::IrExpr) {
            if self.found { return; }
            match &expr.kind {
                IrExprKind::Call { target: CallTarget::Module { module, func, .. }, .. }
                    if module.as_str() == "string" && is_case_fn(func.as_str()) =>
                {
                    self.found = true;
                    return;
                }
                IrExprKind::Call { target: CallTarget::Method { method, .. }, .. }
                    if is_case_fn(method.as_str()) =>
                {
                    self.found = true;
                    return;
                }
                IrExprKind::RuntimeCall { symbol, .. }
                    if matches!(
                        symbol.as_str(),
                        "almide_rt_string_to_upper"
                            | "almide_rt_string_to_lower"
                            | "almide_rt_string_capitalize"
                    ) =>
                {
                    self.found = true;
                    return;
                }
                _ => {}
            }
            walk_expr(self, expr);
        }
        fn visit_stmt(&mut self, stmt: &almide_ir::IrStmt) {
            if self.found { return; }
            walk_stmt(self, stmt);
        }
    }

    let mut scanner = CaseScanner { found: false };
    for func in &program.functions {
        scanner.visit_expr(&func.body);
        if scanner.found { return true; }
    }
    false
}

/// True iff the program references `math.sin` / `math.cos` / `math.tan` in any
/// dispatch form (resolved module call, unresolved method call, or runtime call).
/// Conservative — same contract as `program_uses_case_op`: a miss would leave the
/// always-compiled trig runtime baking stale table offsets (silently wrong), so
/// this is strictly broader than dispatch and also walks module bodies, not just
/// top-level functions.
fn program_uses_trig(program: &IrProgram) -> bool {
    use almide_ir::{IrExprKind, CallTarget};
    use almide_ir::visit::{IrVisitor, walk_expr, walk_stmt};

    fn is_trig_fn(name: &str) -> bool {
        let name = name.strip_prefix("math.").unwrap_or(name);
        matches!(name, "sin" | "cos" | "tan")
    }

    struct TrigScanner { found: bool }
    impl IrVisitor for TrigScanner {
        fn visit_expr(&mut self, expr: &almide_ir::IrExpr) {
            if self.found { return; }
            match &expr.kind {
                IrExprKind::Call { target: CallTarget::Module { module, func, .. }, .. }
                    if module.as_str() == "math" && is_trig_fn(func.as_str()) =>
                {
                    self.found = true;
                    return;
                }
                IrExprKind::Call { target: CallTarget::Method { method, .. }, .. }
                    if is_trig_fn(method.as_str()) =>
                {
                    self.found = true;
                    return;
                }
                IrExprKind::RuntimeCall { symbol, .. }
                    if matches!(
                        symbol.as_str(),
                        "almide_rt_math_sin" | "almide_rt_math_cos" | "almide_rt_math_tan"
                    ) =>
                {
                    self.found = true;
                    return;
                }
                _ => {}
            }
            walk_expr(self, expr);
        }
        fn visit_stmt(&mut self, stmt: &almide_ir::IrStmt) {
            if self.found { return; }
            walk_stmt(self, stmt);
        }
    }

    let mut scanner = TrigScanner { found: false };
    for func in &program.functions {
        scanner.visit_expr(&func.body);
        if scanner.found { return true; }
    }
    for module in &program.modules {
        for func in &module.functions {
            scanner.visit_expr(&func.body);
            if scanner.found { return true; }
        }
    }
    false
}

fn program_uses_fs(program: &IrProgram) -> bool {
    use almide_ir::{IrExprKind, IrStmtKind, CallTarget};
    use almide_ir::visit::{IrVisitor, walk_expr, walk_stmt};

    struct FsScanner { found: bool }
    impl IrVisitor for FsScanner {
        fn visit_expr(&mut self, expr: &almide_ir::IrExpr) {
            if self.found { return; }
            if let IrExprKind::Call { target: CallTarget::Module { module, .. }, .. } = &expr.kind {
                if module == "fs" { self.found = true; return; }
            }
            if let IrExprKind::RuntimeCall { symbol, .. } = &expr.kind {
                if symbol.starts_with("almide_rt_fs_") { self.found = true; return; }
            }
            walk_expr(self, expr);
        }
        fn visit_stmt(&mut self, stmt: &almide_ir::IrStmt) {
            if self.found { return; }
            walk_stmt(self, stmt);
        }
    }

    let mut scanner = FsScanner { found: false };
    for func in &program.functions {
        scanner.visit_expr(&func.body);
        if scanner.found { return true; }
    }
    false
}
