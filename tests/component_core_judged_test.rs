//! #4877: a program `almide build --target wasm` packages as a component has
//! its core module judged, on the build route and on the wasm run route, as
//! every other wasm build is (#4836). The two halves:
//!
//! - judged in process: the judge is the `almide-verify` library linked into
//!   the compiler (#4836, owner ruling 2026-11-24), never whatever binary of
//!   that name sits beside it — a stub there that rejects everything (and
//!   logs that it ran) is not run, and the build and run go on. Before the
//!   ruling a build with no verifier installed shipped unjudged; the refusal
//!   half is `scripts/check-build-certifies.sh` (re-injected emitter bugs).
//! - accepted: the program that held the judging back — a plain fn comparing
//!   `http.HttpCall` handles, whose dropped copies run the handle's release
//!   (host op 59) — certifies. Its caps witness charged that release with the
//!   network capability, so `caps p` was REJECTED.
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
fn a_component_is_judged_by_the_linked_verifier_not_a_binary_beside_the_compiler() {
    let td = tempfile::tempdir().expect("tempdir");
    let bin = td.path().join("bin");
    std::fs::create_dir_all(&bin).expect("mkdir bin");
    // The old gate looked for `almide-verify` beside its own executable.
    std::fs::copy(almide(), bin.join("almide")).expect("copy almide");
    let fake = bin.join("almide-verify");
    let log = td.path().join("stub-ran");
    std::fs::write(&fake, format!("#!/bin/sh\ntouch '{}'\necho 'REJECT  caps  (every function)'\nexit 1\n", log.display())).expect("write fake verifier");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).expect("chmod fake verifier");
    write_program(td.path());
    let build = Command::new(bin.join("almide"))
        .args(["build", "p.almd", "--target", "wasm", "-o", "p.wasm"])
        .current_dir(td.path())
        .output()
        .expect("spawn almide build");
    assert!(build.status.success(), "the component build did not certify in process:\n{}", out(&build));
    assert!(td.path().join("p.wasm").exists(), "the certified build wrote no output");
    let run = Command::new(bin.join("almide"))
        .args(["run", "p.almd", "--target", "wasm"])
        .current_dir(td.path())
        .output()
        .expect("spawn almide run");
    assert!(run.status.success(), "the component run did not certify in process:\n{}", out(&run));
    assert!(!log.exists(), "the build gate ran the almide-verify binary beside the compiler instead of the linked verifier");
}

#[test]
fn a_plain_fn_that_drops_http_call_handles_certifies_as_a_component() {
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
