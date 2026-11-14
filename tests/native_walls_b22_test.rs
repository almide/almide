//! v1 walls, round 22 (family B): a heap-result `if` chain is lowered once
//! per level. A nested `if` arm that declined used to be retried by a second
//! router on the same arm, so one declining arm deep in a chain cost 2^depth
//! lowerings: a 34-arm string match in arlk's hol.almd never finished, and the
//! 28-arm chain below took more than 40 s at 14 arms before the fix.
//!
//! The arms are the shape hol's `command` walled on when this was found (a
//! `!` call through a `mut` param, then a guard raising `err`). The test
//! bounds the time the native render takes, whether it renders or walls.

use std::sync::mpsc;
use std::time::Duration;

const DEEP_CHAIN: &str = r#"
type Obj =
  | ONum(Int)
  | OName(String)
  | OList(List[Obj])
  | OGone
type St = { out: List[String], gens: Map[String, Int] }
local fn kind(o: Obj) -> String = match o { ONum(_) => "number", OName(_) => "name", OList(_) => "list", OGone => "gone" }
local fn pop(mut stack: List[Obj]) -> Obj! = match list.pop(stack) {
  some(o) => ok(o),
  none => err("the stack is empty"),
}
local fn pop_num(mut stack: List[Obj]) -> Int! = match pop(stack)! {
  ONum(n) => ok(n),
  o => err("expected a number, found " + kind(o)),
}
local fn command(mut st: St, mut stack: List[Obj], mut dict: Map[Int, Obj], cmd: String) -> Unit! = match cmd {
  "v0" => {
    let v = pop_num(stack)!
    guard v == 6 or v == 5 else err("article version " + int.to_string(v) + " is not supported")
    ok(())
  },
  "v1" => {
    let v = pop_num(stack)!
    guard v == 6 or v == 5 else err("article version " + int.to_string(v) + " is not supported")
    ok(())
  },
  "v2" => {
    let v = pop_num(stack)!
    guard v == 6 or v == 5 else err("article version " + int.to_string(v) + " is not supported")
    ok(())
  },
  "v3" => {
    let v = pop_num(stack)!
    guard v == 6 or v == 5 else err("article version " + int.to_string(v) + " is not supported")
    ok(())
  },
  "v4" => {
    let v = pop_num(stack)!
    guard v == 6 or v == 5 else err("article version " + int.to_string(v) + " is not supported")
    ok(())
  },
  "v5" => {
    let v = pop_num(stack)!
    guard v == 6 or v == 5 else err("article version " + int.to_string(v) + " is not supported")
    ok(())
  },
  "v6" => {
    let v = pop_num(stack)!
    guard v == 6 or v == 5 else err("article version " + int.to_string(v) + " is not supported")
    ok(())
  },
  "v7" => {
    let v = pop_num(stack)!
    guard v == 6 or v == 5 else err("article version " + int.to_string(v) + " is not supported")
    ok(())
  },
  "v8" => {
    let v = pop_num(stack)!
    guard v == 6 or v == 5 else err("article version " + int.to_string(v) + " is not supported")
    ok(())
  },
  "v9" => {
    let v = pop_num(stack)!
    guard v == 6 or v == 5 else err("article version " + int.to_string(v) + " is not supported")
    ok(())
  },
  "v10" => {
    let v = pop_num(stack)!
    guard v == 6 or v == 5 else err("article version " + int.to_string(v) + " is not supported")
    ok(())
  },
  "v11" => {
    let v = pop_num(stack)!
    guard v == 6 or v == 5 else err("article version " + int.to_string(v) + " is not supported")
    ok(())
  },
  "v12" => {
    let v = pop_num(stack)!
    guard v == 6 or v == 5 else err("article version " + int.to_string(v) + " is not supported")
    ok(())
  },
  "v13" => {
    let v = pop_num(stack)!
    guard v == 6 or v == 5 else err("article version " + int.to_string(v) + " is not supported")
    ok(())
  },
  "v14" => {
    let v = pop_num(stack)!
    guard v == 6 or v == 5 else err("article version " + int.to_string(v) + " is not supported")
    ok(())
  },
  "v15" => {
    let v = pop_num(stack)!
    guard v == 6 or v == 5 else err("article version " + int.to_string(v) + " is not supported")
    ok(())
  },
  "v16" => {
    let v = pop_num(stack)!
    guard v == 6 or v == 5 else err("article version " + int.to_string(v) + " is not supported")
    ok(())
  },
  "v17" => {
    let v = pop_num(stack)!
    guard v == 6 or v == 5 else err("article version " + int.to_string(v) + " is not supported")
    ok(())
  },
  "v18" => {
    let v = pop_num(stack)!
    guard v == 6 or v == 5 else err("article version " + int.to_string(v) + " is not supported")
    ok(())
  },
  "v19" => {
    let v = pop_num(stack)!
    guard v == 6 or v == 5 else err("article version " + int.to_string(v) + " is not supported")
    ok(())
  },
  "v20" => {
    let v = pop_num(stack)!
    guard v == 6 or v == 5 else err("article version " + int.to_string(v) + " is not supported")
    ok(())
  },
  "v21" => {
    let v = pop_num(stack)!
    guard v == 6 or v == 5 else err("article version " + int.to_string(v) + " is not supported")
    ok(())
  },
  "v22" => {
    let v = pop_num(stack)!
    guard v == 6 or v == 5 else err("article version " + int.to_string(v) + " is not supported")
    ok(())
  },
  "v23" => {
    let v = pop_num(stack)!
    guard v == 6 or v == 5 else err("article version " + int.to_string(v) + " is not supported")
    ok(())
  },
  "v24" => {
    let v = pop_num(stack)!
    guard v == 6 or v == 5 else err("article version " + int.to_string(v) + " is not supported")
    ok(())
  },
  "v25" => {
    let v = pop_num(stack)!
    guard v == 6 or v == 5 else err("article version " + int.to_string(v) + " is not supported")
    ok(())
  },
  "v26" => {
    let v = pop_num(stack)!
    guard v == 6 or v == 5 else err("article version " + int.to_string(v) + " is not supported")
    ok(())
  },
  "v27" => {
    let v = pop_num(stack)!
    guard v == 6 or v == 5 else err("article version " + int.to_string(v) + " is not supported")
    ok(())
  },
  _ => err("unknown " + cmd),
}
effect fn main() -> Unit = {
  var st = St { out: [], gens: [:] }
  var s = [ONum(6)]
  var d: Map[Int, Obj] = [:]
  command(st, s, d, "v27")!
  println(int.to_string(list.len(s)))
}
"#;

#[test]
fn a_deep_heap_result_if_chain_lowers_in_bounded_time() {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || {
            let r = almide_mir::pipeline::try_render_rust_source(DEEP_CHAIN).map(|_| ()).map_err(|e| e.to_string());
            let _ = tx.send(r);
        })
        .unwrap();
    let verdict = rx.recv_timeout(Duration::from_secs(10));
    assert!(verdict.is_ok(), "the native render of a 28-level heap-result if chain did not finish in 10 s");
}
