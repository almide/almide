//! #4666: a list element read through a borrow keyed on its index variable
//! is the element that was read, even when the index is rewritten before the
//! bound name is copied, handed on or used as a key.
//!
//! The certified native render spelled the copy over the index again, so it
//! copied the neighbouring element (`b,c,d,e` where wasm prints `a,b,c,d`).
//! A read whose index variable is rewritten first is now copied from the
//! reference it holds (`moved_index_reads`).
//!
//! Each program prints one fixed text on the native render (ownership
//! certificate required, rendered by the certified render itself) and on wasm.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-moved-index-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// (stdout, stderr) of `almide run <src>` on `target`, the certificate required.
fn run(src: &Path, target: Option<&str>) -> (String, String) {
    let mut cmd = Command::new(almide());
    cmd.arg("run").arg(src).env("ALMIDE_CERTIFY_OWNERSHIP", "fail");
    if let Some(t) = target {
        cmd.args(["--target", t]);
    }
    let out = cmd.output().expect("almide");
    (String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
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
        let (stdout, stderr) = run(&src, target);
        assert_eq!(stdout, expected, "{tag} on {target:?}:\n{stderr}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// The reproduction from #4666.
const ISSUE: &str = r#"fn main() -> Unit = {
  let ws = ["a", "b", "c", "d", "e"]
  var out: List[String] = []
  var j = 0
  for _i in 0..<4 {
    let w = ws[j]
    j = j + 1
    let c = w
    out = out + [c]
  }
  println(list.join(out, ","))
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
fn the_reported_read_copies_the_element_that_was_read() {
    prints("issue", ISSUE, "a,b,c,d\n");
}

#[test]
fn a_read_copied_after_its_index_moved_is_the_element_that_was_read() {
    prints("index-moves", INDEX_MOVES, "a,a,b,b,c,c,d,d\na=1,b=2,c=3,d=4\nx,x,x\na,a',c,c',c,c',e,e'\n");
}
