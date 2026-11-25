//! The window cutter (src/window.rs): which loads and stores it proposes a
//! guarded-access window for, and the bytes it hands the kernel. The verdict
//! on a window is `proofs/GuardWindow.v`'s; these tests hold the SEARCH to
//! the shapes that proof accepts, and the first one's bytes are the list
//! `cutter_read_window` in that file proves accepted.

use almide_anchor_check::window::shipped;

fn module(body: &str) -> Vec<u8> {
    let text = format!(
        r#"(module
             (memory 1)
             (func $abort (param i32))
             (func $other (param i32))
             (func (param $b i32) (param $i i64) (param $j i64) (result i64)
               (local $cnt i32)
               {body}))"#
    );
    wat::parse_str(&text).expect("the test module parses")
}

/// The emitter's `xs[i]`: the compare, the aborting arm, the load.
const READ: &str = "
    local.get $i
    local.get $b
    i32.load offset=4
    i32.const 8
    i32.div_u
    i64.extend_i32_u
    i64.ge_u
    if
      i32.const 1024
      call $abort
      unreachable
    end
    local.get $b
    local.get $i
    i32.wrap_i64
    i32.const 8
    i32.mul
    i32.add
    i64.load offset=12";

#[test]
fn the_guarded_read_is_one_window_and_its_bytes_are_the_instructions() {
    let (accesses, windows) = shipped(&module(READ)).expect("the module decodes");
    // The length read and the element read; only the element read has a window.
    assert_eq!(accesses, 2);
    assert_eq!(windows.len(), 1);
    assert_eq!((windows[0].func, windows[0].access), (2, 18));
    #[rustfmt::skip]
    let want: Vec<u8> = vec![
        32, 1, 32, 0, 40, 2, 4, 65, 8, 110, 173, 90,
        4, 64, 65, 128, 8, 16, 0, 0, 11,
        32, 0, 32, 1, 167, 65, 8, 108, 106, 41, 3, 12,
    ];
    assert_eq!(windows[0].bytes, want);
}

fn proposed(body: &str) -> usize {
    shipped(&module(body)).expect("the module decodes").1.len()
}

#[test]
fn a_count_read_before_a_call_is_not_a_window() {
    // The count is taken once, a call runs, and the compare reads the local:
    // the window cannot reach back over the call to the length read.
    let body = "
        local.get $b
        i32.load offset=4
        i32.const 8
        i32.div_u
        local.set $cnt
        local.get $b
        call $other
        local.get $i
        local.get $cnt
        i64.extend_i32_u
        i64.ge_u
        if
          i32.const 1024
          call $abort
          unreachable
        end
        local.get $b
        local.get $i
        i32.wrap_i64
        i32.const 8
        i32.mul
        i32.add
        i64.load offset=12";
    assert_eq!(proposed(body), 0);
}

#[test]
fn a_store_between_the_compare_and_the_access_is_not_a_window() {
    let body = READ.replace(
        "    local.get $b\n    local.get $i\n    i32.wrap_i64",
        "    local.get $b\n    i32.const 0\n    i32.store offset=4\n    local.get $b\n    local.get $i\n    i32.wrap_i64",
    );
    assert_ne!(body, READ);
    assert_eq!(proposed(&body), 0);
}

#[test]
fn an_arm_that_does_not_abort_is_not_a_window() {
    let body = READ.replace("      unreachable\n", "");
    assert_ne!(body, READ);
    assert_eq!(proposed(&body), 0);
}

#[test]
fn an_access_at_another_index_is_not_a_window() {
    let body = READ.replace("    local.get $i\n    i32.wrap_i64", "    local.get $j\n    i32.wrap_i64");
    assert_ne!(body, READ);
    assert_eq!(proposed(&body), 0);
}

#[test]
fn a_count_in_bytes_is_not_a_window() {
    let body = READ.replace("    i32.const 8\n    i32.div_u\n", "");
    assert_ne!(body, READ);
    assert_eq!(proposed(&body), 0);
}

#[test]
fn an_access_one_slot_on_is_not_a_window() {
    let body = READ.replace("i64.load offset=12", "i64.load offset=20");
    assert_ne!(body, READ);
    assert_eq!(proposed(&body), 0);
}
