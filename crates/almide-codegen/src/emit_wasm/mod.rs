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
mod strings;
mod runtime;
mod runtime_eq;
mod rt_string;
mod rt_string_extra;
mod rt_numeric;
mod expressions;
mod calls;
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
mod closures;
mod equality;
mod collections;
mod control;
pub mod statements;
mod functions;
pub mod scratch;
mod dce;

use std::collections::HashMap;
use wasm_encoder::{
    CodeSection, DataSection, ElementSection, Elements, ExportSection,
    Function, FunctionSection, GlobalSection, GlobalType, ImportSection,
    MemorySection, MemoryType, Module, RefType, TableSection, TableType,
    TypeSection, ValType,
};

use almide_ir::IrProgram;
use almide_lang::types::Ty;

// Memory layout constants
const SCRATCH_ITOA: u32 = 16;
const NEWLINE_OFFSET: u32 = 48;

/// A compiled WASM function ready for the code section.
pub struct CompiledFunc {
    pub type_idx: u32,
    pub func: Function,
}

/// String stdlib runtime function indices.
pub struct StringRuntime {
    pub eq: u32,
    pub contains: u32,
    pub trim: u32,
    pub slice: u32,
    pub char_count: u32,
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
    pub is_upper: u32,
    pub is_lower: u32,
    pub cmp: u32,
}

/// Indices of built-in runtime functions.
pub struct RuntimeFuncs {
    pub fd_write: u32,
    pub alloc: u32,
    pub println_str: u32,
    pub int_to_string: u32,
    pub println_int: u32,
    pub concat_str: u32,
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
    pub string: StringRuntime,
    pub value_stringify: u32,
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
}

/// Import descriptor for WASM import section.
struct ImportInfo {
    module: String,
    name: String,
    type_idx: u32,
}

/// Central state for WASM binary emission.
pub struct WasmEmitter {
    // Type section (deduplicated function signatures)
    pub(crate) types: Vec<(Vec<ValType>, Vec<ValType>)>,
    type_map: HashMap<(Vec<ValType>, Vec<ValType>), u32>,

    // Imports
    imports: Vec<ImportInfo>,
    num_imports: u32,

    // Function index tracking
    next_func_idx: u32,
    pub func_map: HashMap<String, u32>,
    // func_idx → type_idx for defined (non-import) functions
    pub func_type_indices: HashMap<u32, u32>,

    // Compiled function bodies (in definition order)
    compiled: Vec<CompiledFunc>,

    // String pool
    strings: HashMap<String, u32>,
    data_bytes: Vec<u8>,

    // Runtime function indices
    pub rt: RuntimeFuncs,

    // Globals
    pub heap_ptr_global: u32,
    // Top-level let globals: VarId → (global index, ValType)
    pub top_let_globals: HashMap<u32, (u32, ValType)>,
    pub top_let_init: Vec<(u32, ValType, i64)>, // (global_idx, type, const_init_bits) in order
    pub next_global: u32,

    // Function table: func_idx → table_idx (for call_indirect / FnRef)
    pub func_table: Vec<u32>, // list of func_idx in table order
    pub func_to_table_idx: HashMap<u32, u32>, // func_idx → table index

    // User-defined public functions to export (name list, populated during emit)
    pub user_exports: Vec<String>,

    // Type info: record/variant name → field list (for field offset computation)
    pub record_fields: HashMap<String, Vec<(String, almide_lang::types::Ty)>>,
    // Variant info: variant type name → list of (case_name, tag, fields)
    pub variant_info: HashMap<String, Vec<VariantCase>>,
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
    // Deep-equality functions per variant type: type_name → func_idx
    pub eq_funcs: HashMap<String, u32>,
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
            types: Vec::new(),
            type_map: HashMap::new(),
            imports: Vec::new(),
            num_imports: 0,
            next_func_idx: 0,
            func_map: HashMap::new(),
            func_type_indices: HashMap::new(),
            compiled: Vec::new(),
            strings: HashMap::new(),
            // First byte is newline at NEWLINE_OFFSET
            data_bytes: vec![0x0A],
            rt: RuntimeFuncs {
                fd_write: 0, alloc: 0,
                println_str: 0, println_int: 0,
                int_to_string: 0, float_to_string: 0,
                float_parse: 0, float_to_fixed: 0, float_pow: 0,
                math_sin: 0, math_cos: 0, math_tan: 0,
                math_log: 0, math_log10: 0, math_log2: 0, math_exp: 0,
                concat_str: 0, concat_list: 0,
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
                    is_whitespace: 0, is_upper: 0, is_lower: 0,
                    cmp: 0,
                    char_count: 0,
                },
                value_stringify: 0,
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
            },
            heap_ptr_global: 0,
            top_let_globals: HashMap::new(),
            top_let_init: Vec::new(),
            next_global: 1, // 0 = heap_ptr
            func_table: Vec::new(),
            func_to_table_idx: HashMap::new(),
            record_fields: HashMap::new(),
            variant_info: HashMap::new(),
            default_fields: HashMap::new(),
            lambdas: Vec::new(),
            fn_ref_wrappers: HashMap::new(),
            lambda_counter: std::cell::Cell::new(0),
            effect_fns: HashSet::new(),
            mutable_captures: HashSet::new(),
            eq_funcs: HashMap::new(),
            user_exports: Vec::new(),
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
    pub func: Function,
    pub var_map: HashMap<u32, u32>,
    pub depth: u32,
    pub loop_stack: Vec<LoopLabels>,
    // Scratch local allocator
    pub scratch: scratch::ScratchAllocator,
    // Variable table for name lookups (pattern matching)
    pub var_table: &'a almide_ir::VarTable,
    // Return type for stub calls (set by emit_call before delegating to handlers)
    pub stub_ret_ty: Ty,
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
}

// ── Public API ──────────────────────────────────────────────────────

/// Emit a WASM binary from an IR program (WASI mode).
pub fn emit(program: &IrProgram) -> Vec<u8> {
    let mut emitter = WasmEmitter::new();

    // Phase 1: Register types and function indices
    runtime::register_runtime(&mut emitter);

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

    // Register type declarations (record and variant field layouts).
    // Include both the main program and all imported modules so nominal
    // types from `import mod` resolve during codegen.
    let all_type_decls = program.type_decls.iter()
        .chain(program.modules.iter().flat_map(|m| m.type_decls.iter()));
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
            _ => {}
        }
    }

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
        emitter.top_let_init.push((global_idx, vt, const_bits));
    }

    // Register ALL function signatures (including test functions)
    let mut user_meta: Vec<u32> = Vec::new();
    let mut user_func_indices: Vec<u32> = Vec::new();
    let mut test_func_indices: Vec<(u32, String)> = Vec::new(); // (func_idx, test_name)
    let has_main = program.functions.iter().any(|f| f.name == "main" && !f.is_test);

    for func in &program.functions {
        // Resolve param and ret types: Unknown/TypeVar can leak through from
        // lifted lambdas whose outer `Ty::Fn` had unresolved entries. Fall back
        // to VarTable (for params) and expression inspection (for ret).
        let params: Vec<ValType> = func.params.iter()
            .filter_map(|p| {
                let pty = if matches!(&p.ty, almide_lang::types::Ty::Unknown | almide_lang::types::Ty::TypeVar(_) | almide_lang::types::Ty::OpenRecord { .. }) {
                    let vt_ty = &program.var_table.get(p.var).ty;
                    if !matches!(vt_ty, almide_lang::types::Ty::Unknown | almide_lang::types::Ty::TypeVar(_) | almide_lang::types::Ty::OpenRecord { .. }) {
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
        let resolved_ret_ty = if matches!(&func.ret_ty, almide_lang::types::Ty::Unknown | almide_lang::types::Ty::TypeVar(_)) {
            closures::resolve_expr_ty(&func.body, &program.var_table, &emitter.record_fields)
        } else {
            func.ret_ty.clone()
        };
        let results = values::ret_type(&resolved_ret_ty);
        let type_idx = emitter.register_type(params, results);
        // Use prefixed name for test functions to avoid colliding with user functions
        let reg_name = if func.is_test {
            format!("__test_{}", func.name)
        } else {
            func.name.to_string()
        };
        let func_idx = emitter.register_func(&reg_name, type_idx);
        user_meta.push(type_idx);
        user_func_indices.push(func_idx);
        if func.is_test {
            test_func_indices.push((func_idx, func.name.to_string()));
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
            let func_name_sanitized = func.name.to_string().replace(' ', "_").replace('-', "_").replace('.', "_");
            let prefixed_name = format!("almide_rt_{}_{}", mod_ident, func_name_sanitized);
            let params: Vec<ValType> = func.params.iter()
                .filter_map(|p| values::ty_to_valtype(&p.ty))
                .collect();
            let results = values::ret_type(&func.ret_ty);
            let type_idx = emitter.register_type(params, results);
            let func_idx = emitter.register_func(&prefixed_name, type_idx);
            module_func_meta.push((mi, fi, type_idx));
            user_func_indices.push(func_idx);
            if func.is_effect {
                emitter.effect_fns.insert(prefixed_name);
            }
        }
    }

    // Check if any top-level let needs dynamic initialization (non-constant values).
    // LitStr needs init because string pointers are resolved at runtime via data section.
    let needs_init = program.top_lets.iter().any(|tl| !matches!(&tl.value.kind,
        almide_ir::IrExprKind::LitInt { .. } | almide_ir::IrExprKind::LitFloat { .. } |
        almide_ir::IrExprKind::LitBool { .. }
    ));
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

    // Build function table (for call_indirect / FnRef)
    for &func_idx in &user_func_indices {
        let table_idx = emitter.func_table.len() as u32;
        emitter.func_table.push(func_idx);
        emitter.func_to_table_idx.insert(func_idx, table_idx);
    }

    // Pre-scan for lambdas and FnRefs, register them
    closures::pre_scan_closures(program, &mut emitter);

    // Pre-register variant deep-equality functions (must be before compilation starts)
    register_variant_eq_funcs(&mut emitter);

    // Phase 2: Compile function bodies (order must match registration order)
    runtime::compile_runtime(&mut emitter);

    // User + test functions
    let mut user_idx = 0;
    for func in &program.functions {
        let type_idx = user_meta[user_idx];
        // Pass init_globals_idx to main function so top-level lets get initialized
        let is_main = func.name == "main" && !func.is_test;
        let init_idx = if is_main { init_globals_idx } else { None };
        let compiled = functions::compile_function_with_init(&mut emitter, func, &program.var_table, type_idx, init_idx);
        emitter.add_compiled(compiled);
        user_idx += 1;
    }

    // Module functions (user packages)
    for &(mi, fi, type_idx) in &module_func_meta {
        let module = &program.modules[mi];
        let func = &module.functions[fi];
        let compiled = functions::compile_function(&mut emitter, func, &module.var_table, type_idx);
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

    // Lambda bodies and FnRef wrappers
    closures::compile_lambda_bodies(program, &mut emitter);

    // Compile variant deep-equality functions (bodies, after all user code)
    compile_variant_eq_funcs(&mut emitter, &program.var_table);

    // Phase 2.5: Dead Code Elimination
    let dce_count = dce::eliminate_dead_code(&mut emitter);

    // Collect public user functions for WASM export
    for func in &program.functions {
        if func.is_test { continue; }
        if !matches!(func.visibility, almide_ir::IrVisibility::Public) { continue; }
        if func.generics.as_ref().map_or(false, |g| !g.is_empty()) { continue; }
        if func.name.as_str() == "main" { continue; }
        emitter.user_exports.push(func.name.to_string());
    }

    // Phase 3: Assemble (DCE already ran in Phase 2.5: {} functions eliminated)
    let _ = dce_count;
    assemble(&emitter)
}

/// Assemble all sections into a final WASM binary.
fn assemble(emitter: &WasmEmitter) -> Vec<u8> {
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
    let mut memory = MemorySection::new();
    memory.memory(MemoryType {
        minimum: 64, // 4MB
        maximum: None,
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    module.section(&memory);

    // ── Global section ──
    let mut globals = GlobalSection::new();
    let heap_start = NEWLINE_OFFSET + emitter.data_bytes.len() as u32;
    let heap_start_aligned = (heap_start + 7) & !7;
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &wasm_encoder::ConstExpr::i32_const(heap_start_aligned as i32),
    );
    // Top-level let globals
    for &(_, vt, bits) in &emitter.top_let_init {
        let init = match vt {
            ValType::I64 => wasm_encoder::ConstExpr::i64_const(bits),
            ValType::F64 => wasm_encoder::ConstExpr::f64_const(f64::from_bits(bits as u64)),
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
    if let Some(&main_idx) = emitter.func_map.get("main") {
        exports.export("_start", wasm_encoder::ExportKind::Func, main_idx);
    } else if let Some(&runner_idx) = emitter.func_map.get("__test_runner") {
        exports.export("_start", wasm_encoder::ExportKind::Func, runner_idx);
    }
    // Export __alloc for FFI callers to allocate WASM linear memory
    if let Some(&alloc_idx) = emitter.func_map.get("__alloc") {
        exports.export("__alloc", wasm_encoder::ExportKind::Func, alloc_idx);
    }
    // Export public user functions (collected during emit)
    for name in &emitter.user_exports {
        if let Some(&idx) = emitter.func_map.get(name.as_str()) {
            exports.export(name, wasm_encoder::ExportKind::Func, idx);
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
        codes.function(&cf.func);
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

    module.finish()
}

// ── Test runner ─────────────────────────────────────────────────

/// Compile the __init_globals function.
#[allow(dead_code)] // Will be activated when top-let WASM codegen is wired up
fn compile_init_globals(emitter: &mut WasmEmitter, program: &IrProgram) {
    let void_type = emitter.register_type(vec![], vec![]);

    let mut local_decls = Vec::new();
    // ScratchAllocator locals
    let scratch_i32_cap = 32usize;
    let scratch_i64_cap = 16usize;
    let scratch_f64_cap = 4usize;
    let scratch_i32_base = local_decls.len() as u32;
    for _ in 0..scratch_i32_cap { local_decls.push((1, ValType::I32)); }
    let scratch_i64_base = local_decls.len() as u32;
    for _ in 0..scratch_i64_cap { local_decls.push((1, ValType::I64)); }
    let scratch_f64_base = local_decls.len() as u32;
    for _ in 0..scratch_f64_cap { local_decls.push((1, ValType::F64)); }
    let scratch_v128_cap = 4usize;
    let scratch_v128_base = local_decls.len() as u32;
    for _ in 0..scratch_v128_cap { local_decls.push((1, ValType::V128)); }

    let wasm_func = Function::new(local_decls);
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
        };

        for tl in &program.top_lets {
            compiler.emit_expr(&tl.value);
            if let Some(&(global_idx, _)) = compiler.emitter.top_let_globals.get(&tl.var.0) {
                compiler.func.instruction(&wasm_encoder::Instruction::GlobalSet(global_idx));
            }
        }
        compiler.func.instruction(&wasm_encoder::Instruction::End);
        compiler.func
    };

    emitter.add_compiled(CompiledFunc { type_idx: void_type, func: compiled_func });
}

/// Compile a test runner function that calls each test, printing results.
fn compile_test_runner(emitter: &mut WasmEmitter, tests: &[(u32, String)], init_globals: Option<u32>) {
    let void_type = emitter.register_type(vec![], vec![]);
    let mut f = Function::new([]);

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
    emitter.add_compiled(CompiledFunc { type_idx: void_type, func: f });
}

/// Pre-register variant deep-equality functions for all variant types with pointer fields.
/// Must be called before Phase 2 (compilation) so func_idx is known at emit time.
fn register_variant_eq_funcs(emitter: &mut WasmEmitter) {
    let type_idx = emitter.register_type(
        vec![ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    // Collect variant names that need deep eq (have pointer fields)
    let names: Vec<String> = emitter.variant_info.iter()
        .filter(|(_, cases)| {
            cases.iter().any(|c| c.fields.iter().any(|(_, ft)| {
                !matches!(ft, almide_lang::types::Ty::Int | almide_lang::types::Ty::Float | almide_lang::types::Ty::Bool | almide_lang::types::Ty::Unit)
            }))
        })
        .map(|(name, _)| name.clone())
        .collect();
    for name in names {
        let func_idx = emitter.register_func(&format!("__eq_{}", name), type_idx);
        emitter.eq_funcs.insert(name, func_idx);
    }
}

/// Compile variant deep-equality function bodies.
/// Each function: (a: i32, b: i32) -> i32 — compares tag then dispatches to per-case field comparison.
fn compile_variant_eq_funcs(emitter: &mut WasmEmitter, var_table: &almide_ir::VarTable) {
    // Collect eq_funcs entries (name → func_idx) and corresponding cases
    let eq_entries: Vec<(String, u32)> = emitter.eq_funcs.iter()
        .map(|(n, &idx)| (n.clone(), idx))
        .collect();

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

        let wasm_func = wasm_encoder::Function::new(local_decls);
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

        emitter.add_compiled(CompiledFunc { type_idx, func: compiled_func });
    }
}

use std::collections::HashSet;

/// Walk all IR expressions/statements and collect anonymous record shapes
/// (i.e. `Ty::Record { fields }`). Each unique field-set is registered in
/// `emitter.record_fields` under a synthetic name `__anon_record_N` so the
/// emit-phase Member access fallback (which iterates record_fields looking
/// for a match by field name) can find them when a lambda param's own type
/// was left as Unknown/TypeVar by type inference.
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
        record_fields: &mut HashMap<String, Vec<(String, Ty)>>,
    ) {
        match ty {
            Ty::Record { fields } | Ty::OpenRecord { fields } => {
                let field_vec: Vec<(String, Ty)> = fields.iter()
                    .map(|(n, t)| (n.to_string(), t.clone()))
                    .collect();
                let key: Vec<(String, String)> = field_vec.iter()
                    .map(|(n, t)| (n.clone(), format!("{:?}", t)))
                    .collect();
                if seen.insert(key) {
                    let name = format!("__anon_record_{}", record_fields.len());
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
        record_fields: &mut HashMap<String, Vec<(String, Ty)>>,
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
                let is_unresolved = |t: &Ty| matches!(t, Ty::Unknown | Ty::TypeVar(_));
                if field_vec.iter().all(|(_, t)| !is_unresolved(t)) && seen.insert(key) {
                    let name = format!("__anon_record_{}", record_fields.len());
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
        record_fields: &mut HashMap<String, Vec<(String, Ty)>>,
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
            modules: vec![],
            type_registry: Default::default(),
            effect_fn_names: Default::default(),
            effect_map: Default::default(),
            codegen_annotations: Default::default(),
        };
        let bytes = emit(&program);
        assert_eq!(&bytes[0..4], b"\0asm");
        assert_eq!(&bytes[4..8], &[1, 0, 0, 0]);
    }
}
