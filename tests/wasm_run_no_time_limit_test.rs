//! #2615: `almide run` has no time limit on either target. The embedded
//! wasm host used to arm the in-process TEST runner's 30 s epoch watchdog
//! on the product path too, so a program native ran to completion in 36 s
//! died on wasm with `Error: wasm trap: interrupt` (and every loop header
//! paid the epoch check). The watchdog is harness equipment now: only the
//! in-process test runner arms it, and `ALMIDE_WASM_WATCHDOG_SECS` sets its
//! period.
//!
//! The test keeps its runtime short by lowering that period to 1 s on the
//! `almide run` child: a product run that arms the harness watchdog again —
//! the only way a watchdog is armed — traps on this ~4 s program; a product
//! run with no limit prints what native prints. The elapsed-time floor is
//! the control: it proves the run really outlived the 1 s the knob names,
//! so the pass is not a fast program slipping under the watchdog.

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

/// ~4 s on either leg (Apple M-series, release build): a tail-recursive
/// loop the structural wasm leg renders, with a result that depends on
/// every step so no leg can skip the work.
const SRC: &str = "fn spin(i: Int, n: Int, acc: Int) -> Int =
  if i >= n then acc else spin(i + 1, n, (acc * 31 + i) % 1000003)

effect fn main() -> Unit = {
  println(int.to_string(spin(0, 1000000000, 7)))
}
";

fn run(path: &Path, wasm: bool) -> (i32, String, String, Duration) {
    let mut cmd = Command::new(almide_bin());
    cmd.arg("run").arg(path);
    if wasm {
        cmd.args(["--target", "wasm"]).env("ALMIDE_WASM_WATCHDOG_SECS", "1");
    }
    let started = Instant::now();
    let out = cmd.output().expect("spawn almide run");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
        started.elapsed(),
    )
}

#[test]
fn wasm_run_outlives_the_harness_watchdog_and_matches_native() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("spin.almd");
    std::fs::write(&path, SRC).unwrap();

    let (ncode, nout, nerr, _) = run(&path, false);
    assert_eq!(ncode, 0, "native run failed: {nerr}");

    let (wcode, wout, werr, took) = run(&path, true);
    assert!(
        !werr.contains("interrupt"),
        "`almide run --target wasm` armed a watchdog (#2615): {werr}"
    );
    assert_eq!(wcode, 0, "wasm run failed: {werr}");
    assert_eq!(wout, nout, "wasm stdout diverged from native");
    assert!(
        took >= Duration::from_millis(1500),
        "control: the wasm run took {took:?}, under the 1 s watchdog period plus margin — \
         the program no longer outlives the knob, so this test proves nothing; lengthen SRC"
    );
}
