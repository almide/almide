//! #4932 (owner ruling 2026-11-24): the native test harness's render ships
//! its certificates to the same linked verifier as build/run/bench. A test
//! build's `panic` binds its marked message before the abort; the value
//! branch holding it must split as the program render's bare `panic` does
//! (#2769), or the abort arm sits inside an enclosing branch with a live
//! payload, a shape the ownership certificate cannot nest, and the judge
//! rejects the test render.
use std::process::Command;

#[test]
fn a_panic_arm_inside_a_value_branch_certifies_on_the_native_test_render() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("panic_arm_test.almd");
    std::fs::write(
        &file,
        "fn f(s: String) -> String = if string.len(s) > 3 then panic(\"long\") else s\n\n\
         fn g(s: String, b: Bool) -> String = {\n  \
           let t = if b then s else {\n    \
             let u = s + \"!\"\n    \
             if string.len(u) > 9 then panic(\"long\") else u\n  \
           }\n  \
           if b then \"-\" + t else t\n\
         }\n\n\
         test \"value arms with a panic\" {\n  \
           assert_eq(f(\"ab\"), \"ab\")\n  \
           assert_eq(g(\"ab\", false), \"ab!\")\n  \
           assert_eq(g(\"ab\", true), \"-ab\")\n\
         }\n",
    )
    .expect("source");
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("test")
        .arg(&file)
        .args(["--target", "rust"])
        .current_dir(dir.path())
        .output()
        .expect("almide test");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "{text}");
    assert!(!text.contains("certificate-rejected"), "{text}");
}
