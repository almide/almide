//! #2885 gate: an `effect fn main` may declare any Ok type, and the entry
//! wrapper throws the payload away (check's E044 rule). The retired incumbent
//! wasm leg emitted invalid wasm for `-> Int` and turned a record payload into
//! an `Error: ` line with exit 1. Each Ok shape below must give the same
//! stdout, stderr and exit code on the wasm leg and native, on the value path
//! and on the `!` error path.

use std::process::Command;

/// `ALMIDE_BIN` runs the cells against another build (A/B); the default is
/// this workspace's binary.
fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

/// `(stdout, stderr without the leg banner, exit code)`.
fn run(dir: &std::path::Path, args: &[&str]) -> (String, String, Option<i32>) {
    let mut cmd = Command::new(almide());
    cmd.current_dir(dir).args(args);
    let out = cmd.output().expect("run almide");
    let stderr: String = String::from_utf8_lossy(&out.stderr)
        .lines()
        .filter(|l| !l.starts_with("[almide]"))
        .map(|l| format!("{l}\n"))
        .collect();
    (String::from_utf8_lossy(&out.stdout).into_owned(), stderr, out.status.code())
}

#[test]
fn an_effect_main_declaring_any_ok_type_runs_alike_on_every_leg() {
    // (declared Ok type, a value of it, a declaration it needs)
    let shapes = [
        ("Int", "7", ""),
        ("Float", "1.5", ""),
        ("Bool", "true", ""),
        ("String", "\"payload\"", ""),
        ("List[Int]", "[1, 2]", ""),
        ("List[String]", "[\"a\", \"b\"]", ""),
        ("(Int, String)", "(1, \"a\")", ""),
        ("Point", "Point { x: 1, name: \"p\" }", "type Point = { x: Int, name: String }\n\n"),
    ];
    let mut failures = Vec::new();
    for (ty, value, decl) in shapes {
        for (path, fails) in [("value", false), ("error", true)] {
            let g = if fails { "err(\"boom\")" } else { "ok(3)" };
            let program = format!(
                "{decl}effect fn g() -> Int = {g}\n\neffect fn main() -> {ty} = {{\n  let n = g()!\n  println(\"n = ${{n}}\")\n  {value}\n}}\n"
            );
            let root = tempfile::tempdir().expect("tempdir");
            std::fs::write(root.path().join("t.almd"), &program).expect("write");
            let native = run(root.path(), &["run", "t.almd"]);
            let want = if fails { (String::new(), "Error: boom\n".to_string(), Some(1)) } else { ("n = 3\n".to_string(), String::new(), Some(0)) };
            if native != want {
                failures.push(format!("`-> {ty}` ({path} path) native: {native:?}, expected {want:?}"));
            }
            let got = run(root.path(), &["run", "t.almd", "--target", "wasm"]);
            if got != native {
                failures.push(format!("`-> {ty}` ({path} path) wasm: {got:?}, native {native:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{} cell(s) failed:\n{}", failures.len(), failures.join("\n"));
}
