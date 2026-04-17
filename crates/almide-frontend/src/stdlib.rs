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
        // comparison functions delegate to the single canonical table
        // below so `suggest_alias`, `try_snippet_for_alias`, and
        // downstream `almide fix` rules all agree on the shape.
        _ if comparison_operator_of(module, func).is_some() => {
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
/// user-visible surface should call `module_functions_all` instead —
/// this list shrinks as fns migrate to `stdlib/<m>.almd`, but the
/// reflection surface should not.
pub fn module_functions(module: &str) -> Vec<&'static str> {
    crate::generated::stdlib_sigs::generated_module_functions(module)
}

/// Union of TOML-declared and bundled-declared fn names for a stdlib
/// module. Stable-ordered so snapshot tests over outlines don't
/// flutter when TOML ↔ bundled balance shifts during migration.
pub fn module_functions_all(module: &str) -> Vec<&'static str> {
    let mut all: std::collections::BTreeSet<&'static str> =
        crate::generated::stdlib_sigs::generated_module_functions(module)
            .into_iter()
            .collect();
    for n in crate::bundled_sigs::module_fn_names(module) {
        all.insert(n);
    }
    all.into_iter().collect()
}

/// Look up a stdlib function's type signature.
///
/// Resolution order:
/// 1. Generated TOML sigs (`stdlib/defs/*.toml` → `stdlib_sigs.rs` at
///    build time). Fast path, covers every module that still has a
///    TOML entry.
/// 2. Bundled `.almd` sigs (Stdlib Declarative Unification Stage 2+).
///    When a module is migrated to `stdlib/<m>.almd`, its TOML entry
///    may be deleted; the signatures then live in the bundled source
///    alone. We parse the bundled source on demand and cache the
///    resulting sig map per module — parsing is cheap, cache is
///    shared across the process, and the cache only grows when
///    `canonicalize` / `lookup_sig` actually asks for a bundled
///    module.
pub fn lookup_sig(module: &str, func: &str) -> Option<FnSig> {
    if let Some(sig) = crate::generated::stdlib_sigs::lookup_generated_sig(module, func) {
        return Some(sig);
    }
    lookup_bundled_sig(module, func)
}

/// Bundled-source signature lookup. Delegates to the caching layer in
/// `bundled_sigs` (per-module parse, process-wide cache) so that
/// migrating a stdlib fn to `stdlib/<m>.almd` keeps the type checker
/// informed without any TOML bridge.
fn lookup_bundled_sig(module: &str, func: &str) -> Option<FnSig> {
    crate::bundled_sigs::lookup(module, func)
}
