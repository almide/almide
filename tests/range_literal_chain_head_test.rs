//! #3604 / #3810: a range literal (`a..<b`) that heads a fold is the same
//! list as `list.range(a, b)`, and both legs count it the same way —
//! `(a..b).fold(..)` natively, never a `Vec<i64>` built per call and walked
//! once. Besides the allocation, the built list hid the index range from
//! LLVM: `v[j]` over a `Vec`-sourced `j` kept its bounds check in the loop,
//! and spectralnorm's indexed spelling ran 1.37x its imperative twin on x86
//! under `--release`. A collecting chain (`list.map(0..<n, f)`) and an
//! inclusive `a...b` keep the built list (see `range_literal_fold_bounds`).
use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

const SRC: &str = r#"
fn row(v: List[Float], n: Int) -> Float =
  list.fold(0..<n, 0.0, (acc, j) => acc + float.from_int(j) * v[j])

fn rows(v: List[Float], n: Int) -> List[Float] =
  list.map(0..<n, (i) => row(v, n) + float.from_int(i))

fn incl(n: Int) -> Int = list.fold(0...n, 0, (a, x) => a + x)

effect fn main() -> Unit = {
  let v = [1.0, 2.0, 3.0, 4.0]
  println(float.to_string(row(v, 4)))
  println(float.to_string(row(v, 0)))
  println(float.to_string(row(v, -3)))
  println(int.to_string(list.len(rows(v, 4))))
  println(int.to_string(incl(4)))
  println(int.to_string(list.fold(5..<2, 7, (a, x) => a + x)))
}
"#;

fn write() -> (std::path::PathBuf, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("almide-range-literal-head-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("range_literal_head.almd");
    std::fs::write(&path, SRC).unwrap();
    (dir, path)
}

fn fn_body<'a>(rust: &'a str, name: &str) -> &'a str {
    let i = rust.find(&format!("pub fn {name}(")).unwrap_or_else(|| panic!("no fn {name}"));
    rust[i..].split("\n}\n").next().unwrap()
}

#[test]
fn a_range_literal_chain_head_is_counted_not_built() {
    let (dir, path) = write();
    let out = Command::new(almide()).arg(&path).args(["--target", "rust"]).output().unwrap();
    assert!(out.status.success(), "emit failed:\n{}", String::from_utf8_lossy(&out.stderr));
    let rust = String::from_utf8_lossy(&out.stdout).to_string();
    let body = fn_body(&rust, "row");
    assert!(!body.contains("__almide_rv"), "row: the range literal fold head was built:\n{body}");
    assert!(body.contains("(0i64..n).fold("), "row: the fold head is not the counted range:\n{body}");
    let run = Command::new(almide()).arg("run").arg(&path).output().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(run.status.success(), "run failed:\n{}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "20.0\n0.0\n0.0\n4\n10\n7\n");
}
