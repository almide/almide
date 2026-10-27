//! #4216: inside a module, an unqualified call to a sibling fn declared
//! `@intrinsic("sym")` resolves to `sym`, as the qualified `module.fn(..)`
//! spelling does. It was rendered as the sibling's mangled module-fn name
//! (`almide_rt_leaf___len`), which nothing defines — rustc E0425.
//!
//! The leaf module is granted stdlib authority (`--stdlib-authority`), the
//! one way a source outside the bundle may declare an `@intrinsic`.

use std::process::Command;

#[test]
fn an_unqualified_call_to_an_intrinsic_sibling_uses_its_declared_symbol() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = std::fs::canonicalize(dir.path()).expect("canonical tempdir");
    // The declared symbol (`almide_rt_string_len`) differs from the mangled
    // name of the declaring fn (`almide_rt_leaf___len`).
    std::fs::write(
        root.join("leaf.almd"),
        "@intrinsic(\"almide_rt_string_len\")\nfn __len(s: String) -> Int = _\n\npub fn size(s: String) -> Int = __len(s)\n",
    )
    .expect("write leaf");
    std::fs::write(
        root.join("main.almd"),
        "import leaf\n\nfn main() -> Unit = {\n  println(int.to_string(leaf.size(\"abc\")))\n}\n",
    )
    .expect("write main");
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["run", root.join("main.almd").to_str().expect("path"), "--stdlib-authority", root.to_str().expect("path")])
        .current_dir(&root)
        .output()
        .expect("run almide");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "almide run failed:\n{stderr}");
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "3", "stderr:\n{stderr}");
}
