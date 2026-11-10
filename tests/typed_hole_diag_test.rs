//! #1325 — the typed-hole ruling, both halves.
//!
//! `_` in EXPRESSION position (`ExprKind::Hole`) and `todo("msg")` are a
//! FEATURE: they type-check against whatever the context demands and panic if
//! execution reaches them. `_` in a call ARGUMENT (`ExprKind::Placeholder`) is
//! a different node that nothing can lower — it became `Unit`, so
//! `add(_, 10)` emitted `add((), 10i64)` and died at BUILD behind
//! "codegen produced invalid Rust — this is an Almide bug", blaming the
//! compiler for a user error (the failure mode E045 closed for tuple
//! indexing). That half is E046 at check time now.
//!
//! The diagnostic must not imply partial application: MEASURED on the
//! pre-fix compiler, `let v = add(_, 10)` typed `v` as add's RETURN type, and
//! in pipe position the `_` counts as an extra positional argument (E004).

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn write(dir: &std::path::Path, name: &str, source: &str) -> std::path::PathBuf {
    let file = dir.join(name);
    std::fs::write(&file, source).expect("write fixture");
    file
}

fn check(dir: &std::path::Path, source: &str) -> String {
    let file = write(dir, "hole.almd", source);
    let out = Command::new(almide())
        .arg("check")
        .arg(&file)
        .output()
        .expect("run almide check");
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// `(stdout + stderr, exit code)` of `almide run`.
fn run(dir: &std::path::Path, name: &str, source: &str) -> (String, Option<i32>) {
    let file = write(dir, name, source);
    let out = Command::new(almide())
        .arg("run")
        .arg(&file)
        .output()
        .expect("run almide run");
    (
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
        out.status.code(),
    )
}

const ADD: &str = "fn add(a: Int, b: Int) -> Int = a + b\n";
const ADD3: &str = "fn add3(a: Int, b: Int, c: Int) -> Int = a + b + c\n";

// ── The call-argument placeholder is a check-time error ──────────────

#[test]
fn call_arg_placeholder_is_e046_naming_the_position() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = check(
        dir.path(),
        &format!("{ADD}\neffect fn main() -> Unit = {{\n  let v = add(_, 10)\n  println(\"${{v}}\")\n}}\n"),
    );
    assert!(out.contains("E046"), "`add(_, 10)` must be E046, got:\n{out}");
    assert!(
        out.contains("argument 1 of add()"),
        "the diagnostic must NAME the argument position and the callee, got:\n{out}"
    );
}

#[test]
fn call_arg_placeholder_steers_to_a_lambda_without_claiming_partial_application() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = check(
        dir.path(),
        &format!("{ADD}\neffect fn main() -> Unit = {{\n  let v = add(_, 10)\n  println(\"${{v}}\")\n}}\n"),
    );
    assert!(
        out.contains("does NOT partially apply the call"),
        "the hint must DENY partial application — binding `add(_, 10)` typed the \
         RETURN type, not a function, got:\n{out}"
    );
    assert!(
        out.contains("(x) => add(x, 10)"),
        "the try snippet must be the lambda for the user's OWN call, got:\n{out}"
    );
}

#[test]
fn pipe_position_placeholder_is_e046_too() {
    // The shape that used to reach codegen: the piped subject absorbs the
    // arity slack, so E004 never fires and `almide check` said "No errors
    // found" before dying at build.
    let dir = tempfile::tempdir().expect("tempdir");
    let out = check(
        dir.path(),
        &format!("{ADD3}\neffect fn main() -> Unit = {{\n  let v = 5 |> add3(_, 10)\n  println(\"${{v}}\")\n}}\n"),
    );
    assert!(
        out.contains("E046"),
        "`5 |> add3(_, 10)` must be E046 — the pipe RHS is inferred on its own \
         path and used to slip through, got:\n{out}"
    );
    assert!(
        out.contains("argument 1 of add3()"),
        "the pipe-path diagnostic must name the position too, got:\n{out}"
    );
}

#[test]
fn placeholder_free_program_still_checks_clean() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = check(
        dir.path(),
        &format!(
            "{ADD}\neffect fn main() -> Unit = {{\n  let add10 = (x) => add(x, 10)\n  \
             println(\"${{add10(5)}}\")\n}}\n"
        ),
    );
    assert!(
        out.contains("No errors found"),
        "the lambda the hint steers to must itself check clean, got:\n{out}"
    );
}

// ── Typed holes are a FEATURE: they check, then panic ─────────────────

#[test]
fn typed_hole_and_todo_check_clean() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = check(
        dir.path(),
        "fn a(x: Int) -> Int = _\nfn b(x: Int) -> String = todo(\"later\")\n\
         effect fn main() -> Unit = println(\"${a(1)}${b(2)}\")\n",
    );
    assert!(
        out.contains("No errors found"),
        "`_` in expression position and `todo(..)` are sanctioned typed holes \
         and must type-check against their context, got:\n{out}"
    );
}

#[test]
fn typed_hole_panics_naming_the_almide_source_line() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (out, code) = run(
        dir.path(),
        "hole_run.almd",
        // `_` is on line 1; the panic must say so, because the generated .rs
        // line rustc prints is meaningless to the author.
        "fn compute(x: Int) -> Int = _\n\neffect fn main() -> Unit = {\n  \
         println(\"before\")\n  println(\"${compute(3)}\")\n}\n",
    );
    assert!(out.contains("before"), "output before the hole must survive:\n{out}");
    assert!(
        out.contains("not yet implemented: hole at line 1"),
        "the hole's panic must name the ALMIDE source line, got:\n{out}"
    );
    assert_eq!(code, Some(101), "a hole panics (rust panic exit code):\n{out}");
}

#[test]
fn todo_keeps_its_own_message_and_survives_quotes_in_it() {
    // The escaping half: `todo("say \"hi\"")` used to emit
    // `todo!("say "hi"")` and die behind the invalid-Rust banner.
    let dir = tempfile::tempdir().expect("tempdir");
    let (out, code) = run(
        dir.path(),
        "todo_run.almd",
        "fn compute(x: Int) -> Int = todo(\"say \\\"hi\\\" and \\\\ back\")\n\n\
         effect fn main() -> Unit = {\n  println(\"before\")\n  \
         println(\"${compute(3)}\")\n}\n",
    );
    assert!(
        !out.contains("this is an Almide bug"),
        "a quote in a todo() message must not produce invalid Rust, got:\n{out}"
    );
    assert!(
        out.contains("not yet implemented: say \"hi\" and \\ back"),
        "todo() must panic with the author's own message, verbatim, got:\n{out}"
    );
    assert_eq!(code, Some(101), "todo() panics (rust panic exit code):\n{out}");
}

/// `(stdout + stderr, exit code)` of a native `almide test` of one file.
fn native_test(source: &str) -> (String, Option<i32>) {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "almide.toml", "[package]\nname = \"z\"\nversion = \"0.1.0\"\n");
    write(dir.path(), "hole_test.almd", source);
    let out = Command::new(almide())
        .args(["test", "hole_test.almd", "--target", "rust"])
        .current_dir(dir.path())
        .output()
        .expect("run almide test");
    (
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
        out.status.code(),
    )
}

/// The three typed holes a test file can hold: `todo(..)`, a written `_`, and
/// the bodyless declaration of a host the native target does not have.
const HOLES: &str = "@extern(wasm, \"obsid\", \"set_state\")\nfn set_state(s: String) -> Unit\n\n\
fn later(n: Int) -> Int = todo(\"later\")\n\nfn blank(n: Int) -> String = _\n\n";

#[test]
fn an_unreached_typed_hole_does_not_stop_a_native_test_file() {
    // #4640: the native test render is the only one, so a hole it refused was
    // a compile failure for every test in the file — reached or not.
    let (out, code) = native_test(&format!(
        "{HOLES}fn pick(n: Int) -> Int = if n > 5 then later(n) else n\n\n\
         fn show(n: Int) -> String = if n > 5 then blank(n) else \"small\"\n\n\
         fn tell(n: Int) -> Unit = if n > 5 then set_state(\"x\") else ()\n\n\
         test \"no hole is reached\" {{\n  tell(1)\n  assert_eq(pick(1), 1)\n  assert_eq(show(1), \"small\")\n}}\n"
    ));
    assert_eq!(code, Some(0), "the file builds and its test passes:\n{out}");
    assert!(out.contains("1 test in 1 file"), "{out}");
}

#[test]
fn a_reached_typed_hole_fails_its_native_test_with_its_own_text() {
    // The control: the hole is an abort, not a value. Each test that reaches
    // one fails with the hole's text; the test beside them still passes.
    let (out, code) = native_test(&format!(
        "{HOLES}test \"reaches the todo\" {{\n  assert_eq(later(1), 0)\n}}\n\n\
         test \"reaches the host\" {{\n  set_state(\"x\")\n  assert_eq(1, 1)\n}}\n\n\
         test \"reaches the blank\" {{\n  assert_eq(blank(1), \"\")\n}}\n\n\
         test \"reaches none\" {{\n  assert_eq(1, 1)\n}}\n"
    ));
    assert_eq!(code, Some(1), "a reached hole is a failed test:\n{out}");
    assert!(!out.contains("Compile error"), "the file itself builds:\n{out}");
    for (test, text) in [
        ("reaches the todo", "not yet implemented: later"),
        ("reaches the host", "not yet implemented: hole at line 2"),
        ("reaches the blank", "not yet implemented: hole at line 6"),
    ] {
        assert!(out.contains(&format!("test: {test}")), "`{test}` is reported as failed:\n{out}");
        assert!(out.contains(&format!("error: {text}")), "`{test}` fails with the hole's text:\n{out}");
    }
    assert!(!out.contains("test: reaches none"), "the test that reaches no hole passes:\n{out}");
    assert!(out.contains("4 tests in 1 file"), "{out}");
}
