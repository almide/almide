//! What a guard's acceptance rests on: an aborting arm never falls through,
//! and a value read twice is the same value only when nothing in between
//! can change it. Each case is a hand-written module the emitter does not
//! produce and the checker must refuse.

use almide_anchor_check::{check_bounds, decode, Report};

/// Function 0 is `$alloc`, function 1 the subject, function 2 `$clobber`
/// (it rewrites both globals). Locals of the subject: 0 the list block, 1
/// the index, 2 another index, 3 a block pointer, 4 a count.
fn bounds(body: &str, w: &str, a: &str) -> Result<Report, String> {
    let bytes = wat::parse_str(format!(
        r#"(module (memory 1)
             (global $gi (mut i64) (i64.const 0))
             (global $gb (mut i32) (i32.const 0))
             (func $alloc (param i32) (result i32) i32.const 64)
             (func $main (local i32 i64 i64 i32 i64) {body})
             (func $clobber i64.const 99 global.set $gi i32.const 4096 global.set $gb))"#
    ))
    .expect("wat");
    let bodies = decode(&bytes)?;
    let mut r = Report::default();
    check_bounds(&bodies, w, a, &[], &mut r).map(|()| r)
}

/// `xs[i]` read: the compare at 6, the guard's const at 8, the load at 18.
const READ: &str = "local.get 1
  local.get 0 i32.load offset=4 i32.const 8 i32.div_u i64.extend_i32_u
  i64.ge_u
  (if (then i32.const 0 drop unreachable))
  local.get 0 local.get 1 i32.wrap_i64 i32.const 8 i32.mul i32.add i64.load offset=12 drop";

#[test]
fn the_plain_guarded_read_is_accepted() -> Result<(), String> {
    assert_eq!(bounds(READ, "c0 a0", "1:8 1:18")?.bounds_operands, 1);
    Ok(())
}

// ── an aborting arm that does not abort ────────────────────────────────────

/// The arm still ends in `unreachable`, but a `br 0` before it leaves the
/// `if`: the failed compare falls through to the load.
#[test]
fn a_guard_arm_that_branches_out_before_its_abort_is_refused() {
    let body = READ.replace("drop unreachable", "drop br 0 unreachable");
    let r = bounds(&body, "c0 a0", "1:8 1:19");
    assert!(r.is_err(), "accepted a guard whose arm leaves by `br 0`: {r:?}");
}

/// The same with a conditional branch.
#[test]
fn a_guard_arm_that_may_branch_out_before_its_abort_is_refused() {
    let body = READ.replace("drop unreachable", "drop i32.const 1 br_if 0 unreachable");
    let r = bounds(&body, "c0 a0", "1:8 1:20");
    assert!(r.is_err(), "accepted a guard whose arm may leave by `br_if 0`: {r:?}");
}

/// Between the check and the access, an `if` shaped like an aborting arm
/// rewrites the index and branches out before its `unreachable`: the write
/// reaches the load.
#[test]
fn an_index_written_in_an_arm_that_branches_out_before_its_abort_is_refused() {
    let body = READ.replace(
        "(if (then i32.const 0 drop unreachable))",
        "(if (then i32.const 0 drop unreachable))
         local.get 2 i32.wrap_i64 (if (then i64.const 99 local.set 1 br 0 unreachable))",
    );
    // 12 local.get 2, 13 wrap, 14 if, 15 const, 16 local.set, 17 br, 18 unreachable, 19 end, load 26.
    let r = bounds(&body, "c0 a0", "1:8 1:26");
    assert!(r.is_err(), "accepted an index rewritten on a path that reaches the load: {r:?}");
}

// ── a global read twice across a call ──────────────────────────────────────

/// The index lives in a global; nothing in between: whatever the checker
/// says here is the control for the next case.
#[test]
fn a_global_index_with_nothing_between_is_judged() {
    let body = READ.replace("local.get 1", "global.get $gi");
    let r = bounds(&body, "c0 a0", "1:8 1:18");
    eprintln!("CONTROL global index, nothing between: {r:?}");
}

/// The index lives in a global and a call between the check and the access
/// rewrites it.
#[test]
fn a_global_index_read_across_a_call_is_refused() {
    let body = READ
        .replace("local.get 1", "global.get $gi")
        .replace("(if (then i32.const 0 drop unreachable))", "(if (then i32.const 0 drop unreachable)) call $clobber");
    let r = bounds(&body, "c0 a0", "1:8 1:19");
    assert!(r.is_err(), "accepted a global index a call between may rewrite: {r:?}");
}

/// The list lives in a global; nothing in between (the control).
#[test]
fn a_global_list_with_nothing_between_is_judged() {
    let body = READ.replace("local.get 0", "global.get $gb");
    let r = bounds(&body, "c0 a0", "1:8 1:18");
    eprintln!("CONTROL global list, nothing between: {r:?}");
}

/// The list lives in a global and a call between the check and the access
/// rebinds it.
#[test]
fn a_global_list_read_across_a_call_is_refused() {
    let body = READ
        .replace("local.get 0", "global.get $gb")
        .replace("(if (then i32.const 0 drop unreachable))", "(if (then i32.const 0 drop unreachable)) call $clobber");
    let r = bounds(&body, "c0 a0", "1:8 1:19");
    assert!(r.is_err(), "accepted a global list a call between may rebind: {r:?}");
}
