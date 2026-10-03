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
//! #3223 adds the env.set overlay: the env fixtures run on three legs —
//! native, the stock p1 core module and the p3 component — and must agree
//! byte-for-byte (`p3_component_env_set_matches_native_and_p1`).
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

/// `src` as a stock p1 core module and as a p3 component, both run on
/// wasmtime from `cwd` with `envs` added to the inherited environment.
fn wasm_legs(src: &Path, cwd: &Path, scratch: &Path, envs: &[(&str, &str)]) -> (Seen, Seen) {
    let name = src.file_name().expect("name").to_string_lossy().into_owned();
    let mut seen = Vec::new();
    for (leg, p3) in [("p1", false), ("p3", true)] {
        let wasm = scratch.join(format!("{name}.{leg}.wasm"));
        let mut build = Command::new(almide_bin());
        build.args(["build", src.to_str().expect("utf8"), "--target", "wasm", "-o", wasm.to_str().expect("utf8")]);
        if p3 {
            build.arg("--component").env("ALMIDE_COMPONENT_P3", "1");
        } else {
            build.env_remove("ALMIDE_COMPONENT_P3");
        }
        let built = build.output().expect("almide build");
        assert!(built.status.success(), "{name}: the {leg} build failed:\n{}", String::from_utf8_lossy(&built.stderr));
        let mut run = Command::new("wasmtime");
        run.args(["run", "--dir=/", "-S", "inherit-env=y"]).arg(&wasm).current_dir(cwd).env("PWD", cwd).envs(envs.iter().copied());
        seen.push(observe(run, "wasmtime run"));
    }
    let p3 = seen.pop().expect("p3");
    (seen.pop().expect("p1"), p3)
}

/// The overlay over a variable the host DOES set: the inherited value is
/// read first, then a set shadows it — within the instance only.
const ENV_OVER_HOST: &str = r#"import env

effect fn main() -> Unit = {
  println(env.get("ALMIDE_3223_HOST") ?? "unset")
  env.set("ALMIDE_3223_HOST", "guest")
  println(env.get("ALMIDE_3223_HOST") ?? "unset")
  env.set("ALMIDE_3223_HOST", "")
  println("[${env.get("ALMIDE_3223_HOST") ?? "unset"}]")
}
"#;

/// More names and values than the 64 KiB log holds: the defined refusal,
/// the same line and exit code on both stock worlds (native has no log).
const ENV_LOG_FULL: &str = r#"import env

effect fn main() -> Unit = {
  let v = string.repeat("x", 1000)
  for k in list.range(0, 100) {
    env.set("ALMIDE_3223_K${k}", v)
  }
  println("not reached on a stock world")
}
"#;

/// #3223: env.set (op 37) on the p3 component writes the p1 shim's
/// guest-side overlay, and env.get reads it before the `get-environment`
/// snapshot — C-329's set-then-get answers on all three legs alike.
#[test]
fn p3_component_env_set_matches_native_and_p1() {
    if Command::new(almide_bin()).arg("--version").output().is_err() || !wasmtime_runs_p3() {
        return;
    }
    let scratch = tempfile::tempdir().expect("tempdir");
    let cwd = scratch.path().join("cwd");
    std::fs::create_dir_all(&cwd).expect("cwd");
    let over_host = scratch.path().join("env_over_host.almd");
    std::fs::write(&over_host, ENV_OVER_HOST).expect("write");
    let envs = [("ALMIDE_3223_HOST", "host")];
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("spec/wasm_cross");
    for src in [root.join("env_set_overlay.almd"), over_host] {
        let mut native = Command::new(almide_bin());
        native.arg("run").arg(&src).current_dir(&cwd).env("PWD", &cwd).envs(envs);
        let native = observe(native, "almide run");
        let (p1, p3) = wasm_legs(&src, &cwd, scratch.path(), &envs);
        assert_eq!(p1, native, "{}: p1 differs from native", src.display());
        assert_eq!(p3, native, "{}: p3 differs from native", src.display());
    }

    let full = scratch.path().join("env_log_full.almd");
    std::fs::write(&full, ENV_LOG_FULL).expect("write");
    let (p1, p3) = wasm_legs(&full, &cwd, scratch.path(), &[]);
    assert_eq!(p1, (String::new(), "Error: env.set log full (64 KiB of names and values)\n".to_string(), Some(1)), "p1: {p1:?}");
    assert_eq!(p3, p1, "the p3 log refuses where the p1 log does");
}
