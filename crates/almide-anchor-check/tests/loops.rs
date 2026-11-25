//! The loop cutter (src/loops.rs): which accesses it proposes, and the byte
//! strings it hands the kernel. The verdict on a proposal is
//! `proofs/GuardLoop.v`'s; these tests hold the SEARCH, and the first one's
//! bytes are the proposal `cutter_for_read` in that file proves accepted.

use almide_anchor_check::loops::{shipped, LoopRead};

/// A module whose function 0 stands for the increment routine and whose
/// function 1 holds `body`: param `$xs` (0), locals `$n` (1), `$i` (2) and
/// `$v` (3).
fn module(body: &str) -> Vec<u8> {
    let text = format!(
        r#"(module
             (memory 1)
             (func $inc (param i32))
             (func (param $xs i32)
               (local $n i32) (local $i i32) (local $v i32)
               {body}))"#
    );
    wat::parse_str(&text).expect("the test module parses")
}

fn found(body: &str) -> Vec<LoopRead> {
    shipped(&module(body), Some(0)).expect("the module decodes")
}

/// The loop's own share of the list, taken right before the capture.
const HOLD: &str = "
    local.get $xs
    call $inc";

/// A `for` over 4-byte slots: the capture, the counter's start, the loop with
/// its exit test, the read, the step.
const FOR: &str = "
    local.get $xs
    i32.load offset=4
    i32.const 4
    i32.div_u
    local.set $n
    i32.const 0
    local.set $i
    block
      loop
        local.get $i
        local.get $n
        i32.ge_u
        br_if 1
        local.get $xs
        local.get $i
        i32.const 4
        i32.mul
        i32.add
        i32.load offset=12
        local.set $v
        local.get $i
        i32.const 1
        i32.add
        local.set $i
        br 0
      end
    end";

#[test]
fn the_read_of_a_for_is_one_proposal_and_its_bytes_are_the_instructions() {
    let found = found(&format!("{HOLD}{FOR}"));
    assert_eq!(found.len(), 1);
    let f = &found[0];
    assert_eq!((f.func, f.access), (1, 20));
    assert_eq!(f.hold, vec![32, 0, 16, 0]);
    assert_eq!(f.pre, vec![32, 0, 40, 2, 4, 65, 4, 110, 33, 1]);
    assert_eq!(f.mid, vec![65, 0, 33, 2, 2, 64, 3, 64]);
    assert_eq!(f.ht, vec![32, 2, 32, 1, 79, 13, 1, 32, 0, 32, 2, 65, 4, 108, 106, 40, 2, 12]);
    assert_eq!(f.rest, vec![33, 3, 32, 2, 65, 1, 106, 33, 2, 12, 0, 11, 11]);
}

#[test]
fn a_capture_with_no_hold_before_it_carries_none() {
    let found = found(FOR);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].hold, Vec::<u8>::new());
    assert_eq!(found[0].access, 18);
    // ... and a call of a function the caller does not name is no hold.
    let unnamed = shipped(&module(&format!("{HOLD}{FOR}")), None).expect("the module decodes");
    assert_eq!(unnamed[0].hold, Vec::<u8>::new());
}

#[test]
fn a_read_one_slot_further_is_still_proposed() {
    // The cutter tests no bound: this one is the kernel's to refuse
    // (`refuses_cutter_read_one_slot_further` in the proof script).
    let found = found(&FOR.replace("i32.load offset=12", "i32.load offset=16"));
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].ht, vec![32, 2, 32, 1, 79, 13, 1, 32, 0, 32, 2, 65, 4, 108, 106, 40, 2, 16]);
}

#[test]
fn a_byte_loop_captures_the_length_itself() {
    let body = "
        local.get $xs
        i32.load offset=4
        local.set $n
        i32.const 0
        local.set $i
        block
          loop
            local.get $i
            local.get $n
            i32.ge_u
            br_if 1
            local.get $xs
            local.get $i
            i32.add
            i32.load8_u offset=12
            local.set $v
            local.get $i
            i32.const 1
            i32.add
            local.set $i
            br 0
          end
        end";
    let found = found(body);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].pre, vec![32, 0, 40, 2, 4, 33, 1]);
    assert_eq!(found[0].ht, vec![32, 2, 32, 1, 79, 13, 1, 32, 0, 32, 2, 106, 45, 0, 12]);
}

#[test]
fn a_read_of_another_block_or_by_another_index_is_not_proposed() {
    // The address must be built from the captured block's local and the
    // tested counter, and from nothing else.
    let other_block = FOR.replacen("br_if 1\n        local.get $xs", "br_if 1\n        local.get $v", 1);
    assert_ne!(other_block, FOR);
    assert_eq!(found(&other_block).len(), 0);
    let other_index = FOR.replacen("local.get $xs\n        local.get $i\n        i32.const 4", "local.get $xs\n        local.get $v\n        i32.const 4", 1);
    assert_ne!(other_index, FOR);
    assert_eq!(found(&other_index).len(), 0);
}

#[test]
fn a_count_that_is_not_a_capture_is_not_proposed() {
    let constant = FOR.replacen("local.get $xs\n    i32.load offset=4\n    i32.const 4\n    i32.div_u\n    local.set $n", "i32.const 9\n    local.set $n", 1);
    assert_ne!(constant, FOR);
    assert_eq!(found(&constant).len(), 0);
}

#[test]
fn a_test_of_another_shape_is_not_proposed() {
    // Only `counter >= count` leaving the loop is the shape; anything else
    // is not counted, which the gate's floor notices.
    assert_eq!(found(&FOR.replacen("i32.ge_u", "i32.gt_u", 1)).len(), 0);
}

#[test]
fn a_capture_in_a_block_that_closed_is_not_proposed() {
    let body = format!(
        "block{}\n    end{}",
        "\n    local.get $xs\n    i32.load offset=4\n    i32.const 4\n    i32.div_u\n    local.set $n",
        FOR.replacen("local.get $xs\n    i32.load offset=4\n    i32.const 4\n    i32.div_u\n    local.set $n", "", 1)
    );
    assert_eq!(found(&body).len(), 0);
}
