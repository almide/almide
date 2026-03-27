<!-- description: Cross-language comparison of numeric and collection literal syntax -->
# Literal Syntax Gaps [DONE]

All items implemented as of v0.4.7.

Almide's literal syntax compared against 7 languages (Python, JS, Rust, Haskell, Go, Kotlin, Swift).

## Numeric Literals

| Notation | Python | JS | Rust | Go | Kotlin | Swift | Almide | Status |
|----------|--------|----|------|----|--------|-------|--------|--------|
| Decimal `42` | Y | Y | Y | Y | Y | Y | Y | Done |
| Float `3.14` | Y | Y | Y | Y | Y | Y | Y | Done |
| Negative `-1` | Y | Y | Y | Y | Y | Y | Y | Done |
| Scientific `1.5e10` | Y | Y | Y | Y | Y | Y | Y | **Just added** |
| Scientific negative exp `6.674e-11` | Y | Y | Y | Y | Y | Y | Y | **Just added** |
| Scientific positive exp `3e+8` | Y | Y | Y | Y | Y | Y | Y | **Just added** |
| Scientific int `1e6` | Y | Y | Y | Y | Y | Y | Y | **Just added** |
| Hex `0xFF` | Y | Y | Y | Y | Y | Y | Y | Done (converted to decimal) |
| **Binary `0b1010`** | Y | Y | Y | Y | Y | Y | **N** | Missing |
| **Octal `0o77`** | Y | Y | Y | Y | Y | Y | **N** | Missing |
| **Underscore sep `1_000_000`** | Y | Y | Y | Y | Y | Y | **N** | Missing |
| **Underscore in hex `0xFF_FF`** | Y | Y | Y | Y | Y | Y | **N** | Missing |

### Assessment

- **Binary / Octal**: Every major language supports these. Useful for bitwise operations, permissions, flags.
- **Underscore separators**: Universal in modern languages. Critical for readability of large numbers (`1_000_000` vs `1000000`). No semantic change — just ignored during parsing.

## String Literals

| Feature | Python | JS | Rust | Go | Kotlin | Swift | Almide | Status |
|---------|--------|----|------|----|--------|-------|--------|--------|
| Double-quoted `"hello"` | Y | Y | Y | Y | Y | Y | Y | Done |
| Interpolation `"${expr}"` | — | Y | — | — | Y | Y | Y | Done |
| Escape `\n \t \\ \"` | Y | Y | Y | Y | Y | Y | Y | Done |
| Escape `\$` | — | — | — | — | Y | — | Y | Done |
| **Escape `\r`** (carriage return) | Y | Y | Y | Y | Y | Y | **N** | Missing |
| **Escape `\0`** (null) | Y | Y | Y | Y | Y | Y | **N** | Missing |
| **Unicode `\u{1F600}`** | — | Y | Y | — | Y | Y | **N** | Missing |
| **Unicode `\uXXXX`** | Y | Y | — | Y | Y | — | **N** | Missing |
| Heredoc / multiline `"""..."""` | Y | Y(`) | — | Y(`) | Y | Y | Y (`\|`) | Done (different syntax) |
| Raw string `r"..."` | Y | — | Y | Y(`) | — | — | Y (`r\|...\|`) | Done |
| **Char literal `'a'`** | — | — | Y | — | — | Y | **N** | Not planned (String-only by design) |

### Assessment

- **`\r` and `\0`**: Trivial to add. `\r` needed for Windows line endings, `\0` for C interop/binary.
- **Unicode escapes**: Important for internationalization. `\u{XXXX}` (variable-length, Rust/JS style) is the modern standard.

## Other Literal Types

| Feature | Python | JS | Rust | Go | Kotlin | Swift | Almide | Status |
|---------|--------|----|------|----|--------|-------|--------|--------|
| Boolean `true`/`false` | Y | Y | Y | Y | Y | Y | Y | Done |
| None/null `none` | Y | Y | — | Y | Y | Y | Y | Done |
| Unit `()` | — | — | Y | — | — | — | Y | Done |
| List `[1, 2, 3]` | Y | Y | — | — | Y | Y | Y | Done |
| Tuple `(1, "a")` | Y | — | Y | — | — | Y | Y | Done |
| Record `{ x: 1 }` | — | Y | — | — | — | — | Y | Done |
| Map `map.new()` | — | — | — | — | — | — | Y | Done (stdlib, not literal) |
| **Map literal `{k: v}`** | Y | Y | — | Y | Y | Y | **N** | Not planned (use map.new + set) |
| **Set literal `{1, 2}`** | Y | Y | — | — | Y | Y | **N** | Not planned |
| **Regex `/pattern/`** | — | Y | — | — | Y | — | **N** | Not planned |

## Implementation priority

### Tier 1 — Should add (universal, easy, high LLM frequency)
1. **Underscore separators** `1_000_000` — all 7 languages support. Trivial: ignore `_` in `read_number`. LLMs generate these frequently.
2. **`\r` and `\0` escapes** — all 7 languages. One-line each in `read_string`.

### Tier 2 — Should add (common, moderate effort)
3. **Binary literals** `0b1010` — 7/7 languages. Same pattern as hex.
4. **Octal literals** `0o77` — 7/7 languages. Same pattern as hex.
5. **Unicode escapes** `\u{1F600}` — 5/7 languages. Moderate: parse hex in braces, emit char.

### Tier 3 — Not planned
- Char literals — Almide is String-only by design
- Map/Set literals — use stdlib constructors
- Regex literals — out of scope

## Deferred: Unicode identifiers

Japanese/CJK variable names (`契約者名`, `甲`, `乙`) would improve readability for legal/domain-specific code. Implementation is trivial (`is_alpha_num` → `ch.is_alphanumeric()`), Rust/JS targets both support Unicode identifiers. Deferred because LLMs generate ASCII identifiers more reliably, and Unicode introduces typo risks (fullwidth/halfwidth confusion). Revisit if domain-specific use cases (legal, i18n) become a priority.

## Sources

- [PEP 515 – Underscores in Numeric Literals](https://peps.python.org/pep-0515/)
- [Go Number Literals Proposal](https://github.com/golang/proposal/blob/master/design/19308-number-literals.md)
- [Rosetta Code: Numeric Separator Syntax](https://rosettacode.org/wiki/Numeric_separator_syntax)
- [Wikipedia: String Literal](https://en.wikipedia.org/wiki/String_literal)
- [Python Lexical Analysis](https://docs.python.org/3/reference/lexical_analysis.html)
