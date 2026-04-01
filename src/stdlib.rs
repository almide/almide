/// Centralized stdlib definitions for the Almide compiler.
/// Both the type checker (check.rs) and code generator (codegen) reference this module
/// to avoid duplicating function signatures, module lists, and UFCS mappings.

use crate::types::FnSig;

// Re-export from almide-lang for backwards compatibility.
pub use almide_lang::stdlib_info::{
    STDLIB_MODULES, BUNDLED_MODULES,
    is_stdlib_module, is_any_stdlib, is_bundled_module,
    resolve_ufcs_module, resolve_ufcs_candidates,
};

/// Bundled stdlib modules that should be auto-imported (Tier 1 behavior).
/// These are written in Almide but available without explicit `import`.
pub const AUTO_IMPORT_BUNDLED: &[&str] = &["option", "result"];

/// Short description of a stdlib module (for error hints).
pub fn module_description(name: &str) -> &'static str {
    match name {
        "string" => "string manipulation",
        "list" => "list operations",
        "int" => "integer utilities",
        "float" => "floating-point utilities",
        "bytes" => "byte buffer operations",
        "matrix" => "matrix operations",
        "fs" => "file system operations",
        "env" => "environment variables",
        "map" => "hash map operations",
        "json" => "JSON parsing and querying",
        "http" => "HTTP client",
        "process" => "process execution",
        "math" => "mathematical functions",
        "random" => "random number generation",
        "regex" => "regular expressions",
        "io" => "input/output",
        "result" => "Result type utilities",
        "option" => "Option type utilities",
        "error" => "error handling",
        "datetime" => "date and time operations",
        "testing" => "test assertion utilities",
        "value" => "dynamic value operations",
        "set" => "hash set operations",
        _ => "standard library module",
    }
}

/// Bundled stdlib packages written in Almide (.almd files embedded in the compiler binary).
/// These are loaded as user modules — no hardcoded type signatures or codegen needed.
pub fn get_bundled_source(name: &str) -> Option<&'static str> {
    match name {
        "args" => Some(include_str!("../stdlib/args.almd")),
        "path" => Some(include_str!("../stdlib/path.almd")),
        "option" => Some(include_str!("../stdlib/option.almd")),
        "result" => Some(include_str!("../stdlib/result.almd")),
        _ => None,
    }
}

/// Resolve UFCS module by receiver type (compile-time resolution).
/// Returns the correct module for a method based on the known type of the receiver.
/// Returns None if the receiver type doesn't match any candidate, or the type is Unknown.
pub fn resolve_ufcs_by_type(method: &str, receiver_type: crate::ast::ResolvedType) -> Option<&'static str> {
    use crate::ast::ResolvedType;
    let candidates = resolve_ufcs_candidates(method);
    if candidates.is_empty() {
        return None;
    }
    let module = match receiver_type {
        ResolvedType::String => "string",
        ResolvedType::List => "list",
        ResolvedType::Map => "map",
        ResolvedType::Set => "set",
        ResolvedType::Int => "int",
        ResolvedType::Float => "float",
        ResolvedType::Result => "result",
        ResolvedType::Bytes => "bytes",
        ResolvedType::Matrix => "matrix",
        _ => return None,
    };
    if candidates.contains(&module) {
        Some(module)
    } else {
        None
    }
}

/// Minimum number of required parameters for a stdlib function.
pub fn min_params(module: &str, func: &str) -> Option<usize> {
    match (module, func) {
        ("string", "slice") => Some(2),
        _ => None,
    }
}

/// Names of built-in effect functions (not module-scoped).
pub fn builtin_effect_fns() -> Vec<&'static str> {
    vec!["println", "eprintln", "panic"]
}

/// Return all function names in a stdlib module (for "did you mean?" suggestions).
pub fn module_functions(module: &str) -> Vec<&'static str> {
    crate::generated::stdlib_sigs::generated_module_functions(module)
}

/// Look up a stdlib function's type signature.
pub fn lookup_sig(module: &str, func: &str) -> Option<FnSig> {
    crate::generated::stdlib_sigs::lookup_generated_sig(module, func)
}
