//! #3704 (wasm half): `list.unique` / `list.unique_by` walked the output
//! built so far for every element — O(n²), 19.3 s for 320,000 elements on
//! the inspection's c4 VM. An Int or String element (or key) now dedups
//! through the key index while the output grows in place, and the output's
//! side-table record is retired before it is returned as a List; every
//! other element class keeps its equality scan, so its answers (Float
//! `-0.0 == 0.0`, `NaN != NaN`, structural compound equality) are
//! unchanged.
use std::process::Command;
use std::time::{Duration, Instant};

const SOURCE: &str = r#"effect fn main() -> Unit = {
  println("${list.unique([3, 1, 3, 2, 1])} ${list.unique(["b", "a", "b"])} ${list.unique([true, false, true])}")
  let z = 0.0
  let nan = z / z
  println("${list.unique([0.0, -0.0, 1.5, 1.5])} ${list.len(list.unique([nan, nan]))}")
  println("${list.unique([(1, "p"), (2, "q"), (1, "p")])} ${list.unique([[1], [2], [1]])}")
  println("${list.unique_by([1, 2, 3, 4, 5, 6], (x) => x % 3)} ${list.unique_by(["aa", "b", "cc", "d"], (s) => string.len(s))}")
  let s = "sh" + "ared"
  let xs = [s, "x", s, "y"]
  let u = list.unique(xs)
  println("${u} ${xs} ${list.unique(list.range(0, 40) |> list.map((i) => i % 17))}")
  let big = list.range(0, 300000) |> list.map((i) => i % 200000)
  let bu = list.unique(big)
  let bb = list.unique_by(big, (x) => x % 150000)
  let bs = list.range(0, 100000) |> list.map((i) => "k${i % 70000}")
  println("${list.len(bu)} ${list.len(bb)} ${list.first(bu) ?? -1} ${list.last(bu) ?? -1} ${list.len(list.unique(bs))}")
}
"#;

const EXPECTED: &str = r#"[3, 1, 2] ["b", "a"] [true, false]
[0, 1.5] 2
[(1, "p"), (2, "q")] [[1], [2]]
[1, 2, 3] ["aa", "b"]
["shared", "x", "y"] ["shared", "x", "shared", "y"] [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
200000 150000 0 199999 70000
"#;

#[test]
fn list_unique_is_linear_on_wasm_and_keeps_its_answers() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("unique.almd");
    std::fs::write(&file, SOURCE).expect("source");
    let native = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).output().expect("native run");
    assert!(native.status.success(), "{}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout), EXPECTED, "native");
    // Build first so only the wasm run is timed. Quadratic took ~15 s here.
    let wasm = dir.path().join("unique.wasm");
    let build = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["build"])
        .arg(&file)
        .args(["--target", "wasm", "-o"])
        .arg(&wasm)
        .output()
        .expect("wasm build");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
    let t = Instant::now();
    let run = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("run")
        .arg(&file)
        .args(["--target", "wasm"])
        .env("ALMIDE_WASM_ALLOC_COUNT", "1")
        .output()
        .expect("wasm run");
    let took = t.elapsed();
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(String::from_utf8_lossy(&run.stdout), EXPECTED, "wasm");
    let counts = String::from_utf8_lossy(&run.stderr);
    assert!(counts.contains("live=0"), "wasm must free everything: {counts}");
    assert!(
        took < Duration::from_secs(5),
        "wasm list.unique / unique_by took {took:?} for 300,000 elements — quadratic again?"
    );
}
