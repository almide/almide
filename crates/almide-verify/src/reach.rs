//! The transitive capability checker — the Rust mirror of
//! `proofs/CapabilityReach.v`'s `check_prog_cert`.
//!
//! The witness is a call graph, `;`-separated functions, each
//! `<declared ids>|<direct ids>|<callee indices>`. For every function the
//! TRANSITIVE reach (its own direct capabilities plus every reachable
//! callee's) must be a subset of what it declares.
//!
//! The proof computes the reach as `reaches prog (length prog) i`, a fuelled
//! re-expansion that is exponential in the branching factor. Only its SET
//! matters to `subset_check`, and that set is exactly the direct capabilities
//! of every in-range function reachable from `i`: fuel = the function count
//! covers every simple path, and an out-of-range callee index looks up the
//! empty node (no capabilities, no callees). A breadth-first walk computes the
//! same set in linear time; the differential leg of `proofs/gate.sh` holds the
//! verdicts equal.

use crate::nat::{pnats, split_bar, split_semi, subset, Nat};

/// `Fn` — one call-graph node.
struct Node {
    declared: Vec<Nat>,
    direct: Vec<Nat>,
    callees: Vec<Nat>,
}

/// `parse_fn`: split off `declared`, then `direct`; the rest is the callees.
fn parse_node(segment: &[u8]) -> Node {
    let (declared, rest) = split_bar(segment);
    let (direct, callees) = split_bar(rest);
    Node { declared: pnats(declared), direct: pnats(direct), callees: pnats(callees) }
}

/// The direct capabilities of every function reachable from `start`
/// (inclusive), as the multiset `reaches` produces — order and repeats are
/// irrelevant to the subset test that consumes it.
fn reach(prog: &[Node], start: usize) -> Vec<Nat> {
    let mut seen = vec![false; prog.len()];
    let mut queue = vec![start];
    seen[start] = true;
    let mut caps = Vec::new();
    while let Some(i) = queue.pop() {
        caps.extend(prog[i].direct.iter().cloned());
        for callee in prog[i].callees.iter().filter_map(Nat::as_index) {
            if callee < prog.len() && !seen[callee] {
                seen[callee] = true;
                queue.push(callee);
            }
        }
    }
    caps
}

/// `check_prog_cert` = `prog_within (parse_prog s)`.
pub(crate) fn check_prog_cert(witness: &[u8]) -> bool {
    let prog: Vec<Node> = split_semi(witness).into_iter().map(parse_node).collect();
    (0..prog.len()).all(|i| subset(&prog[i].declared, &reach(&prog, i)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_out_of_range_callee_reaches_nothing() {
        // The proof's `lookup` returns the empty node for index 9.
        assert!(check_prog_cert(b"||9"));
        assert!(check_prog_cert(b"0|0|99999999999999999999999999"));
        assert!(!check_prog_cert(b"|0|9"));
    }

    #[test]
    fn a_callee_capability_is_charged_to_the_caller() {
        assert!(check_prog_cert(b"||;0|0|"));
        assert!(!check_prog_cert(b"||1;0|0|"));
    }

    #[test]
    fn a_cycle_terminates_and_folds_both_sides() {
        assert!(check_prog_cert(b"0|0|1;0||0"));
        assert!(!check_prog_cert(b"|0|1;0||0"));
    }
}
