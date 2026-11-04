//! The v1 native build's runtime crate profile (#4006, owner ruling
//! 2026-11-02).
//!
//! A program the v1 trust spine renders carries its runtime part — the shared
//! string buffer and the runtime bodies it takes along — in one module, which
//! the build compiles as a crate of its own: without debug assertions, as v0
//! compiles its runtime rlib, and with overflow checks pinned off. The
//! program's own code keeps `[profile.dev]` as it was, its debug assertions
//! included.
//!
//! What this test holds, on a real build's project directory:
//! - the program builds as two crates, and the manifest pins the runtime
//!   crate's two settings while `[profile.dev]` is unchanged;
//! - overflow wraps on the native leg, in the program's code and through the
//!   runtime (`int.abs` of the minimum), as before the split;
//! - a `debug_assert!` placed in the PROGRAM crate still fires (the negative
//!   control: the ruling must not switch off the program's checks);
//! - the same `debug_assert!` placed in the RUNTIME crate does not.

use std::path::{Path, PathBuf};
use std::process::Command;

const PROGRAM: &str = r#"fn main() -> Unit = {
  let big = 9223372036854775807
  println(int.to_string(big + 1))
  println(int.to_string(int.abs(-9223372036854775807 - 1)))
}
"#;

const WRAPPED: &str = "-9223372036854775808\n-9223372036854775808\n";

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("almide-rt-profile-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("scratch dir");
    d
}

/// The project directory the build wrote under `tmp`.
fn run_dir(tmp: &Path) -> PathBuf {
    std::fs::read_dir(tmp)
        .expect("tmp")
        .filter_map(|e| e.ok().map(|e| e.path().join("run")))
        .find(|p| p.join("Cargo.toml").is_file())
        .unwrap_or_else(|| panic!("no generated project under {}", tmp.display()))
}

fn cargo_build_and_run(dir: &Path) -> std::process::Output {
    let build = Command::new("cargo").arg("build").arg("--quiet").current_dir(dir).output().expect("spawn cargo");
    assert!(build.status.success(), "cargo build failed:\n{}", String::from_utf8_lossy(&build.stderr));
    Command::new(dir.join("target/debug/almide-out")).output().expect("run the rebuilt binary")
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("mkdir");
    for e in std::fs::read_dir(from).expect("read_dir") {
        let e = e.expect("entry");
        let p = e.path();
        if p.file_name().is_some_and(|n| n == "target") {
            continue;
        }
        if p.is_dir() {
            copy_tree(&p, &to.join(e.file_name()));
        } else {
            std::fs::copy(&p, to.join(e.file_name())).expect("copy");
        }
    }
}

/// Insert `stmt` at the start of the first fn whose header line contains `header`.
fn plant(file: &Path, header: &str, stmt: &str) {
    let src = std::fs::read_to_string(file).expect("read");
    let at = src.find(header).unwrap_or_else(|| panic!("`{header}` not in {}", file.display()));
    let open = at + src[at..].find('{').expect("fn body") + 1;
    let planted = format!("{}\n{stmt}\n{}", &src[..open], &src[open..]);
    std::fs::write(file, planted).expect("write");
}

#[test]
fn the_runtime_crate_builds_without_debug_assertions_and_the_program_keeps_them() {
    let work = scratch("build");
    let tmp = work.join("tmp");
    std::fs::create_dir_all(&tmp).expect("tmp");
    let src = work.join("p.almd");
    std::fs::write(&src, PROGRAM).expect("write program");
    let bin = work.join("p");
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["build", src.to_str().unwrap(), "-o", bin.to_str().unwrap()])
        .env("TMPDIR", &tmp)
        .env("ALMIDE_VERIFIED_DEBUG", "1")
        .output()
        .expect("spawn almide");
    let log = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "almide build failed:\n{log}");
    assert!(log.contains("native: v1 trust-spine render"), "the probe must render on v1:\n{log}");
    let ran = Command::new(&bin).output().expect("run");
    assert_eq!(String::from_utf8_lossy(&ran.stdout), WRAPPED, "overflow must wrap on the native leg");

    // The project: two crates, the runtime's profile pinned, the program's unchanged.
    let run = run_dir(&tmp);
    let manifest = std::fs::read_to_string(run.join("Cargo.toml")).expect("manifest");
    assert!(
        manifest.contains("[profile.dev.package.almide_v1_rt]\ndebug-assertions = false\noverflow-checks = false"),
        "the runtime crate's profile is not pinned:\n{manifest}"
    );
    assert!(manifest.contains("[profile.dev]\nopt-level = 1\noverflow-checks = false\n\n"), "[profile.dev] changed:\n{manifest}");
    assert!(run.join("almide_v1_rt/src/lib.rs").is_file(), "no runtime crate was split out");

    // Negative control: a debug_assert! in the PROGRAM crate fires.
    let user = work.join("user");
    copy_tree(&run, &user);
    plant(&user.join("src/main.rs"), "fn main()", "debug_assert!(false, \"program-check\");");
    let o = cargo_build_and_run(&user);
    assert!(!o.status.success(), "a debug_assert! in the program crate did not fire");
    assert!(String::from_utf8_lossy(&o.stderr).contains("program-check"), "{}", String::from_utf8_lossy(&o.stderr));

    // The same assertion in the RUNTIME crate (the integer printer every line
    // goes through) does not.
    let rt = work.join("rt");
    copy_tree(&run, &rt);
    plant(&rt.join("almide_v1_rt/src/lib.rs"), "pub fn from_i64(", "debug_assert!(false, \"runtime-check\");");
    let o = cargo_build_and_run(&rt);
    assert!(o.status.success(), "a debug_assert! in the runtime crate fired:\n{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(String::from_utf8_lossy(&o.stdout), WRAPPED);
    let _ = std::fs::remove_dir_all(&work);
}
