mod program;
mod ir_expressions;
mod ir_blocks;
pub mod borrow;

use crate::ast::*;

pub(crate) const JSON_RUNTIME: &str = include_str!("json_runtime.txt");
pub(crate) const HTTP_RUNTIME: &str = include_str!("http_runtime.txt");
pub(crate) const TIME_RUNTIME: &str = include_str!("time_runtime.txt");
pub(crate) const REGEX_RUNTIME: &str = include_str!("regex_runtime.txt");
pub(crate) const IO_RUNTIME: &str = include_str!("io_runtime.txt");
pub(crate) const PLATFORM_RUNTIME: &str = include_str!("platform_runtime.txt");
pub(crate) const COLLECTION_RUNTIME: &str = include_str!("collection_runtime.txt");
pub(crate) const CORE_RUNTIME: &str = include_str!("core_runtime.txt");

/// Info about an open record field, with optional nested open record projection.
#[derive(Debug, Clone)]
pub struct OpenFieldInfo {
    pub name: String,
    /// If this field itself is an open record, its struct name and nested fields
    pub nested: Option<(String, Vec<OpenFieldInfo>)>,
}

pub struct EmitOptions {
    /// Skip thread wrapper around main (for WASM targets where threads are unavailable)
    pub no_thread_wrap: bool,
    /// Fast mode: emit unchecked index access for maximum performance
    pub fast_mode: bool,
}

impl Default for EmitOptions {
    fn default() -> Self {
        Self { no_thread_wrap: false, fast_mode: false }
    }
}

pub(crate) struct Emitter {
    pub(crate) out: String,
    pub(crate) indent: usize,
    /// Track if we're inside an effect function (for ? operator)
    pub(crate) in_effect: bool,
    /// Names of effect functions in the program (auto-wrapped, no explicit Result return)
    pub(crate) effect_fns: Vec<String>,
    /// Names of all functions that return Result (for do-block auto-unwrap)
    pub(crate) result_fns: Vec<String>,
    /// Track if we're inside a do block (for auto-unwrap of Result calls)
    pub(crate) in_do_block: std::cell::Cell<bool>,
    /// Names of user-defined modules (for module call dispatch)
    pub(crate) user_modules: Vec<String>,
    /// Track if we're inside a test function
    pub(crate) in_test: bool,
    /// Skip thread wrapper around main
    pub(crate) no_thread_wrap: bool,
    /// Maps import name to versioned mod name for diamond deps
    pub(crate) module_aliases: std::collections::HashMap<String, String>,
    /// Temporarily suppress auto-? (e.g., when match subject has ok/err arms)
    pub(crate) skip_auto_q: std::cell::Cell<bool>,
    /// Collected anonymous record field-name sets → generated struct names
    pub(crate) anon_record_structs: std::cell::RefCell<std::collections::HashMap<Vec<String>, String>>,
    /// Counter for generating unique anonymous record struct names
    pub(crate) anon_record_counter: std::cell::Cell<usize>,
    /// Named record types: field-name set → declared struct name
    pub(crate) named_record_types: std::collections::HashMap<Vec<String>, String>,
    /// Generic variant constructor → enum name (for pattern matching with qualified paths)
    pub(crate) generic_variant_constructors: std::collections::HashMap<String, String>,
    /// Generic variant unit constructors (no payload) — need `()` when used as expressions
    pub(crate) generic_variant_unit_ctors: std::collections::HashSet<String>,
    /// Constructor args that need Box wrapping (recursive variants): (ctor_name, arg_index)
    pub(crate) boxed_variant_args: std::collections::HashSet<(String, usize)>,
    /// Record variant fields that need Box wrapping: (ctor_name, field_name)
    pub(crate) boxed_variant_record_fields: std::collections::HashSet<(String, String)>,
    /// Variables used only once in the current function body — safe to move instead of clone
    pub(crate) single_use_vars: std::collections::HashSet<String>,
    /// Borrow inference results: which params can be passed by reference
    pub(crate) borrow_info: borrow::BorrowInfo,
    /// Parameters that are currently borrowed (&str, &[T]) — need .to_owned()/.to_vec() instead of .clone()
    pub(crate) borrowed_params: std::collections::HashMap<String, String>,
    /// Current module name (for qualifying intra-module calls in borrow lookup)
    pub(crate) current_module: Option<String>,
    /// Fast mode: emit unchecked index access for maximum performance
    pub(crate) fast_mode: bool,
    /// Top-level let names. Value = true if emitted as LazyLock (needs deref+clone on reference)
    pub(crate) top_let_names: std::collections::HashMap<String, bool>,
    /// Typed IR program (available when type checking succeeded)
    pub(crate) ir_program: Option<almide::ir::IrProgram>,
    /// Typed IR for imported user modules (module_name → IrProgram)
    pub(crate) module_irs: std::collections::HashMap<String, almide::ir::IrProgram>,
    /// Open record params: fn_name → [(param_index, struct_name, field_infos)]
    /// Each field_info: (field_name, optional nested open record info)
    pub(crate) open_record_params: std::collections::HashMap<String, Vec<(usize, String, Vec<OpenFieldInfo>)>>,
    /// Shape aliases that resolve to open records: alias_name → field types
    pub(crate) open_record_aliases: std::collections::HashMap<String, Vec<crate::ast::FieldType>>,
}

impl Emitter {
    fn new(options: &EmitOptions) -> Self {
        Self { out: String::new(), indent: 0, in_effect: false, effect_fns: Vec::new(), result_fns: Vec::new(), in_do_block: std::cell::Cell::new(false), user_modules: Vec::new(), in_test: false, no_thread_wrap: options.no_thread_wrap, module_aliases: std::collections::HashMap::new(), skip_auto_q: std::cell::Cell::new(false), anon_record_structs: std::cell::RefCell::new(std::collections::HashMap::new()), anon_record_counter: std::cell::Cell::new(0), named_record_types: std::collections::HashMap::new(), generic_variant_constructors: std::collections::HashMap::new(), generic_variant_unit_ctors: std::collections::HashSet::new(), boxed_variant_args: std::collections::HashSet::new(), boxed_variant_record_fields: std::collections::HashSet::new(), single_use_vars: std::collections::HashSet::new(), borrow_info: borrow::BorrowInfo::new(), borrowed_params: std::collections::HashMap::new(), current_module: None, fast_mode: options.fast_mode, top_let_names: std::collections::HashMap::new(), ir_program: None, module_irs: std::collections::HashMap::new(), open_record_params: std::collections::HashMap::new(), open_record_aliases: std::collections::HashMap::new() }
    }

    pub(crate) fn emit_indent(&mut self) {
        for _ in 0..self.indent {
            self.out.push_str("    ");
        }
    }

    pub(crate) fn emitln(&mut self, s: &str) {
        self.emit_indent();
        self.out.push_str(s);
        self.out.push('\n');
    }

    /// Get or create a struct name for an anonymous record with given field names.
    /// If a named type (struct) with matching fields exists, use that name instead.
    pub(crate) fn anon_record_name(&self, field_names: &[String]) -> String {
        let key: Vec<String> = field_names.to_vec();
        if let Some(name) = self.named_record_types.get(&key) {
            return name.clone();
        }
        self.fresh_anon_record_name(field_names)
    }

    /// Always generate a fresh AlmdRec struct, never reuse named types.
    /// Used for open record params which must accept any record with matching fields.
    pub(crate) fn fresh_anon_record_name(&self, field_names: &[String]) -> String {
        let key: Vec<String> = field_names.to_vec();
        let map = self.anon_record_structs.borrow();
        if let Some(name) = map.get(&key) {
            return name.clone();
        }
        drop(map);
        let counter = self.anon_record_counter.get();
        let name = format!("AlmdRec{}", counter);
        self.anon_record_counter.set(counter + 1);
        self.anon_record_structs.borrow_mut().insert(key, name.clone());
        name
    }
}

pub fn emit_with_options(program: &Program, modules: &[(String, Program, Option<crate::project::PkgId>, bool)], options: &EmitOptions, import_aliases: &[(String, String)], ir: Option<&almide::ir::IrProgram>, module_irs: &std::collections::HashMap<String, almide::ir::IrProgram>) -> String {
    let mut emitter = Emitter::new(options);
    // Register user-level import aliases (import pkg as alias)
    for (alias, target) in import_aliases {
        emitter.module_aliases.insert(alias.clone(), target.clone());
    }
    // Store IR for codegen
    emitter.ir_program = ir.cloned();
    emitter.module_irs = module_irs.clone();
    // Run borrow inference on IR
    if let Some(ir_prog) = ir {
        emitter.borrow_info = borrow::analyze_program(ir_prog, module_irs);
    }
    emitter.emit_program(program, modules);
    emitter.out
}
