//! #3552: on wasm, building a Set copied it on every step. `set.insert`
//! always took the functional copy-grow, even when its receiver died at the
//! call (`w = set.insert(w, x)`, a fold accumulator). `set.from_list` scanned
//! the set built so far linearly and copy-grew it on every new member. The
//! program below allocated 4.0 GB. Now an owned receiver, and the fresh block
//! `from_list` builds, take the in-place window: the block grows in place when
//! it is held only once, and the key index follows the appends.
//!
//! The expected output is the pre-change compiler's:
//! - empty input; duplicates, where the first position is kept;
//! - shared String members; tuple and Bool members;
//! - an alias taken before an insert keeps the old members (value semantics);
//! - member order.
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
  var w: Set[String] = set.new()
  var i = 0
  while i < 30000 {
    w = set.insert(w, "k${i % 20000}")
    i = i + 1
  }
  let alias = w
  w = set.insert(w, "fresh")
  println("${set.len(w)} ${set.len(alias)} ${set.contains(w, "k5")} ${set.contains(alias, "fresh")}")
  let f = list.range(0, 30000) |> list.fold(set.new(), (acc, j) => set.insert(acc, j % 20000))
  let big = list.range(0, 30000) |> list.map((j) => j % 20000) |> set.from_list
  println("${set.len(f)} ${set.len(big)} ${set.contains(big, 19999)} ${set.to_list(set.from_list([9, 8, 9, 7]))}")
}
"#;

const EXPECTED: &str = r#"set.from_list([]) set.from_list([3, 1, 2])
set.from_list(["shared", "x", "y"]) ["shared", "x", "shared", "y"] true
set.from_list([(1, "p"), (2, "q")]) set.from_list([true, false])
20001 20000 true false
20000 20000 true [9, 8, 7]
"#;

#[test]
fn a_set_builds_in_place_and_keeps_its_members() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("set_build.almd");
    std::fs::write(&file, SOURCE).expect("source");
    let run = |wasm: bool| {
        let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
        c.arg("run").arg(&file);
        if wasm {
            c.args(["--target", "wasm"]).env("ALMIDE_WASM_ALLOC_COUNT", "1");
        }
        let out = c.output().expect("almide run");
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        (String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
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
    assert!(bytes < 40_000_000, "building the sets allocated {bytes} bytes — quadratic again? {counts}");
}
