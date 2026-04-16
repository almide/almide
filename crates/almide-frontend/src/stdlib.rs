/// Centralized stdlib definitions for the Almide compiler.

use crate::types::FnSig;

// Re-export from almide-lang for backwards compatibility.
pub use almide_lang::stdlib_info::{
    STDLIB_MODULES, BUNDLED_MODULES, AUTO_IMPORT_BUNDLED,
    is_stdlib_module, is_any_stdlib, is_bundled_module,
    resolve_ufcs_module, resolve_ufcs_candidates,
};

/// Modules that can safely be suggested via "Add `import X`" in error hints.
/// Excludes auto-imported modules and names that are common as variable names
/// (e.g. `value`, `error`, `string`, `list`, `map`, `set`, `option`, `result`).
pub fn is_import_suggestable(name: &str) -> bool {
    matches!(name, "json" | "http" | "fs" | "process" | "regex" | "datetime" | "io" | "random" | "testing" | "bytes" | "matrix" | "env")
}

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
pub fn get_bundled_source(name: &str) -> Option<&'static str> {
    match name {
        "args" => Some(include_str!("../../../stdlib/args.almd")),
        "path" => Some(include_str!("../../../stdlib/path.almd")),
        "option" => Some(include_str!("../../../stdlib/option.almd")),
        "result" => Some(include_str!("../../../stdlib/result.almd")),
        _ => None,
    }
}

/// Resolve UFCS module by receiver type (compile-time resolution).
pub fn resolve_ufcs_by_type(method: &str, receiver_type: almide_lang::ast::ResolvedType) -> Option<&'static str> {
    use almide_lang::ast::ResolvedType;
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

/// Suggest the correct stdlib function for a commonly hallucinated name.
/// Returns `Some("module.function")` if a known alias exists.
pub fn suggest_alias(module: &str, func: &str) -> Option<&'static str> {
    match (module, func) {
        // size → len
        ("set", "size") | ("list", "size") | ("map", "size") | ("string", "size") => {
            Some(match module { "set" => "set.len", "list" => "list.len", "map" => "map.len", _ => "string.len" })
        }
        ("set", "count") | ("list", "count") | ("map", "count") => {
            Some(match module { "set" => "set.len", "list" => "list.len", _ => "map.len" })
        }
        // skip → drop
        ("list", "skip") => Some("list.drop"),
        // string parse functions → int/float module
        ("string", "to_int") | ("string", "to_integer") | ("string", "parse_int") => Some("int.parse"),
        ("string", "to_float") | ("string", "parse_float") => Some("float.parse"),
        // int.from_string → int.parse
        ("int", "from_string") | ("int", "from_str") => Some("int.parse"),
        ("float", "from_string") | ("float", "from_str") => Some("float.parse"),
        // char code
        ("string", "char_code") | ("string", "char_code_at") | ("string", "code_at")
        | ("string", "char_at_code") | ("string", "ord") => Some("string.codepoint"),
        // case conversion
        ("string", "to_lowercase") | ("string", "lowercase") | ("string", "lower") => Some("string.to_lower"),
        ("string", "to_uppercase") | ("string", "uppercase") | ("string", "upper") => Some("string.to_upper"),
        // substring
        ("string", "substring") | ("string", "substr") => Some("string.slice"),
        // length
        ("string", "length") | ("list", "length") | ("map", "length") | ("set", "length") => {
            Some(match module { "string" => "string.len", "list" => "list.len", "map" => "map.len", _ => "set.len" })
        }
        // list operations
        ("list", "push") | ("list", "append") => Some("list.concat (use [xs, [x]] or xs + [x])"),
        ("list", "has") | ("list", "includes") => Some("list.contains"),
        ("list", "find_index") => Some("list.index_of"),
        // string
        ("string", "includes") | ("string", "has") => Some("string.contains"),
        ("string", "index") => Some("string.index_of"),
        ("string", "all") => Some("string.chars + list.all"),
        // Common LLM hallucinations from MSR testing
        ("string", "get_char") | ("string", "charAt") | ("string", "get") => Some("string.char_at"),
        ("string", "from_char") | ("string", "from_char_code") | ("string", "chr") => Some("string.from_codepoint"),
        ("list", "foldLeft") | ("list", "foldRight") | ("list", "reduce") | ("list", "foldl") | ("list", "foldr") => Some("list.fold"),
        ("list", "empty") | ("list", "new") => Some("[] (empty list literal)"),
        ("list", "head") => Some("list.first"),
        ("list", "tail") => Some("list.drop(xs, 1)"),
        ("map", "new") | ("map", "empty") => Some("[:] (empty map literal)"),
        ("map", "has_key") | ("map", "has") | ("map", "includes") => Some("map.contains"),
        // sqrt: Almide only has float.sqrt, not int.sqrt. Most LLMs reach
        // for int.sqrt(n) in is-prime / perfect-square style tasks.
        ("int", "sqrt") => Some("float.sqrt(int.to_float(n))"),
        // comparison functions: Almide uses operators, not methods.
        ("int", "gt") | ("float", "gt") => Some("a > b (operator)"),
        ("int", "lt") | ("float", "lt") => Some("a < b (operator)"),
        ("int", "gte") | ("int", "ge") | ("float", "gte") | ("float", "ge") => Some("a >= b (operator)"),
        ("int", "lte") | ("int", "le") | ("float", "lte") | ("float", "le") => Some("a <= b (operator)"),
        ("int", "eq") | ("float", "eq") | ("string", "eq") | ("bool", "eq") => Some("a == b (operator)"),
        ("int", "neq") | ("int", "ne") | ("float", "neq") | ("float", "ne") => Some("a != b (operator)"),
        _ => None,
    }
}

/// Rich multi-line `try:` snippet for well-known LLM hallucinations that
/// don't map to a single clean function name — conversion-wrappers,
/// operator forms, etc. `suggest_alias` returns free-text for these cases
/// (suppressing the default "fn(...)" try: snippet); this table provides
/// a concrete fix template instead.
pub fn try_snippet_for_alias(module: &str, func: &str) -> Option<&'static str> {
    match (module, func) {
        ("int", "sqrt") => Some(
            "// Almide has float.sqrt; int.sqrt doesn't exist.\n\
             // Convert → sqrt → (optionally) convert back:\n\
             let root_f = float.sqrt(int.to_float(n))       // Float\n\
             let root_i = float.to_int(root_f)              // Int (truncates)\n\
             // — or inline: float.to_int(float.sqrt(int.to_float(n)))"
        ),
        ("int", "gt") | ("int", "lt") | ("int", "gte") | ("int", "ge")
        | ("int", "lte") | ("int", "le") | ("int", "eq") | ("int", "neq") | ("int", "ne")
        | ("float", "gt") | ("float", "lt") | ("float", "gte") | ("float", "ge")
        | ("float", "lte") | ("float", "le") | ("float", "eq") | ("float", "neq") | ("float", "ne") => Some(
            "// Almide uses operators, not comparison functions:\n\
             //   int.gt(a, b)   →  a > b\n\
             //   int.lt(a, b)   →  a < b\n\
             //   int.gte(a, b)  →  a >= b\n\
             //   int.lte(a, b)  →  a <= b\n\
             //   int.eq(a, b)   →  a == b\n\
             //   int.neq(a, b)  →  a != b\n\
             // (same for float, string, bool — == and != work on any type)"
        ),
        _ => None,
    }
}

/// Names of built-in effect functions (not module-scoped).
pub fn builtin_effect_fns() -> Vec<&'static str> {
    vec!["println", "eprintln", "panic"]
}

/// Return all function names in a stdlib module.
pub fn module_functions(module: &str) -> Vec<&'static str> {
    crate::generated::stdlib_sigs::generated_module_functions(module)
}

/// Look up a stdlib function's type signature.
pub fn lookup_sig(module: &str, func: &str) -> Option<FnSig> {
    crate::generated::stdlib_sigs::lookup_generated_sig(module, func)
}
