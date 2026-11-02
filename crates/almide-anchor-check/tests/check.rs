//! The checker over hand-written modules: each shape it accepts, and for
//! each, the one-instruction change it must refuse.

use almide_anchor_check::{check_bounds, check_bounds_held, check_init, decode, At, Report};

mod common;
use common::module;

fn init(body: &str, w: &str, a: &str) -> Result<Report, String> {
    let bytes = module("(local i32 i32 i64 i64)", body);
    let bodies = decode(&bytes)?;
    let mut r = Report::default();
    check_init(&bodies, 0, w, a, &mut r).map(|()| r)
}

/// Locals: 0 the list block, 1 the index, 2 another index, 3 a block or
/// payload pointer, 4 a hoisted count.
fn bounds(body: &str, w: &str, a: &str) -> Result<Report, String> {
    bounds_judged(body, w, a, &[])
}

fn bounds_judged(body: &str, w: &str, a: &str, judges: &[usize]) -> Result<Report, String> {
    let bytes = module("(local i32 i64 i64 i32 i64)", body);
    let bodies = decode(&bytes)?;
    let mut r = Report::default();
    let js: Vec<At> = judges.iter().map(|&op| At { func: 1, op }).collect();
    check_bounds(&bodies, w, a, &js, &mut r).map(|()| r)
}

// ── init: a fixed site ─────────────────────────────────────────────────────

const SITE: &str = "i32.const 16 call $alloc local.set 0
  local.get 0 i64.const 7 i64.store offset=12
  local.get 0 i64.const 8 i64.store offset=20";
const SITE_W: &str = "h12 w8 n2 r0 s0 s1";
const SITE_A: &str = "1:2 1:5 1:8";

#[test]
fn a_site_whose_stores_fill_its_slots_is_accepted() -> Result<(), String> {
    let r = init(SITE, SITE_W, SITE_A)?;
    assert_eq!((r.init_sites, r.init_stores), (1, 2));
    Ok(())
}

#[test]
fn a_store_at_the_wrong_offset_is_refused() {
    let body = SITE.replace("offset=20", "offset=28");
    assert!(init(&body, SITE_W, SITE_A)
        .unwrap_err()
        .contains("offset 28"));
}

#[test]
fn a_store_of_the_wrong_width_is_refused() {
    let body = SITE.replace(
        "i64.const 8 i64.store offset=20",
        "i32.const 8 i32.store offset=20",
    );
    assert!(init(&body, SITE_W, SITE_A).is_err());
}

#[test]
fn a_store_through_another_local_is_refused() {
    let body = SITE.replace("local.get 0 i64.const 8", "local.get 1 i64.const 8");
    assert!(init(&body, SITE_W, SITE_A).unwrap_err().contains("address"));
}

#[test]
fn an_allocation_of_another_size_is_refused() {
    let body = SITE.replace("i32.const 16 call", "i32.const 8 call");
    assert!(init(&body, SITE_W, SITE_A)
        .unwrap_err()
        .contains("allocation"));
}

#[test]
fn the_blocks_local_overwritten_in_the_window_is_refused() {
    let body = SITE.replace(
        "local.get 0 i64.const 8",
        "i32.const 0 local.set 0 local.get 0 i64.const 8",
    );
    assert!(init(&body, SITE_W, "1:2 1:5 1:10").is_err());
}

#[test]
fn an_unrecorded_store_into_the_block_is_refused() {
    let body = SITE.replace(
        "local.get 0 i64.const 8",
        "local.get 0 i64.const 9 i64.store offset=36 local.get 0 i64.const 8",
    );
    assert!(init(&body, SITE_W, "1:2 1:5 1:11")
        .unwrap_err()
        .contains("unrecorded"));
}

#[test]
fn a_store_behind_a_branch_is_refused() {
    let body = SITE.replace(
        "local.get 0 i64.const 8 i64.store offset=20",
        "(block local.get 1 br_if 0 local.get 0 i64.const 8 i64.store offset=20)",
    );
    assert!(init(&body, SITE_W, "1:2 1:5 1:12").is_err());
}

#[test]
fn an_anchor_on_another_instruction_is_refused() {
    assert!(init(SITE, SITE_W, "1:2 1:4 1:8").is_err());
}

// ── init: a loop fill ──────────────────────────────────────────────────────

fn fill(step: i32) -> String {
    format!(
        "local.get 2 i64.const 8 i64.mul i32.wrap_i64 call $alloc local.set 0
         local.get 0 i32.const 12 i32.add local.set 1
         (block (loop
           local.get 2 i64.const 0 i64.le_s br_if 1
           local.get 1 local.get 3 i64.store
           local.get 1 i32.const {step} i32.add local.set 1
           local.get 2 i64.const 1 i64.sub local.set 2
           br 0))
         local.get 0 drop"
    )
}
const FILL_W: &str = "h12 w8 v2 r0 f2 t8";
const FILL_A: &str = "1:5 1:30";

#[test]
fn a_countdown_fill_is_accepted() -> Result<(), String> {
    let r = init(&fill(8), FILL_W, FILL_A)?;
    assert_eq!(r.fill_sites, 1);
    Ok(())
}

#[test]
fn a_cursor_that_advances_past_a_slot_is_refused() {
    assert!(init(&fill(16), FILL_W, FILL_A)
        .unwrap_err()
        .contains("tail"));
}

#[test]
fn a_fill_whose_counter_is_not_the_count_is_refused() {
    assert!(init(&fill(8), "h12 w8 v3 r0 f2 t8", FILL_A).is_err());
}

// ── bounds ─────────────────────────────────────────────────────────────────

/// `xs[i]` read: the compare at 6, the guard's const at 8, the load at 18.
const READ: &str = "local.get 1
  local.get 0 i32.load offset=4 i32.const 8 i32.div_u i64.extend_i32_u
  i64.ge_u
  (if (then i32.const 0 drop unreachable))
  local.get 0 local.get 1 i32.wrap_i64 i32.const 8 i32.mul i32.add i64.load offset=12 drop";

#[test]
fn a_guarded_read_is_accepted() -> Result<(), String> {
    let r = bounds(READ, "c0 a0", "1:8 1:18")?;
    assert_eq!((r.bounds_checks, r.bounds_accesses, r.bounds_operands), (1, 1, 1));
    Ok(())
}

#[test]
fn a_guard_that_does_not_abort_is_refused() {
    let body = READ.replace("drop unreachable", "drop");
    assert!(bounds(&body, "c0 a0", "1:8 1:17")
        .unwrap_err()
        .contains("abort"));
}

#[test]
fn a_guard_with_another_compare_is_refused() {
    let body = READ.replace("i64.ge_u", "i64.ge_s");
    assert!(bounds(&body, "c0 a0", "1:8 1:18").is_err());
}

#[test]
fn an_access_anchored_off_its_load_is_refused() {
    assert!(bounds(READ, "c0 a0", "1:8 1:17").is_err());
}

#[test]
fn a_check_after_its_access_is_refused() {
    let body = "local.get 0 local.get 1 i32.wrap_i64 i32.const 8 i32.mul i32.add i64.load offset=12 drop
      local.get 1 local.get 0 i32.load offset=4 i32.const 8 i32.div_u i64.extend_i32_u i64.ge_u
      (if (then i32.const 0 drop unreachable))";
    assert!(bounds(body, "c0 a0", "1:16 1:6")
        .unwrap_err()
        .contains("not after"));
}

#[test]
fn a_kill_carries_no_anchor_and_an_unlocated_event_is_counted() -> Result<(), String> {
    let r = bounds(READ, "c0 a0 s0;c0 a0", "1:8 1:18 -;- -")?;
    assert_eq!(r.bounds_unlocated, 2);
    assert!(bounds(READ, "c0 a0 s0", "1:8 1:18 1:18").is_err());
    Ok(())
}

// ── bounds: the guard's operands ───────────────────────────────────────────

#[test]
fn a_guard_on_another_index_is_refused() {
    let body = READ.replace("local.get 0 local.get 1 i32.wrap_i64", "local.get 0 local.get 2 i32.wrap_i64");
    assert!(bounds(&body, "c0 a0", "1:8 1:18").unwrap_err().contains("index"));
}

#[test]
fn a_guard_on_a_constant_is_refused() {
    let body = READ.replacen("local.get 1\n", "i64.const 0\n", 1);
    assert!(bounds(&body, "c0 a0", "1:8 1:18").is_err());
}

#[test]
fn a_count_in_the_wrong_slot_size_is_refused() {
    let body = READ.replace("i32.const 8 i32.div_u", "i32.const 4 i32.div_u");
    assert!(bounds(&body, "c0 a0", "1:8 1:18").unwrap_err().contains("slots"));
}

#[test]
fn an_access_wider_than_its_slot_is_refused() {
    let body = READ
        .replace("i32.const 8 i32.div_u", "i32.const 4 i32.div_u")
        .replace("i32.const 8 i32.mul", "i32.const 4 i32.mul");
    assert!(bounds(&body, "c0 a0", "1:8 1:18").unwrap_err().contains("wide"));
}

#[test]
fn an_access_past_the_slot_by_its_offset_is_refused() {
    let body = READ.replace("i64.load offset=12", "i64.load offset=16");
    assert!(bounds(&body, "c0 a0", "1:8 1:18").unwrap_err().contains("whole slot"));
}

#[test]
fn the_count_of_another_list_is_refused() {
    let body = READ.replace(
        "local.get 0 i32.load offset=4",
        "local.get 3 i32.load offset=4",
    );
    assert!(bounds(&body, "c0 a0", "1:8 1:18").is_err());
}

#[test]
fn the_index_changed_between_check_and_access_is_refused() {
    let body = READ.replace(
        "(if (then i32.const 0 drop unreachable))",
        "(if (then i32.const 0 drop unreachable)) i64.const 99 local.set 1",
    );
    assert!(bounds(&body, "c0 a0", "1:8 1:20").is_err());
}

#[test]
fn the_list_rebound_between_check_and_access_is_refused() {
    let body = READ.replace(
        "(if (then i32.const 0 drop unreachable))",
        "(if (then i32.const 0 drop unreachable)) local.get 3 local.set 0",
    );
    assert!(bounds(&body, "c0 a0", "1:8 1:20").is_err());
}

#[test]
fn the_list_handed_to_a_call_between_check_and_access_is_refused() {
    let body = READ.replace(
        "(if (then i32.const 0 drop unreachable))",
        "(if (then i32.const 0 drop unreachable)) local.get 0 call $alloc drop",
    );
    assert!(bounds(&body, "c0 a0", "1:8 1:21").unwrap_err().contains("handed"));
}

#[test]
fn a_count_word_stored_between_check_and_access_is_refused() {
    let body = READ.replace(
        "(if (then i32.const 0 drop unreachable))",
        "(if (then i32.const 0 drop unreachable)) local.get 0 i32.const 0 i32.store offset=4",
    );
    assert!(bounds(&body, "c0 a0", "1:8 1:21").unwrap_err().contains("count"));
}

#[test]
fn a_check_behind_a_branch_does_not_cover_the_access() {
    // The check sits in a block a branch can skip; the access is after it.
    let body = "(block local.get 2 i32.wrap_i64 br_if 0
        local.get 1 local.get 0 i32.load offset=4 i32.const 8 i32.div_u i64.extend_i32_u
        i64.ge_u (if (then i32.const 0 drop unreachable)))
      local.get 0 local.get 1 i32.wrap_i64 i32.const 8 i32.mul i32.add i64.load offset=12 drop";
    assert!(bounds(body, "c0 a0", "1:12 1:23").unwrap_err().contains("dominate"));
}

/// A counting loop: the count hoisted into local 4, the payload pointer
/// into local 3, the check at 18 (const 20), the load at 30.
const LOOP: &str = "local.get 0 i32.load offset=4 i32.const 8 i32.div_u i64.extend_i32_u local.set 4
  local.get 0 i32.const 12 i32.add local.set 3
  (block (loop
    local.get 1 local.get 4 i64.ge_u br_if 1
    local.get 1 local.get 4 i64.ge_u (if (then i32.const 0 drop unreachable))
    local.get 3 local.get 1 i32.wrap_i64 i32.const 8 i32.mul i32.add f64.load drop
    local.get 1 i64.const 1 i64.add local.set 1
    br 0))";

#[test]
fn a_hoisted_count_and_payload_pointer_are_accepted() -> Result<(), String> {
    let r = bounds(LOOP, "c0 a0", "1:20 1:30")?;
    assert_eq!(r.bounds_operands, 1);
    Ok(())
}

#[test]
fn a_list_rebound_inside_the_loop_of_its_hoisted_count_is_refused() {
    let body = LOOP.replace("br 0))", "local.get 3 local.set 0 br 0))");
    assert!(bounds(&body, "c0 a0", "1:20 1:30").unwrap_err().contains("changes the list"));
}

#[test]
fn a_hoisted_count_rewritten_inside_the_loop_is_refused() {
    let body = LOOP.replace("br 0))", "i64.const 1000 local.set 4 br 0))");
    assert!(bounds(&body, "c0 a0", "1:20 1:30").is_err());
}

// ── bounds: a `for` loop's exit test (#4280) ───────────────────────────────

/// Locals: 0 the list block, 1 the count, 2 the counter, 3 another block.
fn for_loop(body: &str, w: &str, a: &str) -> Result<Report, String> {
    let bytes = module("(local i32 i32 i32 i32)", body);
    let bodies = decode(&bytes)?;
    let mut r = Report::default();
    check_bounds(&bodies, w, a, &[], &mut r).map(|()| r)
}

/// `for x in xs`: the count captured at entry into local 1, the exit test's
/// `br_if` at 12, the element's load at 18, the counter's step after it.
const FOR: &str = "local.get 0 i32.load offset=4 i32.const 8 i32.div_u local.set 1
  i32.const 0 local.set 2
  (block (loop
    local.get 2 local.get 1 i32.ge_u br_if 1
    local.get 0 local.get 2 i32.const 8 i32.mul i32.add i64.load offset=12 drop
    local.get 2 i32.const 1 i32.add local.set 2
    br 0))";

#[test]
fn a_for_loops_exit_guarded_read_is_accepted() -> Result<(), String> {
    let r = for_loop(FOR, "e0 a0", "1:12 1:18")?;
    assert_eq!((r.bounds_checks, r.bounds_accesses, r.bounds_operands), (1, 1, 1));
    Ok(())
}

#[test]
fn an_exit_test_with_another_compare_is_refused() {
    let body = FOR.replace("i32.ge_u", "i32.lt_u");
    assert!(for_loop(&body, "e0 a0", "1:12 1:18").unwrap_err().contains("i32.ge_u"));
    // An `i32.eqz` exit is a walk-down's (#4281): its operand must be the
    // cursor walking down, its load one byte.
    let body = FOR.replace("i32.ge_u br_if 1", "i32.ge_u i32.eqz br_if 1");
    let e = for_loop(&body, "e0 a0", "1:13 1:19").unwrap_err();
    assert!(e.contains("walk"), "{e}");
}

#[test]
fn an_exit_test_that_does_not_leave_the_loop_is_refused() {
    let body = FOR.replace("br_if 1", "br_if 0");
    assert!(for_loop(&body, "e0 a0", "1:12 1:18").unwrap_err().contains("br_if 1"));
    let body = FOR.replace("(block (loop", "(block (block");
    assert!(for_loop(&body, "e0 a0", "1:12 1:18").unwrap_err().contains("not a loop"));
}

#[test]
fn an_exit_count_in_bytes_is_refused() {
    let body = FOR.replace("i32.const 8 i32.div_u", "i32.const 1 i32.div_u");
    assert!(for_loop(&body, "e0 a0", "1:12 1:18").unwrap_err().contains("slots"));
}

#[test]
fn a_counter_changed_after_the_exit_test_is_refused() {
    let body = FOR.replace("br_if 1\n", "br_if 1 i32.const 99 local.set 2\n");
    assert!(for_loop(&body, "e0 a0", "1:12 1:20").unwrap_err().contains("tests"));
}

#[test]
fn an_exit_counting_another_list_is_refused() {
    let body = FOR.replacen("local.get 0 i32.load offset=4", "local.get 3 i32.load offset=4", 1);
    assert!(for_loop(&body, "e0 a0", "1:12 1:18").is_err());
}

#[test]
fn a_read_past_the_first_slot_under_an_exit_test_is_refused() {
    let body = FOR.replace("i64.load offset=12", "i64.load offset=20");
    assert!(for_loop(&body, "e0 a0", "1:12 1:18").unwrap_err().contains("payload"));
}

#[test]
fn a_call_handed_the_list_in_a_returning_arm_does_not_count() -> Result<(), String> {
    // The loop head's fuel exit: release the list and return. Indices: the
    // arm is 10..15, the exit test's br_if 19, the load 25.
    let body = FOR.replace(
        "(block (loop\n",
        "(block (loop\n    global.get 0 (if (then local.get 0 call $alloc drop return))\n",
    );
    let module = |b: &str| -> Result<Report, String> {
        let bytes = wat::parse_str(format!(
            r#"(module (memory 1) (global (mut i32) (i32.const 0))
                 (func $alloc (param i32) (result i32) i32.const 64)
                 (func $main (local i32 i32 i32 i32) {b}))"#
        ))
        .expect("wat");
        let bodies = decode(&bytes)?;
        let mut r = Report::default();
        check_bounds(&bodies, "e0 a0", "1:19 1:25", &[], &mut r).map(|()| r)
    };
    assert_eq!(module(&body)?.bounds_operands, 1);
    // The same call where the path falls through is handed the list.
    let falls = body.replace("drop return))", "drop))");
    let bytes = wat::parse_str(format!(
        r#"(module (memory 1) (global (mut i32) (i32.const 0))
             (func $alloc (param i32) (result i32) i32.const 64)
             (func $main (local i32 i32 i32 i32) {falls}))"#
    ))
    .expect("wat");
    let bodies = decode(&bytes)?;
    let mut r = Report::default();
    assert!(check_bounds(&bodies, "e0 a0", "1:18 1:24", &[], &mut r).unwrap_err().contains("handed"));
    Ok(())
}

#[test]
fn a_call_handed_a_held_list_inside_the_loop_is_accepted() -> Result<(), String> {
    // The hold (`call` at 1) takes +1 on the list before its count is read;
    // the body hands the list to a call after the exit test (br_if at 15),
    // and the load is at 24.
    let body = format!(
        "local.get 0 call $alloc drop\n{}",
        FOR.replace("br_if 1\n", "br_if 1 local.get 0 call $alloc drop\n")
    );
    let bytes = module("(local i32 i32 i32 i32)", &body);
    let bodies = decode(&bytes)?;
    let held = |holds: &[At]| {
        let mut r = Report::default();
        check_bounds_held(&bodies, "e0 a0", "1:15 1:24", &[], holds, &mut r).map(|()| r)
    };
    assert_eq!(held(&[At { func: 1, op: 1 }])?.bounds_operands, 1);
    // Unheld, the same call is handed the list.
    assert!(held(&[]).unwrap_err().contains("handed"));
    // A hold on another block does not cover it.
    let other = body.replacen("local.get 0 call $alloc drop", "local.get 3 call $alloc drop", 1);
    let bytes = module("(local i32 i32 i32 i32)", &other);
    let bodies = decode(&bytes)?;
    let mut r = Report::default();
    assert!(check_bounds_held(&bodies, "e0 a0", "1:15 1:24", &[], &[At { func: 1, op: 1 }], &mut r)
        .unwrap_err()
        .contains("handed"));
    Ok(())
}

#[test]
fn a_read_before_its_exit_test_is_refused() {
    let body = "local.get 0 i32.load offset=4 i32.const 8 i32.div_u local.set 1
      i32.const 0 local.set 2
      (block (loop
        local.get 0 local.get 2 i32.const 8 i32.mul i32.add i64.load offset=12 drop
        local.get 2 local.get 1 i32.ge_u br_if 1
        local.get 2 i32.const 1 i32.add local.set 2
        br 0))";
    assert!(for_loop(body, "a0 e0", "1:14 1:19").unwrap_err().contains("no located check"));
}

/// `xs[i] = v`: the count by `i64.div_u`, then the judge (here `$alloc`
/// stands in for it, at 13) into local 3 and back into local 0, the store
/// through local 3 at 24.
const STORE: &str = "local.get 1
  local.get 0 i32.load offset=4 i64.extend_i32_u i64.const 8 i64.div_u
  i64.ge_u (if (then i32.const 0 drop unreachable))
  local.get 0 call $alloc local.set 3 local.get 3 local.set 0
  local.get 3 local.get 1 i32.wrap_i64 i32.const 8 i32.mul i32.add i64.const 5 i64.store offset=12";

#[test]
fn a_store_through_the_judges_block_is_accepted() -> Result<(), String> {
    let r = bounds_judged(STORE, "c0 w0", "1:8 1:24", &[13])?;
    assert_eq!(r.bounds_operands, 1);
    Ok(())
}

#[test]
fn a_store_through_an_unrecorded_judge_is_refused() {
    assert!(bounds(STORE, "c0 w0", "1:8 1:24").is_err());
}

/// `b[i]` on Bytes: the room test, `i32.eqz` at 11 (const 13), the byte load at 21.
const BYTE: &str = "local.get 1 i64.const 0 i64.ge_s
  local.get 1 local.get 0 i32.load offset=4 i64.extend_i32_u i64.const 1 i64.sub i64.le_s
  i32.and i32.eqz (if (then i32.const 0 drop unreachable))
  local.get 0 local.get 1 i32.wrap_i64 i32.add i64.load8_u offset=12 drop";

#[test]
fn a_room_tested_byte_read_is_accepted() -> Result<(), String> {
    let r = bounds(BYTE, "c0 a0", "1:13 1:21")?;
    assert_eq!(r.bounds_operands, 1);
    Ok(())
}

#[test]
fn a_byte_access_wider_than_its_room_is_refused() {
    let body = BYTE.replace("i64.load8_u", "i64.load16_u");
    assert!(bounds(&body, "c0 a0", "1:13 1:21").unwrap_err().contains("room"));
}

#[test]
fn a_room_test_without_its_lower_bound_is_refused() {
    let body = BYTE.replace("local.get 1 i64.const 0 i64.ge_s", "i64.const 7 i64.const 0 i64.ge_s");
    assert!(bounds(&body, "c0 a0", "1:13 1:21").is_err());
}

#[test]
fn a_flag_judged_store_against_a_hoisted_count_is_accepted() -> Result<(), String> {
    // The loop's first store judges the list behind a flag (`if` with no
    // abort), so the access reads the list's variable with no single
    // dominating write; the hoisted count read it in the preheader.
    let body = "local.get 0 i32.load offset=4 i64.extend_i32_u i64.const 8 i64.div_u local.set 4
      (block (loop
        local.get 1 local.get 4 i64.ge_u (if (then i32.const 0 drop unreachable))
        local.get 2 i32.wrap_i64 i32.eqz (if (then local.get 0 call $alloc local.set 0))
        local.get 0 local.set 3
        local.get 3 local.get 1 i32.wrap_i64 i32.const 8 i32.mul i32.add i64.const 5 i64.store offset=12
        local.get 1 i64.const 1 i64.add local.set 1
        br 0))";
    let r = bounds_judged(body, "c0 w0", "1:12 1:33", &[21])?;
    assert_eq!(r.bounds_operands, 1);
    // Unrecorded, the judge is a call handed the list.
    assert!(bounds(body, "c0 w0", "1:12 1:33").unwrap_err().contains("handed"));
    Ok(())
}

#[test]
fn an_index_the_model_does_not_open_compares_by_its_instruction() -> Result<(), String> {
    let body = "local.get 1 i64.const 3 i64.rem_s local.tee 2
      local.get 0 i32.load offset=4 i32.const 8 i32.div_u i64.extend_i32_u
      i64.ge_u (if (then i32.const 0 drop unreachable))
      local.get 0 local.get 2 i32.wrap_i64 i32.const 8 i32.mul i32.add i64.load offset=12 drop";
    let r = bounds(body, "c0 a0", "1:11 1:21")?;
    assert_eq!(r.bounds_operands, 1);
    // The same remainder computed again is another evaluation.
    let again = body.replace(
        "local.get 0 local.get 2 i32.wrap_i64",
        "local.get 0 local.get 1 i64.const 3 i64.rem_s i32.wrap_i64",
    );
    assert!(bounds(&again, "c0 a0", "1:11 1:23").is_err());
    Ok(())
}

/// A check that runs past its deadline is a reject, never a pass or a skip.
#[test]
fn a_check_past_its_deadline_is_a_reject() {
    use std::time::Duration;
    assert_eq!(almide_anchor_check::within(Duration::from_millis(20), || {
        std::thread::sleep(Duration::from_secs(5));
        1
    }), None);
    assert_eq!(almide_anchor_check::within(Duration::from_secs(5), || 1), Some(1));
    // A check that would have PASSED, had it finished: still a reject.
    let late = almide_anchor_check::deadline(Duration::from_millis(20), || {
        std::thread::sleep(Duration::from_secs(5));
        Ok::<(), String>(())
    });
    assert!(late.is_err_and(|e| e.contains("deadline")));
    assert_eq!(almide_anchor_check::deadline(Duration::from_secs(5), || Ok::<u8, String>(1)), Ok(1));
}
