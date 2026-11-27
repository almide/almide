//! #1961: a region reclaim `w-@<from>-<to>` takes no RC call and is held to
//! the shipped `RegionRestore` in its own window — the class heads copied
//! back (`memory.copy`), the save block read back into the bump pointer
//! (`local.get L; i32.load; global.set`), then that local handed to a call
//! that is no RC call (`local.get L; call $free`). Each control below drops
//! one of those, or moves the window off them, and must be refused.

use almide_anchor_check::{check_rc, decode, RcFns, RcReport};

/// Function 0 is `$alloc` (it stands in for the release routine), 1 is
/// `$free`, 2 is the subject, its body `body`; global 0 is the bump pointer.
fn module(body: &str) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(module (memory 1)
             (global $heap (mut i32) (i32.const 0))
             (func $alloc (param i32) (result i32) i32.const 64)
             (func $free (param i32))
             (func $main (local i32 i32) {body}))"#
    ))
    .expect("wat")
}

fn rc(body: &str, events: &str) -> Result<RcReport, String> {
    let bytes = module(body);
    let bodies = decode(&bytes)?;
    let releases = [0u32].into_iter().collect();
    let mut r = RcReport::default();
    check_rc(&bodies, 0, RcFns { inc: None, releases: &releases, sigs: &Default::default(), judges: &[], alloc: None }, &[(2, events.to_string())], &[vec![], vec![]], &mut r).map(|()| r)
}

/// The restore region.rs emits, in its order: ops 0-5 the heads copied back,
/// 6-8 the bump pointer rewound out of the save block, 9-10 the block freed.
const RESTORE: &str = "i32.const 0 local.get 0 i32.const 4 i32.add i32.const 64 memory.copy \
                       local.get 0 i32.load global.set $heap \
                       local.get 0 call $free";

#[test]
fn a_reclaim_is_held_to_the_restore_in_its_window() -> Result<(), String> {
    let r = rc(RESTORE, "w-@0-12")?;
    assert_eq!(r.frames, 1);
    // The producer's window: its first instruction to the save block's free,
    // exactly — the shape witness_bundle.rs renders from the restore's span.
    rc(RESTORE, "w-@0-10")?;
    // Inside a body: the restore after other code, the window exactly on it.
    rc(&format!("nop nop {RESTORE} nop"), "w-@2-12")?;
    Ok(())
}

#[test]
fn a_reclaim_whose_window_misses_part_of_the_restore_is_refused() {
    // The window starts after the heads' copy.
    assert!(rc(RESTORE, "w-@6-12").is_err());
    // The window ends before the save block is freed.
    assert!(rc(RESTORE, "w-@0-9").is_err());
    // No window, or one that did not ship (witness_bundle.rs renders `@?`).
    assert!(rc(RESTORE, "w-").is_err());
    assert!(rc(RESTORE, "w-@?").is_err());
}

#[test]
fn a_reclaim_over_a_patched_restore_is_refused() {
    // The heads are not copied back.
    let no_copy = "local.get 0 i32.load global.set $heap local.get 0 call $free";
    assert!(rc(no_copy, "w-@0-12").is_err());
    // The bump pointer is not rewound.
    let no_rewind = "i32.const 0 local.get 0 i32.const 4 i32.add i32.const 64 memory.copy \
                     local.get 0 i32.load drop local.get 0 call $free";
    assert!(rc(no_rewind, "w-@0-12").is_err());
    // The rewind reads another local than the one freed.
    let other = "i32.const 0 local.get 0 i32.const 4 i32.add i32.const 64 memory.copy \
                 local.get 1 i32.load global.set $heap local.get 0 call $free";
    assert!(rc(other, "w-@0-12").is_err());
    // Nothing is restored at all.
    assert!(rc("nop", "w-@0-1").is_err());
}

#[test]
fn a_reclaim_never_takes_a_release_call() {
    // The save block handed to a RELEASE routine is an RC call: the reclaim
    // does not take it, and the call is left over.
    let released = "i32.const 0 local.get 0 i32.const 4 i32.add i32.const 64 memory.copy \
                    local.get 0 i32.load global.set $heap local.get 0 call $alloc drop";
    assert!(rc(released, "w-@0-13").is_err());
}
