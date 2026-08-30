//! #1490 item 2: the REPL answers through the embedded wasm host first —
//! rustc-free for everything the structural leg lowers — falling back to
//! the rustc path only on a leg refusal. Both paths print the language's
//! own `${expr}` rendering, so the answer is path-independent (the old
//! rustc-only Debug patch made `"hi"` vs `hi` depend on the path).
//!
//! These tests drive the real binary with a piped session. They assert
//! ANSWERS, not which path produced them — the path split is an
//! implementation detail the consistency rule exists to hide. Speed is
//! the observable: a scalar session must answer well inside the rustc
//! floor (a cargo build is tens of seconds cold; the wasm path is
//! milliseconds), which is what the generous-but-real deadline pins.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

fn repl(session: &str) -> (String, Duration) {
    let started = Instant::now();
    // The REPL is the bare binary — no subcommand.
    let mut child = Command::new(almide_bin())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn almide repl");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(session.as_bytes())
        .expect("write session");
    let out = child.wait_with_output().expect("wait repl");
    (String::from_utf8_lossy(&out.stdout).to_string(), started.elapsed())
}

#[test]
fn scalar_arithmetic_answers_fast() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let (out, took) = repl("1 + 2\n:q\n");
    assert!(out.contains("3"), "missing the answer:\n{out}");
    // The rustc path's COLD floor is a cargo build (tens of seconds);
    // the embedded-wasm path answers in well under this. Generous so a
    // loaded CI runner never flakes it, real enough that a rustc-only
    // regression cannot hide.
    assert!(
        took < Duration::from_secs(20),
        "a scalar REPL line took {took:?} — the rustc-free fast path is gone"
    );
}

#[test]
fn session_state_and_language_rendering() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let (out, _) = repl("let name = \"world\"\n\"Hello, \" + name\n[1, 2, 3]\n:q\n");
    // The language's own rendering on every path: strings BARE (the old
    // Debug patch printed `"Hello, world"`), lists in bracket form.
    assert!(out.contains("Hello, world"), "string answer missing/requoted:\n{out}");
    assert!(!out.contains("\"Hello, world\""), "Debug-quoted string leaked back:\n{out}");
    assert!(out.contains("[1, 2, 3]"), "list rendering missing:\n{out}");
}
