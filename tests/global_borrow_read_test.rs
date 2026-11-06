//! A module-level `var` handed to a read-only (borrowed) parameter on the
//! native leg (#4562): `map.get(cache, k)`, `list.get(xs, i)`,
//! `string.starts_with(g, p)`, or a user fn's borrowed param.
//!
//! A heap global lives in a `RefCell<Rc<T>>`, and its bare read is a deep copy
//! of `T`. A borrowed argument took `&` of that copy, so every lookup in a
//! module-level map copied the whole map. The argument now borrows a snapshot
//! of the cell's `Rc`: a reference count, not a copy. The values must not
//! change, and that includes a callee that writes the global while it holds
//! the argument: the argument keeps the call site's value.

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
var m: Map[Int, Int] = map.new()
var xs: List[Int] = []
var g: String = ""

fn bump() -> Unit = {
  m = map.set(m, 1, 100)
}

fn read_after_bump(x: Map[Int, Int]) -> Int = {
  bump()
  map.get(x, 1) ?? 0
}

fn look(k: Int) -> Int = map.get(m, k) ?? 0

fn has(k: Int) -> Bool = map.contains(m, k)

fn at(i: Int) -> Int = list.get(xs, i) ?? 0

fn count() -> Int = list.len(xs)

fn pre() -> Bool = string.starts_with(g, "he")

effect fn main() -> Unit = {
  m = map.set(m, 1, 1)
  let a = read_after_bump(m)
  let b = look(1)
  xs = [1, 2, 3]
  g = "hello"
  println("${a} ${b} ${has(1)} ${has(2)} ${at(2)} ${count()} ${pre()}")
}
"#;

const WANT: &str = "1 100 true false 3 3 true\n";

fn source_file(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-global-borrow-read-{tag}"));
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("main.almd");
    std::fs::write(&file, SRC).unwrap();
    file
}

fn run(target: Option<&str>) -> String {
    let file = source_file(target.unwrap_or("native"));
    let mut cmd = Command::new(almide_bin());
    cmd.arg("run").arg(&file);
    if let Some(t) = target {
        cmd.args(["--target", t]);
    }
    let out = cmd.output().expect("spawn almide");
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

#[test]
fn borrowed_global_arg_keeps_its_values_natively() {
    let out = run(None);
    assert!(out.contains(WANT), "native:\n{out}");
}

#[test]
fn borrowed_global_arg_keeps_its_values_on_wasm() {
    let out = run(Some("wasm"));
    assert!(out.contains(WANT), "wasm:\n{out}");
}

/// The body of the emitted `fn name`, up to the next top-level `}`.
fn fn_body<'a>(rust: &'a str, name: &str) -> &'a str {
    let at = rust.find(&format!("fn {name}(")).unwrap_or_else(|| panic!("no `fn {name}` in the emitted Rust:\n{rust}"));
    let rest = &rust[at..];
    &rest[..rest.find("\n}\n").unwrap_or(rest.len())]
}

/// Every read-only use of a global in the emitted Rust borrows a snapshot of
/// the cell's `Rc` and never deep-copies the global out of its cell. These
/// programs wall on the certified native render (a heap global's slot is
/// outside its subset), so the standard codegen printed here is what
/// `almide build` ships.
#[test]
fn borrowed_global_arg_is_not_copied() {
    let file = source_file("emit");
    let out = Command::new(almide_bin()).arg(&file).args(["--target", "rust"]).output().expect("spawn almide");
    let rust = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "emit failed:\n{}", String::from_utf8_lossy(&out.stderr));
    for name in ["look", "has", "at", "count", "pre"] {
        let body = fn_body(&rust, name);
        assert!(body.contains("std::rc::Rc::clone(&*__almide_cell.borrow())"), "`{name}` does not borrow the global's Rc:\n{body}");
        assert!(!body.contains("__almide_cell.borrow()).clone()"), "`{name}` copies the global out of its cell:\n{body}");
    }
}
