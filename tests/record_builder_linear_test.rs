//! Appending to a field of a rebuilt record, or to a fold accumulator, is
//! linear on native (#4260, #4261).
//!
//! `b = add(b, i)` with `fn add(b: Box, i: Int) -> Box = { ...b, m:
//! map.set(b.m, …), n: b.n + 1 }` (#3404, regressed as #4260) and the
//! inline rebuilds `r = { ...r, xs: r.xs + [i], n: r.n + 1 }`, `r = { ...r,
//! s: r.s + "x" }`, `r = { ...r, m: map.set(r.m, i, i) }` copied the whole
//! record — every collection in it — and the rebuilt field on every step:
//! 160 000 list appends took 23.6 s, 80 000 map updates 38 s. The rebuild is
//! the rhs of `r`'s own reassignment, so its base read is `r`'s last use and
//! moves, and the field it rebuilds moves into the append or the `map.set`
//! (`pass_clone_record_fields::rewrite_spread_reassign`). The fold
//! accumulator rows were linear on native already and are pinned so they
//! stay so.
//!
//! The gate is the PROPERTY, not the output (#4260: #3418's fixtures checked
//! what the program printed, so the regression passed CI): each shape runs at
//! N and 4N and the time ratio must stay under 8. In place it is ~4, a copy
//! per step is ~16. A small run already slower than any linear reading fails
//! at once, so a regression does not spend minutes on the large run. The
//! emit cells say which rule went away when a ratio fails.
//!
//! Skips cleanly when the `almide` binary is unavailable (CI builds it in the
//! build step; locally run `cargo build --release` first).

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

fn tool_available() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
}

const PERF: &str = r#"import env

type Box = { m: Map[String, List[Int]], n: Int }

type R = { xs: List[Int], s: String, m: Map[Int, Int], n: Int }

fn add(b: Box, i: Int) -> Box = { ...b, m: map.set(b.m, int.to_string(i), [i]), n: b.n + 1 }

fn call_form(n: Int) -> Int = {
  var b = Box { m: [:], n: 0 }
  var i = 0
  while i < n {
    b = add(b, i)
    i = i + 1
  }
  b.n
}

fn empty() -> R = R { xs: [], s: "", m: [:], n: 0 }

fn list_field(n: Int) -> Int = {
  var r = empty()
  for i in 0..<n {
    r = { ...r, xs: r.xs + [i], n: r.n + 1 }
  }
  list.len(r.xs)
}

fn str_field(n: Int) -> Int = {
  var r = empty()
  for i in 0..<n {
    r = { ...r, s: r.s + "x", n: r.n + 1 }
  }
  string.len(r.s)
}

fn map_field(n: Int) -> Int = {
  var r = empty()
  for i in 0..<n {
    r = { ...r, m: map.set(r.m, i, i), n: r.n + 1 }
  }
  map.len(r.m)
}

fn fold_rec(n: Int) -> Int = list.len(list.fold(list.range(0, n), empty(), (acc, i) => { ...acc, xs: acc.xs + [i] }).xs)

fn fold_str(n: Int) -> Int = string.len(list.fold(list.range(0, n), "", (acc, i) => acc + "x"))

effect fn main() -> Unit = {
  let args = env.args()
  let n = int.parse(list.get(args, 1) ?? "0") ?? 0
  let r = match list.get(args, 0) ?? "" {
    "call" => call_form(n),
    "list" => list_field(n),
    "str" => str_field(n),
    "map" => map_field(n),
    "fold_rec" => fold_rec(n),
    _ => fold_str(n),
  }
  println(int.to_string(r))
}
"#;

fn scratch(tag: &str) -> std::path::PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("almide-record-builder-{tag}-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Best of three wall-clock runs of `bin shape n`, checking the printed count.
fn best_of_three(bin: &Path, shape: &str, n: u64) -> Duration {
    (0..3).map(|_| {
        let t = Instant::now();
        let out = Command::new(bin).args([shape, &n.to_string()]).output().expect("spawn perf binary");
        let dt = t.elapsed();
        assert!(out.status.success(), "{shape} {n} failed:\n{}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), n.to_string(), "{shape} {n} printed the wrong count");
        dt
    }).min().unwrap()
}

#[test]
fn four_times_the_updates_is_not_sixteen_times_the_time() {
    if !tool_available() { return; }
    let dir = scratch("perf");
    let src = dir.join("perf.almd");
    let bin = dir.join("perf");
    std::fs::write(&src, PERF).unwrap();
    let build = Command::new(almide_bin())
        .args(["build", src.to_str().unwrap(), "-o", bin.to_str().unwrap()])
        .output()
        .expect("spawn almide build");
    assert!(build.status.success(), "build failed:\n{}", String::from_utf8_lossy(&build.stderr));
    // Measured on x86-64 (GCE n2): copying, the list row took 43 ms / 955 ms
    // at 10k / 40k and the map row 1.5 s at 20k; in place every row is a few
    // ms at 100k. The sizes are large enough that the work, not the process
    // start, dominates the in-place reading; a small run over 2 s is already
    // no linear reading (the copying list row needs ~9 s at 100k).
    let (n, big) = (100_000u64, 400_000u64);
    for shape in ["call", "list", "str", "map", "fold_rec", "fold_str"] {
        let small = best_of_three(&bin, shape, n);
        assert!(small < Duration::from_secs(2), "{shape}: {n} updates took {small:?} — the update copies the collection");
        let large = best_of_three(&bin, shape, big);
        let ratio = large.as_secs_f64() / small.as_secs_f64().max(1e-4);
        assert!(ratio < 8.0, "{shape}: {big} updates took {large:?} against {small:?} for {n} — ratio {ratio:.1}, quadratic again");
    }
    std::fs::remove_dir_all(&dir).ok();
}
