//! The `@implements("m.f")` declarations (#3620), as `self_host_runtime()`
//! serves them beside the hand-written registry rows.

use almide_types::implements::{DECLARED, PRIM_FREE};
use almide_types::self_host_registry::self_host_runtime;

/// Every declared row is served, after the hand-written rows.
#[test]
fn the_registry_serves_every_declared_row() {
    let all = self_host_runtime();
    for row in DECLARED {
        assert!(all.iter().any(|r| r == row), "declared row {:?} is not served", row.1);
    }
}

/// A declared call name has one body: no other source, hand-written or
/// declared, registers it too (two sources would both define it).
#[test]
fn a_declared_call_name_is_registered_once() {
    for (dsrc, dpairs) in DECLARED {
        for (_, call) in *dpairs {
            let owners: Vec<_> = self_host_runtime()
                .iter()
                .filter(|(_, pairs)| pairs.iter().any(|(_, c)| c == call))
                .map(|(src, _)| *src)
                .collect();
            assert!(owners == [*dsrc], "{call} is registered by {} sources", owners.len());
        }
    }
}

/// A declaration names a function its own source defines.
#[test]
fn every_declared_impl_fn_is_defined_in_its_source() {
    for (src, pairs) in DECLARED {
        for (impl_fn, call) in *pairs {
            let defined = src.lines().any(|l| {
                let t = l.trim_start().trim_start_matches("effect ");
                t.starts_with(&format!("fn {impl_fn}(")) || t.starts_with(&format!("fn {impl_fn}["))
            });
            assert!(defined, "{call}: `{impl_fn}` is not defined in its source");
        }
    }
}

/// `PRIM_FREE` only names declared bodies, and none of their sources reach `prim.`.
#[test]
fn prim_free_names_declared_bodies_whose_source_never_reaches_prim() {
    for name in PRIM_FREE {
        let (src, _) = DECLARED
            .iter()
            .find(|(_, pairs)| pairs.iter().any(|(i, _)| i == name))
            .unwrap_or_else(|| panic!("{name} is in PRIM_FREE but declares nothing"));
        assert!(!src.contains("prim."), "{name}'s source reaches prim.");
    }
}
