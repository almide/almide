//! A Set whose element has no exact runtime equality (`Set[List[Float]]`,
//! `Set[Option[Int]]`) is refused by the verified native render, which then
//! builds on the standard codegen (#4799). The refusal arms of the set element
//! checks became `wall`s; the program-level refusal a user sees must stay the
//! one it was: the fn naming the set falls outside the native rung subset.

use std::process::Command;

fn walled_reason(tag: &str, src: &str) -> String {
    let dir = std::env::temp_dir().join(format!("almide_set_refusal_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.almd"), src).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .current_dir(&dir)
        .env("ALMIDE_VERIFIED_DEBUG", "1")
        .args(["build", "main.almd", "-o", "main"])
        .output()
        .expect("spawn almide");
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let run = Command::new(dir.join("main")).output().expect("run the built program");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(out.status.success(), "{stderr}");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "2 true false\n");
    stderr
}

#[test]
fn a_set_of_float_lists_is_refused_outside_the_rung_subset() {
    let err = walled_reason(
        "floats",
        "fn has(s: Set[List[Float]], x: Float) -> Bool = set.contains(s, [x])\n\n\
         effect fn main() -> Unit = {\n  let s = set.from_list([[1.5], [2.5], [1.5]])\n  \
         println(\"${set.len(s)} ${has(s, 1.5)} ${has(s, 9.0)}\")\n}\n",
    );
    assert!(err.contains("verified native render walled"), "{err}");
    assert!(err.contains("native: fn `has` param `Applied(Set, [Applied(List, [Float])])` type — outside the native rung subset"), "{err}");
}

#[test]
fn a_set_of_options_is_refused_outside_the_rung_subset() {
    let err = walled_reason(
        "options",
        "fn has(s: Set[Option[Int]], k: Int) -> Bool = set.contains(s, some(k))\n\n\
         effect fn main() -> Unit = {\n  let s = set.from_list([some(1), none, some(1)])\n  \
         println(\"${set.len(s)} ${has(s, 1)} ${has(s, 2)}\")\n}\n",
    );
    assert!(err.contains("verified native render walled"), "{err}");
    assert!(err.contains("native: fn `has` param `Applied(Set, [Applied(Option, [Int])])` type — outside the native rung subset"), "{err}");
}
