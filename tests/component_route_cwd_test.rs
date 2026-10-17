//! A program that runs as its p3 component on the embedded lane (one naming
//! the full-response http family, ADR-0023 step 7) resolves relative paths
//! against the launcher's working directory, as native and the core-module
//! route do.
//!
//! The guest resolves relative paths against `ALMIDE_CWD`, else `PWD`
//! (#874). Native's child is handed `ALMIDE_CWD`; the component route only
//! inherited the environment, so a parent that set the child's cwd without
//! updating `PWD` had the component read the parent's directory instead.
//! Found by the ADR-0027 sandbox campaign (`xtarget-fuzz run --focus
//! http.put_response`): `fs.exists("empty.txt")` in the seeded sandbox
//! answered false on the embedded lane only.
use std::path::Path;
use std::process::Command;

const PROBE: &str = r#"import fs
import http

effect fn main() -> Unit = {
  let r: Result[HttpResponse, String] = http.get_response("http://127.0.0.1:9/x")
  println("${result.is_ok(r)}")
  println("${fs.exists("a.txt")}")
  println(fs.read_text("a.txt") ?? "unreadable")
}
"#;

const EXPECTED: &str = "false\ntrue\nhere\n\n";

fn run(dir: &Path, stale_pwd: &Path, args: &[&str]) -> String {
    let o = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("run")
        .arg(dir.join("probe.almd"))
        .args(args)
        .current_dir(dir)
        .env("PWD", stale_pwd)
        .env_remove("ALMIDE_CWD")
        .output()
        .expect("spawn");
    assert!(o.status.success(), "{args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).expect("utf8")
}

#[test]
fn the_component_route_reads_the_launchers_cwd_not_a_stale_pwd() {
    let root = tempfile::tempdir().expect("scratch");
    let dir = root.path().join("cwd");
    let stale = root.path().join("stale");
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::create_dir_all(&stale).expect("mkdir");
    std::fs::write(dir.join("a.txt"), "here\n").expect("write");
    std::fs::write(stale.join("a.txt"), "stale\n").expect("write");
    std::fs::write(dir.join("probe.almd"), PROBE).expect("write");
    assert_eq!(run(&dir, &stale, &[]), EXPECTED, "native");
    assert_eq!(run(&dir, &stale, &["--target", "wasm"]), EXPECTED, "the embedded lane's component route");
}
