<a id="als-t--text-and-number-semantics-normative"></a>

# ALS §T — Text and Number Semantics (normative)

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](text-and-numbers.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](text-and-numbers.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](text-and-numbers.zh-CN.md)

> Non-normative reader snapshot of the [pinned original](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/text-and-numbers.md). Normative status and requirements below belong to the original, not this copy. No automatic synchronization; historical implementation, conformance and safety claims have not been independently revalidated for this translation. Code and rule identifiers are preserved.

> Last updated: 2026-10-26

> **Status**: normative. These sections are **requirements independent of implementation**; both v0 (native) and v1 (MIR/wasm) must conform. Evidence of conformance is supplied by corresponding fixtures in `spec/wasm_cross/` (three observations: stdout, stderr, exit code). **Breaking the oracle cycle** (flight-evidence-gaps F1): before these sections were established, “correctness” of these behaviors was the v0 implementation itself. Thereafter, v0 is also merely an implementation of these sections.

<a id="als-t1-stringtrim"></a>

## ALS-T1 `string.trim`

`string.trim(s)` removes the longest contiguous sequences of **code points with the Unicode `White_Space` property** from the beginning and end of s. The authority is Unicode White_Space (PropList.txt), comprising these 25 code points as of 2026: U+0009–U+000D, U+0020, U+0085, U+00A0, U+1680, U+2000–U+200A, U+2028, U+2029, U+202F, U+205F, U+3000.

**Decision**: an ASCII-only fast check (U+0009–U+000D, U+0020) is nonconforming. If a Unicode version update changes White_Space, this section follows it and implementations follow this section.
Fixture: `spec/wasm_cross/string_whitespace.almd`.

<a id="als-t2-floatparse"></a>

## ALS-T2 `float.parse`

Accepted grammar (**finite values only**; spellings `inf` / `infinity` / `nan` are never accepted, regardless of case or sign, C-413):

```ebnf
float   := ws* sign? number ws*
number  := digits ("." digits?)? exponent? | "." digits exponent?
exponent:= ("e"|"E") sign? digits
ws      := U+0009 | U+000A | U+000C | U+000D | U+0020
```

`ws` consists of **only five ASCII characters** (C-413), not ALS-T1's Unicode White_Space set, and excludes U+000B (VT). All other whitespace violates the grammar.

**Value requirement**: for accepted decimal notation, return the **exact nearest IEEE-754 binary64 value under round-half-to-even**. This is independent of digit count or exponent magnitude, including the smallest denormal 4.9e-324, maximum 1.7976931348623157e308 and significands over 19 digits. Notation rounding to ±inf (overflow) is an error; underflow gives ±0 with sign preserved (`-0.0` returns negative zero). Results are always finite.

**Error wording (normative)**: empty input: `cannot parse float from empty string`; grammar violation, including nonfinite spellings: `invalid float literal`; overflow: `float literal out of range`. Exit code follows ordinary error propagation.
Fixtures: `spec/wasm_cross/float_parse.almd`, `float_parse_finite_only.almd`, `number_parse_ascii_trim.almd`.

<a id="als-t3-jsonparse"></a>

## ALS-T3 `json.parse`

This is an explanatory summary of adopted C-391, C-400, C-401 and C-413. Accepted grammar follows RFC 8259 and the I-JSON constraints recorded in those contracts.

- Numbers follow ALS-T2's value requirement. Floating-point overflow is `err("float literal out of range")` (C-413), never ±inf.
- Valid surrogate pairs are combined. Unpaired surrogates, invalid escapes, raw control characters, noncharacters and unterminated strings are rejected (C-400).
- Accept **exactly one value** surrounded by JSON whitespace (space / tab / LF / CR). Reject trailing data, duplicate keys, unquoted keys, trailing commas, missing separators, non-JSON whitespace and invalid numbers (C-401).
- Positioned errors use character positions (code-point indices). See C-391 for the container-nesting limit.

The old 0.59.1 descriptions of prefix parsing / dropping unpaired surrogates were superseded by C-400/C-401; overflow yielding ±inf was superseded by C-413.
Tests: `spec/wasm_cross/json_parse_strict_strings.almd`,
`spec/wasm_cross/json_parse_strict_structure.almd`,
`spec/wasm_cross/json_parse_lenient_edges.almd`, `json_string_span.almd`.
Contracts: C-391, C-400, C-401, C-413.

<a id="als-t4-listchunk--listwindow"></a>

## ALS-T4 `list.chunk` / `list.window`

**Decision**: nonpositive size arguments have the following normative behavior:

- `n <= 0`: **domain error**. A negative size is read as 0 under ALS-T16's negative-count rule (almide/almide#4147), so `n < 0`, like `n == 0`, follows ALS-T6's termination convention: `chunk` writes one stderr line `Error: chunk size must be positive`; `window` writes `Error: window size must be positive`; both stop with exit code 1. Raw Rust panic (exit 101), wasm trap (exit 134), or silently returning empty/all windows is nonconforming.

> Note: the previous `n < 0` ruling (`chunk` returns the whole collection as one chunk, `window` returns empty) promoted a Rust v0 implementation detail, usize reinterpretation in `chunks(n as usize)`, into a requirement. It was withdrawn to align with ALS-T16's negative-count-as-zero rule (#4147). `spec/wasm_cross/list_chunk_negative.almd` fixes the negative-size abort. `n == 0` was undefined even under that reinterpretation (Rust panics), and was standardized as a T6-form abort in v0.28.4. Before that, native raw-panicked while wasm `window(xs, 0)` **silently returned len+1 empty windows** (silent-wrong).
> Fixtures: `spec/wasm_cross/list_chunk_windows.almd` (value cases), `list_chunk_zero.almd` / `list_windows_zero.almd` (abort cases). Contracts: C-129.

<a id="als-t5-stringto_upper--stringto_lower"></a>

## ALS-T5 `string.to_upper` / `string.to_lower`

**The base requirement is Unicode full case mapping** (simple mappings in UnicodeData.txt + 1:N mappings in SpecialCasing.txt, e.g. ß→SS). `to_lower` applies the **Final_Sigma context rule** (Unicode §3.13: word-final Σ→ς). Locale-dependent rules, such as Turkish İ/ı, **do not apply** (locale-independent decision).

**No folding into ASCII** (C-412): ASCII scalars use ASCII rules. Non-ASCII scalars use the mappings above, but if a mapped result contains any ASCII scalar, **leave the original scalar unchanged**. This intentionally deviates from Unicode, whose mappings include U+0131→I, U+212A→k and U+017F→S. In current Unicode, the preserved scalars are U+00DF, U+0131, U+0149, U+017F, U+01F0, U+1E96–U+1E9A, U+FB00–U+FB06 for `to_upper`, and U+0130, U+212A for `to_lower`. Final_Sigma context is determined from the original string (preserved U+212A still counts as cased). The initial character of `capitalize` follows the same rule. This mapping is locale-independent and is not used to compare identifier identity.

Implementations must follow Unicode version updates. The current generated tables come from `scripts/gen-case-tables.py`; regardless of the generating implementation, conformance is judged against this section and fixtures `spec/wasm_cross/string_case_unicode.almd`, `string_case_no_ascii_fold.almd`.

<a id="als-t6-整数演算の終了規約termination-convention"></a>

## ALS-T6 Integer-operation termination convention

Integer `/` and `%` are **total**: a zero divisor writes one stderr line `Error: division by zero`; signed minimum ÷ −1 (the true MIN for each bit width) writes `Error: integer overflow`; both **stop with exit code 1**. Hardware traps (wasm unreachable, exit 134, etc.) and silent wrapping are nonconforming. The same convention applies to negative exponents in `int.pow` (`Error: negative exponent`), out-of-range list indexing (`Error: index out of bounds`), and invalid ranges for `int.clamp`/`float.clamp` (lo > hi, and NaN bounds for float: `Error: clamp requires min <= max`).
Fixtures: `spec/wasm_cross/int_div_by_zero*.almd`, `int_mod_*`, `int8_div_overflow.almd`, `int_pow_negative_exponent.almd`, `index_bounds.almd`.

Call-stack exhaustion follows the same termination form: native and the embedded wasm host (`almide run --target wasm`) write one stderr line `Error: stack overflow` and **stop with exit code 1**, without losing stdout already written. Rust's `thread 'main' has overflowed its stack` (SIGABRT, exit 134) and `Error: wasm trap: call stack exhausted` are nonconforming. Exhaustion depth is not unified; C-196 declares it as a target-specific structural resource. Native uses the exhausted thread's own stack (main's platform-default 8 MiB; Rust-default 2 MiB for `fan` arms and parallel-worker threads), while the embedded host uses an 8 MiB wasm stack. Exhaustion on stock runtimes (p1 modules/components run by plain `wasmtime run`, and browsers) is the host's own trap (exit 134 in wasmtime), a declared residual of this convention (C-196).
Fixtures: `spec/wasm_fail/stack_overflow_abort.almd` (abort), `spec/wasm_cross/recursion_depth_within_limits.almd` (recursion within both targets' limits is byte-identical).

<a id="als-t7-トップレベル-let-の評価時機"></a>

## ALS-T7 Evaluation time of top-level let

Module top-level `let` initializers are **evaluated in declaration order at program startup, before main**. An initializer that can abort (for example, using ALS-T6 operations) aborts at startup even if its binding is never used. Initializers may refer to earlier top-level bindings (declaration-order dependencies).
Fixtures: `spec/wasm_cross/top_let_div_eager.almd`, `top_let_div_used.almd`.

<a id="als-t8-整数パースの文法とエラー規範"></a>

## ALS-T8 Integer parsing grammar and errors

`int.parse(s)` first strips leading/trailing **ASCII whitespace** (U+0009, U+000A, U+000C, U+000D, U+0020: ALS-T2's `ws`, excluding VT and non-ASCII whitespace, C-413), so `"\u{00A0}99\u{3000}"` produces `invalid digit found in string`, then reads the remainder as `sign? digit+` (decimal, no underscores). Error messages are byte-identical to Rust `ParseIntError` Display: empty after stripping: `cannot parse integer from empty string`; invalid character: `invalid digit found in string`; out of range: `number too large to fit in target type` / `number too small to fit in target type`.

```almide
test "int.parse trims ASCII whitespace only" {
  assert_eq(int.parse("\u{00A0}99\u{3000}") ?? -1, -1)
  assert_eq(int.parse("\t 99\r\n") ?? -1, 99)
}
```

Fixtures: `spec/wasm_cross/string_whitespace.almd`, `spec/wasm_cross/number_parse_ascii_trim.almd`.
Contracts: C-028, C-029.

<a id="als-t9-固定小数表示"></a>

## ALS-T9 Fixed-decimal display

`float.to_fixed(x, n)` uses **round-half-to-even on the exact binary value** (banker's rounding). Re-rounding through a decimal string or using half-up is nonconforming. The same rule applies for n=0 without a decimal point, negative numbers and boundary values such as exactly 0.5. The domain of n is **0..=4096**; outside it, stop in T6 form (`Error: to_fixed requires decimals in 0..=4096`, exit 1). Nonfinite values return their display forms unchanged: `inf`, `-inf`, `NaN`. The sign of a zero amount is preserved (`to_fixed(-0.4, 0)` is `-0`).
Fixtures: `spec/wasm_cross/to_fixed_domain_abort.almd`, `spec/wasm_cross/to_fixed_wide_precision.almd`.
Contracts: C-025.

<a id="als-t10-数学関数の決定性"></a>

## ALS-T10 Determinism of math functions

Transcendental functions such as `math.sin/cos/tan/exp/log/pow` are **byte-identical across all targets** (both targets share vendored libm; reliance on host libm is nonconforming). ALS-T22 specifies distance from the true value (error bounds).
Contracts: C-026.

<a id="als-t11-バイナリテキスト符号化"></a>

## ALS-T11 Binary-text encodings

`base64.encode/decode` (standard + URL-safe) and `hex.encode/decode` follow RFC 4648. Decode errors have identical wording on both targets (base64 has no positions; positional errors are on the hex side, `invalid hex char at 0`, C-030), including case handling, padding rules and invalid-length detection.

base64 encoding emits canonical form with `=`. `base64.decode` (`+` `/`) and `base64.decode_url` (`-` `_`) accept **only the canonical form of their own alphabet**. One byte sequence has one spelling, without the malleability described in RFC 4648 §12. Checks run in this order and return the err for the first violated rule:

| Rule | err |
|------|-----|
| UTF-8 byte length is a multiple of 4 | `invalid base64 length: <バイト長>` (byte length) |
| At most two trailing `=` | `invalid base64 padding` |
| All remaining bytes belong to that function's alphabet (interior `=` and positions 62/63 of the other alphabet are forbidden) | `invalid base64 character` |
| Bits dropped by `=` are zero (low 2 bits of the last character for one `=`, low 4 bits for two) | `invalid base64 trailing bits` |

`QQ==` gives `A`; `QR==` gives `invalid base64 trailing bits`; `QQ` gives `invalid base64 length: 2`; `QUJD====` gives `invalid base64 padding`; `decode("-_8=")` and `decode_url("+/8=")` give `invalid base64 character`.
Tests: `spec/wasm_cross/encoding_base64.almd`, `spec/wasm_cross/base64_decode_canonical.almd`, `spec/stdlib/encoding_test.almd`.
Contracts: C-027, C-030, C-411.

<a id="als-t12-非-abort-整数除算の一致"></a>

## ALS-T12 Agreement of non-aborting integer division

Non-aborting `/` and `%` are byte-identical to Rust `i64` truncating division/remainder, including rounding direction and remainder sign for negative numbers: `-7 / 2 == -3`, `-7 % 2 == -1`.
Contracts: C-003.

<a id="als-t13-浮動小数の文字列化"></a>

## ALS-T13 Floating-point stringification

`float.to_string` uses the **shortest round-tripping decimal representation** (Dragon4/Ryū-equivalent): `parse(to_string(x)) == x`, with the fewest digits satisfying that condition. Integral values retain `.0`, unlike Display in ALS-R2.
Contracts: C-023.

<a id="als-t14-64-bit-レジスタと-wrap-のマスク飽和"></a>

## ALS-T14 64-bit registers and saturation of wrap masks

`int.wrap_add` / `wrap_sub` / `wrap_mul` / `rotate_left` / `rotate_right` have no width argument; they wrap or rotate the 64-bit `Int` itself as the register (R-int-12). Rotation counts are Euclidean-reduced modulo 64 (negative counts rotate oppositely). For an N-bit register, compose with `int.wrap(x, bits)` (`int.wrap(int.wrap_add(a, b), bits)`). For `int.wrap`, `bits ≥ 64` **saturates** the mask to `u64::MAX`, returning `x` unchanged (not modulo).
Contracts: C-048.

<a id="als-t15-符号と-minmax-の-nan-規則"></a>

## ALS-T15 Signs and NaN rules for min/max

`float.sign` is `f64::signum` (NaN → NaN, ±0 → ±1). `float.min/max` **ignore NaN** (if one operand is NaN, return the other: the IEEE-754 minNum/maxNum family). `float.round` preserves the sign of a zero result (round(-0.0) = -0.0; half away from zero is unchanged). ALS-T23 specifies min/max ordering between ±0 (IEEE-754-2019 minimum/maximum).
Contracts: C-049, C-140.

<a id="als-t16-個数添字の-i64-クランプ"></a>

## ALS-T16 i64 clamping of counts and indices

List / String APIs taking **counts or indices** clamp over the full i64 domain **before narrowing** to an internal width. Wrong accesses caused by wrapping or sign changes are nonconforming. Clamping direction depends on API kind. **Negative counts are read as 0** (almide/almide#4147: previously unsigned reinterpretation made `take(xs, -1)` return everything, allowing `?limit=-1` to remove a count limit). Fixtures `spec/wasm_cross/negative_count_rule.almd`, `string_count_truncation.almd`, `list_count_index_truncation.almd` pin the rules:

| Kind | Rule | Negative/huge results |
|------|------|--------------------|
| Counts (`list.take/drop/take_end/drop_end/split_at`, `string.take/drop/take_end/drop_end/split_at`, `bytes.take/drop/take_end/drop_end/split_at`, widths of `pad_start/pad_end`) and range ends (start/end of `list.slice`, `bytes.slice`, `matrix.slice_rows`) | **Signed** `max(n, 0).min(len)` | Negative means 0: `take(-1)` = empty, `drop(-1)` = whole, `slice(xs, -2, 3)` = `slice(xs, 0, 3)`. `2^32` and above saturate to `len`, not wrap to a small value |
| Sizes (`list.chunk/window`, `string.chunk/window`, `bytes.chunk/window`) | Domain starts at 1 (ALS-T4) | 0 or below, including negatives, aborts |
| Repeat counts (`string.repeat` / `list.repeat` / `bytes.repeat`) | **Signed**, negative means 0 | `repeat(s, -1)` = empty on both targets; panic / trap is nonconforming |
| start/end of `string.slice` | **Signed** `max(0).min(len)` | Negative start is 0 |
| Indices (`list.get/get_or/set/insert/remove_at/swap/update`) | **Unsigned** `i as usize` | Negative indices and indices `2^32` or above take the out-of-range no-op / default / append path |

`list.product`, like `list.sum`, wraps i64 (overflow modulo 2^64).

```almide
test "a negative count is 0: take is empty, drop keeps everything, repeat is empty" {
  assert_eq(string.take("abcde", -1), "")
  assert_eq(string.drop("abcde", -1), "abcde")
  assert_eq(string.repeat("xy", -1), "")
  assert_eq(list.len(list.take([1, 2, 3], -1)), 0)
  assert_eq(list.slice([1, 2, 3], -2, 2), [1, 2])
}
```

Fixtures: `spec/wasm_cross/negative_count_rule.almd`, `spec/wasm_cross/string_count_truncation.almd`, `spec/wasm_cross/list_count_index_truncation.almd`. Contracts: C-054, C-056.

<a id="als-t17-datetimeformat-の指定子置換"></a>

## ALS-T17 Specifier replacement in datetime.format

`datetime.format(ts, pattern)` **sequentially replaces** strftime-style specifiers `%Y %m %d %H %M %S` with zero-padded calendar fields (year 4 digits, others 2). Native / v0-wasm / self-hosted backends run the same sequential `string.replace` series, producing identical bytes. `%` is special only immediately before these specifiers; there is no `%%` escape (unrecognized `%X` passes through). `%Y` has the same **variable width** as `to_iso`: at least 4 digits, zero-padded, with a negative year's sign occupying one position (`-292277022657`, `0000`). A fixed four-position field used to write only the low four digits for overflowing years and non-ASCII bytes for negative years. Contracts: C-128, C-359.

<a id="als-t18-assert-の-abort-形非-test-位置"></a>

## ALS-T18 Assert abort form outside tests

Failure of the `assert` family outside a `test` block stops with a **structured stderr block + exit code 1** (the T6 termination family). Raw Rust panic (exit 101), wasm trap (exit 134) and output lacking value information are nonconforming. The block consists of one `  key: value` per line. Values use ALS-R2 Display with C-153/C-418 escaping of control and invisible characters, never embedding raw newlines:

- `assert_eq(l, r)` → `Error: assertion failed\n  at: line <N>\n  expected: <r>\n  found: <l>`
- `assert_ne(l, r)` → `Error: assertion failed\n  at: line <N>\n  expected: != <l>\n  found: <l>`
- `assert(c)` → `Error: assertion failed\n  at: line <N>`
- `assert(c, msg)` → `Error: assertion failed: <msg>\n  at: line <N>`

Field order is fixed: **unterminated `found` is always last**. Newlines inside values display as escapes such as `\n` (C-153/C-418). The `at:` line is omitted only when the call lacks a span; this property comes from shared frontend lowering and always agrees across targets.
Operands are **evaluated exactly once**; failure messages refer back to bound temporaries. Inside `test` blocks, the harness reporting format applies, outside this section's scope. Implementation uses a single frontend desugaring, inherited as the same IR by native / v0-wasm / v1-wasm / interp.
Fixtures: `spec/wasm_cross/assert_abort_eq.almd`, `assert_abort_ne.almd`, `assert_abort_msg.almd`, `assert_abort_multiline.almd`. Contracts: C-153.

Assertions from `testing`, when directly called outside `test` blocks, stop with the same block (heading `Error: assertion failed`, order `at:` → `expected:` → `found:`) and exit code 1. `expected:` records the relation; `found:` records the operand:

- `testing.assert_gt(a, b)` → `expected: > <b>`, `found: <a>`
- `testing.assert_lt(a, b)` → `expected: < <b>`, `found: <a>`
- `testing.assert_approx(a, b, t)` → `expected: within <t> of <b>`, `found: <a>` (passes only for |a − b| < t; NaN fails)
- `testing.assert_contains(h, n)` → `expected: contains <n>`, `found: <h>` (empty `n` passes)
- `testing.assert_some(o)` → `expected: some(_)`, `found: none`
- `testing.assert_none(o)` → `expected: none`, `found: <o>`
- `testing.assert_ok(r)` → `expected: ok(_)`, `found: <r>`
- `testing.assert_err(r)` → `expected: err(_)`, `found: <r>`

Calls passed as function values are not direct calls and are outside this section.
Fixtures: `spec/wasm_cross/testing_assert_abort_gt.almd`, `_lt`, `_approx`, `_contains`, `_some`, `_none`, `_ok`, `_err`. Contracts: C-381.

<a id="als-t19-数値決定性ファミリー"></a>

## ALS-T19 Numeric determinism family

Observable results of Float computations (stdout, stderr, exit code, and `float.to_bits` values) are **a function only of the program and input**, independent of target, host CPU, OS, build flags or the execution environment's floating-point mode. This sentence is the numeric-determinism requirement. Its components are the existing T2 (`float.parse` correct rounding), T9 (`to_fixed`), T10 (single transcendental implementation), T13 (shortest round-trip display), T15 (signs and ignoring NaN), ALS-C9 (totalOrder), ALS-E3 (`-0.0` display), ALS-C3 (equality), ALS-R4 (nonfinite-constant display), C-210 (canonical NaN observation); and the newly introduced family T20 (rounding and no contraction), T21 (subnormal preservation), T22 (transcendental error bounds), T23 (signed zero), T24 (Float → Int conversion).

**NaN bit observations** are canonicalized at C-210 boundaries. `float.to_bits` returns `0x7FF8000000000000`; bytes f64 / f32 writers write `0x7FF8000000000000` / `0x7FC00000` respectively. This does not change NaN equality rules (ALS-C3). Display and encoding follow their respective sections.

> Implementation note (non-normative): native `Set[Float]` uses PartialEq comparison in `runtime/rs/src/set.rs`. C-210 bit canonicalization alone does not imply a key-identity guarantee. See ALS-C3 / C-292 for equality.

```almide
test "a Float result is a function of the program alone" {
  let x = float.parse("0.1") ?? 0.0
  assert_eq(float.to_bits(x + 0.2), 4599075939470750516)
  assert_eq(float.to_string(x + 0.2), "0.30000000000000004")
}
```

Fixtures for this family — `spec/wasm_cross/float_no_contraction.almd`, `float_subnormal_preserved.almd`, `math_transcendental_bits.almd`, `float_signed_zero_minmax.almd`, `float_to_int_edges.almd` — all additionally cite C-302. Contracts: C-302.

<a id="als-t20-丸めと縮約の禁止"></a>

## ALS-T20 Rounding and prohibition of contraction

Each Float operation (`+` `-` `*` `/`, `math.sqrt`, type conversion) rounds **after every operation** using IEEE-754 binary64 **round-to-nearest, ties-to-even**. Other rounding modes are unobservable. **Contraction is nonconforming**: `a * b + c` rounds twice, after multiplication and addition, and must not be replaced by one-rounding fused multiply-add (FMA). wasm has no fused instruction, and the native backend must not generate one either. Fused operations do not exist in this language; if needed, they would be specified separately as named functions, not an implicit mode.

Example: for `a = 1 + 2^-52`, `b = 1 - 2^-53`, the exact `a * b` is `1 + 2^-53 - 2^-105`, rounding to `1.0` in binary64. Thus `a * b - 1.0 = 0.0`. Contraction would leave `2^-53 - 2^-105 ≈ 1.1e-16`, also breaking agreement between targets.

```almide
test "a*b+c rounds twice — no fused multiply-add" {
  let a = float.parse("1.0000000000000002") ?? 0.0
  let b = float.parse("0.9999999999999999") ?? 0.0
  assert_eq(a * b - 1.0, 0.0)
  assert_eq(a * b + (0.0 - 1.0), 0.0)
}
```

Fixtures: `spec/wasm_cross/float_no_contraction.almd`, `spec/stdlib/float_determinism_test.almd`. Contracts: C-303.

<a id="als-t21-非正規数の保存"></a>

## ALS-T21 Preservation of subnormals

binary64 subnormals (`|x| < 2^-1022`, minimum `2^-1074 ≈ 4.9e-324`) are **preserved as values** on every target and host. Flush-to-zero and denormals-are-zero, on either input or output, are nonconforming: `2.2250738585072014e-308 / 2.0` is `2^-1023`, not 0, and `float.to_bits` returns `2251799813685248` (= `2^51`). Subnormals do not disappear at parse (T2), display (T13), arithmetic or `to_bits` boundaries.

```almide
test "subnormals survive arithmetic and observation" {
  let tiny = float.parse("2.2250738585072014e-308") ?? 0.0
  assert_eq(float.to_bits(tiny / 2.0), 2251799813685248)
  assert(tiny / 2.0 != 0.0)
  assert_eq(float.to_bits(float.parse("5e-324") ?? 0.0), 1)
}
```

Fixtures: `spec/wasm_cross/float_subnormal_preserved.almd`, `spec/stdlib/float_determinism_test.almd`. Contracts: C-304.

<a id="als-t22-超越関数の誤差上限"></a>

## ALS-T22 Error bounds of transcendental functions

Where T10 specifies byte agreement across all targets (one vendored implementation, no host libm), this section declares **distance from the true value**. Distance is measured as “ulp difference from the correctly rounded result”; for same-sign binary64 values, the absolute difference between `float.to_bits` values.

| Function | Bound |
|------|------|
| `math.sqrt` | Correctly rounded (0 ulp), required by IEEE-754 |
| `math.exp` `math.log` `math.log2` `math.log10` | ≤ 1 ulp |
| `math.sin` `math.cos` `math.tan` | ≤ 1 ulp |
| `float.pow` / Float `**` | ≤ 1 ulp |
| `math.log_gamma` | Undeclared; only byte agreement (T10) is normative, with no accuracy claim |

No accuracy is claimed for unlisted functions. Bounds are checked by execution at sample points (the test below and `spec/stdlib/math_accuracy_test.almd`; references are calculated to 70 decimal digits and correctly rounded to binary64).

```almide
fn ulp_from(x: Float, reference_bits: Int) -> Int = int.abs(float.to_bits(x) - reference_bits)

test "sqrt is correctly rounded; exp/log/sin stay within 1 ulp at sample points" {
  assert_eq(ulp_from(math.sqrt(2.0), 4609047870845172685), 0)
  assert(ulp_from(math.exp(1.0), 4613303445314885481) <= 1)
  assert(ulp_from(math.log(2.0), 4604418534313441775) <= 1)
  assert(ulp_from(math.sin(1.0), 4605754516372524270) <= 1)
}
```

Fixtures: `spec/wasm_cross/math_transcendental_bits.almd`, `spec/stdlib/math_accuracy_test.almd`. Contracts: C-305.

<a id="als-t23-符号付きゼロ"></a>

## ALS-T23 Signed zero

Zero signs propagate under IEEE-754 rules: `-0.0 * 1.0 = -0.0`, `-0.0 + 0.0 = 0.0` (opposite-sign zeros sum to `+0` under ties-to-even, also when `+0.0` is a literal; `x + 0.0 → x` is false for `x = -0.0` and must not be folded, whereas `x - 0.0 → x` and `x * 1.0 → x` hold), and `1.0 / -inf = -0.0`. Equality ignores sign (`0.0 == -0.0` is true, ALS-C3); display preserves it (`-0.0` → `-0.0`, ALS-E3).
`float.min/max` follow **IEEE-754-2019 `minimum`/`maximum` zero ordering**, treating `-0.0 < +0.0`: `min(-0.0, 0.0) = min(0.0, -0.0) = -0.0`, `max(-0.0, 0.0) = max(0.0, -0.0) = +0.0`. This is independent of argument order (commutative) and consistent with ALS-C9 totalOrder. NaN is ignored as in T15: with one NaN return the other operand, with two return NaN.

**Decision (2026-08-21, ADR-0016)**: the previous rule “return the first argument for equal ±0” (old C-049 wording) is withdrawn and replaced by this section.

```almide
fn neg_zero() -> Float = (0.0 - float.abs(-1.0)) * 0.0

test "signed zero propagates, compares equal, and orders -0 < +0 in min/max" {
  let mz = neg_zero()
  assert_eq(float.to_string(mz), "-0.0")
  assert(mz == 0.0)
  assert_eq(float.to_string(mz + 0.0), "0.0")
  assert_eq(float.sign(float.min(mz, 0.0)), -1.0)
  assert_eq(float.sign(float.min(0.0, mz)), -1.0)
  assert_eq(float.sign(float.max(mz, 0.0)), 1.0)
  assert_eq(float.sign(float.max(0.0, mz)), 1.0)
  assert_eq(float.sign(float.min(0.0, mz)), -1.0)
  assert_eq(float.sign(float.max(mz, 0.0)), 1.0)
}
```

Fixtures: `spec/wasm_cross/float_signed_zero_minmax.almd`, `spec/wasm_cross/float_sign_minmax_ieee.almd`, `spec/stdlib/float_determinism_test.almd` (propagation/equality), `spec/stdlib/float_minmax_zero_order_test.almd` (±0 ordering: still the old rule through 0.59.1, so this file stays red until the pin advances), `spec/stdlib/float_signed_zero_sum_test.almd` (sum with literal `0.0`: this file's failure found native 0.58.0 folding `x + 0.0 -> x`; 0.59.1 fixed it by retaining only sign-exact identities, now green).
Contracts: C-306.

<a id="als-t24-float--int-変換"></a>

## ALS-T24 Float → Int conversion

`int.from_saturating(x)` **truncates toward zero**, **saturates to i64 endpoints** outside the range, and maps NaN to `0`: `from_saturating(2.7) = 2`, `from_saturating(-2.7) = -2`, `from_saturating(+inf)` is `9223372036854775807`, `from_saturating(-inf)` is `-9223372036854775808`, `from_saturating(9.3e18)` is `9223372036854775807` (saturation), `from_saturating(NaN) = 0`, `from_saturating(-0.0) = 0`.
The checked family (`int8.from_checked` … `uint64.from_checked`, `float32.from_checked`) returns **`some` only when the value is exactly representable in the target type**: fractional values, out-of-range values, NaN and ±inf return `none` (`int64.from_checked(2.7) = none`, `int64.from_checked(-0.0) = some(0)`, `float32.from_checked(0.1) = none`). `float.floor/ceil/round` are Float → Float and outside this section (T15 covers round's ±0).

```almide
test "int.from_saturating truncates toward zero, saturates, and maps NaN to 0" {
  let inf = 1.0 / 0.0
  assert_eq(int.from_saturating(2.7), 2)
  assert_eq(int.from_saturating(-2.7), -2)
  assert_eq(int.from_saturating(inf), 9223372036854775807)
  assert_eq(int.from_saturating(0.0 - inf), -9223372036854775808)
  assert_eq(int.from_saturating(0.0 / 0.0), 0)
  assert_eq(int64.from_checked(2.7), none)
  assert_eq(int64.from_checked(-0.0), some(0))
}
```

Fixtures: `spec/wasm_cross/float_to_int_edges.almd`, `spec/stdlib/float_determinism_test.almd`. Contracts: C-307.

<a id="als-t25-matrix-の超越計算と積の順序"></a>

## ALS-T25 Matrix transcendental calculations and product order

Elementwise transcendental calculations in `matrix` (those reaching `float.pow` or similar, such as `matrix.map(m, (x) => float.pow(x, e))`) use **bundled musl libm** (T22), not host libm. They emit the same bytes on both targets, and the same program emits identical bytes regardless of build platform (C-223). Products (`a * b`) use the native scalar loop on every leg: accumulate from 0.0, ascending k, multiply then add, without fused multiply-add. Activation functions formerly using canonical fast-exp (`softmax_rows`, `gelu`, `swiglu_gate`) and rope's inverse-frequency table left the standard library in dialect epoch 14 for module nn.kernels in package almide-graphics/nn (C-270).
Fixture: `spec/wasm_cross/matrix_pow_libm.almd`. Contracts: C-223.

<a id="als-t26-datetime-の暦フィールドと-parse_iso-の文法"></a>

## ALS-T26 Datetime calendar fields and parse_iso grammar

`datetime.year` / `month` / `day` / `weekday`, extracting calendar fields from Unix seconds `ts`, and `to_iso` / `format` using them, compute day numbers by **floor division**. Even when `ts` is negative and not on a day boundary, they return the day containing that second: `to_iso(-1)` is `1969-12-31T23:59:59Z`; `weekday(-1)` is `Wednesday`. The time fields (`hour` / `minute` / `second`) continue to use floor remainder, so both identify the same day. `from_parts` is the inverse within its domain (`from_parts(1969, 12, 31, 23, 59, 59)` is `ok(-1)`).

`datetime.from_parts(y, mo, d, h, mi, s)` returns `Result[Int, String]` and errs for out-of-range fields with **the same domain and wording as `parse_iso`**: the first invalid field in year → month → day → hour → minute → second order (almide/almide#4149). It does not carry: `from_parts(2026, 2, 30, 0, 0, 0)` is `err("day out of range: 30")`, not 2026-03-02. The two constructors never interpret the same input as different dates.
Fixture: `spec/wasm_cross/datetime_from_parts_strict.almd`.

`datetime.parse_iso(s)` trims surrounding whitespace and reads this grammar:

```ebnf
datetime := date "T" time zone?
date     := digit+ "-" digit+ "-" digit+
time     := digit+ ":" digit+ ":" digit+
zone     := "Z" | ("+" | "-") digit+ ":" digit+
```

Every field uses ASCII digits only, with arbitrary width (`2024-1-5T1:2:3` is accepted; signs, whitespace and letters are not). `Z` and numeric offsets are mutually exclusive. An offset is **applied**, and the result identifies a UTC instant: `2024-01-15T10:30:00+09:00` is 1705282200. Domains: year 0..=9999 (RFC 3339 §5.6 `date-fullyear = 4DIGIT`; an out-of-range year errs rather than returning an ok wrapped instant); month 1..=12; day 1..=days in that month under proleptic Gregorian leap-year rules; hour 0..=23; minute/second 0..=59, without leap seconds; offset hour 0..=23 and minute 0..=59.

Error strings (the first violated field wins):

| Input | err |
|------|-----|
| Out-of-range year/month/day/hour/minute/second | `<field> out of range: <value>` (`year` / `month` / `day` / `hour` / `minute` / `second`) |
| Invalid or out-of-range offset | `invalid offset: <符号以降の文字列>` (text from the sign onward) |
| Other invalid spelling | `invalid datetime format` |
| Missing `T` | `expected YYYY-MM-DDTHH:MM:SSZ` |

Tests: `spec/wasm_cross/datetime_pre_epoch.almd`, `spec/wasm_cross/datetime_parse_iso_strict.almd`, `spec/wasm_cross/datetime_parse_iso_year_range.almd`, `spec/stdlib/datetime_test.almd`. Contracts: C-359, C-360, C-410.

<a id="als-t27-urlparse-の-authority-規範"></a>

## ALS-T27 Authority requirements of url.parse

`url.parse(s)` reads `scheme://host[:port][/path][?query][#fragment]` and checks the **host**, not the authority. An empty host yields `url.parse: empty host`; a spelling with only a port such as `http://:8` also has an empty host. Ports contain ASCII digits only, in 0..=65535. Nondigits, including `+80`, `-0`, ` 80`, yield `url.parse: invalid port: <text>`; out-of-range values yield `url.parse: port out of range: <text>`. The two documented out-of-scope forms are rejected by name, not read as hosts: userinfo yields `url.parse: userinfo is not supported: <authority>`, IPv6 literals yield `url.parse: IPv6 hosts are not supported: <authority>`. Hosts containing characters outside RFC 3986 reg-name (unreserved / sub-delims / `%`) yield `url.parse: invalid host: <host>`. Accepted URLs round-trip through `to_string`.
Tests: `spec/wasm_cross/url_authority_edges.almd`, `spec/stdlib/url_test.almd`. Contracts: C-361.

<a id="als-t28-float32-演算の-binary32-丸め"></a>

## ALS-T28 Binary32 rounding of Float32 operations

Every `Float32` operation (`+` `-` `*` `/` `%` `**`, unary `-`) rounds its result to f32 **after each operation**, using IEEE-754 **binary32** ties-to-even, yielding the same value as native `f32` operations. Even an implementation carrying `Float32` in wider f64 slots must round to binary32 immediately after every operation; otherwise widening with `float64.from`, comparisons and subsequent operations all diverge from native. Conversion to `Float32` (`float32.from(x)`, whether x is `Float`, `Int` or a sized integer) rounds once. Comparisons (`==` `!=` `<` `<=` `>` `>=`) compare rounded binary32 values. Bare float literals in a `Float32` context narrow to binary32 upon creation (ALS-E3). `math.*` accepts only `Float`, so `Float32` passes through explicit widening (`float.from(x)`). For `Float32`, `a ** b` is the result of `Float` `**` on both widened operands, using ALS-T10's single pow implementation, rounded once to binary32.
Examples: `1/3` widens to `0.3333333432674408`; `2^24 + 1` rounds to `2^24`; summing `0.1` ten times yields `1.0000001192092896`.
Test: `spec/wasm_cross/float32_arithmetic_rounds.almd`. Contracts: C-371.

<a id="als-t29-float32-の表示"></a>

## ALS-T29 Float32 display

`Float32` displays as the **shortest decimal representation round-tripping to the same binary32** (Rust `f32` Display, positional notation without exponents). `0.1` is `0.1`, not widened f64 `0.10000000149011612`. If two shortest candidates are exactly equidistant, round the magnitude upward (`2^-12` is `0.00024414063`). Interpolation and containers (lists, records, Option, tuples and variant payloads) drop `.0` on integral values (ALS-R2 / C-011); `float32.to_string` retains it. Preserve the sign of `-0`; nonfinite values are `inf` / `-inf` / `NaN`.
Test: `spec/wasm_cross/float32_display_shortest.almd`. Contracts: C-372.

<a id="als-t30-testingassert_aborts--子で走る-abort-表明"></a>

## ALS-T30 testing.assert_aborts — asserting an abort in a child

Within a `test` block, `testing.assert_aborts(s, f)` executes **only `f`** in a child. Native starts the same test binary as a fresh child process; wasm creates a fresh instance of the same module. The child starts from the program's initial state: top-level `let` and `var` have initial values and cannot see any parent state. cwd, environment variables and argv match the test. `f` may be an effect closure; its value is discarded.

`f` must be an inline lambda and must not capture test locals (`let`, `var`, `where` bindings or enclosing-lambda arguments). Captures and non-lambda-literal function values are rejected with E098. Only top-level `fn`, `let`, `var` and imported modules may be referenced. Calling outside a `test` block (in `fn` or `main`), using it as a function value, or calling inside another `assert_aborts` closure is E099 at check time.

In the child, `f` is lowered **as a program body**. `assert` and `testing.assert_*` stop with ALS-T18's block; `panic` stops with C-219's `PANIC: <msg>`.

The child counts as aborting **only on a defined abort**: exit status 1 and a stderr line starting with `Error: ` or `PANIC: ` (the abort heading). **Abort text** extends from the last heading line to the end of stderr. `assert_aborts` passes when this text contains `s` as a bytewise, case-sensitive substring. Earlier `eprintln` output is not used for matching. `panic`, out-of-range indexing, division by zero, stack overflow and failed asserts all count as aborts.

Normal return, other exit statuses (`process.exit(n)` with n ≠ 1, raw panic, trap), or exit 1 without a heading fails. Failure uses ALS-T18 field order: `Error: assertion failed`, `at: line <N>`, `expected: abort containing "<s>"`, `found: <理由>`. `<理由>` (reason) is one of `returned normally`, `abort "<abort テキスト>"` (abort text), `exit code <n>`, `exit code 101 (raw panic)`, `trap (exit 134)`. If the child wrote output, `child stdout:` and `child stderr:` sections follow. Child output appears only in this report and is not forwarded on success.
The test continues after each call; multiple calls in one test each have their own child.
Tests: `spec/stdlib/coverage_assert_aborts_test.almd`, `spec/stdlib/testing_polarity_test.almd`, `spec/lang/assert_aborts_continues_test.almd`. Contracts: C-384.

<a id="als-t31-hmac-と定数時間比較"></a>

## ALS-T31 HMAC and constant-time comparison

`hash.hmac_sha256(key, b)` and `hash.hmac_sha512(key, b)` implement HMAC over SHA-256 and SHA-512 (RFC 2104), with **the key as the first argument**. The key is `Secret[Bytes]`, and the resulting tag is also `Secret[Bytes]`. Keys longer than the block length (64/128 bytes) are hashed first; shorter keys are zero-padded to that length. Revealed tags are 32/64 bytes and match RFC 4231 test cases 1–7 for both digests. Tags display as `Secret(***)`, and `==` is constant-time comparison (ALS-C3, C-426). There is no `_hex` cell: write `hex.encode(secret.reveal(tag))` for hexadecimal. `hash.is_valid_hmac_sha256(key, b, tag)` / `is_valid_hmac_sha512` return true only when the revealed value of `Secret[Bytes]` `tag` equals the computed MAC, comparing through `bytes.is_equal_constant_time`. Different-length tags are false. Decode hexadecimal tags with `hex.decode` before passing them; both hex cases are accepted. `secret.new(hex.decode(sig) ?? bytes.from_list([]))` turns odd length, nonhex characters or an empty string into an empty tag; validation returns false without aborting.

`bytes.is_equal_constant_time(a, b)` / `string.is_equal_constant_time(a, b)` return true only when both length and bytes match (String compares UTF-8 bytes). A length difference immediately returns false; length is public information. With equal lengths, they read every byte pair and compare the OR of all `a[i] xor b[i]` against 0 once after the scan. There are no content-dependent branches, indices or exits.
Tests: `spec/wasm_cross/hash_hmac_rfc4231.almd`, `spec/wasm_cross/bytes_is_equal_constant_time.almd`. Contracts: C-422, C-423.

> Source caveats: dated red/green fixture states and former implementations in T23 are historical, not a current test report. T13's general round-trip formula is retained although T2 only accepts finite inputs and NaN is unequal to itself. T24 combines integer checked conversions with float32.from_checked, then broadly says fractional values return none; this wording is retained, not resolved into a new conversion rule. T22 states bounds and sample-point validation, not an independently established exhaustive proof here. T5's explicit no-ASCII-fold exception remains an intentional departure from its base Unicode mapping. Constant-time claims in T31 retain their equal-length scope; they do not claim length secrecy or whole-program timing safety.
