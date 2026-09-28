//! #2745: `break` / `continue` in the statement positions the structural wasm
//! leg lowers — `guard … else break|continue`, a statement-position `match`
//! arm, nested under `if` — in every loop form (for over a list, a range, a
//! map; while; nested loops; a loop inside a closure), each with a heap local
//! bound in the body BEFORE the jump (C-256).
//!
//! Ownership: a body's heap locals are frame credits. The jump leaves them
//! where a fall-through pass leaves them; the next pass's rebind (or the
//! epilogue) releases them. The churn loop runs 3000 passes, so a per-edge
//! leak would show as allocs > frees in the structural leg's allocation
//! counters (`ALMIDE_WASM_ALLOC_COUNT`).
//!
//! Why a Rust test and not a `spec/wasm_cross` fixture: the incumbent renderer
//! walls a break over a heap frame by design, and every `spec/` program is
//! also swept by its walled-real ratchet, which this program would grow by
//! five functions. The incumbent is being retired (#2739); the evidence lives
//! here instead of loosening that ratchet.

use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

const PROGRAM: &str = r#"type Shape = | Circle(Int) | Square(Int) | Empty

fn first_long(xs: List[String]) -> String = {
  var found = "none"
  for x in xs {
    let up = string.to_upper(x) + "!"
    guard string.len(up) > 3 else continue
    found = up
    break
  }
  found
}

fn area(ss: List[Shape]) -> Int = {
  var total = 0
  for s in ss {
    let tag = "shape:" + int.to_string(total)
    match s {
      Circle(r) => {
        if r > 100 then break
        else ()
        total = total + r * 3
      },
      Square(w) => {
        total = total + w * w
      },
      Empty => continue,
    }
    total = total + string.len(tag) - string.len(tag) + 1
  }
  total
}

fn opt_sum(xs: List[Int?]) -> Int = {
  var t = 0
  for o in xs {
    let label = "o" + int.to_string(t)
    match o {
      none => continue,
      some(v) => {
        guard v >= 0 else break
        t = t + v + string.len(label) - string.len(label)
      },
    }
  }
  t
}

fn grid(n: Int) -> List[String] = {
  var out: List[String] = []
  for i in 0..<n {
    let row = "r" + int.to_string(i)
    var j = 0
    while true {
      j = j + 1
      let cell = row + ":" + int.to_string(j)
      if j > i then break
      else ()
      if j % 2 == 0 then continue
      else ()
      out = out + [cell]
    }
    guard i < 4 else break
  }
  out
}

fn map_walk(m: Map[String, Int]) -> Int = {
  var s = 0
  for (k, v) in m {
    let kk = k + "!"
    guard v != 2 else continue
    guard v != 4 else break
    s = s + v * string.len(kk)
  }
  s
}

fn per_item(xs: List[Int]) -> List[Int] = xs |> list.map((x) => {
  var acc = 0
  for i in 0...x {
    let s = int.to_string(i)
    guard i != 2 else continue
    guard string.len(s) < 2 else break
    acc = acc + i
  }
  acc
})

fn churn(n: Int) -> Int = {
  var kept = 0
  var i = 0
  while i < n {
    i = i + 1
    let s = "item-" + int.to_string(i)
    let parts = [s, s + "?"]
    guard i % 3 != 0 else continue
    match list.get(parts, 1) {
      none => continue,
      some(p) => {
        if string.len(p) > 1000 then break
        else ()
        kept = kept + 1
      },
    }
  }
  kept
}

effect fn main() -> Unit = {
  println(first_long(["a", "bb", "ccc", "dddd"]))
  println(first_long(["a"]))
  println(int.to_string(area([Circle(2), Empty, Square(3), Circle(200), Square(9)])))
  println(int.to_string(opt_sum([some(1), none, some(5), some(-1), some(100)])))
  println(grid(7) |> list.join(","))
  println(int.to_string(map_walk([
    "a": 1,
    "bb": 2,
    "ccc": 3,
    "d": 4,
    "e": 5
  ])))
  println(per_item([0, 3, 12]) |> list.map((x) => int.to_string(x)) |> list.join(" "))
  var k = 0
  var log: List[String] = []
  while k < 10 {
    k = k + 1
    let msg = "k=" + int.to_string(k)
    guard k % 3 != 0 else continue
    if k == 8 then break
    else ()
    log = log + [msg]
  }
  println(log |> list.join(";"))
  println(int.to_string(churn(3000)))
}
"#;

const EXPECTED: &str = "CCC!\nnone\n17\n6\nr1:1,r2:1,r3:1,r3:3,r4:1,r4:3\n14\n0 4 43\nk=1;k=2;k=4;k=5;k=7\n2000\n";

#[test]
fn break_and_continue_over_heap_locals_match_native_on_the_structural_leg_without_leaking() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("loops.almd");
    std::fs::write(&src, PROGRAM).unwrap();

    let native = Command::new(almide()).current_dir(dir.path()).args(["run", "loops.almd"]).output().expect("native run");
    assert!(native.status.success(), "native run failed:\n{}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout), EXPECTED, "native output");

    let wasm = Command::new(almide())
        .current_dir(dir.path())
        .args(["run", "loops.almd", "--target", "wasm"])
        .env("ALMIDE_VERIFIED_DEBUG", "1")
        .env("ALMIDE_WASM_ALLOC_COUNT", "1")
        .output()
        .expect("wasm run");
    let stderr = String::from_utf8_lossy(&wasm.stderr);
    assert!(wasm.status.success(), "wasm run failed:\n{stderr}");
    assert!(
        stderr.contains("structural leg emitted the module"),
        "the program must lower on the structural leg, not fall back:\n{stderr}"
    );
    assert_eq!(String::from_utf8_lossy(&wasm.stdout), EXPECTED, "wasm output must be byte-identical to native");

    let line = stderr
        .lines()
        .find(|l| l.starts_with("__ALMD_WASM_ALLOC allocs="))
        .unwrap_or_else(|| panic!("no allocation counter line:\n{stderr}"));
    let field = |k: &str| -> u64 {
        line.split_whitespace()
            .find_map(|w| w.strip_prefix(k))
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| panic!("no `{k}` in {line}"))
    };
    assert_eq!(field("allocs="), field("frees="), "every allocation is released: {line}");
}
