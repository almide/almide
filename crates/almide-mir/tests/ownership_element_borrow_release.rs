//! #3265: a var bound to a BORROW of a list element (`let s = xs[2]`,
//! `let s = cell.xs[2]`) or of a record field must hold a reference of its own
//! when the list's block can be released or written in place before the read:
//! a field-path copy-on-write (`list.pop(cell.xs)`), a rebind under a branch or
//! a loop (`xs = list.set(xs, 2, v)` drops the old block inside its bounds
//! check), or pops in a loop. Without it the borrow outlives its owner.
//!
//! Each body below must reach the certificate view with no rejected line, and
//! must carry an owned line for the bound value: a line that opens with the
//! bind's `Dup` (`a`) and ends with its scope-end drop. A/B: with the bind's
//! `Dup` disabled, every body fails — the first two on a rejected line, the
//! rest on the missing owned line (the certificate accepts those: the rebind's
//! `SetLocal` reopens the parent's line, so the dangling element read lands on
//! a live line). The fixture's record-element body walls in the strict
//! lowering, and `pop_twice` already carries an owned line for its `mut`
//! write-back, so the runtime fixture alone pins those two.

mod common;
use common::reject_reason;

const BODIES: &[&str] = &[
    "an element of a field list keeps its value across a pop of the field",
    "a list element of a field list keeps its value across a pop of the field",
    "an element keeps its value across a set of its slot",
    "an element keeps its value across a rebind under a branch",
    "an element keeps its value across pops in a loop",
    "an element keeps its value across a rebind in a loop",
    "a bound field keeps its value across a field write under a branch",
];

#[test]
fn a_bound_element_owns_its_value_across_a_release_of_its_list() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = std::fs::read_to_string(root.join("spec/lang/element_borrow_release_test.almd"))
        .expect("read the fixture");
    let certs = almide_mir::pipeline::ownership_certificates(&source).expect("the fixture must lower");
    let mut failures = Vec::new();
    for want in BODIES {
        let body: Vec<&(String, String)> = certs.iter().filter(|(n, _)| n.contains(want)).collect();
        let [(_, cert)] = body.as_slice() else {
            failures.push(format!("{want:?}: {} certificate(s), want 1", body.len()));
            continue;
        };
        let bad: Vec<String> = cert.lines().filter_map(|l| reject_reason(l).map(|r| format!("{l:?} — {r}"))).collect();
        if !bad.is_empty() {
            failures.push(format!("{want:?}: rejected line(s) {}", bad.join("; ")));
        }
        if !cert.lines().any(|l| l.starts_with('a') && l.ends_with('d')) {
            failures.push(format!("{want:?}: no owned line in {cert:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
