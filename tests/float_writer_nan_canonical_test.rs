//! C-210 over the whole bytes float-writer family (#3658).
//!
//! Every bytes float writer is a NaN-observation boundary: the appends
//! (`append_f32/f64_le/be`) and the in-place puts (`put_f32/f64_le/be`).
//! Each writes the canonical NaN — f64 `0x7FF8000000000000`, f32 `0x7FC00000`
//! — on native and on `--target wasm`, whatever sign and payload the NaN
//! carries. Before #3658 only the appends canonicalized; the in-place writes
//! put the raw bits in the buffer on both backends while the interpreter
//! canonicalized, so the third oracle disagreed with both.
//!
//! The NaN comes from `float.from_bits(-256)`: sign set, a low payload — the
//! pattern no host's arithmetic produces, so a raw write cannot pass by
//! accident on aarch64 (where `0.0 / 0.0` is already canonical). The bits are
//! read through an `effect fn` so the folder cannot see the value.
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const PER_RUN_LIMIT: Duration = Duration::from_secs(120);

const PROGRAM: &str = r#"effect fn nan_bits() -> Int = 0 - 256

effect fn main() -> Unit = {
  let x = float.from_bits(nan_bits()!)
  var a = bytes.zeros(0)
  bytes.append_f64_le(a, x)
  bytes.append_f64_be(a, x)
  bytes.append_f32_le(a, x)
  bytes.append_f32_be(a, x)
  println("append=${bytes.to_list(a)}")
  var s = bytes.zeros(24)
  bytes.put_f64_le(s, 0, x)
  bytes.put_f64_be(s, 8, x)
  bytes.put_f32_le(s, 16, x)
  bytes.put_f32_be(s, 20, x)
  println("set=${bytes.to_list(s)}")
}
"#;

const F64_LE: &str = "0, 0, 0, 0, 0, 0, 248, 127";
const F64_BE: &str = "127, 248, 0, 0, 0, 0, 0, 0";
const F32_LE: &str = "0, 0, 192, 127";
const F32_BE: &str = "127, 192, 0, 0";

fn expected() -> String {
    [
        format!("append=[{F64_LE}, {F64_BE}, {F32_LE}, {F32_BE}]"),
        format!("set=[{F64_LE}, {F64_BE}, {F32_LE}, {F32_BE}]"),
    ]
    .join("\n")
}

fn almide() -> String {
    std::env::var("ALMIDE_BIN")
        .unwrap_or_else(|_| format!("{}/target/release/almide", env!("CARGO_MANIFEST_DIR")))
}

fn has_wasmtime() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok()
}

/// (exit code, stdout, stderr), or `None` for the exit code when the run was
/// killed at the per-run limit — a hang is a failure, never a wait.
fn run_limited(mut cmd: Command) -> (Option<i32>, String, String) {
    let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().expect("spawn almide");
    let (mut so, mut se) = (child.stdout.take().unwrap(), child.stderr.take().unwrap());
    let to = std::thread::spawn(move || {
        let mut s = String::new();
        std::io::Read::read_to_string(&mut so, &mut s).ok();
        s
    });
    let te = std::thread::spawn(move || {
        let mut s = String::new();
        std::io::Read::read_to_string(&mut se, &mut s).ok();
        s
    });
    let start = Instant::now();
    let status = loop {
        if let Some(st) = child.try_wait().expect("wait") {
            break Some(st);
        }
        if start.elapsed() > PER_RUN_LIMIT {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let (out, err) = (to.join().unwrap(), te.join().unwrap());
    (status.and_then(|s| s.code()), out.trim().to_string(), err.trim().to_string())
}

fn leg(file: &std::path::Path, wasm: bool) -> (Option<i32>, String, String) {
    let mut cmd = Command::new(almide());
    cmd.args(["run", file.to_str().unwrap()]);
    if wasm {
        cmd.args(["--target", "wasm"]);
    }
    run_limited(cmd)
}

#[test]
fn every_bytes_float_writer_writes_the_canonical_nan_on_both_legs() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("main.almd");
    std::fs::write(&file, PROGRAM).unwrap();
    let native = leg(&file, false);
    assert_eq!(native, (Some(0), expected(), String::new()), "native leg");
    if !has_wasmtime() {
        eprintln!("skip wasm leg: wasmtime unavailable");
        return;
    }
    let wasm = leg(&file, true);
    assert_eq!(wasm, native, "wasm leg must be byte-identical to native");
}
