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

/// #4831: `almide bench` (native) bakes in the scope `almide run` bakes in;
/// it built the program with no scope, so a read outside `data` succeeded.
#[test]
fn bench_is_bounded_by_the_projects_fs_scope() {
    let td = tempfile::TempDir::new().unwrap();
    project(td.path());
    let outside = td.path().join("outside.txt");
    std::fs::write(
        td.path().join("fsp/src/bench_main.almd"),
        format!("import fs\neffect fn main() -> Unit = println(fs.read_text(\"{}\")!)\n", outside.display()),
    )
    .unwrap();
    let o = almide(&td.path().join("fsp"), &["bench", "src/bench_main.almd", "--runs", "1"]);
    assert!(!o.status.success(), "bench read outside [permissions] fs:\n{}", text(&o));
    assert!(text(&o).contains("is outside [permissions] fs"), "{}", text(&o));
}

/// #4833: one native test run over two projects — `bounded` with `fs =
/// ["data"]`, `open` with no `fs` key — bounds the first project's tests and
/// not the second's, every time: the scope was a process-wide value the
/// parallel per-file checks overwrote, and both projects' files were one
/// program.
#[test]
fn a_native_test_run_over_two_projects_scopes_each_by_its_own_manifest() {
    let td = tempfile::TempDir::new().unwrap();
    let root = td.path();
    let outside = root.join("outside.txt");
    std::fs::write(&outside, "outside").unwrap();
    for (name, fs_key, want) in [("bounded", "fs = [\"data\"]\n", "refused"), ("open", "", "outside")] {
        std::fs::create_dir_all(root.join(format!("{name}/data"))).unwrap();
        std::fs::write(
            root.join(format!("{name}/almide.toml")),
            format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n\n[permissions]\n{fs_key}"),
        )
        .unwrap();
        for i in 0..2 {
            std::fs::create_dir_all(root.join(format!("{name}/src"))).unwrap();
            std::fs::write(
                root.join(format!("{name}/src/r{i}_test.almd")),
                format!("import fs\ntest \"scope\" {{\n  assert_eq(fs.read_text(\"{}\") ?? \"refused\", \"{want}\")\n}}\n", outside.display()),
            )
            .unwrap();
        }
    }
    for round in 0..3 {
        let o = almide(root, &["test", ".", "--target", "native"]);
        assert!(o.status.success(), "round {round}: a test saw the other project's scope:\n{}", text(&o));
    }
}

/// #4843: `--target rust` prints the source the build compiles, the scope
/// included: the emitted source carried no scope, so a crate built from it
/// read outside `data`.
#[test]
fn emitted_rust_carries_the_projects_fs_scope() {
    let td = tempfile::TempDir::new().unwrap();
    project(td.path());
    let dir = td.path().join("fsp");
    for args in [vec!["src/main.almd", "--target", "rust"], vec!["emit", "src/main.almd", "--target", "rust"]] {
        let o = almide(&dir, &args);
        assert!(o.status.success(), "{}", text(&o));
        let src = String::from_utf8_lossy(&o.stdout);
        assert!(src.contains("__ALMIDE_FS_SCOPE_INSTALLED__"), "`almide {}` emitted no scope:\n{src}", args.join(" "));
        assert!(src.contains("__almide_fs_path!("), "`almide {}` emitted an unguarded fs call", args.join(" "));
    }
}

/// #4842: the wasmtime CLI test route preopens the granted roots, not `/`:
/// under `fs = ["data"]` a read outside `data` is refused there as on the
/// embedded lane. Skips without wasmtime unless `ALMIDE_EXPECT_TOOLS` is set.
#[test]
fn the_wasmtime_test_runner_is_bounded_by_the_fs_scope() {
    let has = Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success());
    if !has {
        assert!(std::env::var_os("ALMIDE_EXPECT_TOOLS").is_none(), "ALMIDE_EXPECT_TOOLS is set but wasmtime is not on PATH");
        return;
    }
    let td = tempfile::TempDir::new().unwrap();
    project(td.path());
    let o = Command::new(env!("CARGO_BIN_EXE_almide"))
        .current_dir(td.path().join("fsp"))
        .args(["test", "src/r_test.almd", "--target", "wasm"])
        .env("ALMIDE_TEST_WASM_RUNNER", "wasmtime")
        .output()
        .expect("spawn almide");
    assert!(o.status.success(), "the wasmtime runner read outside [permissions] fs:\n{}", text(&o));
}

/// #4842: an `ALMIDE_RUN_PROJECT_DIR` other users can write to is refused:
/// it holds the binaries a native run executes.
#[cfg(unix)]
#[test]
fn a_run_project_dir_others_can_write_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    let td = tempfile::TempDir::new().unwrap();
    let shared = td.path().join("shared");
    std::fs::create_dir(&shared).unwrap();
    std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o777)).unwrap();
    std::fs::write(td.path().join("m.almd"), "effect fn main() -> Unit = println(\"ran\")\n").unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_almide"))
        .current_dir(td.path())
        .args(["build", "m.almd", "-o", "m-bin"])
        .env("ALMIDE_RUN_PROJECT_DIR", &shared)
        .output()
        .expect("spawn almide");
    assert!(!o.status.success(), "a world-writable ALMIDE_RUN_PROJECT_DIR was used:\n{}", text(&o));
    assert!(text(&o).contains("ALMIDE_RUN_PROJECT_DIR="), "{}", text(&o));
}
