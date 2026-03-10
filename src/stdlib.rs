/// Centralized stdlib definitions for the Almide compiler.
/// Both the type checker (check.rs) and code generator (emit_rust.rs) reference this module
/// to avoid duplicating function signatures, module lists, and UFCS mappings.

use crate::types::{Ty, FnSig};

/// All built-in stdlib module names (hardcoded in the compiler).
pub const STDLIB_MODULES: &[&str] = &["string", "list", "int", "float", "fs", "env", "map", "json", "http", "process", "math", "random", "regex", "io"];

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
        "time" => Some(include_str!("../stdlib/time.almd")),
        "encoding" => Some(include_str!("../stdlib/encoding.almd")),
        "hash" => Some(include_str!("../stdlib/hash.almd")),
        "term" => Some(include_str!("../stdlib/term.almd")),
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
        "trim" | "split" | "pad_left"
        | "starts_with" | "starts_with_hdlm_qm_" | "starts_with?"
        | "ends_with" | "ends_with_hdlm_qm_" | "ends_with?"
        | "slice" | "to_bytes" | "to_upper" | "to_lower"
        | "to_int" | "replace" | "char_at" | "lines"
        | "chars" | "repeat" | "from_bytes"
        | "is_digit?" | "is_digit_hdlm_qm_"
        | "is_alpha?" | "is_alpha_hdlm_qm_"
        | "is_alphanumeric?" | "is_alphanumeric_hdlm_qm_"
        | "is_whitespace?" | "is_whitespace_hdlm_qm_"
        | "pad_right" | "trim_start" | "trim_end"
        | "strip_prefix" | "strip_suffix"
        | "replace_first" | "last_index_of" | "to_float" => vec!["string"],

        // ── list-only ──
        "each" | "fold" | "find" | "any" | "all"
        | "enumerate" | "zip" | "flatten" | "take" | "drop"
        | "sort_by" | "unique"
        | "last" | "chunk" | "sum" | "product"
        | "first" | "flat_map"
        | "filter_map" | "take_while" | "drop_while"
        | "partition" | "reduce" | "group_by" => vec!["list"],

        // ── map-only ──
        "keys" | "values" | "entries" | "merge"
        | "map_values" => vec!["map"],

        // ── int-only ──
        "to_string" | "to_hex" => vec!["int"],

        // ── ambiguous: string + list ──
        "reverse" => vec!["string", "list"],
        "index_of" => vec!["string", "list"],
        "join" => vec!["string", "list"],
        "count" => vec!["string", "list"],

        // ── ambiguous: string + list + map ──
        "len" => vec!["string", "list", "map"],
        "contains" | "contains?" | "contains_hdlm_qm_" => vec!["string", "list", "map"],
        "is_empty?" | "is_empty_hdlm_qm_" => vec!["list", "map"],

        // ── ambiguous: list + map ──
        "get" | "get_or" | "set" => vec!["list", "map"],
        "swap" => vec!["list"],
        "sort" => vec!["list"],
        "map" | "filter" => vec!["list"],

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
        ResolvedType::Int => "int",
        ResolvedType::Float => "float",
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
    vec!["println", "eprintln"]
}

/// Look up a stdlib function's type signature.
pub fn lookup_sig(module: &str, func: &str) -> Option<FnSig> {
    // Try auto-generated definitions first
    if let Some(sig) = crate::generated::stdlib_sigs::lookup_generated_sig(module, func) {
        return Some(sig);
    }

    let s = |n: &str| -> String { n.to_string() };
    let io_err = || Ty::Named(s("IoError"));

    let sig = match (module, func) {
        // ── string ──
        ("string", "trim") => FnSig { generics: vec![], params: vec![(s("s"), Ty::String)], ret: Ty::String, is_effect: false },
        ("string", "split") => FnSig { generics: vec![], params: vec![(s("s"), Ty::String), (s("sep"), Ty::String)], ret: Ty::List(Box::new(Ty::String)), is_effect: false },
        ("string", "join") => FnSig { generics: vec![], params: vec![(s("list"), Ty::List(Box::new(Ty::String))), (s("sep"), Ty::String)], ret: Ty::String, is_effect: false },
        ("string", "len") => FnSig { generics: vec![], params: vec![(s("s"), Ty::String)], ret: Ty::Int, is_effect: false },
        ("string", "pad_left") => FnSig { generics: vec![], params: vec![(s("s"), Ty::String), (s("n"), Ty::Int), (s("ch"), Ty::String)], ret: Ty::String, is_effect: false },
        ("string", "starts_with?") => FnSig { generics: vec![], params: vec![(s("s"), Ty::String), (s("prefix"), Ty::String)], ret: Ty::Bool, is_effect: false },
        ("string", "ends_with?") => FnSig { generics: vec![], params: vec![(s("s"), Ty::String), (s("suffix"), Ty::String)], ret: Ty::Bool, is_effect: false },
        ("string", "contains") | ("string", "contains?") => FnSig { generics: vec![], params: vec![(s("s"), Ty::String), (s("sub"), Ty::String)], ret: Ty::Bool, is_effect: false },
        ("string", "to_upper") => FnSig { generics: vec![], params: vec![(s("s"), Ty::String)], ret: Ty::String, is_effect: false },
        ("string", "to_lower") => FnSig { generics: vec![], params: vec![(s("s"), Ty::String)], ret: Ty::String, is_effect: false },
        ("string", "replace") => FnSig { generics: vec![], params: vec![(s("s"), Ty::String), (s("from"), Ty::String), (s("to"), Ty::String)], ret: Ty::String, is_effect: false },
        ("string", "to_int") => FnSig { generics: vec![], params: vec![(s("s"), Ty::String)], ret: Ty::Result(Box::new(Ty::Int), Box::new(Ty::String)), is_effect: false },
        ("string", "to_bytes") => FnSig { generics: vec![], params: vec![(s("s"), Ty::String)], ret: Ty::List(Box::new(Ty::Int)), is_effect: false },
        ("string", "char_at") => FnSig { generics: vec![], params: vec![(s("s"), Ty::String), (s("i"), Ty::Int)], ret: Ty::Option(Box::new(Ty::String)), is_effect: false },
        ("string", "slice") => FnSig { generics: vec![], params: vec![(s("s"), Ty::String), (s("start"), Ty::Int), (s("end"), Ty::Int)], ret: Ty::String, is_effect: false },
        ("string", "lines") => FnSig { generics: vec![], params: vec![(s("s"), Ty::String)], ret: Ty::List(Box::new(Ty::String)), is_effect: false },
        ("string", "chars") => FnSig { generics: vec![], params: vec![(s("s"), Ty::String)], ret: Ty::List(Box::new(Ty::String)), is_effect: false },
        ("string", "index_of") => FnSig { generics: vec![], params: vec![(s("s"), Ty::String), (s("needle"), Ty::String)], ret: Ty::Option(Box::new(Ty::Int)), is_effect: false },
        ("string", "repeat") => FnSig { generics: vec![], params: vec![(s("s"), Ty::String), (s("n"), Ty::Int)], ret: Ty::String, is_effect: false },
        ("string", "from_bytes") => FnSig { generics: vec![], params: vec![(s("bytes"), Ty::List(Box::new(Ty::Int)))], ret: Ty::String, is_effect: false },
        ("string", "is_digit?") => FnSig { generics: vec![], params: vec![(s("s"), Ty::String)], ret: Ty::Bool, is_effect: false },
        ("string", "is_alpha?") => FnSig { generics: vec![], params: vec![(s("s"), Ty::String)], ret: Ty::Bool, is_effect: false },
        ("string", "is_alphanumeric?") => FnSig { generics: vec![], params: vec![(s("s"), Ty::String)], ret: Ty::Bool, is_effect: false },
        ("string", "is_whitespace?") => FnSig { generics: vec![], params: vec![(s("s"), Ty::String)], ret: Ty::Bool, is_effect: false },

        // ── list ──
        ("list", "len") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown)))], ret: Ty::Int, is_effect: false },
        ("list", "get") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("i"), Ty::Int)], ret: Ty::Option(Box::new(Ty::Unknown)), is_effect: false },
        ("list", "get_or") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("i"), Ty::Int), (s("default"), Ty::Unknown)], ret: Ty::Unknown, is_effect: false },
        ("list", "set") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("i"), Ty::Int), (s("value"), Ty::Unknown)], ret: Ty::List(Box::new(Ty::Unknown)), is_effect: false },
        ("list", "swap") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("i"), Ty::Int), (s("j"), Ty::Int)], ret: Ty::List(Box::new(Ty::Unknown)), is_effect: false },
        ("list", "sort") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown)))], ret: Ty::List(Box::new(Ty::Unknown)), is_effect: false },
        ("list", "reverse") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown)))], ret: Ty::List(Box::new(Ty::Unknown)), is_effect: false },
        ("list", "contains") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("x"), Ty::Unknown)], ret: Ty::Bool, is_effect: false },
        ("list", "each") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("f"), Ty::Fn { params: vec![Ty::Unknown], ret: Box::new(Ty::Unit) })], ret: Ty::Unit, is_effect: false },
        ("list", "map") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("f"), Ty::Fn { params: vec![Ty::Unknown], ret: Box::new(Ty::Unknown) })], ret: Ty::List(Box::new(Ty::Unknown)), is_effect: false },
        ("list", "filter") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("f"), Ty::Fn { params: vec![Ty::Unknown], ret: Box::new(Ty::Bool) })], ret: Ty::List(Box::new(Ty::Unknown)), is_effect: false },
        ("list", "find") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("f"), Ty::Fn { params: vec![Ty::Unknown], ret: Box::new(Ty::Bool) })], ret: Ty::Option(Box::new(Ty::Unknown)), is_effect: false },
        ("list", "fold") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("init"), Ty::Unknown), (s("f"), Ty::Fn { params: vec![Ty::Unknown, Ty::Unknown], ret: Box::new(Ty::Unknown) })], ret: Ty::Unknown, is_effect: false },
        ("list", "any") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("f"), Ty::Fn { params: vec![Ty::Unknown], ret: Box::new(Ty::Bool) })], ret: Ty::Bool, is_effect: false },
        ("list", "all") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("f"), Ty::Fn { params: vec![Ty::Unknown], ret: Box::new(Ty::Bool) })], ret: Ty::Bool, is_effect: false },
        ("list", "enumerate") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown)))], ret: Ty::List(Box::new(Ty::Unknown)), is_effect: false },
        ("list", "zip") => FnSig { generics: vec![], params: vec![(s("a"), Ty::List(Box::new(Ty::Unknown))), (s("b"), Ty::List(Box::new(Ty::Unknown)))], ret: Ty::List(Box::new(Ty::Unknown)), is_effect: false },
        ("list", "flatten") => FnSig { generics: vec![], params: vec![(s("xss"), Ty::List(Box::new(Ty::Unknown)))], ret: Ty::List(Box::new(Ty::Unknown)), is_effect: false },
        ("list", "take") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("n"), Ty::Int)], ret: Ty::List(Box::new(Ty::Unknown)), is_effect: false },
        ("list", "drop") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("n"), Ty::Int)], ret: Ty::List(Box::new(Ty::Unknown)), is_effect: false },
        ("list", "sort_by") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("f"), Ty::Fn { params: vec![Ty::Unknown], ret: Box::new(Ty::Unknown) })], ret: Ty::List(Box::new(Ty::Unknown)), is_effect: false },
        ("list", "unique") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown)))], ret: Ty::List(Box::new(Ty::Unknown)), is_effect: false },
        ("list", "index_of") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("x"), Ty::Unknown)], ret: Ty::Option(Box::new(Ty::Int)), is_effect: false },
        ("list", "last") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown)))], ret: Ty::Option(Box::new(Ty::Unknown)), is_effect: false },
        ("list", "chunk") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("n"), Ty::Int)], ret: Ty::List(Box::new(Ty::List(Box::new(Ty::Unknown)))), is_effect: false },
        ("list", "sum") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Int)))], ret: Ty::Int, is_effect: false },
        ("list", "product") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Int)))], ret: Ty::Int, is_effect: false },
        ("list", "first") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown)))], ret: Ty::Option(Box::new(Ty::Unknown)), is_effect: false },
        ("list", "is_empty?") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown)))], ret: Ty::Bool, is_effect: false },
        ("list", "flat_map") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("f"), Ty::Fn { params: vec![Ty::Unknown], ret: Box::new(Ty::List(Box::new(Ty::Unknown))) })], ret: Ty::List(Box::new(Ty::Unknown)), is_effect: false },
        ("list", "min") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown)))], ret: Ty::Option(Box::new(Ty::Unknown)), is_effect: false },
        ("list", "max") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown)))], ret: Ty::Option(Box::new(Ty::Unknown)), is_effect: false },
        ("list", "join") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::String))), (s("sep"), Ty::String)], ret: Ty::String, is_effect: false },
        ("list", "filter_map") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("f"), Ty::Fn { params: vec![Ty::Unknown], ret: Box::new(Ty::Option(Box::new(Ty::Unknown))) })], ret: Ty::List(Box::new(Ty::Unknown)), is_effect: false },
        ("list", "take_while") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("f"), Ty::Fn { params: vec![Ty::Unknown], ret: Box::new(Ty::Bool) })], ret: Ty::List(Box::new(Ty::Unknown)), is_effect: false },
        ("list", "drop_while") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("f"), Ty::Fn { params: vec![Ty::Unknown], ret: Box::new(Ty::Bool) })], ret: Ty::List(Box::new(Ty::Unknown)), is_effect: false },
        ("list", "count") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("f"), Ty::Fn { params: vec![Ty::Unknown], ret: Box::new(Ty::Bool) })], ret: Ty::Int, is_effect: false },
        ("list", "partition") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("f"), Ty::Fn { params: vec![Ty::Unknown], ret: Box::new(Ty::Bool) })], ret: Ty::Unknown, is_effect: false },
        ("list", "reduce") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("f"), Ty::Fn { params: vec![Ty::Unknown, Ty::Unknown], ret: Box::new(Ty::Unknown) })], ret: Ty::Option(Box::new(Ty::Unknown)), is_effect: false },
        ("list", "group_by") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("f"), Ty::Fn { params: vec![Ty::Unknown], ret: Box::new(Ty::Unknown) })], ret: Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::List(Box::new(Ty::Unknown)))), is_effect: false },

        // ── map ──
        ("map", "new") => FnSig { generics: vec![], params: vec![], ret: Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown)), is_effect: false },
        ("map", "get") => FnSig { generics: vec![], params: vec![(s("m"), Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown))), (s("key"), Ty::Unknown)], ret: Ty::Option(Box::new(Ty::Unknown)), is_effect: false },
        ("map", "get_or") => FnSig { generics: vec![], params: vec![(s("m"), Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown))), (s("key"), Ty::Unknown), (s("default"), Ty::Unknown)], ret: Ty::Unknown, is_effect: false },
        ("map", "set") => FnSig { generics: vec![], params: vec![(s("m"), Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown))), (s("key"), Ty::Unknown), (s("value"), Ty::Unknown)], ret: Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown)), is_effect: false },
        ("map", "contains") => FnSig { generics: vec![], params: vec![(s("m"), Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown))), (s("key"), Ty::Unknown)], ret: Ty::Bool, is_effect: false },
        ("map", "remove") => FnSig { generics: vec![], params: vec![(s("m"), Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown))), (s("key"), Ty::Unknown)], ret: Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown)), is_effect: false },
        ("map", "keys") => FnSig { generics: vec![], params: vec![(s("m"), Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown)))], ret: Ty::List(Box::new(Ty::Unknown)), is_effect: false },
        ("map", "values") => FnSig { generics: vec![], params: vec![(s("m"), Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown)))], ret: Ty::List(Box::new(Ty::Unknown)), is_effect: false },
        ("map", "len") => FnSig { generics: vec![], params: vec![(s("m"), Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown)))], ret: Ty::Int, is_effect: false },
        ("map", "entries") => FnSig { generics: vec![], params: vec![(s("m"), Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown)))], ret: Ty::List(Box::new(Ty::Unknown)), is_effect: false },
        ("map", "from_list") => FnSig { generics: vec![], params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("f"), Ty::Fn { params: vec![Ty::Unknown], ret: Box::new(Ty::Unknown) })], ret: Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown)), is_effect: false },
        ("map", "merge") => FnSig { generics: vec![], params: vec![(s("a"), Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown))), (s("b"), Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown)))], ret: Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown)), is_effect: false },
        ("map", "is_empty?") => FnSig { generics: vec![], params: vec![(s("m"), Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown)))], ret: Ty::Bool, is_effect: false },
        ("map", "map_values") => FnSig { generics: vec![], params: vec![(s("m"), Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown))), (s("f"), Ty::Fn { params: vec![Ty::Unknown], ret: Box::new(Ty::Unknown) })], ret: Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown)), is_effect: false },
        ("map", "filter") => FnSig { generics: vec![], params: vec![(s("m"), Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown))), (s("f"), Ty::Fn { params: vec![Ty::Unknown, Ty::Unknown], ret: Box::new(Ty::Bool) })], ret: Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown)), is_effect: false },
        ("map", "from_entries") => FnSig { generics: vec![], params: vec![(s("entries"), Ty::List(Box::new(Ty::Unknown)))], ret: Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown)), is_effect: false },

        // ── string (additional) ──
        ("string", "pad_right") => FnSig { generics: vec![], params: vec![(s("s"), Ty::String), (s("n"), Ty::Int), (s("ch"), Ty::String)], ret: Ty::String, is_effect: false },
        ("string", "trim_start") => FnSig { generics: vec![], params: vec![(s("s"), Ty::String)], ret: Ty::String, is_effect: false },
        ("string", "trim_end") => FnSig { generics: vec![], params: vec![(s("s"), Ty::String)], ret: Ty::String, is_effect: false },
        ("string", "count") => FnSig { generics: vec![], params: vec![(s("s"), Ty::String), (s("sub"), Ty::String)], ret: Ty::Int, is_effect: false },
        ("string", "is_empty?") => FnSig { generics: vec![], params: vec![(s("s"), Ty::String)], ret: Ty::Bool, is_effect: false },
        ("string", "reverse") => FnSig { generics: vec![], params: vec![(s("s"), Ty::String)], ret: Ty::String, is_effect: false },
        ("string", "strip_prefix") => FnSig { generics: vec![], params: vec![(s("s"), Ty::String), (s("prefix"), Ty::String)], ret: Ty::Option(Box::new(Ty::String)), is_effect: false },
        ("string", "strip_suffix") => FnSig { generics: vec![], params: vec![(s("s"), Ty::String), (s("suffix"), Ty::String)], ret: Ty::Option(Box::new(Ty::String)), is_effect: false },
        ("string", "replace_first") => FnSig { generics: vec![], params: vec![(s("s"), Ty::String), (s("from"), Ty::String), (s("to"), Ty::String)], ret: Ty::String, is_effect: false },
        ("string", "last_index_of") => FnSig { generics: vec![], params: vec![(s("s"), Ty::String), (s("needle"), Ty::String)], ret: Ty::Option(Box::new(Ty::Int)), is_effect: false },
        ("string", "to_float") => FnSig { generics: vec![], params: vec![(s("s"), Ty::String)], ret: Ty::Result(Box::new(Ty::Float), Box::new(Ty::String)), is_effect: false },

        // ── int ── (auto-generated from stdlib/defs/int.toml)
        // ── float ── (auto-generated from stdlib/defs/float.toml)

        // ── json ── (auto-generated from stdlib/defs/json.toml)


        // ── env ── (auto-generated from stdlib/defs/env.toml)

        // ── process ── (auto-generated from stdlib/defs/process.toml)
        // ── fs ── (auto-generated from stdlib/defs/fs.toml)

        // ── math ── (auto-generated from stdlib/defs/math.toml)

        // ── random ── (auto-generated from stdlib/defs/random.toml)

        // ── time: fully migrated to stdlib/time.almd ──

        // ── io ── (auto-generated from stdlib/defs/io.toml)
        // ── regex ── (auto-generated from stdlib/defs/regex.toml)

        _ => return None,
    };
    Some(sig)
}
