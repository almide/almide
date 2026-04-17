/// Stdlib module registry: module names, UFCS resolution, bundled module list.
///
/// Pure data with no dependencies — shared by checker (main crate) and codegen.

/// All built-in stdlib module names (hardcoded in the compiler).
pub const STDLIB_MODULES: &[&str] = &[
    "string", "list", "int", "float", "bytes", "matrix", "fs", "env", "map",
    "json", "http", "process", "math", "random", "regex", "io", "result",
    "option", "error", "datetime", "testing", "value", "set",
    "base64", "hex",
    // Sized numeric types (Stage 3 of the sized-numeric-types arc).
    // Each hosts UFCS conversion methods (`.to_int64()`,
    // `.to_float32()`, ...). Auto-imported alongside `int` / `float`
    // so users never need `import int32`.
    "int8", "int16", "int32",
    "uint8", "uint16", "uint32", "uint64",
    "float32",
];

/// Bundled stdlib modules written in Almide (.almd files embedded in the compiler binary).
pub const BUNDLED_MODULES: &[&str] = &[
    "args", "path", "list", "int", "base64", "hex", "float", "bytes",
    "int8", "int16", "int32",
    "uint8", "uint16", "uint32", "uint64",
    "float32",
];

/// Bundled modules that should be auto-imported (Tier 1 behavior).
/// Tier-1 stdlib modules with no bundled-Almide content (option, result, etc.)
/// are auto-imported via the hardcoded list in
/// `almide-frontend::import_table::ImportTable::new`; this list is for
/// bundled `.almd` modules that need resolve-time loading.
pub const AUTO_IMPORT_BUNDLED: &[&str] = &[
    "list", "int", "float",
    "int8", "int16", "int32",
    "uint8", "uint16", "uint32", "uint64",
    "float32",
];

/// Check if a module name is a hardcoded stdlib module.
pub fn is_stdlib_module(name: &str) -> bool {
    STDLIB_MODULES.contains(&name)
}

/// Check if a module name is a bundled .almd module.
pub fn is_bundled_module(name: &str) -> bool {
    BUNDLED_MODULES.contains(&name)
}

/// Check if a module name is any kind of stdlib (hardcoded or bundled).
pub fn is_any_stdlib(name: &str) -> bool {
    is_stdlib_module(name) || is_bundled_module(name)
}

/// Return the embedded source text of a bundled stdlib module.
///
/// The source strings live here (not in `almide-frontend::stdlib`) so
/// that every consumer — type checker, codegen passes, tooling — can
/// reach them without gaining a dep on the frontend crate.
///
/// **TODO (Stdlib Declarative Unification follow-up)**: multiple
/// downstream consumers currently parse the returned source and
/// maintain their own cached derived views (FnSig in
/// `almide-frontend::bundled_sigs`, `@inline_rust` templates in
/// `almide-codegen::pass_stdlib_lowering`). The intended end state is
/// one shared cache that feeds both — likely realised as "bundled
/// modules are always lowered to IR during the preamble, even in unit
/// tests that bypass `resolve.rs`", so the IR becomes the single
/// source of parsed metadata. Until that refactor lands, treat the
/// duplicate parses as a knowingly-temporary cost.
pub fn bundled_source(name: &str) -> Option<&'static str> {
    match name {
        "args" => Some(include_str!("../../../stdlib/args.almd")),
        "path" => Some(include_str!("../../../stdlib/path.almd")),
        "list" => Some(include_str!("../../../stdlib/list.almd")),
        "int" => Some(include_str!("../../../stdlib/int.almd")),
        "base64" => Some(include_str!("../../../stdlib/base64.almd")),
        "hex" => Some(include_str!("../../../stdlib/hex.almd")),
        "float" => Some(include_str!("../../../stdlib/float.almd")),
        "bytes" => Some(include_str!("../../../stdlib/bytes.almd")),
        "int8" => Some(include_str!("../../../stdlib/int8.almd")),
        "int16" => Some(include_str!("../../../stdlib/int16.almd")),
        "int32" => Some(include_str!("../../../stdlib/int32.almd")),
        "uint8" => Some(include_str!("../../../stdlib/uint8.almd")),
        "uint16" => Some(include_str!("../../../stdlib/uint16.almd")),
        "uint32" => Some(include_str!("../../../stdlib/uint32.almd")),
        "uint64" => Some(include_str!("../../../stdlib/uint64.almd")),
        "float32" => Some(include_str!("../../../stdlib/float32.almd")),
        _ => None,
    }
}

/// Resolve a method name to its stdlib module (for UFCS / dot syntax).
/// For ambiguous methods, returns the first candidate as default.
pub fn resolve_ufcs_module(method: &str) -> Option<&'static str> {
    let candidates = resolve_ufcs_candidates(method);
    candidates.first().copied()
}

/// Return all stdlib modules that contain a given method name.
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
        "fold" | "any" | "all" => vec!["list", "map", "set"],

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

        // ── ambiguous: list + map + set ──
        "insert" | "clear" => vec!["list", "map", "set"],
        "remove" => vec!["map", "set"],

        // ── ambiguous: string + list + map ──
        "get" | "get_or" => vec!["string", "list", "map"],
        "set" => vec!["list", "map"],

        // ── ambiguous: list ──
        "swap" | "sort" | "min" | "max" => vec!["list"],

        // ── ambiguous: int + float ──
        "abs" | "clamp" => vec!["int", "float"],
        "to_float" => vec!["string", "int"],

        // ── ambiguous: math + float ──
        "sign" => vec!["math", "float"],

        // ── sized numeric conversion methods (Stage 3) ──
        // Every sized int / float provides these UFCS methods. The
        // concrete module (int32, uint8, float32, ...) is picked by
        // the receiver's type at codegen (`resolve_module_from_ty`).
        "to_int8" | "to_int16" | "to_int32" | "to_int64"
        | "to_uint8" | "to_uint16" | "to_uint32" | "to_uint64"
        | "to_float32" | "to_float64" => vec![
            "int", "float",
            "int8", "int16", "int32",
            "uint8", "uint16", "uint32", "uint64",
            "float32",
        ],

        _ => vec![],
    }
}
