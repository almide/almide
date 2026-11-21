//! One linear memory, unshared and 32-bit, is what every rule that compares
//! two addresses takes for granted: a count read from one memory bounds
//! nothing in another, and a shared memory can be written by another thread
//! between a check and its access. Each refused case is a hand-written module
//! the emitter does not produce.

use almide_anchor_check::{check_bounds, decode, Report};

/// The `xs[i]` read of check.rs (`READ`): the compare at 6, the guard's const
/// at 8, the load at 18. `count` and `access` name the memory each reads
/// (empty: memory 0).
fn judged(memories: &str, count: &str, access: &str) -> Result<Report, String> {
    let bytes = wat::parse_str(format!(
        r#"(module {memories}
             (func $alloc (param i32) (result i32) i32.const 64)
             (func $main (local i32 i64)
               local.get 1
               local.get 0 i32.load {count} offset=4 i32.const 8 i32.div_u i64.extend_i32_u
               i64.ge_u
               (if (then i32.const 0 drop unreachable))
               local.get 0 local.get 1 i32.wrap_i64 i32.const 8 i32.mul i32.add i64.load {access} offset=12 drop))"#
    ))
    .unwrap_or_else(|e| panic!("the case is not a module: {e}"));
    let bodies = decode(&bytes)?;
    let mut r = Report::default();
    check_bounds(&bodies, "c0 a0", "1:8 1:18", &[], &mut r).map(|()| r)
}

/// `r` is a refusal, and for the reason `why`.
fn refused(r: Result<Report, String>, why: &str) {
    match r {
        Ok(r) => panic!("accepted: {} check(s), {} access(es)", r.bounds_checks, r.bounds_accesses),
        Err(e) => assert!(e.contains(why), "refused for another reason: {e}"),
    }
}

#[test]
fn a_read_in_the_one_memory_is_accepted() -> Result<(), String> {
    let r = judged("(memory 1)", "", "")?;
    assert_eq!((r.bounds_checks, r.bounds_accesses), (1, 1));
    Ok(())
}

/// An imported memory is the one memory as well as a declared one.
#[test]
fn a_read_in_an_imported_memory_is_accepted() -> Result<(), String> {
    judged(r#"(import "env" "m" (memory 1))"#, "", "").map(|_| ())
}

/// The count comes from memory 1, the element from memory 0: the compare
/// bounds nothing the load touches.
#[test]
fn a_count_read_from_another_memory_is_refused() {
    refused(judged("(memory 1) (memory 1)", "1", ""), "more than one memory");
}

/// The same with the access in the second memory.
#[test]
fn an_access_to_another_memory_is_refused() {
    refused(judged("(memory 1) (memory 1)", "", "1"), "more than one memory");
}

/// A second memory, imported: no rule looks at which memory an instruction
/// names, so the module is not judged whether or not anything reads it.
#[test]
fn an_imported_second_memory_is_refused() {
    refused(judged(r#"(import "env" "m" (memory 1)) (memory 1)"#, "", ""), "more than one memory");
}

/// A shared memory: another thread may write the count word between the
/// check and the access.
#[test]
fn a_shared_memory_is_refused() {
    refused(judged("(memory 1 1 shared)", "", ""), "shared");
}
