//! #4816: `[permissions] fs` bounds the wasm test lane wherever it is run
//! from, and a wasm / wasi build that could not carry the scope is refused.
//! From outside the project the lane read the working directory's manifest
//! (none), so the test read a file outside the granted directory; a module
//! built with `--target wasm` / `--target wasi` read it under a host that
//! preopened `/`.

use std::path::Path;
use std::process::{Command, Output};

fn almide(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_almide")).current_dir(dir).args(args).output().expect("spawn almide")
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

fn project(root: &Path) {
    std::fs::create_dir_all(root.join("fsp/src")).unwrap();
    std::fs::create_dir_all(root.join("fsp/data")).unwrap();
    std::fs::write(root.join("fsp/almide.toml"), "[package]\nname = \"fsp\"\nversion = \"0.1.0\"\n\n[permissions]\nfs = [\"data\"]\n").unwrap();
    let outside = root.join("outside.txt");
    std::fs::write(&outside, "outside").unwrap();
    std::fs::write(
        root.join("fsp/src/r_test.almd"),
        format!("import fs\ntest \"outside\" {{\n  assert_eq(fs.read_text(\"{}\") ?? \"refused\", \"refused\")\n}}\n", outside.display()),
    )
    .unwrap();
    std::fs::write(
        root.join("fsp/src/main.almd"),
        format!("import fs\neffect fn main() -> Unit = println(fs.read_text(\"{}\") ?? \"refused\")\n", outside.display()),
    )
    .unwrap();
}

#[test]
fn the_wasm_test_lane_takes_the_scope_from_the_test_files_project() {
    let td = tempfile::TempDir::new().unwrap();
    project(td.path());
    for args in [vec!["test", "fsp/src/r_test.almd", "--target", "wasm"], vec!["test", "fsp/src/r_test.almd"]] {
        let o = almide(td.path(), &args);
        assert!(o.status.success(), "`almide {}` from outside the project read outside the scope:\n{}", args.join(" "), text(&o));
        assert!(!text(&o).contains("miscompile"), "`almide {}`:\n{}", args.join(" "), text(&o));
    }
}

#[test]
fn a_wasm_or_wasi_build_of_an_fs_scoped_project_is_refused() {
    let td = tempfile::TempDir::new().unwrap();
    project(td.path());
    let dir = td.path().join("fsp");
    for target in ["wasm", "wasi"] {
        let out = td.path().join(format!("m-{target}.wasm"));
        let o = almide(&dir, &["build", "src/main.almd", "--target", target, "-o", out.to_str().unwrap()]);
        assert_eq!(o.status.code(), Some(1), "--target {target} built an fs-scoped program:\n{}", text(&o));
        assert!(text(&o).contains("[permissions] fs cannot be carried into a wasm module"), "{}", text(&o));
        assert!(!out.exists(), "--target {target} wrote a module");
    }
}
