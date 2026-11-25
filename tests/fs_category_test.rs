//! C-460: a stdlib call that reaches a filesystem entry carries an `FS` leaf
//! in `[permissions].allow` (ADR-0026 D2). `unix.connect` looks up its socket
//! file (`FS.read`), `unix.listen` creates one and `unix.shm_create` creates
//! its backing file (`FS.write`); they used to carry only `Net`, so a manifest
//! granting `Net` but no `FS` leaf let them touch files. `fs.temp_dir` touches
//! no entry and is `Env.read`. Each refusal is checked on every route, native
//! and wasm, with the granted form as the control.

use std::path::Path;
use std::process::{Command, Output};

fn project(dir: &Path, allow: &str, program: &str) {
    std::fs::write(
        dir.join("almide.toml"),
        format!("[package]\nname = \"fscat\"\nversion = \"0.1.0\"\n\n[permissions]\nallow = [{allow}]\n"),
    )
    .unwrap();
    std::fs::write(dir.join("main.almd"), program).unwrap();
}

/// `ALMIDE_CONTROL_BIN` points the assertions at another build (the negative
/// control: a compiler whose unix calls carry only `Net` passes them not).
fn almide(dir: &Path, args: &[&str]) -> Output {
    let bin = std::env::var("ALMIDE_CONTROL_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string());
    Command::new(bin).args(args).current_dir(dir).output().expect("almide runs")
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

const ROUTES: [&[&str]; 4] = [
    &["check", "main.almd"],
    &["build", "main.almd", "-o", "app"],
    &["run", "main.almd"],
    &["run", "main.almd", "--target", "wasm"],
];

const LISTENS: &str = "import unix\neffect fn main() -> Unit = {\n  let _ = unix.listen(\"s.sock\")!\n  println(\"touched-4825\")\n}\n";
const CONNECTS: &str = "import unix\neffect fn main() -> Unit = {\n  let _ = unix.connect(\"s.sock\")!\n  println(\"touched-4825\")\n}\n";
const SHM: &str = "import unix\neffect fn main() -> Unit = {\n  let _ = unix.shm_create(64)!\n  println(\"touched-4825\")\n}\n";
const TEMP_DIR: &str = "import fs\neffect fn main() -> Unit = println(\"tmp=${fs.temp_dir() != \"\"}\")\n";

#[test]
fn a_call_that_reaches_a_file_needs_its_fs_leaf_on_every_route() {
    for (program, leaf) in [(LISTENS, "FS.write"), (CONNECTS, "FS.read"), (SHM, "FS.write")] {
        let td = tempfile::TempDir::new().unwrap();
        project(td.path(), "\"Net\", \"IO\"", program);
        for args in ROUTES {
            let o = almide(td.path(), args);
            let t = text(&o);
            assert!(!o.status.success(), "`almide {}` passed under allow = [\"Net\", \"IO\"]:\n{program}\n{t}", args.join(" "));
            assert!(t.contains(&format!("{leaf} is not in [permissions].allow")), "`almide {}` ({leaf}):\n{program}\n{t}", args.join(" "));
            assert!(!t.contains("touched-4825"), "`almide {}` ran:\n{t}", args.join(" "));
        }
        assert!(!td.path().join("s.sock").exists(), "a socket file was made under a refused manifest");
    }
    // fs.temp_dir reads the environment: `Env.read`, not `FS.read`.
    let td = tempfile::TempDir::new().unwrap();
    project(td.path(), "\"FS\", \"IO\"", TEMP_DIR);
    for args in ROUTES {
        let o = almide(td.path(), args);
        let t = text(&o);
        assert!(!o.status.success() && t.contains("Env.read is not in [permissions].allow"), "`almide {}`:\n{t}", args.join(" "));
    }
}

/// The control: with the leaf granted the same programs pass the check, and
/// the ones that touch nothing outside the project run on both legs.
#[test]
fn the_granted_leaf_admits_the_same_calls_on_both_legs() {
    for program in [LISTENS, CONNECTS, SHM] {
        let td = tempfile::TempDir::new().unwrap();
        project(td.path(), "\"Net\", \"FS\", \"IO\"", program);
        let o = almide(td.path(), &["check", "main.almd"]);
        assert!(o.status.success(), "{program}\n{}", text(&o));
    }
    for (program, allow, want) in [(SHM, "\"FS.write\", \"IO\"", "touched-4825"), (TEMP_DIR, "\"Env.read\", \"IO\"", "tmp=true")] {
        let td = tempfile::TempDir::new().unwrap();
        project(td.path(), allow, program);
        for args in [&["run", "main.almd"][..], &["run", "main.almd", "--target", "wasm"]] {
            let o = almide(td.path(), args);
            assert!(o.status.success() && text(&o).contains(want), "`almide {}`:\n{program}\n{}", args.join(" "), text(&o));
        }
    }
}
