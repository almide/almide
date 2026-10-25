//! #3544: on wasm, `map.from_list` scanned the map built so far for every
//! pair and copy-grew it on every append — O(n²), 3.2 GB allocated for the
//! 30,000 pairs below. It now builds through the in-place `map.set` window
//! (amortized growth, the key index) over the fresh block it owns.
//!
//! The expected output is the pre-change compiler's:
//! - empty input; a duplicate key, where the last value wins but the first
//!   position is kept;
//! - shared String keys with List values;
//! - tuple, Bool and Int keys; Option values;
//! - entry order.
//!
//! Native and wasm must both print it, wasm must free everything, and wasm
//! must allocate linearly.
use std::process::Command;

const SOURCE: &str = r#"effect fn main() -> Unit = {
  let e: List[(String, Int)] = []
  println("${map.from_list(e)} ${map.from_list([("a", 1), ("b", 2), ("a", 3), ("c", 4), ("b", 5)])}")
  let s = "sh" + "ared"
  let ps = [(s, [1]), ("x", [2, 3]), (s, [4])]
  let m = map.from_list(ps)
  println("${m} ${ps} ${map.get(m, s)}")
  println("${map.from_list([((1, "p"), "t1"), ((2, "q"), "t2"), ((1, "p"), "t3")])}")
  println("${map.from_list([(true, 1.5), (false, 2.5), (true, 0.5)])} ${map.from_list([(1, none), (2, some("z"))])}")
  let big = list.range(0, 30000) |> list.map((i) => ("k${i % 20000}", i))
  let mb = map.from_list(big)
  println("${map.len(mb)} ${map.get(mb, "k5") ?? -1} ${map.get(mb, "k19999") ?? -1}")
  var acc = 0
  for (k, v) in map.to_list(map.from_list([("z", 1), ("y", 2), ("x", 3)])) { acc = acc * 10 + v }
  println("${acc}")
}
"#;

const EXPECTED: &str = r#"[:] ["a": 3, "b": 5, "c": 4]
["shared": [4], "x": [2, 3]] [("shared", [1]), ("x", [2, 3]), ("shared", [4])] some([4])
[(1, "p"): "t3", (2, "q"): "t2"]
[true: 0.5, false: 2.5] [1: none, 2: some("z")]
20000 20005 19999
123
"#;

#[test]
fn map_from_list_builds_in_place_and_keeps_its_entries() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("from_list.almd");
    std::fs::write(&file, SOURCE).expect("source");
    let run = |wasm: bool| {
        let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
        c.arg("run").arg(&file);
        if wasm {
            c.args(["--target", "wasm"])
                .env("ALMIDE_WASM_ALLOC_COUNT", "1");
        }
        let out = c.output().expect("almide run");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        (
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    };
    assert_eq!(run(false).0, EXPECTED, "native");
    let (wasm, counts) = run(true);
    assert_eq!(wasm, EXPECTED, "wasm");
    let field = |k: &str| -> u64 {
        counts
            .split_whitespace()
            .find_map(|w| w.strip_prefix(k)?.parse().ok())
            .unwrap_or_else(|| panic!("no {k} in {counts}"))
    };
    assert_eq!(field("live="), 0, "{counts}");
    let bytes = field("bytes=");
    assert!(
        bytes < 40_000_000,
        "map.from_list allocated {bytes} bytes — quadratic again? {counts}"
    );
}
