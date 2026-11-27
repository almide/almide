//! #4954: a `match` over a tuple of strings with literal rows (the dojo task
//! `reject-refunds-beyond-the-remaining-payment`'s `match (state, ev)`) hung
//! the native test lane. The native pattern expansion carried each row's
//! outcome as a let-bound `Option` of bindings with one leaf per literal
//! test, and the lowering's tail duplication copied the next row into every
//! leaf: three copies per two-column row, 111,504 native ops for seven rows,
//! minutes in the move analysis. A row whose binders are projections now
//! tests a Bool and holds its body and the next row once each.
//!
//! Twelve rows (with binder and guarded rows among them) would be 3^12 times
//! the one-row size on the old expansion; the run must finish well inside the
//! hang guard and give v0.69.1's answers.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const GUARD: Duration = Duration::from_secs(150);

fn program(rows: usize) -> String {
    let mut s = String::from("fn next_state(state: String, ev: String) -> String? =\n  match (state, ev) {\n");
    for i in 1..=rows {
        s.push_str(&format!("    (\"s{i}\", \"e{i}\") => some(\"t{i}\"),\n"));
    }
    s.push_str("    (s, \"echo\") => some(s + \"!\"),\n");
    s.push_str("    (s, e) if s == e => some(\"same\"),\n");
    s.push_str("    _ => none,\n  }\n\n");
    s.push_str(&format!(
        "test \"rows\" {{\n  assert_eq(next_state(\"s1\", \"e1\"), some(\"t1\"))\n  assert_eq(next_state(\"s{rows}\", \"e{rows}\"), some(\"t{rows}\"))\n  assert_eq(next_state(\"s1\", \"e2\"), none)\n  assert_eq(next_state(\"q\", \"echo\"), some(\"q!\"))\n  assert_eq(next_state(\"z\", \"z\"), some(\"same\"))\n  assert_eq(next_state(\"z\", \"y\"), none)\n}}\n"
    ));
    s
}

#[test]
fn a_string_tuple_literal_match_renders_natively_in_linear_size() {
    let dir = std::env::temp_dir().join(format!("almide-tuple-literal-match-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("rows_test.almd");
    std::fs::write(&src, program(12)).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["test", "--target", "rust"])
        .arg(&src)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn almide");
    let mut out = child.stdout.take().unwrap();
    let mut err = child.stderr.take().unwrap();
    let out = std::thread::spawn(move || {
        let mut t = String::new();
        let _ = out.read_to_string(&mut t);
        t
    });
    let err = std::thread::spawn(move || {
        let mut t = String::new();
        let _ = err.read_to_string(&mut t);
        t
    });
    let start = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().expect("wait") {
            break s;
        }
        if start.elapsed() > GUARD {
            let _ = child.kill();
            let _ = child.wait();
            panic!("`almide test --target rust` on a 12-row tuple literal match ran past the {GUARD:?} hang guard");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let (out, err) = (out.join().unwrap(), err.join().unwrap());
    assert!(status.success(), "native test lane failed:\n{out}\n{err}");
    assert!(!err.contains("native v1 test render"), "the native test render walled:\n{err}");
    let _ = std::fs::remove_dir_all(&dir);
}
