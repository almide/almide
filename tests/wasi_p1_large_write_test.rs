//! #3206 (C-162): the stock preview-1 artifact (`almide build --target wasm`,
//! the structural module through `to_wasi`) writes a payload of any size in
//! full. wasmtime 47 bounds what ONE `fd_write` / `fd_read` iovec may name
//! (its per-call hostcall fuel, ~128 MiB) and answers ENOMEM having moved
//! nothing, and the shims handed it the whole payload in one call and dropped
//! the errno: `println(string.pad_start("wide", 2147483647, " "))` printed an
//! empty line and exited 0, where native printed the 2 GiB line. The fs
//! service's whole-file read and write loops failed the same way, with errno
//! 48 instead of silently. Every p1 write and read is now sliced at 64 KiB
//! and advances by the call's count.
//!
//! 160 MiB is past that bound and small enough for a test. Measured against
//! 0.64.0 (`ALMIDE_BIN`): the run failed at `fs.write` after both 160 MiB
//! writes to stdout had printed nothing. Skipped without wasmtime on PATH.
use std::path::{Path, PathBuf};
use std::process::Command;

const N: usize = 160 * 1024 * 1024;

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

fn probe_dir() -> PathBuf {
    let d = std::env::temp_dir().join(format!("almide-wasi-p1-large-write-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("mkdir");
    let src = format!(
        r#"import fs
import io

effect fn main() -> Unit = {{
  let s = string.pad_start("wide", {N}, " ")
  println(s)
  io.write(bytes.from_string(s))
  fs.write("big.txt", s)!
  let t = fs.read_text("big.txt")!
  println("")
  println(int.to_string(string.len(t)))
  println(string.capitalize("ok"))
}}
"#
    );
    std::fs::write(d.join("probe.almd"), src).expect("write");
    d
}

#[test]
fn a_stock_p1_artifact_writes_and_reads_a_payload_past_the_host_per_call_bound() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    if Command::new("sh").args(["-c", "command -v wasmtime"]).output().map(|o| !o.status.success()).unwrap_or(true) {
        return;
    }
    let dir = probe_dir();
    let wasm = dir.join("probe.wasm");
    let o = Command::new(almide_bin())
        .args(["build", "probe.almd", "--target", "wasm", "-o"])
        .arg(&wasm)
        .current_dir(&dir)
        .output()
        .expect("spawn almide");
    assert!(o.status.success(), "stock build failed:\n{}", String::from_utf8_lossy(&o.stderr));
    let o = Command::new("wasmtime")
        .args(["run", "--dir=/"])
        .arg(format!("--env=PWD={}", dir.display()))
        .arg(&wasm)
        .current_dir(&dir)
        .output()
        .expect("spawn wasmtime");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(o.status.success(), "stock run failed:\n{}", String::from_utf8_lossy(&o.stderr));
    let line = format!("{}wide", " ".repeat(N - 4));
    let expected = format!("{line}\n{line}\n{N}\nOk\n");
    assert_eq!(o.stdout.len(), expected.len(), "stdout length (a dropped write shows here)");
    assert!(o.stdout == expected.as_bytes(), "stdout bytes differ from native's");
}
