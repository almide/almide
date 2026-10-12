//! `string.take_end` / `drop_end` find their cut by walking back from the end
//! of the string, and wasm `string.ends_with` compares the suffix's bytes in
//! place. The stdlib inspection measured all three as O(n) per call where
//! O(k) or O(|suffix|) is the work (GCE c4-standard-4, develop 595a37476):
//! - native `take_end` collected every char into a `Vec<char>` (4 bytes per
//!   char) and wasm counted every codepoint from the start;
//! - wasm `ends_with` built `strip_suffix`'s stripped string, nearly a copy of
//!   the input, only to drop it: 1000 calls on a 150 KB string requested
//!   75 MB.
//!
//! The outputs pin both legs to what they printed before the change
//! (codepoint counting through combining sequences, n = 0, n < 0, n past the
//! end, the empty string).
use std::process::Command;

const SOURCE: &str = r#"fn main() -> Unit = {
  let s = "héllo wörld 👨‍👩‍👧!"
  for n in [0, 1, 3, 7, 30, -1] {
    println("${n}: [${string.take_end(s, n)}] [${string.drop_end(s, n)}]")
  }
  let e = ""
  println("[${string.take_end(e, 2)}] [${string.drop_end(e, 2)}] [${string.take_end(e, -1)}]")
  for p in ["", "!", "👧!", "wörld 👨‍👩‍👧!", "x!", "héllo wörld 👨‍👩‍👧!", "_héllo wörld 👨‍👩‍👧!", "rld"] {
    println("${p}: ${string.ends_with(s, p)} ${string.starts_with(s, p)}")
  }
  println("${string.ends_with("", "")} ${string.ends_with("a", "")} ${string.ends_with("", "a")}")
}
"#;

const EXPECTED: &str = r#"0: [] [héllo wörld 👨‍👩‍👧!]
1: [!] [héllo wörld 👨‍👩‍👧]
3: [‍👧!] [héllo wörld 👨‍👩]
7: [ 👨‍👩‍👧!] [héllo wörld]
30: [héllo wörld 👨‍👩‍👧!] []
-1: [héllo wörld 👨‍👩‍👧!] []
[] [] []
: true true
!: true false
👧!: true false
wörld 👨‍👩‍👧!: true false
x!: false false
héllo wörld 👨‍👩‍👧!: true true
_héllo wörld 👨‍👩‍👧!: false false
rld: false false
true true false
"#;

/// 1000 `ends_with` calls on a 150 KB string, then one take_end / drop_end.
const SIZED: &str = r#"fn main() -> Unit = {
  let s = string.repeat("aé", 50000) + "!"
  var hits = 0
  var i = 0
  while i < 1000 {
    if string.ends_with(s, if i % 2 == 0 then "é!" else "x!") then hits = hits + 1 else hits = hits
    i = i + 1
  }
  println("${hits} ${string.len(string.take_end(s, 3))} ${string.len(string.drop_end(s, 149999))}")
}
"#;

fn run(src: &str, wasm: bool, count: bool) -> (String, String) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("suffix.almd");
    std::fs::write(&file, src).expect("source");
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(&file);
    if wasm {
        c.args(["--target", "wasm"]);
    }
    if count {
        c.env("ALMIDE_WASM_ALLOC_COUNT", "1");
    }
    let out = c.output().expect("almide run");
    assert!(out.status.success(), "wasm={wasm}: {}", String::from_utf8_lossy(&out.stderr));
    (String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn suffix_ops_print_the_same_on_both_legs() {
    assert_eq!(run(SOURCE, false, false).0, EXPECTED, "native");
    assert_eq!(run(SOURCE, true, false).0, EXPECTED, "wasm");
}

#[test]
fn wasm_ends_with_does_not_copy_the_string() {
    let (out, err) = run(SIZED, true, true);
    assert_eq!(out, "500 3 0\n");
    let line = err.lines().find(|l| l.contains("__ALMD_WASM_ALLOC")).expect("alloc count line");
    let bytes: u64 = line
        .split_whitespace()
        .find_map(|w| w.strip_prefix("bytes="))
        .and_then(|v| v.parse().ok())
        .expect("bytes=");
    assert!(bytes < 2_000_000, "ends_with copies the string again: {line}");
    assert!(line.contains("live=0"), "{line}");
}
