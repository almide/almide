//! A dependency's bare `import llm` names the dependency's own `llm`
//! package, never the root package's `src/llm.almd` (#4270).
//!
//! The root's `import self.llm` was registered under the bare key `llm`, the
//! key a dependency's `import llm` loads its package under. With the root's
//! module still loading, the dependency's import found that key and the
//! check stopped with `circular import detected: llm`; a root module loaded
//! first would have answered the dependency's import with the wrong file.
//! The root's module now keeps `self.llm` when a dependency is named `llm`,
//! as a module named like a stdlib module already did (#2864).

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, content).expect("write");
}

/// llm <- dep <- root, where root also has its own `src/llm.almd`.
fn scratch() -> PathBuf {
    let base = std::env::temp_dir().join(format!("almide-issue4270-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    write(&base.join("llm").join("almide.toml"), "[package]\nname = \"llm\"\nversion = \"0.1.0\"\n");
    write(&base.join("llm").join("src").join("mod.almd"), "fn hi() -> String = \"hi\"\n");
    write(
        &base.join("dep").join("almide.toml"),
        "[package]\nname = \"dep\"\nversion = \"0.1.0\"\n\n[dependencies]\nllm = { path = \"../llm\" }\n",
    );
    write(&base.join("dep").join("src").join("mod.almd"), "import llm\n\nfn greet() -> String = llm.hi()\n");
    write(
        &base.join("root").join("almide.toml"),
        "[package]\nname = \"root\"\nversion = \"0.1.0\"\n\n[dependencies]\ndep = { path = \"../dep\" }\n",
    );
    write(&base.join("root").join("src").join("llm.almd"), "import dep\n\nfn ask() -> String = dep.greet() + \"!\"\n");
    write(&base.join("root").join("src").join("main.almd"), "import self.llm\n\neffect fn main() -> Unit = println(llm.ask())\n");
    base.join("root")
}

#[test]
fn a_dependency_import_does_not_resolve_to_the_root_module_of_the_same_name() {
    let root = scratch();
    let check = Command::new(almide())
        .args(["check", "src/main.almd"])
        .current_dir(&root)
        .output()
        .expect("spawn almide");
    let text = format!("{}{}", String::from_utf8_lossy(&check.stdout), String::from_utf8_lossy(&check.stderr));
    assert!(check.status.success(), "check failed:\n{text}");
    for target in ["rust", "wasm"] {
        let out = Command::new(almide())
            .args(["run", "src/main.almd", "--target", target])
            .current_dir(&root)
            .output()
            .expect("spawn almide");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{target} run failed:\nstdout: {stdout}\nstderr: {stderr}");
        assert_eq!(stdout, "hi!\n", "wrong {target} output");
    }
}
