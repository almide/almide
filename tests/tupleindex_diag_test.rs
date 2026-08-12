//! #1266: `.k` on a non-tuple and `.k` out of range both sailed through
//! `almide check` ("No errors found"), then died at build behind the
//! ConcretizeTypes [COMPILER BUG] banner — telling the user their own type
//! error was ours. Both halves are check-time E045 now, and each diagnostic
//! must carry enough to fix the site from the message alone: the non-tuple
//! case names the offending type, the out-of-range case names the arity.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn check(dir: &std::path::Path, source: &str) -> String {
    let file = dir.join("ti.almd");
    std::fs::write(&file, source).expect("write fixture");
    let out = Command::new(almide())
        .arg("check")
        .arg(&file)
        .output()
        .expect("run almide check");
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn tuple_index_on_non_tuple_is_e045_naming_the_type() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = check(
        dir.path(),
        "effect fn main() -> Unit = {\n  let n = 5\n  println(int.to_string(n.0))\n}\n",
    );
    assert!(out.contains("E045"), ".0 on Int must be E045, got:\n{out}");
    assert!(
        out.contains("non-tuple type Int"),
        "the diagnostic must NAME the non-tuple type, got:\n{out}"
    );
}

#[test]
fn tuple_index_out_of_range_is_e045_naming_the_arity() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = check(
        dir.path(),
        "effect fn main() -> Unit = {\n  let t = (1, 2)\n  println(int.to_string(t.5))\n}\n",
    );
    assert!(out.contains("E045"), ".5 on a pair must be E045, got:\n{out}");
    assert!(
        out.contains("valid: .0 through .1"),
        "the diagnostic must NAME the valid range, got:\n{out}"
    );
}

#[test]
fn in_range_tuple_index_still_checks_clean() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = check(
        dir.path(),
        "effect fn main() -> Unit = {\n  let t = (1, \"a\")\n  println(int.to_string(t.0))\n  println(t.1)\n}\n",
    );
    assert!(
        out.contains("No errors found"),
        "valid tuple indexing must stay clean, got:\n{out}"
    );
}
