//! #4661: copies and moves the certified native render made that the
//! program never asked for, pinned by what stdout cannot see.
//!
//! - A fold's accumulator moves through its callback where the step hands it
//!   to a combinator only its runtime body serves (`native_move_params`): a
//!   `step(acc: Map, line)` under `list.fold` / `fs.fold_lines` copied the
//!   whole map per element.
//! - The halves of `string.split_once` / `strip_prefix` / `strip_suffix` over
//!   a borrowed string parameter are slices of it
//!   (`render_native_slices`): each call built its halves as owned blocks.
//! - A chain of propagations returns from each arm
//!   (`render_native_tail_return`).
//! - A buffer cleared in its slot is refilled, not built afresh
//!   (`render_native_inplace`): a line buffer cleared per line was freed and
//!   regrown per line.
//!
//! Each program prints a fixed text on the native render (with the ownership
//! certificate required, and rendered by the certified render itself) and on
//! wasm — callbacks that print or fail keep wasm's order — and the
//! allocation lane (`ALMIDE_ALLOC_COUNT`) bounds the blocks the program's own
//! work allocates. No wall-clock assertion.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-callback-moves-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// (stdout, stderr) of `almide run <src>` on `target`, the certificate required.
fn run(src: &Path, target: Option<&str>, alloc: bool) -> (String, String) {
    let mut cmd = Command::new(almide());
    cmd.arg("run").arg(src).env("ALMIDE_CERTIFY_OWNERSHIP", "fail");
    if let Some(t) = target {
        cmd.args(["--target", t]);
    }
    if alloc {
        cmd.env("ALMIDE_ALLOC_COUNT", "1");
    }
    let out = cmd.output().expect("almide");
    (String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

/// The counting allocator's `allocs` for `src`.
fn allocs(src: &Path) -> u64 {
    let (_, stderr) = run(src, None, true);
    let line = stderr.lines().find(|l| l.starts_with("__ALMD_ALLOC ")).unwrap_or_else(|| panic!("no allocation report:\n{stderr}"));
    line.split_whitespace().find_map(|kv| kv.strip_prefix("allocs=")).and_then(|v| v.parse().ok()).unwrap_or_else(|| panic!("malformed report `{line}`"))
}

/// The blocks `source`'s own work allocates: its count less that of a program
/// that only prints (the process's fixed cost, which differs by platform).
fn net_allocs(tag: &str, source: &str) -> u64 {
    let dir = scratch(tag);
    let (src, base) = (dir.join("prog.almd"), dir.join("baseline.almd"));
    std::fs::write(&src, source).unwrap();
    std::fs::write(&base, "fn main() -> Unit = println(\"baseline\")\n").unwrap();
    let net = allocs(&src).saturating_sub(allocs(&base));
    let _ = std::fs::remove_dir_all(&dir);
    net
}

/// `source` prints `expected` on the native render and on wasm, and the
/// native build is the certified render's own.
fn prints(tag: &str, source: &str, expected: &str) {
    if let Err(e) = almide_mir::pipeline::try_render_rust_source(source) {
        panic!("{tag} left the certified render: {e:?}");
    }
    let dir = scratch(tag);
    let src = dir.join("prog.almd");
    std::fs::write(&src, source).unwrap();
    for target in [None, Some("wasm")] {
        let (stdout, stderr) = run(&src, target, false);
        assert_eq!(stdout, expected, "{tag} on {target:?}:\n{stderr}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

const ACCUMULATORS: &str = r#"type Stats = { lo: Int, hi: Int, n: Int }

fn show(m: Map[String, Stats]) -> String = map.keys(m) |> list.sort |> list.map((k) => match map.get(m, k) {
  some(s) => "${k}:${s.lo}/${s.hi}/${s.n}",
  none => k,
}) |> list.join(" ")

// Handed to the runtime-only upsert in one arm, returned as it is in the other.
fn step(acc: Map[String, Stats], line: String) -> Map[String, Stats] = match string.split_once(line, ";") {
  some((name, text)) => {
    let t = string.len(text)
    map.upsert(acc, name, Stats { lo: t, hi: t, n: 1 }, (s) => Stats {
      lo: if t < s.lo then t else s.lo,
      hi: if t > s.hi then t else s.hi,
      n: s.n + 1,
    })
  },
  none => acc,
}

// Returned unchanged.
fn same(acc: Map[String, Stats], _line: String) -> Map[String, Stats] = acc

// Rebuilt: the accumulator is read and a new map returned.
fn rebuilt(acc: Map[String, Stats], line: String) -> Map[String, Stats] = map.set(map.new(), line, Stats { lo: map.len(acc), hi: 0, n: 0 })

// Used after the call that takes it: the body copies where it must.
fn kept(acc: Map[String, Stats], line: String) -> Map[String, Stats] = {
  let grown = map.set(acc, line, Stats { lo: 0, hi: 0, n: map.len(acc) })
  if map.len(acc) > 2 then acc else grown
}

// Captured by a closure before it is handed on.
fn captured(acc: Map[String, Stats], line: String) -> Map[String, Stats] = {
  let seen = (k: String) => map.contains(acc, k)
  let n = if seen(line) then 1 else 0
  map.set(acc, line, Stats { lo: n, hi: n, n: n })
}

// A callback that prints and one that fails keep their order.
fn loud(acc: Map[String, Stats], line: String) -> Map[String, Stats] = {
  println("step ${line} ${map.len(acc)}")
  map.upsert(acc, line, Stats { lo: 0, hi: 0, n: 1 }, (s) => {
    println("  again ${line}")
    Stats { lo: s.lo, hi: s.hi, n: s.n + 1 }
  })
}

fn main() -> Unit = {
  let lines = ["a;1", "bb;22", "a;333", "none", "bb;4", ""]
  println(show(lines |> list.fold(map.new(), (acc, line) => step(acc, line))))
  println(show(list.fold(lines, map.new(), step)))
  println(int.to_string(map.len(lines |> list.fold(map.new(), (acc, line) => same(acc, line)))))
  println(show(lines |> list.fold(map.new(), (acc, line) => rebuilt(acc, line))))
  println(show(lines |> list.fold(map.new(), (acc, line) => kept(acc, line))))
  println(show(lines |> list.fold(map.new(), (acc, line) => captured(acc, line))))
  println(show(["x", "y", "x"] |> list.fold(map.new(), (acc, line) => loud(acc, line))))
  // The accumulator the fold started from is still the caller's.
  let base = step(map.new(), "k;v")
  let once = lines |> list.fold(base, (acc, line) => step(acc, line))
  println("${show(base)} | ${show(once)}")
}
"#;

const FOLD_LINES: &str = r#"import fs

type Stats = { lo: Int, n: Int }

fn step(acc: Map[String, Stats], line: String) -> Map[String, Stats] = match string.split_once(line, ";") {
  some((name, text)) => {
    let t = string.len(text)
    map.upsert(acc, name, Stats { lo: t, n: 1 }, (s) => Stats { lo: if t < s.lo then t else s.lo, n: s.n + 1 })
  },
  none => acc,
}

effect fn main() -> Unit = {
  let path = fs.temp_dir()! + "/almide_callback_moves_lines.txt"
  let text = list.range(0, 3000) |> list.map((i) => "k${i % 7};${i}") |> list.join("\n")
  fs.write_text(path, text + "\nno separator\n")!
  let m = fs.fold_lines(path, map.new(), (acc, line) => step(acc, line))!
  fs.remove(path)!
  let total = map.keys(m) |> list.sort |> list.map((k) => match map.get(m, k) {
    some(s) => "${k}=${s.lo}/${s.n}",
    none => k,
  }) |> list.join(" ")
  println(total)
}
"#;

const SLICES: &str = r#"fn num(s: String) -> Int = match int.parse(s) {
  ok(v) => v,
  err(_) => -1,
}

// Both halves only read.
fn tenths(s: String) -> Int = match string.strip_prefix(s, "-") {
  some(rest) => 0 - tenths(rest),
  none => match string.split_once(s, ".") {
    some((whole, frac)) => num(whole) * 10 + num(frac),
    none => num(s) * 10,
  },
}

// One half stored, one read.
fn keyed(acc: Map[String, Int], line: String) -> Map[String, Int] = match string.split_once(line, "=") {
  some((k, v)) => map.set(acc, k, tenths(v)),
  none => acc,
}

// Both halves returned, swapped.
fn swap(s: String) -> (String, String) = match string.split_once(s, "é") {
  some((a, b)) => (b, a),
  none => (s, ""),
}

// A half captured by a closure and one interpolated.
fn tagged(s: String) -> String = match string.strip_suffix(s, "!") {
  some(rest) => [1, 2] |> list.map((k) => "${k}${rest}") |> list.join("+"),
  none => "(${s})",
}

// The subject is a local the body rebinds.
fn fields(s0: String) -> String = {
  var s = s0
  var outp = ""
  var rounds = 0
  while rounds < 5 {
    rounds = rounds + 1
    match string.split_once(s, ",") {
      some((a, b)) => {
        outp = outp + "[" + a + "]"
        s = b
      },
      none => {
        outp = outp + "(" + s + ")"
        s = ""
      },
    }
  }
  outp
}

fn main() -> Unit = {
  println("${tenths("12.5")} ${tenths("-3.4")} ${tenths("7")} ${tenths("--1.0")} ${tenths("")} ${tenths(".")}")
  let m = ["a=1.5", "b=-2.0", "junk", "a=9", "=", "née=3.3"] |> list.fold(map.new(), (acc, line) => keyed(acc, line))
  println(map.keys(m) |> list.sort |> list.map((k) => "${k}:${map.get(m, k) ?? 0}") |> list.join(" "))
  let (p, q) = swap("néeé")
  let (r, t) = swap("plain")
  println("${p}|${q}|${r}|${t}")
  println("${tagged("go!")} ${tagged("stay")} ${tagged("!")}")
  println(fields("x,y,,z"))
  var total = 0
  for i in list.range(0, 3000) {
    total = total + tenths("${i}.${i % 10}")
  }
  println(int.to_string(total))
}
"#;

const CHAIN: &str = r#"type Pair = { a: String, b: String, n: Int }

effect fn part(tag: String, s: String) -> String = {
  println("part ${tag}")
  if string.len(s) > 0 then ok(s) else err("empty ${tag}")
}

effect fn size(s: String) -> Int = if string.len(s) < 4 then ok(string.len(s)) else err("long")

effect fn pair(x: String, y: String) -> Pair = {
  let a = part("a", x)!
  let n = size(a)!
  let b = part("b", y)!
  ok(Pair { a: a, b: b, n: n })
}

fn show(r: Result[Pair, String]) -> String = match r {
  ok(p) => "${p.a}+${p.b}+${p.n}",
  err(e) => "err(${e})",
}

effect fn main() -> Unit = {
  println(show(pair("l", "r")))
  println(show(pair("", "r")))
  println(show(pair("long one", "r")))
  println(show(pair("l", "")))
}
"#;

const SLICE_LOOP: &str = r#"fn num(s: String) -> Int = match int.parse(s) {
  ok(v) => v,
  err(_) => -1,
}

fn tenths(s: String) -> Int = match string.strip_prefix(s, "-") {
  some(rest) => 0 - tenths(rest),
  none => match string.split_once(s, ".") {
    some((whole, frac)) => num(whole) * 10 + num(frac),
    none => num(s) * 10,
  },
}

fn main() -> Unit = {
  let texts = ["12.5", "-3.4", "7", "--1.0"]
  var total = 0
  for i in list.range(0, 3000) {
    total = total + tenths(texts[i % 4])
  }
  println(int.to_string(total))
}
"#;

const FOLD_LOOP: &str = r#"type Stats = { lo: Int, n: Int }

fn step(acc: Map[String, Stats], line: String) -> Map[String, Stats] = match string.split_once(line, ";") {
  some((name, text)) => {
    let t = string.len(text)
    map.upsert(acc, name, Stats { lo: t, n: 1 }, (s) => Stats { lo: if t < s.lo then t else s.lo, n: s.n + 1 })
  },
  none => acc,
}

fn main() -> Unit = {
  let lines = ["a;1", "bb;22", "a;333", "none", "c;4", "dd;55", "e;6", "ff;7", "g;8", "h;9", "i;10", "j;11"]
  var m: Map[String, Stats] = map.new()
  for _i in list.range(0, 300) {
    m = lines |> list.fold(m, (acc, line) => step(acc, line))
  }
  println("${map.len(m)} ${(map.get(m, "a") ?? Stats { lo: 0, n: 0 }).n}")
}
"#;

const BUFFERS: &str = r#"import bytes

fn total(b: Bytes) -> Int = bytes.to_list(b) |> list.fold(0, (a, x) => a + x)

fn keep(b: Bytes) -> Bytes = b

fn reset(mut b: Bytes) -> Unit = {
  bytes.clear(b)
  bytes.push(b, 9)
}

fn main() -> Unit = {
  // A line buffer cleared and refilled per line.
  var buf = bytes.zeros(64)
  var sum = 0
  for line in 0..<3000 {
    bytes.clear(buf)
    for i in 0..<60 {
      bytes.push(buf, (line + i) % 251)
    }
    sum = sum + bytes.len(buf) + (bytes.get(buf, 59) ?? 0)
  }
  println("${sum} ${bytes.len(buf)}")

  // A copy taken before the clear keeps what it held.
  var a = bytes.from_string("abc")
  let before = a
  bytes.clear(a)
  bytes.push(a, 122)
  println("${bytes.len(before)} ${total(before)} ${bytes.len(a)} ${total(a)}")

  // Handed on, stored, and cleared through a mut parameter.
  var held: List[Bytes] = []
  var b = bytes.zeros(2)
  for r in 0..<4 {
    bytes.push(b, r + 1)
    held = held + [b]
    if r % 2 == 0 then { bytes.clear(b) } else { reset(b) }
    b = keep(b)
  }
  println("${bytes.len(b)} ${total(b)} ${held |> list.map((h) => bytes.len(h)) |> list.fold(0, (x, y) => x * 10 + y)}")

  // Rebound to a longer zeroed buffer than the slot holds, then a shorter.
  var c = bytes.zeros(1)
  c = bytes.zeros(40)
  bytes.push(c, 5)
  c = bytes.zeros(3)
  println("${bytes.len(c)} ${total(c)}")
}
"#;

#[test]
fn an_accumulator_moves_through_its_callback_and_prints_the_same_on_both_targets() {
    // Returned unchanged, rebuilt, from one arm only, read after the call
    // that takes it, captured; a callback that prints keeps its order; and
    // the map a fold started from is still its caller's afterwards.
    prints("accumulators", ACCUMULATORS, "a:1/3/2 bb:1/2/2\na:1/3/2 bb:1/2/2\n0\n:1/0/0\na;1:0/0/0 a;333:0/0/2 bb;22:0/0/1\n:0/0/0 a;1:0/0/0 a;333:0/0/0 bb;22:0/0/0 bb;4:0/0/0 none:0/0/0\nstep x 0\nstep y 1\nstep x 2\n  again x\nx:0/0/2 y:0/0/1\nk:1/1/1 | a:1/3/2 bb:1/2/2 k:1/1/1\n");
}

#[test]
fn a_line_fold_hands_its_accumulator_to_the_step() {
    prints("fold-lines", FOLD_LINES, "k0=1/429 k1=1/429 k2=1/429 k3=1/429 k4=1/428 k5=1/428 k6=1/428\n");
}

#[test]
fn slices_of_a_borrowed_string_print_the_same_on_both_targets() {
    // Only read, stored, returned, captured, and a subject the body rebinds.
    prints("slices", SLICES, "125 -34 70 10 -10 -11\n:-10 a:90 b:-20 née:33\neé|n|plain|\n1go+2go (stay) 1+2\n[x][y][](z)()\n44998500\n");
}

#[test]
fn a_propagation_chain_fails_at_the_same_step_on_both_targets() {
    prints("chain", CHAIN, "part a\npart b\nl+r+1\npart a\nerr(empty a)\npart a\nerr(long)\npart a\npart b\nerr(empty b)\n");
}

#[test]
fn a_split_that_is_only_read_allocates_nothing() {
    // Owned halves were two blocks per split and one per stripped prefix:
    // 13,000 over these 3,000 calls.
    prints("slice-loop", SLICE_LOOP, "128250\n");
    let n = net_allocs("slice-loop-alloc", SLICE_LOOP);
    assert!(n < 100, "the splits allocate {n} blocks: an owned half per call is back");
}

#[test]
fn a_fold_step_does_not_copy_its_accumulator() {
    // 3,600 steps. A map copied per step is three blocks and more each
    // (13,000 in all); moved, a step allocates its one new key at most.
    prints("fold-loop", FOLD_LOOP, "10 600\n");
    let n = net_allocs("fold-loop-alloc", FOLD_LOOP);
    assert!(n < 5000, "the fold allocates {n} blocks over 3600 steps: the accumulator is copied per step");
}

#[test]
fn a_cleared_buffer_keeps_its_block_and_prints_the_same_on_both_targets() {
    // A copy taken before the clear keeps what it held; a buffer handed on,
    // stored, cleared through a `mut` parameter, and rebound to a longer
    // zeroed buffer than its slot holds all print as wasm does.
    prints("buffers", BUFFERS, "555870 60\n3 294 1 122\n1 9 3121\n3 0\n");
    // 3,000 lines: a fresh buffer per clear was a block per line, regrown
    // three times before it held its sixty bytes.
    let n = net_allocs("buffers-alloc", BUFFERS);
    assert!(n < 100, "the line buffer allocates {n} blocks over 3000 lines: it is built afresh per clear");
}
