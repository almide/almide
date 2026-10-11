//! #3540: on wasm, `string.ends_with` was lowered as `strip_suffix` — the
//! stripped string allocated and copied on every HIT, then dropped — so a
//! match cost O(len(s)) where native costs O(len(suffix)). It now compares
//! the suffix bytes in place and allocates nothing.
//!
//! The pin is the embedded host's allocation counter, not a time: 20,000 hits
//! on a 300 KB string allocated ~6 GB before and nothing past the string after.
use std::process::Command;

const SOURCE: &str = r#"effect fn main() -> Unit = {
  let s = "日本語abc"
  println("${string.ends_with(s, "")} ${string.ends_with(s, "abc")} ${string.ends_with(s, "語abc")} ${string.ends_with(s, s)} ${string.ends_with(s, "x" + s)} ${string.ends_with("", "")} ${string.ends_with("", "a")} ${string.ends_with(s, "b")}")
  println("${string.strip_suffix(s, "abc")} ${string.strip_prefix(s, "日本")} ${string.strip_suffix(s, "zz")}")
  let x = string.repeat("aé", 100000)
  var hits = 0
  var i = 0
  while i < 20000 {
    if string.ends_with(x, "é") then hits = hits + 1 else hits = hits
    i = i + 1
  }
  println("${hits}")
}
"#;

const EXPECTED: &str =
    "true true true true false true false false\nsome(\"日本語\") some(\"語abc\") none\n20000\n";

#[test]
fn ends_with_answers_without_building_the_stripped_string() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("ends_with.almd");
    std::fs::write(&file, SOURCE).expect("source");
    let native = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("run")
        .arg(&file)
        .output()
        .expect("native");
    assert!(
        native.status.success(),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&native.stdout), EXPECTED, "native");
    let wasm = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("run")
        .arg(&file)
        .args(["--target", "wasm"])
        .env("ALMIDE_WASM_ALLOC_COUNT", "1")
        .output()
        .expect("wasm");
    assert!(
        wasm.status.success(),
        "{}",
        String::from_utf8_lossy(&wasm.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&wasm.stdout), EXPECTED, "wasm");
    let counts = String::from_utf8_lossy(&wasm.stderr).into_owned();
    let field = |k: &str| -> u64 {
        counts
            .split_whitespace()
            .find_map(|w| w.strip_prefix(k)?.parse().ok())
            .unwrap_or_else(|| panic!("no {k} in {counts}"))
    };
    assert_eq!(field("live="), 0, "{counts}");
    // The 300 KB string, its repeat buffers and a few small strings: well
    // under 4 MB. One stripped copy per hit was ~6 GB.
    let bytes = field("bytes=");
    assert!(
        bytes < 4_000_000,
        "ends_with allocated {bytes} bytes — a copy per hit again? {counts}"
    );
}
