//! #4005 A4: a heap parameter is declared a block for the linear
//! certificate's translation, and a block is never the null address. A body
//! that tests the parameter against 0 (an Option's `none` is NULL_ADDR) is
//! refused rather than trusted; the same body without the test translates.

use std::collections::{BTreeMap, BTreeSet};

use almide_anchor_check::lin::{translate, Module};

fn translated(body: &str) -> Result<(), String> {
    let bytes = wat::parse_str(format!(
        r#"(module (memory 1)
             (func $alloc (param i32) (result i32) i32.const 64)
             (func $f (param i32) (result i32) {body}))"#
    ))
    .expect("wat");
    let m = Module::read(&bytes)?;
    let heap: BTreeMap<u32, Option<i128>> = [(0, None)].into_iter().collect();
    translate(&m, 1, &BTreeSet::new(), &heap, &BTreeSet::new(), 0).map(|_| ())
}

const READ: &str = "local.get 0 i32.load offset=4";

#[test]
fn a_header_read_of_a_declared_block_translates() {
    assert_eq!(translated(READ), Ok(()));
}

#[test]
fn a_declared_block_tested_with_eqz_is_refused() {
    let body = format!("local.get 0 i32.eqz if (result i32) i32.const 0 else {READ} end");
    assert!(translated(&body).unwrap_err().contains("tested against null"));
}

#[test]
fn a_declared_block_used_as_a_condition_is_refused() {
    let body = format!("local.get 0 if (result i32) {READ} else i32.const 0 end");
    assert!(translated(&body).unwrap_err().contains("tested against null"));
}

#[test]
fn a_declared_block_compared_with_zero_is_refused() {
    for cmp in ["local.get 0 i32.const 0 i32.ne", "i32.const 0 local.get 0 i32.eq"] {
        let body = format!("{cmp} if (result i32) {READ} else i32.const 0 end");
        assert!(translated(&body).unwrap_err().contains("tested against null"), "{cmp}");
    }
}
