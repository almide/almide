//! #3538: on wasm, `option.zip` and `result.partition` copied each payload
//! out of their inputs into a new block without its credit. A temporary input
//! (`option.zip(string.get(s, 0), …)`, `result.partition(list.map(…))`) then
//! freed the payload under the tuple / lists — the printed strings were
//! garbage — and a let-bound input was released twice.
//!
//! The program covers temporary and let-bound inputs, handle payloads (String,
//! List) in either position, and `none`. The expected output is native's; the
//! wasm leg must print the same and leave nothing live.
use std::process::Command;

const SOURCE: &str = r#"fn mk(i: Int) -> Result[String, String] = if i % 2 == 0 then ok("ok" + int.to_string(i)) else err("e" + int.to_string(i))

effect fn main() -> Unit = {
  println("${option.zip(string.get("Hello", 0), some(false))}")
  println("${option.zip(some(1), string.get("Hello", 4))}")
  println("${option.zip(string.first("ab"), string.last("cd"))}")
  println("${option.zip(list.first([[1, 2]]), some(2))}")
  let g = string.get("Hello", 1)
  let z = option.zip(g, g)
  println("${z} ${g}")
  let n: String? = none
  println("${option.zip(n, string.get("x", 0))}")
  let (a, b) = result.partition(list.map([0, 1, 2, 3], mk))
  println("${a} ${b}")
  let rs: List[Result[List[Int], String]] = [ok([1, 2]), err("x" + "y"), ok([3])]
  let (c, d) = result.partition(rs)
  println("${c} ${d} ${rs}")
}
"#;

const EXPECTED: &str = r#"some(("H", false))
some((1, "o"))
some(("a", "d"))
some(([1, 2], 2))
some(("e", "e")) some("e")
none
["ok0", "ok2"] ["e1", "e3"]
[[1, 2], [3]] ["xy"] [ok([1, 2]), err("xy"), ok([3])]
"#;

fn run(dir: &std::path::Path, wasm: bool) -> (String, String) {
    let file = dir.join("payload_share.almd");
    std::fs::write(&file, SOURCE).expect("source");
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(&file);
    if wasm {
        c.args(["--target", "wasm"]).env("ALMIDE_WASM_ALLOC_COUNT", "1");
    }
    let out = c.output().expect("almide run");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn zip_and_partition_payloads_survive_their_inputs_on_both_legs() {
    let dir = tempfile::tempdir().expect("scratch");
    let (native, _) = run(dir.path(), false);
    let (wasm, counts) = run(dir.path(), true);
    assert_eq!(native, EXPECTED, "native");
    assert_eq!(wasm, EXPECTED, "wasm (a garbage string is a payload freed under its tuple)");
    let live = counts
        .split_whitespace()
        .find_map(|w| w.strip_prefix("live=")?.parse::<u64>().ok())
        .unwrap_or_else(|| panic!("no live= in {counts}"));
    assert_eq!(live, 0, "{counts}");
}
