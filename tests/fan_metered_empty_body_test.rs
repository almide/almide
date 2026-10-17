//! #3783: an empty `fan.bounded` / `fan.timeout` body compiled on native and
//! was E082 (`Block-no-tail`) on wasm. Decision: an empty body is a Unit block,
//! as `{}` is in Rust, Kotlin, Swift and Go — accepted on every target, with
//! the same verdict at any budget (it has no charge site).
use std::process::Command;

const SOURCE: &str = r#"fn verdict(r: Result[Unit, String]) -> String = match r {
  ok(_) => "ok",
  err(e) => "err " + e,
}

effect fn main() -> Unit = {
  println(verdict(fan.bounded(compute.ms(1)) { }))
  println(verdict(fan.bounded(compute.ns(0)) { }))
  println(verdict(fan.timeout(duration.ms(100)) { }))
  let u = fan.bounded(compute.ms(1)) { } ?? ()
  println("fused ${verdict(ok(u))}")
}
"#;

fn run(file: &std::path::Path, wasm: bool) -> String {
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(file);
    if wasm {
        c.args(["--target", "wasm"]);
    }
    let out = c.output().expect("almide");
    assert!(out.status.success(), "wasm={wasm}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn an_empty_metered_body_is_unit_on_every_target() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("empty_body.almd");
    std::fs::write(&file, SOURCE).expect("source");
    let native = run(&file, false);
    assert_eq!(native, "ok\nok\nok\nfused ok\n");
    assert_eq!(run(&file, true), native, "native vs wasm");
}
