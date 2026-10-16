//! #3752: the ADR-0001 clock types can be spelled in annotations — a `let`,
//! a parameter, a return type — and such a program builds and runs on both
//! legs. Every annotation was E029 "unknown type"; once the checker accepted
//! them, the annotated operands still carried the checker's tag past the
//! clock firewall (an IR-verify ICE on `x + x`, a wasm `bind-ty:Named` wall).
use std::process::Command;

const SRC: &str = r#"fn twice(x: Duration) -> Duration = x + x
fn budget(n: Int) -> Compute = compute.ms(n)

effect fn main() -> Unit = {
  let d: Duration = duration.ms(1500)
  let c: Compute = budget(2)
  let r = fan.bounded(c) { 41 + 1 } ?? -1
  println("${twice(d)} ${c - compute.ms(3)} ${c * 3} ${r} ${twice(d) > d}")
}
"#;

fn run(target: Option<&str>) -> (bool, String, String) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("clock.almd");
    std::fs::write(&file, SRC).expect("source");
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(&file);
    if let Some(t) = target {
        c.args(["--target", t]);
    }
    let out = c.output().expect("run");
    (out.status.success(), String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn annotated_clock_values_run_on_both_legs() {
    for target in [None, Some("wasm")] {
        let (ok, stdout, stderr) = run(target);
        assert!(ok, "{target:?}: {stderr}");
        assert_eq!(stdout, "3000000000 0 6000000 42 true\n", "{target:?}");
    }
}
