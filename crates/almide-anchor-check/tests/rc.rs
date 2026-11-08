//! The RC check (#4277) over hand-written modules: each event shape it
//! matches, and for each, the call it must refuse left over.

use almide_anchor_check::{check_rc, decode, At, RcFns, RcReport};

mod common;
use common::module;

// ── rc (#4277): recorded events against the release calls ─────────────────

fn rc(body: &str, events: &str) -> Result<RcReport, String> {
    let bytes = module("(local i32 i32)", body);
    let bodies = decode(&bytes)?;
    let mut r = RcReport::default();
    // `$alloc` (function 0) stands in for the release routine.
    let releases = [0u32].into_iter().collect();
    check_rc(&bodies, 0, RcFns { inc: None, releases: &releases, sigs: &Default::default(), judges: &[] }, &[(1, events.to_string())], &[vec![], vec![]], &mut r).map(|()| r)
}

#[test]
fn a_recorded_release_on_its_local_is_matched() -> Result<(), String> {
    let r = rc("local.get 0 call $alloc drop", "d0")?;
    assert_eq!((r.frames, r.events, r.calls), (1, 1, 1));
    // Read through a hold local, the release still names the variable.
    rc("local.get 0 local.set 1 local.get 1 call $alloc drop", "d0")?;
    Ok(())
}

#[test]
fn a_dropped_release_a_doubled_one_and_one_on_another_local_are_refused() {
    assert!(rc("local.get 0 call $alloc drop", "").unwrap_err().contains("no call []; calls with no recorded event [d@1"));
    assert!(rc("local.get 0 call $alloc drop", "d0 d0").unwrap_err().contains("no call [d0]"));
    assert!(rc("local.get 1 call $alloc drop", "d0").is_err());
    assert!(rc("local.get 0 call $alloc drop local.get 0 call $alloc drop", "d0").is_err());
}

#[test]
fn a_dead_release_is_not_live_and_a_pre_abort_release_is_an_event() -> Result<(), String> {
    rc("return local.get 0 call $alloc drop", "")?;
    // sec-b2: an abort ends only the request under `http.serve`, so a
    // release before it is no trusted class: unrecorded, it is refused.
    assert!(rc("local.get 0 call $alloc drop unreachable", "").unwrap_err().contains("calls with no recorded event [d@"));
    // Recorded, the release before the abort is matched.
    let r = rc("local.get 0 call $alloc drop unreachable", "d0")?;
    assert_eq!((r.frames, r.calls), (1, 1));
    Ok(())
}

#[test]
fn a_rebind_release_matches_a_release_or_the_judge_on_its_local() -> Result<(), String> {
    let bytes = module("(local i32 i32)", "local.get 0 call $alloc local.set 0");
    let bodies = decode(&bytes)?;
    let releases = std::collections::HashSet::new();
    let mut r = RcReport::default();
    // The call at 1 is the recorded judge: the rebind's release is inside it.
    check_rc(&bodies, 0, RcFns { inc: None, releases: &releases, sigs: &Default::default(), judges: &[At { func: 1, op: 1 }] }, &[(1, "o0".into())], &[vec![], vec![]], &mut r)?;
    assert_eq!((r.rebind_unmatched, r.judges_unmatched), (0, 0));
    // No judge and no release: the rebind is counted as one with no instruction.
    let mut r = RcReport::default();
    check_rc(&bodies, 0, RcFns { inc: None, releases: &releases, sigs: &Default::default(), judges: &[] }, &[(1, "o0".into())], &[vec![], vec![]], &mut r)?;
    assert_eq!(r.rebind_unmatched, 1);
    Ok(())
}

#[test]
fn an_rc_call_inside_an_arm_window_needs_its_event() -> Result<(), String> {
    let bytes = module("(local i32 i32)", "local.get 0 call $alloc drop");
    let bodies = decode(&bytes)?;
    let releases = [0u32].into_iter().collect();
    // With no event it is refused: the arm-internal class is retired
    // (#4278 B1).
    assert!(check_rc(&bodies, 0, RcFns { inc: None, releases: &releases, sigs: &Default::default(), judges: &[] }, &[(1, String::new())], &[vec![(1, 0, 2)], vec![]], &mut RcReport::default()).is_err());
    // The arm's own temporary, logged in its window, takes it.
    check_rc(&bodies, 0, RcFns { inc: None, releases: &releases, sigs: &Default::default(), judges: &[] }, &[(1, "d-@0-2".into())], &[vec![(1, 0, 2)], vec![]], &mut RcReport::default())?;
    // Outside the window it is an unrecorded release.
    assert!(check_rc(&bodies, 0, RcFns { inc: None, releases: &releases, sigs: &Default::default(), judges: &[] }, &[(1, String::new())], &[vec![(1, 2, 2)], vec![]], &mut RcReport::default()).is_err());
    Ok(())
}

#[test]
fn an_arm_share_out_takes_only_the_rc_inc_in_its_own_window_on_its_variable() -> Result<(), String> {
    // #4278: `$alloc` (function 0) stands in for rc_inc; the arm window is ops 0..=2.
    let bytes = module("(local i32 i32)", "local.get 0 call $alloc drop");
    let bodies = decode(&bytes)?;
    let none = std::collections::HashSet::new();
    let arms = [vec![(1, 0, 2)], vec![]];
    let run = |events: &str| check_rc(&bodies, 0, RcFns { inc: Some(0), releases: &none, sigs: &Default::default(), judges: &[] }, &[(1, events.to_string())], &arms, &mut RcReport::default());
    run("s0@0-2")?;
    // An event on another variable, in another window, or in none: refused.
    assert!(run("s1@0-2").is_err());
    assert!(run("s0@2-2").is_err());
    assert!(run("s0").is_err());
    // The +1 dropped under its event: the share-out has no call.
    let gone = module("(local i32 i32)", "local.get 0 drop");
    assert!(check_rc(&decode(&gone)?, 0, RcFns { inc: Some(0), releases: &none, sigs: &Default::default(), judges: &[] }, &[(1, "s0@0-2".into())], &arms, &mut RcReport::default())
        .unwrap_err()
        .contains("no call [s0]"));
    Ok(())
}

#[test]
fn a_variables_release_inside_its_judge_is_matched() -> Result<(), String> {
    let bytes = module("(local i32 i32)", "local.get 0 call $alloc local.set 0");
    let bodies = decode(&bytes)?;
    let releases = std::collections::HashSet::new();
    let mut r = RcReport::default();
    check_rc(&bodies, 0, RcFns { inc: None, releases: &releases, sigs: &Default::default(), judges: &[At { func: 1, op: 1 }] }, &[(1, "d0".into())], &[vec![], vec![]], &mut r)?;
    // Without the judge there is no instruction for it.
    assert!(check_rc(&bodies, 0, RcFns { inc: None, releases: &releases, sigs: &Default::default(), judges: &[] }, &[(1, "d0".into())], &[vec![], vec![]], &mut RcReport::default()).is_err());
    Ok(())
}

#[test]
fn a_write_inside_a_block_left_by_falling_through_still_defines_the_read() -> Result<(), String> {
    // The hold local is written inside a block that has no early exit.
    let r = rc("(block local.get 0 local.set 1) local.get 1 call $alloc drop", "d0")?;
    assert_eq!(r.calls, 1);
    // A branch out of the block before the write: the read may see another value.
    assert!(rc("(block local.get 0 i32.eqz br_if 0 local.get 0 local.set 1) local.get 1 call $alloc drop", "d0").is_err());
    // Written in one arm of an `if`: not on every path.
    assert!(rc("local.get 0 (if (then local.get 0 local.set 1)) local.get 1 call $alloc drop", "d0").is_err());
    Ok(())
}

#[test]
fn an_unlogged_routine_or_element_release_is_refused() -> Result<(), String> {
    let releases: std::collections::HashSet<u32> = [0u32].into_iter().collect();
    // Inside a taken routine's window with no event: refused — the
    // routine-internal class is retired (#4278 B1).
    let bytes = module("(local i32 i32)", "local.get 0 call $alloc drop");
    let bodies = decode(&bytes)?;
    let mut r = RcReport::default();
    assert!(check_rc(&bodies, 0, RcFns { inc: None, releases: &releases, sigs: &Default::default(), judges: &[] }, &[(1, String::new())], &[vec![], vec![(1, 0, 2)]], &mut r).is_err());
    // So it is inside an arm's window (the arm-internal class is retired).
    assert!(check_rc(&bodies, 0, RcFns { inc: None, releases: &releases, sigs: &Default::default(), judges: &[] }, &[(1, String::new())], &[vec![(1, 0, 2)], vec![]], &mut RcReport::default()).is_err());
    // A value loaded out of a block's slot and released, then the slot
    // rewritten: the slot's consume — an `r` event in the release's window
    // (#4278 B1). With no event it is refused: the element class is retired.
    let bytes = module("(local i32 i32)", "local.get 0 i32.load offset=12 call $alloc drop local.get 0 local.get 1 i32.store offset=12");
    let bodies = decode(&bytes)?;
    check_rc(&bodies, 0, RcFns { inc: None, releases: &releases, sigs: &Default::default(), judges: &[] }, &[(1, "r-@0-2".into())], &[vec![], vec![]], &mut RcReport::default())?;
    assert!(check_rc(&bodies, 0, RcFns { inc: None, releases: &releases, sigs: &Default::default(), judges: &[] }, &[(1, String::new())], &[vec![], vec![]], &mut RcReport::default()).is_err());
    // An `r` event outside its release's window takes nothing.
    assert!(check_rc(&bodies, 0, RcFns { inc: None, releases: &releases, sigs: &Default::default(), judges: &[] }, &[(1, "r-@3-6".into())], &[vec![], vec![]], &mut RcReport::default()).is_err());
    // Released with no store to that slot after it, the slot still holds
    // the released block: refused.
    let bytes = module("(local i32 i32)", "local.get 0 i32.load offset=12 call $alloc drop");
    let bodies = decode(&bytes)?;
    assert!(check_rc(&bodies, 0, RcFns { inc: None, releases: &releases, sigs: &Default::default(), judges: &[] }, &[(1, String::new())], &[vec![], vec![]], &mut RcReport::default()).is_err());
    Ok(())
}

#[test]
fn an_if_whose_arms_both_yield_one_local_is_that_local() -> Result<(), String> {
    let joined = "local.get 1 (if (result i32) (then local.get 0) (else local.get 0)) local.set 1 local.get 1 call $alloc drop";
    let r = rc(joined, "d0")?;
    assert_eq!(r.calls, 1);
    // Arms yielding different locals: not a read of either.
    assert!(rc(&joined.replace("(else local.get 0)", "(else local.get 1)"), "d0").is_err());
    Ok(())
}

#[test]
fn an_event_on_a_cell_holder_matches_the_release_of_its_occupant() -> Result<(), String> {
    // Local 0 holds a cell; its occupant is loaded at offset 12 and released.
    let r = rc("local.get 0 i32.load offset=12 call $alloc drop", &format!("d{}", 0xA000_0000u32))?;
    assert_eq!(r.calls, 1);
    assert!(rc("local.get 1 i32.load offset=12 call $alloc drop", &format!("d{}", 0xA000_0000u32)).is_err());
    Ok(())
}

#[test]
fn a_release_an_arm_consumes_is_matched_inside_its_window() -> Result<(), String> {
    // `$all` declares its parameter consumed (#4278 B1): the arm consuming
    // local 0.
    let r = consuming("local.get 0 call $all", "d0", &[1])?;
    assert_eq!(r.signed, 1);
    // A callee with no signature consumes nothing, even inside the window:
    // `$alloc` reads its argument and returns a constant.
    let bytes = module("(local i32 i32)", "local.get 0 call $alloc drop");
    let bodies = decode(&bytes)?;
    let none = std::collections::HashSet::new();
    assert!(check_rc(&bodies, 0, RcFns { inc: None, releases: &none, sigs: &Default::default(), judges: &[] }, &[(1, "d0".into())], &[vec![(1, 0, 2)], vec![]], &mut RcReport::default()).is_err());
    // Outside any window it is a release with no instruction.
    assert!(check_rc(&bodies, 0, RcFns { inc: None, releases: &none, sigs: &Default::default(), judges: &[] }, &[(1, "d0".into())], &[vec![], vec![]], &mut RcReport::default()).is_err());
    Ok(())
}

/// `main` (function 3) is one arm window; `signed` are the callees whose
/// checked signature declares their one parameter consumed.
fn consuming(main: &str, events: &str, signed: &[u32]) -> Result<RcReport, String> {
    let bytes = wat::parse_str(format!(
        r#"(module (memory 1)
             (func $rel (param i32))
             (func $all (param i32) local.get 0 call $rel)
             (func $some (param i32) local.get 0 (if (then local.get 0 call $rel)))
             (func $main (local i32) {main})
             (func $push_fixed (param i32) (result i32)
               local.get 0 i32.load (if (then local.get 0 return))
               local.get 0 call $rel i32.const 0)
             (func $push_old (param i32) (result i32)
               local.get 0 i32.load (if (then local.get 0 return))
               local.get 0 i32.load (if (then local.get 0 call $rel)) i32.const 0))"#
    ))
    .expect("wat");
    let bodies = decode(&bytes)?;
    let releases = [0u32].into_iter().collect();
    let sigs: std::collections::HashMap<u32, Vec<bool>> = signed.iter().map(|&g| (g, vec![true])).collect();
    let mut r = RcReport::default();
    check_rc(&bodies, 0, RcFns { inc: None, releases: &releases, sigs: &sigs, judges: &[] }, &[(3, events.to_string())], &[vec![(3, 0, usize::MAX)], vec![]], &mut r).map(|()| r)
}

/// The consumption is the callee's DECLARATION (#4278 B1), never a reading
/// of its body: without a signature, even `$all` (which releases on its
/// one path) consumes nothing.
#[test]
fn a_consumed_release_needs_a_callee_whose_signature_consumes_the_parameter() -> Result<(), String> {
    assert_eq!(consuming("local.get 0 call $all", "d0", &[1])?.signed, 1);
    assert!(consuming("local.get 0 call $all", "d0", &[]).unwrap_err().contains("declares that it consumes"));
    Ok(())
}

/// A signature is held to its body by the helper's own certificate, not
/// here: `$push_old` (the pre-#4304 shape, released on one grow path only)
/// is refused by the proven checker of its `rc-sig` record
/// (HelperSig.sig_some_paths_rejects); unsigned, it consumes nothing.
#[test]
fn an_unsigned_push_routine_consumes_nothing() -> Result<(), String> {
    assert!(consuming("local.get 0 call $push_old drop", "d0", &[]).is_err());
    assert_eq!(consuming("local.get 0 call $push_fixed drop", "d0", &[4])?.signed, 1);
    Ok(())
}

/// One taking argument accounts for one consumption: two recorded releases
/// of a variable need two calls that take it.
#[test]
fn one_taking_call_accounts_for_one_consumed_release() -> Result<(), String> {
    assert_eq!(consuming("local.get 0 call $all local.get 0 call $all", "d0 d0", &[1])?.signed, 2);
    assert!(consuming("local.get 0 call $all", "d0 d0", &[1]).is_err());
    Ok(())
}

/// Events meet calls by a maximum matching, not first fit: the alias's
/// release (read through locals 1 and 0) must not be taken by local 0's
/// event when local 0 has a release of its own.
#[test]
fn an_alias_release_does_not_steal_the_aliased_variables_call() -> Result<(), String> {
    let bytes = module("(local i32 i32)", "local.get 0 local.set 1 local.get 1 call $alloc drop local.get 0 call $alloc drop");
    let bodies = decode(&bytes)?;
    let releases: std::collections::HashSet<u32> = [0u32].into_iter().collect();
    let mut r = RcReport::default();
    check_rc(&bodies, 0, RcFns { inc: None, releases: &releases, sigs: &Default::default(), judges: &[] }, &[(1, "d0 d1".into())], &[vec![], vec![]], &mut r)?;
    assert_eq!(r.calls, 2);
    // Three releases for two calls is still refused.
    assert!(check_rc(&bodies, 0, RcFns { inc: None, releases: &releases, sigs: &Default::default(), judges: &[] }, &[(1, "d0 d1 d0".into())], &[vec![], vec![]], &mut RcReport::default()).is_err());
    Ok(())
}

/// An event takes a call outside every arm window before one inside; the
/// one left over inside the window needs the arm's own event.
#[test]
fn an_unnamed_event_takes_the_call_outside_the_arm_window_first() -> Result<(), String> {
    let bytes = module("(local i32 i32)", "local.get 0 call $alloc drop local.get 0 call $alloc drop");
    let bodies = decode(&bytes)?;
    let none = std::collections::HashSet::new();
    assert!(check_rc(&bodies, 0, RcFns { inc: Some(0), releases: &none, sigs: &Default::default(), judges: &[] }, &[(1, "a-".into())], &[vec![(1, 0, 2)], vec![]], &mut RcReport::default()).is_err());
    check_rc(&bodies, 0, RcFns { inc: Some(0), releases: &none, sigs: &Default::default(), judges: &[] }, &[(1, "a- a-@0-2".into())], &[vec![(1, 0, 2)], vec![]], &mut RcReport::default())?;
    Ok(())
}

/// A credit moved into an arm (`m<local>@<window>`) accounts for one release
/// of that variable inside the arm's window, and only there.
#[test]
fn a_move_into_an_arm_accounts_for_one_release_inside_its_window() -> Result<(), String> {
    let bytes = module("(local i32 i32)", "local.get 0 call $alloc drop");
    let bodies = decode(&bytes)?;
    let releases: std::collections::HashSet<u32> = [0u32].into_iter().collect();
    let win = [vec![(1, 0, 2)], vec![]];
    let mut r = RcReport::default();
    check_rc(&bodies, 0, RcFns { inc: None, releases: &releases, sigs: &Default::default(), judges: &[] }, &[(1, "m0@0-2".into())], &win, &mut r)?;
    assert_eq!(r.calls, 1);
    // The move names another window: the release is a frame variable's
    // credit left over inside an arm, refused.
    assert!(check_rc(&bodies, 0, RcFns { inc: None, releases: &releases, sigs: &Default::default(), judges: &[] }, &[(1, "m0@5-9".into())], &win, &mut RcReport::default()).is_err());
    // One move, two releases: one is left over.
    let bytes = module("(local i32 i32)", "local.get 0 call $alloc drop local.get 0 call $alloc drop");
    let bodies = decode(&bytes)?;
    assert!(check_rc(&bodies, 0, RcFns { inc: None, releases: &releases, sigs: &Default::default(), judges: &[] }, &[(1, "m0@0-5".into())], &[vec![(1, 0, 5)], vec![]], &mut RcReport::default()).is_err());
    Ok(())
}

/// One move, released once on each arm of an `if`: both releases are the
/// move's. Released twice on one path: the second is left over.
#[test]
fn a_move_released_once_per_path_is_accounted_on_every_path() -> Result<(), String> {
    let releases: std::collections::HashSet<u32> = [0u32].into_iter().collect();
    let bytes = module(
        "(local i32 i32)",
        "local.get 1 (if (then local.get 0 call $alloc drop) (else local.get 0 call $alloc drop))",
    );
    let bodies = decode(&bytes)?;
    check_rc(&bodies, 0, RcFns { inc: None, releases: &releases, sigs: &Default::default(), judges: &[] }, &[(1, "m0@0-20".into())], &[vec![(1, 0, 20)], vec![]], &mut RcReport::default())?;
    let bytes = module("(local i32 i32)", "local.get 1 (if (then local.get 0 call $alloc drop local.get 0 call $alloc drop))");
    let bodies = decode(&bytes)?;
    assert!(check_rc(&bodies, 0, RcFns { inc: None, releases: &releases, sigs: &Default::default(), judges: &[] }, &[(1, "m0@0-20".into())], &[vec![(1, 0, 20)], vec![]], &mut RcReport::default()).is_err());
    Ok(())
}

/// A release the recorder logged outside every branch site happens once on
/// each path: one release in each arm of an `if` is that one event's. Logged
/// inside a site (on some paths only), the other arm's release is left over.
#[test]
fn an_event_outside_every_branch_site_accounts_for_one_release_per_path() -> Result<(), String> {
    let releases: std::collections::HashSet<u32> = [0u32].into_iter().collect();
    let bytes = module(
        "(local i32 i32)",
        "local.get 1 (if (then local.get 0 call $alloc drop) (else local.get 0 call $alloc drop))",
    );
    let bodies = decode(&bytes)?;
    check_rc(&bodies, 0, RcFns { inc: None, releases: &releases, sigs: &Default::default(), judges: &[] }, &[(1, "d0".into())], &[vec![], vec![]], &mut RcReport::default())?;
    assert!(check_rc(&bodies, 0, RcFns { inc: None, releases: &releases, sigs: &Default::default(), judges: &[] }, &[(1, "( d0 )".into())], &[vec![], vec![]], &mut RcReport::default()).is_err());
    // Twice on one path is never one event's.
    let bytes = module("(local i32 i32)", "local.get 0 call $alloc drop local.get 0 call $alloc drop");
    let bodies = decode(&bytes)?;
    assert!(check_rc(&bodies, 0, RcFns { inc: None, releases: &releases, sigs: &Default::default(), judges: &[] }, &[(1, "d0".into())], &[vec![], vec![]], &mut RcReport::default()).is_err());
    Ok(())
}

/// A C-319 cell variable's `a` event meets the +1 on the block its cell holds
/// (read through the cell), one to one.
#[test]
fn a_cell_variables_event_meets_a_call_through_its_cell() -> Result<(), String> {
    let bytes = module("(local i32 i32)", "local.get 0 i32.load offset=12 local.set 1 local.get 1 call $alloc drop");
    let bodies = decode(&bytes)?;
    let none = std::collections::HashSet::new();
    check_rc(&bodies, 0, RcFns { inc: Some(0), releases: &none, sigs: &Default::default(), judges: &[] }, &[(1, "a0".into())], &[vec![], vec![]], &mut RcReport::default())?;
    assert!(check_rc(&bodies, 0, RcFns { inc: Some(0), releases: &none, sigs: &Default::default(), judges: &[] }, &[(1, "a0 a0".into())], &[vec![], vec![]], &mut RcReport::default()).is_err());
    Ok(())
}

/// Past an `if` whose two arms both leave the function, the code is never
/// run: its releases are no instruction the recorder owes an event for. An
/// arm that branches to the `if`'s own end keeps the code after it live.
#[test]
fn code_past_an_if_whose_arms_both_leave_is_dead() -> Result<(), String> {
    let releases: std::collections::HashSet<u32> = [0u32].into_iter().collect();
    let bytes = module(
        "(local i32 i32)",
        "local.get 1 (if (then local.get 0 call $alloc drop return) (else local.get 0 call $alloc drop return)) local.get 0 call $alloc drop",
    );
    let bodies = decode(&bytes)?;
    check_rc(&bodies, 0, RcFns { inc: None, releases: &releases, sigs: &Default::default(), judges: &[] }, &[(1, "( d0 d0 )".into())], &[vec![], vec![]], &mut RcReport::default())?;
    let bytes = module(
        "(local i32 i32)",
        "local.get 1 (if (then local.get 0 call $alloc drop br 0) (else local.get 0 call $alloc drop return)) local.get 0 call $alloc drop",
    );
    let bodies = decode(&bytes)?;
    assert!(check_rc(&bodies, 0, RcFns { inc: None, releases: &releases, sigs: &Default::default(), judges: &[] }, &[(1, "( d0 d0 )".into())], &[vec![], vec![]], &mut RcReport::default()).is_err());
    Ok(())
}

/// A call's result names the argument its callee may hand back as is: a +1
/// on the result is that variable's.
#[test]
fn a_call_result_names_the_argument_its_callee_may_return() -> Result<(), String> {
    let bytes = wat::parse_str(
        r#"(module (memory 1)
             (func $inc (param i32))
             (func $view (param i32) (result i32) local.get 0 i32.load (if (then local.get 0 return)) i32.const 64)
             (func $main (local i32 i32) local.get 0 call $view local.set 1 local.get 1 call $inc))"#,
    )
    .expect("wat");
    let bodies = decode(&bytes)?;
    let none = std::collections::HashSet::new();
    check_rc(&bodies, 0, RcFns { inc: Some(0), releases: &none, sigs: &Default::default(), judges: &[] }, &[(2, "a0".into())], &[vec![], vec![]], &mut RcReport::default())?;
    // A callee that never returns its argument: the result is not local 0's.
    let bytes = wat::parse_str(
        r#"(module (memory 1)
             (func $inc (param i32))
             (func $fresh (param i32) (result i32) i32.const 64)
             (func $main (local i32 i32) local.get 0 call $fresh local.set 1 local.get 1 call $inc))"#,
    )
    .expect("wat");
    let bodies = decode(&bytes)?;
    assert!(check_rc(&bodies, 0, RcFns { inc: Some(0), releases: &none, sigs: &Default::default(), judges: &[] }, &[(2, "a0".into())], &[vec![], vec![]], &mut RcReport::default()).is_err());
    Ok(())
}

/// A move into a window nested in another may be released by the enclosing
/// arm, after the inner window closes; outside both it is refused.
#[test]
fn a_move_may_be_released_by_an_enclosing_window() -> Result<(), String> {
    let bytes = module("(local i32 i32)", "nop nop nop local.get 0 call $alloc drop");
    let bodies = decode(&bytes)?;
    let releases: std::collections::HashSet<u32> = [0u32].into_iter().collect();
    check_rc(&bodies, 0, RcFns { inc: None, releases: &releases, sigs: &Default::default(), judges: &[] }, &[(1, "m0@0-2".into())], &[vec![(1, 0, 2), (1, 0, 6)], vec![]], &mut RcReport::default())?;
    assert!(check_rc(&bodies, 0, RcFns { inc: None, releases: &releases, sigs: &Default::default(), judges: &[] }, &[(1, "m0@0-2".into())], &[vec![(1, 0, 2)], vec![]], &mut RcReport::default()).is_err());
    Ok(())
}

/// Events logged in the two arms of one branch site never happen on one
/// path: the one `+1` past the join, on the value both arms name, is each
/// one's. Logged outside the site, two events need two calls.
#[test]
fn events_in_two_arms_of_one_site_share_the_call_past_the_join() -> Result<(), String> {
    let bytes = module("(local i32 i32 i32)", "local.get 2 (if (result i32) (then local.get 0) (else local.get 0)) call $alloc drop");
    let bodies = decode(&bytes)?;
    let none = std::collections::HashSet::new();
    check_rc(&bodies, 0, RcFns { inc: Some(0), releases: &none, sigs: &Default::default(), judges: &[] }, &[(1, "( a0 | a0 )".into())], &[vec![], vec![]], &mut RcReport::default())?;
    assert!(check_rc(&bodies, 0, RcFns { inc: Some(0), releases: &none, sigs: &Default::default(), judges: &[] }, &[(1, "a0 a0".into())], &[vec![], vec![]], &mut RcReport::default()).is_err());
    Ok(())
}

/// An event on a path that aborts may name an unreachable release; an
/// unreachable release with no event is never refused.
#[test]
fn an_unreachable_release_is_never_refused_and_may_carry_an_event() -> Result<(), String> {
    let releases: std::collections::HashSet<u32> = [0u32].into_iter().collect();
    let bytes = module("(local i32 i32)", "unreachable local.get 0 call $alloc drop");
    let bodies = decode(&bytes)?;
    check_rc(&bodies, 0, RcFns { inc: None, releases: &releases, sigs: &Default::default(), judges: &[] }, &[(1, "d0".into())], &[vec![], vec![]], &mut RcReport::default())?;
    check_rc(&bodies, 0, RcFns { inc: None, releases: &releases, sigs: &Default::default(), judges: &[] }, &[(1, "".into())], &[vec![], vec![]], &mut RcReport::default())?;
    Ok(())
}

/// A rebind logged in both arms of one site is one release past the join;
/// when matching first handed the exit release to one of them, that arm's
/// rebind moves to share the join's release and the exit event takes its own.
#[test]
fn rebinds_in_two_arms_share_the_release_past_the_join() -> Result<(), String> {
    let body = "local.get 1 (if (then) (else)) local.get 0 call $alloc drop local.get 0 call $alloc drop";
    rc(body, "( o0 | o0 ) d0")?;
    // Outside any site, two rebinds and an exit release are three releases.
    assert!(rc(body, "o0 o0 d0").is_err());
    Ok(())
}

/// A write in the other arm of an `if` never reaches a read in this arm:
/// the read names the value the dominating def gave the local.
#[test]
fn a_write_in_the_other_arm_does_not_hide_the_def() -> Result<(), String> {
    let body = "local.get 0 local.set 1 local.get 0 (if (then i32.const 0 local.set 1) (else local.get 1 call $alloc drop))";
    rc(body, "d0")?;
    // Past the join either write may have run: no def names the read.
    let joined = "local.get 0 local.set 1 local.get 0 (if (then i32.const 0 local.set 1)) local.get 1 call $alloc drop";
    assert!(rc(joined, "d0").is_err());
    Ok(())
}

/// A release past an anchored `if`'s join, on the value its arms chose:
/// one event in each arm, each naming its own shipped arm's value. The
/// anchor (`(@<if>-<else>`) holds the recorder's arm order to the bytes:
/// swapped, or unanchored, the pairing is refused.
#[test]
fn an_anchored_if_pairs_each_arm_event_with_its_arm_value() -> Result<(), String> {
    let bytes = module("(local i32 i32 i32)", "local.get 2 (if (result i32) (then local.get 0) (else local.get 1)) call $alloc drop");
    let bodies = decode(&bytes)?;
    let releases: std::collections::HashSet<u32> = [0u32].into_iter().collect();
    let run = |ev: &str| check_rc(&bodies, 0, RcFns { inc: None, releases: &releases, sigs: &Default::default(), judges: &[] }, &[(1, ev.into())], &[vec![], vec![]], &mut RcReport::default());
    run("(@1-3 d0 |@1-3 d1 )")?;
    assert!(run("(@1-3 d1 |@1-3 d0 )").is_err(), "arms swapped");
    assert!(run("( d0 | d1 )").is_err(), "no anchor");
    assert!(run("(@1-3 d0 |@1-4 d1 )").is_err(), "arm boundary anchored elsewhere");
    assert!(run("(@1-3 ( d0 | ) |@1-3 d1 )").is_err(), "an event on some of the arm's paths only");
    Ok(())
}

/// Two recorder sites anchored to one `if` (its arms' own, then a select
/// site over it): the pairing reads the one whose arms carry the events.
#[test]
fn a_select_site_over_an_anchored_if_pairs_too() -> Result<(), String> {
    let bytes = module("(local i32 i32 i32)", "local.get 2 (if (result i32) (then local.get 0) (else local.get 1)) call $alloc drop");
    let bodies = decode(&bytes)?;
    let releases: std::collections::HashSet<u32> = [0u32].into_iter().collect();
    let run = |ev: &str| check_rc(&bodies, 0, RcFns { inc: None, releases: &releases, sigs: &Default::default(), judges: &[] }, &[(1, ev.into())], &[vec![], vec![]], &mut RcReport::default());
    run("(@1-3 |@1-3 ) (@1-3 d0 |@1-3 d1 )")?;
    assert!(run("(@1-3 |@1-3 ) (@1-3 d1 |@1-3 d0 )").is_err());
    Ok(())
}

/// A temporary's event logged inside a window takes a call inside that
/// window, never a release elsewhere that no event of its own accounts.
#[test]
fn a_windowed_temporary_event_takes_a_call_in_its_window() -> Result<(), String> {
    rc("local.get 0 call $alloc drop", "d-@0-1")?;
    assert!(rc("local.get 0 call $alloc drop", "d-@2-3").is_err());
    Ok(())
}

// ── a release is its local's last read (#4277, rc/after.rs) ───────────────

#[test]
fn a_read_of_the_local_after_its_matched_release_is_refused() {
    // Matched one to one, but the local is read on after its release.
    assert!(rc("local.get 0 call $alloc drop local.get 0 drop", "d0").unwrap_err().contains("local 0 read at @"));
    // In one arm of an `if` only.
    assert!(rc("local.get 0 call $alloc drop i32.const 1 if local.get 0 drop end", "d0").unwrap_err().contains("after its release"));
    // Round a loop's back edge: the next iteration reads the released local.
    assert!(rc("loop local.get 0 call $alloc drop i32.const 1 br_if 0 end", "d0").unwrap_err().contains("after its release"));
    // Through `local.tee`: the tee'd local holds the released block.
    assert!(rc("i32.const 8 local.tee 1 call $alloc drop local.get 1 drop", "d-").unwrap_err().contains("local 1 read at"));
}

#[test]
fn a_release_followed_by_a_rewrite_or_no_path_to_a_read_is_the_last_read() -> Result<(), String> {
    let r = rc("local.get 0 call $alloc drop i32.const 0 local.set 0 local.get 0 drop", "d0")?;
    assert_eq!(r.last_reads, 1);
    // The read lies past a branch out: no path reaches it.
    rc("block local.get 0 call $alloc drop br 0 local.get 0 drop end", "d0")?;
    // The other arm of an `if` reads it, not the arm that released it.
    rc("i32.const 1 if local.get 0 call $alloc drop else local.get 0 drop end", "d0")?;
    // A loop that rebinds the local before its next read.
    rc("loop i32.const 0 local.set 0 local.get 0 call $alloc drop i32.const 1 br_if 0 end", "d0")?;
    Ok(())
}
