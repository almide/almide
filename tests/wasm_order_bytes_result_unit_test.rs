//! `list.sort` / `min` / `max` / `sort_by` order Bytes, Result and Unit
//! elements on wasm the way native's derived `Ord` does, instead of walling
//! with E082 `list-cmp-elem:<ty>` (found next to #3588):
//! - Bytes is `Vec<u8>`: unsigned byte-lexicographic, and a strict prefix
//!   sorts first.
//! - Result derives `Ok < Err`, then the active side's payload.
//! - Unit values are all equal, so a stable sort keeps the input order.
//!
//! Each program runs on both legs and must print the same lines. The nested
//! shapes (Result inside Option, Result of Result) reach the compare through
//! its recursion.
use std::process::Command;

const SOURCE: &str = r#"fn main() -> Unit = {
  let bs = [bytes.from_list([2, 1]), bytes.from_list([255]), bytes.from_list([]), bytes.from_list([2]), bytes.from_list([2, 0, 9])]
  println("${list.map(list.sort(bs), (b) => bytes.to_list(b))}")
  println("${list.map(list.max(bs) |> option.to_list, (b) => bytes.to_list(b))}")
  println("${list.map(list.min(bs) |> option.to_list, (b) => bytes.to_list(b))}")
  let rs: List[Result[Int, String]] = [err("b"), ok(3), err("a"), ok(-1)]
  println("${list.sort(rs)}")
  println("${list.max(rs)} ${list.min(rs)}")
  let ws = ["xx", "y", "zzz"]
  println("${list.sort_by(ws, (w) => bytes.from_string(w))}")
  let ts = [(bytes.from_list([1]), 2), (bytes.from_list([1]), 1), (bytes.from_list([0, 5]), 9)]
  println("${list.map(list.sort(ts), ((b, n)) => "${bytes.to_list(b)}:${n}")}")
  let us = [(), (), ()]
  println("${list.len(list.sort(us))}")
  let ps = [((), 3), ((), 1), ((), 2)]
  println("${list.map(list.sort(ps), ((u, n)) => n)}")
  let a: List[Option[Result[Bytes, Unit]]] = [some(err(())), none, some(ok(bytes.from_list([3]))), some(ok(bytes.from_list([])))]
  println("${list.map(list.sort(a), (x) => match x { none => "n", some(ok(b)) => "o${bytes.to_list(b)}", some(err(_)) => "e" })}")
  let c: List[Result[Result[Int, String], Option[Bytes]]] = [err(none), ok(err("q")), ok(ok(5)), err(some(bytes.from_list([9]))), ok(ok(-5))]
  let show = (r: Result[Result[Int, String], Option[Bytes]]) => match r { ok(ok(n)) => "oo${n}", ok(err(s)) => "oe${s}", err(none) => "en", err(some(b)) => "es${bytes.to_list(b)}" }
  println("${list.map(list.sort(c), show)}")
}
"#;

const EXPECTED: &str = r#"[[], [2], [2, 0, 9], [2, 1], [255]]
[[255]]
[[]]
[ok(-1), ok(3), err("a"), err("b")]
some(err("b")) some(ok(-1))
["xx", "y", "zzz"]
["[0, 5]:9", "[1]:1", "[1]:2"]
3
[1, 2, 3]
["n", "o[]", "o[3]", "e"]
["oo-5", "oo5", "oeq", "en", "es[9]"]
"#;

fn run(wasm: bool) -> String {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("order.almd");
    std::fs::write(&file, SOURCE).expect("source");
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(&file);
    if wasm {
        c.args(["--target", "wasm"]);
    }
    let out = c.output().expect("almide run");
    assert!(out.status.success(), "wasm={wasm}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn bytes_result_and_unit_elements_order_the_same_on_both_legs() {
    assert_eq!(run(false), EXPECTED, "native");
    assert_eq!(run(true), EXPECTED, "wasm");
}
