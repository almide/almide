//! #3707: `m = map.update(m, k, f)` copied the whole map on every step, on
//! both legs, where `m = map.set(m, k, v)` updates in place. `map.update` is
//! now declared `@reuse(m)`: native takes a dying receiver by value
//! (`almide_rt_map_update_owned`), and wasm writes a receiver whose credit
//! the op holds in place when that credit is the block's only one.
//!
//! Value semantics hold by count, not by syntax: a map another binding still
//! holds (`let before = m`) keeps its old values, and an absent key leaves
//! the map as it was.
use std::process::Command;
use std::time::{Duration, Instant};

const SOURCE: &str = r#"effect fn main() -> Unit = {
  var m: Map[Int, Int] = list.range(0, 80000) |> list.map((i) => (i, 0)) |> map.from_list
  let before = m
  var i = 0
  while i < 80000 {
    m = map.update(m, i, (v) => v + i)
    i = i + 1
  }
  m = map.update(m, -1, (v) => v + 1)
  println("${map.len(m)} ${map.get(m, 0)} ${map.get(m, 79999)} ${map.get(m, -1)} ${map.get(before, 79999)}")
  var s: Map[String, String] = map.from_list([("a", "x"), ("b", "y")])
  let keep = s
  s = map.update(s, "a", (v) => v + "!")
  s = map.update(s, "zz", (v) => v + "?")
  println("${s} ${keep}")
}
"#;

const EXPECTED: &str = "80000 some(0) some(79999) none some(0)\n[\"a\": \"x!\", \"b\": \"y\"] [\"a\": \"x\", \"b\": \"y\"]\n";

fn almide() -> Command {
    Command::new(env!("CARGO_BIN_EXE_almide"))
}

#[test]
fn map_update_rebind_reuses_the_dying_map_on_both_legs() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("update.almd");
    std::fs::write(&file, SOURCE).expect("source");

    let exe = dir.path().join("update");
    let build = almide().arg("build").arg(&file).arg("-o").arg(&exe).arg("--release").output().expect("build");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
    let t = Instant::now();
    let native = Command::new(&exe).output().expect("run artifact");
    let took = t.elapsed();
    assert!(native.status.success(), "{}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout), EXPECTED, "native");
    assert!(took < Duration::from_secs(1), "native map.update rebind took {took:?} — copying per step again?");

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
    let bytes = field("bytes=");
    assert!(bytes < 100_000_000, "wasm map.update rebind allocated {bytes} bytes — copying per step again? {counts}");
}
