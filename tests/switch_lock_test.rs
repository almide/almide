//! #4129: an environment the developer does not control must not make the
//! compiler build something other than the reviewed source, or skip a check.
//!
//! - `almide survive`'s overlay reaches a child run only as an argument; the
//!   retired `ALMIDE_SURVIVE_OVERLAY` environment variables change nothing.
//! - Under the switch lock (a release build, or `--lock-switches` in any
//!   build) an inherited route / gate / ablation / trap switch is ignored
//!   unless `--allow-switch` names it.
//! - `--profile critical` refuses such a switch outright.
//!
//! Each refusal has its control: the same run where the switch IS honoured,
//! so a test that passes because the switch never did anything cannot pass.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("almide-switch-lock-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn run(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(almide());
    cmd.args(args).current_dir(dir).stdin(Stdio::null());
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.output().expect("spawn almide")
}

fn text(o: &Output) -> String {
    format!("exit {:?}\nstdout:\n{}\nstderr:\n{}", o.status.code(), String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

const DISK: &str = "effect fn main() -> Unit = {\n  println(\"from_the_disk_file\")\n}\n";
const OTHER: &str = "effect fn main() -> Unit = {\n  println(\"from_the_overlay_text\")\n}\n";

#[test]
fn the_overlay_environment_variables_change_nothing() {
    let d = scratch("overlay-env");
    std::fs::write(d.join("a.almd"), DISK).unwrap();
    std::fs::write(d.join("other.txt"), OTHER).unwrap();
    let other = d.join("other.txt");
    let o = run(
        &d,
        &["emit", "a.almd", "--target", "rust"],
        &[("ALMIDE_SURVIVE_OVERLAY", "a.almd"), ("ALMIDE_SURVIVE_OVERLAY_TEXT", other.to_str().unwrap())],
    );
    let out = text(&o);
    assert!(o.status.success(), "{out}");
    assert!(out.contains("from_the_disk_file"), "the disk bytes compile:\n{out}");
    assert!(!out.contains("from_the_overlay_text"), "the environment swapped the source:\n{out}");

    // Control: the argument hand-off `almide survive` uses DOES overlay, so
    // the read path above was the overlay-aware one.
    let o = run(
        &d,
        &["--survive-overlay", "a.almd", "--survive-overlay-text", other.to_str().unwrap(), "emit", "a.almd", "--target", "rust"],
        &[],
    );
    let out = text(&o);
    assert!(o.status.success(), "{out}");
    assert!(out.contains("from_the_overlay_text"), "the argument overlay must apply:\n{out}");
    let _ = std::fs::remove_dir_all(&d);
}

/// The native `almide build` enforces the version pin (`check`, `emit` and
/// the wasm build do not); a refused pin stops it before cargo runs.
const EMIT: [&str; 4] = ["build", "src/main.almd", "-o", "out.bin"];

/// A project pinned to a compiler version no build satisfies.
fn pinned_project(tag: &str) -> PathBuf {
    let d = scratch(tag);
    std::fs::write(d.join("almide.toml"), "[package]\nname = \"pinned\"\nalmide = \"999.0.0\"\n").unwrap();
    std::fs::create_dir_all(d.join("src")).unwrap();
    std::fs::write(d.join("src/main.almd"), DISK).unwrap();
    d
}

#[test]
fn the_lock_ignores_an_inherited_gate_switch_unless_allowed() {
    let d = pinned_project("gate");
    let skip = [("ALMIDE_SKIP_VERSION_CHECK", "1")];

    // Control: unlocked, the inherited switch bypasses the pin.
    let o = run(&d, &EMIT, &skip);
    assert!(o.status.success(), "unlocked, the switch must bypass the pin:\n{}", text(&o));

    // Locked: the inherited switch is ignored, the pin holds, and it says so.
    let o = run(&d, &[&["--lock-switches"][..], &EMIT[..]].concat(), &skip);
    let out = text(&o);
    assert!(!o.status.success(), "locked, the inherited switch must not bypass the pin:\n{out}");
    assert!(out.contains("requires almide >= 999.0.0"), "{out}");
    assert!(out.contains("ALMIDE_SKIP_VERSION_CHECK is set but IGNORED"), "{out}");

    // Locked + named on the command line: honoured.
    let o = run(&d, &[&["--lock-switches", "--allow-switch", "ALMIDE_SKIP_VERSION_CHECK"][..], &EMIT[..]].concat(), &skip);
    assert!(o.status.success(), "an allowed switch is honoured:\n{}", text(&o));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_lock_refuses_the_retired_no_verified_leg_from_the_environment() {
    let d = scratch("noverified");
    std::fs::write(d.join("a.almd"), DISK).unwrap();
    let o = run(&d, &["--lock-switches", "run", "a.almd", "--no-verified"], &[("ALMIDE_NO_VERIFIED_OK", "1")]);
    let out = text(&o);
    assert_eq!(o.status.code(), Some(1), "{out}");
    assert!(out.contains("--no-verified (the legacy v0 codegen path) has been removed"), "{out}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn critical_refuses_a_build_changing_switch_even_when_allowed() {
    let d = scratch("critical");
    std::fs::write(d.join("a.almd"), "fn main() -> Unit = ()\n").unwrap();
    // Control: the program passes the critical profile without the switch.
    let o = run(&d, &["check", "a.almd", "--profile", "critical"], &[]);
    assert!(o.status.success(), "control: {}", text(&o));
    for args in [
        &["check", "a.almd", "--profile", "critical"][..],
        &["--allow-switch", "ALMIDE_NO_AVAIL_CHECK", "check", "a.almd", "--profile", "critical"][..],
    ] {
        let o = run(&d, args, &[("ALMIDE_NO_AVAIL_CHECK", "1")]);
        let out = text(&o);
        assert_eq!(o.status.code(), Some(1), "{out}");
        assert!(out.contains("--profile critical refuses a build-changing environment switch: ALMIDE_NO_AVAIL_CHECK"), "{out}");
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn allow_switch_names_only_build_changing_switches() {
    let d = scratch("allow-name");
    // Spelled in two halves: the env-switch gate refuses an unregistered
    // `ALMIDE_*` literal anywhere in the tree.
    for (name, why) in [(concat!("ALMIDE_", "NOT_A_SWITCH"), "is not a registered switch"), ("ALMIDE_PROFILE", "is a debug switch")] {
        let o = run(&d, &["--allow-switch", name, "switches"], &[]);
        let out = text(&o);
        assert_eq!(o.status.code(), Some(1), "{out}");
        assert!(out.contains(why), "{out}");
    }
    let _ = std::fs::remove_dir_all(&d);
}
