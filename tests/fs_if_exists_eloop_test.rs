//! #4154 (C-215): a symlink loop is not absence. Every `_if_exists` content
//! reader answers `ok(none)` only for a path that does not exist; a loop
//! (ELOOP) — or a file where a parent directory should be (ENOTDIR) — is the
//! plain reader's `err`, on every leg. The wasm text twin used to ask an
//! `exists` probe first, which a loop answers `false`, so it read the loop as
//! absent (`ok(none)`) while native and its byte and line twins erred: a
//! planted loop made a config file read as "not there", failing open.
//!
//! The stdlib cannot make a symlink, so the tree is built here. Native and the
//! embedded wasm host run in this process's OS and print byte-identical
//! messages; the stock artifact (`almide build --target wasm`, the p1 fs
//! service under wasmtime) spells an errno outside the shared table its own
//! way, so for it the test asserts the err and its call head, never absence.
//! Negative control: the old embedded-host arm (`exists` first) prints
//! `ok(none)` for the loop lines and fails the native comparison.
#![cfg(unix)]

use std::path::Path;
use std::process::Command;

const PROBE: &str = r#"import fs

fn show(label: String, r: Result[String, String]) -> Unit = match r {
  ok(v) => println("${label}: ok ${v}"),
  err(e) => println("${label}: err ${e}"),
}

effect fn main() -> Unit = {
  show("text loop", fs.read_text_if_exists("lp/a") |> result.map((o) => "${o}"))
  show("bytes loop", fs.read_bytes_if_exists("lp/a") |> result.map((o) => "${o}"))
  show("lines loop", fs.read_lines_if_exists("lp/a") |> result.map((o) => "${o}"))
  show("raw loop", fs.read_bytes_raw_if_exists("lp/a") |> result.map((o) => match o { some(_) => "some", none => "none" }))
  show("text file parent", fs.read_text_if_exists("f.txt/x") |> result.map((o) => "${o}"))
  show("text missing", fs.read_text_if_exists("nope/x") |> result.map((o) => "${o}"))
  show("text present", fs.read_text_if_exists("f.txt") |> result.map((o) => "${o}"))
}
"#;

fn almide_bin() -> String {
    env!("CARGO_BIN_EXE_almide").to_string()
}

fn run(dir: &Path, args: &[&str]) -> String {
    let o = Command::new(almide_bin()).arg("run").arg("probe.almd").args(args).current_dir(dir).output().expect("spawn almide");
    assert!(o.status.success(), "{args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).expect("utf8")
}

#[test]
fn a_symlink_loop_is_the_readers_err_not_absence_on_every_leg() {
    let t = tempfile::tempdir().expect("scratch");
    let dir = std::fs::canonicalize(t.path()).expect("canonical");
    std::fs::create_dir(dir.join("lp")).unwrap();
    std::os::unix::fs::symlink("b", dir.join("lp/a")).unwrap();
    std::os::unix::fs::symlink("a", dir.join("lp/b")).unwrap();
    std::fs::write(dir.join("f.txt"), "hi").unwrap();
    std::fs::write(dir.join("probe.almd"), PROBE).unwrap();

    let native = run(&dir, &[]);
    for (label, call) in [
        ("text loop", "fs.read_text_if_exists"),
        ("bytes loop", "fs.read_bytes_if_exists"),
        ("lines loop", "fs.read_lines_if_exists"),
        ("raw loop", "fs.read_bytes_raw_if_exists"),
    ] {
        let head = format!("{label}: err {call}(\"lp/a\"): ");
        assert!(native.lines().any(|l| l.starts_with(&head)), "native: missing `{head}…` in:\n{native}");
    }
    assert!(native.lines().any(|l| l.starts_with("text file parent: err fs.read_text_if_exists(\"f.txt/x\"): ")), "{native}");
    assert!(native.lines().any(|l| l == "text missing: ok none"), "{native}");
    assert!(native.lines().any(|l| l == "text present: ok some(\"hi\")"), "{native}");

    let wasm = run(&dir, &["--target", "wasm"]);
    assert_eq!(wasm, native, "the embedded wasm host differs from native");

    // The stock artifact under wasmtime (skipped without the CLI).
    let wasm_file = dir.join("probe.wasm");
    let o = Command::new(almide_bin()).args(["build", "probe.almd", "--target", "wasm", "-o"]).arg(&wasm_file).current_dir(&dir).output().expect("spawn");
    assert!(o.status.success(), "stock build: {}", String::from_utf8_lossy(&o.stderr));
    let Ok(o) = Command::new("wasmtime").args(["run", "--dir=/"]).arg(format!("--env=PWD={}", dir.display())).arg(&wasm_file).current_dir(&dir).output() else {
        eprintln!("SKIP stock leg: no wasmtime CLI on PATH");
        return;
    };
    assert!(o.status.success(), "stock run: {}", String::from_utf8_lossy(&o.stderr));
    let stock = String::from_utf8_lossy(&o.stdout).to_string();
    let head = |l: &str| l.split_once("): ").map_or(l.to_string(), |(h, _)| h.to_string());
    let heads = |s: &str| s.lines().map(head).collect::<Vec<_>>();
    assert_eq!(heads(&stock), heads(&native), "the stock artifact answers a different shape:\n{stock}");
}
