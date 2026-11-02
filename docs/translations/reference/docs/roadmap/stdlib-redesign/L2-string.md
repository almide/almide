Description: Stdlib redesign L2/L3 — string: one join (list.join), the UTF-8 codec moves wholly to bytes, run_length_encode and length retire, compare/byte_len/UTF-16 are added
<a id="l2l3-string"></a>

# L2/L3: string

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-string.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-string.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-string.zh-CN.md)

> Non-normative reader snapshot pinned to the original commit; the original remains authoritative. No automatic synchronization or independent execution, implementation verification, proof checking or measurement. Historical commands and design instructions are documentation, not executed instructions.
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-string.md)

> Source-history caveat: this is a dated redesign survey and proposal record, not independently rechecked documentation, current implementation evidence, or a new security/performance guarantee. The source’s (F) labels describe its own document checks, and its probe observations retain the named development version. Accepted proposals and migration epochs do not by themselves establish completion. Earlier first-codepoint behavior in R-string-12 is revised to exactly one code point in R-string-14; the rejected predicate count in R-string-13 is superseded by R-string-18. Earlier chars-based recipes coexist with the later to_list rename. The introductory 50 functions/48 copying functions and concluding 50 + 4 = 54 rows are retained despite later table additions. R-string-15’s two-argument slice/drop equivalence is retained alongside the boundary rules, which treat a negative slice start and a negative drop count differently; do not generalize it across those inputs. Constant-time equality explicitly returns immediately for differing byte lengths: the timing claim is not length-independent, and HMAC verification is directed to its separate API.

> Last updated: 2026-10-24
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)・[L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)
> Sources: Rust https://doc.rust-lang.org/std/primitive.str.html (F) ・ Go https://pkg.go.dev/strings (F) ・ Python https://docs.python.org/3/library/stdtypes.html (F) ・ TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/String (F) ・ Gleam https://gleam-stdlib.hexdocs.pm/gleam/string.html (F) ・ Elixir https://elixir.hexdocs.pm/String.html (F) ・ OCaml https://ocaml.org/manual/latest/api/String.html (F) ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.text/ (partly F) ・ Swift https://developer.apple.com/documentation/swift/string （Could not fetch because the page is JS-rendered. Swift cells have no F）
> Measurements: `almide 0.68.0 (dev, 437b8f127)`; probes in `scratchpad/sr/probe-string/` (a–e.almd)

Reading conventions (first appearance):
- `|>` (read: pipe) = insert the value on the left as the first argument of the function on the right
- `??` (read: null coalescing) = use the right-hand side if the left is none / err
- `mut` (read: mut) = an argument that rewrites the caller's `var` in place
- (F) = a cell checked by fetching the official documentation in this review

`string` currently has 50 functions (like `list`, it is available from all files without an import). All are pure computation and use no host facilities. Differences between native and wasm are guarded by the contract ledger (the C-xxx string family, including the code-point contracts for `string.split("")` and RLE).

---

<a id="l2-役割の比較"></a>

## L2: Comparison of roles

| Role | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| Length (characters) | `len` (code-point count ) | `chars().count()` | `utf8.RuneCountInString` | `len` | `length` (UTF-16 units ) | `length` (graphemes ) (F) | `length` (graphemes ) (F) | — | `length` (UTF-16 ) | `count` (graphemes ) |
| Length (bytes) | — | `len` (F) | `len` | `len(s.encode())` | — (F: `TextEncoder` required) | `byte_size` (F) | `byte_size` (Kernel )(F) | `length` (F) | `encodeToByteArray().size` | `utf8.count` |
| Emptiness check | `is_empty` | `is_empty` | — | — | — | `is_empty` (F) | — | `is_empty` | `isEmpty` | `isEmpty` |
| Contains substring | `contains` | `contains` (F) | `Contains` (F) | `in` | `includes` | `contains` (F) | `contains?` (F) | `includes` (5.5 onward )| `contains` | `contains` |
| Prefix / suffix match | `starts_with` / `ends_with` | `starts_with` / `ends_with` | `HasPrefix` / `HasSuffix` (F) | `startswith` / `endswith` (F) | `startsWith` / `endsWith` | `starts_with` / `ends_with` (F) | `starts_with?` / `ends_with?` (F) | `starts_with` / `ends_with` (F) | `startsWith` / `endsWith` | `hasPrefix` / `hasSuffix` |
| Find position | `index_of` / `last_index_of` | `find` / `rfind` | `Index` / `LastIndex` (F) | `find` / `rfind` (F) | `indexOf` / `lastIndexOf` | — | — | `index_opt` / `rindex_opt` (F) | `indexOf` / `lastIndexOf` | `firstRange(of:)` |
| Occurrence count | `count` | `matches().count()` (F) | `Count` (F) | `count` (F) | — | — | `count` (F) | — | — | — |
| Split | `split` / `split_once` / `lines` | `split` / `split_once` / `lines` (F) | `Split` / `Cut` / `Lines` (F) | `split` / `partition` / `splitlines` (F) | `split` (F) | `split` / `split_once` (F) | `split` / `split_at` (F) | `split_on_char` (F) | `split` / `lines` (F) | `split(separator:)` |
| Split on whitespace | — | `split_whitespace` (F) | `Fields` (F) | `split()` (F) | — (F) | — (F) | `split/1` (F) | — (F) | — | — |
| Join | `join` (duplicates `list.join`) | `[..].join` | `Join` (F) | `sep.join` (F) | `Array.join` | `join` (F) | `Enum.join` | `concat` (F) | `joinToString` | `joined(separator:)` |
| Trim (whitespace) | `trim` / `trim_start` / `trim_end` | `trim` / `trim_start` / `trim_end` | `TrimSpace` (F) | `strip` / `lstrip` / `rstrip` (F) | `trim` / `trimStart` / `trimEnd` (F) | `trim` / `trim_start` / `trim_end` (F) | `trim` / `trim_leading` / `trim_trailing` (F) | `trim` (F) | `trim` / `trimStart` / `trimEnd` | `trimmingCharacters` (Foundation )|
| Remove prefix / suffix | `strip_prefix` / `strip_suffix` (Option ) | `strip_prefix` (Option )(F) | `CutPrefix` / `TrimPrefix` (F) | `removeprefix` (unchanged if absent )(F) | — | `remove_prefix` (F) | `replace_prefix` (F) | — | `removePrefix` | — |
| Replace | `replace` / `replace_first` | `replace` / `replacen` | `ReplaceAll` / `Replace(n)` (F) | `replace(count)` (F) | `replaceAll` / `replace` (F) | `replace` (F) | `replace` (F) | — | `replace` / `replaceFirst` | `replacing` |
| Slice | `slice` / `take` / `drop` / `take_end` / `drop_end` | `[a..b]` / `get(a..b)` | `s[a:b]` | `s[a:b]` | `slice` | `slice` / `drop_start` / `drop_end` / `crop` (F) | `slice` (F) | `sub` | `substring` / `take` / `drop` | `prefix` / `dropFirst` |
| Slice by byte range | `byte_slice` | `get(a..b)` (F) | `s[a:b]` | — | — | — | `byte_slice` (F) | `sub` | — | — |
| Extract 1 character | `get` / `first` / `last` | `chars().nth` | — | `s[i]` | `at` (F) | `first` / `last` (F) | `at` / `first` / `last` (F) | `get` | `get` / `first` / `last` | `first` / `last` |
| Character list | `chars` | `chars` (F) | `[]rune(s)` | `list(s)` | `[...s]` | `to_graphemes` (F) | `graphemes` / `codepoints` (F) | — | `toList` | `Array(s)` |
| Uppercase / lowercase | `to_upper` / `to_lower` | `to_uppercase` / `to_lowercase` (F) | `ToUpper` / `ToLower` (F) | `upper` / `lower` (F) | `toUpperCase` / `toLowerCase` | `uppercase` / `lowercase` (F) | `upcase` / `downcase` (F) | `uppercase_ascii` | `uppercase` / `lowercase` | `uppercased` / `lowercased` |
| Capitalize first character | `capitalize` (rest unchanged ) | — | — (`Title` deprecated (F) ) | `capitalize` (lowercase the rest )(F) | — (F) | `capitalise` (lowercase the rest )(F) | `capitalize` (lowercase the rest )(F) | `capitalize_ascii` (rest unchanged )(F) | `replaceFirstChar` | `capitalized` (Foundation )|
| Padding | `pad_start` / `pad_end` | — | — | `rjust` / `ljust` (1 character only )(F) | `padStart` / `padEnd` (multiple characters allowed )(F) | `pad_start` / `pad_end` (multiple characters allowed )(F) | `pad_leading` / `pad_trailing` (F) | — | `padStart` / `padEnd` (Char ) | `padding` (Foundation )|
| Repeat / reverse | `repeat` / `reverse` | `repeat` (F) / `chars().rev()` | `Repeat` (F) | `*` / `[::-1]` | `repeat` | `repeat` / `reverse` (F) | `duplicate` / `reverse` (F) | — | `repeat` / `reversed` | `String(repeating:)` / `reversed` |
| Character classification | `is_digit` / `is_alpha` / `is_alphanumeric` / `is_whitespace` / `is_upper` / `is_lower` | `char::is_*` | `unicode.Is*` | `isdigit` / `isalpha` / `isspace` … (F) | — | — | — | — | `Char.isDigit` … | `Character.isNumber` … |
| Comparison (returns Int) | — | `cmp` (Ord )(F) | `Compare` (F) | — | `localeCompare` (locale-dependent )(F) | `compare` (Order )(F) | — | `compare` (F) | `compareTo` (F) | — |
| Code points | `codepoint` / `from_codepoint` | `char as u32` / `char::from_u32` | `rune` | `ord` / `chr` | `codePointAt` / `fromCodePoint` (F) | `utf_codepoint` / `from_utf_codepoints` (F) | `codepoints` / `to_charlist` (F) | `Uchar` | `codePointAt` (F) | `unicodeScalars` |
| Conversion to/from UTF-8 byte sequences | `to_bytes` / `from_bytes` (List[Int]) and `bytes.from_string` / `bytes.to_string` (Bytes) | `as_bytes` / `String::from_utf8` | `[]byte(s)` / `string(b)` | `encode` / `decode` (F) | `TextEncoder` / `TextDecoder` | `bit_array.from_string` / `to_string` | strings are binary | strings are byte sequences | `encodeToByteArray` / `decodeToString` (F) | `utf8` / `String(decoding:as:)` |
| Conversion to/from UTF-16 | — | `encode_utf16` / `String::from_utf16` (F) | `unicode/utf16` | `encode("utf-16")` (F) | internal representation (`charCodeAt` / `fromCharCode` )(F) | — | `:unicode` | `get_utf_16le_uchar` and others (F) | internal representation | `utf16` / `String(decoding:as:)` |
| Run-length encoding | `run_length_encode` | — | — | — | — | — | — | — | — | — |
| Append / clear in place | `push` / `clear` (`mut` ) | `push_str` / `clear` | `strings.Builder` | — | — | — | — | `Buffer` | `StringBuilder` | `append` / `removeAll` |

<a id="完全性の規則"></a>

### Completeness rules

1. **Boundary family (R-L0-12):** `slice`, `take`, `drop`, `take_end`, and `drop_end` are the same five operations as `list` and return the same answers for the same boundary inputs (negative `n`: `take` returns everything, `drop` returns empty. Measured in a.almd: `take("abc",-1)="abc"`, `drop("abc",-1)=""`). list gives the same answers (measured in probe-take: `list.take([1, 2, 3], -1)` has 3 elements; `list.drop` is empty. `take_end` and `drop_end` behave likewise). A negative `n` is a count, not an index, so R-L0-12's “clamp negative indices to 0” does not apply. list and string agree, and changing this would silently change results under the same names (the form prohibited by R-L0-14, item 4), so retain it. New additions such as `bytes.take` will follow these answers. `bytes` also joins the same matrix. A negative start position for `slice` is clamped to 0 as specified by R-L0-12 (negative indices are out of range and clamped to 0) (`slice("abc", -1, 2) = "ab"`). string already behaves this way; list and bytes will align with it (L2-list R-list-10, L2-bytes R-bytes-12).
2. **Retrieval family (R-L0-06):** Fallible retrievals (`get`, `first`, `last`, `index_of`, `last_index_of`, `strip_prefix`, `strip_suffix`, `codepoint`, `byte_slice`, `split_once`) return `Option` without a suffix. There are currently no gaps. There are no `_or` variants either.
3. **In-place mutation (R-L0-04):** Only `push` and `clear` take `mut`, and both use designated verbs. The other 48 functions return copies.
4. **Where encodings belong (R-L0-07):** Conversions to and from UTF-8 byte sequences both belong in `bytes` (`bytes.from_string` / `bytes.to_string` / `bytes.to_string_lossy` form a pair in the same shape as `bytes.from_list` / `bytes.to_list`). Only representations that do not pass through `Bytes` (a single code point, a sequence of UTF-16 units) remain in `string`.
5. **Deliberate omissions:** The `split` family is closed at `{split, split_once, lines}` (as noted in `stdlib/string.almd:24-25`, it does not include `rsplit_once` or `splitn`). Formatting (printf) is × in L1. There is no character type (Char); one character is a String of length 1.

<a id="l2-の判定"></a>

### L2 verdicts

- **Duplication:** `string.join` and `list.join` perform the same computation (`stdlib/string_join.almd:50` says “identical”; probe a.almd also produces the same output). They have separate intrinsics, so the current synonym gate cannot see this → R-string-01.
- **Direction and type mismatch:** `string.to_bytes` returns `List[Int]`, while `bytes.from_string` returns `Bytes` (cause 1 of #2078: 1 byte becomes 8 bytes). There are also three reverse directions: `string.from_bytes` (List[Int], replacing malformed sequences), `bytes.to_string` (Result), and `bytes.to_string_lossy` → R-string-02.
- **Single-purpose function:** `run_length_encode` (#3852) → R-string-03.
- **Deprecated without a removal date:** `string.length` (#3851 A) → R-string-04.
- **Construction returning a failure sentinel:** `from_codepoint` returns `""` for invalid values (surrogates or out-of-range values) (measured: U+D800 gives `""`). This violates R-L0-06, item 4 (fallible construction must not return sentinel values) → R-string-11.
- **Missing roles (5 or more languages):** Comparison `compare` (6 languages) → R-string-05. Byte count (6 languages have a dedicated spelling; despite having `byte_slice`, the byte count needed to determine its range cannot be obtained without copying) → R-string-06. UTF-16 (R-L1-08) → R-string-07.
- **Roles not added:** Whitespace splitting (4/9) → R-string-08 ×. Trimming specified characters, `splitn`, and listing code points → R-string-10 ×.
- **Guidance for spellings from other languages:** Writing `string.split_whitespace` currently causes E002 to suggest `string.is_whitespace` (measured in c.almd; the meaning is entirely different) → R-string-09.
- **Relationship to R-L0-05:** Writing `string.trim(s)` or `string.to_upper(s)` as a statement currently produces no error (measured in b.almd: displays `[  hi  ]`). All 48 copying functions in string are susceptible to this mistake; `trim`, `replace`, `to_upper`, and `to_lower` are especially likely to be mistaken for in-place mutation because of habits from other languages (Rust's `String` or Swift's `mutating` functions, excluding Python and JS immutable strings). The E042 extension in R-L0-05 will catch this. This page introduces no new proposal for it.

---

<a id="案"></a>

## Proposals

<a id="r-string-01-stringjoin-を撤去し-listjoin-に一本化する---採用"></a>

### R-string-01 Remove `string.join` and consolidate on `list.join` — ○ Accepted

- Content: Add `@deprecated(since=15, use="list.join")` to `string.join(list, sep)` and remove it in epoch 16. `xs |> list.join(", ")` becomes the only spelling.
- Supporting L0 rules: R-L0-01 (one spelling per role), R-L0-02 (place it in the module of the subject = first argument's type `List[String]`).
- (a) Most languages place it on the subject type (Rust slice `join`, JS `Array.join`, Kotlin `joinToString`, Swift `joined`, Elixir `Enum.join`). Others put it on the separator (Python `sep.join`) or in the string module (Go, Gleam, OCaml `concat`), but R-L0-02 determines Almide's placement.
- (b) Identical on native and wasm (both are pure computation; `list.join` is already covered by contracts on both legs).
- (c) In `xs |> list.join(",")`, the type at the head of the pipe determines the module. Keeping `string.join` leaves the “either spelling works” choice.
- (d) Removal. `removal_epoch = 16`. As a one-to-one rename, `almide fix` rewrites `string.join(xs, sep)` → `list.join(xs, sep)`. Usage counts (`\bstring\.join\b`, upper bounds including definitions and generated documentation): spec 104 / tests 26 / docs 13 / stdlib 5 (`path.almd:45,120` and others) / research 28. `list.join`: spec 390 / tests 91 / docs 30 / stdlib 24.
- Proposed gate: Adding cross-module comparison to rule 6 of the R-L0-01 synonym gate (different intrinsics with the same body) detects this (`almide_rt_string_join` and `almide_rt_list_join` are both `xs.join(sep)`).
- issue: #3947

<a id="r-string-02-utf-8-の変換は両方向とも-bytes-に置きstringto_bytes-と-stringfrom_bytes-を撤去する---採用"></a>

### R-string-02 Put both UTF-8 conversion directions in `bytes` and remove `string.to_bytes` and `string.from_bytes` — ○ Accepted

- Content:
  - Add `@deprecated(since=15, use="bytes.from_string")` to `string.to_bytes(s) -> List[Int]`. The hint when List[Int] is needed is `bytes.from_string(s) |> bytes.to_list`.
  - Add `@deprecated(since=15, use="bytes.to_string_lossy")` to `string.from_bytes(xs: List[Int]) -> String` (malformed sequences become U+FFFD). The hint is `bytes.from_list(xs) |> bytes.to_string_lossy`.
  - Remaining canonical spellings: `bytes.from_string` (String → Bytes), `bytes.to_string` (Bytes → Result[String, String]), `bytes.to_string_lossy` (Bytes → String).
- Supporting L0 rules: R-L0-07, item 2 (container construction uses `from_<元>` (元 means source) on the destination type) and item 4 (directional pairs must also align their types), and R-L0-01. R-L0-14, item 4 also permits keeping the name while changing the return type to `Bytes` (a change making old code a type error). Nevertheless, retire the names too: both UTF-8 directions belong in `bytes` (L2-bytes R-bytes-10), and retaining `string.to_bytes` as a `Bytes` version would give the same role two spellings alongside `bytes.from_string` (R-L0-01).
- (a) Rust (`as_bytes` / `String::from_utf8`) and Kotlin (`encodeToByteArray` / `decodeToString`) place conversions on the source type; Gleam's `bit_array.from_string` / `bit_array.to_string` gives both directions to the byte-sequence side. Almide's `bytes` already has the `from_list` / `to_list` pair, so placing `from_string` / `to_string` beside it is the most consistent arrangement.
- (b) native: `bytes.from_string` uses `Vec<u8>` (eliminating #2078's 1 byte = 8 bytes problem). wasm: both are pure computation, requiring no host.
- (c) Currently “I want bytes” has two answers (List[Int] and Bytes), with an 8-fold performance difference. LLMs choose the shorter name `string.to_bytes`.
- (d) Remove 2 functions. `removal_epoch = 16`. Usage counts: `string.to_bytes` spec 27 / tests 3 / docs 11 / stdlib 3 (all comments) / research 6; `string.from_bytes` spec 41 / tests 2 / docs 11 / stdlib 3 (1 call in `http_url_decode.almd:41` and comments) / research 2. Since the rewrite changes shape (adding `|> bytes.to_list`), `almide fix` chooses between two rewrites depending on whether the position expects List[Int].
- Proposed gate: Register the “String ⇄ UTF-8 byte sequence” pair in the R-L0-07 gate (match `to_<T>` against `from_<T>` to ensure the same conversion does not exist in two places). Also check that no public `string` function has a `List[Int]` return or argument type denoting UTF-8.
- issue: #2078 (answer to task 4). The removal itself is new.

<a id="r-string-03-stringrun_length_encode-を撤去する---採用"></a>

### R-string-03 Remove `string.run_length_encode` — ○ Accepted

- Content: `@deprecated(since=15, use="string.chars + list.fold")`; remove at `removal_epoch = 16`. The replacement is not a single function, so `almide fix` will not rewrite it; instead, place the two-line fold hint (`string.chars(s) |> list.fold([], ...)`) in `removed_spellings.rs`.
- Supporting L0 rules: R-L0-00's premise (none of the 9 languages has it in the standard library), the inverse of L1's addition criteria (0/9).
- (a) 0/9. It is an exercise solution, not a fundamental string operation. There is no corresponding decoder either.
- (b) Removal drops the RLE row from the contract (code-point units for `string.split("")` and RLE). Keep only the split rows in fixture `spec/wasm_cross/string_split_rle_codepoint.almd`.
- (c) Removing one public function removes one narrowly meaningful name from the LLM's candidates.
- (d) Removal. Usage counts: spec 8 / tests 2 / docs 11 / stdlib 1 (definition) / research 0.
- Proposed gate: None (one-time removal).
- issue: #3852 (first row of the table in section 2)

<a id="r-string-04-stringlength-の撤去を-epoch-16-に載せる---採用"></a>

### R-string-04 Schedule removal of `string.length` for epoch 16 — ○ Accepted

- Content: Already `@deprecated(since=3, use="string.len")` (E052 warnings and `almide fix` work; measured in d.almd). Add `[[deprecation]]` to `proofs/dialect-epochs.toml` with `removal_epoch = 16`. Keep the `→ WRONG` line in CHEATSHEET:1077.
- Supporting L0 rules: R-L0-11, R-L0-14
- (d) Removal. Keep since at its current value of 3 (the window is already open, so do not reset it to since=15), with `removal_epoch = 16`. `almide fix` performs a one-to-one rewrite from `string.length(s)` → `string.len(s)`. Post-removal hint: “`string.len(s)`”. Usage counts: spec 6 / tests 7 / docs 8 / stdlib 1 (definition).
- issue: #3851 A

<a id="r-string-05-stringcomparea-b---int-を足す---採用"></a>

### R-string-05 Add `string.compare(a, b) -> Int` — ○ Accepted

- Content: Return a negative, 0, or positive Int. Ordering matches `<` (UTF-8 byte order = code-point order; measured in e.almd: `"B" < "a"` is true). It can be passed directly to functions accepting a comparator, as in `list.sort_with(xs, string.compare)`.
- Supporting L0 rules: R-L0-16 (provide `compare(a, b) -> Int` for ordered types), R-L0-10 (`_with` accepts a comparator; L2-list adds `list.sort_with`, so a comparator building block is needed).
- (a) 6 languages have this (Rust `cmp`, Go `Compare` (F), Gleam `compare` (F), OCaml `compare` (F), Kotlin `compareTo` (F), JS `localeCompare` (F, locale-dependent and semantically different)). `compare` is the most common spelling at 3 languages; Almide's `bytes.cmp` is in the minority. Int is the majority return type (Go, OCaml, Kotlin, JS); Almide's `bytes.cmp` also returns Int.
- (b) Pure computation. Locale-independent, so native and wasm agree.
- (c) LLMs write comparators for descending or composite-key sorting. Requiring `(a, b) => if a < b then -1 else if a > b then 1 else 0` every time leads to sign mistakes.
- (d) Addition.
- Proposed gate: Verify `string.compare(a, b) < 0` ⇔ `a < b` on random string pairs across all three of native, wasm, and interp.
- issue: #3902

<a id="r-string-06-stringbyte_lens---int-を足す---採用"></a>

### R-string-06 Add `string.byte_len(s) -> Int` — ○ Accepted

- Content: Return the UTF-8 byte count without copying.
- Supporting L0 rules: R-L0-11 (length is spelled `len`), R-L0-02 (the subject is String). `byte_` uses the same prefix as existing `byte_slice`.
- (a) 6 languages have a dedicated spelling (Rust `len` (F), Go `len`, Gleam `byte_size` (F), Elixir `byte_size` (F), OCaml `length` (F), Swift `utf8.count`).
- (b) native uses `s.len()`; wasm simply reads the string's length field.
- (c) Determining the range for `string.byte_slice(s, a, b)` requires the byte count, but currently the only option is to copy the whole string with `bytes.from_string(s) |> bytes.len`.
- (d) Addition.
- Proposed gate: Property-test `string.byte_len(s) == bytes.len(bytes.from_string(s))`.
- issue: #3945

<a id="r-string-07-utf-16-との変換を足すlatin-1-は足さない---採用"></a>

### R-string-07 Add UTF-16 conversions (do not add Latin-1) — ○ Accepted

- Content:
  - `string.to_utf16(s: String) -> List[Int]` (UTF-16 code units 0..65535; no BOM)
  - `string.from_utf16(units: List[Int]) -> Result[String, String]` (unpaired surrogates and out-of-range values produce `err("invalid UTF-16 at index N")`)
  - Do not add Latin-1.
- Supporting L0 rules: R-L1-08 (add as string functions), R-L0-07, item 4 (align the pair's types: both `List[Int]`), R-L0-06, item 1 (failures with a reason use Result).
- (a) 8 languages have UTF-16 (L1). Latin-1 appears in 5 languages—Python, JS, Kotlin, Swift, Elixir—but all provide it as one option in a generic encoder taking an encoding name, not as a dedicated function. There is no reason to bring a general character-encoding table into Almide, so mark it ×.
- (b) Pure computation; identical on native and wasm. A sequence of UTF-16 units is not `Bytes` (units are 16 bits), so it belongs in `string` rather than `bytes`.
- (c) When handling JS, Windows APIs, or JSON `\uXXXX`, LLMs write code assuming the semantics of `encode_utf16` / `charCodeAt`.
- (d) Addition. Existing `from_codepoint` will also be aligned to express failure using `Option` under R-string-11.
- Proposed gate: Fixtures for the `string.from_utf16(string.to_utf16(s)) == ok(s)` round trip, surrogate pairs (such as U+1F980), and err for isolated surrogates; ensure native and wasm agree.
- issue: #3948

<a id="r-string-08-stringsplit_whitespace-を足す---不採用"></a>

### R-string-08 Add `string.split_whitespace` — × Rejected

- Reason: Only 4/9 have a dedicated function (Rust (F), Go `Fields` (F), Python `split()` (F), Elixir `split/1` (F)), short of L1's “5 or more languages”. Confirmed absent in Gleam, JS, and OCaml (F). It can be written as `regex.split("\\s+", string.trim(s))` (argument order follows the regex verdict).
- Instead, the R-string-09 hint guides LLMs that write this spelling to the correct expression.
- Supporting L0 rules: R-L0-00, L1 verdict criteria

<a id="r-string-09-他言語の綴りへのヒントを意味の合う書き方に直す---採用"></a>

### R-string-09 Correct hints for other-language spellings to semantically appropriate expressions — ○ Accepted

- Content: Add string rows to E002's “Did you mean” table (`ALIASES` in `crates/almide-frontend/src/stdlib.rs:156`).
  - `split_whitespace` / `fields` / `words` → `regex.split("\\s+", string.trim(s))`
  - `to_utf8` / `as_bytes` / `encode` → `bytes.from_string`, `decode` / `from_utf8` → `bytes.to_string`
  - `cmp` / `compare_to` / `locale_compare` → `string.compare`
  - `byte_size` / `byte_length` → `string.byte_len`
  - `pad_left` / `pad_right` / `rjust` / `ljust` → `string.pad_start` / `string.pad_end`
  - `strip` / `lstrip` / `rstrip` → `string.trim` / `trim_start` / `trim_end`
  - `remove_prefix` / `trim_prefix` → `string.strip_prefix`
- Supporting L0 rules: R-L0-01 (one spelling; guide through errors rather than adding aliases).
- (a) The spellings are those of the languages in the L2 table above.
- (b) Checker-only change.
- (c) Writing `string.split_whitespace` currently suggests `string.is_whitespace` (measured in c.almd). Candidates selected by name similarity can have the opposite meaning.
- (d) Addition (diagnostics only; no change to the language surface).
- Proposed gate: Add a test checking every table row: the key is undefined, and the destination is a live public function or a valid expression (the table still contains keys pointing to currently live functions. This is the same test as L2-map R-map-06).
- issue: #3905

<a id="r-string-10-指定文字の-trimsplitnコードポイントのリスト化を足す---不採用"></a>

### R-string-10 Add trimming of specified characters, `splitn`, and code-point listing — × Rejected

- Trimming specified characters (Python `strip(chars)`, Rust `trim_matches`, Go `Trim(s, cutset)`, Kotlin `trim(chars)`, Elixir `trim(s, to_trim)`, Swift `trimmingCharacters`): 6 languages have it, but semantics split among “character set” (Python, Go), “repeated substring” (Elixir), and “pattern” (Rust). Whichever is chosen, LLMs following the other languages' habits will be wrong. It can be expressed with `strip_prefix` / `strip_suffix` and `regex.replace`.
- `splitn`: Retain the decision in `stdlib/string.almd:24-25` (`split_once` handles splitting into two).
- Code-point listing (Gleam `to_utf_codepoints`, Elixir `to_charlist`, etc.): It is a one-liner with `string.chars(s) |> list.filter_map(string.codepoint)`, so it fails L1's third criterion (cannot be covered by current functions).
- Supporting L0 rules: R-L0-00, L1 verdict criteria

---

<a id="r-string-11-stringfrom_codepoint-を-optionstring-を返す形にする---採用"></a>

### R-string-11 Make `string.from_codepoint` return `Option[String]` — ○ Accepted

- Content: Change `string.from_codepoint(n: Int) -> String` to `string.from_codepoint(n: Int) -> Option[String]`. Invalid values (surrogates U+D800–U+DFFF, negative values, values above U+10FFFF) yield none. Currently they return `""`. Keep the name.
- Supporting L0 rules: R-L0-06, item 4 (fallible construction must not return sentinels; use `Option` when there is no reason), R-L0-14, item 4 (a change that necessarily makes old code a type error may keep its name if accompanied by a hint and `almide fix`).
- (a) Rust's `char::from_u32` returns `Option`, Gleam's `utf_codepoint` returns `Result`, and Swift's `Unicode.Scalar(_:)` is a failable initializer (nil). Python's `chr` and JS's `fromCodePoint` throw exceptions. No language returns an empty string.
- (b) Pure computation. Use identical boundary checks on native and wasm.
- (c) Invalid values currently become `""` and are concatenated into strings as is. Mistakes go unnoticed. With `Option`, extracting a value requires `??` or `?`.
- (d) Semantic change (return type `String` → `Option[String]`). Old code necessarily produces a type error where it uses the result as `String`, rather than silently returning a different result (R-L0-14, item 4). Do not add `@deprecated`; include it in epoch 15's `breaks`. `almide fix` rewrites `string.from_codepoint(n)` → `string.from_codepoint(n) ?? ""` (preserving current results), with a hint saying “prefer propagating failure with `?`”. Count usages in spec, tests, docs, and stdlib and report them in the PR.
- Proposed gate: R-L0-G's sentinel check (list public functions returning `String` or `Int` whose names start with `from_`, `parse`, or `decode`, and verify they do not return sentinels for boundary inputs; the same test as L2-bytes R-bytes-09). Add native and wasm fixtures for `from_codepoint(0xD800) == none` and `from_codepoint(0x1F980) == some("🦀")`.
- issue: #3946

<a id="r-string-15-stringslice-の終わりの既定値を外し2-引数の形は-drop-に一本化する---採用"></a>

### R-string-15 Remove `string.slice`'s default end and consolidate the two-argument form on `drop` — ○ Accepted

- Current state: `fn string.slice(s: String, start: Int, end: Int = 末尾) -> String` (末尾 means end) is the only standard function with a default argument. `string.slice(s, n)` returns the same value as `string.drop(s, n)`. `list.slice` and `bytes.slice` take only 3 arguments; calling them with 2 produces E004.
- Proposal: Remove the default for `end`, making it `fn string.slice(s: String, start: Int, end: Int) -> String`. Two-argument calls produce E004 with a hint showing `string.drop(s, n)`. Do not add defaults to list or bytes.
- Supporting L0 rules: R-L0-01 (two-argument `slice` is an alias for `drop`), R-L0-12 (the boundary matrix has the same shape for list, string, and bytes).
- (a) JS, Python, and Kotlin (`substring(a)`) have slices with optional ends, but Almide already has `drop`. Adding defaults to all 3 types would create 3 pairs of synonyms, `slice(n)` and `drop(n)`.
- (b) Changes only the checker and declarations.
- (c) Measured: `string.slice("abcdef", 2)` returns `cdef`, while `list.slice([1,2,3], 1)` produces E004. The two-argument form learned for string fails for list and bytes.
- (d) Semantic change (narrows the accepted forms). Old two-argument calls produce E004, not silently changed results, satisfying R-L0-14.4. Include it in epoch 15's `breaks`; `almide fix` rewrites `string.slice(s, n)` to `string.drop(s, n)`. Usage counts (2 arguments): spec 6 / docs 1 / stdlib 2 / research 4 / examples 1.
- Completeness rule: Types with `slice`, `take`, `drop`, `take_end`, and `drop_end` (list, string, bytes) have the same argument count for identically named functions. No standard public function has default arguments.
- Proposed gate: Add “identically named functions across the 3 types have the same argument count” to R-L0-12's boundary-matrix test, and verify that interface JSON contains 0 default arguments.
- issue: #3969

<a id="r-string-12-stringcodepoint-を-stringto_codepoint-に改名する---採用"></a>

### R-string-12 Rename `string.codepoint` to `string.to_codepoint` — ○ Accepted

- Current state: The reverse direction is `string.from_codepoint(n) -> String?`. The round-trip pair is `codepoint` and `from_codepoint`; writing `to_codepoint` produces E002.
- Proposal: `fn string.to_codepoint(s: String) -> Int?` (the first character's code point; none for an empty string). It forms a `to_`/`from_` pair with `from_codepoint` (like `float.to_bits`/`from_bits`).
- Supporting L0 rules: R-L0-25 (conversion-verb table: round-trip pairs use `to_<X>`/`from_<X>`).
- (a) Spellings vary—JS `codePointAt`/`fromCodePoint`, Python `ord`/`chr`, Rust `char as u32`/`char::from_u32`—with no majority. Decide based on Almide's internal pair.
- (d) 1 rename. `@deprecated(since=15, use="string.to_codepoint")`, `removal_epoch = 16`; rewrite with `almide fix`.
- issue: #3979

<a id="r-string-13-stringcount-を述語で数える形にする---不採用"></a>

### R-string-13 Make `string.count` count by predicate — × Rejected

- Reason: Most string “count” operations count substring occurrences (Rust `matches().count()`, Go `strings.Count`, Python `str.count`, Elixir `String.count`, …). string is not covered by the D3 family (higher-order functions for list, map, set, deque, heap, bytes). For predicate counting, write `string.chars(s) |> list.count(f)`.
- issue: None

<a id="r-string-14-to_codepoint-は-1-コードポイントちょうどのときだけ-some---採用r-string-12-の意味を決める"></a>

### R-string-14 `to_codepoint` returns some only for exactly 1 code point — ○ Accepted (defines R-string-12's semantics)

- Current state: R-string-12 specified “the first character's code point; none if empty”. `to_codepoint("ab")` returns 97, silently discarding the second character. The reverse `from_codepoint(n) -> String?` creates 1 character, so they are not a round-trip pair.
- Proposal: Return some only for strings of exactly 1 code point; otherwise (empty or 2 or more), return none. To read the first character, convert from `string.get(s, 0)`.
- (a) Python `ord("ab")` raises an exception; Rust's `char` is a single-character type. JS `codePointAt(0)` returns the first character, but is a different function that takes a position argument.
- (d) Semantic change (introduced in epoch 15 alongside the rename, so define this as the new name's semantics from the start).
- issue: #3991

<a id="r-string-16-string-に-sequencegrowable-の要件をそろえる---採用"></a>

### R-string-16 Give `string` the full `Sequence` and `Growable` requirements — ○ Accepted

- Current state: `string` belongs to `Text` (including `Ordered`, `Sequence`, `Container`) and `Growable`, but lacks `filter`, `fold`, `pop`, `insert`, and `remove_at`.
- Proposal: Add all 5. Elements are single-character (1-code-point) `String` values, and positions use code points (the same convention as R-string).
  - `filter(s, f: (String) -> Bool) -> String`, `fold(s, init: B, f: (B, String) -> B) -> B`
  - `pop(mut s) -> String?` (extract the last character), `insert(s, i: Int, sub: String) -> String`, `remove_at(s, i: Int) -> String` (as with `list`, abort outside `0..=len` and `0..len` respectively)
- (a) Swift's `String` is a `RangeReplaceableCollection` and has `filter` (returning `String`), `reduce`, `popLast`, `insert(_:at:)`, and `remove(at:)`. Kotlin also provides `filter` and `fold` on `String`.
- `new`, `from_list`, and `to_list` were decided in R-string-17.
- (d) 5 additions.
- issue: #4000

<a id="r-string-17-stringchars-を-to_list-にしnewfrom_list-を置く---採用r-string-16-の保留を解く"></a>

### R-string-17 Rename `string.chars` to `to_list` and add `new` and `from_list` — ○ Accepted (resolves R-string-16's pending decision)

- Content: Rename `string.chars(s)` to `string.to_list(s)`, and add `string.new() -> String` (`""`) and `string.from_list(xs: List[String]) -> String` (concatenation). `string` gains all `Container` requirements.
- Reason: In the protocol hierarchy, `string` is `Text` (including `Ordered`, `Sequence`, `Container`). Avoid making `to_list` and `chars` two spellings for the same role. Swift uses `Array(s)`; Kotlin uses `toList()`. For code based on familiarity with Rust's `chars`, E002 hints show `string.to_list`.
- (d) Rename and additions. Deprecate `chars` with `@deprecated(since=15, use="to_list")` in epoch 15 and remove it in epoch 16.
- issue: #4004

<a id="r-string-18-部分文字列を数える-stringcount-を-count_matches-にしcount-は-container-の述語の形にする---採用"></a>

### R-string-18 Rename substring-counting `string.count` to `count_matches`, and make `count` the `Container` predicate form — ○ Accepted

- Current state: `string.count(s, sub)` returns the number of substring occurrences. In the other 6 containers, `count(xs, f)` counts elements matching a predicate. The same name has different meanings. The first pass retained it because “different argument types make confusion a type error”, but under R-L0-36 a type error alone is not sufficient justification to retain it. In the protocol hierarchy, `string` is also a `Container`, so `count` cannot have two meanings simultaneously.
- Proposal: Rename the substring-counting function to `count_matches(s, needle) -> Int` and make it a `Text` requirement (also provided in `bytes`). Provide `string.count(s, f: (String) -> Bool) -> Int` as the `Container` predicate form.
- (a) Predicate-based `count` exists in 5 languages: Kotlin, Swift (`count(where:)`), Rust (`filter().count()`), Elixir, Gleam. Substring counting with `count` appears in 2 languages, Go and Python.
- (d) Rename and addition. In epoch 15 mark `string.count(s, sub)` (the string-argument form) as `@deprecated(use="count_matches")`, leaving only the predicate form in epoch 16. `almide fix` rewrites string-argument calls to `count_matches`.
- issue: #4012

<a id="r-string-19-定数時間の比較-stringis_equal_constant_time-を置く---採用"></a>

### R-string-19 Add constant-time comparison `string.is_equal_constant_time` — ○ Accepted

- Current state: Webhook signature headers and tokens arrive as `String`. Comparison with `==` returns at the first differing byte, allowing values to be inferred from timing.
- Proposal: `string.is_equal_constant_time(a: String, b: String) -> Bool`. Compare UTF-8 bytes. Return false immediately if byte lengths differ. This uses the same mechanism as `bytes.is_equal_constant_time` (R-bytes-29), a cell in the {Bytes, String} type matrix. Use `hash.is_valid_hmac_sha256` for HMAC verification (R-hash-06).
- Supporting L0 rules: R-L0-09, R-L0-30.
- (a) Python `secrets.compare_digest` (also accepts str), Erlang `crypto:hash_equals`.
- protocol (R-L0-37): Since `String` and `Bytes` have the same role, declare it only once as a `Text` requirement (the `Text` row in [protocol-view](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/protocol-view.md); module functions are the requirements themselves).
- Spelling (R-L0-00): No majority exists (Go `ConstantTimeCompare` and `hmac.Equal`, Python `compare_digest`, Node `timingSafeEqual`, Erlang `hash_equals`, Rust `subtle`'s `ct_eq`, Java `isEqual`). Do not use `compare`, which denotes `Ord`'s three values (R-L0-16); instead use Bool's `is_` (R-L0-09) and spell out “equal” and “constant time”.
- (d) Addition. Contract C-423. issue: #4106

<a id="l3-関数表"></a>

## L3: Function table

| Function | Signature (current, showing mut) | Main spellings in other languages | Verdict | Proposal |
|---|---|---|---|---|
| `trim` | `(s: String) -> String` | Rust/JS/Gleam/Kotlin `trim` / Python `strip` / Go `TrimSpace` | Retain | — |
| `split` | `(s: String, sep: String) -> List[String]` | 9/9 `split` family (Go `Split` ). `split("", ",")` is `[""]` (same as Rust ) | Retain | — |
| `split_once` | `(s: String, sep: String) -> Option[(String, String)]` | Rust/Gleam `split_once` / Go `Cut` / Python `partition` | Retain | — |
| `join` | `(list: List[String], sep: String) -> String` | Go/Gleam `Join` / Python `sep.join` / Rust / JS use the sequence side | Remove (→ `list.join` ) | R-string-01 |
| `len` | `(s: String) -> Int` | Python `len` / Gleam / Elixir `length` (graphemes )/ Swift `count` | Retain | — |
| `length` | `(s: String) -> Int` (deprecated ) | — | Remove (→ `string.len` ) | R-string-04 |
| `contains` | `(s: String, sub: String) -> Bool` | Rust/Gleam/Kotlin/Swift `contains` / JS `includes` | Retain | — |
| `starts_with` | `(s: String, prefix: String) -> Bool` | Rust/Gleam/OCaml `starts_with` / Go `HasPrefix` | Retain | — |
| `ends_with` | `(s: String, suffix: String) -> Bool` | Rust/Gleam/OCaml `ends_with` / Go `HasSuffix` | Retain | — |
| `slice` | `(s: String, start: Int, end: Int) -> String` | Gleam/Elixir/JS `slice` / Python `s[a:b]` | Semantic change (`end`: remove the default; 2 arguments use `drop` ) | R-string-15 |
| `byte_slice` | `(s: String, start: Int, end: Int) -> Option[String]` | Elixir `byte_slice` / Rust `get(a..b)` | Retain | — |
| `pad_start` | `(s: String, n: Int, ch: String) -> String` | JS/Gleam/Kotlin `padStart` / Python `rjust`. `ch` uses only 1 character—the first (measured: `pad_start("7",5,"ab")="aaaa7"`) | Retain | — |
| `to_bytes` | `(s: String) -> List[Int]` | Rust `as_bytes` / Kotlin `encodeToByteArray` / Gleam `bit_array.from_string` | Remove (→ `bytes.from_string` ) | R-string-02 |
| `capitalize` | `(s: String) -> String` | Python/Elixir/Gleam lowercase the rest; OCaml `capitalize_ascii` leaves the rest unchanged (F). No majority, so retain | Retain | — |
| `to_upper` | `(s: String) -> String` | Rust `to_uppercase` / Go `ToUpper` / Python `upper` / Gleam `uppercase` | Retain | — |
| `to_lower` | `(s: String) -> String` | Rust `to_lowercase` / Go `ToLower` / Python `lower` / Gleam `lowercase` | Retain | — |
| `replace` | `(s: String, from: String, to: String) -> String` | Rust/Python/Gleam `replace` / JS / Go `replaceAll`/`ReplaceAll` | Retain | — |
| `get` | `(s: String, i: Int) -> Option[String]` | JS `at` / Elixir `at` / Rust `chars().nth` | Retain | — |
| `lines` | `(s: String) -> List[String]` | Rust/Kotlin `lines` / Python `splitlines` / Go `Lines` | Retain | — |
| `chars` | `(s: String) -> List[String]` | Rust `chars` / Gleam `to_graphemes` / Elixir `graphemes` | Rename → `to_list` | R-string-17 |
| `index_of` | `(s: String, needle: String) -> Option[Int]` | Rust `find` / Go `Index` / JS `indexOf` / OCaml `index_opt` | Retain | — |
| `repeat` | `(s: String, n: Int) -> String` | Rust/JS/Gleam `repeat` / Go `Repeat` / Elixir `duplicate` | Retain | — |
| `from_bytes` | `(bytes: List[Int]) -> String` | Rust `String::from_utf8_lossy` / Kotlin `decodeToString` | Remove (→ `bytes.from_list(xs) \|> bytes.to_string_lossy` ) | R-string-02 |
| `is_digit` | `(s: String) -> Bool` | Python `isdigit` / Rust `is_ascii_digit`; false for empty | Retain | — |
| `is_alpha` | `(s: String) -> Bool` | Python `isalpha` / Rust `is_alphabetic`; false for empty | Retain | — |
| `is_alphanumeric` | `(s: String) -> Bool` | Python `isalnum` / Rust `is_alphanumeric`; false for empty | Retain | — |
| `is_whitespace` | `(s: String) -> Bool` | Python `isspace` (false for empty )/ Rust `is_whitespace`; Almide returns true for empty (as documented; usable for checking empty lines) | Retain | — |
| `is_upper` | `(s: String) -> Bool` | Python `isupper` (also has the same semantics of ignoring non-letters ) | Retain | — |
| `is_lower` | `(s: String) -> Bool` | Python `islower` | Retain | — |
| `codepoint` | `(s: String) -> Option[Int]` | Python `ord` / JS `codePointAt` / Gleam `utf_codepoint_to_int` | Rename → `to_codepoint` (none unless exactly 1 code point ) | R-string-12, R-string-14 |
| `from_codepoint` | `(n: Int) -> String` | Python `chr` / JS `fromCodePoint` / Rust `char::from_u32` (Option ); invalid values yield `""` (measured: U+D800 yields `""` ) | Semantic change (`Option[String]` returned ) | R-string-11 |
| `pad_end` | `(s: String, n: Int, ch: String) -> String` | JS/Gleam/Kotlin `padEnd` / Python `ljust`. `ch` uses a space if empty | Retain | — |
| `trim_start` | `(s: String) -> String` | Rust/JS/Gleam `trim_start` / Python `lstrip` | Retain | — |
| `trim_end` | `(s: String) -> String` | Rust/JS/Gleam `trim_end` / Python `rstrip` | Retain | — |
| `count` | `(s: String, sub: String) -> Int` | Python `count` / Go `Count` (len+1 for empty sub; Almide agrees (F) ). `list.count` (predicate) uses the same verb, but the different argument types make confusion a type error | Rename → `count_matches` | R-string-18 |
| `is_empty` | `(s: String) -> Bool` | Rust/Gleam/OCaml `is_empty` / Kotlin `isEmpty` | Retain | — |
| `reverse` | `(s: String) -> String` | Gleam/Elixir `reverse` (grapheme units )/ Rust `chars().rev()`; Almide uses code-point units | Retain | — |
| `strip_prefix` | `(s: String, prefix: String) -> Option[String]` | Rust `strip_prefix` (Option )/ Python `removeprefix` / Gleam `remove_prefix` | Retain | — |
| `strip_suffix` | `(s: String, suffix: String) -> Option[String]` | Rust `strip_suffix` / Python `removesuffix` | Retain | — |
| `replace_first` | `(s: String, from: String, to: String) -> String` | Kotlin `replaceFirst` / Rust `replacen(..,1)` / JS `replace` | Retain | — |
| `last_index_of` | `(s: String, needle: String) -> Option[Int]` | Rust `rfind` / Go `LastIndex` / JS `lastIndexOf` | Retain | — |
| `first` | `(s: String) -> Option[String]` | Gleam/Elixir/Kotlin `first` | Retain | — |
| `last` | `(s: String) -> Option[String]` | Gleam/Elixir/Kotlin `last` | Retain | — |
| `take` | `(s: String, n: Int) -> String` | Kotlin `take` / Swift `prefix` | Retain | — |
| `take_end` | `(s: String, n: Int) -> String` | Kotlin `takeLast` / Swift `suffix` | Retain | — |
| `drop` | `(s: String, n: Int) -> String` | Kotlin `drop` / Gleam `drop_start` | Retain | — |
| `drop_end` | `(s: String, n: Int) -> String` | Kotlin `dropLast` / Gleam `drop_end` | Retain | — |
| `run_length_encode` | `(s: String) -> List[(String, Int)]` | 0/9 | Remove (→ `string.chars` and `list.fold` ) | R-string-03 |
| `push` | `(mut s: String, suffix: String) -> Unit` | Rust `push_str` / Swift `append` | Retain | — |
| `clear` | `(mut s: String) -> Unit` | Rust `clear` / Swift `removeAll` | Retain | — |
| `compare` (Addition ) | `(a: String, b: String) -> Int` | Go `Compare` / Gleam / OCaml `compare` / Rust `cmp` / Kotlin `compareTo` | Addition | R-string-05 (R-L0-16 ) |
| `byte_len` (Addition ) | `(s: String) -> Int` | Rust `len` / Go `len` / Gleam / Elixir `byte_size` / Swift `utf8.count` | Addition | R-string-06 |
| `to_utf16` (Addition ) | `(s: String) -> List[Int]` | Rust `encode_utf16` / JS `charCodeAt` / Swift `utf16` | Addition | R-string-07 |
| `from_utf16` (Addition ) | `(units: List[Int]) -> Result[String, String]` | Rust `String::from_utf16` (Result )/ JS `fromCharCode` | Addition | R-string-07 |
| `filter` |  (Addition )`(s: String, f: (String) -> Bool) -> String` | Swift `filter` / Kotlin `filter` | Addition | R-string-16 |
| `fold` |  (Addition )`(s: String, init: B, f: (B, String) -> B) -> B` | Swift `reduce` / Kotlin `fold` | Addition | R-string-16 |
| `pop` |  (Addition )`(mut s: String) -> String?` | Swift `popLast` / Rust `pop` | Addition | R-string-16 |
| `insert` |  (Addition )`(s: String, i: Int, sub: String) -> String` | Swift `insert(contentsOf:at:)` / Rust `insert_str` | Addition | R-string-16 |
| `remove_at` |  (Addition )`(s: String, i: Int) -> String` | Swift `remove(at:)` / Rust `remove` | Addition | R-string-16 |
| `new` |  (Addition )`() -> String` | Swift `String()` / Rust `String::new` | Addition | R-string-17 |
| `from_list` |  (Addition )`(xs: List[String]) -> String` | Swift `String(chars)` / Kotlin `joinToString("")` | Addition | R-string-17 |
| `enumerate` |  (Addition )`string.enumerate(s: String) -> List[(Int, String)]` | Swift `enumerated` / Rust `enumerate` | Addition | R-L0-42 |
| `zip` |  (Addition )`string.zip(s: String, ys: List[B]) -> List[(String, B)]` | Swift `zip` / Rust `zip` | Addition | R-L0-42 |
| `zip_with` |  (Addition )`string.zip_with(s: String, ys: List[B], f: (String, B) -> C) -> List[C]` | Kotlin `zip(other, f)` | Addition | R-L0-42 |
| `filter_map` |  (Addition )`string.filter_map(s: String, f: (String) -> B?) -> List[B]` | Swift `compactMap` / Rust `filter_map` | Addition | R-L0-42 |
| `find_map` |  (Addition )`string.find_map(s: String, f: (String) -> B?) -> B?` | Rust `find_map` | Addition | R-L0-42 |
| `reduce` |  (Addition )`string.reduce(s: String, f: (String, String) -> String) -> String?` | Swift `reduce` / Rust `reduce` | Addition | R-L0-42 |
| `scan` |  (Addition )`string.scan(s: String, init: B, f: (B, String) -> B) -> List[B]` | Rust `scan` / Kotlin `runningFold` | Addition | R-L0-42 |
| `partition` |  (Addition )`string.partition(s: String, f: (String) -> Bool) -> (List[String], List[String])` | Kotlin `partition` | Addition | R-L0-42 |
| `group_by` |  (Addition )`string.group_by(s: String, f: (String) -> B) -> Map[B, List[String]]` | Kotlin `groupBy` / Swift `Dictionary(grouping:)` | Addition | R-L0-42 |
| `sort` |  (Addition )`string.sort(s: String) -> List[String]` | Swift `sorted()` | Addition | R-L0-42 |
| `sort_by` |  (Addition )`string.sort_by(s: String, f: (String) -> B) -> List[String]` | Kotlin `sortedBy` | Addition | R-L0-42 |
| `sort_with` |  (Addition )`string.sort_with(s: String, cmp: (String, String) -> Int) -> List[String]` | Swift `sorted(by:)` | Addition | R-L0-42 |
| `min` |  (Addition )`string.min(s: String) -> String?` | Swift `min()` / Rust `min` | Addition | R-L0-42 |
| `max` |  (Addition )`string.max(s: String) -> String?` | Swift `max()` / Rust `max` | Addition | R-L0-42 |
| `min_by` |  (Addition )`string.min_by(s: String, f: (String) -> K) -> String?` | Kotlin `minByOrNull` | Addition | R-L0-42 |
| `max_by` |  (Addition )`string.max_by(s: String, f: (String) -> K) -> String?` | Kotlin `maxByOrNull` | Addition | R-L0-42 |
| `min_with` |  (Addition )`string.min_with(s: String, cmp: (String, String) -> Int) -> String?` | Swift `min(by:)` | Addition | R-L0-42 |
| `max_with` |  (Addition )`string.max_with(s: String, cmp: (String, String) -> Int) -> String?` | Swift `max(by:)` | Addition | R-L0-42 |
| `find_index` |  (Addition )`string.find_index(s: String, f: (String) -> Bool) -> Int?` | Swift `firstIndex(where:)` | Addition | R-L0-42 |
| `take_while` |  (Addition )`string.take_while(s: String, f: (String) -> Bool) -> String` | Swift `prefix(while:)` | Addition | R-L0-42 |
| `drop_while` |  (Addition )`string.drop_while(s: String, f: (String) -> Bool) -> String` | Swift `drop(while:)` | Addition | R-L0-42 |
| `chunk` |  (Addition )`string.chunk(s: String, n: Int) -> List[String]` | Kotlin `chunked` | Addition | R-L0-42 |
| `window` |  (Addition )`string.window(s: String, n: Int) -> List[String]` | Kotlin `windowed` | Addition | R-L0-42 |
| `split_at` |  (Addition )`string.split_at(s: String, n: Int) -> (String, String)` | Rust `split_at` | Addition | R-L0-42 |
| `dedup` |  (Addition )`string.dedup(s: String) -> String` | Rust `dedup` | Addition | R-L0-42 |
| `intersperse` |  (Addition )`string.intersperse(s: String, sep: String) -> String` | Rust `intersperse` | Addition | R-L0-42 |
| `unique` |  (Addition )`string.unique(s: String) -> String` | Kotlin `distinct` | Addition | R-L0-42 |
| `unique_by` |  (Addition )`string.unique_by(s: String, f: (String) -> K) -> String` | Kotlin `distinctBy` | Addition | R-L0-42 |
| `count` |  (Addition )`(s: String, f: (String) -> Bool) -> Int` | Kotlin `count {}` / Swift `count(where:)` | Addition | R-string-18, R-L0-42 |
| `is_equal_constant_time` | `string.is_equal_constant_time(a: String, b: String) -> Bool`(proposal) | Python `secrets.compare_digest` / Erlang `crypto:hash_equals` | Addition | R-string-19 |

Current 50 functions + 4 additions = 54 rows.

---

<a id="l0-への差し戻し処理済み"></a>

## Referrals back to L0 (resolved)

1. **Comparator spelling and type.** L0 now includes R-L0-16 (provide `<mod>.compare(a, b) -> Int` for ordered types). R-string-05 adds `string.compare`. Rename `bytes.cmp` to `bytes.compare` (L2-bytes R-bytes-08).
2. **How constructors express failure.** L0 now includes R-L0-06, item 4 (fallible construction and conversion must not return sentinels), and R-L0-14, item 4 permits “same-name changes that make old code a type error”. Make `string.from_codepoint` return `Option[String]` without renaming it (R-string-11).
