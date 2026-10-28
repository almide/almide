//! The checker over hand-written modules: each shape it accepts, and for
//! each, the one-instruction change it must refuse.

use almide_anchor_check::{check_bounds, check_init, check_rc, decode, At, RcReport, Report};

/// Function 0 is `$alloc`; function 1 is the subject, its body `body`.
fn module(locals: &str, body: &str) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(module (memory 1)
             (func $alloc (param i32) (result i32) i32.const 64)
             (func $main {locals} {body}))"#
    ))
    .expect("wat")
}

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

// ── rc (#4277): recorded events against the release calls ─────────────────

fn rc(body: &str, events: &str) -> Result<RcReport, String> {
    let bytes = module("(local i32 i32)", body);
    let bodies = decode(&bytes)?;
    let mut r = RcReport::default();
    // `$alloc` (function 0) stands in for the release routine.
    let releases = [0u32].into_iter().collect();
    check_rc(&bodies, None, &releases, &[], &[(1, events.to_string())], &[], &mut r).map(|()| r)
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
fn dead_and_pre_abort_releases_are_not_live_events() -> Result<(), String> {
    rc("return local.get 0 call $alloc drop", "")?;
    let r = rc("local.get 0 call $alloc drop unreachable", "")?;
    assert_eq!(r.pre_abort, 1);
    Ok(())
}

#[test]
fn a_rebind_release_matches_a_release_or_the_judge_on_its_local() -> Result<(), String> {
    let bytes = module("(local i32 i32)", "local.get 0 call $alloc local.set 0");
    let bodies = decode(&bytes)?;
    let releases = std::collections::HashSet::new();
    let mut r = RcReport::default();
    // The call at 1 is the recorded judge: the rebind's release is inside it.
    check_rc(&bodies, None, &releases, &[At { func: 1, op: 1 }], &[(1, "o0".into())], &[], &mut r)?;
    assert_eq!((r.rebind_unmatched, r.judges_unmatched), (0, 0));
    // No judge and no release: the rebind is counted as one with no instruction.
    let mut r = RcReport::default();
    check_rc(&bodies, None, &releases, &[], &[(1, "o0".into())], &[], &mut r)?;
    assert_eq!(r.rebind_unmatched, 1);
    Ok(())
}

#[test]
fn an_rc_call_inside_an_arm_window_is_the_arms_own() -> Result<(), String> {
    let bytes = module("(local i32 i32)", "local.get 0 call $alloc drop");
    let bodies = decode(&bytes)?;
    let releases = [0u32].into_iter().collect();
    let mut r = RcReport::default();
    check_rc(&bodies, None, &releases, &[], &[(1, String::new())], &[(1, 0, 2)], &mut r)?;
    assert_eq!(r.arm_internal, 1);
    // Outside the window it is an unrecorded release.
    assert!(check_rc(&bodies, None, &releases, &[], &[(1, String::new())], &[(1, 2, 2)], &mut RcReport::default()).is_err());
    Ok(())
}

#[test]
fn a_variables_release_inside_its_judge_is_matched() -> Result<(), String> {
    let bytes = module("(local i32 i32)", "local.get 0 call $alloc local.set 0");
    let bodies = decode(&bytes)?;
    let releases = std::collections::HashSet::new();
    let mut r = RcReport::default();
    check_rc(&bodies, None, &releases, &[At { func: 1, op: 1 }], &[(1, "d0".into())], &[], &mut r)?;
    // Without the judge there is no instruction for it.
    assert!(check_rc(&bodies, None, &releases, &[], &[(1, "d0".into())], &[], &mut RcReport::default()).is_err());
    Ok(())
}
