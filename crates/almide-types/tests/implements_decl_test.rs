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

/// The top-level function of `src` named `name`: its `fn` line up to the
/// next top-level `fn` (comment lines dropped).
fn body_of<'s>(src: &'s str, name: &str) -> Vec<&'s str> {
    let is_top_fn = |l: &str| {
        let t = l.trim_start_matches("effect ");
        !l.starts_with(' ') && t.starts_with("fn ")
    };
    let starts = |l: &str| {
        let t = l.trim_start_matches("effect ");
        t.starts_with(&format!("fn {name}(")) || t.starts_with(&format!("fn {name}["))
    };
    let lines: Vec<&str> = src.lines().filter(|l| !l.trim_start().starts_with("//")).collect();
    let Some(i) = lines.iter().position(|l| starts(l)) else { return Vec::new() };
    let end = lines[i + 1..].iter().position(|l| is_top_fn(l)).map_or(lines.len(), |j| i + 1 + j);
    lines[i..end].to_vec()
}

/// The names of `src`'s top-level functions.
fn fn_names(src: &str) -> Vec<String> {
    src.lines()
        .filter(|l| !l.starts_with(' '))
        .filter_map(|l| l.trim_start_matches("effect ").strip_prefix("fn "))
        .map(|r| r.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect())
        .collect()
}

/// `PRIM_FREE` only names declared bodies, and none of them reaches `prim.`,
/// through its own lines or a function of its source it calls (#4204).
#[test]
fn prim_free_names_declared_bodies_that_never_reach_prim() {
    for name in PRIM_FREE {
        let (src, _) = DECLARED
            .iter()
            .find(|(_, pairs)| pairs.iter().any(|(i, _)| i == name))
            .unwrap_or_else(|| panic!("{name} is in PRIM_FREE but declares nothing"));
        let names = fn_names(src);
        let mut todo = vec![name.to_string()];
        let mut seen = std::collections::BTreeSet::new();
        while let Some(f) = todo.pop() {
            if !seen.insert(f.clone()) {
                continue;
            }
            let body = body_of(src, &f).join("\n");
            assert!(!body.contains("prim."), "{name} reaches prim. through {f}");
            todo.extend(names.iter().filter(|g| body.contains(&format!("{g}("))).cloned());
        }
    }
}
