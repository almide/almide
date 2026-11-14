//! `render_native_spine_clone`: which types get the loop copy, and its text.

use super::*;

fn case(tag: u32, own_box: &[bool]) -> Case {
    Case { tag, own_box: own_box.to_vec() }
}

#[test]
fn a_chain_of_the_types_own_last_field_is_copied_in_a_loop() {
    // enum T { C0, C1(Box<T>), C2(Box<T>, i64, Box<T>), C3(String) }
    let text = clone_impl("T", &[case(0, &[]), case(1, &[true]), case(2, &[true, false, true]), case(3, &[false])]).expect("a loop copy");
    assert_eq!(
        text,
        "impl Clone for T {\n    fn clone(&self) -> Self {\n        let mut root = Self::C0;\n        let mut src = self;\n        let mut dst = &mut root;\n        loop {\n            match src {\n                Self::C0 => {\n                    std::mem::forget(std::mem::replace(dst, Self::C0));\n                    break;\n                }\n                Self::C1(f0) => {\n                    std::mem::forget(std::mem::replace(dst, Self::C1(Box::new(Self::C0))));\n                    dst = match dst { Self::C1(d) => &mut **d, _ => unreachable!() };\n                    src = &**f0;\n                }\n                Self::C2(f0, f1, f2) => {\n                    std::mem::forget(std::mem::replace(dst, Self::C2(f0.clone(), f1.clone(), Box::new(Self::C0))));\n                    dst = match dst { Self::C2(_, _, d) => &mut **d, _ => unreachable!() };\n                    src = &**f2;\n                }\n                Self::C3(f0) => {\n                    std::mem::forget(std::mem::replace(dst, Self::C3(f0.clone())));\n                    break;\n                }\n            }\n        }\n        root\n    }\n}\n"
    );
}

#[test]
fn the_placeholder_is_the_first_case_without_fields() {
    let text = clone_impl("T", &[case(0, &[false, true]), case(4, &[]), case(5, &[])]).expect("a loop copy");
    assert!(text.contains("let mut root = Self::C4;"), "{text}");
    assert!(text.contains("Self::C0(f0.clone(), Box::new(Self::C4))"), "{text}");
}

#[test]
fn a_type_with_no_fieldless_case_or_no_own_last_field_keeps_the_derived_copy() {
    // enum E { Num(i64), Add(Box<E>, Box<E>), Neg(Box<E>) }: nothing to put in the hole.
    assert!(clone_impl("E", &[case(0, &[false]), case(1, &[true, true]), case(2, &[true])]).is_none());
    // The type's own box is not the last field of any case.
    assert!(clone_impl("T", &[case(0, &[]), case(1, &[true, false])]).is_none());
    // No recursion at all.
    assert!(clone_impl("T", &[case(0, &[]), case(1, &[false])]).is_none());
}
