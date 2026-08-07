//! #1041 → ADR-0006: the `list.try_*` family is GONE (v0.56.0) — the core
//! HOF is fallibility-polymorphic, so the family's meaning lives on ONE name
//! per combinator (`list.map(xs, (x) => f(x)!)!`).
//!
//! This gate went through the ADR's three states in order: completeness
//! matrix (v0.53.6) → freeze at seven (v0.54.0, D2) → EMPTY (this file, D3).
//! It now asserts BOTH halves of the end state:
//!
//!   - the public surface carries NO try_-prefixed fn — a resurrected twin is
//!     the drift this gate exists to catch (a new combinator's fallible form
//!     is the polymorphic instantiation, never a named sibling), and
//!   - the seven `__fallible_*` INTERNAL CARRIERS exist — the checker's
//!     fallible-callback normalization routes to them, so a silently deleted
//!     carrier would break `list.map(xs, (x) => f(x)!)` at a distance.

use std::collections::HashSet;

/// Scrape declarations INCLUDING the `__`-prefixed internals.
fn list_decls() -> Vec<String> {
    let path = format!("{}/stdlib/list.almd", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).expect("read stdlib/list.almd");
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("fn ").or_else(|| line.strip_prefix("pub fn ")) else {
            continue;
        };
        if let Some(name) = rest.split(['(', '[']).next() {
            let name = name.trim();
            if !name.is_empty() {
                out.push(name.to_string());
            }
        }
    }
    out
}

/// The carriers the polymorphic normalization targets (ADR-0006 D1).
const INTERNAL_CARRIERS: &[&str] = &[
    "__fallible_map", "__fallible_filter", "__fallible_flat_map", "__fallible_filter_map",
    "__fallible_fold", "__fallible_find", "__fallible_each",
];

#[test]
fn the_public_try_family_is_empty() {
    for name in list_decls() {
        assert!(
            !name.starts_with("try_"),
            "`list.{name}` resurrects the removed try_ family (ADR-0006 D3): \
             a combinator's fallible form is the polymorphic instantiation \
             (`list.<core>(xs, (x) => f(x)!)!`), never a named twin"
        );
    }
}

#[test]
fn the_seven_internal_carriers_exist() {
    let declared: HashSet<String> = list_decls().into_iter().collect();
    for carrier in INTERNAL_CARRIERS {
        assert!(
            declared.contains(*carrier),
            "`list.{carrier}` is missing — the fallible-callback normalization \
             (normalize_fallible_hof_callback) routes `list.map(xs, (x) => f(x)!)` \
             to this body; deleting it breaks the polymorphic form at a distance"
        );
    }
}
