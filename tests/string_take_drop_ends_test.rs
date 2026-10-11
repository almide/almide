//! #3542: `string.take` / `drop` / `take_end` / `drop_end` walk only as far as
//! their cut. Native `take_end` / `drop_end` used to collect every char into a
//! `Vec<char>`, and the wasm self-host counted every codepoint first; both were
//! O(len) to cut a few codepoints off one end.
//!
//! This pins the VALUES across the edge cases the rewrite touches:
//! - negative, zero, in-range, exact and past-the-end counts;
//! - empty, ASCII, CJK, combining-mark and astral strings.
//!
//! The expected output is the pre-change compiler's. Native and wasm must
//! both print it, and wasm must leave nothing live.
use std::process::Command;

const SOURCE: &str = r#"effect fn main() -> Unit = {
  let ss = ["", "a", "日本語abc", "e\u{301}x", "🎉🎉", "aé" + "aé"]
  let ns = [-5, -1, 0, 1, 2, 3, 4, 6, 7, 100]
  for s in ss {
    for n in ns {
      println("${n}|${string.take(s, n)}|${string.drop(s, n)}|${string.take_end(s, n)}|${string.drop_end(s, n)}")
    }
  }
}
"#;

const EXPECTED: &str = r#"-5||||
-1||||
0||||
1||||
2||||
3||||
4||||
6||||
7||||
100||||
-5|a||a|
-1|a||a|
0||a||a
1|a||a|
2|a||a|
3|a||a|
4|a||a|
6|a||a|
7|a||a|
100|a||a|
-5|日本語abc||日本語abc|
-1|日本語abc||日本語abc|
0||日本語abc||日本語abc
1|日|本語abc|c|日本語ab
2|日本|語abc|bc|日本語a
3|日本語|abc|abc|日本語
4|日本語a|bc|語abc|日本
6|日本語abc||日本語abc|
7|日本語abc||日本語abc|
100|日本語abc||日本語abc|
-5|éx||éx|
-1|éx||éx|
0||éx||éx
1|e|́x|x|é
2|é|x|́x|e
3|éx||éx|
4|éx||éx|
6|éx||éx|
7|éx||éx|
100|éx||éx|
-5|🎉🎉||🎉🎉|
-1|🎉🎉||🎉🎉|
0||🎉🎉||🎉🎉
1|🎉|🎉|🎉|🎉
2|🎉🎉||🎉🎉|
3|🎉🎉||🎉🎉|
4|🎉🎉||🎉🎉|
6|🎉🎉||🎉🎉|
7|🎉🎉||🎉🎉|
100|🎉🎉||🎉🎉|
-5|aéaé||aéaé|
-1|aéaé||aéaé|
0||aéaé||aéaé
1|a|éaé|é|aéa
2|aé|aé|aé|aé
3|aéa|é|éaé|a
4|aéaé||aéaé|
6|aéaé||aéaé|
7|aéaé||aéaé|
100|aéaé||aéaé|
"#;

fn run(file: &std::path::Path, wasm: bool) -> (String, String) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(file);
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
}

#[test]
fn take_drop_and_their_end_forms_keep_every_edge_case() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("take_drop.almd");
    std::fs::write(&file, SOURCE).expect("source");
    let (native, _) = run(&file, false);
    assert_eq!(native, EXPECTED, "native");
    let (wasm, counts) = run(&file, true);
    assert_eq!(wasm, EXPECTED, "wasm");
    assert!(counts.contains("live=0"), "{counts}");
}
