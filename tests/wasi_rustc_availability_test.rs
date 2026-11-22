//! #4913: `almide build --target wasi` renders the program to Rust and
//! compiles it with rustc for wasm32-wasip1, carrying the native runtime
//! whole. `process.pid` reaches `std::process::id`, which wasm32-wasip1 has
//! no implementation for: the module built, then aborted at run time with
//! `no pids on this platform` (exit 134). The fn's row in
//! proofs/target-availability.toml now declares the `rustc-wasip1` leg, and
//! the route refuses a reachable call at check and build time with E081 and
//! a `try:` line — decided from that declaration, not from the fn's name.
//!
//! Novel: build and check (text and `--json`) with `--target wasi` refuse the
//! program. Controls: the same program checks and builds natively and runs;
//! `check --target wasi` accepts it when the call is unreachable, and accepts
//! a program on the same route that calls only env fns. The measurement:
//! every declared `rustc-wasip1` row, built through the check's escape, still
//! aborts on wasmtime (a row whose fn starts serving fails here as stale), and
//! the env-only control runs — skipped without the wasm32-wasip1 target or
//! wasmtime unless `ALMIDE_EXPECT_TOOLS` is set.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The binary cargo built from this tree, never one on PATH.
fn almide_bin() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

const PID: &str = r#"import process

effect fn main() -> Unit = {
  let p = process.pid()
  println("pid > 0: ${p > 0}")
}
"#;

/// The call sits in a fn nothing reaches.
const PID_UNREACHED: &str = r#"import process

effect fn never_called() -> Int = process.pid()

effect fn main() -> Unit = {
  println("no pid here")
}
"#;

const ENV_ONLY: &str = r#"import env

effect fn main() -> Unit = {
  env.sleep_ms(1)
  println("args ${list.len(env.args())} cwd ${env.cwd() ?? "?"} home ${option.is_some(env.get("ALMIDE_4913_UNSET"))}")
}
"#;

fn dir(tag: &str, src: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("almide-4913-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("p.almd"), src).unwrap();
    d
}

fn almide(d: &Path, args: &[&str], envs: &[(&str, &str)]) -> Output {
    Command::new(almide_bin())
        .args(args)
        .envs(envs.iter().copied())
        .current_dir(d)
        .output()
        .expect("spawn almide")
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

fn assert_refused(o: &Output, what: &str) {
    let t = text(o);
    assert!(!o.status.success(), "{what} must be refused:\n{t}");
    assert!(t.contains("error[E081]: `process.pid` is not available on --target wasi"), "{what}: no E081 for process.pid:\n{t}");
    assert!(t.contains("no process id"), "{what}: the reason is missing:\n{t}");
    assert!(t.contains("try: run it natively"), "{what}: the actionable hint is missing:\n{t}");
}

#[test]
fn a_wasi_build_that_reaches_process_pid_is_refused_before_rustc() {
    let d = dir("build", PID);
    let o = almide(&d, &["build", "p.almd", "--target", "wasi", "-o", "p.wasm"], &[]);
    let wrote = d.join("p.wasm").exists() || d.join("p.rs").exists();
    let _ = std::fs::remove_dir_all(&d);
    assert_refused(&o, "build --target wasi");
    assert!(!wrote, "a refused build must not write the module or its Rust source");
}

#[test]
fn a_wasi_check_that_reaches_process_pid_is_refused() {
    let d = dir("check", PID);
    let plain = almide(&d, &["check", "p.almd", "--target", "wasi"], &[]);
    let json = almide(&d, &["check", "p.almd", "--target", "wasi", "--json"], &[]);
    let native = almide(&d, &["check", "p.almd"], &[]);
    let _ = std::fs::remove_dir_all(&d);
    assert_refused(&plain, "check --target wasi");
    let rows = String::from_utf8_lossy(&json.stdout).to_string();
    assert!(!json.status.success(), "check --json --target wasi must fail:\n{}", text(&json));
    assert!(
        rows.lines().any(|l| l.contains("\"E081\"") && l.contains("process.pid") && l.contains("try: run it natively")),
        "check --json --target wasi: no E081 row with its hint:\n{rows}"
    );
    // Control: the native check judges the same program clean.
    assert!(native.status.success(), "native check must accept it:\n{}", text(&native));
}

#[test]
fn the_wasi_check_ignores_an_unreachable_call_and_env_programs() {
    let unreached = dir("unreached", PID_UNREACHED);
    let o = almide(&unreached, &["check", "p.almd", "--target", "wasi"], &[]);
    let _ = std::fs::remove_dir_all(&unreached);
    assert!(o.status.success(), "an unreachable process.pid must not be refused:\n{}", text(&o));
    let env_only = dir("env", ENV_ONLY);
    let o = almide(&env_only, &["check", "p.almd", "--target", "wasi"], &[]);
    let _ = std::fs::remove_dir_all(&env_only);
    assert!(o.status.success(), "an env-only program must check on --target wasi:\n{}", text(&o));
}

#[test]
fn the_native_build_of_the_refused_program_still_runs() {
    let d = dir("native", PID);
    let b = almide(&d, &["build", "p.almd", "-o", "p"], &[]);
    assert!(b.status.success(), "native build failed:\n{}", text(&b));
    let r = Command::new(d.join("p")).output().expect("run the native build");
    let _ = std::fs::remove_dir_all(&d);
    assert!(r.status.success(), "native run failed:\n{}", text(&r));
    assert_eq!(String::from_utf8_lossy(&r.stdout), "pid > 0: true\n");
}

fn wasi_tools() -> bool {
    let wasip1 = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).lines().any(|l| l.trim() == "wasm32-wasip1"));
    let wasmtime = Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success());
    if !(wasip1 && wasmtime) && std::env::var_os("ALMIDE_EXPECT_TOOLS").is_some() {
        panic!("ALMIDE_EXPECT_TOOLS is set but the wasm32-wasip1 target ({wasip1}) or wasmtime ({wasmtime}) is missing");
    }
    wasip1 && wasmtime
}

/// The fns whose row declares the `rustc-wasip1` leg.
fn declared_rows() -> Vec<String> {
    let toml = std::fs::read_to_string(format!("{}/proofs/target-availability.toml", env!("CARGO_MANIFEST_DIR"))).unwrap();
    toml.split("\n[[unavailable]]\n")
        .skip(1)
        .filter(|b| b.lines().any(|l| l.starts_with("legs = [") && l.contains("\"rustc-wasip1\"")))
        .filter_map(|b| b.lines().find_map(|l| l.strip_prefix("fn = \"")?.strip_suffix('"').map(str::to_string)))
        .collect()
}

/// Build `src` for wasm32-wasip1 through the availability escape and run it.
fn build_and_run_wasi(tag: &str, src: &str) -> (Output, Output) {
    let d = dir(tag, src);
    let b = almide(&d, &["build", "p.almd", "--target", "wasi", "-o", "p.wasm"], &[("ALMIDE_NO_AVAIL_CHECK", "1")]);
    assert!(b.status.success(), "{tag}: the wasi build through the escape failed:\n{}", text(&b));
    let r = Command::new("wasmtime").arg("p.wasm").current_dir(&d).output().expect("spawn wasmtime");
    let _ = std::fs::remove_dir_all(&d);
    (b, r)
}

#[test]
fn every_declared_rustc_wasip1_row_still_aborts_and_env_runs() {
    let rows = declared_rows();
    assert!(rows.contains(&"process.pid".to_string()), "process.pid lost its rustc-wasip1 leg: {rows:?}");
    if !wasi_tools() {
        return;
    }
    for f in &rows {
        // A new row brings its probe program here.
        let src = match f.as_str() {
            "process.pid" => PID,
            other => panic!("{other} declares the rustc-wasip1 leg but this test has no probe program for it"),
        };
        let (_, r) = build_and_run_wasi("row", src);
        assert!(
            !r.status.success(),
            "{f} runs on wasm32-wasip1 now: its rustc-wasip1 leg is stale, remove it\n{}",
            text(&r)
        );
    }
    let (_, r) = build_and_run_wasi("env-run", ENV_ONLY);
    assert!(r.status.success(), "the env-only control must run on wasmtime:\n{}", text(&r));
    assert!(String::from_utf8_lossy(&r.stdout).starts_with("args 0 cwd "), "{}", text(&r));
}
