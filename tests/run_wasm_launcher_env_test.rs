//! #4774 — `almide run --target wasm` gives the program the environment
//! `almide run` gives the native child: `ALMIDE_CWD` (the launcher's working
//! directory, #874) is seen by `env.get` and listed by `env.vars` on both
//! targets. The embedded host read only its own process environment, which
//! the launcher never exported the variable into, so wasm answered `none`.

use std::process::Command;

const SRC: &str = r#"import env
effect fn main() -> Unit = {
  let vs = env.vars()!
  let g = env.get("ALMIDE_CWD") ?? "<unset>"
  println("vars: ${map.get(vs, "ALMIDE_CWD") ?? "<unset>"}; get: ${g}")
}
"#;

fn run(dir: &std::path::Path, wasm: bool) -> String {
    let mut args = vec!["run", "prog.almd"];
    if wasm {
        args.extend(["--target", "wasm"]);
    }
    let o = Command::new(env!("CARGO_BIN_EXE_almide")).args(&args).current_dir(dir).output().expect("almide run");
    assert!(o.status.success(), "{args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).into_owned()
}

#[test]
fn both_targets_see_the_launchers_cwd() {
    let dir = std::env::temp_dir().join(format!("almd_launcher_env_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("prog.almd"), SRC).unwrap();
    let cwd = dir.canonicalize().unwrap();
    let want = format!("vars: {0}; get: {0}\n", cwd.display());
    let native = run(&cwd, false);
    let wasm = run(&cwd, true);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(native, want, "native");
    assert_eq!(wasm, native, "wasm differs from native");
}
