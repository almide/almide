//! #3286: a dependency package's top-level `let` read from the app on
//! `--target wasm`. The structural leg's front (`wasm_leg::lower_resolved`)
//! registered each module's versioned name only inside its module loop, AFTER
//! the entry program had lowered, so the entry's `lib.CENTER` use-site carried
//! the bare origin `lib` while the declaration carried the dependency's
//! versioned one. The two never met and the build walled with `var:unmapped`.
//! A same-package module has no versioned name, which is why `import self.m`
//! worked; the native driver pre-registers, which is why native worked.
//!
//! Pinned: a dependency root-module let, a dependency submodule let
//! (`import lib.view as v`), a dependency fn reading its own submodule's let,
//! and a same-package let, with wasm output identical to native.
use std::path::Path;
use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, content).expect("write");
}

fn run(app: &Path, extra: &[&str]) -> (bool, String, String) {
    let out = Command::new(almide())
        .current_dir(app)
        .args(["run", "src/main.almd"])
        .args(extra)
        .output()
        .expect("spawn almide");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn a_dependency_top_let_reads_the_same_on_wasm_as_native() {
    let dir = tempfile::tempdir().expect("tempdir");
    let lib = dir.path().join("lib");
    write(&lib.join("almide.toml"), "[package]\nname = \"lib\"\nversion = \"0.1.0\"\n");
    write(
        &lib.join("src/mod.almd"),
        "import self.view\n\nlet CENTER = 2\n\nlet NAMES = [\"left\", \"right\"]\n\nfn f(x: Int) -> Int = x + 1\n\nfn label() -> String = view.TITLE + \"!\"\n",
    );
    write(&lib.join("src/view.almd"), "let WIDTH = 7\n\nlet TITLE = \"view\"\n");
    let app = dir.path().join("app");
    write(
        &app.join("almide.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nlib = { path = \"../lib\" }\n",
    );
    write(&app.join("src/m.almd"), "let LOCAL = 100\n");
    write(
        &app.join("src/main.almd"),
        "import lib\nimport lib.view as v\nimport self.m\n\neffect fn main() -> Unit = {\n  println(int.to_string(lib.f(lib.CENTER)))\n  println(int.to_string(v.WIDTH))\n  println(v.TITLE)\n  println(list.join(lib.NAMES, \",\"))\n  println(lib.label())\n  println(int.to_string(m.LOCAL))\n}\n",
    );
    let (ok, native, err) = run(&app, &[]);
    assert!(ok, "native run failed:\n{err}");
    assert_eq!(native, "3\n7\nview\nleft,right\nview!\n100\n");

    let (ok, wasm, err) = run(&app, &["--target", "wasm"]);
    assert!(ok, "wasm run failed (was `var:unmapped`):\n{err}");
    assert_eq!(wasm, native, "wasm output must equal native");

    // The stock-WASI build route too: it walled before the run did.
    let out = app.join("a.wasm");
    let build = Command::new(almide())
        .current_dir(&app)
        .args(["build", "src/main.almd", "--target", "wasm", "-o", out.to_str().unwrap()])
        .output()
        .expect("spawn almide");
    assert!(build.status.success(), "wasm build failed:\n{}", String::from_utf8_lossy(&build.stderr));
    if Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success()) {
        let ran = Command::new("wasmtime").arg(&out).output().expect("wasmtime");
        assert_eq!(String::from_utf8_lossy(&ran.stdout), native, "the built module prints what native prints");
    }
}
