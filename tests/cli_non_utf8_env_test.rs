//! #4388: `almide run` on either target survives an environment, a cwd and
//! an argument that are not UTF-8, and agrees with the binary `almide build`
//! makes. `env.get` of a variable whose value is not UTF-8 answers `none`
//! on native (`std::env::var(..).ok()`) and on the embedded wasm host, and
//! `env.vars` leaves it out on both (C-133).
//!
//! Negative control: before #4388, `std::env::vars()` in the run caches'
//! keys (src/cli/crate_inputs.rs, src/cli/wasm_run_cache.rs) panicked, so
//! both legs exited 101 before compiling. `std::env::args()` in
//! src/main.rs did the same for an argument.
#![cfg(unix)]

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

const PROGRAM: &str = r#"import env

effect fn main() -> Unit = {
  println(match env.get("ALMIDE_T4388_BAD") {
    some(v) => "some ${string.len(v)}",
    none => "none",
  })
  println(match env.get("ALMIDE_T4388_OK") {
    some(v) => "some ${v}",
    none => "none",
  })
  let names = map.keys(env.vars()!)
  println("listed bad=${list.contains(names, "ALMIDE_T4388_BAD")} ok=${list.contains(names, "ALMIDE_T4388_OK")}")
}
"#;

/// (exit code, stdout, stderr) of `cmd`.
fn seen(cmd: &mut Command) -> (Option<i32>, String, String) {
    let o = cmd.output().expect("spawn");
    (o.status.code(), String::from_utf8_lossy(&o.stdout).into_owned(), String::from_utf8_lossy(&o.stderr).into_owned())
}

#[test]
fn a_non_utf8_variable_reads_as_none_on_run_native_run_wasm_and_the_built_binary() {
    let t = tempfile::tempdir().expect("scratch");
    let src = t.path().join("app.almd");
    std::fs::write(&src, PROGRAM).unwrap();
    let bad = OsStr::from_bytes(b"x\x80y");
    let want = "none\nsome fine\nlisted bad=false ok=true\n";
    let bin = t.path().join("app");
    let built = seen(Command::new(almide()).arg("build").arg(&src).arg("-o").arg(&bin).env("ALMIDE_T4388_BAD", bad));
    assert_eq!(built.0, Some(0), "almide build: {}", built.2);
    let legs: [(&str, Command); 3] = [
        ("built binary", Command::new(&bin)),
        ("almide run", {
            let mut c = Command::new(almide());
            c.arg("run").arg(&src);
            c
        }),
        ("almide run --target wasm", {
            let mut c = Command::new(almide());
            c.arg("run").arg(&src).args(["--target", "wasm"]);
            c
        }),
    ];
    for (leg, mut cmd) in legs {
        let (code, out, err) = seen(cmd.env("ALMIDE_T4388_BAD", bad).env("ALMIDE_T4388_OK", "fine"));
        assert!(!err.contains("panicked"), "{leg} panicked:\n{err}");
        assert_eq!((code, out.as_str()), (Some(0), want), "{leg}:\n{err}");
    }
}

#[test]
fn a_non_utf8_cwd_and_argument_never_panic_the_cli() {
    let t = tempfile::tempdir().expect("scratch");
    let src = t.path().join("app.almd");
    std::fs::write(&src, "effect fn main() -> Unit = println(\"hi\")\n").unwrap();
    let dir = t.path().join(OsStr::from_bytes(b"d\x80"));
    std::fs::create_dir(&dir).unwrap();
    for target in [None, Some("wasm")] {
        let mut c = Command::new(almide());
        c.arg("run").arg(&src).current_dir(&dir).env("PWD", &dir);
        if let Some(t) = target {
            c.args(["--target", t]);
        }
        let (code, out, err) = seen(&mut c);
        assert!(!err.contains("panicked"), "cwd, {target:?}:\n{err}");
        assert_eq!((code, out.as_str()), (Some(0), "hi\n"), "cwd, {target:?}:\n{err}");
        // An argument clap cannot read as text is a usage error, not a panic.
        let mut c = Command::new(almide());
        c.arg("run").arg(&src);
        if let Some(t) = target {
            c.args(["--target", t]);
        }
        c.arg("--").arg(OsStr::from_bytes(b"a\x80b"));
        let (code, _, err) = seen(&mut c);
        assert!(!err.contains("panicked"), "argument, {target:?}:\n{err}");
        assert_ne!(code, Some(101), "argument, {target:?}:\n{err}");
    }
}
