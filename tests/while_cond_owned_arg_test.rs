//! A `String` param handed on to an owned slot inside a `while` condition
//! (#4752). The classic Rust route borrows such a param (`id: &str`) because
//! every loop use is cloned, but the `while` condition was counted as running
//! once, so the call there received the bare `&str` and rustc refused it
//! (E0308). The same param used in a `for` head, a `match` scrutinee in a
//! loop body and an `if` in the body is the control: those always compiled.
use std::process::Command;

const SOURCE: &str = r#"fn dir(id: String) -> String = "/tmp/" + id

effect fn edir(id: String) -> String = "/e/" + id

fn in_while(id: String) -> Int = {
  var n = 0
  while n < 3 and string.len(dir(id)) > 0 {
    n = n + 1
  }
  n
}

effect fn in_while_effect(id: String) -> Int = {
  var n = 0
  while n < 2 and string.len(edir(id)!) > 3 {
    n = n + 1
  }
  n
}

fn in_loop_positions(id: String) -> Int = {
  var n = 0
  for c in string.to_list(dir(id)) {
    n = n + string.len(c)
  }
  var k = 0
  while k < 2 {
    match dir(id) {
      "" => { k = k + 2 },
      _ => { k = k + 1 },
    }
    if string.len(dir(id)) > 0 then k = k + 0 else k = k + 1
  }
  n + k
}

effect fn main() -> Unit = {
  let id = list.get(["x"], 0) ?? "a"
  println(int.to_string(in_while(id)))
  println(int.to_string(in_while_effect(id)!))
  println(int.to_string(in_loop_positions(id)))
}
"#;

#[test]
fn a_string_param_handed_on_in_a_while_condition_compiles_on_the_rust_route() {
    let dir = tempfile::tempdir().expect("scratch");
    let src = dir.path().join("w.almd");
    std::fs::write(&src, SOURCE).expect("source");
    let out = Command::new(env!("CARGO_BIN_EXE_almide")).arg(&src).args(["--target", "rust"]).output().expect("emit rust");
    assert!(out.status.success(), "emit: {}", String::from_utf8_lossy(&out.stderr));
    let rs = dir.path().join("w.rs");
    std::fs::write(&rs, &out.stdout).expect("rust");
    let bin = dir.path().join("w");
    let rustc = Command::new("rustc")
        .args(["--edition", "2021", "-A", "warnings", "-o"])
        .arg(&bin)
        .arg(&rs)
        .output()
        .expect("rustc");
    assert!(rustc.status.success(), "rustc refused the --target rust output:\n{}", String::from_utf8_lossy(&rustc.stderr));
    let run = Command::new(&bin).output().expect("run");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "3\n2\n8\n");
}
