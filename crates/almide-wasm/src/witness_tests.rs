//! The recorder's unit tests, split from witness.rs for the file budget.

use super::*;

#[test]
fn a_fresh_bind_and_its_epilogue_release_balance() {
    let mut w = WitnessRecorder::new();
    w.bind_fresh(3);
    assert!(w.dec_local(3));
    assert_eq!(w.certificate(), "id\n");
    assert!(balanced(&w.certificate()));
}

#[test]
fn an_alias_bind_shares_the_source_object_and_both_release() {
    // let a = [1]; let b = a — one object, streams to the canonical
    // shared shape the incumbent's tests pin ("iadd").
    let mut w = WitnessRecorder::new();
    w.bind_fresh(3);
    assert!(w.bind_alias(4, 3));
    assert!(w.dec_local(3));
    assert!(w.dec_local(4));
    assert_eq!(w.certificate(), "iadd\n");
    assert!(balanced(&w.certificate()));
}

#[test]
fn a_var_argument_shares_then_moves_into_the_callee() {
    // let a = [1]; f(a) — the site's rc_inc + the credit's move.
    let mut w = WitnessRecorder::new();
    w.bind_fresh(3);
    assert!(w.arg_share_move(3));
    assert!(w.dec_local(3));
    assert_eq!(w.certificate(), "iamd\n");
    assert!(balanced(&w.certificate()));
}

#[test]
fn a_temporary_argument_and_an_owned_tail_each_move_one_credit() {
    let mut w = WitnessRecorder::new();
    w.temp_move();
    w.tail_owned_move();
    assert_eq!(w.certificate(), "im\nim\n");
    assert!(balanced(&w.certificate()));
}

#[test]
fn an_over_release_fails_the_balance_mirror() {
    assert!(!balanced("idd\n"));
    assert!(!balanced("ia\n"));
    assert!(balanced("iadd\nid\n"));
}

#[test]
fn a_decline_withdraws_the_certificate_and_a_poison_outranks_it() {
    let mut w = WitnessRecorder::new();
    w.bind_fresh(3);
    w.decline("module-result:view");
    w.decline("second-reason-loses");
    assert_eq!(w.certificate(), "!decline:module-result:view\n");
    w.poison();
    assert_eq!(w.certificate(), "!poison\n");
}

/// #3259: a read of a block after its release is the `b` probe on its
/// owned line, which the checker rejects. This is the structural poisoned
/// certificate: an owned param read, released early, then read again (the
/// Var route of a borrowed call argument, a field read, a match subject).
#[test]
fn a_read_after_the_release_is_probed_and_rejected() {
    let mut w = WitnessRecorder::new();
    w.param_owned(2);
    w.read(2);
    assert!(w.dec_local(2));
    w.read(2);
    assert_eq!(w.certificate(), "ibdb\n");
    assert!(!balanced(&w.certificate()));
    assert_eq!(w.certificate(), include_str!("../../../proofs/poisoned-certs/3259-structural-read-after-free.cert"));
}

/// #3259: a read is probed only on a line born by `i`. A borrowed param
/// sits at 0 while the caller holds it, and a loop activation of a block
/// bound outside the loop starts from 0 too, so neither takes a probe.
#[test]
fn a_read_of_a_block_this_line_does_not_own_is_not_probed() {
    let mut lent = WitnessRecorder::new();
    lent.param_lent(2);
    lent.read(2);
    let mut unread = WitnessRecorder::new();
    unread.param_lent(2);
    assert_eq!(lent.certificate(), unread.certificate());

    let mut looped = WitnessRecorder::new();
    looped.bind_fresh(3);
    looped.read(3);
    looped.loop_open();
    looped.read(3);
    looped.loop_close();
    assert!(looped.dec_local(3));
    assert_eq!(looped.certificate(), "ibd\n");
    assert!(balanced(&looped.certificate()));
}
