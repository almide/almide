//! #4195: `regex.compile` of a pattern nested far past the 250-group limit
//! answers `err` on both targets. Native used to run the recursive parser
//! on the pattern it had just refused, and 50 000 nested groups overflowed
//! the stack (`Error: stack overflow`, exit 1) where wasm returned the err.
//!
//! Control: a depth inside the limit compiles and matches on both targets.

use std::process::Command;

fn run(src: &str, target: &str) -> (Option<i32>, String, String) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("p.almd"), src).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["run", "p.almd", "--target", target])
        .current_dir(dir.path())
        .output()
        .expect("spawn almide run");
    (out.status.code(), String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

fn program(depth: usize) -> String {
    format!(
        r#"import regex
fn main() -> Unit = {{
  let p = string.repeat("(", {depth}) + "a" + string.repeat(")", {depth})
  match regex.compile(p) {{
    ok(re) => println(if regex.is_match(re, "aaaa") then "match" else "nomatch"),
    err(e) => println("err: ${{e}}"),
  }}
}}
"#
    )
}

#[test]
fn a_pattern_nested_far_past_the_limit_is_an_err_on_both_targets() {
    for target in ["rust", "wasm"] {
        let (code, out, err) = run(&program(50_000), target);
        assert_eq!(code, Some(0), "{target}: {out}\n{err}");
        assert_eq!(out.trim(), "err: groups nested deeper than 250", "{target}: {err}");
    }
}

#[test]
fn a_pattern_inside_the_limit_compiles_and_matches() {
    for target in ["rust", "wasm"] {
        let (code, out, err) = run(&program(200), target);
        assert_eq!(code, Some(0), "{target}: {out}\n{err}");
        assert_eq!(out.trim(), "match", "{target}: {err}");
    }
}
