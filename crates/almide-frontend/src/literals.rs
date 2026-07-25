//! The one implementation of integer-literal decoding.
//!
//! The radix prefix chain used to be written out four times — once in the
//! checker's overflow guard and once each in three lowering sites (plain
//! expressions, negated expressions, and patterns). The checker's copy carried
//! a comment reading "mirrors the radix parsing in lowering so the check and
//! the eventual value agree", which is an admission that the agreement was
//! maintained by hand. It is now maintained by there being one function.

/// Strip an int literal's `0x`/`0b`/`0o` prefix (case-insensitive) and return
/// `(radix, remaining digits)`. No prefix means base 10.
///
/// `clean` must already have its `_` separators removed.
pub(crate) fn radix_and_digits(clean: &str) -> (u32, &str) {
    if let Some(r) = clean.strip_prefix("0x").or_else(|| clean.strip_prefix("0X")) {
        (16, r)
    } else if let Some(r) = clean.strip_prefix("0b").or_else(|| clean.strip_prefix("0B")) {
        (2, r)
    } else if let Some(r) = clean.strip_prefix("0o").or_else(|| clean.strip_prefix("0O")) {
        (8, r)
    } else {
        (10, clean)
    }
}

/// Decode an int literal token to its `i64` value.
///
/// Returns 0 for a token that does not parse. That is not error recovery here:
/// the lexer cannot produce such a token, and a literal too large for `i64` is
/// already reported by the checker's `int_literal_overflows_i64` guard, so by
/// the time lowering runs the only reachable failures have a diagnostic
/// attached and the value is never observed.
pub(crate) fn int_value(raw: &str) -> i64 {
    let clean = raw.replace('_', "");
    let (radix, digits) = radix_and_digits(&clean);
    i64::from_str_radix(digits, radix).unwrap_or(0)
}

/// Decode a negated int literal token to its `i64` value.
///
/// Parsing the negation as part of the token rather than negating afterwards is
/// what lets `-9223372036854775808` land on `i64::MIN`, whose magnitude has no
/// positive `i64` representation to negate.
pub(crate) fn negated_int_value(raw: &str) -> Option<i64> {
    let clean = raw.replace('_', "");
    let (radix, digits) = radix_and_digits(&clean);
    i64::from_str_radix(&format!("-{digits}"), radix).ok()
}
