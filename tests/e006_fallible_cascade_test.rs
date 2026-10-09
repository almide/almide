//! An E006 on a FALLIBLE effect callee is one defect with one repair (#3515).
//!
//! `fn body() -> String = fs.read_text("x")` drew E006 ("mark the caller
//! `effect fn`") plus an E001 on the same expression (`expected String but got
//! Result[String, String]`, "fix the expression type"). Following the E006
//! hint alone led to E041 — the missing `!` — a second repair round, and the
//! E001 pointed away from both steps.
//!
//! The E006 hint now names both steps, and the E001 that is only that call's
//! `Result` met where its `ok` type was wanted is not reported separately. The
//! suppression is cut to that shape: an unrelated E001 in the same fn, a
//! mismatch the `!` would not fix, and the near misses whose `Result` is
//! already consumed (`!`, `match`, `??`) are asserted here alongside it, so a
//! fix that silenced too much would fail this file.

use std::process::Command;

fn check(dir: &std::path::Path, source: &str) -> String {
    let file = dir.join("case.almd");
    std::fs::write(&file, source).expect("write fixture");
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("check")
        .arg(&file)
        .output()
        .expect("run almide check");
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

fn count(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

/// The `hint:` line of the first E006 in `out`.
fn e006_hint(out: &str) -> String {
    out.split("error[E006]").nth(1)
        .and_then(|rest| rest.lines().find(|l| l.trim_start().starts_with("hint:")))
        .unwrap_or_default()
        .to_string()
}

const BANG_STEP: &str = "propagate the call's error with `!`";

/// The issue's probe: one defect, one error, and a hint naming both steps.
#[test]
fn a_fallible_call_in_a_pure_fn_reports_one_error_naming_both_steps() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = check(
        dir.path(),
        "import fs\n\
         fn body() -> String = fs.read_text(\"x\")\n\
         effect fn main() -> Unit = println(body())\n",
    );
    assert_eq!(count(&out, "error[E006]"), 1, "{out}");
    assert_eq!(count(&out, "error[E001]"), 0, "the cascade E001 is the E006's residue:\n{out}");
    let hint = e006_hint(&out);
    assert!(hint.contains("effect fn") && hint.contains("`fs.read_text(..)!`"), "{hint}");
}

/// The annotated-`let` spelling of the same cascade, and the argv reader whose
/// hint already named `!` (`args.flag` is `Result[Bool, String]`).
#[test]
fn the_let_and_argv_reader_cascades_are_dropped_too() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = check(
        dir.path(),
        "import fs\n\
         import args\n\
         fn body() -> String = {\n\
         \x20 let s: String = fs.read_text(\"x\")\n\
         \x20 s\n\
         }\n\
         fn verbose() -> Bool = args.flag(\"v\")\n\
         effect fn main() -> Unit = println(\"x\")\n",
    );
    assert_eq!(count(&out, "error[E006]"), 2, "{out}");
    assert_eq!(count(&out, "error[E001]"), 0, "{out}");
    assert!(out.contains("`args.flag(..)!`"), "the argv hint keeps naming `!`:\n{out}");
}

/// The other direction: an unrelated E001 in the same fn is still reported,
/// and so is a mismatch the `!` does not fix (`Int` wanted, `String` inside).
#[test]
fn an_unrelated_or_unfixed_mismatch_is_still_reported() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = check(
        dir.path(),
        "import fs\n\
         fn body() -> String = {\n\
         \x20 let n: Int = \"oops\"\n\
         \x20 fs.read_text(\"x\")\n\
         }\n\
         fn size() -> Int = fs.read_text(\"x\")\n\
         effect fn main() -> Unit = println(\"x\")\n",
    );
    assert_eq!(count(&out, "error[E006]"), 2, "{out}");
    assert_eq!(count(&out, "type mismatch in let n: expected Int but got String"), 1, "{out}");
    assert_eq!(count(&out, "type mismatch in fn 'size': expected Int but got Result[String, String]"), 1, "{out}");
    assert_eq!(count(&out, "error[E001]"), 2, "{out}");
}

/// Near misses: a call whose `Result` something already consumes keeps the
/// plain hint — `!` there is either present already or wrong.
#[test]
fn a_consumed_result_keeps_the_plain_hint() {
    for body in [
        "fs.read_text(\"x\")!",
        "(fs.read_text(\"x\"))!",
        "fs.read_text(\"x\") ?? \"\"",
        "match fs.read_text(\"x\") { ok(s) => s, err(_) => \"\" }",
    ] {
        let dir = tempfile::tempdir().expect("tempdir");
        let out = check(
            dir.path(),
            &format!("import fs\nfn body() -> String = {body}\neffect fn main() -> Unit = println(\"x\")\n"),
        );
        let hint = e006_hint(&out);
        assert!(hint.contains("Mark the calling function as `effect fn`"), "{body}: {out}");
        assert!(!hint.contains(BANG_STEP), "{body}: the hint asks for a `!` the call does not need:\n{out}");
    }
}

/// An infallible effect callee has no `!` step, and a lambda keeps its own
/// hint (its `!` would also need one on the higher-order call — not a single
/// edit, so the lambda branch does not claim one).
#[test]
fn infallible_and_lambda_callees_keep_their_hints() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = check(
        dir.path(),
        "import io\n\
         import fs\n\
         fn line() -> String = io.read_line()\n\
         fn all() -> List[String] = list.map([\"a\"], (p) => fs.read_text(p))\n\
         effect fn main() -> Unit = println(\"x\")\n",
    );
    assert_eq!(count(&out, "error[E006]"), 2, "{out}");
    assert_eq!(count(&out, BANG_STEP), 0, "{out}");
    assert!(out.contains("A lambda inherits its context's effect capability"), "{out}");
}

/// A top-level `let` has no fn to mark and no body for a `!` to propagate
/// from, so its E006 does not claim the two-step repair.
#[test]
fn a_top_level_let_keeps_the_plain_hint() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = check(
        dir.path(),
        "effect fn tick() -> Int = 1\n\
         let seed = tick()\n\
         effect fn main() -> Unit = println(int.to_string(seed))\n",
    );
    assert_eq!(count(&out, "error[E006]"), 1, "{out}");
    assert_eq!(count(&out, BANG_STEP), 0, "{out}");
}
