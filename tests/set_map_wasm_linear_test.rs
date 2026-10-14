//! #3552 / #3704 family: on wasm, `set.map` scanned the result set built so
//! far for every mapped member — O(n²), 9.7 s for the 400,000 members
//! below. An Int or String result member now dedups through the key index
//! while the result grows in place (the result is a Set, so its own drop
//! retires the index); a Bool or Float member keeps the scan.
use std::process::Command;
use std::time::{Duration, Instant};

const SOURCE: &str = r#"effect fn main() -> Unit = {
  let s = set.from_list([1, 2, 3, 4, 5, 6])
  println("${set.map(s, (x) => x % 3)} ${set.map(s, (x) => "v${x % 2}")} ${set.map(s, (x) => x > 3)}")
  println("${set.map(set.from_list([1, 2]), (x) => int.to_float(x) / 2.0)}")
  let big = list.range(0, 300000) |> set.from_list
  let m = set.map(big, (x) => x % 200000)
  let ms = set.map(set.from_list(list.range(0, 100000)), (x) => "k${x % 70000}")
  println("${set.len(m)} ${set.contains(m, 199999)} ${set.contains(m, 200000)} ${set.len(ms)} ${set.contains(ms, "k5")}")
}
"#;

const EXPECTED: &str = r#"set.from_list([1, 2, 0]) set.from_list(["v1", "v0"]) set.from_list([false, true])
set.from_list([0.5, 1])
200000 true false 70000 true
"#;

#[test]
fn set_map_is_linear_on_wasm_and_keeps_its_answers() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("set_map.almd");
    std::fs::write(&file, SOURCE).expect("source");
    let native = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).output().expect("native run");
    assert!(native.status.success(), "{}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout), EXPECTED, "native");
    // Warm the build cache so only the wasm run is timed.
    let warm = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("check")
        .arg(&file)
        .output()
        .expect("check");
    assert!(warm.status.success(), "{}", String::from_utf8_lossy(&warm.stderr));
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
    assert!(took < Duration::from_secs(4), "wasm set.map took {took:?} for 400,000 members — quadratic again?");
}
