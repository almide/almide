//! `let v = if a then f()! else if b then d else g()!` — a propagating
//! unwrap on an arm of an else-if chain bound to a let. The lift that turns
//! the two-arm branch into one Result read by a single `!` stopped at the
//! first level, so the chain reached the native render with a scalar arm
//! beside a Result arm and was refused (#4640). The chain now lifts arm by
//! arm. The program takes every arm on its ok and its err path, in a Result
//! fn and in `main`.
use std::process::Command;

const PROGRAM: &str = r##"effect fn worker(n: Int) -> Result[Int, String] = if n > 5 then err("big") else ok(n + 1)
effect fn sup(n: Int) -> Result[Int, String] = if n < 0 then err("neg") else ok(n * 2)
effect fn run(a: Int, b: Int) -> Result[Int, String] = {
  let code = if a == 1 then worker(b)!
    else if a == 2 and b == 2 then { println("usage"); 0 }
    else sup(b)!
  println("code " + int.to_string(code))
  ok(code)
}
effect fn main() -> Unit = {
  for a in 0..<4 {
    for b in [-1, 2, 7] {
      match run(a, b) {
        ok(n) => println("ok " + int.to_string(n)),
        err(e) => println("err " + e),
      }
    }
  }
  let code = if 1 == 1 then worker(3)!
    else if 2 == 3 then { println("usage"); 0 }
    else sup(1)!
  println(int.to_string(code))
}
"##;
const EXPECTED: &str = "err neg\ncode 4\nok 4\ncode 14\nok 14\ncode 0\nok 0\ncode 3\nok 3\nerr big\nerr neg\nusage\ncode 0\nok 0\ncode 14\nok 14\nerr neg\ncode 4\nok 4\ncode 14\nok 14\n4\n";

/// Both legs print `EXPECTED`; the native one is the certified render, held
/// to the ownership certificate with no fallback to the standard codegen.
fn both_legs_print(program: &str, expected: &str) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("prog.almd");
    std::fs::write(&file, program).expect("source");
    for extra in [&[][..], &["--target", "wasm"][..]] {
        let out = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).args(extra)
            .env("ALMIDE_CERTIFY_OWNERSHIP", "fail").env("ALMIDE_VERBOSE", "1")
            .current_dir(dir.path()).output().expect("almide run");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(0), "{extra:?}: {stderr}");
        assert!(!stderr.contains("walled"), "{extra:?} fell back to the standard codegen: {stderr}");
        assert_eq!(String::from_utf8_lossy(&out.stdout), expected, "{extra:?}");
    }
}

#[test]
fn every_arm_of_the_chain_propagates_on_both_legs() {
    both_legs_print(PROGRAM, EXPECTED);
}
