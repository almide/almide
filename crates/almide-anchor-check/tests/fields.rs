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
