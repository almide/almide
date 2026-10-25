//! A Unit effect intrinsic bound to an annotated `let` (`let n: Unit =
//! io.print(…)`) runs on both legs; `env.set(…)!`, the ok value of a
//! fallible set (#4200), binds the same way. The wasm leg represents these calls as
//! a `Result[Unit, String]` carrier; in statement position the carrier was
//! dropped, but a binding that wanted a Unit refused it with
//! `wall: ty-mismatch:Result(..)-vs-Unit` while native ran the program.
//! Found by the fuzzer's sandboxed effect campaign (tier E1).
use std::process::Command;

const SOURCE: &str = r#"import env
import io

effect fn main() -> Unit = {
  let a: Unit = env.set("XF_UNIT_LET", "set")!
  let b: Unit = io.print("printed ")
  let c: Unit = env.sleep_ms(0)
  println(env.get("XF_UNIT_LET") ?? "unset")
}
"#;

fn run(wasm: bool) -> String {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("unit_let.almd");
    std::fs::write(&file, SOURCE).expect("source");
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(&file);
    if wasm {
        c.args(["--target", "wasm"]);
    }
    let out = c.output().expect("almide run");
    assert!(out.status.success(), "wasm={wasm}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn a_unit_effect_bound_to_a_unit_let_runs_on_both_legs() {
    assert_eq!(run(false), "printed set\n", "native");
    assert_eq!(run(true), "printed set\n", "wasm");
}
