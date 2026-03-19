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

pub mod values;
mod strings;
mod runtime;
mod expressions;
pub mod statements;
mod functions;

use std::collections::HashMap;
use wasm_encoder::{
    CodeSection, DataSection, ElementSection, Elements, ExportSection,
    Function, FunctionSection, GlobalSection, GlobalType, ImportSection,
    MemorySection, MemoryType, Module, RefType, TableSection, TableType,
    TypeSection, ValType,
};

use crate::ir::IrProgram;

// Memory layout constants
const SCRATCH_ITOA: u32 = 16;
const NEWLINE_OFFSET: u32 = 48;

/// A compiled WASM function ready for the code section.
pub struct CompiledFunc {
    pub type_idx: u32,
    pub func: Function,
}

/// Indices of built-in runtime functions.
pub struct RuntimeFuncs {
    pub fd_write: u32,
    pub alloc: u32,
    pub println_str: u32,
    pub int_to_string: u32,
    pub println_int: u32,
    pub concat_str: u32,
    pub str_eq: u32,
    pub concat_list: u32,
    pub list_eq: u32,
    pub mem_eq: u32,
    pub str_contains: u32,
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
    types: Vec<(Vec<ValType>, Vec<ValType>)>,
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

    // Type info: record/variant name → field list (for field offset computation)
    pub record_fields: HashMap<String, Vec<(String, crate::types::Ty)>>,
    // Variant info: variant type name → list of (case_name, tag, fields)
    pub variant_info: HashMap<String, Vec<VariantCase>>,

    // Lambda/closure info: sequential index → LambdaInfo
    pub lambdas: Vec<LambdaInfo>,
    // FnRef wrappers: original func name → wrapper table_idx
    pub fn_ref_wrappers: HashMap<String, u32>,
    // Lambda counter (for matching pre-scan order during emission)
    pub lambda_counter: std::cell::Cell<usize>,
}

/// A single case of a variant type.
pub struct VariantCase {
    pub name: String,
    pub tag: u32,
    pub fields: Vec<(String, crate::types::Ty)>,
}

/// Pre-scanned lambda information.
pub struct LambdaInfo {
    pub table_idx: u32,
    pub closure_type_idx: u32,
    pub captures: Vec<(crate::ir::VarId, crate::types::Ty)>,
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
                fd_write: 0,
                alloc: 0,
                println_str: 0,
                int_to_string: 0,
                println_int: 0,
                concat_str: 0,
                str_eq: 0,
                concat_list: 0,
                list_eq: 0,
                mem_eq: 0,
                str_contains: 0,
            },
            heap_ptr_global: 0,
            top_let_globals: HashMap::new(),
            top_let_init: Vec::new(),
            next_global: 1, // 0 = heap_ptr
            func_table: Vec::new(),
            func_to_table_idx: HashMap::new(),
            record_fields: HashMap::new(),
            variant_info: HashMap::new(),
            lambdas: Vec::new(),
            fn_ref_wrappers: HashMap::new(),
            lambda_counter: std::cell::Cell::new(0),
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

/// Per-function compilation state.
pub struct FuncCompiler<'a> {
    pub emitter: &'a mut WasmEmitter,
    pub func: Function,
    pub var_map: HashMap<u32, u32>,
    pub depth: u32,
    pub loop_stack: Vec<LoopLabels>,
    // Match/record scratch locals (one i64 + one i32 per nesting depth)
    pub match_i64_base: u32,
    pub match_i32_base: u32,
    pub match_depth: u32,
    // Variable table for name lookups (pattern matching)
    pub var_table: &'a crate::ir::VarTable,
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

    // Register type declarations (record and variant field layouts)
    for td in &program.type_decls {
        match &td.kind {
            crate::ir::IrTypeDeclKind::Record { fields } => {
                let field_list: Vec<(String, crate::types::Ty)> = fields.iter()
                    .map(|f| (f.name.clone(), f.ty.clone()))
                    .collect();
                emitter.record_fields.insert(td.name.clone(), field_list);
            }
            crate::ir::IrTypeDeclKind::Variant { cases, .. } => {
                let mut variant_cases = Vec::new();
                for (tag, case) in cases.iter().enumerate() {
                    let fields = match &case.kind {
                        crate::ir::IrVariantKind::Record { fields } => {
                            fields.iter().map(|f| (f.name.clone(), f.ty.clone())).collect()
                        }
                        crate::ir::IrVariantKind::Tuple { fields } => {
                            fields.iter().enumerate()
                                .map(|(i, ty)| (format!("_{}", i), ty.clone()))
                                .collect()
                        }
                        crate::ir::IrVariantKind::Unit => vec![],
                    };
                    // Also register each case name in record_fields for field access
                    emitter.record_fields.insert(case.name.clone(), fields.clone());
                    variant_cases.push(VariantCase {
                        name: case.name.clone(),
                        tag: tag as u32,
                        fields,
                    });
                }
                emitter.variant_info.insert(td.name.clone(), variant_cases);
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
            crate::ir::IrExprKind::LitInt { value } => *value,
            crate::ir::IrExprKind::LitFloat { value } => value.to_bits() as i64,
            crate::ir::IrExprKind::LitBool { value } => *value as i64,
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
        let params: Vec<ValType> = func.params.iter()
            .filter_map(|p| values::ty_to_valtype(&p.ty))
            .collect();
        let results = values::ret_type(&func.ret_ty);
        let type_idx = emitter.register_type(params, results);
        let func_idx = emitter.register_func(&func.name, type_idx);
        user_meta.push(type_idx);
        user_func_indices.push(func_idx);
        if func.is_test {
            test_func_indices.push((func_idx, func.name.clone()));
        }
    }

    let init_globals_idx: Option<u32> = None; // globals are initialized inline

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
    pre_scan_closures(program, &mut emitter);

    // Phase 2: Compile function bodies (order must match registration order)
    runtime::compile_runtime(&mut emitter);

    // User + test functions
    let mut user_idx = 0;
    for func in &program.functions {
        let type_idx = user_meta[user_idx];
        let compiled = functions::compile_function(&mut emitter, func, &program.var_table, type_idx);
        emitter.add_compiled(compiled);
        user_idx += 1;
    }

    // Test runner (if needed)
    if let Some(_runner_idx) = test_runner_idx {
        compile_test_runner(&mut emitter, &test_func_indices, init_globals_idx);
    }

    // Lambda bodies and FnRef wrappers
    compile_lambda_bodies(program, &mut emitter);

    // Phase 3: Assemble
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
        minimum: 1,
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
fn compile_init_globals(emitter: &mut WasmEmitter, program: &IrProgram) {
    let void_type = emitter.register_type(vec![], vec![]);

    // We need scratch locals for computing top_let values
    let scan_depth = program.top_lets.iter()
        .map(|tl| statements::count_scratch_depth_public(&tl.value))
        .max().unwrap_or(0).max(1);

    let mut local_decls = Vec::new();
    let match_i64_base = local_decls.len() as u32;
    for _ in 0..scan_depth {
        local_decls.push((1, ValType::I64));
    }
    let match_i32_base = local_decls.len() as u32;
    for _ in 0..scan_depth {
        local_decls.push((1, ValType::I32));
    }

    let wasm_func = Function::new(local_decls);
    let compiled_func = {
        let mut compiler = FuncCompiler {
            emitter: &mut *emitter,
            func: wasm_func,
            var_map: HashMap::new(),
            depth: 0,
            loop_stack: Vec::new(),
            match_i64_base,
            match_i32_base,
            match_depth: 0,
            var_table: &program.var_table,
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

// ── Lambda/Closure pre-scan and compilation ─────────────────────

use crate::ir::{IrExpr, IrExprKind, IrStmt, IrStmtKind, VarId};
use std::collections::HashSet;

/// Walk all function bodies to find Lambda and FnRef nodes.
/// Register lambda functions and FnRef wrappers in the emitter.
fn pre_scan_closures(program: &IrProgram, emitter: &mut WasmEmitter) {
    // Collect all lambdas (in tree-walk order)
    let mut lambda_exprs: Vec<(Vec<(VarId, crate::types::Ty)>, IrExpr, HashSet<u32>)> = Vec::new();
    let mut fn_ref_set: HashSet<String> = HashSet::new();
    let mut fn_ref_names: Vec<String> = Vec::new(); // ordered, deduped

    for func in &program.functions {
        // Include test functions in pre-scan/compile
        let mut scope_vars: HashSet<u32> = func.params.iter().map(|p| p.var.0).collect();
        scan_closures_expr(&func.body, &mut scope_vars, &program.var_table,
            &mut lambda_exprs, &mut fn_ref_set);
    }
    // Build ordered fn_ref list (sorted for determinism)
    fn_ref_names = fn_ref_set.into_iter().collect();
    fn_ref_names.sort();

    // Register each lambda as a function
    for (params, _body, captures) in &lambda_exprs {
        // Closure calling convention: (env: i32, declared_params...) -> ret
        let mut wasm_params = vec![ValType::I32]; // env_ptr
        for (_, ty) in params {
            if let Some(vt) = values::ty_to_valtype(ty) {
                wasm_params.push(vt);
            }
        }
        // Determine return type from body
        let ret_types = values::ret_type(&_body.ty);
        let closure_type_idx = emitter.register_type(wasm_params, ret_types);

        let name = format!("__lambda_{}", emitter.lambdas.len());
        let func_idx = emitter.register_func(&name, closure_type_idx);
        let table_idx = emitter.func_table.len() as u32;
        emitter.func_table.push(func_idx);
        emitter.func_to_table_idx.insert(func_idx, table_idx);

        let capture_vars: Vec<(VarId, crate::types::Ty)> = captures.iter()
            .map(|&vid| {
                let info = &program.var_table.get(VarId(vid));
                (VarId(vid), info.ty.clone())
            })
            .collect();

        emitter.lambdas.push(LambdaInfo {
            table_idx,
            closure_type_idx,
            captures: capture_vars,
        });
    }

    // Register FnRef wrappers
    for fn_name in &fn_ref_names {
        if emitter.fn_ref_wrappers.contains_key(fn_name.as_str()) { continue; }
        if let Some(&orig_func_idx) = emitter.func_map.get(fn_name.as_str()) {
            if let Some(&orig_type_idx) = emitter.func_type_indices.get(&orig_func_idx) {
                // Get original params/results
                let (orig_params, orig_results) = emitter.types[orig_type_idx as usize].clone();
                // Wrapper type: (env: i32, original_params...) -> original_results
                let mut wrapper_params = vec![ValType::I32];
                wrapper_params.extend_from_slice(&orig_params);
                let wrapper_type_idx = emitter.register_type(wrapper_params, orig_results);

                let wrapper_name = format!("__wrap_{}", fn_name);
                let wrapper_func_idx = emitter.register_func(&wrapper_name, wrapper_type_idx);
                let table_idx = emitter.func_table.len() as u32;
                emitter.func_table.push(wrapper_func_idx);
                emitter.func_to_table_idx.insert(wrapper_func_idx, table_idx);

                emitter.fn_ref_wrappers.insert(fn_name.clone(), table_idx);
            }
        }
    }
}

/// Compile lambda bodies and FnRef wrappers.
fn compile_lambda_bodies(program: &IrProgram, emitter: &mut WasmEmitter) {
    // Re-scan to get lambda bodies (in same order as pre-scan)
    let mut lambda_exprs: Vec<(Vec<(VarId, crate::types::Ty)>, IrExpr, HashSet<u32>)> = Vec::new();
    let mut fn_ref_set: HashSet<String> = HashSet::new();

    for func in &program.functions {
        // Include test functions in pre-scan/compile
        let mut scope_vars: HashSet<u32> = func.params.iter().map(|p| p.var.0).collect();
        scan_closures_expr(&func.body, &mut scope_vars, &program.var_table,
            &mut lambda_exprs, &mut fn_ref_set);
    }
    let mut fn_ref_names: Vec<String> = fn_ref_set.into_iter().collect();
    fn_ref_names.sort();

    // Compile each lambda
    for (i, (params, body, captures)) in lambda_exprs.iter().enumerate() {
        let info = &emitter.lambdas[i];
        let type_idx = info.closure_type_idx;

        // Build var_map: env_ptr is local 0, params start at 1
        let mut var_map: HashMap<u32, u32> = HashMap::new();
        let mut local_idx = 1u32; // 0 = env_ptr
        for (vid, _) in params {
            var_map.insert(vid.0, local_idx);
            local_idx += 1;
        }

        // Captured vars are loaded from env in the body emission
        // Map them to locals allocated after params
        let capture_list: Vec<(VarId, crate::types::Ty)> = captures.iter()
            .map(|&vid| {
                let vi = program.var_table.get(VarId(vid));
                (VarId(vid), vi.ty.clone())
            })
            .collect();

        // Pre-scan body for additional locals
        let scan = statements::collect_locals(body, &program.var_table);
        let mut local_decls = Vec::new();

        // Captured var locals
        for (vid, ty) in &capture_list {
            if let Some(vt) = values::ty_to_valtype(ty) {
                var_map.insert(vid.0, local_idx);
                local_decls.push((1u32, vt));
                local_idx += 1;
            }
        }

        // Body bind locals
        for (vid, vt) in &scan.binds {
            var_map.insert(vid.0, local_idx);
            local_decls.push((1u32, *vt));
            local_idx += 1;
        }

        // Scratch locals
        let scratch_depth = scan.scratch_depth.max(1);
        let match_i64_base = local_idx;
        for _ in 0..scratch_depth {
            local_decls.push((1, ValType::I64));
            local_idx += 1;
        }
        let match_i32_base = local_idx;
        for _ in 0..scratch_depth {
            local_decls.push((1, ValType::I32));
            local_idx += 1;
        }

        let mut wasm_func = Function::new(local_decls);

        // Load captured vars from env
        for (ci, (vid, ty)) in capture_list.iter().enumerate() {
            if let Some(vt) = values::ty_to_valtype(ty) {
                let cap_local = var_map[&vid.0];
                let offset = ci as u32 * 8; // each capture slot is 8 bytes (padded)
                wasm_func.instruction(&wasm_encoder::Instruction::LocalGet(0)); // env_ptr
                match vt {
                    ValType::I64 => {
                        wasm_func.instruction(&wasm_encoder::Instruction::I64Load(
                            wasm_encoder::MemArg { offset: offset as u64, align: 3, memory_index: 0 }
                        ));
                    }
                    ValType::F64 => {
                        wasm_func.instruction(&wasm_encoder::Instruction::F64Load(
                            wasm_encoder::MemArg { offset: offset as u64, align: 3, memory_index: 0 }
                        ));
                    }
                    _ => {
                        wasm_func.instruction(&wasm_encoder::Instruction::I32Load(
                            wasm_encoder::MemArg { offset: offset as u64, align: 2, memory_index: 0 }
                        ));
                    }
                }
                wasm_func.instruction(&wasm_encoder::Instruction::LocalSet(cap_local));
            }
        }

        // Compile body
        let compiled_func = {
            let mut compiler = FuncCompiler {
                emitter: &mut *emitter,
                func: wasm_func,
                var_map,
                depth: 0,
                loop_stack: Vec::new(),
                match_i64_base,
                match_i32_base,
                match_depth: 0,
                var_table: &program.var_table,
            };
            compiler.emit_expr(body);
            compiler.func.instruction(&wasm_encoder::Instruction::End);
            compiler.func
        };

        emitter.add_compiled(CompiledFunc { type_idx, func: compiled_func });
    }

    // Compile FnRef wrappers
    fn_ref_names.sort(); // deterministic order
    for fn_name in &fn_ref_names {
        if let Some(&orig_func_idx) = emitter.func_map.get(fn_name.as_str()) {
            if let Some(&orig_type_idx) = emitter.func_type_indices.get(&orig_func_idx) {
                let (orig_params, orig_results) = emitter.types[orig_type_idx as usize].clone();
                // Wrapper: (env: i32, params...) -> results  { call original(params...) }
                let mut wrapper_params = vec![ValType::I32];
                wrapper_params.extend_from_slice(&orig_params);
                let wrapper_type_idx = emitter.register_type(wrapper_params, orig_results);

                let mut f = Function::new([]);
                // Skip env (local 0), pass remaining params to original
                for i in 0..orig_params.len() {
                    f.instruction(&wasm_encoder::Instruction::LocalGet((i + 1) as u32));
                }
                f.instruction(&wasm_encoder::Instruction::Call(orig_func_idx));
                f.instruction(&wasm_encoder::Instruction::End);

                emitter.add_compiled(CompiledFunc { type_idx: wrapper_type_idx, func: f });
            }
        }
    }
}

/// Recursively scan an expression for Lambda and FnRef nodes.
fn scan_closures_expr(
    expr: &IrExpr,
    scope_vars: &mut HashSet<u32>,
    var_table: &crate::ir::VarTable,
    lambdas: &mut Vec<(Vec<(VarId, crate::types::Ty)>, IrExpr, HashSet<u32>)>,
    fn_refs: &mut HashSet<String>,
) {
    match &expr.kind {
        IrExprKind::Lambda { params, body } => {
            // Compute captures: vars referenced in body but not in params
            let param_ids: HashSet<u32> = params.iter().map(|(vid, _)| vid.0).collect();
            let mut body_vars = HashSet::new();
            collect_var_refs(body, &mut body_vars);
            let captures: HashSet<u32> = body_vars.difference(&param_ids)
                .copied()
                .filter(|vid| scope_vars.contains(vid))
                .collect();

            let param_list: Vec<(VarId, crate::types::Ty)> = params.iter()
                .map(|(vid, ty)| (*vid, ty.clone()))
                .collect();
            lambdas.push((param_list, *body.clone(), captures));

            // Also scan inside the lambda body for nested lambdas
            let mut inner_scope = scope_vars.clone();
            for (vid, _) in params {
                inner_scope.insert(vid.0);
            }
            scan_closures_expr(body, &mut inner_scope, var_table, lambdas, fn_refs);
        }
        IrExprKind::FnRef { name } => {
            fn_refs.insert(name.clone());
        }
        IrExprKind::Block { stmts, expr } | IrExprKind::DoBlock { stmts, expr } => {
            for stmt in stmts {
                scan_closures_stmt(stmt, scope_vars, var_table, lambdas, fn_refs);
            }
            if let Some(e) = expr { scan_closures_expr(e, scope_vars, var_table, lambdas, fn_refs); }
        }
        IrExprKind::If { cond, then, else_ } => {
            scan_closures_expr(cond, scope_vars, var_table, lambdas, fn_refs);
            scan_closures_expr(then, scope_vars, var_table, lambdas, fn_refs);
            scan_closures_expr(else_, scope_vars, var_table, lambdas, fn_refs);
        }
        IrExprKind::BinOp { left, right, .. } => {
            scan_closures_expr(left, scope_vars, var_table, lambdas, fn_refs);
            scan_closures_expr(right, scope_vars, var_table, lambdas, fn_refs);
        }
        IrExprKind::UnOp { operand, .. } => {
            scan_closures_expr(operand, scope_vars, var_table, lambdas, fn_refs);
        }
        IrExprKind::Call { target, args, .. } => {
            match target {
                crate::ir::CallTarget::Method { object, .. } => scan_closures_expr(object, scope_vars, var_table, lambdas, fn_refs),
                crate::ir::CallTarget::Computed { callee } => scan_closures_expr(callee, scope_vars, var_table, lambdas, fn_refs),
                _ => {}
            }
            for arg in args { scan_closures_expr(arg, scope_vars, var_table, lambdas, fn_refs); }
        }
        IrExprKind::While { cond, body } => {
            scan_closures_expr(cond, scope_vars, var_table, lambdas, fn_refs);
            for stmt in body { scan_closures_stmt(stmt, scope_vars, var_table, lambdas, fn_refs); }
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            scan_closures_expr(iterable, scope_vars, var_table, lambdas, fn_refs);
            for stmt in body { scan_closures_stmt(stmt, scope_vars, var_table, lambdas, fn_refs); }
        }
        IrExprKind::Match { subject, arms } => {
            scan_closures_expr(subject, scope_vars, var_table, lambdas, fn_refs);
            for arm in arms { scan_closures_expr(&arm.body, scope_vars, var_table, lambdas, fn_refs); }
        }
        IrExprKind::Record { fields, .. } => {
            for (_, e) in fields { scan_closures_expr(e, scope_vars, var_table, lambdas, fn_refs); }
        }
        IrExprKind::Tuple { elements } | IrExprKind::List { elements } => {
            for e in elements { scan_closures_expr(e, scope_vars, var_table, lambdas, fn_refs); }
        }
        IrExprKind::Member { object, .. } | IrExprKind::IndexAccess { object, .. } => {
            scan_closures_expr(object, scope_vars, var_table, lambdas, fn_refs);
        }
        IrExprKind::StringInterp { parts } => {
            for p in parts {
                if let crate::ir::IrStringPart::Expr { expr } = p {
                    scan_closures_expr(expr, scope_vars, var_table, lambdas, fn_refs);
                }
            }
        }
        _ => {}
    }
}

fn scan_closures_stmt(
    stmt: &IrStmt,
    scope_vars: &mut HashSet<u32>,
    var_table: &crate::ir::VarTable,
    lambdas: &mut Vec<(Vec<(VarId, crate::types::Ty)>, IrExpr, HashSet<u32>)>,
    fn_refs: &mut HashSet<String>,
) {
    match &stmt.kind {
        IrStmtKind::Bind { var, value, .. } => {
            scan_closures_expr(value, scope_vars, var_table, lambdas, fn_refs);
            scope_vars.insert(var.0);
        }
        IrStmtKind::Assign { value, .. } => {
            scan_closures_expr(value, scope_vars, var_table, lambdas, fn_refs);
        }
        IrStmtKind::Expr { expr } => {
            scan_closures_expr(expr, scope_vars, var_table, lambdas, fn_refs);
        }
        IrStmtKind::Guard { cond, else_ } => {
            scan_closures_expr(cond, scope_vars, var_table, lambdas, fn_refs);
            scan_closures_expr(else_, scope_vars, var_table, lambdas, fn_refs);
        }
        _ => {}
    }
}

/// Collect all Var references in an expression.
fn collect_var_refs(expr: &IrExpr, vars: &mut HashSet<u32>) {
    match &expr.kind {
        IrExprKind::Var { id } => { vars.insert(id.0); }
        IrExprKind::Block { stmts, expr } | IrExprKind::DoBlock { stmts, expr } => {
            for stmt in stmts {
                match &stmt.kind {
                    IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } => collect_var_refs(value, vars),
                    IrStmtKind::Expr { expr } => collect_var_refs(expr, vars),
                    IrStmtKind::Guard { cond, else_ } => { collect_var_refs(cond, vars); collect_var_refs(else_, vars); }
                    _ => {}
                }
            }
            if let Some(e) = expr { collect_var_refs(e, vars); }
        }
        IrExprKind::If { cond, then, else_ } => {
            collect_var_refs(cond, vars); collect_var_refs(then, vars); collect_var_refs(else_, vars);
        }
        IrExprKind::BinOp { left, right, .. } => { collect_var_refs(left, vars); collect_var_refs(right, vars); }
        IrExprKind::UnOp { operand, .. } => collect_var_refs(operand, vars),
        IrExprKind::Call { args, target, .. } => {
            if let crate::ir::CallTarget::Computed { callee } = target { collect_var_refs(callee, vars); }
            if let crate::ir::CallTarget::Method { object, .. } = target { collect_var_refs(object, vars); }
            for a in args { collect_var_refs(a, vars); }
        }
        _ => {}
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
            var_table: crate::ir::VarTable::new(),
            modules: vec![],
            type_registry: Default::default(),
            effect_map: Default::default(),
        };
        let bytes = emit(&program);
        assert_eq!(&bytes[0..4], b"\0asm");
        assert_eq!(&bytes[4..8], &[1, 0, 0, 0]);
    }
}
