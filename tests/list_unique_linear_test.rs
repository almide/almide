//! #3704: native `list.unique` / `list.unique_by` scanned the output built
//! so far for every element — O(n²), measured 4.756e8 → 1.216e11
//! instructions per call over n → 16n. They now collect through
//! `AlmideSet`, which hashes the exact-equality key types and keeps the
//! first of equal members, so the answers below are unchanged: Float keeps
//! `PartialEq` (`-0.0 == 0.0`, `NaN != NaN`), compound members compare
//! structurally, and order is first occurrence.
//!
//! The wasm leg is still quadratic (#3704's second half), so only the
//! small rows run there; the timed row is native.
use std::process::Command;
use std::time::{Duration, Instant};

const SMALL: &str = r#"effect fn main() -> Unit = {
  println("${list.unique([3, 1, 3, 2, 1])} ${list.unique(["b", "a", "b"])} ${list.unique([true, false, true])}")
  let z = 0.0
  let nan = z / z
  println("${list.unique([0.0, -0.0, 1.5, 1.5])} ${list.len(list.unique([nan, nan]))}")
  println("${list.unique([(1, "p"), (2, "q"), (1, "p")])} ${list.unique([[1], [2], [1]])} ${list.unique([some(1), none, some(1), none])}")
  println("${list.unique_by([1, 2, 3, 4, 5, 6], (x) => x % 3)} ${list.unique_by(["aa", "b", "cc", "d"], (s) => string.len(s))}")
}
"#;

const SMALL_EXPECTED: &str = r#"[3, 1, 2] ["b", "a"] [true, false]
[0, 1.5] 2
[(1, "p"), (2, "q")] [[1], [2]] [some(1), none]
[1, 2, 3] ["aa", "b"]
"#;

const BIG: &str = r#"effect fn main() -> Unit = {
  let big = list.range(0, 500000) |> list.map((i) => i % 400000)
  let u = list.unique(big)
  let ub = list.unique_by(big, (x) => x % 300000)
  let s = list.range(0, 200000) |> list.map((i) => "k${i % 150000}")
  println("${list.len(u)} ${list.len(ub)} ${list.first(u) ?? -1} ${list.last(u) ?? -1} ${list.len(list.unique(s))}")
}
"#;

fn run(dir: &std::path::Path, name: &str, src: &str, wasm: bool) -> String {
    let file = dir.join(name);
    std::fs::write(&file, src).expect("source");
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(&file);
    if wasm {
        c.args(["--target", "wasm"]);
    }
    let out = c.output().expect("almide run");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn list_unique_keeps_its_answers_on_both_legs() {
    let dir = tempfile::tempdir().expect("scratch");
    assert_eq!(run(dir.path(), "small.almd", SMALL, false), SMALL_EXPECTED, "native");
    assert_eq!(run(dir.path(), "small.almd", SMALL, true), SMALL_EXPECTED, "wasm");
}

#[test]
fn list_unique_is_linear_on_native() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("big.almd");
    std::fs::write(&file, BIG).expect("source");
    let exe = dir.path().join("big");
    let build = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["build"])
        .arg(&file)
        .arg("-o")
        .arg(&exe)
        .arg("--release")
        .output()
        .expect("almide build");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
    // Only the artifact is timed, not the build. Quadratic took ~20 s here.
    let t = Instant::now();
    let out = Command::new(&exe).output().expect("run artifact");
    let took = t.elapsed();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "400000 300000 0 399999 150000\n");
    assert!(
        took < Duration::from_secs(4),
        "list.unique / unique_by took {took:?} for 500,000 elements — quadratic again?"
    );
}
