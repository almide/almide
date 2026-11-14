//! #4687: the comparison-bound list calls (`unique`, `dedup`, `contains`,
//! `index_of`) over a generated struct or enum that holds a `Value`, a `Map`
//! or a `Float` build and run on native v1 with the language's equality.
//!
//! `list.unique` over `List[(String, Value)]` rendered
//! `almide_rt_list_unique(&v)` on a tuple struct that derived no `PartialEq`
//! (E0277), and so did every comparison call over a struct holding a Value, a
//! Map or a Float. Value and Map compare by their runtime `PartialEq`, which is
//! the language's `==`; a Float field rides an i64 bit lane, so its struct gets
//! a generated `PartialEq` that compares the floats (`0.0 == -0.0`, NaN
//! unequal), as `==` on the same values does.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-struct-eq-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The certified native render must hold the program, then `almide run`
/// prints `expected`.
fn check(tag: &str, src: &str, expected: &str) {
    if let Err(e) = almide_mir::pipeline::try_render_rust_source(src) {
        panic!("{tag}: left the certified native render: {e:?}");
    }
    let dir = scratch(tag);
    let path: &Path = &dir.join("main.almd");
    std::fs::write(path, src).unwrap();
    let out = Command::new(almide()).arg("run").arg(path).env("ALMIDE_CERTIFY_OWNERSHIP", "fail").output().expect("almide");
    let (stdout, stderr) = (String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    let _ = std::fs::remove_dir_all(&dir);
    assert!(out.status.success(), "{tag}: native run failed\n{stderr}");
    assert_eq!(stdout.trim_end(), expected, "{tag}\n{stderr}");
}

const FOUR: &str = r#"println("${list.len(list.unique(xs))} ${list.len(list.dedup(xs))} ${list.contains(xs, b)} ${list.index_of(xs, b)}")"#;

#[test]
fn the_reported_tuple_of_string_and_value() {
    let src = r#"fn main() -> Unit = {
  let xs = list.unique([("a", value.null()), ("a", value.null()), ("b", value.int(1))])
  println("${list.len(xs)}")
}
"#;
    check("issue", src, "2");
}

#[test]
fn structs_holding_a_value_or_a_map() {
    let src = format!(
        r#"type Tagged = {{ s: String, v: Value }}

fn pairs() -> Unit = {{
  let b: (String, Value) = ("b", value.int(1))
  let xs: List[(String, Value)] = [("a", value.null()), ("a", value.null()), b]
  {FOUR}
}}

fn tagged() -> Unit = {{
  let b = Tagged {{ s: "b", v: value.int(1) }}
  let xs: List[Tagged] = [Tagged {{ s: "a", v: value.float(0.0) }}, Tagged {{ s: "a", v: value.float(-0.0) }}, b]
  {FOUR}
}}

fn maps() -> Unit = {{
  let b: (String, Map[String, Int]) = ("b", ["k": 2])
  let xs: List[(String, Map[String, Int])] = [("a", ["k": 1, "j": 2]), ("a", ["j": 2, "k": 1]), b]
  {FOUR}
}}

fn main() -> Unit = {{
  pairs()
  tagged()
  maps()
}}
"#
    );
    check("value", &src, "2 2 true some(2)\n2 2 true some(2)\n2 2 true some(2)");
}

#[test]
fn structs_and_enums_holding_a_float() {
    let src = format!(
        r#"type Reading = {{ n: Int, f: Float }}
type Shape = | Dot(Float) | Blank

fn tuples() -> Unit = {{
  let b: (Int, Float) = (2, 1.5)
  let xs: List[(Int, Float)] = [(1, 0.0), (1, -0.0), b]
  {FOUR}
}}

fn readings() -> Unit = {{
  let b = Reading {{ n: 2, f: 1.5 }}
  let xs: List[Reading] = [Reading {{ n: 1, f: 0.0 }}, Reading {{ n: 1, f: -0.0 }}, b]
  {FOUR}
}}

fn float_lists() -> Unit = {{
  let b: (String, List[Float]) = ("b", [1.0])
  let xs: List[(String, List[Float])] = [("a", [0.0]), ("a", [-0.0]), b]
  {FOUR}
}}

fn not_a_numbers() -> Unit = {{
  let nan = 0.0 / 0.0
  let b: (String, Float) = ("b", nan)
  let xs: List[(String, Float)] = [("a", nan), ("a", nan), b]
  {FOUR}
}}

fn shapes() -> Unit = {{
  let b = Blank
  let xs: List[Shape] = [Dot(0.0), Dot(-0.0), b]
  {FOUR}
}}

fn main() -> Unit = {{
  tuples()
  readings()
  float_lists()
  not_a_numbers()
  shapes()
}}
"#
    );
    check("float", &src, "2 2 true some(2)\n2 2 true some(2)\n2 2 true some(2)\n3 3 false none\n2 2 true some(2)");
}
