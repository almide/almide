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
    let native = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("run")
        .arg(&file)
        .output()
        .expect("native run");
    assert!(
        native.status.success(),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&native.stdout), EXPECTED, "native");
    // Warm the wasm build cache so only the run is timed: `check` builds no
    // wasm, and a cold build of the program (seconds on a loaded runner) once
    // timed past the bound on develop with set.map itself still linear.
    let warm = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("run")
        .arg(&file)
        .args(["--target", "wasm"])
        .output()
        .expect("warm wasm run");
    assert!(
        warm.status.success(),
        "{}",
        String::from_utf8_lossy(&warm.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("run")
        .arg(&file)
        .args(["--target", "wasm"])
        .env("ALMIDE_WASM_ALLOC_COUNT", "1")
        .output()
        .expect("wasm run");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), EXPECTED, "wasm");
    let counts = String::from_utf8_lossy(&run.stderr);
    assert!(
        counts.contains("live=0"),
        "wasm must free everything: {counts}"
    );

    // The bound is RELATIVE: the same program at a quarter of the sizes, timed
    // interleaved on the same runner. Linear work grows ~4x, quadratic ~16x;
    // an absolute bound (4 s) failed a merge-queue run at 7.4 s on a loaded
    // runner while the same build ran it in 0.18 s locally.
    let quarter = dir.path().join("set_map_quarter.almd");
    std::fs::write(&quarter, quarter_source()).expect("quarter source");
    let warm_q = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("run")
        .arg(&quarter)
        .args(["--target", "wasm"])
        .output()
        .expect("warm quarter");
    assert!(
        warm_q.status.success(),
        "{}",
        String::from_utf8_lossy(&warm_q.stderr)
    );
    let time = |f: &std::path::Path| {
        let t = Instant::now();
        let out = Command::new(env!("CARGO_BIN_EXE_almide"))
            .arg("run")
            .arg(f)
            .args(["--target", "wasm"])
            .output()
            .expect("timed run");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        t.elapsed()
    };
    let (mut full, mut small) = (Duration::MAX, Duration::MAX);
    for _ in 0..3 {
        small = small.min(time(&quarter));
        full = full.min(time(&file));
    }
    assert!(
        full < small * 8,
        "wasm set.map at 400,000 members took {full:?} against {small:?} at a quarter of the size — quadratic again?"
    );
}

/// SOURCE with every collection size divided by four.
fn quarter_source() -> String {
    SOURCE
        .replace("300000", "75000")
        .replace("200000", "50000")
        .replace("199999", "49999")
        .replace("100000", "25000")
        .replace("70000", "17500")
}
