//! The native Map's read side borrows its key and the word-count write moves it (#2157).
//!
//! `counts[w] = map.get_or(counts, w, 0) + 1` — the keyed-aggregation loop —
//! used to emit `counts.insert(w.clone(), almide_rt_map_get_or(&counts, w, 0) + 1)`:
//! `get_or` took the key BY VALUE, so `w` was consumed there and the insert
//! had to clone it. Two mechanisms now remove the clone, and each one is
//! pinned here by the emitted Rust:
//!
//! 1. `map.get` / `map.get_or` / `map.contains` / `set.contains` are
//!    `@borrow_ref(key)` and the runtime takes `&Q` (`K: Borrow<Q>`), so the
//!    lookup renders `&w` (or the bare `&str` for a borrow-inferred String
//!    param) and never owns a key it only compares.
//! 2. `MapInsert` with a plain-variable key evaluates its VALUE first
//!    (`map_insert_value_first`): the clone pass counts the value's uses
//!    ahead of the key's, so the key position is the var's last use and moves,
//!    and the walker binds the value to a temporary ahead of the insert so
//!    rustc sees the same order.
//!
//! If a `.clone()` of the key reappears in the loop, one of the two is gone —
//! fix the pass or the declaration, don't relax the assertion. The negative
//! case (a key still used AFTER the insert must keep its clone) guards the
//! other direction.
//!
//! Skips cleanly when the `almide` binary is unavailable (CI builds it in
//! the build step; locally run `cargo build --release` first).

use std::path::Path;
use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

fn tool_available() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
}

/// Emit Rust for `source` and return the body of `fn <name>` (from its
/// `fn <name>(` line to the first column-zero `}`).
fn emitted_fn(source: &str, name: &str, tag: &str) -> String {
    let dir = std::env::temp_dir().join(format!("almide-map-key-move-{}-{}", tag, std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("prog.almd");
    std::fs::write(&src, source).unwrap();

    let output = Command::new(almide_bin())
        .args([src.to_str().unwrap(), "--target", "rust"])
        .output()
        .expect("failed to spawn almide");
    let rust = String::from_utf8_lossy(&output.stdout).to_string();
    std::fs::remove_dir_all(&dir).ok();
    assert!(
        output.status.success(),
        "--target rust emit failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let needle = format!("fn {name}(");
    let start = rust
        .find(&needle)
        .unwrap_or_else(|| panic!("emitted Rust has no `{needle}`:\n{rust}"));
    let rest = &rust[start..];
    let end = rest.find("\n}").map(|i| i + 2).unwrap_or(rest.len());
    rest[..end].to_string()
}

const COUNT_LOOP: &str = "fn count(words: List[String]) -> Map[String, Int] = {\n\
  var counts: Map[String, Int] = [:]\n\
  for w in words {\n\
    counts[w] = map.get_or(counts, w, 0) + 1\n\
  }\n\
  counts\n\
}\n\
\n\
fn main() -> Unit = {\n\
  println(int.to_string(map.len(count([\"a\", \"b\", \"a\"]))))\n\
}\n";

#[test]
fn word_count_loop_borrows_the_lookup_key_and_moves_it_into_the_insert() {
    if !tool_available() {
        eprintln!("skipping: almide binary not available");
        return;
    }
    let body = emitted_fn(COUNT_LOOP, "count", "loop");
    assert!(
        body.contains("almide_rt_map_get_or(&counts, &w, 0i64)"),
        "expected the lookup to BORROW its key (`&w`):\n{body}"
    );
    assert!(
        !body.contains("w.clone()"),
        "the word-count loop clones its key again:\n{body}"
    );
    assert!(
        body.contains("counts.insert(w, __almide_mv)"),
        "expected the value bound first and the key MOVED into the insert:\n{body}"
    );
}

#[test]
fn key_still_used_after_the_insert_keeps_its_clone() {
    if !tool_available() {
        eprintln!("skipping: almide binary not available");
        return;
    }
    let body = emitted_fn(
        "fn tally(words: List[String]) -> Int = {\n\
           var counts: Map[String, Int] = [:]\n\
           var total = 0\n\
           for w in words {\n\
             counts[w] = map.get_or(counts, w, 0) + 1\n\
             total = total + string.len(w)\n\
           }\n\
           total\n\
         }\n\
         \n\
         fn main() -> Unit = {\n\
           println(int.to_string(tally([\"a\", \"bb\"])))\n\
         }\n",
        "tally",
        "after",
    );
    assert!(
        body.contains("counts.insert(w.clone(), __almide_mv)"),
        "a key read after the insert must still be CLONED into it:\n{body}"
    );
}

#[test]
fn string_param_key_probes_as_a_bare_str() {
    if !tool_available() {
        eprintln!("skipping: almide binary not available");
        return;
    }
    let body = emitted_fn(
        "fn has(m: Map[String, Int], k: String) -> Bool = map.contains(m, k)\n\
         \n\
         fn main() -> Unit = {\n\
           println(if has([\"a\": 1], \"a\") then \"y\" else \"n\")\n\
         }\n",
        "has",
        "param",
    );
    assert!(
        body.contains("almide_rt_map_contains(m, k)"),
        "a borrow-inferred `&str` param must probe as itself, not `&k`:\n{body}"
    );
    assert!(!body.contains(".clone()"), "the probe clones its key:\n{body}");
}
