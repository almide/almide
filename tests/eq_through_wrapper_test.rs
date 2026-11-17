//! E016 reaches a call through a generic wrapper (#4796, the program half).
//!
//! `fn uniq[A](xs: List[A]) -> List[A] = list.unique(xs)`: the equality
//! `list.unique` needs is checked where it is called, where `A` is still a
//! type variable, so `uniq` over a list of functions passed check and then
//! failed to build. The compare is now a requirement the wrapper carries to
//! its concrete calls, as the Secret rule (#4239) already carried it. The
//! stdlib's own wrappers (`deque.unique`) are checked in another pass, so
//! their requirement needs the stdlib-wide table #4796 tracks.

use std::io::Write;
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

fn check(tag: &str, src: &str) -> (bool, String) {
    let dir = std::env::temp_dir().join(format!("almd_4796_{tag}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.almd");
    std::fs::File::create(&file).unwrap().write_all(src.as_bytes()).unwrap();
    let out = Command::new(almide_bin()).args(["check", file.to_str().unwrap()]).output().expect("spawn almide");
    std::fs::remove_dir_all(&dir).ok();
    (out.status.success(), format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
}

#[test]
fn a_wrapper_over_a_function_list_is_e016() {
    let (ok, text) = check(
        "fn",
        r#"fn uniq[A](xs: List[A]) -> List[A] = list.unique(xs)
fn h(x: Int) -> Int = x
effect fn main() -> Unit = {
  let d = uniq([h, h])
  println("${list.len(d)}")
}
"#,
    );
    assert!(!ok, "check accepted it:\n{text}");
    assert!(text.contains("E016"), "expected E016:\n{text}");
}

#[test]
fn a_wrapper_over_an_eq_type_still_checks() {
    let (ok, text) = check(
        "int",
        r#"fn uniq[A](xs: List[A]) -> List[A] = list.unique(xs)
effect fn main() -> Unit = {
  let d = uniq([1, 2, 2, 3])
  println("${list.len(d)}")
}
"#,
    );
    assert!(ok, "check refused it:\n{text}");
}
