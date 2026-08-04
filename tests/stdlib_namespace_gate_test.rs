//! #1075 → #1078: the dynamic-surface namespace gate, END STATE.
//!
//! What is enforced: **no fn reachable under two module names.**
//! `value.*` is the data model (constructors/accessors), `json.*` is the
//! format over it (parse / stringify / pretty, plus the json-branded path
//! API and typed-key conveniences, which have no second name).
//!
//! The one-release E040 deprecation window (#1075) closed in #1078: the
//! aliases, the `RETIRED_DYNAMIC_ALIASES` table, the E040 checker, and the
//! `almide fix` rewrite were dropped together. This gate pins the end state:
//!
//! 1. json's surface is EXACTLY the format identity — nothing else resolves,
//!    and none of the dropped alias names may ever reappear;
//! 2. no intrinsic is reachable under two public names (the rule the window
//!    existed to reach);
//! 3. `value.get` never regrows — `get → Option` is the map/list convention,
//!    and the Result accessor is `value.field`.
//!
//! The executable twin of rule 1 is `tests/diagnostics/retired-json-alias/`
//! (a retired spelling must FAIL with E002) and
//! `tests/fix_test.rs::retired_aliases_no_longer_resolve`.

use std::collections::{HashMap, HashSet};

/// Parse `stdlib/<file>.almd` for `(fn_name, intrinsic)` pairs — the public
/// surface declarations (`@intrinsic("…")` directly above `fn name(…)`).
/// Helper fns (self-host impls) live in other files; the two surface files
/// scanned here are the module definitions themselves.
fn surface_decls(file: &str) -> Vec<(String, Option<String>)> {
    let path = format!("{}/stdlib/{}", env!("CARGO_MANIFEST_DIR"), file);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let mut out = Vec::new();
    let mut pending_intrinsic: Option<String> = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("@intrinsic(\"") {
            if let Some(sym) = rest.split('"').next() {
                pending_intrinsic = Some(sym.to_string());
            }
            continue;
        }
        if line.starts_with('@') || line.starts_with("//") {
            continue;
        }
        let fn_line = line
            .strip_prefix("pub fn ")
            .or_else(|| line.strip_prefix("fn "))
            .or_else(|| line.strip_prefix("pub effect fn "))
            .or_else(|| line.strip_prefix("effect fn "));
        if let Some(rest) = fn_line {
            if let Some(name) = rest.split(['(', '[']).next() {
                let name = name.trim();
                if !name.is_empty() && !name.starts_with("__") {
                    out.push((name.to_string(), pending_intrinsic.take()));
                }
            }
        } else if !line.is_empty() {
            pending_intrinsic = None;
        }
    }
    out
}

/// The names dropped in #1078. None of them may ever be declared again —
/// resurrecting one silently re-opens the two-names problem the whole
/// retirement existed to close.
const DROPPED: &[&str] = &[
    "json.null",
    "json.object",
    "json.array",
    "json.keys",
    "json.from_string",
    "json.from_int",
    "json.from_bool",
    "json.from_float",
    "json.as_string",
    "json.as_int",
    "json.as_float",
    "json.as_bool",
    "json.as_array",
    "json.get",
    "value.get",
];

/// Rule 1: json's surface is EXACTLY the format identity, and the dropped
/// aliases do not resolve — in either module.
#[test]
fn json_surface_is_exactly_the_format_identity() {
    // The one-sentence identity: json is the FORMAT (parse/print), plus the
    // json-branded path API and typed-key conveniences that have no second
    // name. Anything else is a regression toward the two-names problem.
    let allowed: HashSet<&str> = [
        "parse", "stringify", "stringify_pretty",
        "root", "field", "index", "get_path", "set_path", "remove_path",
        "get_string", "get_int", "get_float", "get_bool", "get_array",
        "to_map",
    ]
    .into();
    let mut declared: HashSet<String> = HashSet::new();
    for (file, module) in [("json.almd", "json"), ("value.almd", "value")] {
        for (name, _) in surface_decls(file) {
            declared.insert(format!("{module}.{name}"));
        }
    }
    for (name, _) in surface_decls("json.almd") {
        assert!(
            allowed.contains(name.as_str()),
            "json.{name} is outside json's format identity — the dynamic surface \
             lives on value.*; do not grow json's side"
        );
    }
    for dropped in DROPPED {
        assert!(
            !declared.contains(*dropped),
            "{dropped} is a dropped alias and may not be re-declared — \
             one fn, one name"
        );
    }
    // The survivors must all exist, or a dropped alias lost its operation.
    for survivor in [
        "value.null", "value.object", "value.array", "value.keys", "value.str",
        "value.int", "value.bool", "value.float", "value.as_string", "value.as_int",
        "value.as_float", "value.as_bool", "value.as_array", "value.field",
    ] {
        assert!(declared.contains(survivor), "survivor {survivor} is not declared");
    }
}

/// Rule 2: no intrinsic is reachable under two public names — the end state
/// the deprecation window existed to reach, now with no exceptions.
#[test]
fn no_intrinsic_is_reachable_under_two_names() {
    let mut by_intrinsic: HashMap<String, Vec<String>> = HashMap::new();
    for (file, module) in [("json.almd", "json"), ("value.almd", "value")] {
        for (name, intrinsic) in surface_decls(file) {
            if let Some(sym) = intrinsic {
                by_intrinsic.entry(sym).or_default().push(format!("{module}.{name}"));
            }
        }
    }
    for (sym, names) in by_intrinsic {
        assert!(
            names.len() <= 1,
            "intrinsic {sym} is reachable under {names:?} — one fn, one name"
        );
    }
}

/// Rule 3: the value side never regrows a `get`-shaped Result accessor:
/// `value.field` is the survivor, and `get → Option` is the map/list
/// convention.
#[test]
fn value_get_never_regrows() {
    let has_get = surface_decls("value.almd").iter().any(|(n, _)| n == "get");
    assert!(
        !has_get,
        "value.get regrew — get→Option is the map/list convention; \
         the Result accessor is value.field"
    );
}
