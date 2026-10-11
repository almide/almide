//! #3534: wasm `list.flatten` concatenated each inner list onto a growing
//! result, re-copying it every time — 800,000 one-element lists took 32.5 s
//! on wasm while native was linear. It now sums the inner lengths, allocates
//! once and copies each inner payload in order.
//!
//! The pin is the byte count the embedded host's allocation counter reports,
//! not a time: the quadratic form allocated the sum of every intermediate
//! result (~2·n² bytes of i64 slots), the linear form the inner lists, the
//! result and nothing else.
use std::process::Command;

const SOURCE: &str = r#"type P = { x: Int }
effect fn main() -> Unit = {
  println("${list.flatten([[1, 2], [], [3], [4, 5, 6]])}")
  let e: List[List[Int]] = []
  println("${list.flatten(e)}")
  let s = "shared"
  println("${list.flatten([["a", s], [s], []])} ${s}")
  println("${list.flatten([[[1], [2]], [[3]]])}")
  println("${list.flatten([[1.5], [2.25, 3.0]])}")
  println("${list.flatten([[{ x: 1 }], [{ x: 2 }, { x: 3 }]])}")
  let inner = [7, 8]
  println("${inner} ${list.flatten([inner, inner, [9]])}")
  let big = list.range(0, 200000) |> list.map((i) => [i])
  let flat = list.flatten(big)
  println("${list.len(flat)} ${list.get(flat, 199999) ?? -1}")
}
"#;

fn run(dir: &std::path::Path, wasm: bool) -> (String, String) {
    let file = dir.join("flatten.almd");
    std::fs::write(&file, SOURCE).expect("source");
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(&file);
    if wasm {
        c.args(["--target", "wasm"]).env("ALMIDE_WASM_ALLOC_COUNT", "1");
    }
    let out = c.output().expect("almide run");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    (String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn wasm_flatten_is_linear_and_agrees_with_native() {
    let dir = tempfile::tempdir().expect("scratch");
    let (native, _) = run(dir.path(), false);
    let (wasm, counts) = run(dir.path(), true);
    assert_eq!(wasm, native, "wasm and native disagree");
    assert!(native.ends_with("200000 199999\n"), "{native}");
    let field = |k: &str| -> u64 {
        counts
            .split_whitespace()
            .find_map(|w| w.strip_prefix(k)?.parse().ok())
            .unwrap_or_else(|| panic!("no {k} in {counts}"))
    };
    assert_eq!(field("live="), 0, "{counts}");
    // 200,000 inner blocks (~24 B each), the outer spine and the result:
    // well under 16 MB. The quadratic form allocated ~80 GB.
    let bytes = field("bytes=");
    assert!(bytes < 16_000_000, "flatten allocated {bytes} bytes — quadratic again? {counts}");
}
