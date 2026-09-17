//! The per-pass IR verifier runs in the profile that ships: the RELEASE
//! binary (`ALMIDE_BIN`, `target/release/almide`) refuses a program whose IR
//! is broken after a named pass, naming that pass. Until now the walk was
//! debug / `ALMIDE_VERIFY_IR` only, so nothing checked the release
//! pipeline's intermediate IR at all. `ALMIDE_IR_FAULT=<pass>` injects the
//! `verify_binder_ownership` violation (one function's locals bound in a
//! second function) right after the pass, so the gate can be watched
//! turning red from outside — a negative control, never a user feature.
use std::process::Command;

const PROGRAM: &str = "fn twice(n: Int) -> Int = n * 2\nfn main() -> Unit = println(int.to_string(twice(21)))\n";

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| format!("{}/target/release/almide", env!("CARGO_MANIFEST_DIR")))
}

fn emit(dir: &std::path::Path, fault: Option<&str>) -> (bool, String) {
    std::fs::write(dir.join("main.almd"), PROGRAM).unwrap();
    let mut cmd = Command::new(almide());
    cmd.current_dir(dir).args(["main.almd", "--target", "rust"]);
    if let Some(pass) = fault {
        cmd.env("ALMIDE_IR_FAULT", pass);
    }
    let out = cmd.output().unwrap();
    (out.status.success(), String::from_utf8_lossy(&out.stderr).to_string())
}

#[test]
fn the_release_binary_refuses_a_broken_ir_after_the_named_pass() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, stderr) = emit(dir.path(), None);
    assert!(ok, "the program emits without the fault:\n{stderr}");
    for pass in ["CloneInsertion", "BorrowInsertion"] {
        let (ok, stderr) = emit(dir.path(), Some(pass));
        assert!(!ok, "the injected fault after {pass} must fail the build:\n{stderr}");
        assert!(stderr.contains(&format!("IR verification failed after pass '{pass}'")), "the failure names the pass:\n{stderr}");
        assert!(stderr.contains("is bound in two functions"), "the verifier names the violation:\n{stderr}");
    }
}
