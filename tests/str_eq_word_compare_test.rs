//! wasm `$str_eq` compares strings eight bytes at a time, then the tail
//! byte by byte. Every length from 0 to 40, a difference at every position
//! (in the word part and in the tail), equal lengths with equal contents,
//! different lengths and multi-byte text must answer what native answers.
use std::process::Command;

const SOURCE: &str = r##"fn flip(s: String, at: Int) -> String =
  string.slice(s, 0, at) + "#" + string.slice(s, at + 1, string.len(s))

effect fn main() -> Unit = {
  let base = "abcdefghijklmnopqrstuvwxyz0123456789ABCD"
  let lens = list.range(0, 41)
  let same = lens |> list.map((n) => string.slice(base, 0, n) == string.slice(base + "", 0, n))
  let diff = lens |> list.flat_map((n) => list.range(0, n) |> list.map((at) => if flip(string.slice(base, 0, n), at) == string.slice(base, 0, n) then 1 else 0))
  let longer = lens |> list.map((n) => string.slice(base, 0, n) == string.slice(base, 0, n) + "x")
  println("${list.all(same, (b) => b)} ${list.sum(diff)} ${list.len(diff)} ${list.any(longer, (b) => b)}")
  println("${"héllo wörld, ünïcödé!" == "héllo wörld, ünïcödé!"} ${"héllo wörld, ünïcödé!" == "héllo wörld, ünïcödë!"}")
  let k = "k100019999"
  let keys = list.range(0, 20000) |> list.map((i) => "k${100000000 + i}")
  println("${list.find(keys, (x) => x == k)} ${list.count(keys, (x) => x == "k100000000")}")
}
"##;

const EXPECTED: &str = "true 0 820 false\ntrue false\nsome(\"k100019999\") 1\n";

#[test]
fn string_equality_agrees_on_both_legs_at_every_length_and_position() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("streq.almd");
    std::fs::write(&file, SOURCE).expect("source");
    for wasm in [false, true] {
        let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
        c.arg("run").arg(&file);
        if wasm {
            c.args(["--target", "wasm"]);
        }
        let out = c.output().expect("almide run");
        assert!(out.status.success(), "wasm={wasm}: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout), EXPECTED, "wasm={wasm}");
    }
}
