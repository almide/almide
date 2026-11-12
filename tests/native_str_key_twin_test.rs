//! A map updated through a closure's shared cell takes its borrowed string
//! key by reference on native v1, and copies it only where it adds an entry.
//!
//! `let bump = (k: String) => { m[k] = .. }` over a captured `var m` reads
//! the map out of the cell, sets the key and puts the map back. The set
//! rendered as `almide_rt_map_set(m, AlmideString::from(k), v)`: one copy of
//! the key per update, freed again at once when the key was already there.
//! The runtime's borrowed-string-key twin (`almide_rt_map_set_str`, paired
//! from the signatures) takes `k` itself.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-str-key-twin-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// (stdout, stderr) of the native `almide run <src>`, the certificate required.
fn run(src: &Path) -> (String, String) {
    let out = Command::new(almide())
        .arg("run")
        .arg(src)
        .env("ALMIDE_CERTIFY_OWNERSHIP", "fail")
        .env("ALMIDE_ALLOC_COUNT", "1")
        .output()
        .expect("almide");
    (String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

/// A closure counting `n` updates of one long key, and two short ones.
fn counter(n: usize) -> String {
    format!(
        r#"type Acc = {{ sum: Int, count: Int }}

effect fn main() -> Unit = {{
  var m: Map[String, Acc] = map.new()
  let bump = (k: String) => {{
    match map.get(m, k) {{
      some(s) => {{
        m[k] = Acc {{ sum: s.sum + 1, count: s.count + 1 }}
      }},
      none => {{
        m[k] = Acc {{ sum: 1, count: 1 }}
      }},
    }}
  }}
  for i in 0..<{n} {{
    bump("a key long enough to be stored out of line")
  }}
  bump("x")
  bump("y")
  bump("x")
  let long = map.get(m, "a key long enough to be stored out of line")
  let x = map.get(m, "x")
  println("${{map.len(m)}} ${{(long ?? Acc {{ sum: 0, count: 0 }}).count}} ${{(x ?? Acc {{ sum: 0, count: 0 }}).sum}}")
}}
"#
    )
}

#[test]
fn the_set_takes_the_borrowed_key_itself() {
    let rust = almide_mir::pipeline::try_render_rust_source(&counter(4)).unwrap_or_else(|e| panic!("left the certified render: {e:?}"));
    let at = rust.find("fn almd___lambda_main_").expect("the lambda");
    let end = rust[at..].find("\n}\n").expect("its end");
    let body = &rust[at..at + end];
    assert!(body.contains("almide_rt_map_set_str("), "the set does not take the key by reference:\n{body}");
    assert!(!body.contains("AlmideString::from("), "the lambda copies its key:\n{body}");
}

/// The counting allocator's `allocs` for `source`, after checking its output.
fn allocs(tag: &str, source: &str, expect: &str) -> u64 {
    let dir = scratch(tag);
    let src = dir.join("prog.almd");
    std::fs::write(&src, source).unwrap();
    let (stdout, stderr) = run(&src);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(stdout, expect, "{stderr}");
    let line = stderr.lines().find(|l| l.starts_with("__ALMD_ALLOC ")).unwrap_or_else(|| panic!("no allocation report:\n{stderr}"));
    line.split_whitespace().find_map(|kv| kv.strip_prefix("allocs=")).and_then(|v| v.parse().ok()).unwrap_or_else(|| panic!("malformed report `{line}`"))
}

#[test]
fn an_update_of_a_present_key_allocates_nothing() {
    let small = allocs("small", &counter(100), "3 100 2\n");
    let large = allocs("large", &counter(2000), "3 2000 2\n");
    // 1,900 more updates of a key already in the map: no more blocks, where
    // a copy of the key per update allocated one each.
    assert!(large < small + 100, "{large} blocks at 2,000 updates against {small} at 100 — a key copy per update?");
}

/// New keys through the twin append in order, one probe each, across the
/// size where the map starts its key index, and later updates of any of them
/// (first, middle, last) land on the entry already there.
#[test]
fn new_keys_append_in_order_across_the_index_threshold() {
    let source = r#"effect fn main() -> Unit = {
  var m: Map[String, Int] = map.new()
  let put = (k: String, v: Int) => {
    m[k] = v
  }
  for i in 0..<40 {
    put("key${i}", i)
  }
  put("key0", 100)
  put("key20", 120)
  put("key39", 139)
  put("key40", 140)
  let keys = map.keys(m) |> list.join(",")
  let total = map.values(m) |> list.fold(0, (a, b) => a + b)
  println("${map.len(m)} ${total} ${keys}")
}
"#;
    let rust = almide_mir::pipeline::try_render_rust_source(source).unwrap_or_else(|e| panic!("left the certified render: {e:?}"));
    assert!(rust.contains("almide_rt_map_set_str("), "the set does not take the key by reference");
    let keys: Vec<String> = (0..=40).map(|i| format!("key{i}")).collect();
    let total: i64 = (0..40).sum::<i64>() + 100 + 120 + 139 - 20 - 39 + 140;
    let expect = format!("41 {total} {}\n", keys.join(","));
    let dir = scratch("threshold");
    let src = dir.join("prog.almd");
    std::fs::write(&src, source).unwrap();
    let (stdout, stderr) = run(&src);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(stdout, expect, "{stderr}");
}
