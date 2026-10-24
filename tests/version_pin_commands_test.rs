//! #4172: `almide.toml`'s `almide = "X.Y.Z"` minimum holds on every command
//! that checks or compiles the package. It used to be read only by the native
//! build's compile driver, so `check`, `emit`, `build --target wasm` and
//! `run --target wasm` compiled a package with an older compiler without a
//! word.
//!
//! Control: the same commands on the same project with a pin this compiler
//! satisfies, so a refusal is the pin's and not the command's.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn project(tag: &str, pin: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("almide-pin-commands-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("src")).unwrap();
    std::fs::write(d.join("almide.toml"), format!("[package]\nname = \"pinned\"\nalmide = \"{pin}\"\n")).unwrap();
    std::fs::write(
        d.join("src/main.almd"),
        "effect fn main() -> Unit = {\n  println(\"ran\")\n}\n\ntest \"t\" {\n  assert_eq(1, 1)\n}\n",
    )
    .unwrap();
    d
}

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(almide())
        .args(args)
        .current_dir(dir)
        .env_remove("ALMIDE_SKIP_VERSION_CHECK")
        .stdin(Stdio::null())
        .output()
        .expect("spawn almide")
}

fn text(o: &Output) -> String {
    format!("exit {:?}\nstdout:\n{}\nstderr:\n{}", o.status.code(), String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

/// The commands that compile or check without a cargo build.
const COMMANDS: &[&[&str]] = &[
    &["check", "src/main.almd"],
    &["emit", "src/main.almd", "--target", "rust"],
    &["build", "src/main.almd", "--target", "wasm", "-o", "out.wasm"],
    &["run", "src/main.almd", "--target", "wasm"],
    &["test", "src/main.almd", "--target", "wasm"],
    &["compile"],
];

#[test]
fn every_compiling_command_refuses_an_older_compiler() {
    let d = project("refused", "999.0.0");
    for args in COMMANDS {
        let o = run(&d, args);
        let out = text(&o);
        assert_eq!(o.status.code(), Some(1), "`almide {}` must refuse the pin:\n{out}", args.join(" "));
        assert!(out.contains("requires almide >= 999.0.0"), "`almide {}`:\n{out}", args.join(" "));
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_satisfied_pin_lets_the_same_commands_run() {
    for (tag, pin) in [("plain", "0.0.1"), ("ge", ">=0.0.1")] {
        let d = project(tag, pin);
        for args in [&["check", "src/main.almd"][..], &["build", "src/main.almd", "--target", "wasm", "-o", "out.wasm"][..]] {
            let o = run(&d, args);
            assert!(o.status.success(), "pin {pin:?}, `almide {}`:\n{}", args.join(" "), text(&o));
        }
        let o = run(&d, &["run", "src/main.almd", "--target", "wasm"]);
        assert!(String::from_utf8_lossy(&o.stdout).contains("ran"), "pin {pin:?}:\n{}", text(&o));
        let _ = std::fs::remove_dir_all(&d);
    }
}

#[test]
fn a_pin_with_another_operator_is_invalid() {
    let d = project("op", "<1.0.0");
    let o = run(&d, &["check", "src/main.almd"]);
    let out = text(&o);
    assert_eq!(o.status.code(), Some(1), "{out}");
    assert!(out.contains("invalid `almide` version pin"), "{out}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn package_management_still_runs_under_an_unsatisfied_pin() {
    // So the pin can be read and changed with the old compiler.
    let d = project("deps", "999.0.0");
    let o = run(&d, &["deps"]);
    assert!(o.status.success(), "{}", text(&o));
    let o = run(&d, &["fmt", "--check", "src/main.almd"]);
    assert!(!text(&o).contains("requires almide"), "fmt must not be refused by the pin:\n{}", text(&o));
    let _ = std::fs::remove_dir_all(&d);
}
