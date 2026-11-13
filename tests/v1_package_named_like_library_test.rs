//! A package whose name is also a library module's (`base64`) and whose test
//! imports it as itself: its calls resolve to the package's own fns on every
//! leg. The native render dropped those bodies as ones the runtime serves
//! and then refused the call for its argument type (#4640); it now keeps the
//! bodies of a module the program itself brought.
use std::process::Command;

#[test]
fn the_package_fn_answers_on_both_legs() {
    let dir = tempfile::tempdir().expect("scratch");
    let root = dir.path().join("base64");
    std::fs::create_dir_all(root.join("src")).expect("src");
    std::fs::create_dir_all(root.join("spec")).expect("spec");
    std::fs::write(root.join("almide.toml"), "[package]\nname = \"base64\"\nversion = \"0.1.0\"\n").expect("manifest");
    std::fs::write(root.join("src/mod.almd"), "fn encode(input: String) -> String =\n  \"U:\" + input\n").expect("module");
    std::fs::write(
        root.join("spec/b_test.almd"),
        "import self as base64\n\ntest \"the package fn answers\" {\n  assert_eq(base64.encode(\"ab\"), \"U:ab\")\n}\n",
    )
    .expect("test file");
    for extra in [&["--target", "native"][..], &["--target", "wasm"][..]] {
        let out = Command::new(env!("CARGO_BIN_EXE_almide")).arg("test").args(extra).arg("spec/b_test.almd")
            .env("ALMIDE_CERTIFY_OWNERSHIP", "fail").env("ALMIDE_VERBOSE", "1")
            .current_dir(&root).output().expect("almide test");
        let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        assert_eq!(out.status.code(), Some(0), "{extra:?}: {text}");
        assert!(!text.contains("walled") && !text.contains("Unsupported"), "{extra:?} did not render: {text}");
        assert!(text.contains("1 test in 1 file"), "{extra:?}: {text}");
    }
}
