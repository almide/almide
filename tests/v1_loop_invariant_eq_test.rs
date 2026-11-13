//! #4661: an equality a loop cannot change is compared once on the certified
//! native render, and an in-place map update borrows a string key it only
//! looks up — neither of which a program can tell from the spelling that
//! compares every round and shares the key's handle every update.
//!
//! Each program prints one fixed text on the native render (ownership
//! certificate required, rendered by the certified render itself) and on
//! wasm. The comparison is remembered where it is written, so the programs
//! cover what a lifted comparison would get wrong: a loop that runs no round
//! or never reaches the site, a print or a `continue` ahead of the site, an
//! operand an enclosing loop rebinds, and an operand the loop itself writes.
//! The allocation lane (`ALMIDE_ALLOC_COUNT`) bounds the blocks the keyed
//! updates allocate. No wall-clock assertion.
//!
//! The key's place is read where the update is made, which is only the
//! element that was read while the index it was read at stands: a read whose
//! index variable is rewritten first is copied from the reference it holds
//! (`moved_index_reads`). Spelled over the index again, the copy was of the
//! neighbouring element.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-invariant-eq-{}-{tag}", std::process::id()));
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

const COMPARISONS: &str = r#"type P = { name: String, xs: List[Int] }
type T =
  | Leaf(Int)
  | Node(List[T])

fn mk(k: Int) -> P = { name: "n" + int.to_string(k % 2), xs: [1, k % 2] }

fn tree(k: Int) -> T = Node([Leaf(k % 2), Node([Leaf(1)])])

fn rounds(a: P, b: P, n: Int) -> Int = {
  var acc = 0
  for i in 0..<n {
    println("round ${i}")
    if a == b then { acc = acc + 1 } else { acc = acc + 10 }
    if a != b then { acc = acc + 100 } else { () }
  }
  acc
}

fn main() -> Unit = {
  let a = mk(0)
  let b = mk(2)
  let c = mk(1)

  // no round, one round, three rounds; equal and unequal operands
  println("${rounds(a, b, 0)} ${rounds(a, b, 1)} ${rounds(a, c, 3)}")

  // the site is first reached in a later round, after a continue
  var late = 0
  for i in 0..<4 {
    if i < 2 then { continue } else { () }
    println("reached ${i}")
    let same = a.xs == b.xs
    if same then { late = late + i } else { () }
  }
  println("${late}")

  // an operand the enclosing loop rebinds is remembered for one of its rounds
  var per = 0
  for j in 0..<4 {
    let d = mk(j)
    for i in 0..<3 {
      if a == d then { per = per + 1 } else { per = per + 10 }
    }
  }
  println("${per}")

  // an operand the loop writes is compared every round
  var e = mk(0)
  var moved = 0
  var k = 0
  while k < 4 {
    if a == e then { moved = moved + 1 } else { moved = moved + 10 }
    e = mk(k + 1)
    k = k + 1
  }
  println("${moved}")

  // recursive variants, a string, an option, and a block's tail
  let t = tree(0)
  let u = tree(2)
  let v = tree(1)
  let s: Option[String] = some("x")
  var mix = 0
  for i in 0..<3 {
    if t == u then { mix = mix + 1 } else { () }
    if t == v then { mix = mix + 10 } else { () }
    if a.name == c.name then { mix = mix + 100 } else { () }
    let w = {
      println("block ${i}")
      if s == none then 1000 else 2000
    }
    mix = mix + w
  }
  println("${mix}")

  // inside a closure the loop is the closure's own
  let f = (n: Int) => {
    var got = 0
    for i in 0..<n {
      if b == a then { got = got + i + 1 } else { () }
    }
    got
  }
  println("${f(0)} ${f(3)}")
}
"#;

const KEYS: &str = r#"type E = { name: String, n: Int }

fn show(m: Map[String, Int]) -> String = map.keys(m) |> list.sort |> list.map((k) => "${k}=${map.get(m, k) ?? -1}") |> list.join(",")

fn main() -> Unit = {
  let ws = ["b", "a", "", "b", "ü"]
  let es: List[E] = [{ name: "x", n: 2 }, { name: "y", n: 3 }, { name: "x", n: 5 }]
  var counts: Map[String, Int] = [:]
  var last: Map[String, Int] = [:]
  var sums: Map[String, Int] = [:]
  for i in 0..<12 {
    let w = ws[i % 5]
    counts[w] = (map.get(counts, w) ?? 0) + 1
    last = map.set(last, ws[i % 5], i)
    sums = map.upsert(sums, es[i % 3].name, es[i % 3].n, (v) => v + es[i % 3].n)
    println(w)
  }
  // the keys' sources are whole afterwards
  println(list.join(ws, "|"))
  println("${show(counts)} / ${show(last)} / ${show(sums)} / ${es[2].name}")
}
"#;

/// 3,000 distinct keys, each added once and found once, the element read
/// spelled at both the lookup and the store.
const MANY_KEYS: &str = r#"fn main() -> Unit = {
  let ws = list.map(list.range(0, 3000), (i) => "key-number-${i}")
  var counts: Map[String, Int] = [:]
  for i in 0..<6000 {
    counts[ws[i % 3000]] = (map.get(counts, ws[i % 3000]) ?? 0) + 1
  }
  println("${map.len(counts)} ${map.get(counts, "key-number-2999") ?? 0}")
}
"#;

/// A borrowed element read whose index variable is rewritten before the read
/// is copied, handed on or used as a key: the copy is of the element that was
/// read, not of the one the index names by then.
const INDEX_MOVES: &str = r#"type E = { name: String, n: Int }

fn show(m: Map[String, Int]) -> String = map.keys(m) |> list.sort |> list.map((k) => "${k}=${map.get(m, k) ?? -1}") |> list.join(",")

fn keep(s: String) -> String = s

fn main() -> Unit = {
  let ws = ["a", "b", "c", "d", "e"]
  let es: List[E] = [{ name: "x", n: 1 }, { name: "y", n: 2 }, { name: "z", n: 3 }]

  // the index moves between the read and its copy
  var out: List[String] = []
  var j = 0
  for _i in 0..<4 {
    let w = ws[j]
    j = j + 1
    let c = w
    out = out + [c, keep(w)]
  }
  println(list.join(out, ","))

  // … and between the read and the update it keys
  var m: Map[String, Int] = [:]
  var k = 0
  while k < 4 {
    let w = ws[k]
    k = k + 1
    m = map.upsert(m, w, k, (v) => v + 1)
  }
  println(show(m))

  // read ahead of a loop that moves the index every round
  var seen: List[String] = []
  var p = 0
  let first = es[p].name
  for _i in 0..<3 {
    seen = seen + [first]
    p = (p + 1) % 3
  }
  println(list.join(seen, ","))

  // moved in one branch only; the same read used on both sides of the move
  var both: List[String] = []
  var q = 0
  for i in 0..<4 {
    let w = ws[q]
    both = both + [w]
    if i % 2 == 0 then { q = q + 2 } else { () }
    both = both + [w + "'"]
  }
  println(list.join(both, ","))
}
"#;

#[test]
fn a_read_copied_after_its_index_moved_is_the_element_that_was_read() {
    prints("index-moves", INDEX_MOVES, "a,a,b,b,c,c,d,d\na=1,b=2,c=3,d=4\nx,x,x\na,a',c,c',c,c',e,e'\n");
}

#[test]
fn a_remembered_comparison_prints_the_same_on_both_targets() {
    prints(
        "comparisons",
        COMPARISONS,
        "round 0\nround 0\nround 1\nround 2\n0 1 330\nreached 2\nreached 3\n5\n66\n22\nblock 0\nblock 1\nblock 2\n6003\n0 6\n",
    );
}

#[test]
fn a_borrowed_key_updates_the_same_entries_on_both_targets() {
    prints(
        "keys",
        KEYS,
        "b\na\n\nb\nü\nb\na\n\nb\nü\nb\na\nb|a||b|ü\n=2,a=3,b=5,ü=2 / =7,a=11,b=10,ü=9 / x=28,y=12 / x\n",
    );
}

#[test]
fn an_added_key_shares_its_handle() {
    prints("many-keys", MANY_KEYS, "3000 2\n");
    // The list holds one block per key and the map grows by doubling: 3,011
    // blocks. A key whose bytes are copied for every update is one block
    // more per update: 9,011.
    let n = net_allocs("many-keys-alloc", MANY_KEYS);
    assert!(n < 5000, "the keyed updates allocate {n} blocks for 3000 keys and 6000 updates: a key is copied per update");
}
