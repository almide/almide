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

    // Function table: func_idx → table_idx (for call_indirect / FnRef)
    pub func_table: Vec<u32>, // list of func_idx in table order
    pub func_to_table_idx: HashMap<u32, u32>, // func_idx → table index

    // Type info: record/variant name → field list (for field offset computation)
    pub record_fields: HashMap<String, Vec<(String, crate::types::Ty)>>,
    // Variant info: variant type name → list of (case_name, tag, fields)
    pub variant_info: HashMap<String, Vec<VariantCase>>,
}

/// A single case of a variant type.
pub struct VariantCase {
    pub name: String,
    pub tag: u32,
    pub fields: Vec<(String, crate::types::Ty)>,
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
            },
            heap_ptr_global: 0,
            func_table: Vec::new(),
            func_to_table_idx: HashMap::new(),
            record_fields: HashMap::new(),
            variant_info: HashMap::new(),
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

    // Register user function signatures
    let mut user_meta: Vec<u32> = Vec::new(); // type_idx per user function
    let mut user_func_indices: Vec<u32> = Vec::new();
    for func in &program.functions {
        if func.is_test {
            continue;
        }
        let params: Vec<ValType> = func.params.iter()
            .filter_map(|p| values::ty_to_valtype(&p.ty))
            .collect();
        let results = values::ret_type(&func.ret_ty);
        let type_idx = emitter.register_type(params, results);
        let func_idx = emitter.register_func(&func.name, type_idx);
        user_meta.push(type_idx);
        user_func_indices.push(func_idx);
    }

    // Build function table (for call_indirect / FnRef)
    // Include all user functions in the table
    for &func_idx in &user_func_indices {
        let table_idx = emitter.func_table.len() as u32;
        emitter.func_table.push(func_idx);
        emitter.func_to_table_idx.insert(func_idx, table_idx);
    }

    // Phase 2: Compile function bodies
    runtime::compile_runtime(&mut emitter);

    let mut user_idx = 0;
    for func in &program.functions {
        if func.is_test {
            continue;
        }
        let type_idx = user_meta[user_idx];
        let compiled = functions::compile_function(&mut emitter, func, &program.var_table, type_idx);
        emitter.add_compiled(compiled);
        user_idx += 1;
    }

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
    module.section(&globals);

    // ── Export section ──
    let mut exports = ExportSection::new();
    exports.export("memory", wasm_encoder::ExportKind::Memory, 0);
    if let Some(&main_idx) = emitter.func_map.get("main") {
        exports.export("_start", wasm_encoder::ExportKind::Func, main_idx);
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
