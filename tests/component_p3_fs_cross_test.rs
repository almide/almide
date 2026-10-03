//! #3140: the p3 component answers the fs surface byte-for-byte as native.
//!
//! Every fs cross-target fixture (`spec/wasm_cross/fs_*.almd`,
//! `spec/embedded_cross/fs_*.almd`, plus the fan prefetch, the platform
//! reporting and the line-ending fixtures) is run natively (`almide run`) and
//! as a WASI 0.3 component (`ALMIDE_COMPONENT_P3=1 almide build --component`,
//! then a stock `wasmtime run --dir=/ -S inherit-env=y`, the stock-p1 gate's
//! own grant), from the same cwd with the same `PWD` — and stdout, stderr and
//! the exit code must be identical. The component serves these through the
//! spliced p1 fs service over its wasi:filesystem@0.3 adapter, so this is the
//! op-family evidence for every fs op the p3 shim used to refuse: list_dir,
//! read_lines (and fold_lines / for_each_line / the range and chunked
//! readers), file_size, modified_at, copy, rename, the temp-dir and temp-file
//! creators, is_symlink, walk, stat, glob, the *_if_exists readers and the
//! raw-bytes readers, with their `fs.<call>("<path>")` error heads.
//!
//! The runtime floor is the pin policy's (wasmtime 46); below it the test
//! skips, and under ALMIDE_EXPECT_TOOLS (CI) that is a failure.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().expect("utf8 path").to_string();
    }
    "almide".to_string()
}

/// The installed wasmtime runs p3 components (the pin policy's floor, 46).
fn wasmtime_runs_p3() -> bool {
    let Ok(o) = Command::new("wasmtime").arg("--version").output() else {
        assert!(std::env::var_os("ALMIDE_EXPECT_TOOLS").is_none(), "ALMIDE_EXPECT_TOOLS: wasmtime is not on PATH");
        return false;
    };
    let v = String::from_utf8_lossy(&o.stdout).to_string();
    let major = v
        .split_whitespace()
        .nth(1)
        .and_then(|ver| ver.split('.').next())
        .and_then(|m| m.parse::<u32>().ok())
        .unwrap_or(0);
    if major < 46 {
        assert!(
            std::env::var_os("ALMIDE_EXPECT_TOOLS").is_none(),
            "ALMIDE_EXPECT_TOOLS: {} is below the p3 floor (46)",
            v.trim()
        );
        eprintln!("skipping: {} is below the p3 floor (46)", v.trim());
        return false;
    }
    true
}

/// What a leg observed: stdout, stderr, exit code.
type Seen = (String, String, Option<i32>);

/// Run `cmd` to completion under a 120 s watchdog.
fn observe(mut cmd: Command, what: &str) -> Seen {
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().expect(what);
    let deadline = Instant::now() + Duration::from_secs(120);
    while child.try_wait().expect("try_wait").is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("{what}: still running after 120 s");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let out = child.wait_with_output().expect(what);
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code(),
    )
}

/// The fixtures: every `fs_*` fixture of both cross lanes, and the fs-reaching
/// fixtures named otherwise.
fn fixtures() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("spec");
    let mut out = Vec::new();
    for lane in ["wasm_cross", "embedded_cross"] {
        let mut names: Vec<PathBuf> = std::fs::read_dir(root.join(lane))
            .expect("fixture dir")
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.extension().is_some_and(|x| x == "almd")
                    && p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("fs_"))
            })
            .collect();
        names.sort();
        out.extend(names);
    }
    for extra in ["fan_prefetch_fs.almd", "env_platform_reporting.almd", "line_endings_bare_cr.almd"] {
        out.push(root.join("wasm_cross").join(extra));
    }
    out
}

/// `src` natively and as a p3 component, both from `cwd`: (native, p3), or
/// the p3 build's failure.
fn both_legs(src: &Path, cwd: &Path, scratch: &Path) -> Result<(Seen, Seen), String> {
    let name = src.file_name().expect("name").to_string_lossy().into_owned();
    let mut native = Command::new(almide_bin());
    native.arg("run").arg(src).current_dir(cwd).env("PWD", cwd);
    let native = observe(native, "almide run");
    let wasm = scratch.join(format!("{name}.wasm"));
    let built = Command::new(almide_bin())
        .args(["build", src.to_str().expect("utf8"), "--target", "wasm", "--component", "-o", wasm.to_str().expect("utf8")])
        .env("ALMIDE_COMPONENT_P3", "1")
        .output()
        .expect("almide build");
    if !built.status.success() {
        return Err(format!("{name}: the p3 build failed:\n{}", String::from_utf8_lossy(&built.stderr)));
    }
    let mut p3 = Command::new("wasmtime");
    p3.args(["run", "--dir=/", "-S", "inherit-env=y"]).arg(&wasm).current_dir(cwd).env("PWD", cwd);
    Ok((native, observe(p3, "wasmtime run")))
}

#[test]
fn p3_component_fs_fixtures_match_native() {
    if Command::new(almide_bin()).arg("--version").output().is_err() || !wasmtime_runs_p3() {
        return;
    }
    let scratch = tempfile::tempdir().expect("tempdir");
    let cwd = scratch.path().join("cwd");
    std::fs::create_dir_all(&cwd).expect("cwd");
    let fixtures = fixtures();
    assert!(fixtures.len() >= 19, "the fs fixture set shrank to {}", fixtures.len());
    let mut problems = Vec::new();
    for src in &fixtures {
        let name = src.file_name().expect("name").to_string_lossy().into_owned();
        match both_legs(src, &cwd, scratch.path()) {
            Err(e) => problems.push(e),
            Ok((native, p3)) if p3 != native => {
                problems.push(format!("{name}: the p3 component differs from native\n  native: {native:?}\n  p3:     {p3:?}"))
            }
            Ok(_) => {}
        }
    }
    assert!(problems.is_empty(), "{} fixture(s) diverge:\n\n{}", problems.len(), problems.join("\n\n"));
}

/// The #3210 shape on p3: payloads far past one 64 KiB slice — a write, an
/// append, the text and bytes readers, a copy and the fan prefetch read —
/// go through the adapter's per-slice stream calls and come back whole.
const LARGE: &str = r#"import fs

effect fn main() -> Unit = {
  let d = fs.create_temp_dir("almide_p3_large")!
  let p = d + "/big.txt"
  let s = string.repeat("0123456789abcdef", 40000)
  fs.write(p, s)!
  let t = fs.read_text(p)!
  println("len=${string.len(t)} same=${t == s}")
  fs.append(p, s)!
  println("size=${fs.file_size(p)!}")
  println("bytes=${list.len(fs.read_bytes(p)!)}")
  fs.copy(p, d + "/c.txt")!
  println("copy=${fs.file_size(d + "/c.txt")!}")
  let parts = fan.map([p, d + "/c.txt"], (q) => fs.read_text(q)!)!
  println("fan=${list.len(parts)} ${string.len(list.get(parts, 1) ?? "")}")
  fs.remove_all(d)!
}
"#;

#[test]
fn p3_component_reads_and_writes_past_the_host_per_call_bound() {
    if Command::new(almide_bin()).arg("--version").output().is_err() || !wasmtime_runs_p3() {
        return;
    }
    let scratch = tempfile::tempdir().expect("tempdir");
    let src = scratch.path().join("large.almd");
    std::fs::write(&src, LARGE).expect("write");
    let (native, p3) = both_legs(&src, scratch.path(), scratch.path()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(native.0, "len=640000 same=true\nsize=1280000\nbytes=1280000\ncopy=1280000\nfan=2 1280000\n", "native: {native:?}");
    assert_eq!(p3, native, "the p3 component differs from native past the per-call bound");
}
