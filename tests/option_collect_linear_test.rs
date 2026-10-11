//! #3543: `option.collect` folded with `some(vals) => some(vals + [v])`, a
//! binder read out of the accumulator that could not move, so every step
//! copied the list built so far — O(n²) on both legs, and through it
//! `option.collect_map`. It now checks for a `none`, then gathers the values
//! once.
//!
//! The expected output is the pre-change compiler's:
//! - empty input; all `some`; a `none` first, in the middle or last;
//! - String and List payloads;
//! - `collect_map`;
//! - a 50,000-element input.
//!
//! On wasm the run must stay linear in allocation: the quadratic form
//! allocated the sum of every intermediate list, about 10 GB at this size.
use std::process::Command;

const SOURCE: &str = r#"effect fn main() -> Unit = {
  let e: List[Int?] = []
  println("${option.collect(e)} ${option.collect([some(1), some(2)])} ${option.collect([some(1), none, some(3)])} ${option.collect([none, some(1)])}")
  let ss: List[String?] = [some("a" + "b"), some("c")]
  println("${option.collect(ss)} ${ss}")
  let ls: List[List[Int]?] = [some([1]), some([2, 3])]
  println("${option.collect(ls)}")
  println("${option.collect_map([1, 2, 3], (x) => if x > 0 then some(x * 2) else none)} ${option.collect_map([1, -2], (x) => if x > 0 then some(x) else none)}")
  let big = list.range(0, 50000) |> list.map((i) => some(i))
  println("${list.len(option.collect(big) ?? [])}")
}
"#;

const EXPECTED: &str = r#"some([]) some([1, 2]) none none
some(["ab", "c"]) [some("ab"), some("c")]
some([[1], [2, 3]])
some([2, 4, 6]) none
50000
"#;

#[test]
fn option_collect_keeps_its_values_and_allocates_linearly() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("collect.almd");
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
        bytes < 50_000_000,
        "option.collect allocated {bytes} bytes — quadratic again? {counts}"
    );
}
