//! A `mut` collection param rebound by an op over its own old value
//! (`s = set.insert(s, x)` on `mut s: Set[Int]`), and a `mut` Map param
//! written by index, on every leg (#4582).
//!
//! The standard native codegen called the BORROWING op on the `&mut` param
//! (`*s = almide_rt_set_insert(&s, x)`), which copies the whole set, so a
//! helper called in a loop was quadratic (50,000 calls: 6 s against 0.01 s for
//! the same statement on a local `var`). The assign overwrites the param, so
//! the old value is dead once it lands: the param now moves into the op's
//! owned twin (`almide_rt_set_insert_owned(std::mem::take(s), x)`), for every
//! op the reuse table gives a twin. The values must not change: a caller that
//! kept the old collection (`let old = s`) still sees it, because the caller
//! hands the helper its own `var`, not the alias.

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

const SRC: &str = r#"
type Bag = { tags: Set[Int], n: Int }

fn adds(mut s: Set[Int], x: Int) -> Unit = { s = set.insert(s, x) }

fn dels(mut s: Set[String], x: String) -> Unit = { s = set.remove(s, x) }

fn put(mut m: Map[Int, Int], k: Int, v: Int) -> Unit = { m[k] = v }

fn drop1(mut m: Map[String, Int], k: String) -> Unit = { m = map.remove(m, k) }

fn keep(mut xs: List[Int], n: Int) -> Unit = { xs = list.take(xs, n) }

fn tag(mut b: Bag, x: Int) -> Unit = { b.tags = set.insert(b.tags, x) }

fn grow(mut s: Set[Int], x: Int) -> Unit = { s = set.insert(s, x + set.len(s)) }

effect fn main() -> Unit = {
  var s: Set[Int] = set.new()
  adds(s, 1)
  let old = s
  adds(s, 2)
  for i in 0..<2000 { adds(s, i) }
  println("${set.len(old)} ${set.contains(old, 2)} ${set.len(s)} ${set.contains(s, 1999)}")
  var t: Set[String] = set.from_list(["a", "b"])
  let t0 = t
  dels(t, "a")
  println("${set.len(t0)} ${set.len(t)} ${set.contains(t, "b")}")
  var m: Map[Int, Int] = [:]
  put(m, 1, 10)
  let m0 = m
  put(m, 1, 11)
  println("${map.get(m0, 1) ?? 0} ${map.get(m, 1) ?? 0}")
  var n: Map[String, Int] = ["x": 1, "y": 2]
  let n0 = n
  drop1(n, "x")
  println("${map.len(n0)} ${map.len(n)} ${map.get(n, "y") ?? 0}")
  var xs = [1, 2, 3, 4]
  let x0 = xs
  keep(xs, 2)
  println("${list.len(x0)} ${list.len(xs)}")
  var g: Bag = { tags: set.new(), n: 0 }
  tag(g, 3)
  let g0 = g
  tag(g, 4)
  println("${set.len(g0.tags)} ${set.len(g.tags)}")
  var u: Set[Int] = set.new()
  grow(u, 10)
  let u0 = u
  grow(u, 10)
  println("${set.len(u0)} ${set.contains(u, 11)}")
}
"#;

const WANT: &str = "1 false 2000 true\n2 1 true\n10 11\n2 1 2\n4 2\n1 2\n1 true\n";

fn source_file(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-mut-collection-param-{tag}"));
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("main.almd");
    std::fs::write(&file, SRC).unwrap();
    file
}

fn run(tag: &str, args: &[&str]) -> String {
    let file = source_file(tag);
    let out = Command::new(almide_bin())
        .arg("run")
        .arg(&file)
        .args(args)
        .env("ALMIDE_NO_VERIFIED_OK", "1")
        .output()
        .expect("spawn almide");
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

#[test]
fn mut_collection_param_rebind_keeps_its_values_natively() {
    let out = run("native", &[]);
    assert!(out.contains(WANT), "native:\n{out}");
}

/// The standard codegen, which ships wherever the certified render walls.
#[test]
fn mut_collection_param_rebind_keeps_its_values_on_the_standard_codegen() {
    let out = run("v0", &["--no-verified"]);
    assert!(out.contains(WANT), "standard codegen:\n{out}");
}

#[test]
fn mut_collection_param_rebind_keeps_its_values_on_wasm() {
    let out = run("wasm", &["--target", "wasm"]);
    assert!(out.contains(WANT), "wasm:\n{out}");
}

/// The emitted body of `fn name` in the standard codegen's Rust.
fn fn_body<'a>(rust: &'a str, name: &str) -> &'a str {
    let head = format!("pub fn {name}(");
    let at = rust.find(&head).unwrap_or_else(|| panic!("no `{head}` in the emitted Rust"));
    let end = rust[at..].find("\n}\n").map_or(rust.len(), |e| at + e);
    &rust[at..end]
}

#[test]
fn a_rebound_mut_param_moves_into_the_owned_twin() {
    let file = source_file("emit");
    let out = Command::new(almide_bin()).arg(&file).args(["--target", "rust"]).output().expect("spawn almide");
    assert!(out.status.success(), "emit failed:\n{}", String::from_utf8_lossy(&out.stderr));
    let rust = String::from_utf8_lossy(&out.stdout);
    for (name, twin) in [
        ("adds", "almide_rt_set_insert_owned(std::mem::take(s)"),
        ("dels", "almide_rt_set_remove_owned(std::mem::take(s)"),
        ("drop1", "almide_rt_map_remove_owned(std::mem::take(m)"),
        ("keep", "almide_rt_list_take_owned(std::mem::take(xs)"),
        ("tag", "almide_rt_set_insert_owned(std::mem::take(&mut b.tags)"),
    ] {
        let body = fn_body(&rust, name);
        assert!(body.contains(twin), "`{name}` must move its param into the owned twin (`{twin}…`):\n{body}");
    }
    // `grow` reads the param again in a later argument: the take would leave
    // that read an empty set, so it keeps the borrowing op.
    let grow = fn_body(&rust, "grow");
    assert!(!grow.contains("std::mem::take"), "`grow` reads `s` twice; it must not take it:\n{grow}");
    // The index write stays an in-place insert on the `&mut` map.
    let put = fn_body(&rust, "put");
    assert!(put.contains("m.insert(") && !put.contains("clone()"), "`put` must write the map in place:\n{put}");
}
