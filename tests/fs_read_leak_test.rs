//! Every `fs.read_*` variant (and `fs.stat`, the other read that stages a
//! host buffer) leaves no heap block live on wasm, on its ok and its err
//! path, and prints what native prints (#3677).
//!
//! `fs.read_bytes` and `fs.read_bytes_if_exists` pulled the file into a
//! staging block, decoded it into the returned value, and never
//! released the staging block: one leaked block per successful read.
//! `fs.stat` did the same with its 32-byte stat buffer. The same defect as
//! #3629 (`io.read_n_bytes`) and #3631 (`io.read_byte`).
use std::process::Command;

fn run(dir: &std::path::Path, name: &str, wasm: bool) -> (String, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_almide"));
    cmd.arg("run").arg(dir.join(format!("{name}.almd"))).current_dir(dir);
    if wasm {
        cmd.args(["--target", "wasm"]).env("ALMIDE_WASM_ALLOC_COUNT", "1");
    }
    let out = cmd.output().expect("almide run");
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(out.status.success(), "{name} (wasm={wasm}): {err}");
    (String::from_utf8_lossy(&out.stdout).into_owned(), err)
}

/// `call` answers a Result; `ok` renders its payload `v` as a String.
fn probe(call: &str, ok: &str) -> String {
    format!(
        "import fs\neffect fn main() -> Unit = {{\n  match {call} {{\n    ok(v) => println(\"ok \" + {ok}),\n    err(e) => println(\"err \" + e),\n  }}\n}}\n"
    )
}

#[test]
fn fs_reads_leave_no_block_live_on_wasm() {
    let dir = tempfile::tempdir().expect("scratch");
    std::fs::create_dir(dir.path().join("sub")).expect("sub");
    std::fs::write(dir.path().join("sub/f.txt"), "hello\nworld\n").expect("file");
    let len = "int.to_string(list.len(v))";
    let opt_len = "match v { some(xs) => int.to_string(list.len(xs)), none => \"none\" }";
    let blen = "int.to_string(bytes.len(v))";
    let opt_blen = "match v { some(b) => int.to_string(bytes.len(b)), none => \"none\" }";
    let opt_text = "v ?? \"none\"";
    // (name, call, ok-render): ok paths read sub/f.txt; err paths read a
    // missing file, or for the if-exists twins (whose missing file is
    // ok(none)) a directory.
    let cases: Vec<(&str, String, &str)> = vec![
        ("read_text_ok", "fs.read_text(\"sub/f.txt\")".into(), "v"),
        ("read_text_err", "fs.read_text(\"nope\")".into(), "v"),
        ("read_text_if_exists_ok", "fs.read_text_if_exists(\"sub/f.txt\")".into(), opt_text),
        ("read_text_if_exists_none", "fs.read_text_if_exists(\"nope\")".into(), opt_text),
        ("read_text_if_exists_err", "fs.read_text_if_exists(\"sub\")".into(), opt_text),
        ("read_lines_ok", "fs.read_lines(\"sub/f.txt\")".into(), len),
        ("read_lines_err", "fs.read_lines(\"nope\")".into(), len),
        ("read_lines_if_exists_ok", "fs.read_lines_if_exists(\"sub/f.txt\")".into(), opt_len),
        ("read_lines_if_exists_none", "fs.read_lines_if_exists(\"nope\")".into(), opt_len),
        ("read_lines_if_exists_err", "fs.read_lines_if_exists(\"sub\")".into(), opt_len),
        ("read_bytes_ok", "fs.read_bytes(\"sub/f.txt\")".into(), blen),
        ("read_bytes_err", "fs.read_bytes(\"nope\")".into(), blen),
        ("read_bytes_if_exists_ok", "fs.read_bytes_if_exists(\"sub/f.txt\")".into(), opt_blen),
        ("read_bytes_if_exists_none", "fs.read_bytes_if_exists(\"nope\")".into(), opt_blen),
        ("read_bytes_if_exists_err", "fs.read_bytes_if_exists(\"sub\")".into(), opt_blen),
        ("stat_ok", "fs.stat(\"sub/f.txt\")".into(), "int.to_string(v.size)"),
        ("stat_err", "fs.stat(\"nope\")".into(), "int.to_string(v.size)"),
    ];
    let mut failures = Vec::new();
    for (name, call, ok) in &cases {
        std::fs::write(dir.path().join(format!("{name}.almd")), probe(call, ok)).expect("source");
        let (native, _) = run(dir.path(), name, false);
        let (wasm, err) = run(dir.path(), name, true);
        if native != wasm {
            failures.push(format!("{name}: native {native:?} != wasm {wasm:?}"));
        }
        let line = err.lines().find(|l| l.contains("__ALMD_WASM_ALLOC")).unwrap_or("no alloc count line");
        if !line.contains(" live=0 ") {
            failures.push(format!("{name}: {line}"));
        }
    }
    assert!(failures.is_empty(), "fs reads on wasm:\n{}", failures.join("\n"));
}
