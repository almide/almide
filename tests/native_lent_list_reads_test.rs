//! #4661: two reads the native render copied where v0 lends.
//!
//! - `list.get(xs, i) ?? value.null()` over a local list of Values, in a loop
//!   that reads the list every round: the element is lent and the fallback is
//!   held in a slot declared before the join (`native_held_list.rs`,
//!   `borrow_join::default_arm`), where each read copied a whole Value tree.
//! - `list.sort_by` with a tuple key: the key goes to the runtime as a Rust
//!   tuple (`native_hof_lent.rs` `tuple_key`), which orders as the record
//!   does, field by field.
//!
//! Both print the same on the native render (ownership certificate required)
//! and on wasm.
use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

const WALK: &str = r#"import json

fn width(v: Value) -> Int = value.get_int(v, "w") ?? 0

fn main() -> Unit = {
  let doc = json.parse("[{\"w\":1},{\"w\":2},{\"w\":3},{\"x\":4}]") ?? value.null()
  let xs = value.as_array(doc) ?? []
  var i = 0
  var s = 0
  while i < list.len(xs) + 1 {
    let a = list.get(xs, i) ?? value.null()
    s = s + width(a) * 10 + (value.get_int(a, "x") ?? 100)
    i = i + 1
  }
  println("${s}")
}
"#;

const SORT: &str = r#"fn main() -> Unit = {
  let entries = [("pear", 2), ("fig", 3), ("apple", 2), ("date", 1), ("fig", 1)]
  let by_count = entries |> list.sort_by(((w, c)) => (0 - c, w))
  println(by_count |> list.map(((w, c)) => "${w}:${c}") |> list.join(","))
  let by_word = entries |> list.sort_by(((w, c)) => (w, c))
  println(by_word |> list.map(((w, c)) => "${w}:${c}") |> list.join(","))
}
"#;

fn both(tag: &str, src: &str, expected: &str) -> String {
    let dir = std::env::temp_dir().join(format!("almide-lent-list-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("prog.almd");
    std::fs::write(&file, src).unwrap();
    for target in [None, Some("wasm")] {
        let mut cmd = Command::new(almide());
        cmd.arg("run").arg(&file).env("ALMIDE_CERTIFY_OWNERSHIP", "fail");
        if let Some(t) = target {
            cmd.args(["--target", t]);
        }
        let out = cmd.output().expect("almide");
        assert_eq!(String::from_utf8_lossy(&out.stdout), expected, "{tag} on {target:?}: {}", String::from_utf8_lossy(&out.stderr));
    }
    let out = Command::new(almide())
        .arg("build").arg(&file).arg("-o").arg(dir.join("prog"))
        .env("ALMIDE_CERTIFY_OWNERSHIP", "fail")
        .env("ALMIDE_RUN_PROJECT_DIR", dir.join("project"))
        .output()
        .expect("almide");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let rust = std::fs::read_to_string(dir.join("project/src/main.rs")).expect("the generated program");
    let _ = std::fs::remove_dir_all(&dir);
    rust
}

#[test]
fn an_element_of_a_local_list_read_in_a_loop_is_lent() {
    let rust = both("walk", WALK, "464\n");
    assert!(rust.contains("Option<&"), "the element is copied out of the list:\n{rust}");
}

#[test]
fn a_tuple_sort_key_goes_as_a_rust_tuple() {
    let rust = both("sort", SORT, "fig:3,apple:2,pear:2,date:1,fig:1\napple:2,date:1,fig:1,fig:3,pear:2\n");
    assert!(rust.contains(",) { let __k = {"), "the key is the record struct:\n{rust}");
}
