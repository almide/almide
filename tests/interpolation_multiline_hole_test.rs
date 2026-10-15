//! #2810 (ruled 2026-10-15: accept): an interpolation hole may span lines in
//! a plain string as it does in a heredoc — including a hole whose
//! expression starts on the line after `${`, which both used to reject with
//! "Expected expression (got Newline)". Python 3.12+ (PEP 701), Ruby and JS
//! template literals accept the shape; Swift's single-line strings do not.
use std::process::Command;

const SRC: &str = r#"effect fn main() -> Unit = {
  let n = 3
  println("a${
    n + 1
  }b")
  println("c${n +
    2}d")
  let h = """
    x${
      n * 2
    }y
    """
  println(h)
  println("e${ n }f")
}
"#;

fn run(target: Option<&str>) -> (bool, String, String) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("hole.almd");
    std::fs::write(&file, SRC).expect("source");
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(&file);
    if let Some(t) = target {
        c.args(["--target", t]);
    }
    let out = c.output().expect("run");
    (out.status.success(), String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn a_hole_spanning_lines_reads_in_plain_strings_and_heredocs_on_both_legs() {
    for target in [None, Some("wasm")] {
        let (ok, stdout, stderr) = run(target);
        assert!(ok, "{target:?}: {stderr}");
        assert_eq!(stdout, "a4b\nc5d\nx6y\ne3f\n", "{target:?}");
    }
}
