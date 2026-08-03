/// Stdlib module registry: module names, UFCS resolution, bundled module list.
///
/// Pure data with no dependencies — shared by checker (main crate) and codegen.

/// All built-in stdlib module names (hardcoded in the compiler).
pub const STDLIB_MODULES: &[&str] = &[
    "string", "list", "int", "float", "bytes", "matrix", "fs", "env", "map",
    "json", "http", "process", "math", "random", "regex", "io", "result",
    "option", "error", "datetime", "testing", "value", "set",
    "base64", "hex", "net", "zlib",
    // Sized numeric types (Stage 3 of the sized-numeric-types arc).
    // Each hosts UFCS conversion methods (`.to_int64()`,
    // `.to_float32()`, ...). Auto-imported alongside `int` / `float`
    // so users never need `import int32`.
    "int8", "int16", "int32", "int64",
    "uint8", "uint16", "uint32", "uint64",
    "float32", "float64",
];

/// Bundled stdlib modules written in Almide (.almd files embedded in the compiler binary).
pub const BUNDLED_MODULES: &[&str] = &[
    "args", "path", "list", "int", "base64", "hex", "float", "bytes",
    "error", "math", "datetime", "value", "option", "result",
    "map", "set", "string",
    "env", "io", "random", "regex", "testing",
    "process", "fs", "http", "html", "json", "matrix", "mem", "net", "zlib",
    "int8", "int16", "int32", "int64",
    "uint8", "uint16", "uint32", "uint64",
    "float32", "float64",
    // The v1 primitive floor (raw memory + fd_write); v1 maps `prim.*` to MIR ops.
    "prim",
];

/// Bundled modules that should be auto-imported (Tier 1 behavior).
/// Tier-1 stdlib modules with no bundled-Almide content (option, result, etc.)
/// are auto-imported via the hardcoded list in
/// `almide-frontend::import_table::ImportTable::new`; this list is for
/// bundled `.almd` modules that need resolve-time loading.
pub const AUTO_IMPORT_BUNDLED: &[&str] = &[
    "list", "int", "float",
    // Stdlib modules migrated from TOML to bundled `.almd` need
    // auto-import so their signatures and dispatch reach every file
    // without requiring a redundant `import <name>`. The set mirrors
    // the auto-import behavior the generated `stdlib_sigs` previously
    // provided.
    "error", "math", "datetime", "value", "option", "result",
    "map", "set", "string",
    "int8", "int16", "int32", "int64",
    "uint8", "uint16", "uint32", "uint64",
    "float32", "float64",
    // env / io / random / regex / testing are NOT auto-imported —
    // users still need `import env` / `import io` etc. These bundled
    // modules exist so `@inline_rust` templates are discoverable when
    // the call site explicitly imports them; auto-import would bring
    // effect-surface helpers into every file unsolicited.
];

/// Nominal types backed by runtime Rust structs, owned by a stdlib module:
/// `(module, type name)`. They appear in the module's own signatures
/// (`http.serve(port, f: (HttpRequest) -> HttpResponse)`) but have no `type`
/// declaration anywhere, so without this registry a user ANNOTATION naming
/// one is an E029 — the docs advertise a type the writer cannot spell
/// (#1053). The checker accepts the bare and `module.`-qualified spellings
/// whenever the owner module is imported. Completeness is machine-checked:
/// `runtime_backed_types_matrix` (almide-frontend tests) asserts this list
/// covers exactly the undeclared nominal leaves of every bundled signature.
pub const RUNTIME_BACKED_TYPES: &[(&str, &str)] = &[
    ("http", "HttpRequest"),
    ("http", "HttpResponse"),
    // The opaque path handle of `json.root()`/`json.field(...)` — self-hosted
    // as a `List[String]` newtype, still spelled `JsonPath` in signatures.
    ("json", "JsonPath"),
];

/// The owning module of a runtime-backed nominal type, accepting the bare
/// (`HttpRequest`) and qualified (`http.HttpRequest`) spellings. `None` when
/// the name is not runtime-backed (the common case).
pub fn runtime_backed_type_owner(name: &str) -> Option<&'static str> {
    let bare = name.rsplit_once('.').map(|(_, b)| b).unwrap_or(name);
    let qualifier = name.rsplit_once('.').map(|(q, _)| q);
    RUNTIME_BACKED_TYPES.iter().find_map(|(module, ty)| {
        let qualifier_ok = qualifier.map_or(true, |q| q == *module);
        (*ty == bare && qualifier_ok).then_some(*module)
    })
}

/// How a retired dynamic-surface alias maps onto its survivor.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RetiredAliasKind {
    /// Same signature, same behavior — a plain name swap.
    Rename,
    /// The survivor returns `Result` where the alias returned `Option`:
    /// swap the name AND append `?` (the Result→Option conversion) to the
    /// call, which preserves the expression's type exactly.
    RenameAndNarrow,
}

/// #1075: the dynamic surface had two module names for one concept. The
/// survivor split — `value.*` is the DATA MODEL (constructors/accessors),
/// `json.*` is the FORMAT over it (parse / stringify / pretty, plus the
/// json-branded path and typed-key conveniences) — retires every fn that
/// was reachable under two names. `value.get` is retired inside its own
/// module too: `value.field` is the same native intrinsic with the safer
/// wasm lowering (Object-tag guard), and `get → Option` is the convention
/// everywhere else (`map.get` / `list.get`), which made a Result-returning
/// `value.get` a false friend.
///
/// One release of E040 warnings with a mechanical `almide fix` rewrite,
/// then the aliases drop. This table is the single source of truth: the
/// checker's warning, `almide fix`'s rewrite, and the namespace gate
/// (`tests/stdlib_namespace_gate_test.rs`) all read it, so the drop
/// release deletes the aliases and this table together and the gate then
/// enforces the end state (no fn reachable under two module names).
pub const RETIRED_DYNAMIC_ALIASES: &[(&str, &str, RetiredAliasKind)] = &[
    ("json.null", "value.null", RetiredAliasKind::Rename),
    ("json.object", "value.object", RetiredAliasKind::Rename),
    ("json.array", "value.array", RetiredAliasKind::Rename),
    ("json.keys", "value.keys", RetiredAliasKind::Rename),
    ("json.from_string", "value.str", RetiredAliasKind::Rename),
    ("json.from_int", "value.int", RetiredAliasKind::Rename),
    ("json.from_bool", "value.bool", RetiredAliasKind::Rename),
    ("json.from_float", "value.float", RetiredAliasKind::Rename),
    ("json.as_string", "value.as_string", RetiredAliasKind::RenameAndNarrow),
    ("json.as_int", "value.as_int", RetiredAliasKind::RenameAndNarrow),
    ("json.as_float", "value.as_float", RetiredAliasKind::RenameAndNarrow),
    ("json.as_bool", "value.as_bool", RetiredAliasKind::RenameAndNarrow),
    ("json.as_array", "value.as_array", RetiredAliasKind::RenameAndNarrow),
    ("json.get", "value.field", RetiredAliasKind::RenameAndNarrow),
    ("value.get", "value.field", RetiredAliasKind::Rename),
];

/// Look up a retired dynamic-surface alias by its qualified name.
pub fn retired_dynamic_alias(name: &str) -> Option<(&'static str, RetiredAliasKind)> {
    RETIRED_DYNAMIC_ALIASES
        .iter()
        .find(|(old, _, _)| *old == name)
        .map(|(_, new, kind)| (*new, *kind))
}

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
/// Both downstream consumers (`almide-frontend::bundled_sigs` for
/// FnSig extraction, `almide-codegen::pass_stdlib_lowering` for
/// `@inline_rust` template extraction) feed the returned source into
/// `almide_syntax::parse_cached`, a process-wide AST cache. A single
/// parse per source pointer backs both views, so the FnSig table the
/// type checker queries and the templates codegen emits cannot drift
/// out of step as bundled `.almd` modules evolve.
pub fn bundled_source(name: &str) -> Option<&'static str> {
    bundled_source_core(name)
        .or_else(|| bundled_source_collections(name))
        .or_else(|| bundled_source_io(name))
        .or_else(|| bundled_source_sized_numeric(name))
}

fn bundled_source_core(name: &str) -> Option<&'static str> {
    match name {
        "args" => Some(crate::embedded::SRC_ARGS),
        "path" => Some(crate::embedded::SRC_PATH),
        "list" => Some(crate::embedded::SRC_LIST),
        "int" => Some(crate::embedded::SRC_INT),
        "base64" => Some(crate::embedded::SRC_BASE64),
        "hex" => Some(crate::embedded::SRC_HEX),
        "float" => Some(crate::embedded::SRC_FLOAT),
        "bytes" => Some(crate::embedded::SRC_BYTES),
        "error" => Some(crate::embedded::SRC_ERROR),
        "value" => Some(crate::embedded::SRC_VALUE),
        _ => None,
    }
}

fn bundled_source_collections(name: &str) -> Option<&'static str> {
    match name {
        "option" => Some(crate::embedded::SRC_OPTION),
        "result" => Some(crate::embedded::SRC_RESULT),
        "map" => Some(crate::embedded::SRC_MAP),
        "set" => Some(crate::embedded::SRC_SET),
        "string" => Some(crate::embedded::SRC_STRING),
        "env" => Some(crate::embedded::SRC_ENV),
        "io" => Some(crate::embedded::SRC_IO),
        "random" => Some(crate::embedded::SRC_RANDOM),
        "regex" => Some(crate::embedded::SRC_REGEX),
        "testing" => Some(crate::embedded::SRC_TESTING),
        _ => None,
    }
}

fn bundled_source_io(name: &str) -> Option<&'static str> {
    match name {
        "process" => Some(crate::embedded::SRC_PROCESS),
        "fs" => Some(crate::embedded::SRC_FS),
        "http" => Some(crate::embedded::SRC_HTTP),
        "html" => Some(crate::embedded::SRC_HTML),
        "json" => Some(crate::embedded::SRC_JSON),
        "matrix" => Some(crate::embedded::SRC_MATRIX),
        "mem" => Some(crate::embedded::SRC_MEM),
        "math" => Some(crate::embedded::SRC_MATH),
        "datetime" => Some(crate::embedded::SRC_DATETIME),
        _ => None,
    }
}

fn bundled_source_sized_numeric(name: &str) -> Option<&'static str> {
    match name {
        "int8" => Some(crate::embedded::SRC_INT8),
        "int16" => Some(crate::embedded::SRC_INT16),
        "int32" => Some(crate::embedded::SRC_INT32),
        "int64" => Some(crate::embedded::SRC_INT64),
        "uint8" => Some(crate::embedded::SRC_UINT8),
        "uint16" => Some(crate::embedded::SRC_UINT16),
        "uint32" => Some(crate::embedded::SRC_UINT32),
        "uint64" => Some(crate::embedded::SRC_UINT64),
        "float32" => Some(crate::embedded::SRC_FLOAT32),
        "float64" => Some(crate::embedded::SRC_FLOAT64),
        "net" => Some(crate::embedded::SRC_NET),
        "zlib" => Some(crate::embedded::SRC_ZLIB),
        // The v1 primitive floor (raw memory + fd_write). v1 maps `prim.*` to MIR
        // primitive ops by module name; the @intrinsic symbols are v0 placeholders.
        "prim" => Some(crate::embedded::SRC_PRIM),
        _ => None,
    }
}

#[cfg(test)]
mod bundled_source_coverage {
    /// Every declared bundled module must actually carry a source. `html` sat in
    /// `BUNDLED_MODULES` with no `bundled_source` arm, so `almide compile html`
    /// reported "no bundled source wired in stdlib_info" for a module the binary
    /// was otherwise treating as bundled (#863). The two lists are hand-written
    /// and in different functions, so nothing but this test keeps them in step.
    #[test]
    fn every_bundled_module_has_a_source() {
        let missing: Vec<&str> = super::BUNDLED_MODULES
            .iter()
            .copied()
            .filter(|m| super::bundled_source(m).is_none())
            .collect();
        assert!(missing.is_empty(), "BUNDLED_MODULES with no bundled_source arm: {missing:?}");
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
    let r = resolve_ufcs_exclusive(method);
    if !r.is_empty() {
        return r;
    }
    let r = resolve_ufcs_ambiguous(method);
    if !r.is_empty() {
        return r;
    }
    resolve_ufcs_sized_numeric(method)
}

/// Methods owned by exactly one stdlib module (or a small closed set of
/// module-native container methods: list/map/set, list/map).
fn resolve_ufcs_exclusive(method: &str) -> Vec<&'static str> {
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
        // (`to_string` is NOT here: every sized-numeric module declares one,
        // so it belongs to the sized-numeric table below — see #893.)
        "to_hex"
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

        _ => vec![],
    }
}

/// Methods shared by more than one stdlib module where the caller's type
/// disambiguates (string/list overlap, numeric overlap, etc).
fn resolve_ufcs_ambiguous(method: &str) -> Vec<&'static str> {
    let r = resolve_ufcs_ambiguous_string_list(method);
    if !r.is_empty() {
        return r;
    }
    resolve_ufcs_ambiguous_container(method)
}

/// Ambiguous methods where the overlap is anchored on `string` and/or `list`
/// (plus their map/set siblings for the container-shaped ones).
fn resolve_ufcs_ambiguous_string_list(method: &str) -> Vec<&'static str> {
    match method {
        // ── ambiguous: string + list ──
        "first" | "last" => vec!["string", "list"],
        "take" | "drop" | "take_end" | "drop_end" => vec!["string", "list"],
        "reverse" => vec!["string", "list"],
        "index_of" => vec!["string", "list"],
        "join" => vec!["string", "list"],
        "slice" => vec!["string", "list"],

        // ── ambiguous: string + list + map + set ──
        "len" => vec!["string", "list", "map", "set"],
        "length" => vec!["string", "list"],
        "contains" => vec!["string", "list", "map", "set"],
        "is_empty" => vec!["string", "list", "map", "set"],

        // ── ambiguous: string + list + map ──
        "count" => vec!["string", "list", "map"],

        _ => vec![],
    }
}

/// Ambiguous methods anchored on container/numeric overlaps (list+result+
/// option, list+map+set, int+float, ...) rather than on `string`.
fn resolve_ufcs_ambiguous_container(method: &str) -> Vec<&'static str> {
    match method {
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

/// Sized numeric conversion methods (Stage 3). Every sized int / float
/// provides these UFCS methods. The concrete module (int32, uint8, float32,
/// ...) is picked by the receiver's type at codegen (`resolve_module_from_ty`).
fn resolve_ufcs_sized_numeric(method: &str) -> Vec<&'static str> {
    match method {
        "to_int8" | "to_int16" | "to_int32" | "to_int64"
        | "to_uint8" | "to_uint16" | "to_uint32" | "to_uint64"
        | "to_float32" | "to_float64"
        // `to_string` too (#893): every sized-numeric module declares one, and
        // the UFCS resolver reaches this table when `env.functions` has no
        // entry — which is the case for an embedder that hands the renderer a
        // bare source (no resolved bundled modules), so `big.to_string()`
        // reported "undefined method" through the API while the CLI, whose
        // resolver HAD registered the bundled signatures, accepted it. The
        // candidate list is only consulted after `builtin_module_for_type`
        // has already picked the ONE module for the receiver's type, so
        // listing them together cannot make a call ambiguous.
        | "to_string" => vec![
            "int", "float",
            "int8", "int16", "int32", "int64",
            "uint8", "uint16", "uint32", "uint64",
            "float32", "float64",
        ],

        _ => vec![],
    }
}
