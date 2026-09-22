//! The `almide-verify` binary's contract: the `proofs/checker` interface for
//! one witness (stdout ACCEPT / REJECT, exit 0 / 1, 2 on misuse), the bundle
//! mode's outcome codes, and a version line that names the verifier's own
//! version rather than the compiler's.

use std::process::{Command, Output};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_almide-verify"))
}

fn scratch(name: &str, bytes: &[u8]) -> String {
    let dir = std::env::temp_dir().join(format!("almide-verify-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let path = dir.join(name);
    std::fs::write(&path, bytes).expect("scratch file");
    path.display().to_string()
}

fn run(args: &[&str]) -> Output {
    bin().args(args).output().expect("almide-verify runs")
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

#[test]
fn one_witness_speaks_the_extracted_checker_interface() {
    let good = scratch("good.cert", b"i{dx|}d\n");
    let bad = scratch("bad.cert", b"i{x|}d\n");
    let o = run(&["ownership", &good]);
    assert_eq!((o.status.code(), stdout(&o).trim()), (Some(0), "ACCEPT"));
    let o = run(&["ownership", &bad]);
    assert_eq!((o.status.code(), stdout(&o).trim()), (Some(1), "REJECT"));
}

#[test]
fn misuse_exits_two() {
    let good = scratch("misuse.cert", b"id\n");
    assert_eq!(run(&["teleport", &good]).status.code(), Some(2));
    assert_eq!(run(&["ownership", "/nonexistent/almide-verify.cert"]).status.code(), Some(2));
    assert_eq!(run(&[]).status.code(), Some(2));
}

#[test]
fn the_version_is_the_verifiers_own() {
    let o = run(&["--version"]);
    assert_eq!(o.status.code(), Some(0));
    assert!(stdout(&o).starts_with(&format!("almide-verify {} ", env!("CARGO_PKG_VERSION"))), "{}", stdout(&o));
}

#[test]
fn bundle_outcomes_have_distinct_exit_codes() {
    let header = "almide-certificate-bundle 1\nproducer test\n";
    let certified = scratch("certified.bundle", format!("{header}witness ownership 2 main\nid\nwitness names 3 main\n1|1\n").as_bytes());
    let rejected = scratch("rejected.bundle", format!("{header}witness ownership 3 main\nidd\n").as_bytes());
    let incomplete = scratch("incomplete.bundle", format!("{header}witness ownership 2 main\nid\nuncertified f walled\n").as_bytes());
    let empty = scratch("empty.bundle", header.as_bytes());
    let malformed = scratch("malformed.bundle", b"almide-certificate-bundle 1\nwitness ownership 99 main\nid\n");

    let o = run(&["bundle", &certified]);
    assert_eq!(o.status.code(), Some(0), "{}", stdout(&o));
    assert!(stdout(&o).contains("-> CERTIFIED"));
    assert!(stdout(&o).contains("producer: test (producer claim, not checked)"));
    assert_eq!(run(&["bundle", &rejected]).status.code(), Some(1));
    assert_eq!(run(&["bundle", &incomplete]).status.code(), Some(3));
    assert_eq!(run(&["bundle", &empty]).status.code(), Some(1));
    assert_eq!(run(&["bundle", &malformed]).status.code(), Some(2));
}
