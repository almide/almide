/// Centralized stdlib definitions for the Almide compiler.
/// Both the type checker (check.rs) and code generator (emit_rust.rs) reference this module
/// to avoid duplicating function signatures, module lists, and UFCS mappings.

use crate::types::FnSig;

/// All built-in stdlib module names (hardcoded in the compiler).
pub const STDLIB_MODULES: &[&str] = &["string", "list", "int", "float", "bytes", "matrix", "fs", "env", "map", "json", "http", "process", "math", "random", "regex", "io", "result", "option", "error", "datetime", "testing", "value", "set"];

/// Bundled stdlib modules that should be auto-imported (Tier 1 behavior).
/// These are written in Almide but available without explicit `import`.
pub const AUTO_IMPORT_BUNDLED: &[&str] = &["option", "result"];


/// Check if a module name is a hardcoded stdlib module.
pub fn is_stdlib_module(name: &str) -> bool {
    STDLIB_MODULES.contains(&name)
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



/// Check if a module name is any kind of stdlib (hardcoded or bundled).
pub fn is_any_stdlib(name: &str) -> bool {
    is_stdlib_module(name) || get_bundled_source(name).is_some()
}

/// Resolve a method name to its stdlib module (for UFCS / dot syntax).
/// e.g. `x.trim()` → `string.trim(x)`
/// For ambiguous methods (exist in multiple modules), returns `None`.
/// Use `resolve_ufcs_candidates` for those.
pub fn resolve_ufcs_module(method: &str) -> Option<&'static str> {
    let candidates = resolve_ufcs_candidates(method);
    if candidates.is_empty() {
        None
    } else {
        // For single candidate, return it directly.
        // For ambiguous methods, return the first candidate as a reasonable default.
        // The Rust emitter uses this; the TS emitter uses resolve_ufcs_candidates() for runtime dispatch.
        Some(candidates[0])
    }
}

/// Return all stdlib modules that contain a given method name.
/// Used for runtime dispatch when a method is ambiguous (e.g. `len` in string/list/map).
pub fn resolve_ufcs_candidates(method: &str) -> Vec<&'static str> {
    match method {
        // ── string-only ──
        "trim" | "split" | "pad_start"
        | "starts_with" | "ends_with"
        | "to_bytes" | "to_upper" | "to_lower" | "capitalize"
        | "replace" | "lines"
        | "chars" | "repeat" | "from_bytes"
        | "is_digit" | "is_alpha" | "is_alphanumeric"
        | "is_whitespace" | "is_upper" | "is_lower"
        | "codepoint" | "from_codepoint"
        | "pad_end" | "trim_start" | "trim_end"
        | "strip_prefix" | "strip_suffix"
        | "replace_first" | "last_index_of" => vec!["string"],

        // ── list-only ──
        "enumerate"
        | "sort_by" | "unique" | "unique_by"
        | "chunk" | "sum" | "product"
        | "filter_map" | "take_while" | "drop_while"
        | "reduce" | "group_by"
        | "remove_at" | "find_index"
        | "scan" | "intersperse"
        | "windows" | "dedup" | "zip_with"
        | "push" | "pop"
        | "shuffle" | "window" => vec!["list"],

        // ── list + map + set ──
        "each" | "fold" | "any" | "all" => vec!["list", "map", "set"],

        // ── list + map ──
        "find" | "partition" | "update" => vec!["list", "map"],

        // ── map-only ──
        "keys" | "values" | "entries" | "merge"
        | "delete"
        => vec!["map"],

        // ── set-only ──
        "union" | "intersection" | "difference" | "symmetric_difference"
        | "is_subset" | "is_disjoint" => vec!["set"],

        // ── int-only ──
        "to_string" | "to_hex"
        | "band" | "bor" | "bxor" | "bnot" | "bshl" | "bshr"
        | "wrap_add" | "wrap_mul" | "rotate_right" | "rotate_left"
        | "to_u32" | "to_u8" => vec!["int"],

        // ── float-only ──
        "to_fixed" | "round" | "floor" | "ceil" | "sqrt"
        | "is_nan" | "is_infinite" | "to_int" => vec!["float"],

        // ── option-only ──
        "is_some" | "is_none" | "to_result" | "or_else" => vec!["option"],
        "to_list" => vec!["set", "option"],

        // ── result-only ──
        "map_err"
        | "is_ok" | "is_err"
        | "to_err_option" | "to_option" => vec!["result"],

        // ── error-only ──
        "context" | "message" | "chain" => vec!["error"],

        // ── datetime-only ──
        "is_before" | "is_after" => vec!["datetime"],

        // ── ambiguous: string + list ──
        "first" | "last" => vec!["string", "list"],
        "take" | "drop" | "take_end" | "drop_end" => vec!["string", "list"],
        "reverse" => vec!["string", "list"],
        "index_of" => vec!["string", "list"],
        "join" => vec!["string", "list"],
        "slice" => vec!["string", "list"],

        // ── ambiguous: string + list + map + set ──
        "len" => vec!["string", "list", "map", "set"],
        "contains" => vec!["string", "list", "map", "set"],
        "is_empty" => vec!["string", "list", "map", "set"],

        // ── ambiguous: string + list + map ──
        "count" => vec!["string", "list", "map"],

        // ── ambiguous: list + result + option ──
        "flat_map" => vec!["list", "result", "option"],
        "unwrap_or" | "unwrap_or_else" => vec!["result", "option"],
        "flatten" | "zip" => vec!["list", "option"],

        // ── ambiguous: list + map + result + option ──
        "map" | "filter" => vec!["list", "map", "set", "result", "option"],

        // ── ambiguous: list + map ──
        "insert" | "clear" => vec!["list", "map", "set"],
        "remove" => vec!["map", "set"],

        // ── ambiguous: list + map ──
        "get" | "get_or" => vec!["string", "list", "map"],
        "set" => vec!["list", "map"],

        // ── ambiguous: list ──
        "swap" | "sort" | "min" | "max" => vec!["list"],

        // ── ambiguous: int + float ──
        "abs" | "clamp" => vec!["int", "float"],
        "to_float" => vec!["string", "int"],

        // ── ambiguous: math + float ──
        "sign" => vec!["math", "float"],

        _ => vec![],
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
    // Map receiver type to module name
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
        _ => return None, // Unknown, Record, etc. — cannot resolve at compile time
    };
    if candidates.contains(&module) {
        Some(module)
    } else {
        None
    }
}

/// Minimum number of required parameters for a stdlib function.
/// Most functions require all params; some have optional trailing params.
pub fn min_params(module: &str, func: &str) -> Option<usize> {
    match (module, func) {
        ("string", "slice") => Some(2), // 3rd param (end) is optional
        _ => None, // use sig.params.len()
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
/// All signatures are auto-generated from stdlib/defs/*.toml
pub fn lookup_sig(module: &str, func: &str) -> Option<FnSig> {
    crate::generated::stdlib_sigs::lookup_generated_sig(module, func)
}
