//! Oracle-derived Unicode property range tables for WASM string predicates.
//!
//! Native Almide implements `string.is_alpha`/`is_alphanumeric`/`is_upper`/
//! `is_lower` via Rust's `char::is_alphabetic` / `is_alphanumeric` /
//! `is_uppercase` / `is_lowercase`, which are full-Unicode. The WASM target
//! historically byte-looped over the ASCII ranges only, so every non-ASCII
//! scalar diverged from native.
//!
//! To close the gap WITHOUT shipping a UCD parser, we derive the property sets
//! directly from the `char` methods AT COMPILE TIME: iterate every scalar in
//! `0..=0x10FFFF`, ask the oracle method, and run-length compress the answers
//! into sorted inclusive `[lo, hi]` codepoint ranges. The ranges are embedded
//! in the WASM data section and binary-searched at runtime per decoded scalar.
//!
//! This is the same oracle-probe strategy already proven for the case tables
//! (`rt_string_case`) and the whitespace ranges (`whitespace_ranges` in
//! `rt_string`): the `char` standard library is the single source of truth, so
//! native and WASM are correct-by-construction equivalent, and the `#[test]`
//! Σ-probe (see tests at the bottom) re-derives the answer and asserts
//! byte-for-byte agreement over the entire scalar space — a regression lock.
//!
//! Unlike the case tables (front-embedded at a fixed offset, always live), these
//! tables go through `intern_bytes` so the dead-data eliminator can drop any
//! table no live predicate references. The membership helper therefore emits the
//! BARE interned offset as its only data `i32.const`; the `[len][cap]` header
//! skip and per-entry stride are added at RUNTIME so a DCE relocation of the
//! offset stays correct (see `compile_prop_membership`).

use std::sync::LazyLock;

/// Highest Unicode scalar value (inclusive). Surrogates `0xD800..=0xDFFF` are
/// not scalar values; `char::from_u32` returns `None` for them and they are
/// simply skipped during derivation (they never appear in valid UTF-8).
const MAX_SCALAR: u32 = 0x10FFFF;

/// Each embedded range entry is two little-endian `u32`s: `[lo, hi]` inclusive.
pub const RANGE_ENTRY_BYTES: usize = 8;

/// Which `char` property a table encodes. The variant order is irrelevant; the
/// derivation reads the property method, nothing else.
#[derive(Clone, Copy)]
pub enum UnicodeProp {
    Alphabetic,
    Alphanumeric,
    Uppercase,
    Lowercase,
}

impl UnicodeProp {
    fn holds(self, c: char) -> bool {
        match self {
            UnicodeProp::Alphabetic => c.is_alphabetic(),
            UnicodeProp::Alphanumeric => c.is_alphanumeric(),
            UnicodeProp::Uppercase => c.is_uppercase(),
            UnicodeProp::Lowercase => c.is_lowercase(),
        }
    }

    /// Stable interning key for the table's data-section blob. Prefixed with a
    /// NUL so it can never collide with an interned source string literal (whose
    /// key is its exact textual content).
    fn intern_key(self) -> &'static str {
        match self {
            UnicodeProp::Alphabetic => "\u{0}unicode:alphabetic",
            UnicodeProp::Alphanumeric => "\u{0}unicode:alphanumeric",
            UnicodeProp::Uppercase => "\u{0}unicode:uppercase",
            UnicodeProp::Lowercase => "\u{0}unicode:lowercase",
        }
    }
}

/// Run-length compress the scalars satisfying `prop` into sorted inclusive
/// `[lo, hi]` ranges, serialized as little-endian `u32` pairs.
fn derive_table_bytes(prop: UnicodeProp) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut run_start: Option<u32> = None;
    let mut prev = 0u32;
    for cp in 0..=MAX_SCALAR {
        let holds = char::from_u32(cp).map_or(false, |c| prop.holds(c));
        match (holds, run_start) {
            (true, None) => run_start = Some(cp),
            (false, Some(start)) => {
                bytes.extend_from_slice(&start.to_le_bytes());
                bytes.extend_from_slice(&prev.to_le_bytes());
                run_start = None;
            }
            _ => {}
        }
        if holds {
            prev = cp;
        }
    }
    if let Some(start) = run_start {
        bytes.extend_from_slice(&start.to_le_bytes());
        bytes.extend_from_slice(&MAX_SCALAR.to_le_bytes());
    }
    bytes
}

/// Derivation is non-trivial (a 1.1M-scalar scan); cache per property so the
/// emitter and the Σ-probe tests share one pass instead of re-deriving.
fn cached_table_bytes(prop: UnicodeProp) -> &'static [u8] {
    static ALPHABETIC: LazyLock<Vec<u8>> = LazyLock::new(|| derive_table_bytes(UnicodeProp::Alphabetic));
    static ALPHANUMERIC: LazyLock<Vec<u8>> = LazyLock::new(|| derive_table_bytes(UnicodeProp::Alphanumeric));
    static UPPERCASE: LazyLock<Vec<u8>> = LazyLock::new(|| derive_table_bytes(UnicodeProp::Uppercase));
    static LOWERCASE: LazyLock<Vec<u8>> = LazyLock::new(|| derive_table_bytes(UnicodeProp::Lowercase));
    match prop {
        UnicodeProp::Alphabetic => &ALPHABETIC,
        UnicodeProp::Alphanumeric => &ALPHANUMERIC,
        UnicodeProp::Uppercase => &UPPERCASE,
        UnicodeProp::Lowercase => &LOWERCASE,
    }
}

/// Intern a property's range table into the data section, returning the memory
/// offset of its `[len][cap][data]` blob. Deduplicated by the property's key,
/// so repeated calls (one per predicate that needs the table) share one copy.
pub(super) fn intern_table(emitter: &mut super::WasmEmitter, prop: UnicodeProp) -> u32 {
    let bytes = cached_table_bytes(prop);
    emitter.intern_bytes(prop.intern_key(), bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Re-decode a serialized range table back into a membership closure and
    /// assert it agrees with the oracle `char` method over EVERY scalar. This is
    /// the Σ-probe regression lock: if a future toolchain shifts a Unicode
    /// property, this fails loudly instead of silently diverging from native.
    fn assert_table_matches(prop: UnicodeProp) {
        let bytes = cached_table_bytes(prop);
        // Decode ranges.
        let ranges: Vec<(u32, u32)> = bytes
            .chunks_exact(RANGE_ENTRY_BYTES)
            .map(|c| {
                let lo = u32::from_le_bytes([c[0], c[1], c[2], c[3]]);
                let hi = u32::from_le_bytes([c[4], c[5], c[6], c[7]]);
                (lo, hi)
            })
            .collect();
        // Ranges must be sorted and non-overlapping (binary search precondition).
        for w in ranges.windows(2) {
            assert!(w[0].1 < w[1].0, "ranges out of order/overlap: {:?} then {:?}", w[0], w[1]);
        }
        let member = |cp: u32| ranges.binary_search_by(|&(lo, hi)| {
            if cp < lo { std::cmp::Ordering::Greater }
            else if cp > hi { std::cmp::Ordering::Less }
            else { std::cmp::Ordering::Equal }
        }).is_ok();
        for cp in 0..=MAX_SCALAR {
            let oracle = char::from_u32(cp).map_or(false, |c| prop.holds(c));
            assert_eq!(member(cp), oracle, "scalar U+{cp:04X} table != char method");
        }
    }

    #[test]
    fn alphabetic_table_matches_char_method() {
        assert_table_matches(UnicodeProp::Alphabetic);
    }

    #[test]
    fn alphanumeric_table_matches_char_method() {
        assert_table_matches(UnicodeProp::Alphanumeric);
    }

    #[test]
    fn uppercase_table_matches_char_method() {
        assert_table_matches(UnicodeProp::Uppercase);
    }

    #[test]
    fn lowercase_table_matches_char_method() {
        assert_table_matches(UnicodeProp::Lowercase);
    }
}
