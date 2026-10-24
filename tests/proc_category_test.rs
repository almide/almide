//! Starting a process is `Proc`, never implied by `Env` (#4155, ADR-0026 D2).
//!
//! `process` was classified `Env`, so a manifest granting `Env` to read
//! `$HOME` also ran any command — and a started command reaches files and the
//! network. The categories are now D2's leaves: `process.exec` is `Proc`, which
//! no parent grants; the environment readers stay `Env.read`; files are `FS`,
//! no longer part of `IO`. Each refusal is checked on the native and the wasm
//! leg, with a control that the granted form runs on both.

use std::path::Path;
use std::process::{Command, Output};

const SPAWNS: &str = "import env\nimport process\neffect fn main() -> Unit = {\n  \
    println(env.get(\"HOME\") ?? \"none\")\n  println(process.exec(\"echo\", [\"spawned-4155\"])!)\n}\n";
const READS_ENV: &str = "import env\neffect fn main() -> Unit = println(\"home=${env.get(\"HOME\") != none}\")\n";
const READS_FILE: &str = "import fs\neffect fn main() -> Unit = println(fs.read_text(\"s.txt\")!)\n";

fn project(dir: &Path, permissions: &str, program: &str) {
    std::fs::write(
        dir.join("almide.toml"),
        format!("[package]\nname = \"proccat\"\nversion = \"0.1.0\"\n\n[permissions]\n{permissions}\n"),
    )
    .unwrap();
    std::fs::write(dir.join("main.almd"), program).unwrap();
    std::fs::write(dir.join("s.txt"), "file-4155").unwrap();
}

/// `ALMIDE_CONTROL_BIN` points the assertions at another build (the
/// negative control: a compiler where `process` is `Env` passes them not).
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
    &["build", "main.almd", "--target", "wasm", "-o", "app.wasm"],
    &["run", "main.almd", "--target", "wasm"],
];

#[test]
fn env_does_not_grant_starting_a_process() {
    let td = tempfile::TempDir::new().unwrap();
    project(td.path(), "allow = [\"Env\", \"IO\"]", SPAWNS);
    for args in ROUTES {
        let o = almide(td.path(), args);
        let t = text(&o);
        assert!(!o.status.success(), "`almide {}` ran a command under allow = [\"Env\", \"IO\"]:\n{t}", args.join(" "));
        assert!(t.contains("Proc is not in [permissions].allow"), "`almide {}`:\n{t}", args.join(" "));
        assert!(t.contains("`Env` no longer grants it"), "`almide {}` gave no migration note:\n{t}", args.join(" "));
        assert!(!t.contains("spawned-4155"), "`almide {}` started the command:\n{t}", args.join(" "));
    }
}

/// The control: what `Env` is for still runs on both legs.
#[test]
fn env_still_reads_the_environment_on_both_legs() {
    let td = tempfile::TempDir::new().unwrap();
    project(td.path(), "allow = [\"Env\", \"IO\"]", READS_ENV);
    for args in [&["run", "main.almd"][..], &["run", "main.almd", "--target", "wasm"]] {
        let o = almide(td.path(), args);
        assert!(o.status.success() && text(&o).contains("home="), "`almide {}`:\n{}", args.join(" "), text(&o));
    }
}

/// The control: `Proc` with a command list starts the listed command on both
/// legs, with no warning.
#[test]
fn proc_with_a_command_list_runs_on_both_legs() {
    let td = tempfile::TempDir::new().unwrap();
    project(td.path(), "allow = [\"Env\", \"IO\", \"Proc\"]\nproc = [\"echo\"]", SPAWNS);
    for args in [&["run", "main.almd"][..], &["run", "main.almd", "--target", "wasm"]] {
        let o = almide(td.path(), args);
        let t = text(&o);
        assert!(o.status.success() && t.contains("spawned-4155"), "`almide {}`:\n{t}", args.join(" "));
        assert!(!t.contains("grants `Proc` with no"), "`almide {}` warned with a list present:\n{t}", args.join(" "));
    }
}

/// `Proc` without a `proc` list is a warning, and refused by `--profile
/// critical`.
#[test]
fn proc_without_a_command_list_warns_and_critical_refuses_it() {
    let td = tempfile::TempDir::new().unwrap();
    project(td.path(), "allow = [\"Env\", \"IO\", \"Proc\"]", SPAWNS);
    let o = almide(td.path(), &["check", "main.almd"]);
    assert!(o.status.success(), "{}", text(&o));
    assert!(text(&o).contains("warning: [permissions].allow grants `Proc` with no `proc = [...]` command list"), "{}", text(&o));
    let o = almide(td.path(), &["check", "main.almd", "--profile", "critical", "--allow", "IO", "--allow", "Env", "--allow", "Process"]);
    assert!(!o.status.success(), "critical admitted `Proc` without a list:\n{}", text(&o));
    assert!(text(&o).contains("error: [permissions].allow grants `Proc` with no `proc = [...]` command list"), "{}", text(&o));
}

/// Files are `FS`: a manifest that granted `IO` for a file read before epoch
/// 15 is refused with a note naming the new parent; `FS` admits it.
#[test]
fn files_are_fs_not_io() {
    let td = tempfile::TempDir::new().unwrap();
    project(td.path(), "allow = [\"IO\"]", READS_FILE);
    for args in ROUTES {
        let o = almide(td.path(), args);
        let t = text(&o);
        assert!(!o.status.success() && t.contains("FS.read is not in [permissions].allow"), "`almide {}`:\n{t}", args.join(" "));
        assert!(t.contains("files are `FS`"), "`almide {}`:\n{t}", args.join(" "));
    }
    project(td.path(), "allow = [\"FS.read\", \"IO\"]", READS_FILE);
    for args in [&["run", "main.almd"][..], &["run", "main.almd", "--target", "wasm"]] {
        let o = almide(td.path(), args);
        assert!(o.status.success() && text(&o).contains("file-4155"), "`almide {}`:\n{}", args.join(" "), text(&o));
    }
}
