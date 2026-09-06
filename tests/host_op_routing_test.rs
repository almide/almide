//! #1921: host routing is decided from the EMITTED op set, not from a
//! module-level import scan. `import fs` used to deny the structural leg
//! unconditionally — on the run path too, where the embedded host serves
//! every fs op — and an fs program whose only fs use sat inside a
//! `!`-consumed `fan.map` built on neither leg. Now the structural leg
//! lowers the program; on the BUILD path its emitted host ops are audited
//! against the p1 shim's served set and an unserved op reroutes the module
//! to the incumbent's WASI rendering.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

const FS_PROGRAM: &str = r#"import fs
effect fn main() -> Unit = {
  let dir = fs.temp_dir()
  let p = "${dir}/almide_1921_routing_probe.txt"
  fs.write(p, "hello")!
  let t = fs.read_text(p)!
  println("read=${t} exists=${fs.exists(p)}")
}
"#;

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
    std::fs::write(&src, FS_PROGRAM).expect("write repro");
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

/// The build path: the p1 `to_wasi` shim carries no fs ops, so the
/// structural module is audited by its emitted op set and rerouted to the
/// incumbent's WASI rendering — by OP, naming the op, not by import name.
#[test]
fn fs_program_build_reroutes_by_emitted_op() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("fs_probe.almd");
    let wasm = dir.path().join("fs_probe.wasm");
    std::fs::write(&src, FS_PROGRAM).expect("write repro");
    let (ok, _, stderr) = run_debug(&[
        "build",
        src.to_str().unwrap(),
        "--target",
        "wasm",
        "-o",
        wasm.to_str().unwrap(),
    ]);
    assert!(ok, "build must succeed via the incumbent; stderr:\n{stderr}");
    assert!(
        stderr.contains("has no stock-WASI service"),
        "the reroute must be decided by the emitted host op (#1921); stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("incumbent renderer"),
        "the build must hand the fs module to the incumbent; stderr:\n{stderr}"
    );
    assert!(wasm.exists(), "the incumbent's WASI module must be written");
}
