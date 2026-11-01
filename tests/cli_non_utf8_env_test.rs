//! #4388: `almide run` on either target survives an environment, a cwd and
//! an argument that are not UTF-8, and agrees with the binary `almide build`
//! makes. `env.get` of a variable whose value is not UTF-8 answers `none`
//! on native (`std::env::var(..).ok()`) and on the embedded wasm host, and
//! `env.vars` leaves it out on both (C-133).
//!
//! An argument that is not UTF-8 reads as its lossy decode on all three
//! (#4391 rule 1).
//!
//! Negative control: before #4388, `std::env::vars()` in the run caches'
//! keys (src/cli/crate_inputs.rs, src/cli/wasm_run_cache.rs) panicked, so
//! both legs exited 101 before compiling. `std::env::args()` in
//! src/main.rs did the same for an argument, and before #4391 the built
//! binary's `env.args` panicked on one.
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
fn a_non_utf8_cwd_never_panics_and_an_argument_reads_as_its_lossy_decode() {
    let t = tempfile::tempdir().expect("scratch");
    let src = t.path().join("app.almd");
    std::fs::write(
        &src,
        r#"import env

fn hx(s: String) -> String = "${bytes.to_list(bytes.from_string(s))}"

effect fn main() -> Unit = println("${env.args() |> list.map((a) => hx(a))}")
"#,
    )
    .unwrap();
    let dir = t.path().join(OsStr::from_bytes(b"d\x80"));
    std::fs::create_dir(&dir).unwrap();
    let bin = t.path().join("app");
    let built = seen(Command::new(almide()).arg("build").arg(&src).arg("-o").arg(&bin));
    assert_eq!(built.0, Some(0), "almide build: {}", built.2);
    // #4391 rule 1: the built binary, `almide run` and `almide run --target
    // wasm` all hand the program the argument's lossy decode.
    let want = "[\"[97, 239, 191, 189, 98]\"]\n";
    let (code, out, err) = seen(Command::new(&bin).arg(OsStr::from_bytes(b"a\x80b")));
    assert_eq!((code, out.as_str()), (Some(0), want), "built binary:\n{err}");
    for target in [None, Some("wasm")] {
        let mut c = Command::new(almide());
        c.arg("run").arg(&src).current_dir(&dir).env("PWD", &dir);
        if let Some(t) = target {
            c.args(["--target", t]);
        }
        let (code, out, err) = seen(&mut c);
        assert!(!err.contains("panicked"), "cwd, {target:?}:\n{err}");
        assert_eq!((code, out.as_str()), (Some(0), "[]\n"), "cwd, {target:?}:\n{err}");
        let mut c = Command::new(almide());
        c.arg("run").arg(&src);
        if let Some(t) = target {
            c.args(["--target", t]);
        }
        c.arg("--").arg(OsStr::from_bytes(b"a\x80b"));
        let (code, out, err) = seen(&mut c);
        assert!(!err.contains("panicked"), "argument, {target:?}:\n{err}");
        assert_eq!((code, out.as_str()), (Some(0), want), "argument, {target:?}:\n{err}");
    }
}

/// #4391 rules 2 and 4: a directory entry whose name is not UTF-8 is listed
/// as its lossy decode by `fs.list_dir`, `fs.walk` and `fs.glob` on native and
/// on the embedded host. Stock wasmtime's `fd_readdir` refuses such a
/// directory (WASI `ilseq`) before the artifact sees a name; that refusal is
/// the classified host-limitation text, never `wasi errno 25`.
#[test]
fn a_non_utf8_entry_name_lists_as_its_lossy_decode_and_stock_wasmtime_refuses_it_by_name() {
    let t = tempfile::tempdir().expect("scratch");
    let base = std::fs::canonicalize(t.path()).expect("canonical");
    let d = base.join("d");
    std::fs::create_dir(&d).unwrap();
    std::fs::write(d.join(OsStr::from_bytes(b"f\x80g")), "").unwrap();
    std::fs::write(d.join("ok"), "").unwrap();
    let src = base.join("app.almd");
    let dir = d.to_str().expect("utf8 scratch");
    std::fs::write(
        &src,
        format!(
            r#"import fs

fn hx(s: String) -> String = "${{bytes.to_list(bytes.from_string(s))}}"

fn names(r: Result[List[String], String]) -> String = match r {{
  ok(xs) => "ok ${{xs |> list.map((x) => hx(x))}}",
  err(e) => "err ${{e}}",
}}

effect fn main() -> Unit = {{
  println("list ${{names(fs.list_dir("{dir}"))}}")
  println("walk ${{names(fs.walk("{dir}"))}}")
  println("glob ${{names(fs.glob("{dir}/*"))}}")
}}
"#
        ),
    )
    .unwrap();
    let pre = dir.bytes().map(|b| b.to_string()).collect::<Vec<_>>().join(", ");
    let want = format!(
        "list ok [\"[102, 239, 191, 189, 103]\", \"[111, 107]\"]\n\
         walk ok [\"[{pre}, 47, 102, 239, 191, 189, 103]\", \"[{pre}, 47, 111, 107]\"]\n\
         glob ok [\"[{pre}, 47, 102, 239, 191, 189, 103]\", \"[{pre}, 47, 111, 107]\"]\n"
    );
    for target in [None, Some("wasm")] {
        let mut c = Command::new(almide());
        c.arg("run").arg(&src);
        if let Some(t) = target {
            c.args(["--target", t]);
        }
        let (code, out, err) = seen(&mut c);
        assert_eq!((code, out.as_str()), (Some(0), want.as_str()), "almide run {target:?}:\n{err}");
    }
    let wasm = base.join("app.wasm");
    let built = seen(Command::new(almide()).arg("build").arg(&src).args(["--target", "wasm", "-o"]).arg(&wasm));
    assert_eq!(built.0, Some(0), "almide build --target wasm: {}", built.2);
    let Ok(o) = Command::new("wasmtime").arg("--dir=/").arg(&wasm).output() else {
        assert!(std::env::var_os("ALMIDE_EXPECT_TOOLS").is_none(), "ALMIDE_EXPECT_TOOLS: wasmtime is not on PATH");
        return;
    };
    let refused = "the WASI host refuses a name that is not valid UTF-8";
    let out = String::from_utf8_lossy(&o.stdout);
    assert_eq!(
        out,
        format!(
            "list err fs.list_dir(\"{dir}\"): {refused}\nwalk err fs.walk(\"{dir}\"): {refused}\nglob err fs.glob(\"{dir}\"): {refused}\n"
        ),
        "stock wasmtime"
    );
}
