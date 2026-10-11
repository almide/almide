//! #2044: `fan` runs in parallel on native when its callback returns, takes
//! or captures heap values: `String`, and `List` / `Set` / `Map` / `Result` of
//! Send-safe types. Before, only scalars, tuples, records and `Option` of them
//! qualified, for two reasons:
//! - `String` was excluded ahead of ADR-0013's refcounted repr, although the
//!   native `String` is a Rust `String`;
//! - a lambda capturing a heap value is wrapped by CaptureClone in a
//!   `{ let k2 = k; … }` prologue, which the routing did not see through.
//!
//! So every text-shaped fan ran sequentially (measured 0.088 s parallel vs
//! 0.088 s `ALMIDE_FAN_SEQUENTIAL=1` before; 0.024 s vs 0.088 s after, 8
//! tasks on 14 cores). This test pins the routing in the generated Rust, and
//! that the parallel output is byte-identical to the sequential run and wasm.
//! It times nothing, so it cannot flake.
//!
//! When ADR-0013 stage 2 gives `String` a non-atomic shared buffer, it must
//! take the thread-shared flip the 2026-09-08 ruling chose rather than drop
//! `String` from the Send-safe set; this test will say so.
use std::process::Command;

const SOURCE: &str = r#"fn spin(seed: Int) -> Int = list.range(0, 200000) |> list.fold(seed, (a, i) => (a * 31 + i) % 1000003)

effect fn label(prefix: String, xs: List[String]) -> List[String] =
  fan.map(xs, (x) => ok(prefix + x + int.to_string(spin(string.len(x)))))!

effect fn main() -> Unit = {
  let words = ["alpha", "be", "gamma", "", "delta"]
  let tag = "t:"
  let shared = ["s1", "s3"]
  let a = fan.map(words, (w) => ok(tag + string.to_upper(w)))!
  let b = fan.map(list.range(0, 4), (i) => ok(list.map(shared, (s) => s + int.to_string(i))))!
  let c = fan.map(list.range(0, 3), (i) => ok(map.set(map.new(), "k${i}", i * 2)))!
  let d = fan.map([3, 1, 3], (i) => ok(set.from_list([i, i + 1])))!
  let e = fan.map(words, (w) => ok(if w == "" then err("empty") else ok(string.len(w))))!
  let f = fan {
    list.map(words, (w) => w + "!")
  }
  println("${a}")
  println("${b}")
  println("${c}")
  println("${d}")
  println("${e}")
  println("${f}")
  println("${label("p-", words)!}")
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
fn heap_shaped_fans_route_to_the_parallel_runtime_and_print_what_the_sequential_run_prints() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("fan_heap.almd");
    std::fs::write(&file, SOURCE).expect("source");
    let rust = almide(&["--target", "rust"], &file, None);
    let body = rust.split("pub fn label").nth(1).expect("label is emitted");
    // label's fan.map and main's a..e are `fan.map`; f is `list.map` in a `fan` block.
    assert_eq!(body.matches("almide_rt_fan_map_par(").count(), 6, "every fan.map routes parallel:\n{body}");
    assert!(!body.contains("almide_rt_fan_map("), "a fan.map stayed sequential:\n{body}");
    assert!(body.contains("almide_rt_list_par_map("), "the fan block's list.map routes parallel");
    let parallel = almide(&["run"], &file, None);
    let sequential = almide(&["run"], &file, Some(("ALMIDE_FAN_SEQUENTIAL", "1")));
    let wasm = almide(&["run", "--target", "wasm"], &file, None);
    assert_eq!(parallel, sequential, "parallel vs sequential");
    assert_eq!(parallel, wasm, "native vs wasm");
}
