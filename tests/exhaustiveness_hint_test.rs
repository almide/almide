//! E010 non-exhaustive diagnostic MVP: hint contains paste-ready arm
//! templates so LLMs can copy the missing arms directly.
//!
//! Roadmap: variant-exhaustiveness-refinement §1.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn write_tmp(name: &str, body: &str) -> String {
    let td = std::env::temp_dir().join("almide-exhaust-test");
    std::fs::create_dir_all(&td).unwrap();
    let p = td.join(name);
    std::fs::write(&p, body).unwrap();
    p.to_string_lossy().into_owned()
}

#[test]
fn variant_tuple_payload_emits_paste_ready_arm() {
    let src = r#"
type Tree = | Leaf | Node(Int, Tree, Tree)

fn sum(t: Tree) -> Int =
  match t {
    Leaf => 0
  }

effect fn main() -> Unit = {
  let _ = sum(Leaf)
}
"#;
    let path = write_tmp("variant_tuple.almd", src);
    let out = Command::new(almide()).args(["check", &path]).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let all = format!("{}{}", stdout, stderr);
    assert!(all.contains("E010"), "code missing:\n{}", all);
    assert!(all.contains("Node(arg1, arg2, arg3) => _"),
        "paste-ready arm missing:\n{}", all);
    assert!(all.contains("`_ => todo()`"),
        "incremental fallback hint missing:\n{}", all);
}

#[test]
fn option_missing_arm_uses_named_binding() {
    let src = r#"
fn f(o: Option[Int]) -> Int =
  match o {
    some(_) => 1
  }

effect fn main() -> Unit = {
  let _ = f(some(42))
}
"#;
    let path = write_tmp("option_missing.almd", src);
    let out = Command::new(almide()).args(["check", &path]).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let all = format!("{}{}", stdout, stderr);
    assert!(all.contains("E010"), "code missing:\n{}", all);
    assert!(all.contains("none => _"),
        "paste-ready arm missing:\n{}", all);
}

#[test]
fn unit_variant_arm_has_no_bindings() {
    let src = r#"
type Color = | Red | Green | Blue

fn name(c: Color) -> String =
  match c {
    Red => "red"
  }

effect fn main() -> Unit = {
  let _ = name(Red)
}
"#;
    let path = write_tmp("unit_variant.almd", src);
    let out = Command::new(almide()).args(["check", &path]).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let all = format!("{}{}", stdout, stderr);
    assert!(all.contains("E010"));
    assert!(all.contains("Green => _"), "Green arm missing:\n{}", all);
    assert!(all.contains("Blue => _"), "Blue arm missing:\n{}", all);
}

#[test]
fn catch_all_hint_for_infinite_domain() {
    let src = r#"
fn f(n: Int) -> Int =
  match n {
    0 => 0,
    1 => 1
  }

effect fn main() -> Unit = {
  let _ = f(0)
}
"#;
    let path = write_tmp("int_match.almd", src);
    let out = Command::new(almide()).args(["check", &path]).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let all = format!("{}{}", stdout, stderr);
    assert!(all.contains("E010"));
    assert!(all.contains("catch-all"), "Int hint missing:\n{}", all);
}
