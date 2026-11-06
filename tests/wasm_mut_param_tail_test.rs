//! A user fn whose `mut b: Bytes` param is handed on to a stdlib `mut` call in
//! tail position (`fn setf(mut b: Bytes, i: Int, v: Float) = bytes.put_f32_le(b,
//! i * 4, v)`), on both legs (#4580).
//!
//! The wasm leg lowered that tail `bytes.put_f32_le` as a plain call: the frame
//! shared `b` into it (rc 2) and released its own credit only after it
//! returned, so the callee's copy-on-write judge copied the whole buffer on
//! every call. A 20 MB buffer written a million times never finished. The call
//! now keeps its tail position and the dying param moves in. The values must
//! not change: a caller that still holds the old buffer still sees it.

use std::path::Path;
use std::process::Command;
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

const SRC: &str = r#"
fn setf(mut b: Bytes, i: Int, v: Float) -> Unit = bytes.put_f32_le(b, i * 4, v)

fn setf_fwd(mut b: Bytes, i: Int, v: Float) -> Unit = setf(b, i, v)

fn stamp(mut b: Bytes, o: Int, x: Int) -> Unit = {
  bytes.put_u8(b, o, x)
  bytes.put_u8(b, o + 1, x + 1)
}

fn getf(b: Bytes, i: Int) -> Float = bytes.get_f32_le(b, i * 4) ?? 0.0

effect fn main() -> Unit = {
  var b = bytes.zeros(16)
  setf(b, 0, 1.0)
  let old = b
  setf(b, 0, 2.0)
  setf_fwd(b, 1, 3.0)
  stamp(b, 8, 5)
  let kept = b
  for i in 0..<4 { setf(b, 3, float.from(i)) }
  println("${getf(old, 0)} ${getf(old, 1)} ${getf(b, 0)} ${getf(b, 1)} ${bytes.get(b, 9) ?? 0} ${getf(kept, 3)} ${getf(b, 3)} ${bytes.len(b)}")
}
"#;

const WANT: &str = "1 0 2 3 6 0 3 16\n";

/// 200,000 calls of the helper on a 20 MB buffer: 4 TB of copying when each
/// call copies the buffer (minutes), milliseconds when none does.
const SCALE: &str = r#"
fn setf(mut b: Bytes, i: Int, v: Float) -> Unit = bytes.put_f32_le(b, i * 4, v)

effect fn main() -> Unit = {
  let n = 200000
  var buf = bytes.zeros(20 * 1024 * 1024)
  for i in 0..<n { setf(buf, i, float.from(i % 7)) }
  println("${bytes.get_f32_le(buf, (n - 1) * 4) ?? 0.0} ${bytes.get_f32_le(buf, 6 * 4) ?? 0.0}")
}
"#;

fn source_file(tag: &str, src: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-mut-param-tail-{tag}"));
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("main.almd");
    std::fs::write(&file, src).unwrap();
    file
}

fn run(tag: &str, src: &str, target: Option<&str>) -> String {
    let file = source_file(&format!("{tag}-{}", target.unwrap_or("native")), src);
    let mut cmd = Command::new(almide_bin());
    cmd.arg("run").arg(&file);
    if let Some(t) = target {
        cmd.args(["--target", t]);
    }
    let out = cmd.output().expect("spawn almide");
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

#[test]
fn mut_param_tail_call_keeps_its_values_natively() {
    let out = run("values", SRC, None);
    assert!(out.contains(WANT), "native:\n{out}");
}

#[test]
fn mut_param_tail_call_keeps_its_values_on_wasm() {
    let out = run("values", SRC, Some("wasm"));
    assert!(out.contains(WANT), "wasm:\n{out}");
}

/// The wasm run must not scale with the buffer: built first, then run under a
/// deadline far above the fixed time (milliseconds) and far below the copying
/// one (minutes on a CI runner).
#[test]
fn mut_param_tail_call_does_not_copy_the_buffer_on_wasm() {
    let file = source_file("scale", SCALE);
    let wasm = file.with_extension("wasm");
    let built = Command::new(almide_bin())
        .arg("build")
        .arg(&file)
        .args(["--target", "wasm", "-o"])
        .arg(&wasm)
        .output()
        .expect("spawn almide build");
    assert!(built.status.success(), "build failed:\n{}", String::from_utf8_lossy(&built.stderr));
    let mut child = Command::new(almide_bin())
        .arg("run")
        .arg(&file)
        .args(["--target", "wasm"])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("spawn almide run");
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if child.try_wait().expect("wait").is_some() {
            break;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("200,000 mut-param calls on a 20 MB buffer did not finish in 60 s on wasm: the buffer is copied per call (#4580)");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let out = child.wait_with_output().expect("output");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("2 6\n"), "wasm:\n{text}");
}
