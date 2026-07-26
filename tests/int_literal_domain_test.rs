//! E024 must distinguish WHY a literal is out of range, because the three
//! deviations take three different fixes and the wrong hint sends the reader
//! somewhere there is nothing.
//!
//! - **magnitude** — too large for the width. A smaller literal, or a wider
//!   type, fixes it.
//! - **sign** — negated in an unsigned context. No magnitude fixes it; an
//!   unsigned type has no negative values at all. `-0` is exempt: it is `0`.
//! - **carrier** — inside `UInt64`'s *declared* domain but above the `i64` the
//!   IR carries every integer in. Neither a smaller literal nor a wider type
//!   fixes it, because there is no wider type (#872).
//!
//! The sign case was a real acceptance gap: `let k: UInt64 = -5` passed
//! `almide check` — 5 is in range for UInt64 — and native rustc then rejected
//! the emitted `-5u64` with `error[E0600]: cannot apply unary operator '-' to
//! type 'u64'` (differential fuzz). The carrier case was worse: it was accepted
//! and *ran*, printing the same wrong answer on both targets, which is exactly
//! why the cross-target differential fuzzer never surfaced it.
//!
//! This pins the classification at the diagnostic's surface. The three hints
//! are distinguishable strings on purpose — merging them would pass a test that
//! only asserted "E024 fires".

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn check(dir: &std::path::Path, source: &str) -> String {
    let file = dir.join("lit.almd");
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

fn body(binding: &str) -> String {
    format!("fn main() -> Unit = {{\n  {binding}\n  println(\"done\")\n}}\n")
}

/// (binding, the hint fragment that identifies its deviation class)
const REJECTED: &[(&str, &str)] = &[
    // ── magnitude ────────────────────────────────────────────────────────
    ("let a: Int = 9223372036854775808", "would silently fold to 0"),
    ("let a: Int8 = 128", "would silently fold to 0"),
    ("let a: UInt8 = 256", "would silently fold to 0"),
    ("let a: UInt32 = 4294967296", "would silently fold to 0"),
    // Past u64::MAX is a magnitude failure even for UInt64 — no type holds it.
    ("let a: UInt64 = 99999999999999999999999", "would silently fold to 0"),
    // Past the u128 the comparison itself is done in. The classifier used to
    // answer "fits" when its own intermediate overflowed, and `int_value` then
    // folded the literal to 0 — a silent wrong value produced by the escape
    // hatch of the check that exists to stop silent wrong values.
    (
        "let a: Int = 99999999999999999999999999999999999999999999",
        "would silently fold to 0",
    ),
    // Same, with the sign independently wrong: the sign is reported, because no
    // magnitude would have made it fit either.
    (
        "let a: UInt64 = -99999999999999999999999999999999999999999999",
        "has no negative values at all",
    ),
    // ── sign ─────────────────────────────────────────────────────────────
    ("let a: UInt64 = -5", "has no negative values at all"),
    ("let a: UInt8 = -1", "has no negative values at all"),
    ("let a: UInt16 = -32768", "has no negative values at all"),
    ("let a: UInt32 = -1", "has no negative values at all"),
    // A negated literal that would be in range as a MAGNITUDE is still a sign
    // error — the distinction the unsigned branch used to miss entirely.
    ("let a: UInt8 = -200", "has no negative values at all"),
    // ── carrier ──────────────────────────────────────────────────────────
    ("let a: UInt64 = 9223372036854775808", "above the i64 the compiler carries"),
    ("let a: UInt64 = 18446744073709551615", "above the i64 the compiler carries"),
];

/// Bindings that must keep compiling. Each one sits on a boundary that a
/// careless bound would take out with the real errors.
const ACCEPTED: &[&str] = &[
    // Every unsigned width's maximum that the carrier reaches.
    "let a: UInt8 = 255",
    "let a: UInt16 = 65535",
    "let a: UInt32 = 4294967295",
    "let a: UInt64 = 9223372036854775807",
    // `-0` is `0`, which every unsigned type represents. The sign rule must not
    // eat it.
    "let a: UInt8 = -0",
    "let a: UInt32 = -0",
    "let a: UInt64 = -0",
    // Every signed width's minimum — reachable only as a negated literal, so
    // the sign rule must stay scoped to UNSIGNED contexts.
    "let a: Int8 = -128",
    "let a: Int16 = -32768",
    "let a: Int32 = -2147483648",
    "let a: Int64 = -9223372036854775808",
    "let a: Int = -9223372036854775808",
    // Signed maxima.
    "let a: Int8 = 127",
    "let a: Int = 9223372036854775807",
    // Hex goes through the same classifier, so a hex literal at the same
    // boundary must land the same way. (`0b`/`0o` are handled by
    // `radix_and_digits` but not lexed — see #873.)
    "let a: UInt64 = 0x7FFFFFFFFFFFFFFF",
    "let a: UInt8 = 0xFF",
    // Underscore separators are stripped before the radix split.
    "let a: UInt64 = 9_223_372_036_854_775_807",
    "let a: UInt64 = 0x7FFF_FFFF_FFFF_FFFF",
    // A separator-only tail must not read as an empty digit run.
    "let a: Int = 1_000",
];

/// The whole point of E024: a literal must never quietly become a different
/// value. Each of these used to compile and print `0`.
const MUST_NOT_SILENTLY_COMPILE: &[&str] = &[
    // Past the classifier's own u128 intermediate.
    "let a: Int = 99999999999999999999999999999999999999999999",
    // A radix prefix with no digits. The lexer declines the prefix (#873), so
    // `0` lexes alone and `x` becomes an undefined identifier (E003) — this
    // pins the LOUDNESS, not the wording or the layer that catches it.
    "let a: Int = 0x",
];

#[test]
fn each_deviation_gets_its_own_hint() {
    let dir = tempfile::tempdir().expect("tempdir");
    for (binding, expected_hint) in REJECTED {
        let out = check(dir.path(), &body(binding));
        assert!(
            out.contains("E024"),
            "`{binding}` must be E024, got:\n{out}"
        );
        assert!(
            out.contains(expected_hint),
            "`{binding}` must be hinted as `{expected_hint}`, got:\n{out}"
        );
    }
}

#[test]
fn the_hint_quotes_the_literal_as_written() {
    let dir = tempfile::tempdir().expect("tempdir");
    // A negated literal is reported WITH its sign: "literal '5' is out of range
    // for UInt64" reads as a compiler bug when the source says `-5`.
    let out = check(dir.path(), &body("let a: UInt64 = -5"));
    assert!(
        out.contains("integer literal '-5' is out of range"),
        "negated literal must be quoted with its sign, got:\n{out}"
    );
}

#[test]
fn nothing_folds_to_zero_in_silence() {
    let dir = tempfile::tempdir().expect("tempdir");
    for binding in MUST_NOT_SILENTLY_COMPILE {
        let out = check(dir.path(), &body(binding));
        assert!(
            out.contains("error["),
            "`{binding}` must be rejected, not folded to 0 in silence, got:\n{out}"
        );
    }
}

#[test]
fn representable_boundaries_still_compile() {
    let dir = tempfile::tempdir().expect("tempdir");
    for binding in ACCEPTED {
        let out = check(dir.path(), &body(binding));
        assert!(
            !out.contains("E024"),
            "`{binding}` is representable and must not be E024, got:\n{out}"
        );
    }
}

/// A radix literal past the carrier must be caught too — the classifier splits
/// the prefix before comparing, so a hex form cannot slip past a decimal bound.
#[test]
fn radix_forms_are_classified_the_same() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = check(dir.path(), &body("let a: UInt64 = 0x8000000000000000"));
    assert!(
        out.contains("E024") && out.contains("above the i64 the compiler carries"),
        "hex one past the carrier must be the carrier deviation, got:\n{out}"
    );
    let out = check(dir.path(), &body("let a: UInt8 = -0x1"));
    assert!(
        out.contains("E024") && out.contains("has no negative values at all"),
        "negated hex in an unsigned context must be the sign deviation, got:\n{out}"
    );
    let out = check(dir.path(), &body("let a: UInt8 = 0x100"));
    assert!(
        out.contains("E024") && out.contains("would silently fold to 0"),
        "hex past the width must be the magnitude deviation, got:\n{out}"
    );
}
