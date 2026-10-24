//! `[permissions]` bound every route (the #4151–#4159 audit).
//!
//! Three gaps let a program the manifest forbids run:
//! - the manifest was read from the working directory, not the entry file's
//!   project, so `almide run proj/src/main.almd` from elsewhere had none;
//! - `almide check` lowered only the entry file, so an operation inside a
//!   user module was never charged;
//! - the wasm test lane (`almide test --target wasm`, and the wasm-first
//!   default) ran the test without consulting the manifest.
//!
//! Each route is refused under `allow = ["IO", "Abort"]` for a file read,
//! and the control grants `FS` and sees the same program pass.

use std::path::Path;
use std::process::{Command, Output};

const SECRET: &str = "secret-routes";

fn project(root: &Path, allow: &str) {
    std::fs::create_dir_all(root.join("proj/src")).unwrap();
    std::fs::create_dir_all(root.join("elsewhere")).unwrap();
    std::fs::write(
        root.join("proj/almide.toml"),
        format!("[package]\nname = \"routes\"\nversion = \"0.1.0\"\n\n[permissions]\nallow = [{allow}]\n"),
    )
    .unwrap();
    std::fs::write(root.join("proj/s.txt"), SECRET).unwrap();
    std::fs::write(root.join("elsewhere/s.txt"), SECRET).unwrap();
    std::fs::write(root.join("proj/src/helper.almd"), "import fs\neffect fn read() -> String = fs.read_text(\"s.txt\")!\n").unwrap();
    std::fs::write(root.join("proj/src/main.almd"), "import helper\neffect fn main() -> Unit = println(helper.read()!)\n").unwrap();
    std::fs::write(
        root.join("proj/src/reads_test.almd"),
        "import fs\ntest \"reads\" {\n  assert_eq(fs.read_text(\"s.txt\") ?? \"none\", \"none\")\n}\n",
    )
    .unwrap();
}

fn almide(dir: &Path, args: &[&str]) -> Output {
    let bin = std::env::var("ALMIDE_CONTROL_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string());
    Command::new(bin).args(args).current_dir(dir).output().expect("almide runs")
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

/// `(working directory, args)` for each route.
fn routes(root: &Path) -> Vec<(std::path::PathBuf, Vec<&'static str>)> {
    let proj = root.join("proj");
    let elsewhere = root.join("elsewhere");
    vec![
        // The entry file's project, from outside it.
        (elsewhere.clone(), vec!["check", "../proj/src/main.almd"]),
        (elsewhere.clone(), vec!["run", "../proj/src/main.almd"]),
        (elsewhere, vec!["run", "../proj/src/main.almd", "--target", "wasm"]),
        // `check` judges the module's function.
        (proj.clone(), vec!["check", "src/main.almd"]),
        // Both test lanes.
        (proj.clone(), vec!["test", "src/reads_test.almd"]),
        (proj, vec!["test", "src/reads_test.almd", "--target", "wasm"]),
    ]
}

#[test]
fn every_route_applies_the_entry_files_permissions() {
    let td = tempfile::TempDir::new().unwrap();
    project(td.path(), "\"IO\", \"Abort\"");
    for (dir, args) in routes(td.path()) {
        let o = almide(&dir, &args);
        let t = text(&o);
        assert!(!o.status.success(), "`almide {}` accepted a file read under allow = [\"IO\", \"Abort\"]:\n{t}", args.join(" "));
        assert!(t.contains("FS.read is not in [permissions].allow"), "`almide {}` did not name FS.read:\n{t}", args.join(" "));
        assert!(!t.contains(SECRET), "`almide {}` read the file:\n{t}", args.join(" "));
    }
}

/// The control: granting `FS` admits the same program on every route (the
/// test's assertion then fails on the file's contents, which is the read).
#[test]
fn granting_fs_admits_every_route() {
    let td = tempfile::TempDir::new().unwrap();
    project(td.path(), "\"FS\", \"IO\", \"Abort\"");
    for (dir, args) in routes(td.path()) {
        let o = almide(&dir, &args);
        let t = text(&o);
        assert!(!t.contains("[permissions].allow"), "`almide {}` refused a granted read:\n{t}", args.join(" "));
        if args[0] == "test" {
            assert!(t.contains(SECRET), "`almide {}` did not run the read:\n{t}", args.join(" "));
        } else {
            assert!(o.status.success(), "`almide {}`:\n{t}", args.join(" "));
        }
    }
}
