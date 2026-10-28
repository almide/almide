//! A one-byte `io.read(io.stdin()!, 1)` leaves no heap block live on wasm. The
//! structural arm of its predecessor `io.read_byte` read the byte into a
//! one-byte staging block and never released it: one block per byte read.
//! The inspection campaign's effect catalogue (#3627) put the byte read into
//! programs for every function under test, so this one
//! leak failed the parity criterion across about a hundred unrelated
//! functions.
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn a_one_byte_read_releases_its_staging_block_on_wasm() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("leak.almd");
    let src = "import io\neffect fn main() -> Unit = {\n  let a = (bytes.get(io.read(io.stdin()!, 1)!, 0) ?? -1)\n  let b = (bytes.get(io.read(io.stdin()!, 1)!, 0) ?? -1)\n  let c = (bytes.get(io.read(io.stdin()!, 1)!, 0) ?? -1)\n  println(\"${a} ${b} ${c}\")\n}\n";
    std::fs::write(&file, src).expect("source");
    let mut child = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("run")
        .arg(&file)
        .args(["--target", "wasm"])
        .current_dir(dir.path())
        .env("ALMIDE_WASM_ALLOC_COUNT", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("almide run");
    child.stdin.as_mut().expect("stdin").write_all(b"ab").expect("write stdin");
    let out = child.wait_with_output().expect("wait");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{err}");
    // two bytes, then EOF.
    assert_eq!(String::from_utf8_lossy(&out.stdout), "97 98 -1\n");
    let line = err.lines().find(|l| l.contains("__ALMD_WASM_ALLOC")).expect("the alloc count line");
    assert!(line.contains(" live=0 "), "read_byte left blocks live: {line}");
}
