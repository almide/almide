//! The fresh-block cutter (src/fresh.rs): which accesses it proposes, and the
//! three byte strings it hands the kernel. The verdict on a proposal is
//! `proofs/GuardFresh.v`'s; these tests hold the SEARCH, and the first one's
//! bytes are the triple `cutter_fresh_store` in that file proves accepted.

use almide_anchor_check::fresh::shipped;

/// A module whose function 0 stands for the allocator and whose function 2
/// holds `body`: params `$v` (0) and `$w` (1), locals `$x` (2) and `$y` (3).
fn module(body: &str) -> Vec<u8> {
    let text = format!(
        r#"(module
             (memory 1)
             (func $alloc (param i32) (result i32) i32.const 0)
             (func $other (param i32) (result i32) local.get 0)
             (func (param $v i64) (param $w i32)
               (local $x i32) (local $y i32)
               {body}))"#
    );
    wat::parse_str(&text).expect("the test module parses")
}

fn proposed(body: &str) -> usize {
    shipped(&module(body), 0).expect("the module decodes").len()
}

/// A record being filled: the allocation, a value computed by a call, the store.
const STORE: &str = "
    i32.const 16
    call $alloc
    local.set $x
    local.get $w
    call $other
    local.set $y
    local.get $x
    local.get $v
    i64.store offset=20";

#[test]
fn the_store_into_a_fresh_block_is_one_proposal_and_its_bytes_are_the_instructions() {
    let found = shipped(&module(STORE), 0).expect("the module decodes");
    assert_eq!(found.len(), 1);
    let f = &found[0];
    assert_eq!((f.func, f.access), (2, 8));
    assert_eq!(f.pre, vec![65, 16, 16, 0, 33, 2]);
    assert_eq!(f.mid, vec![32, 1, 16, 1, 33, 3]);
    assert_eq!(f.tail, vec![32, 2, 32, 0, 55, 3, 20]);
}

#[test]
fn a_store_past_the_block_is_still_proposed() {
    // The cutter tests no bound: this one is the kernel's to refuse
    // (`refuses_cutter_store_past_the_block` in the proof script).
    let found = shipped(&module(&STORE.replace("offset=20", "offset=28")), 0).expect("the module decodes");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].tail, vec![32, 2, 32, 0, 55, 3, 28]);
}

#[test]
fn a_constant_added_to_the_local_and_a_load_are_proposed() {
    let body = "
        i32.const 16
        call $alloc
        local.set $x
        local.get $x
        i32.const 8
        i32.add
        local.get $v
        i64.store offset=12
        local.get $x
        i64.load offset=12
        drop";
    let found = shipped(&module(body), 0).expect("the module decodes");
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].mid, Vec::<u8>::new());
    assert_eq!(found[0].tail, vec![32, 2, 65, 8, 106, 32, 0, 55, 3, 12]);
    assert_eq!(found[1].tail, vec![32, 2, 41, 3, 12]);
}

#[test]
fn a_local_assigned_again_is_not_proposed() {
    // The latest assignment of the address's local is not the allocation.
    let body = "
        i32.const 16
        call $alloc
        local.set $x
        local.get $w
        local.set $x
        local.get $x
        local.get $v
        i64.store offset=12";
    assert_eq!(proposed(body), 0);
}

#[test]
fn another_callee_or_a_computed_size_is_not_proposed() {
    assert_eq!(proposed(&STORE.replacen("call $alloc", "call $other", 1)), 0);
    assert_eq!(proposed(&STORE.replacen("i32.const 16", "local.get $w", 1)), 0);
    // ... and the function the caller names is the only allocator.
    assert_eq!(shipped(&module(STORE), 1).expect("the module decodes").len(), 0);
}

#[test]
fn an_access_in_a_block_opened_after_the_allocation_is_not_proposed() {
    // The three strings must lie at one control depth.
    let body = "
        i32.const 16
        call $alloc
        local.set $x
        block
          local.get $x
          local.get $v
          i64.store offset=12
        end";
    assert_eq!(proposed(body), 0);
}

#[test]
fn an_allocation_in_a_block_that_closed_is_not_proposed() {
    let body = "
        block
          i32.const 16
          call $alloc
          local.set $x
        end
        local.get $x
        local.get $v
        i64.store offset=12";
    assert_eq!(proposed(body), 0);
}

#[test]
fn a_whole_block_in_between_is_proposed_with_its_bytes() {
    // Calls, stores and whole blocks may lie in between: the proof script's
    // scan reads them and holds them to leaving the local alone.
    let body = "
        i32.const 16
        call $alloc
        local.set $x
        block
          local.get $w
          call $other
          drop
        end
        local.get $x
        local.get $v
        i64.store offset=12";
    let found = shipped(&module(body), 0).expect("the module decodes");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].mid, vec![2, 64, 32, 1, 16, 1, 26, 11]);
}
