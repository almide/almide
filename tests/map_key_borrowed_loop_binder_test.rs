//! A loop binder the body only borrows is bound `&String` off `.iter()`
//! (#1673). Passing it to a generic key slot — `map.get`, `map.contains`,
//! whose runtime twins take `k: &Q` with `K: Borrow<Q>` — must spell the
//! naked binder: `&c` there is `&&String`, rustc resolves `Q = &String` and
//! reports E0277 `String: Borrow<&String>` (#2256). A concrete `&String`
//! slot deref-coerced the doubled borrow, which is why every other read of
//! the binder built and only the map key did not.
//!
//! `almide check` is green for this program on every leg; the evidence is the
//! native build, so the run goes through the default route, the pinned v0
//! leg and wasm, and the emitted Rust is pinned to the naked spelling.
use std::process::Command;

const PROGRAM: &str = r#"fn count_chars(s: String) -> Map[String, Int] = {
  var counts: Map[String, Int] = map.new()
  for c in string.chars(s) {
    counts = map.set(counts, c, (map.get(counts, c) ?? 0) + 1)
  }
  counts
}

fn count_words(words: List[String]) -> Int = {
  var counts: Map[String, Int] = map.new()
  var repeats = 0
  for w in words {
    if map.contains(counts, w) then { repeats = repeats + 1 } else ()
    counts = map.set(counts, w, (map.get(counts, w) ?? 0) + 1)
  }
  repeats
}

effect fn main() -> Unit = {
  let counts = count_chars("hello")
  println(int.to_string(map.len(counts)))
  println(int.to_string(map.get(counts, "l") ?? 0))
  println(int.to_string(count_words(["a", "b", "a", "c", "a"])))
}
"#;

const EXPECTED: &str = "4\n2\n2";

fn almide_bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| format!("{}/target/release/almide", env!("CARGO_MANIFEST_DIR")))
}

fn fn_body<'a>(rust: &'a str, name: &str) -> &'a str {
    rust.split(&format!("pub fn {name}(")).nth(1).unwrap_or_else(|| panic!("no fn {name}")).split("\n}").next().unwrap()
}

#[test]
fn borrowed_loop_binder_is_the_map_key_without_a_second_borrow() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("main.almd");
    std::fs::write(&source, PROGRAM).unwrap();
    let bin = almide_bin();

    let emitted = Command::new(&bin).arg("emit").arg(&source).output().unwrap();
    assert!(emitted.status.success(), "{}", String::from_utf8_lossy(&emitted.stderr));
    let rust = String::from_utf8(emitted.stdout).unwrap();
    for (name, binder) in [("count_chars", "c"), ("count_words", "w")] {
        let body = fn_body(&rust, name);
        // The binder keeps its per-element borrow ...
        assert!(body.contains(".iter()") && !body.contains(".cloned()"), "{name} lost its borrowed binder: {body}");
        // ... and reaches the key slot as the reference it already is.
        assert!(body.contains(&format!("almide_rt_map_get_or(&counts, {binder}, 0i64)")), "{name}: {body}");
        assert!(!body.contains(&format!("&{binder},")) && !body.contains(&format!("&{binder})")), "{name} doubles the borrow: {body}");
    }
    assert!(fn_body(&rust, "count_words").contains("almide_rt_map_contains(&counts, w)"), "{}", fn_body(&rust, "count_words"));

    let runs: [(&str, &[&str], &[(&str, &str)]); 3] = [
        ("native", &[], &[]),
        ("native v0", &["--no-verified"], &[("ALMIDE_NO_VERIFIED_OK", "1")]),
        ("wasm", &["--target", "wasm"], &[]),
    ];
    for (label, args, envs) in runs {
        let mut cmd = Command::new(&bin);
        cmd.arg("run").arg(&source).args(args);
        for (k, v) in envs { cmd.env(k, v); }
        let out = cmd.output().unwrap();
        assert!(out.status.success(), "{label}: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim_end(), EXPECTED, "{label}");
    }
}
