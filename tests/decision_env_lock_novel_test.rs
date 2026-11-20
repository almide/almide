//! Novel-item test for the env.* rows of the decision-site ledger
//! (proofs/decision-sites.toml): the switch lock decides from each switch's
//! DECLARED scope. Its own test binary: it sets build-changing switches in its
//! process environment, which no other test may inherit.
#[path = "support/novel.rs"]
mod novel;

/// env.*: the switch lock decides from each switch's DECLARED scope. Every
/// registered switch whose scope changes the build — enumerated from the
/// registry at run time, so one registered later is in it — is read as unset
/// when inherited under the lock; and a fresh name nobody registered is
/// refused outright (a read of it panics). Control: unlocked, each is read.
#[test]
fn novel_every_build_changing_switch_is_ignored_under_the_lock() {
    use almide_base::env;
    let held: Vec<&env::Switch> = env::SWITCHES.iter().filter(|s| s.scope.changes_the_build()).collect();
    assert!(held.len() > 20, "only {} build-changing switches", held.len());
    let read = |sw: &env::Switch| match sw.kind {
        env::Kind::Flag => env::flag(sw.name),
        env::Kind::Value => env::var(sw.name).is_some(),
    };
    for sw in &held {
        // SAFETY: this test binary runs this one test; no other thread reads
        // the environment while it is set.
        unsafe { std::env::set_var(sw.name, "1") };
    }
    // Control: unlocked, every inherited switch is honoured.
    let unread: Vec<&str> = held.iter().filter(|sw| !read(sw)).map(|sw| sw.name).collect();
    assert!(unread.is_empty(), "control: unlocked, these were not read: {unread:?}");
    env::lock(Vec::new());
    let honoured: Vec<&str> = held.iter().filter(|sw| read(sw)).map(|sw| sw.name).collect();
    assert!(honoured.is_empty(), "under the lock these inherited switches were still honoured: {honoured:?}");
    let fresh = novel::switch_name();
    unsafe { std::env::set_var(&fresh, "1") };
    let r = std::panic::catch_unwind(|| env::flag(&fresh));
    assert!(r.is_err(), "an unregistered switch {fresh} was read instead of refused");
}
