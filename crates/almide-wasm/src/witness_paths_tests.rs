//! The branch- and loop-frame unit tests, split from witness_paths.rs for
//! the file budget.

use super::*;
use Ev::*;

fn cert(log: &[Ev], n: u32) -> String {
    render(log, n).expect("renders")
}

fn bind(local: u32, obj: u32) -> Ev {
    Bind { local, obj, owner: true }
}

#[test]
fn a_straight_line_renders_flat() {
    assert_eq!(cert(&[Birth(0), Op(0, 'i'), bind(3, 0), LOp(3, 'd')], 1), "id\n");
}

#[test]
fn a_param_shared_in_one_arm_renders_as_a_whole_line_branch() {
    // i ; if { a m } else { } ; d
    let log = [Birth(0), Op(0, 'i'), bind(3, 0), Open, LOp(3, 'a'), LOp(3, 'm'), Arm, Close, LOp(3, 'd')];
    assert_eq!(cert(&log, 1), "{iamd|id}\n");
}

#[test]
fn a_local_bound_in_one_arm_is_released_at_the_epilogue_on_that_path_only() {
    let log = [Open, Birth(0), Op(0, 'i'), bind(4, 0), Arm, Close, LOp(4, 'd')];
    assert_eq!(cert(&log, 1), "{|id}\n");
}

#[test]
fn a_var_reassigned_in_one_arm_is_followed_across_the_join() {
    // var x = a; if { x = b (release a) } else { } ; epilogue releases x
    let log = [
        Birth(0), Op(0, 'i'), bind(3, 0),
        Open, LOp(3, 'd'), Birth(1), Op(1, 'i'), bind(3, 1), Arm, Close,
        LOp(3, 'd'),
    ];
    assert_eq!(cert(&log, 2), "id\n{|id}\n");
}

#[test]
fn an_abort_holding_a_prefix_of_a_returning_path_needs_no_arm() {
    // i ; if { abort } else { } ; d — the aborting `i` is a prefix of `id`.
    let log = [Birth(0), Op(0, 'i'), bind(3, 0), Open, Abort, Arm, Close, LOp(3, 'd')];
    assert_eq!(cert(&log, 1), "id\n");
}

#[test]
fn an_abort_no_returning_path_extends_ends_in_the_terminal() {
    // if { i ; abort } else { } — born on the aborting path only.
    let log = [Open, Birth(0), Op(0, 'i'), Abort, Arm, Close];
    assert_eq!(cert(&log, 1), "{|it}\n");
    // A share the returning path never takes: the aborting path stands on
    // its own, and the returning one still balances.
    let log = [Birth(0), Op(0, 'i'), bind(3, 0), Open, LOp(3, 'a'), Abort, Arm, Close, LOp(3, 'd')];
    assert_eq!(cert(&log, 1), "{iat|id}\n");
}

#[test]
fn an_exit_ends_its_path() {
    let log = [Birth(0), Op(0, 'i'), bind(3, 0), Open, LOp(3, 'd'), Exit, Arm, Close, LOp(3, 'd')];
    assert_eq!(cert(&log, 1), "id\n");
    // A missing release on the exiting arm: two paths, one leaking.
    let leak = [Birth(0), Op(0, 'i'), bind(3, 0), Open, Exit, Arm, Close, LOp(3, 'd')];
    assert_eq!(cert(&leak, 1), "{i|id}\n");
}

#[test]
fn a_nested_exit_inside_an_exiting_arm_is_its_own_flat_item() {
    // An exiting arm that folded a nested exit is two exit paths from
    // the outer entry, each flat; a folded path's items hoist to the
    // line start with the plain ops before them.
    let (h, f) = hoist("i{dx|}a{amx|}d");
    assert_eq!(h, vec!["{idx|}".to_string(), "{iaamx|}".to_string()]);
    assert_eq!(f, "iad");
    assert_eq!(flat_exits("a{bx|}c", false), vec![("ab".to_string(), false), ("ac".to_string(), false)]);
    assert_eq!(flat_exits("a{bt|}c", true), vec![("ab".to_string(), true), ("ac".to_string(), true)]);
}

#[test]
fn more_than_two_whole_paths_are_terminal_items_and_a_tail() {
    // Two sequential one-arm shares: three distinct paths, each checked
    // from 0 on its own — two as `{…x|}` items, the last as the tail.
    let site = [Open, LOp(3, 'a'), LOp(3, 'm'), Arm, Close];
    let mut log = vec![Birth(0), Op(0, 'i'), bind(3, 0)];
    log.extend(site.clone());
    log.extend(site);
    log.push(LOp(3, 'd'));
    assert_eq!(cert(&log, 1), "{iamamdx|}{iamdx|}id\n");
}

#[test]
fn several_exits_fold_into_branch_return_items() {
    // x is live across two `!` sites, each exit releasing it, with a
    // share-and-move between them: three distinct whole paths.
    let site = |arm: &[Ev]| {
        let mut v = vec![Open];
        v.extend_from_slice(arm);
        v.extend([Exit, Arm, Close]);
        v
    };
    let share = [LOp(3, 'a'), LOp(3, 'm')];
    let run = |first: &[Ev]| {
        let mut log = vec![Birth(0), Op(0, 'i'), bind(3, 0)];
        log.extend(site(first));
        log.extend(share.clone());
        log.extend(site(&[LOp(3, 'd')]));
        log.extend(share.clone());
        log.push(LOp(3, 'd'));
        cert(&log, 1)
    };
    assert_eq!(run(&[LOp(3, 'd')]), "i{dx|}am{dx|}amd\n");
    // An exit that forgets the release: its item does not reach 0.
    assert_eq!(run(&[]), "i{x|}am{dx|}amd\n");
}

#[test]
fn a_loop_body_is_an_activation_and_its_locals_live_one_iteration() {
    // xs (param, owned); for .. { let t = f(); g(xs) } ; epilogue d xs, d t
    let log = [
        Birth(0), Op(0, 'i'), bind(3, 0),
        LoopOpen,
        DecOld(4), Birth(1), Op(1, 'i'), bind(4, 1),
        LOp(3, 'a'), LOp(3, 'm'),
        LoopClose,
        LOp(3, 'd'), LOp(4, 'd'),
    ];
    // xs: frame line `id`, activation line `am`; t: its iteration `id`.
    assert_eq!(cert(&log, 2), "id\nam\n\nid\n");
}

#[test]
fn a_rebind_in_the_same_iteration_releases_the_earlier_block() {
    // The unrolled lane: two copies of `let t = f()` in one iteration.
    let log = [
        LoopOpen,
        DecOld(4), Birth(0), Op(0, 'i'), bind(4, 0),
        DecOld(4), Birth(1), Op(1, 'i'), bind(4, 1),
        LoopClose,
        LOp(4, 'd'),
    ];
    assert_eq!(cert(&log, 2), "\nid\n\nid\n");
}

#[test]
fn a_break_ends_the_iteration() {
    let log = [LoopOpen, DecOld(4), Birth(0), Op(0, 'i'), bind(4, 0), Open, Jump, Arm, Close, LoopClose];
    assert_eq!(cert(&log, 1), "\nid\n");
}

#[test]
fn an_exit_inside_a_loop_body_withdraws() {
    let log = [Birth(0), Op(0, 'i'), bind(3, 0), LoopOpen, LOp(3, 'a'), Exit, LoopClose, LOp(3, 'd')];
    assert_eq!(render(&log, 1), Err("loop-exit".into()));
}

#[test]
fn an_unbalanced_log_withdraws() {
    assert!(render(&[Open, Birth(0)], 1).is_err());
    assert!(render(&[LoopOpen, Birth(0)], 1).is_err());
}

#[test]
fn the_control_state_marks_dead_code_after_an_exit_and_a_jump() {
    let mut b = Branches::default();
    let mut log = Vec::new();
    b.open(&mut log);
    b.arm(&mut log);
    b.exit(&mut log);
    assert!(b.dead());
    b.arm(&mut log);
    assert!(!b.dead());
    b.exit(&mut log);
    b.close(&mut log);
    assert!(b.dead(), "every arm exited: the frame is dead after the join");
    let mut b = Branches::default();
    b.loop_open(&mut log);
    b.jump(&mut log);
    assert!(b.dead());
    b.loop_close(&mut log);
    assert!(!b.dead(), "the code after a loop is live");
    assert!(b.settled());
}
