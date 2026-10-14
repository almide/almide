//! #3552: on wasm, `set.from_list` scanned the set built so far for every
//! element and copy-grew it on every append — O(n²), 2.4 GB allocated for the
//! 60,000 elements below. It now probes through the key index and appends in
//! place with amortized growth over the fresh block it owns, as
//! `map.from_list` does since #3544.
//!
//! The expected output is the pre-change compiler's:
//! - empty input; duplicates, where the first position is kept;
//! - shared String elements;
//! - tuple and Bool elements;
//! - Int and String sets past the index threshold;
//! - element order.
//!
//! Native and wasm must both print it, wasm must free everything, and wasm
//! must allocate linearly.
use std::process::Command;

const SOURCE: &str = r#"effect fn main() -> Unit = {
  let e: List[Int] = []
  println("${set.from_list(e)} ${set.from_list([3, 1, 3, 2, 1])}")
  let s = "sh" + "ared"
  let xs = [s, "x", s, "y"]
  let ss = set.from_list(xs)
  println("${ss} ${xs} ${set.contains(ss, s)}")
  println("${set.from_list([(1, "p"), (2, "q"), (1, "p")])} ${set.from_list([true, false, true])}")
  let big = list.range(0, 30000) |> list.map((i) => i % 20000)
  let sb = set.from_list(big)
  println("${set.len(sb)} ${set.contains(sb, 5)} ${set.contains(sb, 19999)} ${set.contains(sb, 20000)}")
  let bigs = list.range(0, 30000) |> list.map((i) => "k${i % 20000}")
  let sbs = set.from_list(bigs)
  println("${set.len(sbs)} ${set.contains(sbs, "k7")} ${set.contains(sbs, "k20000")}")
  var acc = 0
  for v in set.to_list(set.from_list([3, 2, 1, 3])) { acc = acc * 10 + v }
  println("${acc}")
}
"#;

const EXPECTED: &str = r#"set.from_list([]) set.from_list([3, 1, 2])
set.from_list(["shared", "x", "y"]) ["shared", "x", "shared", "y"] true
set.from_list([(1, "p"), (2, "q")]) set.from_list([true, false])
20000 true true false
20000 true false
321
"#;

#[test]
fn set_from_list_builds_in_place_and_keeps_its_members() {
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
        "set.from_list allocated {bytes} bytes — quadratic again? {counts}"
    );
}
