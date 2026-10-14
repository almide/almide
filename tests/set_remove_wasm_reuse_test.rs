//! `s = set.remove(s, x)` copied the whole set on every step on wasm, where
//! native takes the dying set by value (`almide_rt_set_remove_owned`, from
//! `@reuse(s)`). The stdlib inspection's complexity criterion found it on
//! GCE (2026-10-14): the rebind loop measured O(1) per call natively and
//! O(n) on wasm. wasm now removes in place when the op holds the block's
//! only credit.
//!
//! Value semantics hold by count, not by syntax: a set another binding still
//! holds (`let before = s`) keeps its members, and an absent member leaves
//! the set as it was.
use std::process::Command;

const SOURCE: &str = r#"effect fn main() -> Unit = {
  var s: Set[Int] = list.range(0, 40000) |> set.from_list
  let before = s
  var i = 0
  while i < 40000 {
    s = set.remove(s, i * 2)
    i = i + 1
  }
  s = set.remove(s, -1)
  println("${set.len(s)} ${set.contains(s, 1)} ${set.contains(s, 2)} ${set.contains(s, 39999)} ${set.len(before)} ${set.contains(before, 2)}")
  var t: Set[String] = set.from_list(["a", "b", "c"])
  let keep = t
  t = set.remove(t, "b")
  t = set.remove(t, "zz")
  t = set.insert(t, "d")
  println("${set.to_list(t)} ${set.to_list(keep)} ${set.contains(t, "b")}")
}
"#;

const EXPECTED: &str = "20000 true false true 40000 true\n[\"a\", \"c\", \"d\"] [\"a\", \"b\", \"c\"] false\n";

fn almide() -> Command {
    Command::new(env!("CARGO_BIN_EXE_almide"))
}

#[test]
fn set_remove_rebind_reuses_the_dying_set_on_wasm() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("remove.almd");
    std::fs::write(&file, SOURCE).expect("source");

    let native = almide().arg("run").arg(&file).output().expect("native run");
    assert!(native.status.success(), "{}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout), EXPECTED, "native");

    let wasm = almide()
        .arg("run")
        .arg(&file)
        .args(["--target", "wasm"])
        .env("ALMIDE_WASM_ALLOC_COUNT", "1")
        .output()
        .expect("wasm run");
    assert!(wasm.status.success(), "{}", String::from_utf8_lossy(&wasm.stderr));
    assert_eq!(String::from_utf8_lossy(&wasm.stdout), EXPECTED, "wasm");
    let counts = String::from_utf8_lossy(&wasm.stderr);
    let field = |k: &str| -> u64 {
        counts
            .split_whitespace()
            .find_map(|w| w.strip_prefix(k)?.parse().ok())
            .unwrap_or_else(|| panic!("no {k} in {counts}"))
    };
    assert_eq!(field("live="), 0, "{counts}");
    // One copy per step is 40000 copies of up to 320 KB: gigabytes.
    let bytes = field("bytes=");
    assert!(bytes < 50_000_000, "wasm set.remove rebind allocated {bytes} bytes — copying per step? {counts}");
}
