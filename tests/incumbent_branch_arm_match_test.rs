//! A `match` sitting in a BRANCH ARM's tail used to have no real-branch route
//! on the incumbent wasm leg (#2325): its `if` sibling tried `try_lower_unit_if`
//! and only fell back to the linearization on a decline, while a `match` went
//! straight to the linearization — which WALLS the moment an arm bears a call or
//! an assignment, because running both arms would run the untaken arm's effects.
//! So an ordinary dispatch table nested inside a branch was unrenderable.
//!
//! The shape that made it a release blocker is generated, not written: the
//! mutual-TCO pass fuses a tail-recursive group into one dispatcher whose member
//! arms sit inside the `if tag == k` chain, so a member containing `match kind {
//! … }` took the fused helper down with it, and every caller then failed to
//! link. The first test is exactly that shape in user code; the second is the
//! mechanism on its own, and pins that only the MATCHED arm runs.
use std::process::Command;

/// `step` and `bump` are mutually tail-recursive, so the optimizer fuses them
/// into `__mutual_tco_0_bump` — whose body carries `step`'s `match kind`.
const FUSED: &str = r#"fn step(kind: Int, n: Int) -> Int = match kind {
  1 => bump(n + 1),
  2 => bump(n + 10),
  _ => n,
}

fn bump(n: Int) -> Int = if n < 100 then step(if n % 2 == 0 then 1 else 2, n) else n

fn main() -> Unit = println(int.to_string(bump(0)))
"#;

/// Both halves of the linearization's refusal, nested in a branch arm: arms that
/// PRINT (a call) and arms that ASSIGN. Linearizing either would show up in the
/// output — three lines per `classify` call, or a net-zero `n`.
const ARMS: &str = r#"fn classify(k: Int) -> Unit = if k > 0 then {
  match k {
    1 => println("one"),
    2 => println("two"),
    _ => println("many"),
  }
} else println("none")

fn main() -> Unit = {
  classify(1)
  classify(2)
  classify(7)
  classify(0)
  var tag = 0
  var n = 0
  var steps = 0
  while steps < 6 {
    if steps >= 0 then {
      match tag {
        0 => {
          n = n + 1
          tag = 1
        },
        1 => {
          n = n + 10
          tag = 2
        },
        _ => {
          n = n + 100
          tag = 0
        },
      }
    } else n = 0 - 1
    steps = steps + 1
  }
  println(int.to_string(n))
}
"#;

fn almide() -> String {
    std::env::var("ALMIDE_BIN")
        .unwrap_or_else(|_| format!("{}/target/release/almide", env!("CARGO_MANIFEST_DIR")))
}

/// Run the source on the incumbent wasm leg and natively, and return the
/// incumbent's (success, stdout, stderr) plus native's stdout.
fn run_both(src: &str) -> (bool, String, String, String) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("main.almd");
    std::fs::write(&file, src).unwrap();
    let inc = Command::new(almide())
        .args(["run", file.to_str().unwrap(), "--target", "wasm"])
        .env("ALMIDE_WASM_INCUMBENT", "1")
        .output()
        .unwrap();
    let native = Command::new(almide()).args(["run", file.to_str().unwrap()]).output().unwrap();
    (
        inc.status.success(),
        String::from_utf8_lossy(&inc.stdout).trim().to_string(),
        String::from_utf8_lossy(&inc.stderr).to_string(),
        String::from_utf8_lossy(&native.stdout).trim().to_string(),
    )
}

#[test]
fn a_fused_mutual_tco_dispatcher_carrying_a_match_links_on_the_incumbent() {
    let (ok, out, err, native) = run_both(FUSED);
    assert!(ok, "the incumbent must render the fused dispatcher:\n{err}");
    assert_eq!(out, "101");
    assert_eq!(native, out, "native and the incumbent must agree");
}

#[test]
fn a_match_in_a_branch_arm_runs_only_the_matched_arm_on_the_incumbent() {
    let (ok, out, err, native) = run_both(ARMS);
    assert!(ok, "the incumbent must render a match nested in a branch arm:\n{err}");
    assert_eq!(out, "one\ntwo\nmany\nnone\n222");
    assert_eq!(native, out, "native and the incumbent must agree");
}
