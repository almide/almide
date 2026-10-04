//! #3270: a `mut` param written inside a branch arm. `lower_place_mutation`
//! took the param's copy-on-write `Dup` inside the arm, the arm's teardown
//! dropped it, and the var still named the dropped copy after the join: the
//! read after the `if` was a use after free on the then path and a value the
//! else path never computed. The copy is now taken at function entry.
//!
//! Each helper of the spec fixture must pass `verify_ownership` and reach the
//! certificate view with no rejected line. The certificate alone does not see
//! the unfixed shape: the read probes only a line born by an `i`, and the
//! copy's line opens with the `Dup`'s `a` (A/B: unfixed, all three helpers
//! fail `verify_ownership` and certify).

mod common;
use common::reject_reason;

const FNS: &[&str] = &["set_at", "set_if", "bump_if"];

#[test]
fn a_mut_param_written_in_an_arm_is_copied_before_the_branch() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = std::fs::read_to_string(root.join("spec/lang/mut_param_branch_write_test.almd"))
        .expect("read the fixture");
    let verdicts = almide_mir::pipeline::ownership_verdicts(&source).expect("the fixture must lower");
    let mut failures = Vec::new();
    for want in FNS {
        let found: Vec<&(String, String, bool)> = verdicts.iter().filter(|(n, _, _)| n == want).collect();
        let [(_, cert, verified)] = found.as_slice() else {
            failures.push(format!("{want}: {} certificate(s), want 1", found.len()));
            continue;
        };
        let bad: Vec<String> = cert.lines().filter_map(|l| reject_reason(l).map(|r| format!("{l:?} — {r}"))).collect();
        if !bad.is_empty() {
            failures.push(format!("{want}: rejected line(s) {}", bad.join("; ")));
        }
        if !verified {
            failures.push(format!("{want}: verify_ownership rejects it"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
