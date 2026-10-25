//! `io.read(io.stdin()!, n)` leaves no heap block live on wasm. The structural
//! arm of its predecessor `io.read_n_bytes` took the host's bytes into a
//! staging block, decoded them into the returned `List[Int]`, and never
//! released the staging block: one block per
//! call, so a read-to-EOF loop leaked one per chunk. Found while porting
//! `process.stdin_lines` to wasm (#3620), whose body reads stdin this way.
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn an_n_byte_read_releases_its_staging_block_on_wasm() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("leak.almd");
    let src = "import io\neffect fn main() -> Unit = {\n  let a = bytes.to_list(io.read(io.stdin()!, 4)!)\n  let b = bytes.to_list(io.read(io.stdin()!, 4096)!)\n  let c = bytes.to_list(io.read(io.stdin()!, 8)!)\n  println(\"${list.len(a)} ${list.len(b)} ${list.len(c)}\")\n}\n";
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
    child.stdin.as_mut().expect("stdin").write_all(b"abcdefg").expect("write stdin");
    let out = child.wait_with_output().expect("wait");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{err}");
    // a full take, a short take, and a take at EOF.
    assert_eq!(String::from_utf8_lossy(&out.stdout), "4 3 0\n");
    let line = err.lines().find(|l| l.contains("__ALMD_WASM_ALLOC")).expect("the alloc count line");
    assert!(line.contains(" live=0 "), "read_n_bytes left blocks live: {line}");
}
