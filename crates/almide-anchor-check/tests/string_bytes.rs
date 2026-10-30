//! String byte reads (#4281): a byte loop's exit test and a guard opening
//! an aborting `if`, each over a string's `len` word with one-byte slots —
//! the shapes the bounds check accepts, and the one-instruction changes it
//! must refuse.

use almide_anchor_check::{check_bounds, decode, Report};

/// Function 0 is `$alloc`; function 1 is the subject. Locals: 0 the
/// string, 1 the index, 2 a captured count (or another string).
fn bounds(body: &str, w: &str, a: &str) -> Result<Report, String> {
    let bytes = wat::parse_str(format!(
        r#"(module (memory 1)
             (func $alloc (param i32) (result i32) i32.const 64)
             (func $main (local i32 i32 i32) {body}))"#
    ))
    .expect("wat");
    let bodies = decode(&bytes)?;
    let mut r = Report::default();
    check_bounds(&bodies, w, a, &[], &mut r).map(|()| r)
}

const LOOP: &str = "local.get 0 i32.load offset=4 local.set 2
  block loop
    local.get 1 local.get 2 i32.ge_u br_if 1
    local.get 0 local.get 1 i32.add i32.load8_u offset=12 drop
    local.get 1 i32.const 1 i32.add local.set 1
    br 0
  end end";

#[test]
fn a_byte_loop_over_the_len_word_is_accepted() -> Result<(), String> {
    let r = bounds(LOOP, "e0 a0", "1:8 1:12")?;
    assert_eq!((r.bounds_checks, r.bounds_accesses, r.bounds_operands), (1, 1, 1));
    Ok(())
}

#[test]
fn a_byte_loop_exit_one_byte_loose_is_refused() {
    let body = LOOP.replace("i32.ge_u br_if 1", "i32.gt_u br_if 1");
    assert!(bounds(&body, "e0 a0", "1:8 1:12").is_err());
}

#[test]
fn a_byte_loop_read_one_byte_past_its_slot_is_refused() {
    let body = LOOP.replace("i32.load8_u offset=12", "i32.load8_u offset=13");
    assert!(bounds(&body, "e0 a0", "1:8 1:12").unwrap_err().contains("offset 13"));
}

#[test]
fn a_byte_loop_reading_another_string_is_refused() {
    let body = LOOP.replace("local.get 0 local.get 1 i32.add", "local.get 2 local.get 1 i32.add");
    assert!(bounds(&body, "e0 a0", "1:8 1:12").is_err());
}

// The surface's own test (#4281): `idx >=u len(s)` opening an `if` whose
// `else` holds the read — no second test. Anchors: the compare's `if` (5),
// the load (13).
const TEST: &str = "local.get 1 local.get 0 i32.load offset=4 i32.ge_u
  if (result i32) i32.const 0
  else local.get 0 local.get 1 i32.add i32.load8_u offset=12 end drop";

#[test]
fn a_read_in_its_tests_else_is_accepted() -> Result<(), String> {
    let r = bounds(TEST, "c0 a0", "1:4 1:10")?;
    assert_eq!((r.bounds_checks, r.bounds_operands), (1, 1));
    Ok(())
}

#[test]
fn a_read_in_its_tests_then_arm_is_refused() {
    let body = "local.get 1 local.get 0 i32.load offset=4 i32.ge_u
      if (result i32) local.get 0 local.get 1 i32.add i32.load8_u offset=12
      else i32.const 0 end drop";
    assert!(bounds(body, "c0 a0", "1:4 1:8").is_err());
}

#[test]
fn a_read_after_its_tests_if_is_refused() {
    let body = "local.get 1 local.get 0 i32.load offset=4 i32.ge_u
      if (result i32) i32.const 0 else i32.const 1 end drop
      local.get 0 local.get 1 i32.add i32.load8_u offset=12 drop";
    assert!(bounds(body, "c0 a0", "1:4 1:13").is_err());
}

#[test]
fn a_test_one_byte_loose_is_refused() {
    let body = TEST.replace("i32.load offset=4 i32.ge_u", "i32.load offset=4 i32.const 1 i32.add i32.ge_u");
    assert!(bounds(&body, "c0 a0", "1:6 1:12").is_err());
}

#[test]
fn a_test_on_another_index_is_refused() {
    let body = TEST.replace("local.get 0 local.get 1 i32.add", "local.get 0 local.get 2 i32.add");
    assert!(bounds(&body, "c0 a0", "1:4 1:10").is_err());
}

#[test]
fn a_test_without_an_else_is_refused() {
    let body = "local.get 1 local.get 0 i32.load offset=4 i32.ge_u
      if i32.const 0 drop end
      local.get 0 local.get 1 i32.add i32.load8_u offset=12 drop";
    assert!(bounds(body, "c0 a0", "1:4 1:12").is_err());
}

// `len(s) == 0` opening an `if` whose `else` reads byte 0.
const EMPTY: &str = "local.get 0 i32.load offset=4 i32.eqz
  if (result i32) i32.const 0
  else local.get 0 i32.load8_u offset=12 end drop";

#[test]
fn byte_zero_under_the_empty_test_is_accepted() -> Result<(), String> {
    assert_eq!(bounds(EMPTY, "c0 a0", "1:3 1:7")?.bounds_operands, 1);
    Ok(())
}

#[test]
fn byte_one_under_the_empty_test_is_refused() {
    let body = EMPTY.replace("i32.load8_u offset=12", "i32.load8_u offset=13");
    assert!(bounds(&body, "c0 a0", "1:3 1:7").is_err());
}

#[test]
fn an_indexed_read_under_the_empty_test_is_refused() {
    let body = EMPTY.replace("else local.get 0 i32.load8_u", "else local.get 0 local.get 1 i32.add i32.load8_u");
    assert!(bounds(&body, "c0 a0", "1:3 1:9").is_err());
}

#[test]
fn the_empty_test_of_another_string_is_refused() {
    let body = EMPTY.replacen("local.get 0 i32.load offset=4", "local.get 2 i32.load offset=4", 1);
    assert!(bounds(&body, "c0 a0", "1:3 1:7").is_err());
}

// A prefix compare: the loop runs k over len(p) (local 2), and reads s
// (local 0) at k in the `else` of `len(p) >u len(s)`. Local 3 holds len(p).
fn prefix(fact: &str) -> String {
    format!(
        "local.get 2 i32.load offset=4 local.set 3
  local.get 3 local.get 0 i32.load offset=4 {fact}
  if (result i32) i32.const 0
  else
    block loop
      local.get 1 local.get 3 i32.ge_u br_if 1
      local.get 0 local.get 1 i32.add i32.load8_u offset=12 drop
      local.get 1 i32.const 1 i32.add local.set 1
      br 0
    end end
    i32.const 1
  end drop"
    )
}

fn bounds4(body: &str, w: &str, a: &str) -> Result<Report, String> {
    let bytes = wat::parse_str(format!(
        r#"(module (memory 1)
             (func $alloc (param i32) (result i32) i32.const 64)
             (func $main (local i32 i32 i32 i32) {body}))"#
    ))
    .expect("wat");
    let bodies = decode(&bytes)?;
    let mut r = Report::default();
    check_bounds(&bodies, w, a, &[], &mut r).map(|()| r)
}

#[test]
fn a_prefix_read_bounded_by_the_length_test_is_accepted() -> Result<(), String> {
    let r = bounds4(&prefix("i32.gt_u"), "e0 a0", "1:15 1:19")?;
    assert_eq!(r.bounds_operands, 1);
    Ok(())
}

#[test]
fn a_prefix_read_bounded_by_equal_lengths_is_accepted() -> Result<(), String> {
    let r = bounds4(&prefix("i32.ne"), "e0 a0", "1:15 1:19")?;
    assert_eq!(r.bounds_operands, 1);
    Ok(())
}

#[test]
fn a_prefix_read_under_a_loose_length_test_is_refused() {
    // `len(p) <u len(s)` false says nothing that bounds k by len(s).
    assert!(bounds4(&prefix("i32.lt_u"), "e0 a0", "1:15 1:19").is_err());
}

#[test]
fn a_prefix_read_of_a_string_the_test_did_not_measure_is_refused() {
    // The length test measures p against itself; the read is of s.
    let body = prefix("i32.gt_u").replacen("local.get 3 local.get 0 i32.load", "local.get 3 local.get 2 i32.load", 1);
    assert!(bounds4(&body, "e0 a0", "1:15 1:19").is_err());
}
