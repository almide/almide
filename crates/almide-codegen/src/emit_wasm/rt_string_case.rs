//! Emit-time Unicode case-mapping tables for the WASM runtime.
//!
//! Native `string.to_upper/to_lower/capitalize` call Rust `str::to_uppercase()` /
//! `str::to_lowercase()` (full Unicode). The WASM runtime must produce
//! **byte-identical** output. We achieve this by GENERATING the case-mapping
//! tables AT EMIT TIME from the SAME `char::to_uppercase()` / `char::to_lowercase()`
//! the native runtime links — so the tables are an exact serialization of `std`
//! and are locked to the compiler toolchain's Unicode version. No UCD files, no
//! `build.rs`, no external crate: Rust `std` IS the oracle.
//!
//! The one context-sensitive scalar in all of Unicode — Greek capital sigma
//! U+03A3, whose lowercasing depends on `Final_Sigma` (→ ς word-finally, else σ) —
//! is resolved by a runtime scan over the `Cased` / `Case_Ignorable` property sets.
//! Those two properties are *also* derived purely from `str::to_lowercase` (via a
//! U+03A3 probe — see [`cased`] / [`case_ignorable`]), NEVER from
//! `char::is_lowercase`, which mislabels modifier letters (e.g. Lm U+02B0) and
//! would silently corrupt Final_Sigma.

use std::sync::LazyLock;

/// Greek capital sigma — the sole context-sensitive scalar; excluded from the
/// lower map and resolved at runtime.
pub const SIGMA: u32 = 0x03A3;
/// Greek small final sigma ς (U+03C2) — Final_Sigma "word-final" result.
pub const FINAL_SIGMA: u32 = 0x03C2;
/// Greek small sigma σ (U+03C3) — Final_Sigma "medial" result.
pub const MEDIAL_SIGMA: u32 = 0x03C3;

/// One case-mapping direction, serialized for embedding as three parallel blobs:
/// `keys` (binary-search array), `val_offsets` (record offset within `vals` per
/// key), and `vals` (packed `[out_len:u8][utf8 bytes...]` records).
pub struct CaseMap {
    /// Sorted scalars that have a non-identity mapping.
    pub keys: Vec<u32>,
    /// For each key, the byte offset of its value record within `vals`.
    pub val_offsets: Vec<u32>,
    /// Packed value records: `[out_len:u8][utf8 bytes...]` per key.
    pub vals: Vec<u8>,
}

/// All four oracle-derived tables.
pub struct CaseTables {
    /// `to_upper` map (`char::to_uppercase`), ASCII excluded (inline fast path).
    pub upper: CaseMap,
    /// `to_lower` map (`char::to_lowercase`), ASCII excluded AND U+03A3 excluded
    /// (Final_Sigma resolved at runtime).
    pub lower: CaseMap,
    /// Sorted scalars with the Unicode `Cased` property.
    pub cased: Vec<u32>,
    /// Sorted scalars with the Unicode `Case_Ignorable` property.
    pub case_ignorable: Vec<u32>,
}

fn is_surrogate(cp: u32) -> bool {
    (0xD800..=0xDFFF).contains(&cp)
}

/// `Cased(c)`: place `c` immediately before Σ; `str::to_lowercase` maps Σ to
/// final-sigma ς iff Σ is preceded by a `Cased` char (and followed by none).
fn cased(c: char) -> bool {
    format!("{}\u{03A3}", c).to_lowercase().ends_with('\u{03C2}')
}

/// `Case_Ignorable(c)`: in `"a{c}Σ"` the backward `Final_Sigma` scan skips an
/// ignorable `c` and reaches the cased `'a'` → ς. `Cased` chars (which also yield
/// ς, for a different reason) and `'a'` itself are excluded.
fn case_ignorable(c: char) -> bool {
    if c == 'a' || cased(c) {
        return false;
    }
    format!("a{}\u{03A3}", c).to_lowercase().ends_with('\u{03C2}')
}

fn push_utf8(out: &mut Vec<u8>, chars: &[char]) {
    for c in chars {
        let mut buf = [0u8; 4];
        out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
    }
}

fn build_map(upper: bool) -> CaseMap {
    let mut keys = Vec::new();
    let mut val_offsets = Vec::new();
    let mut vals = Vec::new();
    for cp in 0u32..=0x10FFFF {
        if is_surrogate(cp) {
            continue;
        }
        // ASCII is handled by the inline byte fast path; never in the table.
        if cp < 0x80 {
            continue;
        }
        // Σ lowercasing is context-sensitive → resolved by the runtime scan.
        if !upper && cp == SIGMA {
            continue;
        }
        let c = char::from_u32(cp).unwrap();
        let mapped: Vec<char> = if upper {
            c.to_uppercase().collect()
        } else {
            c.to_lowercase().collect()
        };
        if mapped.as_slice() == [c] {
            continue; // identity mapping
        }
        let mut rec = Vec::new();
        push_utf8(&mut rec, &mapped);
        assert!(rec.len() <= u8::MAX as usize, "case map value too long: U+{cp:04X}");
        val_offsets.push(vals.len() as u32);
        vals.push(rec.len() as u8);
        vals.extend_from_slice(&rec);
        keys.push(cp);
    }
    CaseMap { keys, val_offsets, vals }
}

fn build_property(pred: fn(char) -> bool) -> Vec<u32> {
    let mut v = Vec::new();
    for cp in 0u32..=0x10FFFF {
        if is_surrogate(cp) {
            continue;
        }
        if pred(char::from_u32(cp).unwrap()) {
            v.push(cp);
        }
    }
    v
}

/// Build (once, cached) the four case tables. Pure and deterministic (~4M char
/// ops, <100ms); the `LazyLock` keeps it to once per process, not per module.
pub fn generate_case_tables() -> &'static CaseTables {
    static TABLES: LazyLock<CaseTables> = LazyLock::new(|| CaseTables {
        upper: build_map(true),
        lower: build_map(false),
        cased: build_property(cased),
        case_ignorable: build_property(case_ignorable),
    });
    &TABLES
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup(map: &CaseMap, scalar: u32) -> Option<&[u8]> {
        match map.keys.binary_search(&scalar) {
            Ok(i) => {
                let off = map.val_offsets[i] as usize;
                let len = map.vals[off] as usize;
                Some(&map.vals[off + 1..off + 1 + len])
            }
            Err(_) => None,
        }
    }

    fn identity_bytes(scalar: u32) -> Vec<u8> {
        let mut buf = [0u8; 4];
        char::from_u32(scalar)
            .unwrap()
            .encode_utf8(&mut buf)
            .as_bytes()
            .to_vec()
    }

    /// Models the runtime `to_upper` pipeline for a single scalar.
    fn pipeline_upper(scalar: u32, t: &CaseTables) -> Vec<u8> {
        if scalar < 0x80 {
            let b = scalar as u8;
            return vec![if (0x61..=0x7A).contains(&b) { b - 32 } else { b }];
        }
        lookup(&t.upper, scalar)
            .map(|v| v.to_vec())
            .unwrap_or_else(|| identity_bytes(scalar))
    }

    /// Models the runtime `to_lower` pipeline for a single *isolated* scalar
    /// (lone Σ → σ: Before=false, Final_Sigma medial).
    fn pipeline_lower_isolated(scalar: u32, t: &CaseTables) -> Vec<u8> {
        if scalar < 0x80 {
            let b = scalar as u8;
            return vec![if (0x41..=0x5A).contains(&b) { b + 32 } else { b }];
        }
        if scalar == SIGMA {
            return identity_bytes(MEDIAL_SIGMA);
        }
        lookup(&t.lower, scalar)
            .map(|v| v.to_vec())
            .unwrap_or_else(|| identity_bytes(scalar))
    }

    /// Layer 1: the emitted tables reproduce `char::to_uppercase`/`to_lowercase`
    /// for ALL 1.1M scalars, byte-for-byte.
    #[test]
    fn tables_reproduce_char_methods() {
        let t = generate_case_tables();
        for cp in 0u32..=0x10FFFF {
            if is_surrogate(cp) {
                continue;
            }
            let c = char::from_u32(cp).unwrap();

            let want_u: Vec<u8> = c.to_uppercase().collect::<String>().into_bytes();
            let got_u = pipeline_upper(cp, t);
            assert_eq!(got_u, want_u, "to_upper mismatch at U+{cp:04X}");

            // char::to_lowercase has no context, so it equals the isolated pipeline.
            let want_l: Vec<u8> = c.to_lowercase().collect::<String>().into_bytes();
            let got_l = pipeline_lower_isolated(cp, t);
            assert_eq!(got_l, want_l, "to_lower mismatch at U+{cp:04X}");
        }
    }

    /// Models the runtime `to_lower` over a whole string using ONLY the serialized
    /// `cased` / `case_ignorable` membership arrays — the exact data the wasm
    /// Final_Sigma scan reads.
    fn my_lower(chars: &[char], t: &CaseTables) -> String {
        let is_cased = |c: char| t.cased.binary_search(&(c as u32)).is_ok();
        let is_ci = |c: char| t.case_ignorable.binary_search(&(c as u32)).is_ok();
        let mut out = String::new();
        for i in 0..chars.len() {
            let c = chars[i];
            if c as u32 == SIGMA {
                let before = (0..i).rev().find(|&j| !is_ci(chars[j])).map_or(false, |j| is_cased(chars[j]));
                let after = ((i + 1)..chars.len()).find(|&j| !is_ci(chars[j])).map_or(false, |j| is_cased(chars[j]));
                out.push(char::from_u32(if before && !after { FINAL_SIGMA } else { MEDIAL_SIGMA }).unwrap());
            } else {
                out.extend(c.to_lowercase());
            }
        }
        out
    }

    /// Layer 1b: the serialized property sets + Final_Sigma scan reproduce
    /// `str::to_lowercase` over an exhaustive window matrix.
    #[test]
    fn final_sigma_via_serialized_sets() {
        let t = generate_case_tables();
        // Behaviorally-distinct alphabet: cased/uncased letters, a titlecase Lt,
        // digit, space, period, apostrophe, soft-hyphen (CI), combining (CI), two
        // modifier letters (CI, the is_lowercase trap), and Σ itself.
        let alpha: Vec<char> = vec![
            'a', 'Z', 'α', 'Α', '\u{01F2}', '1', ' ', '.', '\'',
            '\u{00AD}', '\u{0301}', '\u{02B0}', '\u{02BC}', '\u{03A3}', 'b',
        ];
        let mut window = Vec::new();
        let mut count = 0usize;
        fn rec(prefix: &mut Vec<char>, depth: usize, alpha: &[char], t: &CaseTables, count: &mut usize) {
            if depth == 0 {
                let s: String = prefix.iter().collect();
                assert_eq!(my_lower(prefix, t), s.to_lowercase(), "Final_Sigma window {s:?}");
                *count += 1;
                return;
            }
            for &a in alpha {
                prefix.push(a);
                rec(prefix, depth - 1, alpha, t, count);
                prefix.pop();
            }
        }
        for len in 1..=4 {
            rec(&mut window, len, &alpha, t, &mut count);
        }
        assert_eq!(count, 54240);
    }

    /// Sanity: counts match the independently verified design figures.
    #[test]
    fn table_counts() {
        let t = generate_case_tables();
        // ASCII a-z (26) excluded from upper, A-Z (26) from lower vs the raw 1580/1487.
        assert_eq!(t.upper.keys.len(), 1554);
        assert_eq!(t.lower.keys.len(), 1461);
        assert_eq!(t.cased.len(), 4364);
        assert_eq!(t.case_ignorable.len(), 2794);
        // Disjoint by construction.
        for cp in &t.cased {
            assert!(t.case_ignorable.binary_search(cp).is_err(), "overlap at U+{cp:04X}");
        }
        // keys are sorted (binary search precondition).
        assert!(t.upper.keys.windows(2).all(|w| w[0] < w[1]));
        assert!(t.lower.keys.windows(2).all(|w| w[0] < w[1]));
    }
}
