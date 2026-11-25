//! A Set whose element is a List of Floats holds real floats on the verified
//! native route and compares them with the collection key equality (ALS-C11,
//! C-466); a Float two runtime containers deep has no native form and the
//! native build route refuses it with E081. An Option of an exact element is
//! exact (`Set[Option[Int]]`).

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

/// A `Set[List[Float]]` renders on the verified route (ALS-C11).
#[test]
fn a_set_of_float_lists_renders_on_the_verified_route() {
    let err = walled_reason(
        "float_lists",
        "fn has(s: Set[List[Float]], x: Float) -> Bool = set.contains(s, [x])\n\n\
         effect fn main() -> Unit = {\n  let s = set.from_list([[1.5], [2.5], [1.5]])\n  \
         println(\"${set.len(s)} ${has(s, 1.5)} ${has(s, 9.0)}\")\n}\n",
    );
    assert!(!err.contains("walled"), "{err}");
}

/// A Float two runtime containers deep in the element rides a lane no key
/// equality reads: the native build route refuses the set with E081 before
/// anything is rendered.
#[test]
fn a_set_of_nested_float_lists_is_refused_by_the_native_route() {
    let dir = std::env::temp_dir().join(format!("almide_set_refusal_floats_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.almd"),
        "fn has(s: Set[List[List[Float]]], x: Float) -> Bool = set.contains(s, [[x]])\n\n\
         effect fn main() -> Unit = {\n  let s = set.from_list([[[1.5]], [[2.5]], [[1.5]]])\n  \
         println(\"${set.len(s)} ${has(s, 1.5)} ${has(s, 9.0)}\")\n}\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .current_dir(&dir)
        .args(["build", "main.almd", "-o", "main"])
        .output()
        .expect("spawn almide");
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let built = dir.join("main").exists();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(!built, "a binary was written: {stderr}");
    assert!(stderr.contains("error[E081]: fn `has` holds a `Set[List[List[Float]]]`, which has no implementation on the native target"), "{stderr}");
}

/// An Option's `==` is its payload's, so a `Set[Option[Int]]` has an exact
/// element equality and renders on the verified route (#4006).
#[test]
fn a_set_of_options_renders_on_the_verified_route() {
    let err = walled_reason(
        "options",
        "fn has(s: Set[Option[Int]], k: Int) -> Bool = set.contains(s, some(k))\n\n\
         effect fn main() -> Unit = {\n  let s = set.from_list([some(1), none, some(1)])\n  \
         println(\"${set.len(s)} ${has(s, 1)} ${has(s, 2)}\")\n}\n",
    );
    assert!(!err.contains("walled"), "{err}");
    assert!(err.contains("native: v1 trust-spine render"), "{err}");
}
