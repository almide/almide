//! #3691 — `map.delete` is amortized O(1) on both legs, and every observable
//! of a map that has had entries deleted is what it was.
//!
//! Before the fix a delete was linear in the map's size on both legs: native's
//! `AlmideMap::remove` shifted the entry vector and rebuilt the whole hash
//! index, and the wasm leg shifted the entry block and dropped the block's
//! index, so the next lookup scanned and the one after rebuilt it. A delete of
//! the oldest key followed by an append took 33 µs at 20k entries and 313 µs
//! at 80k natively (`almide run --release`), 36 µs and 147 µs on wasm.
//!
//! The fix leaves a hole where the entry was (insertion order is observable,
//! C-033, so the survivors cannot be reordered) and compacts once the holes
//! outnumber the live entries. The semantics cell pins what must not move:
//! iteration order, `len` / `keys` / `values` / `entries` / `fold` / display /
//! equality after deletes, a deleted key re-inserted at the END, an overwrite
//! keeping its place, a missing key, deleting everything, enough churn to
//! compact several times, the functional `map.remove` (rebind, tail call,
//! fold), `set.remove`, and the aliases, holders and closures bound before a
//! delete keeping the map they saw (C-033). EXPECTED is the pre-fix
//! compiler's output; the two legs must match it byte for byte.
//!
//! The scaling cell times only the delete loop (the program reads the
//! monotonic clock around it) at n and 4n entries: a linear delete takes about
//! 4× per op, a constant one about 1×; the bound 2.5 sits between them.
//!
//! Every subprocess runs under a wall-clock cap, so a regression to the linear
//! delete fails instead of hanging the suite. The scaling cell is release-only
//! (a debug compiler's embedded wasm engine would time itself, not the map);
//! CI's release-only root step names this file.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const SEMANTICS: &str = r#"type Holder = { snap: Map[String, Int], tag: Int }

fn show(m: Map[String, Int]) -> String = map.entries(m) |> list.map(((k, v)) => "${k}=${v}") |> list.join(",")

fn showi(m: Map[Int, String]) -> String = map.fold(m, "", (acc, k, v) => acc + "${k}:${v};")

fn digest(m: Map[Int, Int]) -> Int = map.fold(m, 0, (acc, k, v) => (acc * 31 + k * 7 + v) % 1000000007)

fn drop_evens(m: Map[Int, Int], i: Int, n: Int) -> Map[Int, Int] = if i >= n then m
else drop_evens(map.remove(m, i * 2), i + 1, n)

fn trimmed(n: Int) -> Map[Int, Int] = {
  var x: Map[Int, Int] = list.range(0, n) |> list.fold(map.new(), (a, i) => map.set(a, i, i))
  for i in 0..<n {
    if i % 3 == 0 then map.delete(x, i) else ()
  }
  x
}

fn bump(mut m: Map[Int, Int]) -> Unit = {
  map.delete(m, 1)
  map.insert(m, 1, 99)
}

effect fn main() -> Unit = {
  // String keys past the index threshold: delete then iterate every way
  var m: Map[String, Int] = list.range(0, 40) |> list.fold(map.new(), (acc, i) => map.set(acc, "k${i}", i))
  let alias = m
  let h = { snap: m, tag: 7 }
  map.delete(m, "k3")
  map.delete(m, "k17")
  map.delete(m, "nope")
  map.delete(m, "k39")
  println("${map.len(m)} ${map.is_empty(m)} ${map.get(m, "k3")} ${map.get(m, "k4")} ${map.contains(m, "k17")} ${map.get_or(m, "k39", -1)}")
  println(show(m))
  println("${map.keys(m) |> list.take(5)} ${map.values(m) |> list.drop(33)}")
  var acc = ""
  for (k, v) in m {
    if v % 9 == 0 then acc = acc + k + "," else ()
  }
  println(acc)
  // an alias and a holder bound before the deletes keep the old map (C-033)
  println("${map.len(alias)} ${map.get(alias, "k3")} ${map.len(h.snap)} ${map.get(h.snap, "k39")} ${h.tag}")
  // delete then re-insert appends at the end; an overwrite keeps position
  map.delete(m, "k0")
  m = map.set(m, "k0", 100)
  m = map.set(m, "k1", 101)
  map.insert(m, "k3", 103)
  println(show(m) |> string.split(",") |> list.drop(34) |> list.join(","))
  println("${map.keys(m) |> list.take(3)} ${map.len(m)}")
  // equality against a map built fresh, either order of construction
  let fresh = map.entries(m) |> list.reverse |> map.from_list
  println("${m == fresh} ${m == alias} ${map.len(fresh)}")
  // display through interpolation after deletes
  var small: Map[String, Int] = map.from_list([("a", 1), ("b", 2), ("c", 3)])
  map.delete(small, "b")
  println("${small}")
  // delete everything, then rebuild
  var all: Map[Int, String] = list.range(0, 50) |> list.fold(map.new(), (acc2, i) => map.set(acc2, i, "v${i}"))
  for i in 0..<50 {
    map.delete(all, (i * 7) % 50)
  }
  println("${map.len(all)} ${map.is_empty(all)} ${all} ${map.keys(all)}")
  map.insert(all, 5, "five")
  map.insert(all, 2, "two")
  println("${showi(all)} ${map.len(all)}")
  // many deletes and re-inserts: the compaction path, interleaved with reads
  var big: Map[Int, Int] = list.range(0, 3000) |> list.fold(map.new(), (acc3, i) => map.set(acc3, i, i * 3))
  let before = big
  var j = 0
  var hits = 0
  while j < 7700 {
    let k = (j * 7919) % 3000
    if map.contains(big, k) then {
      map.delete(big, k)
      hits = hits + 1
    } else {
      big = map.set(big, k, j)
    }
    if j % 1000 == 0 then println("${j} ${map.len(big)} ${map.get(big, k)} ${digest(big)}") else ()
    j = j + 1
  }
  println("${hits} ${map.len(big)} ${digest(big)} ${map.keys(big) |> list.take(6)} ${map.len(before)} ${digest(before)}")
  let rebuilt = map.entries(big) |> map.from_list
  println("${rebuilt == big} ${digest(rebuilt) == digest(big)} ${big == before}")
  // FIFO: delete the oldest, append a new one
  var q: Map[Int, Int] = list.range(0, 100) |> list.fold(map.new(), (acc4, i) => map.set(acc4, i, i))
  for i in 0..<500 {
    map.delete(q, i)
    q = map.set(q, 100 + i, i)
  }
  println("${map.len(q)} ${map.keys(q) |> list.take(4)} ${digest(q)} ${map.get(q, 399)} ${map.get(q, 400)}")
  // the functional remove: a rebind, a tail call, a fold, and a kept source
  var r: Map[Int, Int] = list.range(0, 60) |> list.fold(map.new(), (acc5, i) => map.set(acc5, i, i))
  let r0 = r
  for i in 0..<30 {
    r = map.remove(r, i * 2 + 1)
  }
  let t = drop_evens(r0, 0, 30)
  let f = list.range(0, 20) |> list.fold(r0, (acc6, i) => map.remove(acc6, i))
  println("${map.len(r)} ${map.keys(r) |> list.take(4)} ${map.len(t)} ${map.keys(t) |> list.take(4)} ${map.len(f)} ${map.len(r0)} ${digest(r0)}")
  // a map passed on after deletes, and a closure reading a kept snapshot
  var p: Map[Int, Int] = list.range(0, 30) |> list.fold(map.new(), (acc7, i) => map.set(acc7, i, i))
  let snap = p
  let peek = () => map.len(snap)
  map.delete(p, 4)
  map.delete(p, 5)
  println("${digest(p)} ${peek()} ${map.len(p)} ${map.keys(p) |> list.take(6)}")
  // sets: remove keeps the survivors' order
  var s: Set[Int] = list.range(0, 40) |> set.from_list
  let s0 = s
  for i in 0..<20 {
    s = set.remove(s, i * 2)
  }
  s = set.insert(s, 0)
  s = set.remove(s, 999)
  println("${set.len(s)} ${set.to_list(s) |> list.take(5)} ${set.to_list(s) |> list.drop(18)} ${set.len(s0)} ${set.contains(s, 2)} ${set.contains(s, 0)}")
  var ss: Set[String] = list.range(0, 30) |> list.map((i) => "s${i}") |> set.from_list
  for i in 0..<30 {
    if i % 3 != 0 then ss = set.remove(ss, "s${i}") else ()
  }
  println("${ss} ${set.len(ss)} ${ss == (["s27", "s0", "s3", "s6", "s9", "s12", "s15", "s18", "s21", "s24"] |> set.from_list)}")
  // a map returned after in-place deletes, a mut param, holders taken after deletes
  var tm = trimmed(40)
  bump(tm)
  let hl = [tm, tm]
  map.delete(tm, 2)
  println("${map.len(tm)} ${map.keys(tm) |> list.take(4)} ${map.keys(tm) |> list.drop(24)} ${hl |> list.map((x) => map.len(x))} ${digest(tm)}")
  // handle values released by deletes and by compaction
  var lv: Map[String, List[Int]] = list.range(0, 64) |> list.fold(map.new(), (a, i) => map.set(a, "L${i}", [i, i + 1]))
  for i in 0..<60 {
    map.delete(lv, "L${i}")
    map.insert(lv, "N${i}", [i])
  }
  println("${map.len(lv)} ${map.entries(lv) |> list.take(6)} ${map.get(lv, "N59")} ${map.get(lv, "L1")}")
}
"#;

const EXPECTED: &str = r#"37 false none some(4) false -1
k0=0,k1=1,k2=2,k4=4,k5=5,k6=6,k7=7,k8=8,k9=9,k10=10,k11=11,k12=12,k13=13,k14=14,k15=15,k16=16,k18=18,k19=19,k20=20,k21=21,k22=22,k23=23,k24=24,k25=25,k26=26,k27=27,k28=28,k29=29,k30=30,k31=31,k32=32,k33=33,k34=34,k35=35,k36=36,k37=37,k38=38
["k0", "k1", "k2", "k4", "k5"] [35, 36, 37, 38]
k0,k9,k18,k27,k36,
40 some(3) 40 some(39) 7
k37=37,k38=38,k0=100,k3=103
["k1", "k2", "k4"] 38
true false 38
["a": 1, "c": 3]
0 true [:] []
5:five;2:two; 2
0 2999 none 459863973
1000 1999 none 660472576
2000 999 none 304144935
3000 1 some(3000) 3000
4000 1001 some(4000) 363245917
5000 2001 some(5000) 367803602
6000 2999 none 929943710
7000 1999 none 509229760
4700 1300 186955850 [1300, 219, 2138, 1057, 2976, 1895] 3000 459863973
true true false
100 [500, 501, 502, 503] 329934257 none none
30 [0, 2, 4, 6] 30 [1, 3, 5, 7] 40 60 884247106
489865884 30 28 [0, 1, 2, 3, 6, 7]
21 [1, 3, 5, 7, 9] [37, 39, 0] 40 false true
set.from_list(["s0", "s3", "s6", "s9", "s12", "s15", "s18", "s21", "s24", "s27"]) 10 true
25 [4, 5, 7, 8] [1] [26, 26] 965691569
64 [("L60", [60, 61]), ("L61", [61, 62]), ("L62", [62, 63]), ("L63", [63, 64]), ("N0", [0]), ("N1", [1])] some([59]) none
"#;

/// Build an n-entry map, then time `d` rounds of "delete the oldest key,
/// append a new one" — the delete has to cross the whole map's order.
const SCALING: &str = r#"import env

effect fn churn(n: Int, d: Int) -> Unit = {
  var m: Map[Int, Int] = map.new()
  for i in 0..<n {
    map.insert(m, i, i)
  }
  let t0 = datetime.monotonic_ns()!
  for i in 0..<d {
    map.delete(m, i)
    m = map.set(m, n + i, i)
  }
  let t1 = datetime.monotonic_ns()!
  println("${map.len(m)} ${map.get(m, n + d - 1)} ${t1 - t0}")
}

effect fn main() -> Unit = {
  let args = env.args()
  let n = int.parse(list.get(args, 0) ?? "0") ?? 0
  let d = int.parse(list.get(args, 1) ?? "0") ?? 0
  churn(n, d)!
}
"#;

/// Wall-clock cap on one subprocess (a compile included).
const PER_RUN_LIMIT: Duration = Duration::from_secs(180);

/// `almide run [--target wasm] --release <file> -- <args>` under the cap:
/// (exit code, stdout, stderr). A run past the cap is killed and fails.
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
        panic!("{leg} {args:?}: still running after {PER_RUN_LIMIT:?} — killed (a linear delete again?)");
    };
    (status.code().unwrap_or(-1), out, err)
}

fn write(dir: &tempfile::TempDir, name: &str, src: &str) -> std::path::PathBuf {
    let file = dir.path().join(name);
    std::fs::write(&file, src).expect("write source");
    file
}

#[test]
fn deletes_keep_every_observable_on_both_legs() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = write(&dir, "map_delete_semantics.almd", SEMANTICS);
    for wasm in [false, true] {
        let (code, out, err) = run_capped(&file, wasm, &[]);
        assert_eq!(code, 0, "wasm={wasm}: exit {code}\n{err}");
        assert_eq!(out, EXPECTED, "wasm={wasm}: the output moved");
    }
}

/// Nanoseconds per delete round at map size `n`: the best of three runs of
/// the program's own clock around the loop.
fn ns_per_round(file: &Path, wasm: bool, n: u64, d: u64) -> f64 {
    let (n_s, d_s) = (n.to_string(), d.to_string());
    (0..3)
        .map(|_| {
            let (code, out, err) = run_capped(file, wasm, &[&n_s, &d_s]);
            assert_eq!(code, 0, "wasm={wasm} n={n}: exit {code}\n{err}");
            let fields: Vec<&str> = out.split_whitespace().collect();
            assert_eq!(fields.len(), 3, "wasm={wasm} n={n}: unexpected output {out:?}");
            assert_eq!(fields[0], n_s, "wasm={wasm} n={n}: the map lost or gained entries");
            assert_eq!(fields[1], format!("some({})", d - 1), "wasm={wasm} n={n}: the last append is missing");
            fields[2].parse::<f64>().expect("elapsed ns") / d as f64
        })
        .fold(f64::INFINITY, f64::min)
}

#[test]
#[cfg_attr(debug_assertions, ignore = "times the delete loop: runs in CI's release-only root step")]
fn four_times_the_map_is_not_four_times_the_delete() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = write(&dir, "map_delete_scaling.almd", SCALING);
    let (n, d) = (25_000u64, 100_000u64);
    for wasm in [false, true] {
        let small = ns_per_round(&file, wasm, n, d);
        let large = ns_per_round(&file, wasm, 4 * n, d);
        let ratio = large / small.max(1.0);
        assert!(
            ratio < 2.5,
            "wasm={wasm}: a delete round took {large:.0} ns at {} entries against {small:.0} ns at {n} — ratio {ratio:.1}, linear in the map's size again",
            4 * n
        );
    }
}
