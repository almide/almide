/// Stdlib module registry: module names, UFCS resolution, bundled module list.
///
/// Pure data with no dependencies — shared by checker (main crate) and codegen.

/// All built-in stdlib module names (hardcoded in the compiler).
pub const STDLIB_MODULES: &[&str] = &[
    "string", "list", "int", "float", "bytes", "matrix", "fs", "env", "map",
    "json", "http", "process", "math", "random", "regex", "io", "result",
    "option", "error", "datetime", "testing", "value", "set",
    "base64", "hex",
];

/// Bundled stdlib modules written in Almide (.almd files embedded in the compiler binary).
pub const BUNDLED_MODULES: &[&str] = &["args", "path", "option", "result"];

/// Bundled modules that should be auto-imported (Tier 1 behavior).
pub const AUTO_IMPORT_BUNDLED: &[&str] = &["option", "result"];

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

        _ => vec![],
    }
}
