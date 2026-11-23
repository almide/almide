//! The function a `judges` or `holds` site names, held to the routine's body:
//! the shipped shapes are accepted, and a site that names any other function,
//! or a routine one instruction off, is refused.

use almide_anchor_check::{check_bounds, check_named_routines, decode, At, Report};

mod common;

/// Functions, as a `call` names them: 0 `$alloc`, 1 the copy routine, 2 the
/// release, 3 `$inc`, 4 `$block_copy`, 5 `$cow`, 6 `$inc_elems`,
/// 7 `$cow_elems`, 8 a second allocator, 9 the subject. The subject calls
/// `$cow` at 1, `$cow_elems` at 4 and `$inc` at 7.
const MODULE: &str = r#"(module (memory 1) (global $floor (mut i32) (i32.const 1024))
  (func $alloc (param i32) (result i32) i32.const 64)
  (func $copy (param i32 i32 i32))
  (func $dec (param i32))
  (func $inc (param i32)
    local.get 0 global.get $floor i32.lt_u (if (then return))
    local.get 0 local.get 0 i32.load i32.const 1 i32.add i32.store)
  (func $block_copy (param i32) (result i32) (local i32 i32)
    local.get 0 i32.load offset=4 local.set 1
    local.get 1 call $alloc local.set 2
    local.get 2 i32.const 12 i32.add local.get 0 i32.const 12 i32.add local.get 1 call $copy
    local.get 2)
  (func $cow (param i32) (result i32) (local i32)
    local.get 0 global.get $floor i32.lt_u (if (then local.get 0 return))
    local.get 0 i32.load i32.const 1 i32.le_u (if (then local.get 0 return))
    local.get 0 call $block_copy local.set 1
    local.get 0 call $dec
    local.get 1)
  (func $inc_elems (param i32) (local i32 i32)
    local.get 0 i32.const 12 i32.add local.set 1
    local.get 1 local.get 0 i32.load offset=4 i32.add local.set 2
    (block (loop
      local.get 1 local.get 2 i32.ge_u br_if 1
      local.get 1 i32.load call $inc
      local.get 1 i32.const 4 i32.add local.set 1
      br 0)))
  (func $cow_elems (param i32) (result i32) (local i32)
    local.get 0 call $cow local.tee 1 local.get 0 i32.ne (if (then local.get 1 call $inc_elems))
    local.get 1)
  (func $alloc2 (param i32) (result i32) i32.const 80)
  (func $main (param i32)
    local.get 0 call $cow drop
    local.get 0 call $cow_elems drop
    local.get 0 call $inc))"#;

const SUBJECT: usize = 9;

fn named(module: &str, judges: &[usize], holds: &[usize]) -> Result<(), String> {
    let bytes = wat::parse_str(module).map_err(|e| e.to_string())?;
    let bodies = decode(&bytes)?;
    let at = |ops: &[usize]| ops.iter().map(|&op| At { func: SUBJECT, op }).collect::<Vec<_>>();
    check_named_routines(&bodies, 0, 0, &at(judges), &at(holds))
}

#[test]
fn the_shipped_judges_and_hold_are_accepted() -> Result<(), String> {
    named(MODULE, &[1, 4], &[7])
}

#[test]
fn a_judge_site_that_names_another_function_is_refused() {
    // `$inc` named as a judge.
    assert!(named(MODULE, &[7], &[]).unwrap_err().contains("not the copy-on-write judge"));
    // An instruction that is no call.
    assert!(named(MODULE, &[0], &[]).unwrap_err().contains("not a call"));
}

#[test]
fn a_hold_site_that_names_another_function_is_refused() {
    assert!(named(MODULE, &[], &[1]).unwrap_err().contains("not the +1"));
}

#[test]
fn a_judge_whose_copy_takes_another_count_is_refused() {
    // The copy is allocated at 4 bytes, not at its argument's count.
    let short = MODULE.replace("local.get 1 call $alloc local.set 2", "i32.const 4 call $alloc local.set 2");
    assert!(named(&short, &[1], &[]).unwrap_err().contains("not the copy-on-write judge"));
    // The copy is allocated by a function the anchor file does not name `$alloc`.
    let other = MODULE.replace("local.get 1 call $alloc local.set 2", "local.get 1 call $alloc2 local.set 2");
    assert!(named(&other, &[1], &[]).unwrap_err().contains("not the copy-on-write judge"));
    // Through `$cow_elems` too.
    assert!(named(&other, &[4], &[]).unwrap_err().contains("not the copy-on-write judge"));
}

#[test]
fn a_judge_one_instruction_off_is_refused() {
    // A block held twice passes through uncopied.
    let loose = MODULE.replace("i32.const 1 i32.le_u", "i32.const 2 i32.le_u");
    assert!(named(&loose, &[1], &[]).is_err());
    // The argument is handed back where the copy belongs.
    let same = MODULE.replace("local.get 0 call $dec\n    local.get 1)", "local.get 0 call $dec\n    local.get 0)");
    assert!(named(&same, &[1], &[]).is_err());
}

#[test]
fn a_hold_that_writes_the_count_word_is_refused() {
    let wide = MODULE.replace("i32.add i32.store)", "i32.add i32.store offset=4)");
    assert!(named(&wide, &[], &[7]).unwrap_err().contains("not the +1"));
    // And `$cow_elems`, whose `$inc_elems` calls it, with it.
    assert!(named(&wide, &[4], &[]).is_err());
}

/// What the check is for. `xs[i] = v` with the list rebound, between the
/// check and the store, to the result of a call the anchor file names a
/// judge: the bounds check takes the site as given and accepts, whatever the
/// function is (here it answers a block of its own). Held to the judge's
/// body, the site is refused.
#[test]
fn a_rebind_through_a_call_that_is_no_judge_is_refused() -> Result<(), String> {
    let body = "local.get 1
      local.get 0 i32.load offset=4 i64.extend_i32_u i64.const 8 i64.div_u
      i64.ge_u (if (then i32.const 0 drop unreachable))
      local.get 0 call $alloc local.set 3 local.get 3 local.set 0
      local.get 3 local.get 1 i32.wrap_i64 i32.const 8 i32.mul i32.add i64.const 5 i64.store offset=12";
    let bytes = common::module("(local i32 i64 i64 i32 i64)", body);
    let bodies = decode(&bytes)?;
    let judge = [At { func: 1, op: 13 }];
    let mut r = Report::default();
    check_bounds(&bodies, "c0 w0", "1:8 1:24", &judge, &mut r)?;
    assert!(check_named_routines(&bodies, 0, 0, &judge, &[]).unwrap_err().contains("not the copy-on-write judge"));
    Ok(())
}
