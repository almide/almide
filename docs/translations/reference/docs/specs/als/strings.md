<a id="als--文字列strings"></a>

# ALS — Strings

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](strings.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](strings.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](strings.zh-CN.md)

> Non-normative reader snapshot of the [pinned original](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/strings.md). Normative status and requirements below belong to the original, not this copy. No automatic synchronization; historical implementation, conformance and safety claims have not been independently revalidated for this translation. Code and rule identifiers are preserved.

> Last updated: 2026-09-10

The string requirements of the Almide Language Specification. Defines observable behavior (stdout, stderr and exit code) independently of the implementations (v0 native / v1 wasm). Each section is referenced by the contract ledger (the `spec` field of docs/contracts/contracts.toml), and `scripts/check-contracts.sh` enforces the existence of the sections.

<a id="als-s1-コードポイント意味論"></a>

## ALS-S1 Code-point semantics

Strings are UTF-8 byte sequences, and `string.len`, `slice`, `chars`, `string.get` and `index_of` count in **code points** (not bytes or graphemes; `s[i]` indexing syntax is rejected for String with E026, and element access is `string.get(s, i) -> Option[String]`).
These operations must consistently use this unit for multibyte characters, including CJK, emoji and combining characters. Out of range, `string.get` returns none and `slice` returns the empty string. Explicit byte-level operations are defined in ALS-S4.

Contracts: C-016.

<a id="als-s2-空パターンの検索規則"></a>

## ALS-S2 Search rules for empty patterns

`string.count_matches("")` returns **code-point count + 1** (the empty-pattern semantics of Rust `str::matches`). `last_index_of` returns the **code-point position** of the last match; for an empty pattern it returns `len` (the end position). `index_of("")` is 0.
Contracts: C-017.

<a id="als-s3-文字種述語"></a>

## ALS-S3 Character-class predicates

The lifting rule differs **for each predicate**. There is **no** uniform rule that all universal predicates return true on the empty string (2026-08-21 decision, reference-evaluator finding F8). The following table is normative; fixtures fix each row:

| Predicate | Character set | Verdict | `""` |
|------|----------|------|------|
| `string.is_alpha` | Unicode `Alphabetic` (`char::is_alphabetic`) | Every code point matches, and the string is **nonempty** | false |
| `string.is_alphanumeric` | Unicode alphanumeric (`char::is_alphanumeric`: includes letters, numbers, `²`, `½`, Arabic digits, etc.) | Every code point matches, and the string is **nonempty** | false |
| `string.is_digit` | **ASCII `0-9` only** (not fullwidth, Arabic or Indic digits) | Every code point matches, and the string is **nonempty** | false |
| `string.is_upper` / `is_lower` | Cased code points | There is **at least one cased code point**, and **all** cased code points are uppercase (lowercase). Uncased characters (`.`, whitespace, CJK, digits) are ignored (`A.B` is upper; `日2` is false for both; `Ａa` is false for both) | false |
| `string.is_whitespace` | Unicode `White_Space` (the set in ALS-T1) | Every code point matches. **Empty is vacuously true** | **true** |

ASCII-only approximations are nonconforming except for `is_digit`. `replace_first`, `strip_prefix`, `strip_suffix` and `cmp` are observationally equivalent to the corresponding Rust str methods (`cmp` uses byte lexicographic order).

```almide
test "each predicate lifts its own way over the empty string" {
  assert_eq(string.is_alpha(""), false)
  assert_eq(string.is_alphanumeric(""), false)
  assert_eq(string.is_digit(""), false)
  assert_eq(string.is_upper(""), false)
  assert_eq(string.is_whitespace(""), true)
  assert_eq(string.is_upper("A.B"), true)
  assert_eq(string.is_alphanumeric("\u{00BD}"), true)
  assert_eq(string.is_digit("\u{FF11}"), false)
}
```

Fixtures: `spec/wasm_cross/string_predicates.almd`, `spec/wasm_cross/string_ops_drain.almd`,
`spec/wasm_cross/string_whitespace.almd`, `spec/wasm_cross/string_is_alphanumeric.almd`.
Contracts: C-018, C-019.

<a id="als-s4-バイト列との相互変換"></a>

## ALS-S4 Conversion to and from byte sequences

`bytes.to_string_lossy` performs **lossy UTF-8 decoding** (invalid sequences are replaced by U+FFFD, using the same replacement rules as `String::from_utf8_lossy`). `bytes.from_string` returns the UTF-8 bytes unchanged, and `to_string_lossy ∘ from_string` is the identity on valid strings.
`string.byte_slice(s, start, end)` copies a half-open interval measured in UTF-8 bytes and returns `some(String)` only if both ends are code-point boundaries and `0 <= start <= end <= バイト長` (byte length). Otherwise it returns `none`. An empty interval on valid boundaries is `some("")`. Endpoint checks do not scan outside the interval, and the amount copied is the number of bytes in the specified interval. The clamping rules of code-point-based `string.slice` remain unchanged.

Fixtures: `spec/wasm_cross/string_from_bytes.almd`,
`spec/wasm_cross/string_byte_slice.almd`.
Contracts: C-022, C-348.

<a id="als-s5-split-の区切り規範"></a>

## ALS-S5 Delimiter rules for split

`string.split` treats escapes and special characters in the delimiter string literally (not as a regular expression). Consecutive delimiters produce empty elements, as do leading and trailing delimiters (observationally equivalent to Rust `str::split`).
Contracts: C-050.

<a id="als-s6-規模不変性"></a>

## ALS-S6 Scale invariance

Repeated string operations such as split / replace remain correct independently of input scale (MB-sized inputs): internal buffer boundaries and reallocations do not change the result.
Contracts: C-074.

> Source context: ALS-S1 states the out-of-range slice result broadly, while ALS-S4 also preserves string.slice clamping rules. Both statements are retained without inferring a unified edge-case rule; consult the pinned specification and fixtures for a particular interval.
