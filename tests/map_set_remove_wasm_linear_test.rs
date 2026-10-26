//! #4208 — the FUNCTIONAL `map.remove` / `set.remove` is amortized O(1) on
//! both legs, and every observable of a map or set it shrank is what it was.
//!
//! #3691 made the in-place map delete of a local var leave a hole on wasm,
//! but the functional remove over a receiver whose credit the op holds — the
//! fold callback `(m, i) => map.remove(m, i)` and the rebind
//! `m = map.remove(m, k)` — still shifted the later entries and retired the
//! block's index, so the next lookup scanned and the one after rebuilt it:
//! O(n) per call, 2.3 s for 100k removals where native took 1 ms. Never a
//! regression: that lane shifted from its first commit (7b38f6556, #3640),
//! and #3691's gate timed only the in-place delete. Now a remove in TAIL position of
//! its var's rebind or fold step (map_remove.rs `tail_removes_of`) holes the
//! entry like the delete does, the var settles on every other read, and the
//! fold's result settles once at the end.
//!
//! The semantics cell pins what must not move (EXPECTED is the native leg's
//! output, which the pre-fix wasm leg matched byte for byte): the original
//! a fold started from, removes under `if`, reads inside the fold body,
//! rebinds interleaved with `set` / `get` / `len` / iteration, Int and String
//! sets, removing everything, released heap values. The scaling cell times
//! only the remove loop at n and 4n: a linear remove takes ~4× per op, a
//! constant one ~1×; the bound 2.5 sits between them. Release-only (CI's
//! release-only root step names this file); every run is capped.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const SEMANTICS: &str = r#"fn digest(m: Map[Int, Int]) -> Int = map.fold(m, 0, (acc, k, v) => (acc * 31 + k * 7 + v) % 1000000007)

fn sdig(s: Set[String]) -> String = set.to_list(s) |> list.take(5) |> list.join(",")

effect fn main() -> Unit = {
  let m0: Map[Int, Int] = list.range(0, 100) |> list.map((i) => (i, i * 2)) |> map.from_list
  let m1 = list.range(0, 50) |> list.fold(m0, (m, i) => map.remove(m, i * 2))
  println("${map.len(m1)} ${map.len(m0)} ${map.keys(m1) |> list.take(5)} ${digest(m1)} ${digest(m0)}")
  let m2 = list.range(0, 300) |> list.fold(list.range(0, 300) |> list.map((i) => (i, i)) |> map.from_list, (m, i) =>
    if i % 3 == 0 then map.remove(m, i) else if map.contains(m, i + 1) and map.len(m) > 10 then map.remove(m, i + 1) else m)
  println("${map.len(m2)} ${map.get(m2, 4)} ${map.get(m2, 5)} ${map.keys(m2) |> list.take(6)} ${digest(m2)}")
  var m3: Map[String, Int] = list.range(0, 200) |> list.map((i) => ("k${i}", i)) |> map.from_list
  for i in 0..<150 {
    m3 = map.remove(m3, "k${i}")
    if i % 10 == 0 then m3 = map.set(m3, "n${i}", i) else ()
    if i % 37 == 0 then println("${i} ${map.len(m3)} ${map.get(m3, "k${i + 1}")} ${map.contains(m3, "k${i}")} ${map.keys(m3) |> list.take(3)}") else ()
  }
  println("${map.len(m3)} ${map.to_list(m3) |> list.take(4)} ${map.values(m3) |> list.drop(60)}")
  var total = 0
  for (k, v) in m3 {
    total = total + v + string.len(k)
  }
  println("${total} ${m3 == (map.to_list(m3) |> map.from_list)}")
  let s0: Set[Int] = list.range(0, 64) |> set.from_list
  let s1 = list.range(0, 64) |> list.fold(s0, (s, i) => if i % 4 == 0 then s else set.remove(s, i))
  println("${set.len(s1)} ${set.to_list(s1)} ${set.contains(s1, 8)} ${set.contains(s1, 9)} ${set.len(s0)} ${s1}")
  var ss: Set[String] = list.range(0, 100) |> list.map((i) => "s${i}") |> set.from_list
  for i in 0..<90 {
    ss = set.remove(ss, "s${i}")
    if set.contains(ss, "s${i + 1}") and i % 30 == 0 then println("${i} ${set.len(ss)} ${set.is_empty(ss)} ${sdig(ss)}") else ()
  }
  ss = set.insert(ss, "z")
  println("${set.len(ss)} ${ss} ${set.is_empty(ss)}")
  let e = list.range(0, 40) |> list.fold(list.range(0, 40) |> set.from_list, (s, i) => set.remove(s, i))
  println("${set.len(e)} ${set.is_empty(e)} ${e}")
  let em = list.range(0, 40) |> list.fold(list.range(0, 40) |> list.map((i) => (i, "v${i}")) |> map.from_list, (m, i) => map.remove(m, 39 - i))
  println("${map.len(em)} ${map.is_empty(em)} ${em}")
  let lv = list.range(0, 50) |> list.fold(list.range(0, 50) |> list.map((i) => ("L${i}", [i, i])) |> map.from_list, (m, i) => map.remove(m, "L${i * 2}"))
  println("${map.len(lv)} ${map.to_list(lv) |> list.take(3)} ${map.get(lv, "L49")}")
}
"#;

const EXPECTED: &str = r#"50 100 [1, 3, 5, 7, 9] 636352595 609086271
100 some(4) none [1, 4, 7, 10, 13, 16] 866531297
0 200 some(1) false ["k1", "k2", "k3"]
37 166 some(38) false ["k38", "k39", "k40"]
74 133 some(75) false ["k75", "k76", "k77"]
111 100 some(112) false ["k112", "k113", "k114"]
148 66 some(149) false ["k149", "k150", "k151"]
65 [("k150", 150), ("k151", 151), ("k152", 152), ("k153", 153)] [100, 110, 120, 130, 140]
10024 true
16 [0, 4, 8, 12, 16, 20, 24, 28, 32, 36, 40, 44, 48, 52, 56, 60] true false 64 set.from_list([0, 4, 8, 12, 16, 20, 24, 28, 32, 36, 40, 44, 48, 52, 56, 60])
0 99 false s1,s2,s3,s4,s5
30 69 false s31,s32,s33,s34,s35
60 39 false s61,s62,s63,s64,s65
11 set.from_list(["s90", "s91", "s92", "s93", "s94", "s95", "s96", "s97", "s98", "s99", "z"]) false
0 true set.from_list([])
0 true [:]
25 [("L1", [1, 1]), ("L3", [3, 3]), ("L5", [5, 5])] some([49, 49])
"#;

/// Build an n-entry map or set, then time removing every key: `mode` picks
/// the fold over a map, the fold over a set, the map rebind loop or the set
/// rebind loop. Keys go oldest-first (the shift's worst case) for the folds
/// and newest-first (no shift, only the index retirement) for the rebinds.
const SCALING: &str = r#"import env

effect fn run(mode: Int, n: Int) -> Unit = {
  let keys = list.range(0, n)
  let m0: Map[Int, Int] = keys |> list.map((i) => (i, i)) |> map.from_list
  let s0: Set[Int] = keys |> set.from_list
  let t0 = datetime.monotonic_ns()!
  let left = match mode {
    0 => map.len(keys |> list.fold(m0, (m, i) => map.remove(m, i))),
    1 => set.len(keys |> list.fold(s0, (s, i) => set.remove(s, i))),
    2 => {
      var m = m0
      for i in 0..<n {
        m = map.remove(m, n - 1 - i)
      }
      map.len(m)
    },
    _ => {
      var s = s0
      for i in 0..<n {
        s = set.remove(s, n - 1 - i)
      }
      set.len(s)
    },
  }
  let t1 = datetime.monotonic_ns()!
  println("${left} ${t1 - t0}")
}

effect fn main() -> Unit = {
  let args = env.args()
  let mode = int.parse(list.get(args, 0) ?? "0") ?? 0
  let n = int.parse(list.get(args, 1) ?? "0") ?? 0
  run(mode, n)!
}
"#;

const MODES: [&str; 4] = ["map.remove in a fold", "set.remove in a fold", "m = map.remove(m, k)", "s = set.remove(s, x)"];

/// Wall-clock cap on one subprocess (a compile included).
const PER_RUN_LIMIT: Duration = Duration::from_secs(180);

/// `almide run [--target wasm] --release <file> -- <args>` under the cap.
fn run_capped(file: &Path, wasm: bool, args: &[&str]) -> (i32, String, String) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg("--release").arg(file);
    if wasm {
        c.args(["--target", "wasm"]);
    }
    if !args.is_empty() {
        c.arg("--").args(args);
    }
    let mut child = c.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().expect("spawn almide run");
    let (mut so, mut se) = (child.stdout.take().unwrap(), child.stderr.take().unwrap());
    let to = std::thread::spawn(move || {
        let mut s = String::new();
        so.read_to_string(&mut s).ok();
        s
    });
    let te = std::thread::spawn(move || {
        let mut s = String::new();
        se.read_to_string(&mut s).ok();
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
    let leg = if wasm { "wasm" } else { "native" };
    let Some(status) = status else {
        panic!("{leg} {args:?}: still running after {PER_RUN_LIMIT:?} — killed (a linear remove again?)");
    };
    (status.code().unwrap_or(-1), out, err)
}

fn write(dir: &tempfile::TempDir, name: &str, src: &str) -> std::path::PathBuf {
    let file = dir.path().join(name);
    std::fs::write(&file, src).expect("write source");
    file
}

#[test]
fn removes_keep_every_observable_on_both_legs() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = write(&dir, "remove_semantics.almd", SEMANTICS);
    for wasm in [false, true] {
        let (code, out, err) = run_capped(&file, wasm, &[]);
        assert_eq!(code, 0, "wasm={wasm}: exit {code}\n{err}");
        assert_eq!(out, EXPECTED, "wasm={wasm}: the output moved");
    }
}

/// Nanoseconds per remove at size `n`: the best of three runs.
fn ns_per_remove(file: &Path, wasm: bool, mode: usize, n: u64) -> f64 {
    let (m_s, n_s) = (mode.to_string(), n.to_string());
    (0..3)
        .map(|_| {
            let (code, out, err) = run_capped(file, wasm, &[&m_s, &n_s]);
            assert_eq!(code, 0, "wasm={wasm} {} n={n}: exit {code}\n{err}", MODES[mode]);
            let fields: Vec<&str> = out.split_whitespace().collect();
            assert_eq!(fields.len(), 2, "wasm={wasm} {} n={n}: unexpected output {out:?}", MODES[mode]);
            assert_eq!(fields[0], "0", "wasm={wasm} {} n={n}: entries survived the removal", MODES[mode]);
            fields[1].parse::<f64>().expect("elapsed ns") / n as f64
        })
        .fold(f64::INFINITY, f64::min)
}

#[test]
#[cfg_attr(debug_assertions, ignore = "times the remove loop: runs in CI's release-only root step")]
fn four_times_the_collection_is_not_four_times_the_remove() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = write(&dir, "remove_scaling.almd", SCALING);
    let n = 25_000u64;
    let mut failures = Vec::new();
    for wasm in [false, true] {
        for mode in 0..MODES.len() {
            let small = ns_per_remove(&file, wasm, mode, n);
            let large = ns_per_remove(&file, wasm, mode, 4 * n);
            let ratio = large / small.max(1.0);
            eprintln!("wasm={wasm} {}: {small:.0} ns at {n}, {large:.0} ns at {} — ratio {ratio:.2}", MODES[mode], 4 * n);
            if ratio >= 2.5 {
                failures.push(format!(
                    "wasm={wasm} {}: a remove took {large:.0} ns at {} entries against {small:.0} ns at {n} — ratio {ratio:.1}, linear in the size again",
                    MODES[mode],
                    4 * n
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
