//! E025 must not be reported on top of the recovery its own predecessor created (#2096).
//!
//! An undefined function is E002; the call's type recovers as `Ty::Unknown`.
//! `validate_unresolved_binding_types` already skips a wholly-`Unknown` site —
//! "a prior error was already reported" — but `!` on an `Unknown` operand minted
//! a FRESH inference var, and an unbound `?N` is indistinguishable from a
//! genuinely undecidable slot. So the reader got two errors for one defect, and
//! the second one's hint said to annotate a binding that exists only because
//! recovery put it there: a plausible instruction that does not fix the program.
//!
//! The guard has to cut exactly there. E025 is load-bearing on its own — an
//! unconstrained slot is never silently defaulted, and the hint cites Rust E0282
//! deliberately — so the genuine case is asserted in the same file as the
//! suppression. A fix that silenced both would pass a one-sided test.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn check(dir: &std::path::Path, name: &str, source: &str) -> String {
    let file = dir.join(name);
    std::fs::write(&file, source).expect("write fixture");
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

fn count(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

/// The issue's repro: one defect, and it must read as one.
#[test]
fn an_undefined_call_under_a_bang_reports_once() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = check(
        dir.path(),
        "cascade.almd",
        "import random\n\
         effect fn main() -> Unit = {\n\
        \x20 let xs = [1, 2, 3, 4, 5]\n\
        \x20 println(\"${random.sample(xs, 3)!}\")\n\
         }\n",
    );
    assert_eq!(count(&out, "error[E002]"), 1, "the real defect must still be reported:\n{out}");
    assert_eq!(
        count(&out, "error[E025]"),
        0,
        "E025 fired on the Unknown that E002's own recovery created:\n{out}"
    );
    assert!(out.contains("1 error(s) found"), "{out}");
}

/// The same shape without the `!`, so the suppression is not accidentally
/// specific to the unwrap spelling that exposed it.
#[test]
fn an_undefined_call_without_a_bang_also_reports_once() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = check(
        dir.path(),
        "plain.almd",
        "fn main() -> Unit = {\n\
        \x20 let v = list.nope([1, 2, 3])\n\
        \x20 println(\"${v}\")\n\
         }\n",
    );
    assert_eq!(count(&out, "error[E002]"), 1, "{out}");
    assert_eq!(count(&out, "error[E025]"), 0, "{out}");
}

/// The other direction. An unconstrained slot with NO preceding error is still
/// undecidable and still refused, hint intact — this is the behaviour the
/// suppression must not reach.
#[test]
fn a_genuine_unconstrained_slot_still_reports_e025() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = check(
        dir.path(),
        "genuine.almd",
        "fn main() -> Unit = {\n\
        \x20 let xs = []\n\
        \x20 println(\"${list.len(xs)}\")\n\
         }\n",
    );
    assert_eq!(count(&out, "error[E002]"), 0, "no recovery is involved here:\n{out}");
    assert_eq!(count(&out, "error[E025]"), 1, "E025 must survive for the real case:\n{out}");
    assert!(
        out.contains("Rust E0282"),
        "the hint's deliberate citation must survive:\n{out}"
    );
}
