//! The fields check over hand-written modules (#4279): each shape it
//! accepts, and the one-instruction change it must refuse.

use almide_anchor_check::{check_fields, decode, Report};

/// Function 0 is `$alloc`; function 1 is the subject, its body `body`.
fn module(body: &str) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(module (memory 1)
             (func $alloc (param i32) (result i32) i32.const 64)
             (func $main (local i32 i32 i64) {body}))"#
    ))
    .expect("wat")
}

fn fields(body: &str, w: &str, a: &str) -> Result<Report, String> {
    let bytes = module(body);
    let bodies = decode(&bytes)?;
    let mut r = Report::default();
    check_fields(&bodies, 0, 0, w, a, &mut r).map(|()| r)
}

// ── a record: its site and a field read ────────────────────────────────────

const REC: &str = "i32.const 16 call $alloc local.set 0
  local.get 0 i64.const 3 i64.store offset=12
  local.get 0 i32.const 4 i32.store offset=20
  local.get 0 i32.load offset=20 drop";
const REC_W: &str = "L3 c0 n1 z16 o0 w8 o8 w4;k3 c0 s0 s1;a3 c0 f1";
const REC_A: &str = "-;1:2 1:5 1:8;1:10";

#[test]
fn a_record_site_and_a_field_read_are_accepted() -> Result<(), String> {
    let r = fields(REC, REC_W, REC_A)?;
    assert_eq!((r.field_sites, r.field_stores, r.field_accesses), (1, 2, 1));
    Ok(())
}

#[test]
fn a_read_one_slot_off_its_field_is_refused() {
    let body = REC.replace("i32.load offset=20", "i32.load offset=24");
    assert!(fields(&body, REC_W, REC_A).unwrap_err().contains("offset 24"));
}

#[test]
fn a_site_store_at_another_fields_offset_is_refused() {
    let body = REC.replace("i64.store offset=12", "i64.store offset=20");
    assert!(fields(&body, REC_W, REC_A).unwrap_err().contains("offset 20"));
}

#[test]
fn an_allocation_smaller_than_the_layout_is_refused() {
    let body = REC.replace("i32.const 16 call", "i32.const 8 call");
    assert!(fields(&body, REC_W, REC_A).unwrap_err().contains("allocation"));
}

#[test]
fn an_unrecorded_store_into_the_site_is_refused() {
    let body = REC.replace(
        "local.get 0 i32.const 4 i32.store offset=20",
        "local.get 0 i32.const 9 i32.store offset=24 local.get 0 i32.const 4 i32.store offset=20",
    );
    assert!(fields(&body, REC_W, "-;1:2 1:5 1:11;1:13").unwrap_err().contains("unrecorded"));
}

/// A field value computed by a branch whose other arm traps (a match with a
/// trapping arm as a tuple field, #4599): the trapping arm's `end` conjures
/// its result, so the block's address below the branch is still the store's.
const REC_TRAP_ARM: &str = "i32.const 16 call $alloc local.set 0
  local.get 0 i64.const 3 i64.store offset=12
  local.get 0
  local.get 1 if (result i32) i32.const 4 else unreachable end
  i32.store offset=20
  local.get 0 i32.load offset=20 drop";

#[test]
fn a_field_computed_by_a_branch_with_a_trapping_arm_is_accepted() -> Result<(), String> {
    let r = fields(REC_TRAP_ARM, REC_W, "-;1:2 1:5 1:13;1:15")?;
    assert_eq!((r.field_sites, r.field_stores, r.field_accesses), (1, 2, 1));
    Ok(())
}

#[test]
fn a_store_through_a_value_the_trapping_arm_conjures_is_refused() {
    // The address itself now comes from the branch: no block local below it.
    let body = REC_TRAP_ARM.replace(
        "local.get 0\n  local.get 1 if (result i32) i32.const 4 else unreachable end",
        "local.get 1 if (result i32) local.get 0 else unreachable end\n  i32.const 4",
    );
    assert!(fields(&body, REC_W, "-;1:2 1:5 1:13;1:15").is_err());
}

#[test]
fn a_layout_carrying_an_anchor_is_refused() {
    assert!(fields(REC, REC_W, "1:0;1:2 1:5 1:8;1:10").is_err());
}

// ── a variant: a case field under its tag test ─────────────────────────────

const VAR_W: &str = "L4 c0 n2 z8 o0 w4;L4 c1 n2 z16 o0 w4 o8 w8;t4;a4 c1 f1";

fn guarded(tag: i32) -> String {
    format!(
        "local.get 0 i32.load offset=12 i32.const {tag} i32.eq
         if
           local.get 0 i64.load offset=20 drop
         end"
    )
}

#[test]
fn a_case_field_under_its_tag_test_is_accepted() -> Result<(), String> {
    let r = fields(&guarded(1), VAR_W, "-;-;1:1;1:6")?;
    assert_eq!((r.field_tags, r.field_accesses, r.field_guarded), (1, 1, 1));
    Ok(())
}

#[test]
fn a_case_field_under_another_cases_test_is_refused() {
    assert!(fields(&guarded(0), VAR_W, "-;-;1:1;1:6").unwrap_err().contains("tag test"));
}

#[test]
fn a_case_field_read_with_no_test_is_refused() {
    let body = "local.get 0 i64.load offset=20 drop";
    assert!(fields(body, "L4 c0 n2 z8 o0 w4;L4 c1 n2 z16 o0 w4 o8 w8;a4 c1 f1", "-;-;1:1").unwrap_err().contains("tag test"));
}

#[test]
fn a_case_field_after_its_local_is_rewritten_is_refused() {
    let body = "local.get 0 i32.load offset=12 i32.const 1 i32.eq
         if
           local.get 1 local.set 0
           local.get 0 i64.load offset=20 drop
         end";
    assert!(fields(body, VAR_W, "-;-;1:1;1:8").unwrap_err().contains("tag test"));
}

/// The nested shape a pattern test and its binds take: the tag test runs on
/// a hold the field was loaded into, inside an `if (result i32) .. else
/// i32.const 0 end`; the bind re-loads the same field into the hold.
fn nested(between: &str) -> String {
    format!(
        "local.get 0 i32.load offset=12 i32.const 1 i32.eq
         if (result i32)
           local.get 0 i32.load offset=20 local.set 1
           local.get 1 i32.load offset=12 i32.const 1 i32.eq
         else
           i32.const 0
         end
         if
           {between}
           local.get 0 i32.load offset=20 local.set 1
           local.get 1 i64.load offset=20 drop
         end"
    )
}

#[test]
fn a_case_field_of_a_re_loaded_hold_is_accepted() -> Result<(), String> {
    // ops: 0 get 1 load 2 const 3 eq 4 if 5 get 6 load 7 set 8 get 9 load(tag)
    // 10 const 11 eq 12 else 13 const 14 end 15 if 16 get 17 load 18 set 19 get 20 load
    let w = "L4 c0 n2 z8 o0 w4;L4 c1 n2 z16 o0 w4 o8 w8;t4;t4;a4 c1 f1";
    let r = fields(&nested(""), w, "-;-;1:1;1:9;1:20")?;
    assert_eq!(r.field_guarded, 1);
    Ok(())
}

#[test]
fn a_store_between_the_test_and_the_re_load_is_refused() {
    let w = "L4 c0 n2 z8 o0 w4;L4 c1 n2 z16 o0 w4 o8 w8;a4 c1 f1";
    // `between` adds 3 instructions before the bind.
    let body = nested("local.get 0 i32.const 0 i32.store offset=20");
    assert!(fields(&body, w, "-;-;1:23").unwrap_err().contains("tag test"));
}

/// A call between the test and the re-load: refused when the callee may
/// write memory, accepted when its body only reads.
fn with_callee(callee: &str) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(module (memory 1)
             (func $alloc (param i32) (result i32) i32.const 64)
             (func $main (local i32 i32 i64) {})
             (func $callee {callee}))"#,
        nested("call $callee")
    ))
    .expect("wat")
}

fn nested_call(callee: &str) -> Result<Report, String> {
    let bytes = with_callee(callee);
    let bodies = decode(&bytes)?;
    let mut r = Report::default();
    // `call` adds one instruction before the bind.
    check_fields(&bodies, 0, 0, "L4 c0 n2 z8 o0 w4;L4 c1 n2 z16 o0 w4 o8 w8;a4 c1 f1", "-;-;1:21", &mut r).map(|()| r)
}

#[test]
fn a_read_only_call_between_the_test_and_the_re_load_is_accepted() -> Result<(), String> {
    assert_eq!(nested_call("i32.const 0 i32.load drop")?.field_guarded, 1);
    Ok(())
}

#[test]
fn a_writing_call_between_the_test_and_the_re_load_is_refused() {
    assert!(nested_call("i32.const 0 i32.const 1 i32.store").unwrap_err().contains("tag test"));
}

#[test]
fn a_callee_whose_only_write_is_a_call_that_never_returns_is_read_only() -> Result<(), String> {
    // `$alloc` stores nothing here but stands for any callee: a call followed
    // by `unreachable` cannot return into the caller's window.
    let callee = "i32.const 0 i32.load drop i32.const 1 call $writer unreachable";
    let bytes = wat::parse_str(format!(
        r#"(module (memory 1)
             (func $alloc (param i32) (result i32) i32.const 64)
             (func $main (local i32 i32 i64) {})
             (func $callee {callee})
             (func $writer (param i32) i32.const 0 i32.const 1 i32.store))"#,
        nested("call $callee")
    ))
    .expect("wat");
    let bodies = decode(&bytes)?;
    let mut r = Report::default();
    check_fields(&bodies, 0, 0, "L4 c0 n2 z8 o0 w4;L4 c1 n2 z16 o0 w4 o8 w8;a4 c1 f1", "-;-;1:21", &mut r)?;
    assert_eq!(r.field_guarded, 1);
    Ok(())
}

// ── a variant held in a global, read across a call ─────────────────────────

/// The block lives in global `$g`. Function 2 (`$swap`) rebinds `$g` and
/// writes no memory; function 3 (`$quiet`) does nothing.
fn global_held(between: &str) -> Result<Report, String> {
    let bytes = wat::parse_str(format!(
        r#"(module (memory 1)
             (global $g (mut i32) (i32.const 64))
             (func $alloc (param i32) (result i32) i32.const 64)
             (func $main
               global.get $g i32.load offset=12 i32.const 1 i32.eq
               if
                 {between}
                 global.get $g i64.load offset=20 drop
               end)
             (func $swap i32.const 128 global.set $g)
             (func $quiet nop))"#
    ))
    .expect("wat");
    let bodies = decode(&bytes)?;
    let mut r = Report::default();
    let read = if between.is_empty() { "1:6" } else { "1:7" };
    check_fields(&bodies, 0, 0, VAR_W, &format!("-;-;1:1;{read}"), &mut r).map(|()| r)
}

#[test]
fn a_case_field_of_a_global_with_nothing_between_is_accepted() -> Result<(), String> {
    assert_eq!(global_held("")?.field_guarded, 1);
    Ok(())
}

#[test]
fn a_case_field_of_a_global_across_a_call_that_sets_no_global_is_accepted() -> Result<(), String> {
    assert_eq!(global_held("call $quiet")?.field_guarded, 1);
    Ok(())
}

/// The call between the tag test and the read rebinds the global: the read
/// is of another block, whose tag nothing tested.
#[test]
fn a_case_field_of_a_global_across_a_call_that_rebinds_it_is_refused() {
    let r = global_held("call $swap");
    assert!(r.is_err(), "accepted a case field read of a global a call between may rebind: {:?}", r.map(|r| r.field_guarded));
}
