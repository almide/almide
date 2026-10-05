//! A record field pattern that binds a name equal to its field (`Bag { name, .. }`)
//! is emitted in Rust's shorthand (`Bag::Bag { name, .. }`), not `name: name`,
//! which rustc reports as `non_shorthand_field_patterns` — generated Rust must
//! compile without warnings. A renamed binder (`Bag { name: n, .. }`) keeps the
//! `field: binder` form.

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

const PROGRAM: &str = r#"type Bag =
  | Bag { name: String, n: Int }

type Point = { x: Int, y: Int }

fn label(b: Bag) -> String =
  match b {
    Bag { name, .. } => name,
  }

fn renamed(b: Bag) -> Int =
  match b {
    Bag { n: count, .. } => count,
  }

fn sum(p: Point) -> Int =
  match p {
    Point { x, y } => x + y,
  }

fn main() -> Unit = {
  println(label(Bag { name: "x", n: 1 }))
  println(int.to_string(renamed(Bag { name: "y", n: 7 })))
  println(int.to_string(sum(Point { x: 2, y: 3 })))
}
"#;

#[test]
fn a_field_bound_under_its_own_name_is_emitted_in_shorthand() {
    let dir = std::env::temp_dir().join(format!("almide_record_shorthand_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("shorthand.almd");
    std::fs::write(&src, PROGRAM).unwrap();

    let emitted = Command::new(almide_bin()).arg(&src).args(["--target", "rust"]).output().expect("run almide");
    assert!(emitted.status.success(), "emit failed: {}", String::from_utf8_lossy(&emitted.stderr));
    let rust = String::from_utf8_lossy(&emitted.stdout);
    assert!(rust.contains("Bag::Bag { name, .. }"), "expected the shorthand field pattern");
    assert!(!rust.contains("name: name"), "a same-named binder must not render as `name: name`");
    assert!(rust.contains("n: count"), "a renamed binder keeps `field: binder`");

    let run = Command::new(almide_bin()).arg("run").arg(&src).output().expect("run almide");
    assert!(run.status.success(), "run failed: {}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "x\n7\n5\n");
    let _ = std::fs::remove_dir_all(&dir);
}
