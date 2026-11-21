//! #4877: a program `almide build --target wasm` packages as a component has
//! its core module judged by `almide-verify`, on the build route and on the
//! wasm run route, as every other wasm build is (#4836). The two halves:
//!
//! - judged: with a verifier beside the compiler that rejects everything, the
//!   build and the run of a component program are refused. Before the fix a
//!   component build dropped its certificate unjudged and both succeeded.
//! - accepted: with the real verifier, the program that held the judging back
//!   — a plain fn comparing `http.HttpCall` handles, whose dropped copies run
//!   the handle's release (host op 59) — certifies. Its caps witness charged
//!   that release with the network capability, so `caps p` was REJECTED.
#![cfg(unix)]
use std::path::{Path, PathBuf};
use std::process::Command;

const PROGRAM: &str = "import http\nfn p(xs: List[http.HttpCall]) -> Bool = list.any(list.zip(xs, xs), ((a, b)) => a == b)\nfn main() -> Unit = println(\"${p([])}\")\n";

fn almide() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_almide"))
}

fn out(o: &std::process::Output) -> String {
    format!("exit={:?}\nstdout:\n{}\nstderr:\n{}", o.status.code(), String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

fn write_program(dir: &Path) {
    std::fs::write(dir.join("p.almd"), PROGRAM).expect("write program");
}

#[test]
fn a_component_build_and_run_are_refused_by_a_verifier_that_rejects() {
    let td = tempfile::tempdir().expect("tempdir");
    let bin = td.path().join("bin");
    std::fs::create_dir_all(&bin).expect("mkdir bin");
    // The compiler finds `almide-verify` beside its own executable.
    std::fs::copy(almide(), bin.join("almide")).expect("copy almide");
    let fake = bin.join("almide-verify");
    std::fs::write(&fake, "#!/bin/sh\necho 'REJECT  caps  (every function)'\nexit 1\n").expect("write fake verifier");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).expect("chmod fake verifier");
    write_program(td.path());
    let build = Command::new(bin.join("almide"))
        .args(["build", "p.almd", "--target", "wasm", "-o", "p.wasm"])
        .current_dir(td.path())
        .output()
        .expect("spawn almide build");
    assert!(!build.status.success(), "a rejecting verifier did not refuse the component build:\n{}", out(&build));
    assert!(out(&build).contains("certificate-rejected"), "{}", out(&build));
    assert!(!td.path().join("p.wasm").exists(), "the refused build still wrote its output");
    let run = Command::new(bin.join("almide"))
        .args(["run", "p.almd", "--target", "wasm"])
        .current_dir(td.path())
        .output()
        .expect("spawn almide run");
    assert!(!run.status.success(), "a rejecting verifier did not refuse the component run:\n{}", out(&run));
    assert!(out(&run).contains("certificate-rejected"), "{}", out(&run));
}

#[test]
fn a_plain_fn_that_drops_http_call_handles_certifies_as_a_component() {
    let verifier = almide().with_file_name("almide-verify");
    assert!(verifier.is_file(), "no almide-verify beside {} — build the workspace (cargo build --release)", almide().display());
    let td = tempfile::tempdir().expect("tempdir");
    write_program(td.path());
    let build = Command::new(almide())
        .args(["build", "p.almd", "--target", "wasm", "-o", "p.wasm"])
        .current_dir(td.path())
        .output()
        .expect("spawn almide build");
    assert!(build.status.success(), "the component core module did not certify:\n{}", out(&build));
    let run = Command::new(almide())
        .args(["run", "p.almd", "--target", "wasm"])
        .current_dir(td.path())
        .output()
        .expect("spawn almide run");
    assert!(run.status.success(), "{}", out(&run));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "false\n", "{}", out(&run));
}
