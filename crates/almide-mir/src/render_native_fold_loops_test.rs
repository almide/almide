//! `render_native_fold_loops`: the loops that become a range's fold, and
//! the ones that stay.

use super::fold_loops;

const SUM: &str = r#"    let mut v5: i64 = almd_len(v1);
    let mut v6: i64 = 1i64;
    let mut v7: i64 = 0i64;
    v7 = v6;
    let mut v8: i64 = 0i64;
    let mut v9: i64 = 1i64;
    loop {
        let mut v10: i64 = ((v8 as u64) < (v5 as u64)) as i64;
        if v10 == 0 { break; }
        // v11: a note
        let mut v13: i64 = v0.wrapping_add(v8);
        let v18: bool = v13 == v9;
        let mut v14: i64 = 0;
        if v18 {
            let mut v25: i64 = v7.wrapping_add(v13);
            v14 = v25;
        } else {
            v14 = v7;
        }
        v7 = v14;
        let mut v26: i64 = v8.wrapping_add(v9);
        v8 = v26;
    }
    rt_print(v7);
}
"#;

#[test]
fn a_reduction_over_a_range_folds_the_range() {
    let got = fold_loops(SUM.to_string());
    assert!(got.contains("    v7 = (0u64..(v5 as u64)).fold(v7, |mut v7: i64, v8: u64| -> i64 {\n        let v8: i64 = v8 as i64;\n        // v11: a note\n        let mut v13: i64 = v0.wrapping_add(v8);"), "{got}");
    assert!(got.contains("        v7 = v14;\n        v7\n    });\n    rt_print(v7);"), "{got}");
    // The counter, its test and its step are the range's now.
    assert!(!got.contains("let mut v8: i64 = 0i64;") && !got.contains("v10") && !got.contains("v26") && !got.contains("loop {"), "{got}");
}

#[test]
fn a_float_accumulator_keeps_its_type() {
    let src = SUM.replace("let mut v7: i64 = 0i64;", "let mut v7: f64 = 0.0;");
    assert!(fold_loops(src).contains("fold(v7, |mut v7: f64, v8: u64| -> f64 {"));
}

#[test]
fn a_loop_that_is_not_one_scalar_reduction_stays() {
    let same = |src: String| assert_eq!(fold_loops(src.clone()), src);
    // A second variable of the outside written in the body.
    same(SUM.replace("        v7 = v14;\n", "        v7 = v14;\n        v6 = v13;\n"));
    // No variable of the outside written at all.
    same(SUM.replace("        v7 = v14;\n", ""));
    // The body leaves early.
    same(SUM.replace("            v14 = v7;\n", "            break;\n"));
    same(SUM.replace("            v14 = v7;\n", "            return v7;\n"));
    // The counter is read after the loop, written in the body, or starts elsewhere.
    same(SUM.replace("rt_print(v7);", "rt_print(v8);"));
    same(SUM.replace("        v7 = v14;\n", "        v7 = v14;\n        v8 = v13;\n"));
    same(SUM.replace("let mut v8: i64 = 0i64;", "let mut v8: i64 = 3i64;"));
    // The bound is written in the body, or the step is not one.
    same(SUM.replace("        v7 = v14;\n", "        v7 = v14;\n        v5 = v13;\n"));
    same(SUM.replace("let mut v9: i64 = 1i64;", "let mut v9: i64 = 2i64;"));
    // A heap accumulator, an edit of an outside value, a borrow for writing.
    same(SUM.replace("let mut v7: i64 = 0i64;", "let mut v7: Vec<i64> = Vec::new();"));
    same(SUM.replace("        v7 = v14;\n", "        v7 = v14;\n        v3.push(v13);\n"));
    same(SUM.replace("        v7 = v14;\n", "        v7 = v14;\n        grow(&mut v3);\n"));
    // A signed test is the list form `render_native_loops` owns.
    same(SUM.replace("((v8 as u64) < (v5 as u64)) as i64", "(v8 < v5) as i64"));
}
