//! #3782: a `fan.race` whose winner is not Int or String compiled on neither
//! target — the lowering's dead placeholder for a losing arm was `0` for every
//! type but String, and a Float region spliced through the v1 render as `i64`.
//! The winner now rides a `Result[T, String]` slot (no placeholder of `T`),
//! so every winner type works; this pins Float, List and record winners, the
//! block, mapper, budgeted and `??` spellings, on native and wasm.
use std::process::Command;

const SOURCE: &str = r#"type P = { x: Float, tag: String }

fn fl(x: Float) -> Float = x * 2.0
fn heavy(n: Int) -> Int = {
  var i = 0
  var acc = 0
  while i < n { acc = (acc + i) % 1000; i = i + 1 }
  acc
}
fn costly(n: Int, v: Float) -> Float = float.from_int(heavy(n)) * 0.0 + v

effect fn main() -> Unit = {
  // plain Float arms; the cheaper second arm wins
  let a = fan.race { costly(500, 1.5), costly(5, 2.5) } ?? 0.0
  // Result Float arms, an Err arm disqualified
  let b = fan.race { err("no"), ok(fl(1.25)) } ?? 0.0   // literal Result arms: the open error type closes to String
  // List winner
  let c = fan.race { [1, 2, 3], [4, 5] } ?? []
  // record winner
  let d = fan.race { { x: 1.5, tag: "one" }, { x: 2.5, tag: "two" } } ?? { x: 0.0, tag: "none" }
  // mapper form, Float winner (all elements cost the same: the first, 3.0 → 6; an integral Float prints without `.0`)
  let e = fan.race([3.0, 1.0, 2.0], (v) => ok(fl(v))) ?? 0.0
  // budgeted: every arm over budget → the fallback
  let f = fan.race(compute.ns(3)) { costly(500, 9.0), costly(600, 8.0) } ?? -1.0
  // the non-fused Result
  let g: Result[List[Int], String] = fan.race { [7], [8, 9] }
  println("${a} ${b} ${c} ${d.tag} ${d.x} ${e} ${f} ${g}")
}
"#;

fn run(file: &std::path::Path, wasm: bool) -> String {
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(file);
    if wasm {
        c.args(["--target", "wasm"]);
    }
    let out = c.output().expect("almide");
    assert!(out.status.success(), "wasm={wasm}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn race_winners_of_every_type_compile_and_agree_across_targets() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("race_types.almd");
    std::fs::write(&file, SOURCE).expect("source");
    let native = run(&file, false);
    assert_eq!(native, "2.5 2.5 [1, 2, 3] one 1.5 6 -1 ok([7])\n");
    assert_eq!(run(&file, true), native, "native vs wasm");
}
