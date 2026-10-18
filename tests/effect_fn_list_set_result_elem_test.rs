//! #3591: inside an effect fn, `x = list.set(x, i, v)` is rewritten to an
//! in-place write through a hoisted `let __set_v = v`. When the list holds
//! Results, auto_try read that synthetic bind as an implicit-propagation site
//! and unwrapped `v` with `?`: native emitted `let v: i64 = (Ok(2))?` (rustc
//! E0308) and wasm walled with `ty-mismatch`. The element is written as the
//! Result it is, on both legs, for an ok and an err value, in a loop, and
//! next to a read of the list.
use std::process::Command;

const SOURCE: &str = r#"effect fn main() -> Unit = {
  var x: List[Result[Int, String]] = [ok(1), err("e")]
  x = list.set(x, 0, ok(2))
  x = list.set(x, 1, err("f"))
  x = list.set(x, 9, ok(9))
  var i = 0
  while i < 3 {
    x = list.set(x, i % 2, if i == 2 then err("g") else ok(i))
    let r: (List[Int], List[String]) = result.partition(x)
    println("${r}")
    i = i + 1
  }
  var o: List[Option[Result[Int, String]]] = [none]
  o = list.set(o, 0, some(err("h")))
  println("${x} ${o}")
}
"#;

const EXPECTED: &str = r#"([0], ["f"])
([0, 1], [])
([1], ["g"])
[err("g"), ok(1)] [some(err("h"))]
"#;

fn run(wasm: bool) -> String {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("set.almd");
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
fn an_effect_fn_list_set_rebind_writes_the_result_element_on_both_legs() {
    assert_eq!(run(false), EXPECTED, "native");
    assert_eq!(run(true), EXPECTED, "wasm");
}
