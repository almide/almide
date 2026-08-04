//! #1041: the `list.try_*` family completeness matrix (the API-family rule —
//! a family is extended by matrix, never point-wise).
//!
//! THE RULE: a `try_` twin exists for exactly the TRANSFORMING closure-bearing
//! core — `map`, `filter`, `flat_map`, `filter_map`, `fold`, `find`, `each` —
//! each with its callback lifted to `-> Result[_, E]` and the result wrapped
//! in `Result[_, E]`. Deliberate omissions (each with its reason, so a future
//! reader can tell a decision from a gap):
//!   - `any` / `all` / `count`: an erring PREDICATE query is `try_find`'s
//!     domain — `try_find(xs, p)` answers all three shapes.
//!   - `sort_by`: an erring key extractor has no meaningful partial order.
//!
//! Scraped from `stdlib/list.almd` itself (the namespace-gate pattern), so a
//! new core combinator or a hand-added point-wise `try_` drifts RED here, not
//! silently.

use std::collections::HashSet;

fn list_surface() -> Vec<String> {
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
            if !name.is_empty() && !name.starts_with("__") {
                out.push(name.to_string());
            }
        }
    }
    out
}

const TRY_CORE: &[&str] = &["map", "filter", "flat_map", "filter_map", "fold", "find", "each"];
const DELIBERATE_OMISSIONS: &[&str] = &["any", "all", "count", "sort_by"];

#[test]
fn every_transforming_core_has_its_try_twin() {
    let declared: HashSet<String> = list_surface().into_iter().collect();
    for core in TRY_CORE {
        assert!(
            declared.contains(*core) || *core == "each",
            "core combinator `list.{core}` disappeared — update the family rule"
        );
        assert!(
            declared.contains(&format!("try_{core}")),
            "`list.try_{core}` is missing — the try_ family is a MATRIX over \
             the transforming core; land the twin (with tests) or amend the \
             rule here with the reason (#1041)"
        );
    }
}

#[test]
fn omissions_stay_deliberate_not_accidental_growth() {
    let declared: HashSet<String> = list_surface().into_iter().collect();
    for omitted in DELIBERATE_OMISSIONS {
        assert!(
            !declared.contains(&format!("try_{omitted}")),
            "`list.try_{omitted}` appeared, but it is a RECORDED omission \
             (try_find answers the predicate-query shapes; sort_by has no \
             erring order) — either remove it or move it into TRY_CORE with \
             its completeness story (#1041)"
        );
    }
}

#[test]
fn no_unruled_try_member_exists() {
    // Every try_-prefixed fn on the surface must be a TRY_CORE twin — a
    // point-wise addition outside the matrix is the drift this gate exists
    // to catch.
    let ruled: HashSet<String> = TRY_CORE.iter().map(|c| format!("try_{c}")).collect();
    for name in list_surface() {
        if let Some(rest) = name.strip_prefix("try_") {
            assert!(
                ruled.contains(&name),
                "`list.try_{rest}` is outside the family rule — add its core \
                 to TRY_CORE (with the full twin set) rather than growing the \
                 family point-wise (#1041)"
            );
        }
    }
}
