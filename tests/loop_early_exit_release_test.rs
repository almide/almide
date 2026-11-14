//! #4648: an early exit inside a `for` / `while` body releases the block a
//! body `let` still holds from an earlier pass. The body's heap locals are
//! frame credits (the next pass's rebind releases the previous occupant), so
//! an error or guard return lowered before the bind — `let m = mk(i)!` exits
//! inside its own initializer — left that block live on the wasm leg.
//! Native and wasm print the same; the wasm leg must end with nothing live.

use std::process::Command;

const FOR_BANG: &str = r#"
effect fn mk(n: Int) -> Result[List[Int], String] = if n > 3 then err("big ${n}") else ok([n, n])

effect fn run(k: Int) -> Result[Int, String] = {
  var n = 0
  for i in 0..<k {
    let m = mk(i)!
    n = n + list.len(m)
  }
  ok(n)
}

effect fn main() -> Unit = {
  println(match run(3) { ok(n) => "ok ${n}", err(e) => "err ${e}" })
  println(match run(6) { ok(n) => "ok ${n}", err(e) => "err ${e}" })
}
"#;

const WHILE_BANG: &str = r#"
effect fn mk(n: Int) -> Result[String, String] = if n > 2 then err("stop ${n}") else ok("v${n}")

effect fn run() -> Result[Int, String] = {
  var i = 0
  var total = 0
  while i < 10 {
    let s = mk(i)!
    total = total + string.len(s)
    i = i + 1
  }
  ok(total)
}

effect fn main() -> Unit = {
  println(match run() { ok(n) => "ok ${n}", err(e) => "err ${e}" })
}
"#;

const GUARD_RETURN: &str = r#"
fn run(xs: List[Int]) -> Result[Int, String] = {
  var n = 0
  for x in xs {
    guard x < 5 else err("big ${x}")
    let m = [x, x, x]
    n = n + list.len(m)
  }
  ok(n)
}

effect fn main() -> Unit = {
  println(match run([1, 2, 9]) { ok(n) => "ok ${n}", err(e) => "err ${e}" })
}
"#;

const NESTED: &str = r#"
effect fn mk(n: Int) -> Result[List[Int], String] = if n > 4 then err("big ${n}") else ok([n])

effect fn run() -> Result[Int, String] = {
  var n = 0
  for i in 0..<3 {
    let outer = [i, i]
    for j in 0..<3 {
      let m = mk(i * 2 + j)!
      n = n + list.len(m) + list.len(outer)
    }
  }
  ok(n)
}

effect fn main() -> Unit = {
  println(match run() { ok(n) => "ok ${n}", err(e) => "err ${e}" })
}
"#;

fn almide() -> Command {
    Command::new(env!("CARGO_BIN_EXE_almide"))
}

#[test]
fn an_early_exit_in_a_loop_body_leaves_nothing_live_on_wasm() {
    let cases = [
        ("for_bang", FOR_BANG, "ok 6\nerr big 4\n"),
        ("while_bang", WHILE_BANG, "err stop 3\n"),
        ("guard_return", GUARD_RETURN, "err big 9\n"),
        ("nested", NESTED, "err big 5\n"),
    ];
    for (name, source, expected) in cases {
        let dir = tempfile::tempdir().expect("scratch");
        let file = dir.path().join(format!("{name}.almd"));
        std::fs::write(&file, source).expect("source");

        let native = almide().arg("run").arg(&file).output().expect("native run");
        assert!(native.status.success(), "[{name}] {}", String::from_utf8_lossy(&native.stderr));
        assert_eq!(String::from_utf8_lossy(&native.stdout), expected, "[{name}] native");

        let wasm = almide()
            .arg("run")
            .arg(&file)
            .args(["--target", "wasm"])
            .env("ALMIDE_WASM_ALLOC_COUNT", "1")
            .output()
            .expect("wasm run");
        assert!(wasm.status.success(), "[{name}] {}", String::from_utf8_lossy(&wasm.stderr));
        assert_eq!(String::from_utf8_lossy(&wasm.stdout), expected, "[{name}] wasm");
        let counts = String::from_utf8_lossy(&wasm.stderr);
        let live: u64 = counts
            .split_whitespace()
            .find_map(|w| w.strip_prefix("live=")?.parse().ok())
            .unwrap_or_else(|| panic!("[{name}] no live= in {counts}"));
        assert_eq!(live, 0, "[{name}] blocks live at exit: {counts}");
    }
}
