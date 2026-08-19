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

/// Decode an int literal token to the `i64` SLOT the IR carries.
///
/// The slot is a 64-BIT PATTERN, not a signed number: a magnitude in
/// `UInt64`'s upper half (`i64::MAX+1 ..= u64::MAX`) parses as `u64` and is
/// reinterpreted, exactly as the renderers interpret it — `IntOp::DivU` and
/// friends read the same slot unsigned (#872). Every OTHER context rejects
/// such a magnitude with E024 before lowering (its declared domain stops at
/// or below `i64::MAX`), so the reinterpretation is only ever observed where
/// it is the right reading.
///
/// Returns 0 for a token that parses as neither. That is not error recovery:
/// the lexer cannot produce such a token, and a magnitude past `u64::MAX` is
/// E024 in every context — the fallback context when no type is recorded is
/// `Int`, the strictest of them.
pub(crate) fn int_value(raw: &str) -> i64 {
    let clean = raw.replace('_', "");
    let (radix, digits) = radix_and_digits(&clean);
    if let Ok(v) = i64::from_str_radix(digits, radix) {
        return v;
    }
    u64::from_str_radix(digits, radix).map(|u| u as i64).unwrap_or(0)
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
