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

const GUARD: &str = "local.get 1 local.get 0 i32.load offset=4 i32.ge_u
  if i32.const 0 drop unreachable end
  local.get 0 local.get 1 i32.add i32.load8_u offset=12 drop";

#[test]
fn a_guarded_byte_read_is_accepted() -> Result<(), String> {
    let r = bounds(GUARD, "c0 a0", "1:5 1:12")?;
    assert_eq!((r.bounds_checks, r.bounds_operands), (1, 1));
    Ok(())
}

#[test]
fn a_guarded_read_of_byte_zero_is_accepted() -> Result<(), String> {
    let body = "i32.const 0 local.get 0 i32.load offset=4 i32.ge_u
      if i32.const 0 drop unreachable end
      local.get 0 i32.load8_u offset=12 drop";
    assert_eq!(bounds(body, "c0 a0", "1:5 1:10")?.bounds_operands, 1);
    Ok(())
}

#[test]
fn a_guard_one_byte_loose_is_refused() {
    let body = GUARD.replace("i32.load offset=4 i32.ge_u", "i32.load offset=4 i32.const 1 i32.add i32.ge_u");
    assert!(bounds(&body, "c0 a0", "1:7 1:14").is_err());
}

#[test]
fn a_guard_on_another_index_is_refused() {
    let body = GUARD.replace("local.get 0 local.get 1 i32.add", "local.get 0 local.get 2 i32.add");
    assert!(bounds(&body, "c0 a0", "1:5 1:12").is_err());
}

#[test]
fn a_guard_that_does_not_abort_is_refused() {
    let body = GUARD.replace("drop unreachable", "drop nop");
    assert!(bounds(&body, "c0 a0", "1:5 1:12").is_err());
}
