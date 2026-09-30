//! #1921: host routing is decided from the EMITTED op set, not from a
//! module-level import scan. `import fs` used to deny the structural leg
//! unconditionally — on the run path too, where the embedded host serves
//! every fs op — and an fs program whose only fs use sat inside a
//! `!`-consumed `fan.map` built on neither leg. Now the structural leg
//! lowers the program; on the BUILD path its emitted host ops are audited
//! against the p1 shim's served set and an unserved op is a wall (E082; it
//! rerouted to the incumbent's WASI rendering until #2752). Since #2742 the p1 shim serves the fs
//! ops itself, so an fs program BUILDS on the structural leg too.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

/// The probe program, writing its own file: the run and build tests run in
/// parallel, and with one shared path each could truncate the file between
/// the other's write and read (`read= exists=true` in a merge-queue run).
fn fs_program(tag: &str) -> String {
    format!(
        r#"import fs
effect fn main() -> Unit = {{
  let dir = fs.temp_dir()
  let p = "${{dir}}/almide_1921_routing_probe_{tag}_{pid}.txt"
  fs.write(p, "hello")!
  let t = fs.read_text(p)!
  println("read=${{t}} exists=${{fs.exists(p)}}")
}}
"#,
        pid = std::process::id()
    )
}

fn run_debug(args: &[&str]) -> (bool, String, String) {
    let out = Command::new(almide())
        .args(args)
        .env("ALMIDE_VERIFIED_DEBUG", "1")
        .output()
        .expect("run almide");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// The run path: the embedded host serves the fs ops, so the structural
/// leg keeps the module it emitted — no import-name denial.
#[test]
fn fs_program_runs_on_the_structural_leg() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("fs_probe.almd");
    std::fs::write(&src, fs_program("run")).expect("write repro");
    let (ok, stdout, stderr) = run_debug(&["run", src.to_str().unwrap(), "--target", "wasm"]);
    assert!(ok, "run must succeed; stderr:\n{stderr}");
    assert!(
        stderr.contains("structural leg emitted the module"),
        "the fs program must run on the structural leg (#1921); stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("incumbent renderer"),
        "the run path must not reroute an fs program by import name; stderr:\n{stderr}"
    );
    assert_eq!(stdout.trim(), "read=hello exists=true");
}

/// The build path: the p1 `to_wasi` shim serves the fs ops (#2742), so the
/// audit passes and the structural module ships — no reroute — and the stock
/// artifact answers what the embedded host answers.
#[test]
fn fs_program_builds_on_the_structural_leg() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("fs_probe.almd");
    let wasm = dir.path().join("fs_probe.wasm");
    std::fs::write(&src, fs_program("build")).expect("write repro");
    let (ok, _, stderr) = run_debug(&[
        "build",
        src.to_str().unwrap(),
        "--target",
        "wasm",
        "-o",
        wasm.to_str().unwrap(),
    ]);
    assert!(ok, "build must succeed; stderr:\n{stderr}");
    assert!(
        stderr.contains("structural leg emitted the module"),
        "the fs program must build on the structural leg (#2742); stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("has no stock-WASI service") && !stderr.contains("incumbent renderer"),
        "no fs op may reroute the build to the incumbent (#2742); stderr:\n{stderr}"
    );
    let Ok(out) = Command::new("wasmtime")
        .args(["run", "--dir=/", "-S", "inherit-env=y", wasm.to_str().unwrap()])
        .output()
    else {
        return; // no wasmtime here: the route assertion above is the test
    };
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "read=hello exists=true");
}
