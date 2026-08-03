//! #1075: the dynamic-surface namespace gate.
//!
//! End state being enforced: **no fn reachable under two module names.**
//! `value.*` is the data model (constructors/accessors), `json.*` is the
//! format over it (parse / stringify / pretty, plus the json-branded path
//! API and typed-key conveniences, which have no second name).
//!
//! During the one-release deprecation window the aliases still exist and
//! warn (E040); this gate pins that window exactly:
//!
//! 1. every name that IS dual-reachable appears in
//!    `RETIRED_DYNAMIC_ALIASES` (no UN-deprecated duplicates can be added),
//! 2. `json.almd`'s public surface is exactly the allowed identity set plus
//!    the deprecated aliases (nothing new can grow on the json side),
//! 3. every table entry still resolves (so the drop release must delete
//!    the aliases and the table TOGETHER — at which point rule 2's
//!    deprecated set is empty and the gate asserts the final identity).

use almide::stdlib_info::{RETIRED_DYNAMIC_ALIASES};
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

fn retired_set() -> HashSet<&'static str> {
    RETIRED_DYNAMIC_ALIASES.iter().map(|(old, _, _)| *old).collect()
}

/// Rule 2: json's surface is the format identity + deprecated aliases, nothing else.
#[test]
fn json_surface_is_format_plus_deprecated_window() {
    // The one-sentence identity: json is the FORMAT (parse/print), plus the
    // json-branded path API and typed-key conveniences that have no second
    // name. Anything else must be a table-listed retired alias.
    let allowed: HashSet<&str> = [
        "parse", "stringify", "stringify_pretty",
        "root", "field", "index", "get_path", "set_path", "remove_path",
        "get_string", "get_int", "get_float", "get_bool", "get_array",
        "to_map",
    ]
    .into();
    let retired = retired_set();
    for (name, _) in surface_decls("json.almd") {
        let qualified = format!("json.{name}");
        assert!(
            allowed.contains(name.as_str()) || retired.contains(qualified.as_str()),
            "json.{name} is neither in json's format identity nor a table-listed retired alias — \
             the dynamic surface lives on value.* (#1075); do not grow json's side"
        );
    }
}

/// Rule 1: an intrinsic reachable under two public names must be in the table.
#[test]
fn dual_intrinsic_bindings_are_all_deprecated() {
    let mut by_intrinsic: HashMap<String, Vec<String>> = HashMap::new();
    for (file, module) in [("json.almd", "json"), ("value.almd", "value")] {
        for (name, intrinsic) in surface_decls(file) {
            if let Some(sym) = intrinsic {
                by_intrinsic.entry(sym).or_default().push(format!("{module}.{name}"));
            }
        }
    }
    let retired = retired_set();
    for (sym, names) in by_intrinsic {
        if names.len() < 2 {
            continue;
        }
        let undeprecated: Vec<&String> =
            names.iter().filter(|n| !retired.contains(n.as_str())).collect();
        assert!(
            undeprecated.len() <= 1,
            "intrinsic {sym} is reachable under {names:?} and more than one \
             ({undeprecated:?}) is not a retired alias — one fn, one name (#1075)"
        );
    }
}

/// Rule 3: the table and the aliases retire together — every entry's OLD
/// name still exists as a declaration, and every survivor exists too.
#[test]
fn retirement_table_matches_declarations() {
    let mut declared: HashSet<String> = HashSet::new();
    for (file, module) in [("json.almd", "json"), ("value.almd", "value")] {
        for (name, _) in surface_decls(file) {
            declared.insert(format!("{module}.{name}"));
        }
    }
    for (old, new, _) in RETIRED_DYNAMIC_ALIASES {
        assert!(
            declared.contains(*old),
            "{old} is in RETIRED_DYNAMIC_ALIASES but no longer declared — \
             the drop release must remove the alias AND its table row in the same PR"
        );
        assert!(
            declared.contains(*new),
            "{old}'s survivor {new} is not declared — the table names a fn that does not exist"
        );
    }
}

/// The value side never re-grows a `get`-shaped Result accessor once the
/// alias drops: `value.field` is the survivor. (While the window is open the
/// table covers `value.get`; after the drop this pins the end state.)
#[test]
fn value_get_is_only_reachable_via_table() {
    let retired = retired_set();
    let has_get = surface_decls("value.almd").iter().any(|(n, _)| n == "get");
    assert!(
        !has_get || retired.contains("value.get"),
        "value.get exists but is not in the retirement table — \
         get→Option is the map/list convention; the Result accessor is value.field (#1075)"
    );
}
