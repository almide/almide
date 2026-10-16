//! #2594: a `fan.map` whose callback is an effect fn runs in parallel on
//! native when that fn's only effect is a nested fan over proven work. Before,
//! every effect callback took the sequential runtime, so a map of maps
//! (`fan.map(xs, (x) => inner(x))` with `inner` fanning out itself) ran its
//! outer elements one at a time (docs/benchmarks/fan-floor.md, `nest`).
//!
//! The fan view of the purity analysis (pass_stream_fusion `Purity::for_fan`)
//! admits an `effect fn` whose body reaches nothing but proven fns, nested
//! `fan`s and `!`. A fn that reaches any effect module call, here
//! `env.sleep_ms`, directly or through another fn, stays sequential, so the
//! order of its effects is unchanged. This test pins the routing in the
//! generated Rust, and that the parallel output is byte-identical to the
//! sequential run and to wasm. It times nothing, so it cannot flake.
use std::process::Command;

const SOURCE: &str = r#"import env

fn work(s: Int, i: Int) -> Int = list.range(0, 2000) |> list.fold(s + i, (a, j) => (a * 31 + j) % 1000003)

effect fn inner(s: Int) -> Result[Int, String] = {
  let xs = fan { list.map(list.range(0, 10), (i) => work(s, i)) }
  if s == 7 then err("seven") else ok(list.fold(xs, 0, (a, x) => (a + x) % 1000003))
}

effect fn outer(s: Int) -> Result[Int, String] = {
  let a = inner(s)!
  ok(a + 1)
}

effect fn sleepy(s: Int) -> Result[Int, String] = {
  env.sleep_ms(1)
  inner(s)
}

effect fn via_sleepy(s: Int) -> Result[Int, String] = sleepy(s)

effect fn main() -> Unit = {
  let a = fan.map(list.range(0, 6), (q) => inner(q))!
  let b = fan.map(list.range(0, 6), (q) => outer(q))!
  let c = fan.map(list.range(0, 3), (q) => sleepy(q))!
  let d = fan.map(list.range(0, 3), (q) => via_sleepy(q))!
  println("${a} ${b} ${c} ${d}")
  let e: Result[List[Int], String] = fan.map(list.range(5, 9), (q) => inner(q))
  println("${e}")
}
"#;

fn almide(args: &[&str], file: &std::path::Path, env: Option<(&str, &str)>) -> String {
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    // `run` takes the file after the subcommand; emission is `almide <file> --target rust`.
    match args.split_first() {
        Some((&"run", rest)) => c.arg("run").arg(file).args(rest),
        _ => c.arg(file).args(args),
    };
    if let Some((k, v)) = env {
        c.env(k, v);
    }
    let out = c.output().expect("almide");
    assert!(out.status.success(), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn an_effect_callback_that_only_fans_out_routes_parallel_and_one_that_sleeps_does_not() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("fan_nested.almd");
    std::fs::write(&file, SOURCE).expect("source");
    let rust = almide(&["--target", "rust"], &file, None);
    let main = rust.split("fn __almide_main").nth(1).expect("main is emitted");
    // a, b and e reach only nested fans; c and d reach env.sleep_ms.
    assert_eq!(main.matches("almide_rt_fan_map_par(").count(), 3, "a, b and e route parallel:\n{main}");
    assert_eq!(main.matches("almide_rt_fan_map(").count(), 2, "c and d stay sequential:\n{main}");
    let parallel = almide(&["run"], &file, None);
    let forced = almide(&["run"], &file, Some(("ALMIDE_FAN_COST_OFF", "1")));
    let sequential = almide(&["run"], &file, Some(("ALMIDE_FAN_SEQUENTIAL", "1")));
    let wasm = almide(&["run", "--target", "wasm"], &file, None);
    assert_eq!(parallel, sequential, "parallel vs sequential");
    assert_eq!(forced, sequential, "forced parallel vs sequential");
    assert_eq!(parallel, wasm, "native vs wasm");
}
