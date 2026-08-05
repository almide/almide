//! #1041 → ADR-0006: the `list.try_*` family is FROZEN at its shipped seven.
//!
//! The family was a monomorphic workaround for the missing fallibility
//! polymorphism (Swift's rethrows); ADR-0006 dissolves it — `list.map(xs,
//! (x) => f(x)!)!` replaces every twin once #1108 lands. Until then this
//! gate is the freeze guard (D2), inverted from its original completeness
//! direction:
//!
//!   - the seven shipped twins stay EXACTLY as they are (the deprecated
//!     surface must keep working through the window), and
//!   - NO new `try_` member may appear — a new core combinator does NOT get
//!     a twin anymore (the polymorphic form will cover it); an eighth twin
//!     here is the drift this gate now exists to catch.
//!
//! At the #1108 landing this file flips once more: the seven become
//! deprecated (mechanical-rewrite hints, ADR-0006 D3) and are removed the
//! following minor — then this gate asserts the family is EMPTY.

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

/// The shipped seven — frozen (ADR-0006 D2). Removal happens as ONE act at
/// the #1108 landing, never member-by-member.
const FROZEN_TRY_FAMILY: &[&str] =
    &["try_map", "try_filter", "try_flat_map", "try_filter_map", "try_fold", "try_find", "try_each"];

#[test]
fn the_frozen_seven_stay_shipped_through_the_window() {
    let declared: HashSet<String> = list_surface().into_iter().collect();
    for twin in FROZEN_TRY_FAMILY {
        assert!(
            declared.contains(*twin),
            "`list.{twin}` disappeared — the deprecated family must keep working \
             until the #1108 polymorphic landing removes ALL seven together \
             (ADR-0006 D3); a member-by-member removal breaks the window contract"
        );
    }
}

#[test]
fn no_new_try_member_ever_appears() {
    let frozen: HashSet<&str> = FROZEN_TRY_FAMILY.iter().copied().collect();
    for name in list_surface() {
        if name.starts_with("try_") {
            assert!(
                frozen.contains(name.as_str()),
                "`list.{name}` is a NEW try_ member — the family is frozen \
                 (ADR-0006 D2): the fallibility-polymorphic form \
                 `list.<core>(xs, (x) => f(x)!)!` covers new combinators once \
                 #1108 lands; do not grow the workaround"
            );
        }
    }
}
