//! Assigning a propagating value to a module-level `var` (`g = f()!`).
//!
//! A module global is a thread-local, and the assignment was emitted inside
//! its `with` closure — `G.with(|c| c.set(f()?))` — so the `?` tried to
//! return from the closure, whose type is `()`: rustc E0277, and the program
//! did not build. The value is now evaluated first, outside the closure.

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

const SRC: &str = r#"
var count = 0
var label = "none"
var data: List[Int] = []

effect fn parse(s: String) -> Result[Int, String] = int.parse(s)
effect fn name(n: Int) -> Result[String, String] = if n > 0 then ok("n${int.to_string(n)}") else err("not positive")
effect fn items(n: Int) -> Result[List[Int], String] = ok(0..<n |> list.map((i) => i * n))

effect fn step(s: String) -> Result[Unit, String] = {
  count = parse(s)!
  label = name(count)!
  data = items(count)!
  ok(())
}

effect fn main() -> Unit = {
  step("3")!
  println("${int.to_string(count)} ${label} ${int.to_string(list.len(data))}")
  println(match step("0") { ok(_) => "ok", err(e) => e })
  println("${int.to_string(count)} ${label}")
}
"#;

fn run(target: Option<&str>) -> String {
    let dir = std::env::temp_dir().join(format!("almide-global-assign-{}", target.unwrap_or("native")));
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("main.almd");
    std::fs::write(&file, SRC).unwrap();
    let mut cmd = Command::new(almide_bin());
    cmd.arg("run").arg(&file);
    if let Some(t) = target {
        cmd.args(["--target", t]);
    }
    let out = cmd.output().expect("spawn almide");
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

const WANT: &str = "3 n3 3\nnot positive\n0 n3\n";

#[test]
fn propagating_assignment_to_a_global_builds_and_runs_natively() {
    let out = run(None);
    assert!(out.contains(WANT), "native:\n{out}");
}

#[test]
fn propagating_assignment_to_a_global_runs_on_wasm_the_same() {
    let out = run(Some("wasm"));
    assert!(out.contains(WANT), "wasm:\n{out}");
}
