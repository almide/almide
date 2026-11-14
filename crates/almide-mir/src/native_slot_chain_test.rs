//! `native_slot_chain`: the store/take pairs that cancel, and those that stay.

use super::chain;

const TAKE: &str = "ALMD_HSLOT_0.with(|s| std::mem::take(&mut *s.borrow_mut()))";
const SNAP_TAKE: &str = "ALMD_HSLOT_0.with(|s| { let mut c = s.borrow_mut(); match std::rc::Rc::get_mut(&mut c) { Some(v) => std::mem::take(v), None => (**c).clone() } })";

fn run(src: &str) -> String {
    let mut out = src.to_string();
    chain(&mut out);
    out
}

#[test]
fn a_store_followed_by_a_take_of_the_same_slot_is_a_move() {
    let src = format!("    let mut v35: Vec<u8> = {TAKE};\n    let mut v39: Vec<u8> = put(v35, v0);\n    ALMD_HSLOT_0.with(|s| *s.borrow_mut() = v39);\n    // v42: a note\n    let mut v44: i64 = 8192i64;\n    let mut v45: Vec<u8> = {TAKE};\n    let mut v49: Vec<u8> = put(v45, v1);\n    ALMD_HSLOT_0.with(|s| *s.borrow_mut() = v49);\n");
    let want = format!("    let mut v35: Vec<u8> = {TAKE};\n    let mut v39: Vec<u8> = put(v35, v0);\n    // v42: a note\n    let mut v44: i64 = 8192i64;\n    let mut v45: Vec<u8> = v39;\n    let mut v49: Vec<u8> = put(v45, v1);\n    ALMD_HSLOT_0.with(|s| *s.borrow_mut() = v49);\n");
    assert_eq!(run(&src), want);
}

#[test]
fn a_slot_held_for_a_snapshot_cancels_the_same_way() {
    let store = |v: &str| format!("    ALMD_HSLOT_0.with(|s| {{ let mut c = s.borrow_mut(); match std::rc::Rc::get_mut(&mut c) {{ Some(v) => *v = {v}, None => *c = std::rc::Rc::new({v}) }} }});\n");
    let src = format!("{}    let mut v45: Vec<u8> = {SNAP_TAKE};\n{}", store("v40"), store("v50"));
    assert_eq!(run(&src), format!("    let mut v45: Vec<u8> = v40;\n{}", store("v50")));
}

#[test]
fn anything_between_the_two_keeps_both() {
    let same = |between: &str, take: &str| {
        let src = format!("    ALMD_HSLOT_0.with(|s| *s.borrow_mut() = v39);\n{between}    let mut v45: Vec<u8> = {take};\n");
        assert_eq!(run(&src), src);
    };
    // A call, a read of the slot, a branch, a value that is not a constant.
    same("    let mut v41: i64 = almd_peek();\n", TAKE);
    same("    almd_show();\n", TAKE);
    same("    if v3 != 0 {\n    }\n", TAKE);
    same("    let mut v41: i64 = v7.wrapping_add(v8);\n", TAKE);
    // Another slot, the other spelling, a take at another depth.
    same("", &TAKE.replace("HSLOT_0", "HSLOT_1"));
    same("", SNAP_TAKE);
    let nested = format!("    ALMD_HSLOT_0.with(|s| *s.borrow_mut() = v39);\n        let mut v45: Vec<u8> = {TAKE};\n");
    assert_eq!(run(&nested), nested);
    // A store of something other than a plain value.
    let moved = format!("    ALMD_HSLOT_0.with(|s| *s.borrow_mut() = std::mem::take(&mut v39));\n    let mut v45: Vec<u8> = {TAKE};\n");
    assert_eq!(run(&moved), moved);
}
