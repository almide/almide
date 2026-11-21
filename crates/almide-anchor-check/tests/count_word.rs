//! The count word between a check and its access: a write the checker can
//! read as landing on it is refused, whatever the store's width or kind.
//! Each refused case is a hand-written module the emitter does not produce.

use almide_anchor_check::{check_bounds, decode, Report};

mod common;

/// The `xs[i]` read of check.rs (`READ`) with `write`, of `n` instructions,
/// between the guard and the access: the compare at 6, the guard's const at
/// 8, the load at 18 + `n`. Local 0 is the list block.
fn judged(write: &str, n: usize) -> Result<Report, String> {
    let body = format!(
        "local.get 1
         local.get 0 i32.load offset=4 i32.const 8 i32.div_u i64.extend_i32_u
         i64.ge_u
         (if (then i32.const 0 drop unreachable))
         {write}
         local.get 0 local.get 1 i32.wrap_i64 i32.const 8 i32.mul i32.add i64.load offset=12 drop"
    );
    let bytes = common::module("(local i32 i64 i64 i32 i64)", &body);
    let bodies = decode(&bytes)?;
    let mut r = Report::default();
    check_bounds(&bodies, "c0 a0", &format!("1:8 1:{}", 18 + n), &[], &mut r).map(|()| r)
}

/// `r` is a refusal.
fn refused(r: Result<Report, String>) {
    if let Ok(r) = r {
        panic!("accepted: {} check(s), {} access(es)", r.bounds_checks, r.bounds_accesses);
    }
}

/// The reference-count word, and the capacity word after the count: neither
/// store reaches the count word.
#[test]
fn a_store_beside_the_count_word_is_accepted() -> Result<(), String> {
    judged("local.get 0 i32.const 2 i32.store offset=0", 3)?;
    judged("local.get 0 i32.const 64 i32.store offset=8", 3)?;
    Ok(())
}

/// A slot of the payload.
#[test]
fn a_store_into_the_payload_is_accepted() -> Result<(), String> {
    judged("local.get 0 i64.const 7 i64.store offset=12", 3).map(|_| ())
}

#[test]
fn a_store_of_the_count_word_is_refused() {
    refused(judged("local.get 0 i32.const 0 i32.store offset=4", 3));
    // The same address with the constant split off.
    refused(judged("local.get 0 i32.const 4 i32.add i32.const 0 i32.store", 5));
}

/// Eight bytes from the block's address cover the count word.
#[test]
fn a_wide_store_over_the_count_word_is_refused() {
    refused(judged("local.get 0 i64.const 0 i64.store offset=0", 3));
}

/// Eight bytes from the count word itself.
#[test]
fn a_wide_store_from_the_count_word_is_refused() {
    refused(judged("local.get 0 i64.const 0 i64.store offset=4", 3));
}

/// One byte inside the count word.
#[test]
fn a_byte_store_into_the_count_word_is_refused() {
    refused(judged("local.get 0 i32.const 0 i32.store8 offset=5", 3));
}

/// A fill that starts at the count word.
#[test]
fn a_fill_from_the_count_word_is_refused() {
    refused(judged("local.get 0 i32.const 4 i32.add i32.const 0 i32.const 4 memory.fill", 6));
}

/// A copy that starts at the block's address: its length is not static, so
/// it is taken to reach the count word.
#[test]
fn a_copy_over_the_header_is_refused() {
    refused(judged("local.get 0 local.get 3 i32.const 8 memory.copy", 4));
}

/// An atomic store of the count word: the checks follow no atomic access,
/// so the module is not judged.
#[test]
fn an_atomic_store_of_the_count_word_is_refused() {
    refused(judged("local.get 0 i32.const 0 i32.atomic.store offset=4", 3));
}
