/// Centralized stdlib definitions for the Almide compiler.

use crate::types::FnSig;

// Re-export from almide-lang for backwards compatibility.
pub use almide_lang::stdlib_info::{
    bundled_source as get_bundled_source,
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

/// One-line description of each stdlib module, for error hints.
///
/// Covers every entry in `BUNDLED_MODULES` — `stdlib_module_descriptions_are_complete`
/// in this file's tests fails if a module is added without one, because the
/// fallback ("standard library module") makes the hint say nothing and a silent
/// fallback is indistinguishable from a deliberate omission.
const MODULE_DESCRIPTIONS: &[(&str, &str)] = &[
    ("string", "string manipulation"),
    ("list", "list operations"),
    ("int", "integer utilities"),
    ("float", "floating-point utilities"),
    ("bytes", "byte buffer operations"),
    ("matrix", "matrix operations"),
    ("fs", "file system operations"),
    ("env", "environment variables"),
    ("map", "hash map operations"),
    ("json", "JSON parsing and querying"),
    ("http", "HTTP client"),
    ("process", "process execution"),
    ("math", "mathematical functions"),
    ("random", "random number generation"),
    ("regex", "regular expressions"),
    ("io", "input/output"),
    ("result", "Result type utilities"),
    ("option", "Option type utilities"),
    ("error", "error handling"),
    ("datetime", "date and time operations"),
    ("testing", "test assertion utilities"),
    ("value", "dynamic value operations"),
    ("set", "hash set operations"),
    ("args", "command-line arguments"),
    ("path", "file path manipulation"),
    ("base64", "Base64 encoding and decoding"),
    ("hex", "hexadecimal encoding and decoding"),
    ("hash", "non-cryptographic digests and SHA-256"),
    ("html", "HTML escaping and construction"),
    ("mem", "raw memory checkpoints"),
    ("net", "TCP sockets"),
    ("url", "URL parsing, building, and percent-encoding (RFC 3986-lite)"),
    ("zlib", "zlib compression and decompression"),
    ("prim", "primitive memory and file-descriptor operations"),
    ("int8", "8-bit signed integer conversions"),
    ("int16", "16-bit signed integer conversions"),
    ("int32", "32-bit signed integer conversions"),
    ("int64", "64-bit signed integer conversions"),
    ("uint8", "8-bit unsigned integer conversions"),
    ("uint16", "16-bit unsigned integer conversions"),
    ("uint32", "32-bit unsigned integer conversions"),
    ("uint64", "64-bit unsigned integer conversions"),
    ("float32", "32-bit floating-point conversions"),
    ("float64", "64-bit floating-point conversions"),
];

/// Short description of a stdlib module (for error hints).
pub fn module_description(name: &str) -> &'static str {
    MODULE_DESCRIPTIONS
        .iter()
        .find(|(m, _)| *m == name)
        .map(|(_, d)| *d)
        .unwrap_or("standard library module")
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

/// Every known hallucinated `(module, function)` and what to write instead.
///
/// This is a lookup table, so it is written as data rather than as branches.
/// The previous `match` form encoded the same rows as ~40 arms, several of which
/// had to re-`match module` inside the arm to pick which module's `len` to name
/// — a shape that grows a branch per row and hid the fact that the whole thing
/// is one map. Rows are the unit of maintenance here: this table's job is to
/// cover what LLMs actually write, so it is appended to often, and appending a
/// row must not mean reasoning about control flow.
///
/// Suggestions are not always bare `module.fn` names: where no single function
/// is the answer, the value is the shape to write (`"[] (empty list literal)"`).
const ALIASES: &[(&str, &str, &str)] = &[
    // size / count / length → len
    ("set", "size", "set.len"),
    ("list", "size", "list.len"),
    ("map", "size", "map.len"),
    ("string", "size", "string.len"),
    ("set", "count", "set.len"),
    ("list", "count", "list.len"),
    ("map", "count", "map.len"),
    ("string", "length", "string.len"),
    ("list", "length", "list.len"),
    ("map", "length", "map.len"),
    ("set", "length", "set.len"),
    // skip → drop
    ("list", "skip", "list.drop"),
    // parsing lives on the target type's module, not on `string`
    ("string", "to_int", "int.parse"),
    ("string", "to_integer", "int.parse"),
    ("string", "parse_int", "int.parse"),
    ("string", "to_float", "float.parse"),
    ("string", "parse_float", "float.parse"),
    ("int", "from_string", "int.parse"),
    ("int", "from_str", "int.parse"),
    ("float", "from_string", "float.parse"),
    ("float", "from_str", "float.parse"),
    // char code
    ("string", "char_code", "string.codepoint"),
    ("string", "char_code_at", "string.codepoint"),
    ("string", "code_at", "string.codepoint"),
    ("string", "char_at_code", "string.codepoint"),
    ("string", "ord", "string.codepoint"),
    // case conversion
    ("string", "to_lowercase", "string.to_lower"),
    ("string", "lowercase", "string.to_lower"),
    ("string", "lower", "string.to_lower"),
    ("string", "to_uppercase", "string.to_upper"),
    ("string", "uppercase", "string.to_upper"),
    ("string", "upper", "string.to_upper"),
    // substring
    ("string", "substring", "string.slice"),
    ("string", "substr", "string.slice"),
    // list operations
    ("list", "push", "list.concat (use [xs, [x]] or xs + [x])"),
    ("list", "append", "list.concat (use [xs, [x]] or xs + [x])"),
    ("list", "has", "list.contains"),
    ("list", "includes", "list.contains"),
    ("list", "find_index", "list.index_of"),
    // string membership
    ("string", "includes", "string.contains"),
    ("string", "has", "string.contains"),
    ("string", "index", "string.index_of"),
    ("string", "all", "string.chars + list.all"),
    // Common LLM hallucinations from MSR testing
    ("string", "get_char", "string.char_at"),
    ("string", "charAt", "string.char_at"),
    ("string", "get", "string.char_at"),
    ("string", "from_char", "string.from_codepoint"),
    ("string", "from_char_code", "string.from_codepoint"),
    ("string", "chr", "string.from_codepoint"),
    ("list", "foldLeft", "list.fold"),
    ("list", "foldRight", "list.fold"),
    ("list", "reduce", "list.fold"),
    ("list", "foldl", "list.fold"),
    ("list", "foldr", "list.fold"),
    ("list", "empty", "[] (empty list literal)"),
    ("list", "new", "[] (empty list literal)"),
    ("list", "head", "list.first"),
    ("list", "tail", "list.drop(xs, 1)"),
    ("map", "new", "[:] (empty map literal)"),
    ("map", "empty", "[:] (empty map literal)"),
    ("map", "has_key", "map.contains"),
    ("map", "has", "map.contains"),
    ("map", "includes", "map.contains"),
    // Almide has only `float.sqrt`. Most LLMs reach for `int.sqrt(n)` in
    // is-prime / perfect-square style tasks.
    ("int", "sqrt", "float.sqrt(int.to_float(n))"),
];

/// Suggest the correct stdlib function for a commonly hallucinated name.
/// Returns `Some("module.function")` if a known alias exists.
pub fn suggest_alias(module: &str, func: &str) -> Option<&'static str> {
    if let Some((_, _, fix)) = ALIASES.iter().find(|(m, f, _)| *m == module && *f == func) {
        return Some(fix);
    }
    // Comparison functions derive from the single canonical operator table so
    // `suggest_alias`, `try_snippet_for_alias`, and `almide fix`'s
    // Call-to-Binary rewrite all agree on the shape.
    match comparison_operator_of(module, func)? {
        ">" => Some("a > b (operator)"),
        "<" => Some("a < b (operator)"),
        ">=" => Some("a >= b (operator)"),
        "<=" => Some("a <= b (operator)"),
        "==" => Some("a == b (operator)"),
        "!=" => Some("a != b (operator)"),
        _ => None,
    }
}

/// Canonical mapping: `<module>.<func>` → operator string, for LLM
/// hallucinations like `int.gt(a, b)` that should be `a > b`. This is the
/// single source of truth — `suggest_alias`, `try_snippet_for_alias`, and
/// `almide fix`'s Call-to-Binary rewrite all derive from here.
///
/// `==` and `!=` apply to any type in Almide (structural equality), so
/// we cover int/float/string/bool for those; ordering ops are numeric
/// only (int/float).
pub fn comparison_operator_of(module: &str, func: &str) -> Option<&'static str> {
    match (module, func) {
        ("int" | "float", "gt") => Some(">"),
        ("int" | "float", "lt") => Some("<"),
        ("int" | "float", "gte" | "ge") => Some(">="),
        ("int" | "float", "lte" | "le") => Some("<="),
        ("int" | "float" | "string" | "bool", "eq") => Some("=="),
        ("int" | "float" | "string" | "bool", "neq" | "ne") => Some("!="),
        _ => None,
    }
}

/// Rich multi-line `try:` snippet for well-known LLM hallucinations that
/// don't map to a single clean function name (conversion-wrappers,
/// operator forms). `suggest_alias` returns free-text for these cases
/// (suppressing the default "fn(...)" try: snippet); this table provides
/// a concrete fix template instead.
pub fn try_snippet_for_alias(module: &str, func: &str) -> Option<&'static str> {
    if (module, func) == ("int", "sqrt") {
        return Some(
            "// Almide has float.sqrt; int.sqrt doesn't exist.\n\
             // Convert → sqrt → (optionally) convert back:\n\
             let root_f = float.sqrt(int.to_float(n))       // Float\n\
             let root_i = float.to_int(root_f)              // Int (truncates)\n\
             // — or inline: float.to_int(float.sqrt(int.to_float(n)))"
        );
    }
    if comparison_operator_of(module, func).is_some() {
        return Some(
            "// Almide uses operators, not comparison functions:\n\
             //   int.gt(a, b)   →  a > b\n\
             //   int.lt(a, b)   →  a < b\n\
             //   int.gte(a, b)  →  a >= b\n\
             //   int.lte(a, b)  →  a <= b\n\
             //   int.eq(a, b)   →  a == b\n\
             //   int.neq(a, b)  →  a != b\n\
             // (same for float, string, bool — == and != work on any type)"
        );
    }
    None
}

/// Names of built-in effect functions (not module-scoped).
pub fn builtin_effect_fns() -> Vec<&'static str> {
    vec!["println", "eprintln", "panic"]
}

/// Return the TOML-declared fn names for a stdlib module.
///
/// This is the **dispatch** list: fns that still live in
/// `stdlib/defs/<m>.toml` and therefore feed `arg_transforms` /
/// `rt_<m>_<f>` codegen. The main-crate prune logic uses it to
/// decide which bundled fns to drop (bundled fns whose name
/// collides with TOML are dropped unless they override with
/// `@inline_rust` / `@wasm_intrinsic`).
///
/// Reflection paths (outline, docs-gen) that want the complete
/// user-visible surface should call this fn. Post Stdlib Declarative
/// Unification, every stdlib module lives in `stdlib/<m>.almd`, so
/// the TOML-generated table is no longer a source — all names flow
/// through `bundled_sigs`.
pub fn module_functions(module: &str) -> Vec<&'static str> {
    crate::bundled_sigs::module_fn_names(module)
}

/// Kept as an alias so callers that still reach for "the union of
/// TOML + bundled names" stay compiling. With TOML gone, the union is
/// just the bundled set.
pub fn module_functions_all(module: &str) -> Vec<&'static str> {
    module_functions(module)
}

/// Look up a stdlib function's type signature. Since the Stdlib
/// Declarative Unification arc landed, every stdlib module is
/// `@inline_rust`-bundled `.almd`, so the lookup delegates straight
/// to `bundled_sigs` — the generated TOML table is no longer in play.
pub fn lookup_sig(module: &str, func: &str) -> Option<FnSig> {
    lookup_bundled_sig(module, func)
}

/// Bundled-source signature lookup. Delegates to the caching layer in
/// `bundled_sigs` (per-module parse, process-wide cache) so that
/// migrating a stdlib fn to `stdlib/<m>.almd` keeps the type checker
/// informed without any TOML bridge.
fn lookup_bundled_sig(module: &str, func: &str) -> Option<FnSig> {
    crate::bundled_sigs::lookup(module, func)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A duplicate key in `ALIASES` makes the later row unreachable. The `match`
    /// this table replaced would not have caught it either — Rust does not warn
    /// on duplicate string-literal tuple patterns — so the check is explicit.
    #[test]
    fn alias_table_has_no_duplicate_keys() {
        let mut seen = std::collections::HashSet::new();
        for (module, func, fix) in ALIASES {
            assert!(
                seen.insert((*module, *func)),
                "duplicate alias key {module}.{func} (second row suggests {fix})"
            );
        }
    }

    /// Every alias must point somewhere a reader can act on: either a real
    /// `module.fn`, or prose that names the shape to write instead.
    #[test]
    fn alias_suggestions_are_non_empty() {
        for (module, func, fix) in ALIASES {
            assert!(!fix.is_empty(), "empty suggestion for {module}.{func}");
        }
    }

    /// A module with no description falls back to "standard library module",
    /// which tells the reader nothing. Adding a bundled module must therefore
    /// mean adding its description in the same change.
    #[test]
    fn stdlib_module_descriptions_are_complete() {
        let missing: Vec<&str> = BUNDLED_MODULES
            .iter()
            .copied()
            .filter(|m| module_description(m) == "standard library module")
            .collect();
        assert!(missing.is_empty(), "modules without a description: {missing:?}");
    }

    /// The description table must not carry rows for modules that no longer
    /// exist — a stale row is a description nothing can ever show.
    #[test]
    fn stdlib_module_descriptions_have_no_stale_rows() {
        let stale: Vec<&str> = MODULE_DESCRIPTIONS
            .iter()
            .map(|(m, _)| *m)
            .filter(|m| !BUNDLED_MODULES.contains(m) && !STDLIB_MODULES.contains(m))
            .collect();
        assert!(stale.is_empty(), "descriptions for unknown modules: {stale:?}");
    }
}
