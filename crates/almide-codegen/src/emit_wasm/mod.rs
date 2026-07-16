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
pub(crate) mod reachability;

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
    /// The REGISTERED function index this body belongs to (#526). When set,
    /// `add_compiled` asserts the body lands at exactly that position —
    /// converting the "compile order mirrors registration order" CONVENTION
    /// (held across ~157 runtime routines, where a same-signature swap binds
    /// the wrong body to a name and validates cleanly) into an invariant.
    pub expected_func_idx: Option<u32>,
}

impl CompiledFunc {
    /// Construct from a TrackedFunction. This is the ONLY constructor —
    /// enforces that call_targets is always populated.
    pub fn tracked(type_idx: u32, tf: TrackedFunction) -> Self {
        Self { type_idx, func: tf.inner, call_targets: tf.call_targets, patched_body: None, expected_func_idx: None }
    }

    /// `tracked` + the registered function index this body is FOR. Prefer
    /// this in every compile_* that knows its `emitter.rt.*` slot.
    pub fn tracked_for(func_idx: u32, type_idx: u32, tf: TrackedFunction) -> Self {
        let mut c = Self::tracked(type_idx, tf);
        c.expected_func_idx = Some(func_idx);
        c
    }

    /// A minimal trapping body for `type_idx`: `unreachable; end`. Valid for ANY
    /// signature (`unreachable` is stack-polymorphic). Used by the #644
    /// reachability prune to occupy the slot of an unreachable function whose
    /// real body would panic the emitter (native-only intrinsic), and matched by
    /// the post-compile DCE stub shape. `call_targets` is empty so DCE sees no
    /// outgoing edges from it.
    pub fn trap_stub(type_idx: u32) -> Self {
        let mut tf = TrackedFunction::new([]);
        tf.instruction(&wasm_encoder::Instruction::Unreachable);
        tf.instruction(&wasm_encoder::Instruction::End);
        Self::tracked(type_idx, tf)
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
    /// `__alloc_pinned(size)` — alloc whose block is stamped `PINNED_RC` so
    /// rc ops can never free it: for HOST-WRITTEN scratch (WASI fs buffers)
    /// whose data area a syscall may overwrite — a freed-then-reused such
    /// block had its free-list `next` clobbered by the host (the C-042
    /// poison class). Pinning makes fs scratch immortal BY CONSTRUCTION.
    pub alloc_pinned: u32,
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
    pub math_atan: u32,
    pub math_tanh: u32,
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
    pub value_eq: u32,
    pub json_stringify_pretty: u32,
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
    /// args_sizes_get(argc_ptr, argv_buf_size_ptr) -> errno — WASI argv discovery (process.args).
    pub args_sizes_get: u32,
    /// args_get(argv_ptr, argv_buf_ptr) -> errno — fills the pointer array + NUL-terminated arg strings.
    pub args_get: u32,
    /// environ_sizes_get(count_ptr, buf_size_ptr) -> errno — WASI environ discovery (env.get).
    pub environ_sizes_get: u32,
    /// environ_get(environ_ptr, environ_buf_ptr) -> errno — fills the pointer array +
    /// NUL-terminated `KEY=VALUE` strings.
    pub environ_get: u32,
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
    // Declared RECORD types keyed by their SORTED field-name set → type name.
    // Mirrors the native walker's `named_records` (declarations.rs): a record
    // LITERAL inferred without annotation keeps its structural `Ty::Record`, but
    // its repr must adopt the declared nominal type's name + declaration field
    // order to stay byte-identical with native (#627). Excludes variant cases.
    pub named_records: BTreeMap<Vec<String>, String>,
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

include!("mod_p2.rs");
include!("mod_p3.rs");
include!("mod_p4.rs");
include!("mod_p5.rs");
