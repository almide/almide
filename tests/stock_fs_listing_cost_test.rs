//! The stock p1 fs service (`crates/almide-wasi/src/fs_service.wat`) sorts
//! every `fs.list_dir` / `fs.walk` / `fs.glob` answer (C-272: bytewise, as
//! native's `sort()`). It sorted by INSERTION, so one listing of a directory
//! of n entries in the filesystem's hash order cost Θ(n²) comparisons:
//! 4000 entries took 0.13 s on wasmtime and 16000 took 1.8 s. Native and the
//! embedded host sort in O(n log n).
//!
//! The cost is pinned by wasmtime's FUEL (one unit per wasm instruction), not
//! by a clock: each listing runs under a fixed budget that an O(n log n)
//! sort meets with room to spare and a quadratic one exceeds several times
//! over, so the verdict does not move with the machine's load. The answer is
//! also differed against native, so the faster sort is the same sort.
use std::path::{Path, PathBuf};
use std::process::Command;

/// Entries in the probe directory: large enough that n² dominates startup.
const ENTRIES: usize = 6000;

/// The instruction budget of ONE listing program (startup included). An
/// O(n log n) sort of 6000 names spends a few million; insertion sort over
/// the same names in hash order spends hundreds of millions.
const FUEL: u64 = 150_000_000;

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

/// A fresh directory holding `d/` with ENTRIES one-byte files, plus one
/// nested file so `walk` descends.
fn probe_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-stock-listing-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("d/sub")).expect("mkdir");
    for i in 0..ENTRIES {
        std::fs::write(dir.join(format!("d/f{}", 100_000 + i)), "a").expect("write");
    }
    std::fs::write(dir.join("d/sub/inner"), "a").expect("write");
    dir
}

fn program(call: &str) -> String {
    format!(
        "import fs\n\neffect fn main() -> Unit = {{\n  let xs = {call}!\n  println(\"${{list.len(xs)}}\")\n  println(xs |> list.join(\",\"))\n}}\n"
    )
}

fn native(dir: &Path) -> String {
    let o = Command::new(almide_bin()).args(["run", "probe.almd"]).current_dir(dir).output().expect("spawn almide");
    assert!(o.status.success(), "native run failed:\n{}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).expect("utf8")
}

/// The shipped artifact under a stock wasmtime with a fuel budget.
fn stock_fueled(dir: &Path) -> String {
    let o = Command::new(almide_bin())
        .args(["build", "probe.almd", "--target", "wasm", "-o", "probe.wasm"])
        .current_dir(dir)
        .output()
        .expect("spawn almide");
    assert!(o.status.success(), "stock build failed:\n{}", String::from_utf8_lossy(&o.stderr));
    let o = Command::new("wasmtime")
        .args(["run", "--dir=.", "-W"])
        .arg(format!("fuel={FUEL}"))
        .arg("probe.wasm")
        .current_dir(dir)
        .output()
        .expect("spawn wasmtime");
    assert!(
        o.status.success(),
        "the stock listing did not finish within {FUEL} instructions for {ENTRIES} entries \
         (a quadratic sort?):\n{}",
        String::from_utf8_lossy(&o.stderr)
    );
    String::from_utf8(o.stdout).expect("utf8")
}

fn check(tag: &str, call: &str) {
    if !has_wasmtime() {
        return;
    }
    let dir = probe_dir(tag);
    std::fs::write(dir.join("probe.almd"), program(call)).expect("write");
    let want = native(&dir);
    assert!(want.lines().next().is_some_and(|n| n.parse::<usize>().unwrap() >= ENTRIES), "native: {want:.80}");
    let got = stock_fueled(&dir);
    assert!(got == want, "stock {call} differs from native (first 200 bytes):\n{:.200}\nvs\n{:.200}", got, want);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn stock_list_dir_sorts_in_n_log_n() {
    check("list_dir", "fs.list_dir(\"d\")");
}

#[test]
fn stock_walk_sorts_in_n_log_n() {
    check("walk", "fs.walk(\"d\")");
}

#[test]
fn stock_glob_sorts_in_n_log_n() {
    check("glob", "fs.glob(\"d/*\")");
}
