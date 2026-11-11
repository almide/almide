//! A fn that calls itself in tail position and has a `guard c else E` in one
//! of its tail blocks. The native render turns the self call into a loop,
//! and the loop conversion reads the tail `if` arm by arm; a guard statement
//! left in the loop body had no way to deliver its else, so the fn was
//! refused (#4640). The guard is now read as the branch it is before the
//! conversion. `SHAPES` holds the else as an `err`, as a plain value and as
//! another self call, a guard after a bind and two guards in a row, at a
//! depth the loop must carry (8 000 and 16 000 calls). `PARSER` is the shape
//! the refusal came from: propagating calls before and after the guard, and a
//! bare `err` else.
use std::process::Command;

const SHAPES: &str = r##"fn peek(t: String, p: Int) -> String = string.slice(t, p, p + 1)

fn items_of(t: String, p: Int, acc: List[Int]) -> Result[(List[Int], Int), String] = {
  let items = acc + [p]
  if peek(t, p) == "," then {
    let q = p + 1
    if peek(t, q) == "]" then ok((items, q + 1))
    else items_of(t, q, items)
  } else {
    guard peek(t, p) == "]" else err("expected ']' at " + int.to_string(p))
    ok((items, p + 1))
  }
}

fn depth(t: String, p: Int, acc: List[Int]) -> Int = {
  let seen = acc + [p]
  match peek(t, p) {
    "," => depth(t, p + 1, seen),
    _ => {
      let n = list.len(seen)
      guard peek(t, p) == "]" else 0 - n
      guard n < 10000 else 10000
      n * 2
    },
  }
}

fn skip(t: String, p: Int, acc: List[String]) -> String = {
  let parts = acc + [peek(t, p)]
  if peek(t, p) != "," then {
    guard peek(t, p) != "" else skip("]", 0, parts)
    list.join(parts, "")
  } else skip(t, p + 1, parts)
}

fn show(r: Result[(List[Int], Int), String]) -> String = match r {
  ok((xs, n)) => "ok " + int.to_string(list.len(xs)) + " " + int.to_string(n),
  err(e) => "err " + e,
}

effect fn main() -> Unit = {
  for s in ["a,b", "a]", "ax", "", "a,,]", "a,]", "a,x,]"] {
    println(show(items_of(s, 1, [7])))
    println(int.to_string(depth(s, 1, [])))
    println(skip(s, 1, []))
  }
  let long = string.repeat(",", 8000)
  println(show(items_of(long + "]", 0, [])))
  println(show(items_of(long + "x", 0, [])))
  println(int.to_string(depth(long + "]", 0, [])))
  println(int.to_string(depth(long + long + "]", 0, [])))
  println(int.to_string(string.len(skip(long, 0, []))))
}
"##;
const SHAPES_EXPECTED: &str = "err expected ']' at 2\n-2\n,b\nok 2 2\n2\n]\nerr expected ']' at 1\n-1\nx\nerr expected ']' at 1\n-1\n]\nok 3 4\n6\n,,]\nok 2 3\n4\n,]\nerr expected ']' at 2\n-2\n,x\nok 8000 8001\nerr expected ']' at 8000\n16002\n10000\n8001\n";

const PARSER: &str = r##"fn peek(t: String, p: Int) -> String = string.slice(t, p, p + 1)
fn key(t: String, p: Int) -> Result[(List[String], Int), String] =
  if peek(t, p) == "k" then ok((["k" + int.to_string(p)], p + 1)) else err("no key at " + int.to_string(p))
fn val(t: String, p: Int) -> Result[(Int, Int), String] =
  if peek(t, p) == "v" then ok((p * 10, p + 1)) else err("no value at " + int.to_string(p))
fn doc(t: String, p: Int, root: List[String], tbl: List[String], aot: Bool) -> Result[List[String], String] = {
  if p >= string.len(t) then ok(root)
  else {
    let c = peek(t, p)
    if c == "[" and peek(t, p + 1) == "[" then {
      let (path, np) = key(t, p + 2)!
      doc(t, np, root + ["aot"], path, true)
    }
    else if c == "[" then {
      let (path, np) = key(t, p + 1)!
      doc(t, np, root, path, false)
    }
    else {
      let (keys, p3) = key(t, p)!
      guard peek(t, p3) == "=" else err("expected '=' at " + int.to_string(p3))
      let (v, p5) = val(t, p3 + 1)!
      let new_root = if aot then root + tbl + keys + ["a" + int.to_string(v)]
                     else root + tbl + keys + [int.to_string(v)]
      doc(t, p5, new_root, tbl, aot)
    }
  }
}
effect fn main() -> Unit = {
  for s in ["", "k=v", "k=vk=v", "[kk=v", "[[kk=vk=v", "kv", "k=x", "x", "[x", "[[kk=v[kk=vk", "k=vk=vk=vk=vk:v"] {
    match doc(s, 0, [], [], false) {
      ok(xs) => println("ok " + list.join(xs, ",")),
      err(e) => println("err " + e),
    }
  }
}
"##;
const PARSER_EXPECTED: &str = "ok \nok k0,20\nok k0,20,k3,50\nok k1,k2,40\nok aot,k2,k3,a50,k2,k6,a80\nerr expected '=' at 1\nerr no value at 2\nerr no key at 0\nerr no key at 1\nerr expected '=' at 12\nerr expected '=' at 13\n";

/// Both legs print `expected`; the native one is the certified render, held
/// to the ownership certificate with no fallback to the standard codegen.
fn both_legs_print(program: &str, expected: &str) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("prog.almd");
    std::fs::write(&file, program).expect("source");
    for extra in [&[][..], &["--target", "wasm"][..]] {
        let out = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).args(extra)
            .env("ALMIDE_CERTIFY_OWNERSHIP", "fail").env("ALMIDE_VERBOSE", "1")
            .current_dir(dir.path()).output().expect("almide run");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(0), "{extra:?}: {stderr}");
        assert!(!stderr.contains("walled"), "{extra:?} fell back to the standard codegen: {stderr}");
        assert_eq!(String::from_utf8_lossy(&out.stdout), expected, "{extra:?}");
    }
}

#[test]
fn a_tail_guard_delivers_its_else_from_the_loop_on_both_legs() {
    both_legs_print(SHAPES, SHAPES_EXPECTED);
}

#[test]
fn a_bare_err_else_between_propagating_calls_leaves_the_loop_on_both_legs() {
    both_legs_print(PARSER, PARSER_EXPECTED);
}
