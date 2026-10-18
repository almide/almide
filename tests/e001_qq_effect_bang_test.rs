//! `effect_call(..) ?? fallback` on an effect fn declared `-> Option[T]` is
//! one defect with one repair (#3522).
//!
//! The call is `Result[Option[T], String]`, so `??` defaults the `Err` and
//! wants an `Option[T]` fallback. A `T` fallback drew the generic E001 on the
//! fallback plus a second E001 wherever the binding was used (it stayed
//! `Option[T]`), and neither named the missing `!`. Now the fallback's E001
//! names `call(..)! ?? fallback`, and the `??` is typed `T`, so the later use
//! raises nothing. The near misses — a pure Option call, a fallback that is
//! not the call's `T`, an unrelated mismatch in the same fn — are asserted
//! here too, so a fix that silenced too much would fail this file.
//!
//! `ALMIDE_BIN` points the file at another build (the A/B against develop).

use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn check(dir: &std::path::Path, source: &str) -> String {
    let file = dir.join("case.almd");
    std::fs::write(&file, source).expect("write fixture");
    let out = Command::new(almide())
        .arg("check")
        .arg(&file)
        .output()
        .expect("run almide check");
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

fn count(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

const QQ_MISMATCH: &str = "type mismatch in ?? fallback: expected Option[String] but got String";

/// The issue's probe: one E001, on the fallback, naming the exact repair; the
/// `println(ledger)` cascade is gone.
#[test]
fn an_argv_reader_with_a_payload_fallback_reports_one_error_naming_the_bang() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = check(
        dir.path(),
        "import args\n\
         effect fn main() -> Unit = {\n\
         \x20 let ledger = args.positional_at(0) ?? \"ledger.toml\"\n\
         \x20 println(ledger)\n\
         }\n",
    );
    assert_eq!(count(&out, "error[E001]"), 1, "{out}");
    assert_eq!(count(&out, QQ_MISMATCH), 1, "{out}");
    assert!(!out.contains("call to println()"), "the cascade on the binding's use is dropped:\n{out}");
    assert!(
        out.contains("`args.positional_at(0)! ?? \"ledger.toml\"`"),
        "the hint spells the repair:\n{out}"
    );
}

/// A user effect fn declared `-> Option[String]`, and the unrelated E001 in
/// the same fn that must still be reported.
#[test]
fn a_user_effect_fn_drops_the_cascade_but_not_an_unrelated_mismatch() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = check(
        dir.path(),
        "effect fn load(p: String) -> Option[String] = some(p)\n\
         effect fn main() -> Unit = {\n\
         \x20 let n: Int = \"oops\"\n\
         \x20 let v = load(\"x\") ?? \"none\"\n\
         \x20 println(v)\n\
         \x20 println(int.to_string(n))\n\
         }\n",
    );
    assert_eq!(count(&out, QQ_MISMATCH), 1, "{out}");
    assert!(out.contains("`load` is an effect fn"), "{out}");
    assert!(out.contains("`load(\"x\")! ?? \"none\"`"), "{out}");
    assert_eq!(count(&out, "type mismatch in let n: expected Int but got String"), 1, "{out}");
    assert!(!out.contains("call to println()"), "{out}");
    assert_eq!(count(&out, "error[E001]"), 2, "{out}");
}

/// Near misses keep today's diagnostic and its generic hint: a pure fn
/// returning `Option[String]` (its `??` unwraps the Option, no `!`), and an
/// effect fn whose fallback is neither `Option[String]` nor `String`.
#[test]
fn a_pure_option_call_and_a_non_payload_fallback_keep_the_plain_mismatch() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = check(
        dir.path(),
        "fn first(s: String) -> Option[String] = some(s)\n\
         effect fn load(p: String) -> Option[String] = some(p)\n\
         effect fn main() -> Unit = {\n\
         \x20 let a = first(\"x\") ?? \"d\"\n\
         \x20 let b = first(\"x\") ?? 0\n\
         \x20 let c = load(\"x\") ?? 5\n\
         \x20 let d = load(\"x\")! ?? \"e\"\n\
         \x20 let e = load(\"x\") ?? some(\"f\")\n\
         \x20 println(a + d)\n\
         \x20 println(c ?? \"\")\n\
         \x20 println(e ?? \"\")\n\
         }\n",
    );
    assert_eq!(count(&out, "type mismatch in ?? fallback: expected String but got Int"), 1, "{out}");
    assert_eq!(count(&out, "type mismatch in ?? fallback: expected Option[String] but got Int"), 1, "{out}");
    assert_eq!(count(&out, "error[E001]"), 2, "{out}");
    assert!(!out.contains("is an effect fn"), "no `!` hint off the #3522 shape:\n{out}");
}
