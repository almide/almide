//! #4631 (b1'): the per-kind bounds over hand-written frames, and the
//! negative controls — an op dropped, added or flipped is refused; a `|`
//! moved inside a branch that keeps the intervals meeting still passes
//! (the residue #4632 closes).

use super::*;

fn meets(witness: &str, events: &str) -> bool {
    [Kind::Inc, Kind::Rel].iter().all(|&k| witness_span(witness, k).meets(rc_span(events, k)))
}

const FLAT_W: &str = "iadd";
const FLAT_RC: &str = "a1 d1 d1";

#[test]
fn a_flat_frame_whose_calls_match_its_letters_passes() {
    assert!(meets(FLAT_W, FLAT_RC));
    assert_eq!(rc_span(FLAT_RC, Kind::Rel), Span { min: 2, max: Some(2) });
    assert_eq!(witness_span(FLAT_W, Kind::Inc), Span { min: 1, max: Some(1) });
}

#[test]
fn a_dropped_op_is_refused() {
    assert!(!meets("iad", FLAT_RC)); // a release dropped
    assert!(!meets("idd", FLAT_RC)); // an rc_inc dropped
}

#[test]
fn an_added_op_is_refused() {
    assert!(!meets("iadd\nid", FLAT_RC)); // a release with no call
    assert!(!meets("iaadd", FLAT_RC)); // an rc_inc with no call
}

#[test]
fn a_flipped_op_is_refused() {
    assert!(!meets("iaad", FLAT_RC)); // d -> a
    assert!(!meets("iddd", FLAT_RC)); // a -> d
}

#[test]
fn a_branch_takes_either_arm() {
    let rc = "( d1 |@3-9 d2 )";
    assert_eq!(rc_span(rc, Kind::Rel), Span { min: 1, max: Some(1) });
    assert!(meets("{id|id}", rc));
    assert!(!meets("{idd|idd}", rc));
    // a site with one arm may be skipped
    assert_eq!(rc_span("( d1 )", Kind::Rel), Span { min: 0, max: Some(1) });
}

#[test]
fn an_exit_arm_ends_the_line() {
    assert_eq!(witness_span("{idx|}id", Kind::Rel), Span { min: 1, max: Some(1) });
    assert_eq!(witness_span("{dx|}d", Kind::Rel), Span { min: 1, max: Some(1) });
}

#[test]
fn a_loop_line_claims_no_upper_bound() {
    assert_eq!(witness_span("i(d)", Kind::Rel), Span { min: 0, max: None });
}

#[test]
fn a_bar_moved_inside_a_branch_still_passes() {
    // The residue: `{i|d}` and `{id|}` claim the same per-kind interval
    // against `( | ) i… d…`; only #4632's re-derivation tells them apart.
    let rc = "( | ) d1";
    assert!(meets("{|id}", rc));
    assert!(meets("{id|}", rc));
}
