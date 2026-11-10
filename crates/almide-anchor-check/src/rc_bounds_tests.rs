//! #4631 (b1-inc): the bounds over hand-written frames, and the negative
//! controls — an rc_inc added, dropped, or flipped with a release is
//! refused; a `|` moved inside a branch still passes (the residue #4632
//! closes).

use super::*;

fn inc_meets(witness: &str, events: &str) -> bool {
    witness_span(witness, Kind::Inc).meets(rc_span(events, Kind::Inc))
}

const W: &str = "iadd";
const RC: &str = "a1 d1 d1";

#[test]
fn a_flat_frame_whose_rc_incs_match_passes() {
    assert!(inc_meets(W, RC));
    assert_eq!(rc_span(RC, Kind::Inc), Span { min: 1, max: Some(1) });
    assert_eq!(witness_span(W, Kind::Inc), Span { min: 1, max: Some(1) });
}

#[test]
fn an_added_rc_inc_is_refused() {
    assert!(!inc_meets("iaadd", RC));
    assert!(!inc_meets("iadd\nad", RC));
    assert!(!inc_meets("iadd\nisd", RC)); // a share-out is an rc_inc too
}

#[test]
fn a_dropped_rc_inc_is_refused() {
    assert!(!inc_meets("id", RC));
}

#[test]
fn an_rc_inc_flipped_to_a_release_is_refused() {
    // a -> d: the bounds refuse it (one rc_inc call, none claimed).
    assert!(!inc_meets("iddd", RC));
    // d -> a: the bounds refuse it (two claimed, one call).
    assert!(!inc_meets("iaad", RC));
}

#[test]
fn a_branch_takes_either_arm() {
    let rc = "( a1 |@3-9 a2 ) d1";
    assert_eq!(rc_span(rc, Kind::Inc), Span { min: 1, max: Some(1) });
    assert!(inc_meets("{iadd|iadd}", rc));
    assert!(!inc_meets("{iaadd|iaadd}", rc));
    // a site with one arm may be skipped
    assert_eq!(rc_span("( a1 )", Kind::Inc), Span { min: 0, max: Some(1) });
}

#[test]
fn an_exit_arm_ends_the_line() {
    assert_eq!(witness_span("{iadx|}ad", Kind::Inc), Span { min: 1, max: Some(1) });
}

#[test]
fn a_loop_line_claims_no_upper_bound() {
    assert_eq!(witness_span("i(ad)d", Kind::Inc), Span { min: 0, max: None });
}

#[test]
fn a_bar_moved_inside_a_branch_still_passes() {
    // The residue: `{|iad}` and `{iad|}` claim the same interval.
    let rc = "( | ) a1";
    assert!(inc_meets("{|iadd}", rc));
    assert!(inc_meets("{iadd|}", rc));
}

#[test]
fn the_release_family_is_spanned_but_not_enforced() {
    // `o` on a local that held nothing has no witness letter: the release
    // spans disagree on a correct frame, which is why only rc_inc is held.
    assert_eq!(rc_span("o1 d1", Kind::Rel), Span { min: 2, max: Some(2) });
    assert_eq!(witness_span("id", Kind::Rel), Span { min: 1, max: Some(1) });
}
