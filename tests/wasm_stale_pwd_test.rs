//! Cross-target fixture for #874: relative fs paths must resolve against the
//! launcher's REAL working directory even when the inherited `PWD` env var is
//! stale — the exact spawn shape of Node `execFileSync(..., { cwd })` and IDE
//! run configs, which set the child cwd without touching `PWD`.
//!
//! The launcher exports `ALMIDE_CWD` (its `current_dir()`) to both targets;
//! the wasm `$path_norm` prefers it over `PWD`, and `env.get("PWD")` keeps
//! observing the same (stale) inherited value on both legs.

use std::path::PathBuf;
use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

const PROGRAM: &str = r#"import fs
import env

effect fn main() -> Result[Unit, String] = {
  let text = fs.read_text("data.csv")
  println("read: ${string.trim(text)}")
  println("pwd: ${env.get("PWD") ?? "none"}")
  ok(())
}
"#;

fn run_with_stale_pwd(proj: &PathBuf, wasm: bool) -> (i32, String) {
    let mut args = vec!["run", "src/main.almd"];
    if wasm {
        args.push("--target");
        args.push("wasm");
    }
    let out = Command::new(almide())
        .args(&args)
        .current_dir(proj) // sets the child cwd WITHOUT updating PWD…
        .env("PWD", "/nonexistent-stale-pwd") // …which stays stale
        .output()
        .expect("run almide");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
    )
}

#[test]
fn relative_fs_paths_survive_a_stale_pwd_on_both_targets() {
    let dir = tempfile::tempdir().expect("tempdir");
    let proj = dir.path().to_path_buf();
    std::fs::create_dir_all(proj.join("src")).expect("mkdir src");
    std::fs::write(proj.join("data.csv"), "hello,world\n").expect("write data");
    std::fs::write(proj.join("src/main.almd"), PROGRAM).expect("write program");

    let (native_code, native_out) = run_with_stale_pwd(&proj, false);
    assert_eq!(native_code, 0, "native leg failed:\n{native_out}");
    assert!(
        native_out.contains("read: hello,world"),
        "native must read the relative file:\n{native_out}"
    );
    assert!(
        native_out.contains("pwd: /nonexistent-stale-pwd"),
        "env.get(\"PWD\") must keep the inherited (stale) value:\n{native_out}"
    );

    if !wasmtime_available() {
        eprintln!("SKIP: wasmtime not on PATH — stale-PWD parity enforced on Linux CI");
        return;
    }
    let (wasm_code, wasm_out) = run_with_stale_pwd(&proj, true);
    assert_eq!(wasm_code, 0, "wasm leg failed:\n{wasm_out}");
    assert_eq!(
        native_out, wasm_out,
        "stale-PWD relative-path behavior diverged across targets"
    );
}
