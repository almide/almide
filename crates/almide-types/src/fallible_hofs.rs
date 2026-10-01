//! The fallibility-polymorphic HOF matrix (ADR-0006 D1, #3163).
//!
//! ADR-0006's rule is one sentence: "if the callback is `!`, the HOF is `!`".
//! The checker realises it by name — a call whose callback propagates
//! (`(x) => f(x)!`, or a named fn declared `-> T!`) is rewritten before
//! inference to the module's `__fallible_<hof>` carrier, which short-circuits
//! on the FIRST err in the container's iteration order (list order; a `Map` /
//! `Set` iterates in insertion order on every target).
//!
//! The rule held for seven `list` cells while the diagnostics said "the core
//! list HOFs accept fallible callbacks natively", so `list.all(xs, (x) =>
//! p(x)!)` was rejected under a hint that said it was accepted (#3163). This
//! table is now the ONE place the family lives: the checker's rewrite, the
//! dead-carrier guard, the hints, the interpreter's HOF allowlist and the
//! matrix gate (`tests/list_fallible_family_gate_test.rs`) all read it, so a
//! cell cannot be accepted in one place and refused in another.
//!
//! Completeness rule (asserted by the gate): every public fn of `list`, `map`,
//! `set`, `option` and `result` that takes a callback is EITHER a row of
//! [`FALLIBLE_HOFS`] — and `stdlib/<module>.almd` declares its
//! `__fallible_<hof>` carrier — OR a row of [`TOTAL_ONLY_HOFS`] with the
//! reason the fallible form does not apply. The fs streaming walkers have
//! their own matrix (`tests/fs_streaming_family_gate_test.rs`).

/// `(module, hof)` — each instantiates `module.__fallible_<hof>` when its
/// callback is fallible.
pub const FALLIBLE_HOFS: &[(&str, &str)] = &[
    ("list", "map"),
    ("list", "filter"),
    ("list", "flat_map"),
    ("list", "filter_map"),
    ("list", "fold"),
    ("list", "find"),
    ("list", "each"),
    ("list", "any"),
    ("list", "all"),
    ("list", "count"),
    ("list", "find_index"),
    ("list", "partition"),
    ("list", "take_while"),
    ("list", "drop_while"),
    ("list", "reduce"),
    ("list", "scan"),
    ("list", "sort_by"),
    ("list", "group_by"),
    ("list", "unique_by"),
    ("list", "zip_with"),
    ("list", "update"),
    ("list", "iterate"),
    ("map", "map"),
    ("map", "filter"),
    ("map", "fold"),
    ("map", "any"),
    ("map", "all"),
    ("map", "count"),
    ("map", "find"),
    ("map", "update"),
    ("map", "upsert"),
    ("set", "map"),
    ("set", "filter"),
    ("set", "fold"),
    ("set", "any"),
    ("set", "all"),
    ("option", "map"),
    ("option", "flat_map"),
    ("option", "filter"),
    ("option", "unwrap_or_else"),
    ("option", "or_else"),
];

/// `(module, hof, reason)` — callback-taking fns with NO fallible form, and why.
pub const TOTAL_ONLY_HOFS: &[(&str, &str, &str)] = &[
    (
        "result",
        "map",
        "the container is itself the failure channel: a fallible callback is `result.flat_map`, \
         or `f(r!)!` in an effect fn",
    ),
    (
        "result",
        "map_err",
        "the callback maps the error channel itself; a second failure inside it has no channel to \
         short-circuit into",
    ),
    (
        "result",
        "flat_map",
        "its callback already returns the container's Result; a lambda `!` is that Result's own \
         channel",
    ),
    (
        "result",
        "unwrap_or_else",
        "the callback runs only on the err side; a fallible recovery is `result.or_else`",
    ),
    (
        "result",
        "or_else",
        "its callback already returns a Result; a lambda `!` is that Result's own channel",
    ),
    (
        "result",
        "filter",
        "the container is itself the failure channel: test `r!` in an effect fn",
    ),
    (
        "option",
        "collect_map",
        "a derived convenience over `list.map`; its fallible form is the composition spelled out: \
         `option.collect(list.map(xs, (x) => f(x)!)!)`",
    ),
];

/// Does `module.hof` take a fallible callback (and so have a carrier)?
pub fn is_fallible_hof(module: &str, hof: &str) -> bool {
    FALLIBLE_HOFS.iter().any(|&(m, h)| m == module && h == hof)
}

/// Is `module.name` one of the `__fallible_*` carriers the rewrite targets?
pub fn is_fallible_carrier(module: &str, name: &str) -> bool {
    name.strip_prefix("__fallible_").is_some_and(|hof| is_fallible_hof(module, hof))
}

/// The accepted cells as diagnostics print them, grouped by module:
/// `list.{map, filter, …}, map.{…}, …`.
pub fn summary() -> String {
    let mut out: Vec<String> = Vec::new();
    let mut module = "";
    let mut names: Vec<&str> = Vec::new();
    let flush = |module: &str, names: &mut Vec<&str>, out: &mut Vec<String>| {
        if !names.is_empty() {
            out.push(format!("{}.{{{}}}", module, names.join(", ")));
            names.clear();
        }
    };
    for &(m, h) in FALLIBLE_HOFS {
        if m != module {
            flush(module, &mut names, &mut out);
            module = m;
        }
        names.push(h);
    }
    flush(module, &mut names, &mut out);
    out.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_cell_is_both_accepted_and_excluded() {
        for &(m, h, _) in TOTAL_ONLY_HOFS {
            assert!(!is_fallible_hof(m, h), "{m}.{h} is in both tables");
        }
    }

    #[test]
    fn the_summary_names_every_cell() {
        let s = summary();
        assert!(s.starts_with("list.{map, filter,"));
        assert!(s.contains("option.{map, flat_map, filter, unwrap_or_else, or_else}"));
    }

    #[test]
    fn carriers_are_recognised_by_their_cell() {
        assert!(is_fallible_carrier("list", "__fallible_all"));
        assert!(!is_fallible_carrier("result", "__fallible_map"));
        assert!(!is_fallible_carrier("list", "all"));
    }
}
