//! `bytes.to_list(io.read(io.stdin()!, n)!)` answers min(n, what stdin has) on every leg (C-161's
//! 2026-08-17 correction: no cap). The stock artifact's op-35 shim
//! (`crates/almide-wasi/src/wasi_shims.rs`) made ONE `fd_read` of at most
//! 4096 bytes per call, so a call asking for 20000 bytes of a 5 MB stream
//! answered 4096 — and 256 such calls read 1048576 bytes where native and
//! the embedded host read 5120000. The p2 component's shim made one
//! `blocking-read`, which answers whatever the stream has ready (16 KiB on
//! wasmtime), and the p3 component's read landed in the park's staging page
//! whatever the count, so a 1 MB request wrote past it (an out-of-bounds
//! trap here, a corrupted heap below the memory end). A short answer is EOF
//! to every caller (`io.read_all`, the chunked self-host), so every shim now
//! fills the request, stopping only at EOF.
//!
//! The probe reads below the park's staging room, above it (1 MB), and far
//! above what stdin holds (100 MB of a 2.8 MB remainder), then reads at EOF.
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const PROBE: &str = r#"import io

effect fn main() -> Unit = {
  var total = 0
  var i = 0
  while i < 64 {
    let r: List[Int] = bytes.to_list(io.read(io.stdin()!, 20000)!)
    total = total + list.len(r)
    i = i + 1
  }
  let big: List[Int] = bytes.to_list(io.read(io.stdin()!, 1000000)!)
  let newlines = list.len(big |> list.filter((b) => b == 10))
  let rest: List[Int] = bytes.to_list(io.read(io.stdin()!, 100000000)!)
  let last = list.last(rest) ?? -1
  let tail: List[Int] = bytes.to_list(io.read(io.stdin()!, 10)!)
  println("${total} ${list.len(big)} ${newlines} ${list.len(rest)} ${last} ${list.len(tail)}")
}
"#;

const WANT: &str = "1280000 1000000 50 2840000 10 0\n";

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

fn has_wasmtime() -> bool {
    let ok = Command::new("wasmtime").arg("--version").output().is_ok();
    if !ok {
        assert!(std::env::var_os("ALMIDE_EXPECT_TOOLS").is_none(), "ALMIDE_EXPECT_TOOLS: wasmtime is not on PATH");
        eprintln!("skipping: wasmtime is not on PATH");
    }
    ok
}

/// 256 lines of 19999 'a' and a newline: 5120000 bytes.
fn stdin_bytes() -> Vec<u8> {
    let mut line = vec![b'a'; 19999];
    line.push(b'\n');
    line.repeat(256)
}

fn probe_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("almide-stock-stdin-take-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("mkdir");
    std::fs::write(d.join("probe.almd"), PROBE).expect("write");
    std::fs::write(d.join("in.txt"), stdin_bytes()).expect("write");
    d
}

/// stdin redirected from the file.
fn run_redirected(mut c: Command, dir: &Path) -> Output {
    let f = std::fs::File::open(dir.join("in.txt")).expect("open");
    c.current_dir(dir).stdin(f).output().expect("spawn")
}

/// stdin through a pipe written from another thread, so reads come back
/// short whenever the writer is behind.
fn run_piped(mut c: Command, dir: &Path) -> Output {
    let mut child = c.current_dir(dir).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().expect("spawn");
    let mut sink = child.stdin.take().expect("stdin");
    let feeder = std::thread::spawn(move || {
        for chunk in stdin_bytes().chunks(7001) {
            if sink.write_all(chunk).is_err() {
                return;
            }
        }
    });
    let o = child.wait_with_output().expect("wait");
    feeder.join().expect("feeder");
    o
}

fn stdout_of(o: Output, leg: &str) -> String {
    assert!(o.status.success(), "{leg} failed:\n{}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).expect("utf8")
}

fn almide(args: &[&str]) -> Command {
    let mut c = Command::new(almide_bin());
    c.args(args);
    c
}

fn build(dir: &Path, out: &str, extra: &[&str], env: &[(&str, &str)]) {
    let mut c = almide(&["build", "probe.almd", "--target", "wasm", "-o", out]);
    c.args(extra).current_dir(dir).envs(env.iter().copied());
    let o = c.output().expect("spawn almide");
    assert!(o.status.success(), "build {extra:?} failed:\n{}", String::from_utf8_lossy(&o.stderr));
}

fn wasmtime(wasm: &str) -> Command {
    let mut c = Command::new("wasmtime");
    c.args(["run", wasm]);
    c
}

#[test]
fn native_and_embedded_fill_the_request() {
    let dir = probe_dir("host");
    assert_eq!(stdout_of(run_redirected(almide(&["run", "probe.almd"]), &dir), "native"), WANT);
    let embedded = run_redirected(almide(&["run", "probe.almd", "--target", "wasm"]), &dir);
    assert_eq!(stdout_of(embedded, "embedded"), WANT);
}

#[test]
fn stock_p1_fills_the_request() {
    if !has_wasmtime() {
        return;
    }
    let dir = probe_dir("p1");
    build(&dir, "p1.wasm", &[], &[]);
    assert_eq!(stdout_of(run_redirected(wasmtime("p1.wasm"), &dir), "stock p1 (redirected)"), WANT);
    assert_eq!(stdout_of(run_piped(wasmtime("p1.wasm"), &dir), "stock p1 (piped)"), WANT);
}

#[test]
fn p2_component_fills_the_request() {
    if !has_wasmtime() {
        return;
    }
    let dir = probe_dir("p2");
    build(&dir, "p2.wasm", &["--component"], &[]);
    assert_eq!(stdout_of(run_redirected(wasmtime("p2.wasm"), &dir), "p2 component (redirected)"), WANT);
    assert_eq!(stdout_of(run_piped(wasmtime("p2.wasm"), &dir), "p2 component (piped)"), WANT);
}

/// wasmtime 46+ runs a WASI 0.3 component with no flag.
fn wasmtime_runs_p3() -> bool {
    let Ok(o) = Command::new("wasmtime").arg("--version").output() else {
        return false;
    };
    let v = String::from_utf8_lossy(&o.stdout).to_string();
    let major = v.split_whitespace().nth(1).and_then(|ver| ver.split('.').next()).and_then(|m| m.parse::<u32>().ok());
    let ok = major.unwrap_or(0) >= 46;
    if !ok {
        eprintln!("skipping: {} is below the p3 floor (46)", v.trim());
    }
    ok
}

#[test]
fn p3_component_fills_the_request() {
    if !has_wasmtime() || !wasmtime_runs_p3() {
        return;
    }
    let dir = probe_dir("p3");
    build(&dir, "p3.wasm", &["--component"], &[("ALMIDE_COMPONENT_P3", "1")]);
    assert_eq!(stdout_of(run_redirected(wasmtime("p3.wasm"), &dir), "p3 component (redirected)"), WANT);
    assert_eq!(stdout_of(run_piped(wasmtime("p3.wasm"), &dir), "p3 component (piped)"), WANT);
}
