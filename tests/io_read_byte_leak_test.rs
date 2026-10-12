//! `io.read_byte` leaves no heap block live on wasm. The structural arm read
//! the byte into a one-byte staging block and never released it: one block
//! per byte read. The inspection campaign's effect catalogue (#3627) put
//! `io.read_byte` into programs for every function under test, so this one
//! leak failed the parity criterion across about a hundred unrelated
//! functions.
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn read_byte_releases_its_staging_block_on_wasm() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("leak.almd");
    let src = "import io\neffect fn main() -> Unit = {\n  let a = io.read_byte()\n  let b = io.read_byte()\n  let c = io.read_byte()\n  println(\"${a} ${b} ${c}\")\n}\n";
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
