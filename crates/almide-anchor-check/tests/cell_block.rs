//! #4949: a list read out of one word twice — a closure-captured `var`'s
//! cell — is one block when the two loads read the same offset from the same
//! address and nothing between them stores, writes memory or calls (the
//! trusted rule `same_loaded_block` in src/operands.rs). Each refused case is
//! a hand-written module the emitter does not produce.

use almide_anchor_check::{check_bounds, decode, Report};

mod common;

/// Local 0 is the cell; the list is the word at its offset 12. The guard
/// counts `load(load(cell, 12), 4) / 8` (compare at 7, its const at 9);
/// `between` (`n` instructions) runs after the guard; the access re-reads
/// the cell as `{cell_read}` (two instructions) and stores slot `i` (at
/// 20 + n).
fn judged(between: &str, n: usize, cell_read: &str) -> Result<Report, String> {
    let body = format!(
        "local.get 1
         local.get 0 i32.load offset=12 i32.load offset=4 i32.const 8 i32.div_u i64.extend_i32_u
         i64.ge_u
         (if (then i32.const 0 drop unreachable))
         {between}
         {cell_read} local.get 1 i32.wrap_i64 i32.const 8 i32.mul i32.add i64.const 7 i64.store offset=12"
    );
    let bytes = common::module("(local i32 i64 i64 i32 i64)", &body);
    let bodies = decode(&bytes)?;
    let mut r = Report::default();
    check_bounds(&bodies, "c0 w0", &format!("1:9 1:{}", 21 + n), &[], &mut r).map(|()| r)
}

const SAME: &str = "local.get 0 i32.load offset=12";

fn refused(r: Result<Report, String>) {
    if let Ok(r) = r {
        panic!("accepted: {} check(s), {} access(es)", r.bounds_checks, r.bounds_accesses);
    }
}

#[test]
fn the_same_cell_word_read_twice_is_one_block() -> Result<(), String> {
    let r = judged("", 0, SAME)?;
    assert_eq!((r.bounds_checks, r.bounds_accesses, r.bounds_operands), (1, 1, 1));
    // A local write and arithmetic between the loads touch no memory.
    judged("i32.const 5 local.set 3", 2, SAME).map(|_| ())
}

#[test]
fn a_store_between_the_two_loads_is_refused() {
    // Any store, even one the checker cannot place on the cell.
    refused(judged("local.get 3 i32.const 0 i32.store offset=0", 3, SAME));
    // The cell word itself rewritten.
    refused(judged("local.get 0 i32.const 64 i32.store offset=12", 3, SAME));
}

#[test]
fn a_memory_write_between_the_two_loads_is_refused() {
    refused(judged("local.get 3 i32.const 0 i32.const 4 memory.fill", 4, SAME));
}

#[test]
fn a_call_between_the_two_loads_is_refused() {
    refused(judged("i32.const 4 call 0 drop", 3, SAME));
}

#[test]
fn a_different_address_is_refused() {
    refused(judged("", 0, "local.get 3 i32.load offset=12"));
    // The same local rewritten between the loads is another address.
    refused(judged("i32.const 128 local.set 0", 2, SAME));
}

#[test]
fn a_different_offset_is_refused() {
    refused(judged("", 0, "local.get 0 i32.load offset=16"));
}
