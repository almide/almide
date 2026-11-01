//! #4386: a `guard … else continue` inside an `if` arm in a loop body leaves
//! the loop ITERATION. The MIR guard desugar read that `continue` as `()` in
//! the nested block, so the verified native render fell through to the
//! loop-body statements after the `if` and printed `.x2..x4.`.

use std::process::Command;

const PROGRAM: &str = r#"fn under_if(xs: List[Int]) -> String = {
  var out = ""
  for x in xs {
    let label = "x${x}"
    if x > 1 then {
      guard x != 3 else continue
      out = out + label
    } else ()
    out = out + "."
  }
  out
}

effect fn main() -> Unit = {
  println(under_if([1, 2, 3, 4]))
}
"#;

#[test]
fn a_guard_continue_in_an_if_arm_skips_the_rest_of_the_iteration() {
    let dir = std::env::temp_dir().join(format!("almide-4386-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("under_if.almd");
    std::fs::write(&file, PROGRAM).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).output().expect("spawn almide");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(out.status.success(), "native run failed: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), ".x2.x4.");
}

