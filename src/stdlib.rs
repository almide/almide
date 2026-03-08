/// Centralized stdlib definitions for the Almide compiler.
/// Both the type checker (check.rs) and code generator (emit_rust.rs) reference this module
/// to avoid duplicating function signatures, module lists, and UFCS mappings.

use crate::types::{Ty, FnSig};

/// All built-in stdlib module names.
pub const STDLIB_MODULES: &[&str] = &["string", "list", "int", "float", "fs", "env", "map", "json", "path"];

/// Check if a module name is a stdlib module.
pub fn is_stdlib_module(name: &str) -> bool {
    STDLIB_MODULES.contains(&name)
}

/// Resolve a method name to its stdlib module (for UFCS / dot syntax).
/// e.g. `x.trim()` → `string.trim(x)`
pub fn resolve_ufcs_module(method: &str) -> Option<&'static str> {
    match method {
        "trim" | "split" | "join" | "pad_left"
        | "starts_with" | "starts_with_qm_" | "starts_with?"
        | "ends_with" | "ends_with_qm_" | "ends_with?"
        | "slice" | "to_bytes" | "contains" | "to_upper" | "to_lower"
        | "to_int" | "replace" | "char_at" | "lines" => Some("string"),

        "get" | "get_or" | "sort" | "reverse"
        | "each" | "map" | "filter" | "find" | "fold"
        | "any" | "all" | "len" => Some("list"),

        "to_string" | "to_hex" => Some("int"),

        "keys" | "values" | "entries" => Some("map"),

        _ => None,
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
    let s = |n: &str| -> String { n.to_string() };
    let io_err = || Ty::Named(s("IoError"));

    let sig = match (module, func) {
        // ── string ──
        ("string", "trim") => FnSig { params: vec![(s("s"), Ty::String)], ret: Ty::String, is_effect: false },
        ("string", "split") => FnSig { params: vec![(s("s"), Ty::String), (s("sep"), Ty::String)], ret: Ty::List(Box::new(Ty::String)), is_effect: false },
        ("string", "join") => FnSig { params: vec![(s("list"), Ty::List(Box::new(Ty::String))), (s("sep"), Ty::String)], ret: Ty::String, is_effect: false },
        ("string", "len") => FnSig { params: vec![(s("s"), Ty::String)], ret: Ty::Int, is_effect: false },
        ("string", "pad_left") => FnSig { params: vec![(s("s"), Ty::String), (s("n"), Ty::Int), (s("ch"), Ty::String)], ret: Ty::String, is_effect: false },
        ("string", "starts_with?") => FnSig { params: vec![(s("s"), Ty::String), (s("prefix"), Ty::String)], ret: Ty::Bool, is_effect: false },
        ("string", "ends_with?") => FnSig { params: vec![(s("s"), Ty::String), (s("suffix"), Ty::String)], ret: Ty::Bool, is_effect: false },
        ("string", "contains") | ("string", "contains?") => FnSig { params: vec![(s("s"), Ty::String), (s("sub"), Ty::String)], ret: Ty::Bool, is_effect: false },
        ("string", "to_upper") => FnSig { params: vec![(s("s"), Ty::String)], ret: Ty::String, is_effect: false },
        ("string", "to_lower") => FnSig { params: vec![(s("s"), Ty::String)], ret: Ty::String, is_effect: false },
        ("string", "replace") => FnSig { params: vec![(s("s"), Ty::String), (s("from"), Ty::String), (s("to"), Ty::String)], ret: Ty::String, is_effect: false },
        ("string", "to_int") => FnSig { params: vec![(s("s"), Ty::String)], ret: Ty::Result(Box::new(Ty::Int), Box::new(Ty::String)), is_effect: false },
        ("string", "to_bytes") => FnSig { params: vec![(s("s"), Ty::String)], ret: Ty::List(Box::new(Ty::Int)), is_effect: false },
        ("string", "char_at") => FnSig { params: vec![(s("s"), Ty::String), (s("i"), Ty::Int)], ret: Ty::Option(Box::new(Ty::String)), is_effect: false },
        ("string", "slice") => FnSig { params: vec![(s("s"), Ty::String), (s("start"), Ty::Int), (s("end"), Ty::Int)], ret: Ty::String, is_effect: false },
        ("string", "lines") => FnSig { params: vec![(s("s"), Ty::String)], ret: Ty::List(Box::new(Ty::String)), is_effect: false },

        // ── list ──
        ("list", "len") => FnSig { params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown)))], ret: Ty::Int, is_effect: false },
        ("list", "get") => FnSig { params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("i"), Ty::Int)], ret: Ty::Option(Box::new(Ty::Unknown)), is_effect: false },
        ("list", "get_or") => FnSig { params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("i"), Ty::Int), (s("default"), Ty::Unknown)], ret: Ty::Unknown, is_effect: false },
        ("list", "sort") => FnSig { params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown)))], ret: Ty::List(Box::new(Ty::Unknown)), is_effect: false },
        ("list", "reverse") => FnSig { params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown)))], ret: Ty::List(Box::new(Ty::Unknown)), is_effect: false },
        ("list", "contains") => FnSig { params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("x"), Ty::Unknown)], ret: Ty::Bool, is_effect: false },
        ("list", "each") => FnSig { params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("f"), Ty::Fn { params: vec![Ty::Unknown], ret: Box::new(Ty::Unit) })], ret: Ty::Unit, is_effect: false },
        ("list", "map") => FnSig { params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("f"), Ty::Fn { params: vec![Ty::Unknown], ret: Box::new(Ty::Unknown) })], ret: Ty::List(Box::new(Ty::Unknown)), is_effect: false },
        ("list", "filter") => FnSig { params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("f"), Ty::Fn { params: vec![Ty::Unknown], ret: Box::new(Ty::Bool) })], ret: Ty::List(Box::new(Ty::Unknown)), is_effect: false },
        ("list", "find") => FnSig { params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("f"), Ty::Fn { params: vec![Ty::Unknown], ret: Box::new(Ty::Bool) })], ret: Ty::Option(Box::new(Ty::Unknown)), is_effect: false },
        ("list", "fold") => FnSig { params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("init"), Ty::Unknown), (s("f"), Ty::Fn { params: vec![Ty::Unknown, Ty::Unknown], ret: Box::new(Ty::Unknown) })], ret: Ty::Unknown, is_effect: false },
        ("list", "any") => FnSig { params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("f"), Ty::Fn { params: vec![Ty::Unknown], ret: Box::new(Ty::Bool) })], ret: Ty::Bool, is_effect: false },
        ("list", "all") => FnSig { params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("f"), Ty::Fn { params: vec![Ty::Unknown], ret: Box::new(Ty::Bool) })], ret: Ty::Bool, is_effect: false },

        // ── map ──
        ("map", "new") => FnSig { params: vec![], ret: Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown)), is_effect: false },
        ("map", "get") => FnSig { params: vec![(s("m"), Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown))), (s("key"), Ty::Unknown)], ret: Ty::Option(Box::new(Ty::Unknown)), is_effect: false },
        ("map", "get_or") => FnSig { params: vec![(s("m"), Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown))), (s("key"), Ty::Unknown), (s("default"), Ty::Unknown)], ret: Ty::Unknown, is_effect: false },
        ("map", "set") => FnSig { params: vec![(s("m"), Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown))), (s("key"), Ty::Unknown), (s("value"), Ty::Unknown)], ret: Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown)), is_effect: false },
        ("map", "contains") => FnSig { params: vec![(s("m"), Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown))), (s("key"), Ty::Unknown)], ret: Ty::Bool, is_effect: false },
        ("map", "remove") => FnSig { params: vec![(s("m"), Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown))), (s("key"), Ty::Unknown)], ret: Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown)), is_effect: false },
        ("map", "keys") => FnSig { params: vec![(s("m"), Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown)))], ret: Ty::List(Box::new(Ty::Unknown)), is_effect: false },
        ("map", "values") => FnSig { params: vec![(s("m"), Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown)))], ret: Ty::List(Box::new(Ty::Unknown)), is_effect: false },
        ("map", "len") => FnSig { params: vec![(s("m"), Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown)))], ret: Ty::Int, is_effect: false },
        ("map", "entries") => FnSig { params: vec![(s("m"), Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown)))], ret: Ty::List(Box::new(Ty::Unknown)), is_effect: false },
        ("map", "from_list") => FnSig { params: vec![(s("xs"), Ty::List(Box::new(Ty::Unknown))), (s("f"), Ty::Fn { params: vec![Ty::Unknown], ret: Box::new(Ty::Unknown) })], ret: Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown)), is_effect: false },

        // ── int ──
        ("int", "to_string") => FnSig { params: vec![(s("n"), Ty::Int)], ret: Ty::String, is_effect: false },
        ("int", "to_hex") => FnSig { params: vec![(s("n"), Ty::Int)], ret: Ty::String, is_effect: false },

        // ── float ──
        ("float", "to_string") => FnSig { params: vec![(s("n"), Ty::Float)], ret: Ty::String, is_effect: false },
        ("float", "to_int") => FnSig { params: vec![(s("n"), Ty::Float)], ret: Ty::Int, is_effect: false },
        ("float", "round") => FnSig { params: vec![(s("n"), Ty::Float)], ret: Ty::Float, is_effect: false },
        ("float", "floor") => FnSig { params: vec![(s("n"), Ty::Float)], ret: Ty::Float, is_effect: false },
        ("float", "ceil") => FnSig { params: vec![(s("n"), Ty::Float)], ret: Ty::Float, is_effect: false },
        ("float", "abs") => FnSig { params: vec![(s("n"), Ty::Float)], ret: Ty::Float, is_effect: false },
        ("float", "sqrt") => FnSig { params: vec![(s("n"), Ty::Float)], ret: Ty::Float, is_effect: false },
        ("float", "parse") => FnSig { params: vec![(s("s"), Ty::String)], ret: Ty::Result(Box::new(Ty::Float), Box::new(Ty::String)), is_effect: false },

        // ── json ──
        ("json", "parse") => FnSig { params: vec![(s("text"), Ty::String)], ret: Ty::Result(Box::new(Ty::Named(s("Json"))), Box::new(Ty::String)), is_effect: false },
        ("json", "stringify") => FnSig { params: vec![(s("j"), Ty::Named(s("Json")))], ret: Ty::String, is_effect: false },
        ("json", "get") => FnSig { params: vec![(s("j"), Ty::Named(s("Json"))), (s("key"), Ty::String)], ret: Ty::Option(Box::new(Ty::Named(s("Json")))), is_effect: false },
        ("json", "get_string") => FnSig { params: vec![(s("j"), Ty::Named(s("Json"))), (s("key"), Ty::String)], ret: Ty::Option(Box::new(Ty::String)), is_effect: false },
        ("json", "get_int") => FnSig { params: vec![(s("j"), Ty::Named(s("Json"))), (s("key"), Ty::String)], ret: Ty::Option(Box::new(Ty::Int)), is_effect: false },
        ("json", "get_bool") => FnSig { params: vec![(s("j"), Ty::Named(s("Json"))), (s("key"), Ty::String)], ret: Ty::Option(Box::new(Ty::Bool)), is_effect: false },
        ("json", "get_array") => FnSig { params: vec![(s("j"), Ty::Named(s("Json"))), (s("key"), Ty::String)], ret: Ty::Option(Box::new(Ty::List(Box::new(Ty::Named(s("Json")))))), is_effect: false },
        ("json", "keys") => FnSig { params: vec![(s("j"), Ty::Named(s("Json")))], ret: Ty::List(Box::new(Ty::String)), is_effect: false },
        ("json", "to_string") => FnSig { params: vec![(s("j"), Ty::Named(s("Json")))], ret: Ty::Option(Box::new(Ty::String)), is_effect: false },
        ("json", "to_int") => FnSig { params: vec![(s("j"), Ty::Named(s("Json")))], ret: Ty::Option(Box::new(Ty::Int)), is_effect: false },
        ("json", "from_string") => FnSig { params: vec![(s("s"), Ty::String)], ret: Ty::Named(s("Json")), is_effect: false },
        ("json", "from_int") => FnSig { params: vec![(s("n"), Ty::Int)], ret: Ty::Named(s("Json")), is_effect: false },
        ("json", "from_bool") => FnSig { params: vec![(s("b"), Ty::Bool)], ret: Ty::Named(s("Json")), is_effect: false },
        ("json", "null") => FnSig { params: vec![], ret: Ty::Named(s("Json")), is_effect: false },
        ("json", "array") => FnSig { params: vec![(s("items"), Ty::List(Box::new(Ty::Named(s("Json")))))], ret: Ty::Named(s("Json")), is_effect: false },
        ("json", "from_map") => FnSig { params: vec![(s("m"), Ty::Map(Box::new(Ty::String), Box::new(Ty::Named(s("Json")))))], ret: Ty::Named(s("Json")), is_effect: false },

        // ── path ──
        ("path", "join") => FnSig { params: vec![(s("base"), Ty::String), (s("child"), Ty::String)], ret: Ty::String, is_effect: false },
        ("path", "dirname") => FnSig { params: vec![(s("p"), Ty::String)], ret: Ty::String, is_effect: false },
        ("path", "basename") => FnSig { params: vec![(s("p"), Ty::String)], ret: Ty::String, is_effect: false },
        ("path", "extension") => FnSig { params: vec![(s("p"), Ty::String)], ret: Ty::Option(Box::new(Ty::String)), is_effect: false },
        ("path", "is_absolute?") => FnSig { params: vec![(s("p"), Ty::String)], ret: Ty::Bool, is_effect: false },

        // ── env ──
        ("env", "unix_timestamp") => FnSig { params: vec![], ret: Ty::Int, is_effect: true },
        ("env", "args") => FnSig { params: vec![], ret: Ty::List(Box::new(Ty::String)), is_effect: true },

        // ── fs ──
        ("fs", "read_text") => FnSig { params: vec![(s("path"), Ty::String)], ret: Ty::Result(Box::new(Ty::String), Box::new(io_err())), is_effect: true },
        ("fs", "read_bytes") => FnSig { params: vec![(s("path"), Ty::String)], ret: Ty::Result(Box::new(Ty::List(Box::new(Ty::Int))), Box::new(io_err())), is_effect: true },
        ("fs", "write") => FnSig { params: vec![(s("path"), Ty::String), (s("content"), Ty::String)], ret: Ty::Result(Box::new(Ty::Unit), Box::new(io_err())), is_effect: true },
        ("fs", "write_bytes") => FnSig { params: vec![(s("path"), Ty::String), (s("bytes"), Ty::List(Box::new(Ty::Int)))], ret: Ty::Result(Box::new(Ty::Unit), Box::new(io_err())), is_effect: true },
        ("fs", "append") => FnSig { params: vec![(s("path"), Ty::String), (s("content"), Ty::String)], ret: Ty::Result(Box::new(Ty::Unit), Box::new(io_err())), is_effect: true },
        ("fs", "mkdir_p") => FnSig { params: vec![(s("path"), Ty::String)], ret: Ty::Result(Box::new(Ty::Unit), Box::new(io_err())), is_effect: true },
        ("fs", "exists?") => FnSig { params: vec![(s("path"), Ty::String)], ret: Ty::Bool, is_effect: true },
        ("fs", "read_lines") => FnSig { params: vec![(s("path"), Ty::String)], ret: Ty::Result(Box::new(Ty::List(Box::new(Ty::String))), Box::new(io_err())), is_effect: true },
        ("fs", "remove") => FnSig { params: vec![(s("path"), Ty::String)], ret: Ty::Result(Box::new(Ty::Unit), Box::new(io_err())), is_effect: true },
        ("fs", "list_dir") => FnSig { params: vec![(s("path"), Ty::String)], ret: Ty::Result(Box::new(Ty::List(Box::new(Ty::String))), Box::new(io_err())), is_effect: true },

        _ => return None,
    };
    Some(sig)
}
