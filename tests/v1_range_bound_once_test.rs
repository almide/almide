//! #4662: `for i in a..<b` reads its bounds once, before the first round.
//!
//! The certified native render counted against the bound's own local, so a
//! body that reassigned the bound variable moved the bound under the loop:
//! `var n = 5; for _i in 0..<n { n = n + 10 }` ran 5 rounds on wasm and never
//! ended natively. `snapshot_range_bounds` binds such a bound ahead of the
//! loop, and `native_range_loops.rs` counts `for i in list.range(a, b)` over
//! variable bounds on the same promise.
//!
//! Each program prints one fixed text on the native render (ownership
//! certificate required, no fallback to the standard render) and on wasm.
//! A run that outlives its limit is killed and fails the row — the regressed
//! shape does not end.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-range-bound-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// (stdout, stderr) of `almide run <src>` on `target`, killed after two minutes.
fn run(src: &Path, target: Option<&str>) -> (String, String) {
    let mut cmd = Command::new(almide());
    cmd.arg("run").arg(src).env("ALMIDE_CERTIFY_OWNERSHIP", "fail");
    if let Some(t) = target {
        cmd.args(["--target", t]);
    }
    let (out, err) = (src.with_extension("out"), src.with_extension("err"));
    let mut child = cmd
        .stdout(Stdio::from(std::fs::File::create(&out).unwrap()))
        .stderr(Stdio::from(std::fs::File::create(&err).unwrap()))
        .spawn()
        .expect("almide");
    let start = Instant::now();
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if start.elapsed() > Duration::from_secs(120) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("{} on {target:?} did not end:\n{}", src.display(), std::fs::read_to_string(&out).unwrap_or_default());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    (std::fs::read_to_string(&out).unwrap(), std::fs::read_to_string(&err).unwrap())
}

/// `source` prints `expected` on the native render and on wasm, and the
/// native build is the certified render's own.
fn prints(tag: &str, source: &str, expected: &str) {
    let dir = scratch(tag);
    let src = dir.join("prog.almd");
    std::fs::write(&src, source).unwrap();
    if let Err(e) = almide_mir::pipeline::try_render_rust_source(source) {
        panic!("{tag} left the certified render: {e:?}");
    }
    for target in [None, Some("wasm")] {
        let (stdout, stderr) = run(&src, target);
        assert_eq!(stdout, expected, "{tag} on {target:?}:\n{stderr}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

const RANGES: &str = r#"type Box = { n: Int, lo: Int }

var calls = 0

fn bound(k: Int) -> Int = {
  calls = calls + 1
  println("bound ${k}")
  k
}

fn grow() -> Unit = {
  calls = calls + 100
}

var top = 4

fn bump_top() -> Unit = {
  top = top + 7
}

fn main() -> Unit = {
  // bound is a var reassigned in the body
  var n = 5
  var c = 0
  for _i in 0..<n {
    n = n + 10
    c = c + 1
  }
  println("a ${c} ${n}")

  // inclusive
  var m = 3
  var d = 0
  for i in 0...m {
    m = m + 2
    d = d + i
  }
  println("b ${d} ${m}")

  // start bound too
  var lo = 2
  var hi = 6
  var e = 0
  for i in lo..<hi {
    lo = lo + 100
    hi = hi - 1
    e = e + i
  }
  println("c ${e} ${lo} ${hi}")

  // a field of a reassigned record
  var b = { n: 4, lo: 1 }
  var f = 0
  for i in b.lo..<b.n {
    b = { n: b.n + 5, lo: b.lo + 5 }
    f = f + i
  }
  println("d ${f} ${b.n} ${b.lo}")

  // a call with an effect runs once
  var g = 0
  for i in 0..<bound(3) {
    g = g + i
  }
  println("e ${g} ${calls}")
  for i in bound(1)...bound(4) {
    g = g + i
  }
  println("f ${g} ${calls}")

  // nested loops, inner bound is the outer's var and both move
  var w = 3
  var h = 0
  for i in 0..<w {
    w = w + 1
    var z = i + 1
    for j in 0..<z {
      z = z + 3
      w = w + 1
      h = h + j + 1
    }
  }
  println("g ${h} ${w}")

  // a global written through a call
  var k = 0
  for _i in 0..<top {
    bump_top()
    k = k + 1
  }
  println("h ${k} ${top}")

  // a let-bound range read only as the loop head
  var q = 4
  let r = 0..<q
  q = 40
  var s = 0
  for i in r {
    q = q + 1
    s = s + i
  }
  println("i ${s} ${q}")

  // a bound moved by a break-carrying body and by a while inside
  var u = 6
  var v = 0
  for i in 0..<u {
    while u < 9 {
      u = u + 1
    }
    if i == 4 then { break } else { () }
    v = v + 1
  }
  println("j ${v} ${u}")

  // arithmetic over a moved var
  var x = 2
  var y = 0
  for _i in 0..<x * 3 + 1 {
    x = x * 2
    y = y + 1
    if y > 50 then { break } else { () }
  }
  println("k ${y} ${x}")
  grow()
  println("l ${calls}")
}
"#;

const RANGES_OUT: &str = "a 5 55\nb 6 11\nc 14 402 2\nd 6 19 16\nbound 3\ne 3 1\nbound 1\nbound 4\nf 13 3\ng 10 12\nh 4 32\ni 6 44\nj 4 9\nk 7 256\nl 103\n";

const LIST_RANGES: &str = r#"var seen = 0

fn edge(k: Int) -> Int = {
  seen = seen + 1
  k
}

fn main() -> Unit = {
  var n = 5
  var rounds = 0
  for _i in list.range(0, n) {
    n = n + 10
    rounds = rounds + 1
  }
  var lo = 1
  var hi = 4
  var sum = 0
  for i in list.range(lo, hi * 2) {
    lo = lo + 50
    hi = hi + 50
    sum = sum + i
  }
  var t = 0
  for i in list.range(edge(2), edge(6)) {
    t = t + i
  }
  var outer = 3
  var cells = 0
  for i in list.range(0, outer) {
    outer = outer + 2
    for _j in list.range(i, outer) {
      outer = outer + 1
      cells = cells + 1
    }
  }
  println("${rounds} ${n} ${sum} ${lo} ${hi} ${t} ${seen} ${cells} ${outer}")
}
"#;

#[test]
fn a_range_reads_its_bounds_once() {
    prints("ranges", RANGES, RANGES_OUT);
}

#[test]
fn a_counted_list_range_reads_its_bounds_once() {
    prints("list-ranges", LIST_RANGES, "5 55 28 351 354 14 2 39 48\n");
}
