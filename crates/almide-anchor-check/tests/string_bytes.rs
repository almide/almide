//! String byte reads (#4281): a byte loop's exit test and a guard opening
//! an aborting `if`, each over a string's `len` word with one-byte slots —
//! the shapes the bounds check accepts, and the one-instruction changes it
//! must refuse.

use almide_anchor_check::{check_bounds, check_bounds_alloc, decode, Report, Sites};

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

// ── #4281: the six reads that take more than one fact ──────────────────

/// Function 0 is `$alloc`; function 1 the subject with `n` i32 locals. The
/// anchors are the instructions marked `(;@;)`, in order, as `1:<index>`.
fn marked(n: usize, body: &str, w: &str) -> Result<Report, String> {
    let locals = vec!["i32"; n].join(" ");
    let bytes = wat::parse_str(format!(
        r#"(module (memory 1)
             (func $alloc (param i32) (result i32) i32.const 64)
             (func $main (local {locals}) {body}))"#
    ))
    .expect("wat");
    let bodies = decode(&bytes)?;
    let mut r = Report::default();
    check_bounds_alloc(&bodies, w, &anchors(body), Sites::default(), 0, &mut r).map(|()| r)
}

/// The instruction index of each `(;@;)` mark: instruction tokens before it.
fn anchors(body: &str) -> String {
    let flat = body.replace("(result i32)", "");
    let (mut k, mut out) = (0usize, Vec::new());
    for t in flat.split_whitespace() {
        if t == "(;@;)" {
            out.push(format!("1:{k}"));
        } else if t.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) && !t.starts_with("offset=") {
            k += 1;
        }
    }
    out.join(" ")
}

// ends_with / strip_suffix: s (0) read at (len(s) − len(p)) + k, k (1) < len(p)
// (3, from p = 2) and the `else` of `len(p) >u len(s)`. Local 4 is the moved
// base when `moved`.
fn suffix(fact: &str, moved: bool) -> String {
    let (setup, base) = if moved {
        ("local.get 0 local.get 0 i32.load offset=4 local.get 3 i32.sub i32.add local.set 4", "local.get 4")
    } else {
        ("", "local.get 0 local.get 0 i32.load offset=4 local.get 3 i32.sub i32.add")
    };
    format!(
        "local.get 2 i32.load offset=4 local.set 3
  local.get 3 local.get 0 i32.load offset=4 {fact}
  if (result i32) i32.const 0
  else {setup}
    block loop
      local.get 1 local.get 3 i32.ge_u (;@;) br_if 1
      {base} local.get 1 i32.add (;@;) i32.load8_u offset=12 drop
      local.get 1 i32.const 1 i32.add local.set 1
      br 0
    end end
    i32.const 1
  end drop"
    )
}

#[test]
fn a_suffix_read_is_accepted_through_a_moved_base_or_an_offset() -> Result<(), String> {
    assert_eq!(marked(5, &suffix("i32.gt_u", true), "e0 a0")?.bounds_operands, 1);
    assert_eq!(marked(5, &suffix("i32.gt_u", false), "e0 a0")?.bounds_operands, 1);
    Ok(())
}

#[test]
fn a_suffix_read_without_the_length_test_is_refused() {
    assert!(marked(5, &suffix("i32.lt_u", true), "e0 a0").is_err());
}

#[test]
fn a_suffix_read_one_byte_past_is_refused() {
    let body = suffix("i32.gt_u", true).replace("local.get 3 i32.sub i32.add local.set 4", "local.get 3 i32.sub i32.add i32.const 1 i32.add local.set 4");
    assert!(marked(5, &body, "e0 a0").is_err());
}

#[test]
fn a_suffix_offset_by_another_length_is_refused() {
    // len(s) − len(s) + k: the offset does not subtract the loop's count.
    let body = suffix("i32.gt_u", true).replace("i32.load offset=4 local.get 3 i32.sub i32.add local.set 4", "i32.load offset=4 local.get 0 i32.load offset=4 i32.sub i32.add local.set 4");
    assert!(marked(5, &body, "e0 a0").is_err());
}

// lines' CR: inside a byte loop over len(s) (count in 2), at a newline k is
// copied to 3 and, when 3 >u start (4), byte 3 − 1 is read.
fn cr(test: &str) -> String {
    format!(
        "local.get 0 i32.load offset=4 local.set 2
  block loop
    local.get 1 local.get 2 i32.ge_u (;@;) br_if 1
    local.get 1 local.set 3
    local.get 3 local.get 4 {test}
    if local.get 0 local.get 3 i32.add i32.const 1 i32.sub (;@;) i32.load8_u offset=12 drop end
    local.get 1 i32.const 1 i32.add local.set 1
    br 0
  end end"
    )
}

#[test]
fn the_byte_before_a_positive_cursor_is_accepted() -> Result<(), String> {
    assert_eq!(marked(5, &cr("i32.gt_u"), "e0 a0")?.bounds_operands, 1);
    Ok(())
}

#[test]
fn the_byte_before_a_cursor_not_known_positive_is_refused() {
    // `3 >=u 4` admits 3 == 0: the read would be byte −1.
    assert!(marked(5, &cr("i32.ge_u"), "e0 a0").is_err());
}

#[test]
fn a_read_after_its_loop_is_refused() {
    let body = "local.get 0 i32.load offset=4 local.set 2
  block loop
    local.get 1 local.get 2 i32.ge_u (;@;) br_if 1
    local.get 1 i32.const 1 i32.add local.set 1
    br 0
  end end
  local.get 0 local.get 1 i32.add (;@;) i32.load8_u offset=12 drop";
    assert!(marked(3, body, "e0 a0").is_err());
}

// last's walk: p (1) from len(s) − 1 in the `else` of `len(s) == 0`, down
// while p != 0, reading s + p. read_line's: p from len(s), reading p − 1.
fn walk(start: &str, read: &str, step: &str) -> String {
    format!(
        "local.get 0 i32.load offset=4 i32.eqz
  if else
    {start} local.set 1
    block loop
      local.get 1 i32.eqz (;@;) br_if 1
      {read} (;@;) i32.load8_u offset=12 drop
      {step}
      br 0
    end end
  end"
    )
}

const LAST: [&str; 3] = [
    "local.get 0 i32.load offset=4 i32.const 1 i32.sub",
    "local.get 0 local.get 1 i32.add",
    "local.get 1 i32.const 1 i32.sub local.set 1",
];

#[test]
fn a_walk_down_from_the_last_byte_is_accepted() -> Result<(), String> {
    assert_eq!(marked(3, &walk(LAST[0], LAST[1], LAST[2]), "e0 a0")?.bounds_operands, 1);
    Ok(())
}

#[test]
fn a_walk_down_from_len_reading_the_byte_before_is_accepted() -> Result<(), String> {
    let r = marked(3, &walk("local.get 0 i32.load offset=4", "local.get 0 local.get 1 i32.add i32.const 1 i32.sub", LAST[2]), "e0 a0")?;
    assert_eq!(r.bounds_operands, 1);
    Ok(())
}

#[test]
fn a_walk_down_from_len_reading_at_the_cursor_is_refused() {
    assert!(marked(3, &walk("local.get 0 i32.load offset=4", LAST[1], LAST[2]), "e0 a0").is_err());
}

#[test]
fn a_walk_that_steps_up_is_refused() {
    assert!(marked(3, &walk(LAST[0], LAST[1], "local.get 1 i32.const 1 i32.add local.set 1"), "e0 a0").is_err());
}

#[test]
fn a_walk_from_the_last_byte_of_a_possibly_empty_string_is_refused() {
    let body = walk(LAST[0], LAST[1], LAST[2]).replace("i32.eqz\n  if else", "drop i32.const 1\n  if else");
    assert!(marked(3, &body, "e0 a0").is_err());
}

#[test]
fn a_walk_over_another_string_is_refused() {
    assert!(marked(3, &walk(LAST[0], "local.get 2 local.get 1 i32.add", LAST[2]), "e0 a0").is_err());
}

// read_line's byte: a fresh 1-byte block (local 0), its byte 0 read.
#[test]
fn a_byte_inside_a_fresh_allocation_is_accepted() -> Result<(), String> {
    let body = "i32.const 1 call $alloc local.set 0 local.get 0 i32.load8_u offset=12 drop";
    let mut r = Report::default();
    let bytes = wat::parse_str(format!(
        r#"(module (memory 1)
             (func $alloc (param i32) (result i32) i32.const 64)
             (func $main (local i32) {body}))"#
    ))
    .expect("wat");
    let bodies = decode(&bytes)?;
    check_bounds_alloc(&bodies, "c0 a0", "1:1 1:4", Sites::default(), 0, &mut r)?;
    assert_eq!(r.bounds_operands, 1);
    // one byte past the allocation
    let past = body.replace("offset=12", "offset=13");
    let bytes = wat::parse_str(format!(
        r#"(module (memory 1)
             (func $alloc (param i32) (result i32) i32.const 64)
             (func $main (local i32) {past}))"#
    ))
    .expect("wat");
    let bodies = decode(&bytes)?;
    assert!(check_bounds_alloc(&bodies, "c0 a0", "1:1 1:4", Sites::default(), 0, &mut Report::default()).is_err());
    // a call that is not $alloc (function 1 is the subject itself)
    assert!(check_bounds_alloc(&bodies, "c0 a0", "1:1 1:4", Sites::default(), 1, &mut Report::default()).is_err());
    Ok(())
}

#[test]
fn a_static_strings_byte_is_accepted() -> Result<(), String> {
    // s is a pool constant read through local 0: an address, not an offset.
    let body = "i32.const 112 local.set 0
  local.get 0 i32.load offset=4 local.set 2
  block loop
    local.get 1 local.get 2 i32.ge_u (;@;) br_if 1
    local.get 0 local.get 1 i32.add (;@;) i32.load8_u offset=12 drop
    local.get 1 i32.const 1 i32.add local.set 1
    br 0
  end end";
    assert_eq!(marked(3, body, "e0 a0")?.bounds_operands, 1);
    Ok(())
}
