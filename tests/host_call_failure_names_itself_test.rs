//! A failed `process.exec` names the command it could not start (#2090).
//!
//! The spawn branch reported the `io::Error` and nothing else, so
//! `process.exec("ghh-not-a-binary")` and a missing FILE rendered byte-identically
//! as `No such file or directory (os error 2)` — a failure that said what went
//! wrong and never what it went wrong ON. The command is in scope at the failure
//! site and was discarded there.
//!
//! Note the neighbouring branches were already fine: a process that RAN and
//! failed reports `process '{cmd}' exited with status {status}`. Only "could not
//! start it" was anonymous.
//!
//! `process` has no wasm host binding (E081), so this leg is the only one — no
//! cross-target equality to maintain. The `fs` half of the issue is NOT here: its
//! message is built in generated WAT on the structural leg, which is its own
//! piece of work.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn run(dir: &std::path::Path, source: &str) -> String {
    let file = dir.join("spawn.almd");
    std::fs::write(&file, source).expect("write fixture");
    let out = Command::new(almide())
        .args(["run", file.to_str().unwrap()])
        .output()
        .expect("run almide");
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn a_failed_spawn_names_the_command() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = run(
        dir.path(),
        "import process\n\
         effect fn main() -> Unit = {\n\
        \x20 let _ = process.exec(\"ghh-not-a-binary\", [])!\n\
         }\n",
    );
    assert!(
        out.contains("process.exec(\"ghh-not-a-binary\")"),
        "the failure must name the command it could not start:\n{out}"
    );
    // The platform's own text stays a verbatim suffix — a reader who recognises
    // `(os error 2)` today keeps recognising it, and anything classifying on the
    // errno tail keeps working.
    assert!(
        out.contains("No such file or directory (os error 2)"),
        "the errno text must survive verbatim:\n{out}"
    );
}

/// The point of the issue: two unrelated failures must not render identically.
#[test]
fn a_failed_spawn_is_distinguishable_from_a_missing_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let spawn = run(
        dir.path(),
        "import process\n\
         effect fn main() -> Unit = {\n\
        \x20 let _ = process.exec(\"ghh-not-a-binary\", [])!\n\
         }\n",
    );
    let read = run(
        dir.path(),
        "import fs\n\
         effect fn main() -> Unit = {\n\
        \x20 let _ = fs.read_text(\"/nope/missing.txt\")!\n\
         }\n",
    );
    assert_ne!(
        spawn.trim(),
        read.trim(),
        "two unrelated failures still render identically"
    );
}
