//! A call that lends a `Bytes` to a user fn's `mut` param while another of its
//! arguments reads the same buffer through a call (`setf(buf, i, getf(buf, i)
//! + 1.0)`), on both legs (#4594).
//!
//! The wasm call site moved the buffer into the `mut` param only when no
//! other argument mentioned it beyond a scalar index or field read; a call
//! reading it counted as a mention, so the buffer was shared into the call
//! (rc 2) and the callee's copy-on-write judge copied all of it on every call.
//! A call that only lends the buffer to a borrowed param of a callee that can
//! neither write it nor keep it (no `mut` param, no closure argument, no
//! module-level variable in its reach) now lets the move happen. The values
//! must not change: an alias bound before the call keeps the old buffer, and a
//! reader that stashes the buffer in a module-level `var` still sees the value
//! from before the write.

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
var keep = bytes.zeros(0)

fn getf(b: Bytes, i: Int) -> Float = bytes.get_f32_le(b, i * 4) ?? 0.0

fn setf(mut b: Bytes, i: Int, v: Float) -> Unit = bytes.put_f32_le(b, i * 4, v)

fn grab(b: Bytes, i: Int) -> Float = {
  keep = b
  getf(b, i)
}

fn bump(mut buf: Bytes, n: Int) -> Unit = {
  for i in 0..<n { setf(buf, i, getf(buf, i) + 1.0) }
}

fn bump_grab(mut buf: Bytes, i: Int) -> Unit = setf(buf, i, grab(buf, i) + 10.0)

effect fn main() -> Unit = {
  var buf = bytes.zeros(16)
  bump(buf, 4)
  let old = buf
  for i in 0..<4 { setf(buf, i, getf(buf, i) + 1.0) }
  bump(buf, 2)
  bump_grab(buf, 0)
  setf(buf, 1, getf(buf, 1) + getf(old, 1))
  println("${getf(old, 0)} ${getf(old, 3)} ${getf(buf, 0)} ${getf(buf, 1)} ${getf(buf, 2)} ${getf(buf, 3)} ${getf(keep, 0)}")
}
"#;

const WANT: &str = "1 1 13 4 2 2 3\n";

/// 50,000 calls on a 20 MB buffer, each reading the buffer in an argument:
/// a terabyte of copying when each call copies the buffer (minutes),
/// milliseconds when none does. `a` threads a `mut` param (m1), `v` makes
/// the calls on a local `var` (m3).
const SCALE: &str = r#"
import env

fn getf(b: Bytes, i: Int) -> Float = bytes.get_f32_le(b, i * 4) ?? 0.0

fn setf(mut b: Bytes, i: Int, v: Float) -> Unit = bytes.put_f32_le(b, i * 4, v)

fn bump(mut buf: Bytes, n: Int) -> Unit = {
  for i in 0..<n { setf(buf, i, getf(buf, i) + 1.0) }
}

effect fn main() -> Unit = {
  let n = 50000
  var buf = bytes.zeros(20 * 1024 * 1024)
  if (list.get(env.args(), 0) ?? "a") == "a" then bump(buf, n)
  else {
    for i in 0..<n { setf(buf, i, getf(buf, i) + 1.0) }
  }
  println("${getf(buf, n - 1)} ${getf(buf, n)}")
}
"#;

fn source_file(tag: &str, src: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-mut-param-arg-read-{tag}"));
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
fn mut_param_arg_read_keeps_its_values_natively() {
    let out = run("values", SRC, None);
    assert!(out.contains(WANT), "native:\n{out}");
}

#[test]
fn mut_param_arg_read_keeps_its_values_on_wasm() {
    let out = run("values", SRC, Some("wasm"));
    assert!(out.contains(WANT), "wasm:\n{out}");
}

/// Each leg must not scale with the buffer: built first, then run under a
/// deadline far above the fixed time (milliseconds) and far below the copying
/// one (minutes on a CI runner).
fn assert_no_copy(target: Option<&str>, mode: &str) {
    let leg = target.unwrap_or("native");
    let file = source_file(&format!("scale-{leg}-{mode}"), SCALE);
    let mut build = Command::new(almide_bin());
    build.arg("build").arg(&file).arg("-o").arg(file.with_extension("bin"));
    if let Some(t) = target {
        build.args(["--target", t]);
    }
    let built = build.output().expect("spawn almide build");
    assert!(built.status.success(), "build failed:\n{}", String::from_utf8_lossy(&built.stderr));
    let mut run = Command::new(almide_bin());
    run.arg("run").arg(&file);
    if let Some(t) = target {
        run.args(["--target", t]);
    }
    let mut child = run.arg("--").arg(mode).stdout(std::process::Stdio::piped()).spawn().expect("spawn almide run");
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if child.try_wait().expect("wait").is_some() {
            break;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("50,000 `mut` calls reading their buffer in an argument (mode {mode}) did not finish in 60 s on {leg}: the buffer is copied per call (#4594)");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let out = child.wait_with_output().expect("output");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("1 0\n"), "{leg} {mode}:\n{text}");
}

#[test]
fn mut_param_arg_read_does_not_copy_on_wasm() {
    assert_no_copy(Some("wasm"), "a");
}

#[test]
fn local_var_arg_read_does_not_copy_on_wasm() {
    assert_no_copy(Some("wasm"), "v");
}

#[test]
fn mut_param_arg_read_does_not_copy_natively() {
    assert_no_copy(None, "a");
}

#[test]
fn local_var_arg_read_does_not_copy_natively() {
    assert_no_copy(None, "v");
}
