//! #4388: `almide run` on either target survives an environment, a cwd and
//! an argument that are not UTF-8, and agrees with the binary `almide build`
//! makes. `env.get` of a variable whose value is not UTF-8 answers `none`
//! on native (`std::env::var(..).ok()`) and on the embedded wasm host, and
//! `env.vars` leaves it out on both (C-133).
//!
//! #4391 (C-449, C-450): an argument and a directory entry name that are
//! not UTF-8 read as their lossy decode on every leg, and stock wasmtime's
//! refusal of such a name is a classified error.
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
  println(match env.get("T4388_BAD") {
    some(v) => "some ${string.len(v)}",
    none => "none",
  })
  println(match env.get("T4388_OK") {
    some(v) => "some ${v}",
    none => "none",
  })
  let names = map.keys(env.vars()!)
  println("listed bad=${list.contains(names, "T4388_BAD")} ok=${list.contains(names, "T4388_OK")}")
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
    let built = seen(Command::new(almide()).arg("build").arg(&src).arg("-o").arg(&bin).env("T4388_BAD", bad));
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
        let (code, out, err) = seen(cmd.env("T4388_BAD", bad).env("T4388_OK", "fine"));
        assert!(!err.contains("panicked"), "{leg} panicked:\n{err}");
        assert_eq!((code, out.as_str()), (Some(0), want), "{leg}:\n{err}");
    }
}

/// The C-449 fixture: `env.args` as hex, `env.get` / `env.vars` of
/// `T4391_BAD` and `T4391_OK`.
const ARGS_FIXTURE: &str = include_str!("../spec/embedded_cross/env_non_utf8_host_text.almd");

/// What every leg prints for the fixture given `a\x80b` as its argument and
/// as `T4391_BAD`'s value: the argument's lossy decode, the variable unread.
const ARGS_WANT: &str = "args=[\"[97, 239, 191, 189, 98]\"]\nbad none\nok some fine\nlisted bad=false ok=true\n";

#[test]
fn a_non_utf8_cwd_never_panics_and_an_argument_reads_as_its_lossy_decode() {
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
    }
    // #4391: an argument that is not UTF-8 is its lossy decode on the built
    // binary and under `almide run` on both targets — never exit 101.
    let src = t.path().join("args.almd");
    std::fs::write(&src, ARGS_FIXTURE).unwrap();
    let bin = t.path().join("args");
    let built = seen(Command::new(almide()).arg("build").arg(&src).arg("-o").arg(&bin));
    assert_eq!(built.0, Some(0), "almide build: {}", built.2);
    let raw = OsStr::from_bytes(b"a\x80b");
    let legs: [(&str, Command); 3] = [
        ("built binary", Command::new(&bin)),
        ("almide run", {
            let mut c = Command::new(almide());
            c.arg("run").arg(&src).arg("--");
            c
        }),
        ("almide run --target wasm", {
            let mut c = Command::new(almide());
            c.arg("run").arg(&src).args(["--target", "wasm", "--"]);
            c
        }),
    ];
    for (leg, mut cmd) in legs {
        let (code, out, err) = seen(cmd.arg(raw).env("T4391_BAD", raw).env("T4391_OK", "fine"));
        assert!(!err.contains("panicked"), "{leg} panicked:\n{err}");
        assert_eq!((code, out.as_str()), (Some(0), ARGS_WANT), "{leg}:\n{err}");
    }
}

/// The C-450 fixture: names that are not UTF-8, made by the shell, listed
/// by `fs.list_dir`, `fs.walk` and `fs.glob`.
const NAMES_FIXTURE: &str = include_str!("../spec/embedded_cross/fs_non_utf8_entry_names.almd");

/// A directory holding `f\x80g`, `ok`, `é` and `\x80z`, or `None` on a
/// filesystem that refuses a name that is not UTF-8 (APFS answers EILSEQ).
fn non_utf8_dir(root: &std::path::Path) -> Option<std::path::PathBuf> {
    let d = root.join("names");
    std::fs::create_dir(&d).unwrap();
    for name in [&b"f\x80g"[..], b"ok", "é".as_bytes(), b"\x80z"] {
        if let Err(e) = std::fs::write(d.join(OsStr::from_bytes(name)), "") {
            eprintln!("this filesystem refuses {name:?} ({e}): the stock check has nothing to list");
            return None;
        }
    }
    Some(d)
}

#[test]
fn a_non_utf8_entry_name_lists_as_its_lossy_decode_and_stock_wasmtime_refuses_it_by_name() {
    let t = tempfile::tempdir().expect("scratch");
    let src = t.path().join("names.almd");
    std::fs::write(&src, NAMES_FIXTURE).unwrap();
    // Sorted by the DECODED bytes, as native sorts what it decoded: `é`
    // (C3 A9) before `\u{FFFD}z` (EF BF BD 7A), the reverse of the raw order.
    let probe = tempfile::tempdir().expect("probe");
    let want = if non_utf8_dir(probe.path()).is_some() {
        let ls = "[\"f\u{FFFD}g\", \"ok\", \"é\", \"\u{FFFD}z\"]";
        let paths = "[\"<D>/f\u{FFFD}g\", \"<D>/ok\", \"<D>/é\", \"<D>/\u{FFFD}z\"]";
        format!("list_dir ok {ls}\nwalk ok {paths}\nglob ok {paths}\n")
    } else {
        "this host's filesystem refuses a name that is not UTF-8\n".to_string()
    };
    for target in [None, Some("wasm")] {
        let mut c = Command::new(almide());
        c.arg("run").arg(&src);
        if let Some(t) = target {
            c.args(["--target", t]);
        }
        let (code, out, err) = seen(&mut c);
        assert!(!err.contains("panicked"), "{target:?}:\n{err}");
        assert_eq!((code, out.as_str()), (Some(0), want.as_str()), "{target:?}:\n{err}");
    }
    // The stock artifact on wasmtime: its `fd_readdir` refuses the directory
    // (WASI `ilseq`), and each call answers the classified error by name.
    if non_utf8_dir(t.path()).is_none() {
        return;
    }
    let stock = t.path().join("stock.almd");
    std::fs::write(
        &stock,
        "import fs\n\nfn shown(r: Result[List[String], String]) -> String = match r {\n  ok(ns) => \"ok ${list.len(ns)}\",\n  err(m) => \"err ${m}\",\n}\n\neffect fn main() -> Unit = {\n  println(shown(fs.list_dir(\"/names\")))\n  println(shown(fs.walk(\"/names\")))\n  println(shown(fs.glob(\"/names/*\")))\n}\n",
    )
    .unwrap();
    let wasm = t.path().join("stock.wasm");
    let built = seen(Command::new(almide()).arg("build").arg(&stock).args(["--target", "wasm", "-o"]).arg(&wasm));
    assert_eq!(built.0, Some(0), "almide build --target wasm: {}", built.2);
    let preopen = format!("{}::/", t.path().display());
    let (code, out, err) = seen(Command::new("wasmtime").args(["run", "--dir", &preopen]).arg(&wasm));
    let text = almide_base::fs_errno::FS_HOST_ILSEQ_TEXT;
    let want = format!(
        "err fs.list_dir(\"/names\"): {text}\nerr fs.walk(\"/names\"): {text}\nerr fs.glob(\"/names\"): {text}\n"
    );
    assert!(!out.contains("wasi errno"), "a raw errno reached the program:\n{out}");
    assert_eq!((code, out.as_str()), (Some(0), want.as_str()), "stock wasmtime:\n{err}");
}
