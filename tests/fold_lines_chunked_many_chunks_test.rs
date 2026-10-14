//! Native `fs.fold_lines_chunked` spawned ONE thread per chunk, and a chunk
//! is at least one byte, so `workers` past the file size meant one thread
//! per byte. A 4096-byte input with `workers = 2147483647` crashed the
//! native run (exit 101, "failed to spawn thread: Resource temporarily
//! unavailable") on a c4-standard-4 VM where wasm answered `ok`; the stdlib
//! inspection's parity criterion found it (2026-10-14, seed 1791975162).
//! The partition loop also walked all `workers` candidates, 2^31 of them.
//!
//! The answer is unchanged: one partial per non-empty chunk, in chunk order.
//! Only the threads that compute it are bounded, and a thread the OS
//! refuses is an `err`, never a crash.
use std::process::Command;
use std::time::{Duration, Instant};

const SOURCE: &str = r#"import fs

effect fn main() -> Unit = {
  let r: Result[List[Int], String] = fs.fold_lines_chunked("lines.txt", 2147483647, 0, (acc, l) => acc + 1)
  match r {
    ok(parts) => println("ok ${list.len(parts)} ${list.sum(parts)}"),
    err(e) => println("err ${e}"),
  }
  let s: Result[List[Int], String] = fs.fold_lines_chunked("lines.txt", 3, 0, (acc, l) => acc + string.len(l))
  println("${s}")
}
"#;

#[test]
fn many_chunks_fold_on_bounded_threads_with_the_same_answer() {
    let dir = tempfile::tempdir().expect("scratch");
    // 2,000 lines of 4 bytes: 8,000 bytes, so 8,000 one-byte chunks.
    let text: String = (0..2000).map(|i| format!("{:03}\n", i % 1000)).collect();
    std::fs::write(dir.path().join("lines.txt"), &text).expect("input");
    let file = dir.path().join("chunks.almd");
    std::fs::write(&file, SOURCE).expect("source");

    let exe = dir.path().join("chunks");
    let build = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("build")
        .arg(&file)
        .arg("-o")
        .arg(&exe)
        .arg("--release")
        .output()
        .expect("build");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
    let t = Instant::now();
    let native = Command::new(&exe).current_dir(dir.path()).output().expect("run artifact");
    let took = t.elapsed();
    assert!(native.status.success(), "native crashed: {}", String::from_utf8_lossy(&native.stderr));

    let wasm = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("run")
        .arg(&file)
        .args(["--target", "wasm"])
        .current_dir(dir.path())
        .output()
        .expect("wasm run");
    assert!(wasm.status.success(), "{}", String::from_utf8_lossy(&wasm.stderr));
    let n = String::from_utf8_lossy(&native.stdout);
    assert_eq!(n, String::from_utf8_lossy(&wasm.stdout), "native and wasm differ");
    assert!(n.starts_with("ok 8000 2000\n"), "{n}");
    assert!(took < Duration::from_secs(1), "native took {took:?}");
}
