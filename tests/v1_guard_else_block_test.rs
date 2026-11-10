//! `guard c else { …; err(e) }` in an effect fn that returns Unit, on the
//! certified native render.
//!
//! Such a fn's body gets an `ok(())` value on every path that ends in Unit.
//! The else block of the guard ends in the fn's own Result, and it was taken
//! for a Unit path too: its `err(e)` became a discarded statement in front of
//! `ok(())`, so the caller saw success and the run went on.
use std::process::Command;

/// The early exit seen by a caller that inspects it and by one that propagates.
const ELSE_BLOCK: &str = r##"effect fn work(n: Int) -> Unit = {
  println("before")
  guard n > 0 else {
    println("leaving")
    err("E" + int.to_string(n))
  }
  println("in")
}

effect fn main() -> Unit = {
  for n in [2, 0] {
    match work(n) {
      ok(_) => println("fine"),
      err(e) => println("caught " + e),
    }
  }
  work(-1)!
  println("end")
}
"##;

const ELSE_BLOCK_STDOUT: &str = "before\nin\nfine\nbefore\nleaving\ncaught E0\nbefore\nleaving\n";

#[test]
fn an_err_ending_a_guard_else_block_of_a_unit_effect_fn_reaches_the_caller() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("guard_else_block.almd");
    std::fs::write(&file, ELSE_BLOCK).expect("source");
    for extra in [&[][..], &["--target", "wasm"][..]] {
        let run = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).args(extra)
            .env("ALMIDE_CERTIFY_OWNERSHIP", "fail").env("ALMIDE_VERBOSE", "1")
            .current_dir(dir.path()).output().expect("almide run");
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert_eq!(run.status.code(), Some(1), "{extra:?}: {stderr}");
        assert_eq!(String::from_utf8_lossy(&run.stdout), ELSE_BLOCK_STDOUT, "{extra:?}");
        assert!(stderr.contains("Error: E-1"), "{extra:?}: {stderr}");
        assert!(!stderr.contains("walled"), "{extra:?} fell back: {stderr}");
    }
}
