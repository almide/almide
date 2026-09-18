//! A one-line wrapper forwarding a list to a fn in ANOTHER module borrows it
//! (#2164). The same wrapper forwarding to a fn in the same file already took
//! `&[T]`; the cross-module hop fell to `Vec<T>` + a clone at every call,
//! because the ownership walk only consulted the borrow snapshot for a
//! `Named` callee and let a `Module` callee drop to the pessimistic
//! fallback. A read-only accessor called once per element then cost O(n)
//! per call — 3.6 s for a 10 000-token scan, 0.00 s through the same file.
//!
//! Emit-shape test in the mold of `mutual_recursion_borrow_test.rs`, over a
//! two-module package. Skips cleanly when the `almide` binary is unavailable.

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

const OTHER: &str = "type Tok = { kind: String, text: String, line: Int }\n\
    fn get_far(ts: List[Tok], i: Int) -> Tok = {\n\
      var out = Tok { kind: \"\", text: \"\", line: 0 }\n\
      match list.get(ts, i) { some(t) => { out = t }, none => () }\n\
      out\n\
    }\n";

const MAIN: &str = "import self.other\n\
    fn get_here(ts: List[other.Tok], i: Int) -> other.Tok = {\n\
      var out = other.Tok { kind: \"\", text: \"\", line: 0 }\n\
      match list.get(ts, i) { some(t) => { out = t }, none => () }\n\
      out\n\
    }\n\
    fn via_far(ts: List[other.Tok], i: Int) -> other.Tok = other.get_far(ts, i)\n\
    fn via_here(ts: List[other.Tok], i: Int) -> other.Tok = get_here(ts, i)\n\
    fn main() -> Unit = {\n\
      let ts = [other.Tok { kind: \"id\", text: \"t\", line: 1 }]\n\
      println(via_far(ts, 0).kind + via_here(ts, 0).kind)\n\
    }\n";

fn emitted_package() -> String {
    let dir = std::env::temp_dir().join(format!("almide-xmod-borrow-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("almide.toml"), "[package]\nname = \"xmod\"\nversion = \"0.1.0\"\n").unwrap();
    std::fs::write(dir.join("src/other.almd"), OTHER).unwrap();
    std::fs::write(dir.join("src/main.almd"), MAIN).unwrap();
    let output = Command::new(almide_bin())
        .current_dir(&dir)
        .args(["src/main.almd", "--target", "rust"])
        .output()
        .expect("failed to spawn almide");
    let rust = String::from_utf8_lossy(&output.stdout).to_string();
    std::fs::remove_dir_all(&dir).ok();
    assert!(output.status.success(), "--target rust emit failed:\n{}", String::from_utf8_lossy(&output.stderr));
    rust
}

fn signature_of(rust: &str, name: &str) -> String {
    let needle = format!("pub fn {name}(");
    let start = rust.find(&needle).unwrap_or_else(|| panic!("no `{needle}` in emitted Rust"));
    rust[start..].lines().next().unwrap().to_string()
}

#[test]
fn a_wrapper_into_another_module_borrows_the_list_like_one_into_its_own_file() {
    if !tool_available() { return; }
    let rust = emitted_package();
    let far = signature_of(&rust, "via_far");
    let here = signature_of(&rust, "via_here");
    assert!(here.contains("ts: &["), "same-file wrapper is the reference shape: {here}");
    assert!(far.contains("ts: &["), "cross-module wrapper must borrow too, got: {far}");
    assert!(!far.contains("ts: Vec<"), "cross-module wrapper must not take the list by value: {far}");
}

#[test]
fn the_wrapper_forwards_the_borrow_without_a_clone() {
    if !tool_available() { return; }
    let rust = emitted_package();
    let start = rust.find("pub fn via_far(").unwrap();
    let body: String = rust[start..].lines().take(4).collect::<Vec<_>>().join("\n");
    assert!(!body.contains(".clone()") && !body.contains(".to_vec()"), "no copy on the hop: {body}");
}
