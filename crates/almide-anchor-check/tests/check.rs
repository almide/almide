//! The checker over hand-written modules: each shape it accepts, and for
//! each, the one-instruction change it must refuse.

use almide_anchor_check::{check_bounds, check_init, decode, Report};

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

fn bounds(body: &str, w: &str, a: &str) -> Result<Report, String> {
    let bytes = module("(local i32 i64)", body);
    let bodies = decode(&bytes)?;
    let mut r = Report::default();
    check_bounds(&bodies, w, a, &mut r).map(|()| r)
}

// ── init: a fixed site ─────────────────────────────────────────────────────

const SITE: &str = "i32.const 16 call $alloc local.set 0
  local.get 0 i64.const 7 i64.store offset=12
  local.get 0 i64.const 8 i64.store offset=20";
const SITE_W: &str = "h12 w8 n2 r0 s0 s1";
const SITE_A: &str = "1:2 1:5 1:8";

#[test]
fn a_site_whose_stores_fill_its_slots_is_accepted() {
    let r = init(SITE, SITE_W, SITE_A).unwrap();
    assert_eq!((r.init_sites, r.init_stores), (1, 2));
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
fn a_countdown_fill_is_accepted() {
    let r = init(&fill(8), FILL_W, FILL_A).unwrap();
    assert_eq!(r.fill_sites, 1);
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

const READ: &str = "local.get 1 i64.const 3 i64.ge_u
  (if (then i32.const 0 drop unreachable))
  local.get 0 i64.load offset=12 drop";

#[test]
fn a_guarded_read_is_accepted() {
    let r = bounds(READ, "c0 a0", "1:4 1:9").unwrap();
    assert_eq!((r.bounds_checks, r.bounds_accesses), (1, 1));
}

#[test]
fn a_guard_that_does_not_abort_is_refused() {
    let body = READ.replace("drop unreachable", "drop");
    assert!(bounds(&body, "c0 a0", "1:4 1:8")
        .unwrap_err()
        .contains("abort"));
}

#[test]
fn a_guard_with_another_compare_is_refused() {
    let body = READ.replace("i64.ge_u", "i64.ge_s");
    assert!(bounds(&body, "c0 a0", "1:4 1:9").is_err());
}

#[test]
fn an_access_anchored_off_its_load_is_refused() {
    assert!(bounds(READ, "c0 a0", "1:4 1:8").is_err());
}

#[test]
fn a_check_after_its_access_is_refused() {
    let body = "local.get 0 i64.load offset=12 drop
      local.get 1 i64.const 3 i64.ge_u (if (then i32.const 0 drop unreachable))";
    assert!(bounds(body, "c0 a0", "1:7 1:1")
        .unwrap_err()
        .contains("not after"));
}

#[test]
fn a_kill_carries_no_anchor_and_an_unlocated_event_is_counted() {
    let r = bounds(READ, "c0 a0 s0;c0 a0", "1:4 1:9 -;- -").unwrap();
    assert_eq!(r.bounds_unlocated, 2);
    assert!(bounds(READ, "c0 a0 s0", "1:4 1:9 1:9").is_err());
}
