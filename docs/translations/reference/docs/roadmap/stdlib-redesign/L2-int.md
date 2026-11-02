> Source description: Stdlib redesign L2/L3 — int: retire the from_&lt;sized&gt; column and the u32/u8 one-offs, complete the wrap family, add floored division and radix conversion
<a id="l2l3-int"></a>

# L2/L3: int

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-int.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-int.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-int.zh-CN.md)

> Non-normative reader snapshot of the pinned source; original remains authoritative. No automatic synchronization, independent implementation validation or execution of quoted instructions. Historical status, plans and comparative claims retain their source scope.
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-int.md)

> Source-history caveat: this is a dated redesign record, not the current integer API or new measurements. The heading says 2026-10-21 while R-int-13 adds a 2026-10-23 observation. Earlier source-side conversion proposals coexist with later R-L0-40 generic destination-side conversions; N-bit signatures coexist with R-int-12 type-width revisions. Preserve the stated 72+13 count and to_bigint mention without inventing a missing table row or reconciling later additions. The opening wrapping-add observation and later broad overflow-safety wording have different scopes; no universal overflow guarantee follows. The table groups Rust Euclidean operations with floored operations, but does not establish equivalence for every divisor sign. F markers and peer comparisons are the original historical checks, not independently refreshed facts. Accepted proposals, deprecation epochs, fixes and gates are not independently verified implementation or execution.


> Last updated: 2026-10-21
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md), [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)
> Related: [L2-sized-numbers](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md) (conversion matrix), [L2-math](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-math.md) (type twins), [L2-float](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-float.md)
> Sources: Rust https://doc.rust-lang.org/std/primitive.i64.html · Go https://pkg.go.dev/math/bits and https://pkg.go.dev/strconv · Python https://docs.python.org/3/library/stdtypes.html#numeric-types-int-float-complex · TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Number · Gleam https://gleam-stdlib.hexdocs.pm/gleam/int.html (F) · Elixir https://elixir.hexdocs.pm/Integer.html (F) and https://hexdocs.pm/elixir/Bitwise.html · OCaml https://ocaml.org/manual/latest/api/Int.html (F) · Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin/-long/ (F) · Swift https://developer.apple.com/documentation/swift/int

(F) = cells whose spelling was checked by fetching that official page in this investigation. The Gleam, Elixir, OCaml and Kotlin function-name lists were retrieved on 2026-10-21.

Measurements (`almide 0.68.0 dev 437b8f127`, identical results on native and `--target wasm`):

| Expression | Result | Interpretation |
|---|---|---|
| `int.wrap_add(1, 2, 0)` | `0` | `bits = 0` silently returns 0 |
| `int.wrap_add(1, 2, -1)` | `3` | Negative `bits` means “all bits”; `rotate_left` aborts on the same input (C-047) |
| `int.to_hex(-1)` → `int.from_hex` | `err: number too large…` | `to_hex` writes a 64-bit pattern; `from_hex` reads a signed number. They do not round-trip |
| `int.from_hex("-ff")` | `-255` | `from_hex` accepts a sign |
| `-7 / 2`, `-7 % 3` | `-3`, `-1` | Division truncates toward zero. There is no floored remainder |
| `5 & 3` | Lexical error | No bitwise operators; `int.band` etc. are the only spelling |
| `int.max_value() + 1` | `-9223372036854775808` | `Int` arithmetic wraps |

<a id="l2-役割の比較"></a>

## L2: Role comparison

| Role | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| To string | `to_string` | `to_string` | `strconv.Itoa` | `str` | `toString` | `to_string` (F) | `to_string` (F) | `to_string` (F) | `toString` | `String(n)` |
| From string | `parse` | `parse` | `strconv.Atoi` | `int(s)` | `parseInt` | `parse` (F) | `parse` (F) | `of_string_opt` | `toIntOrNull` | `Int(s)` |
| To string in a specified radix | `to_hex` only | `format!("{:b}")` | `FormatInt(n, b)` | `bin`/`oct`/`hex` | `toString(r)` | `to_base_string` (F) | `to_string(n, b)` (F) | — | `toString(r)` | `String(n, radix:)` |
| Parse in a specified radix | `from_hex` only | `from_str_radix` | `ParseInt(s, b, 64)` | `int(s, b)` | `parseInt(s, r)` | `base_parse` (F) | `parse(s, b)` (F) | — | `toIntOrNull(r)` | `Int(s, radix:)` |
| To Float | `to_float` | `as f64` | `float64(n)` | `float(n)` | (same type) | `to_float` (F) | — | `to_float` (F) | `toDouble` | `Double(n)` |
| Absolute/minimum/maximum/clamp | `abs` `min` `max` `clamp`（`math` also has duplicates） | `abs` `min` `max` `clamp` | `min` `max` | `abs` `min` `max` | `Math.*` | `absolute_value` `min` `max` `clamp` (F) | `abs` | `abs` `min` `max` (F) | `absoluteValue` `coerceIn` (F) | `abs` `min` `max` |
| Sign | `math.sign` | `signum` | — | — | `Math.sign` | — | — | — | `sign` (F) | `signum()` |
| Power | `math.pow` | `pow` | — | `**` | `**` | `power`（Float）(F) | `pow` (F) | — | — | — |
| Parity | `is_even` `is_odd` | — | — | — | — | `is_even` `is_odd` (F) | `is_even` `is_odd` (F) | — | — | `isMultiple(of:)` |
| Floored division/remainder | — | `div_euclid` `rem_euclid` | — | `//` `%` | — | `floor_divide` `modulo` (F) | `floor_div` `mod` (F) | `fdiv` `erem` (F) | `floorDiv` `mod` (F) | — |
| Bit AND/OR/XOR/NOT | `band` `bor` `bxor` `bnot` | operators | operators | operators | operators | `bitwise_and` etc. (F) | `band` `bor` `bxor` `bnot` | `logand` etc. (F) | `and` `or` `xor` `inv` (F) | operators |
| Arithmetic shifts | `bshl` `bshr` | `<<` `>>` | `<<` `>>` | `<<` `>>` | `<<` `>>` | `bitwise_shift_*` (F) | `bsl` `bsr` | `shift_left` `shift_right` (F) | `shl` `shr` (F) | `<<` `>>` |
| Logical right shift (zero-fill) | — | unsigned-type `>>` | unsigned-type `>>` | — | `>>>` | — | — | `shift_right_logical` (F) | `ushr` (F) | unsigned-type `>>` |
| Bit counting/rotation/reversal | `pop_count` `count_*_zeros` `rotate_*` `bit_reverse` `byte_swap` `bit_width` | available | `math/bits` | `bit_count` `bit_length` | `Math.clz32` | — | `popcount` (F) | `popcount` `leading_zeros` (F) | `countOneBits` `rotateLeft` (F) | available |
| N-bit wrapping arithmetic | `wrap_add` `wrap_mul`（`wrap_sub` absent） | `wrapping_*` | type conversion itself | — | `BigInt.asUintN` | — | — | — | — | `&+` `&-` `&*` |
| Lower/upper bound | `min_value` `max_value` | `MIN` `MAX` | `math.MinInt64` | — | `MIN_SAFE_INTEGER` | — | — | `min_int` `max_int` (F) | `MIN_VALUE` (F) | `.min` `.max` |
| Conversions with fixed-width types | `to_<型>` and `from_<型>` both | `as`・`from`・`try_from` | `T(x)` | — | — | — | — | `Int32.of_int` | `toByte()` etc. (F) | `init(truncatingIfNeeded:)` etc. |


<a id="完全性の規則"></a>

### Completeness rules

1. **The conversion matrix** follows [L2-sized-numbers](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md). `int` is one of 12 carriers; the `int` module contains only conversions going **out of `Int`** (R-L0-07). Incoming conversions to `Int` are `to_int` in the source type’s module.
2. **Wrapping arithmetic family** (R-L0-08, #3854): four cells — `wrap(n, bits)`, `wrap_add`, `wrap_sub`, `wrap_mul` — must exist together. Signature `(…, bits: Int) -> Int`; `bits` follows rotation (`rotate_*`): `bits <= 0` aborts, `bits >= 64` stays 64-bit. Deliberately omit `wrap_neg` (equivalent to `int.wrap(0 - a, bits)`; `0 - min_value` also wraps to the correct value).
3. **No type twins** (R-L0-01, R-L0-02): arithmetic whose subject is `Int` (`abs`, `min`, `max`, `sign`, `pow`) belongs only in `int`, not also `math` ([L2-math](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-math.md) R-math-01).
4. **Radix conversions come as a pair**: to string (`to_string_radix`) and from string (`parse_radix`) must round-trip under the same radix and sign rules.
5. **Ordered types have `compare`** (R-L0-16): `int.compare(a, b) -> Int` (negative/0/positive). Add it to all numeric carriers simultaneously as one cell of [R-sized-02](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md).
6. Keep bitwise names in the `b`-prefixed family (`band` `bor` `bxor` `bnot` `bshl` `bshr`), including new cells.

<a id="l2-の判定"></a>

### L2 assessment

- Missing roles: `compare` (required by R-L0-16) → [R-sized-02](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md). Floored division/remainder (6/9) → R-int-03. Radix conversions (8/9) → R-int-04. Logical right shift (6/9) → R-int-06. Move `sign` and `pow` from `math` → R-math-01.
- Duplicates: `int.to_u32` / `int.to_u8` versus `int.to_uint32` / `int.to_uint8` (#3851 E) → R-int-02. `int.from_hex` versus new `int.parse_radix(s, 16)` → R-int-04. `int.from_<型>` versus `<型>.to_int` (R-L0-07) → R-int-01. In those preserved identifiers, 型 means “type.”
- Family gaps: no `wrap_sub` or `wrap` (#3854) → R-int-02. No `int.to_float_checked` or `int.to_float64_checked` (lossy `Int`→`Float` conversion rounds beyond 2^53) → [R-sized-01](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md).
- Spelling: no majority for `band`/`bor`/`bxor`, so retain → R-int-05 (rejected).

<a id="案"></a>

## Proposals

<a id="r-int-01-intfrom_型-の列を撤去し元の型の-to_int-に移す---採用"></a>

### R-int-01 Remove the `int.from_<型>` column and move to the source type’s `to_int` — ○ Accepted

- Content: remove 10 functions: `int.from_int8` `from_int16` `from_int32` `from_int64` `from_uint8` `from_uint16` `from_uint32` `from_uint64`, plus `int.from_uint64_checked` and `int.from_uint64_saturating`. Add `int8.to_int` … `uint64.to_int`, `uint64.to_int_checked`, `uint64.to_int_saturating` instead (main additions in [R-sized-01](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md)).
- L0 basis: R-L0-07 (value conversion is the source type’s `to_<型>`; if present on both sides, keep only side 1), R-L0-01. 型 in preserved names denotes the type.
- (a) 5/9 languages place `to_X` on the source type (Rust, Gleam, Elixir, OCaml, Kotlin `toLong()`). None of the nine uses the `Int.from_int8` form.
- (b) Pure computation. Current `int.from_<型>` are intrinsics; add new `to_int` as aliases calling the same intrinsic and reattach rows in `self_host_registry.rs`. Values stay unchanged on both targets.
- (c) In `x |> int8.to_int`, the type at the pipe’s start determines the module. Today `int8.to_int16` works but `int8.to_int` does not, so an LLM inferring from the rule will necessarily miss.
- (d) Removal/addition pair. `@deprecated(since=15, use="int8.to_int")` etc., removal epoch 16. `almide fix`: `int.from_<T>(x)` → `<T>.to_int(x)` (one-to-one). Usage counts (spec/tests/docs/stdlib): eight functions total 70/38/47/362 (the 362 stdlib uses are sized-module bodies calling them internally; migration takes these to zero); `from_uint64_checked` 9/2/7/1; `from_uint64_saturating` 4/2/4/0. Include CHEATSHEET:1068 and llms.txt:412 rewrites in the same change.
- Proposed gate: in `src/cli/docs_gen.rs`, change `check_numeric_conversion_matrix` from “the Int column is `int.from_<s>`” to “`<s>.to_int`”, also checking absence of `int.from_<s>` (same change as the R-sized-01 gate).
- Issue: #3935

<a id="r-int-02-折り返しの族を-4-セルにそろえto_u32to_u8-を撤去する---採用"></a>

### R-int-02 Complete the four-cell wrap family and remove `to_u32`/`to_u8` — ○ Accepted

- Content:
  - Add `int.wrap(n: Int, bits: Int) -> Int` (`n mod 2^bits`, nonnegative when `bits < 64`) and `int.wrap_sub(a: Int, b: Int, bits: Int) -> Int`.
  - Align boundaries: `bits <= 0` aborts in `wrap`/`wrap_add`/`wrap_sub`/`wrap_mul` (same wording form as `rotate_*` C-047). `bits >= 64` stays 64-bit (as C-048).
  - Remove `int.to_u32` (= `int.wrap(n, 32)`) and `int.to_u8` (= `int.wrap(n, 8)`).
  - Do not add `wrap_neg` (completeness rule 2).
- L0 basis: R-L0-08 (fill matrix gaps), R-L0-01 (`to_u32` and `to_uint32` are a confusing pair with identical values but different types).
- (a) Every language with wrapping arithmetic has wrapping subtraction (Rust `wrapping_sub`, Swift `&-`). None has addition and multiplication but no subtraction.
- (b) Pure computation; can share `__wrap_mask` in `stdlib/int_wrap.almd`. Today `bits <= 0` returns “0 or all bits” on native and wasm (opening measurements); both targets change identically to abort.
- (c) After seeing `wrap_add` and `wrap_mul`, an LLM writes `int.wrap_sub(a, b, 32)` (#3854). A one-character difference between `to_u32` and `to_uint32` changes the return type from `Int` to `UInt32`.
- (d) Two additions, two removals, one group of boundary-semantic changes. The boundary change narrows the domain by aborting inputs previously returning meaningless values; it changes no result for inputs that return values (not R-L0-14 item 4’s “same name, different meaning”). As domain narrowing, like positional writing in R-L0-12, it is not included in epoch `breaks`. Removals: `@deprecated(since=15, use="int.wrap")`, removal epoch16. `almide fix`: `int.to_u32(x)` → `int.wrap(x, 32)`; `int.to_u8(x)` → `int.wrap(x, 8)`. Usage counts: `to_u32` 6/2/10/0, `to_u8` 6/2/9/0, `wrap_add` 8/2/8/0, `wrap_mul` 7/2/6/0.
- Proposed gate: as #3854 proposes, beside `stdlib_synonym_gate_test.rs`, test that all four wrap-family cells exist, signatures agree, and `self_host_registry.rs` has rows. Pin boundary inputs `bits = 0`, `-1`, `64`, `65` in `spec/wasm_cross` fixtures for both-target equality (place a new contract C number in als first).
- Issues: #3854 (section 1), #3851 E

<a id="r-int-03-床方向の割り算と剰余-intfloor_div--intmodulo-を足す---採用"></a>

### R-int-03 Add floored division and remainder `int.floor_div` / `int.modulo` — ○ Accepted

- Content: `int.floor_div(a: Int, b: Int) -> Int` (quotient rounded toward −∞), `int.modulo(a: Int, b: Int) -> Int` (result sign matches `b`; `a == floor_div(a, b) * b + modulo(a, b)`). `b == 0` aborts like `/` (contract violation, R-L0-06 item3). `floor_div(min_value, -1)` aborts like `/` ("integer overflow").
- L0 basis: R-L0-02 (subject `Int` means `int`); L1 addition criterion (6/9 have it). Choose names within R-L0-00: remainder `modulo` (Gleam; Elixir/Kotlin `mod` collides with Almide visibility keyword `mod`, unusable as a function name), quotient `floor_div` (Elixir, meaning of OCaml `fdiv`, Kotlin `floorDiv`).
- (a) 6/9 have it (Python, Gleam, Elixir, OCaml, Kotlin and Rust’s Euclidean version). Only Go, JS and Swift do not.
- (b) Pure computation; writable in Almide from `/` and `%`.
- (c) Current `%` is remainder toward zero, so rotating an array with `(i - 1) % n` gives a negative index. An LLM coming from Python’s floored `%` silently misses here. A named function allows one CHEATSHEET line: “floored remainder is `int.modulo`.”
- (d) Addition, zero uses.
- Proposed gate: pin four sign quadrants and abort on `b = 0` in `spec/stdlib/int_floor_div_test.almd`, add one wasm_cross fixture.
- Issue: #3918

<a id="r-int-04-基数の変換を-to_string_radix--parse_radix-の対にしfrom_hex-を撤去する---採用"></a>

### R-int-04 Pair radix conversions as `to_string_radix` / `parse_radix`, remove `from_hex` — ○ Accepted

- Content:
  - Add `int.to_string_radix(n: Int, radix: Int) -> String` (signed: `-255` at radix16 is `"-ff"`; lowercase), `int.parse_radix(s: String, radix: Int) -> Result[Int, String]` (trim surrounding whitespace, accept `+`/`-`, case-insensitive; only for radix16/8/2 accept the corresponding optional `0x`/`0o`/`0b` prefix). Abort if `radix` is outside2–36 (contract violation).
  - Remove `int.from_hex` (= `int.parse_radix(s, 16)`; define `parse_radix` to keep identical “0x optional, sign accepted, err above2^63−1” behavior).
  - Retain `int.to_hex`, a separate role: write a 64-bit pattern as hex (`-1` → sixteen `f`s). Add a CHEATSHEET line to use `to_string_radix(n, 16)` for signed hex.
- L0 basis: R-L0-01 (`from_hex` and `parse_radix(s, 16)` have the same role), L1 addition criterion (8/9 support radix). Select spelling within R-L0-00 (no majority: Gleam `to_base_string`/`base_parse`, Kotlin `toString(radix)`/`toInt(radix)`, Swift `radix:`). Almide has no optional arguments, so put `radix` in the name to distinguish existing `to_string`/`parse`.
- (a) 8/9 have it (only OCaml Stdlib lacks it).
- (b) Pure computation.
- (c) Binary notation (`to_string_radix(n, 2)`) is standard in algorithm problems but currently must be handwritten. A separate round-tripping pair provides an escape from the `to_hex`/`from_hex` non-round-trip trap (opening measurements).
- (d) Two additions, one removal. `@deprecated(since=15, use="int.parse_radix")`, removal epoch16. `almide fix`: `int.from_hex(s)` → `int.parse_radix(s, 16)`. Usage: `from_hex`52/2/22/1, `to_hex`23/2/10/2.
- Proposed gate: spec testing `to_string_radix`→`parse_radix` round-trips for radix2–36 and `min_value`, `max_value`, `0`, `-1`; differential test passing existing `from_hex` specs through `parse_radix(s, 16)` during the pre-removal window.
- Issue: #3920

<a id="r-int-05-bandborbxor-などを多数派の綴りに改名する---不採用改名しないr-int-10-で確かめた"></a>

### R-int-05 Rename `band`/`bor`/`bxor` and others to majority spellings — × Rejected (no rename; confirmed in R-int-10)

- Content: do not rename.
- L0 basis: R-L0-00. The four languages with named bit functions disagree (Gleam `bitwise_and`, Elixir `band`, OCaml `logand`, Kotlin `and`, all (F)). The other five have only operators. With no majority, retain the current spelling (Elixir's `b` prefix).
- (c) Writing operator `&` produces a lexical error with a hint, but it does not name `int.band`. Diagnostics, not renaming, should help: create a separate small issue to add “bit operations use `int.band` / `int.bor` / `int.bxor` / `int.bshl` / `int.bshr`” to E0xx (unknown-char) hints.
- Issue: #3905 (diagnostic hint only)

<a id="r-int-06-論理右シフト-intbshr_logical-を足す---採用"></a>

### R-int-06 Add logical right shift `int.bshr_logical` — ○ Accepted

- Content: `int.bshr_logical(a: Int, n: Int) -> Int`. Right shift filling high bits with 0. As with `bshr`, `n` is its remainder modulo 64.
- L0 basis: L1 addition criterion (6/9), R-L0-00 (OCaml `shift_right_logical` reshaped into Almide's `b` family; Kotlin `ushr` and JS `>>>` do not fit the family).
- (a) 6/9 (unsigned `>>` in Rust/Go/Swift, JS `>>>`, OCaml, Kotlin).
- (b) Pure computation. The runtime already has `prim.bshr_u`; only a public name is needed.
- (c) In hash functions and random numbers (including SplitMix64 in R-random-01), arithmetic shift mixes in the sign and silently yields a different value. Currently one must handwrite `int.band(int.bshr(x, k), mask)`.
- (d) Addition. Usage count 0.
- Proposed gate: add logical shifts of negative numbers to `spec/stdlib/int_bits_test.almd`, and require both targets to match in a wasm_cross fixture.
- Issue: #3919

<a id="r-int-07-pop_count-を-count_ones-に改名する---採用"></a>

### R-int-07 Rename `pop_count` to `count_ones` — ○ Accepted

- Current: the bit-counting family has `count_leading_zeros` and `count_trailing_zeros`, but only the function counting 1 bits is called `pop_count`.
- Proposal: rename to `int.count_ones`, aligning the family on `count_`.
- L0 basis: R-L0-00 (inconsistent spelling within a family).
- (a) Rust `count_ones`, Kotlin `countOneBits`, Python `bit_count`, Go `OnesCount64`. The spelling `pop_count` is merely close to OCaml/Elixir `popcount`.
- (d) Rename. `@deprecated(since=15, use="int.count_ones")`, `removal_epoch = 16`; rewrite with `almide fix`.
- Issue: #3985

<a id="r-int-08-intto_hex-を-to_string_radixx-16-にまとめる---不採用"></a>

### R-int-08 Merge `int.to_hex` into `to_string_radix(x, 16)` — × Rejected

- Reason: their results differ for negative numbers; they are not synonyms. `int.to_hex(-1)` is a 64-bit pattern in hex (`"ffffffffffffffff"`), while `int.to_string_radix(-1, 16)` is signed (`"-1"`). Displaying hashes or bit patterns differs from expressing a number in another radix.
- Issue: none

<a id="r-int-09-intsign-と-floatsignum-の綴りを-1-つにする---不採用"></a>

### R-int-09 Unify the spellings of `int.sign` and `float.signum` — × Rejected

- Reason: zero is treated differently, so sharing a name would restore the mistake avoided by R-float-05. `int.sign(0)` and `bigint.sign(0)` return 0, whereas `float.signum(0.0)` returns 1.0 (like Rust `signum`). Reserve `sign` for functions where “0 maps to 0” (R-float-05 completeness rule).
- Issue: none

<a id="r-int-10-ビット演算の綴りを計測で決める---採用関数のまま十二巡目に演算子の案を取り消し"></a>

### R-int-10 Decide bit-operation spelling by measurement — ○ Accepted (keep functions; operator proposal withdrawn in round twelve)

- Current: `band`, `bor`, `bxor`, `bnot`, `bshl`, `bshr`, `bshr_logical` (Elixir's `b` prefix). R-sized-10/R-bigint-07 made them `Integer` requirements, extending to 61 functions across 10 integer types.
- R-int-05's (×) reason remains valid: the four languages using names disagree, with no majority. Nor will renaming be decided solely because of type errors (R-L0-36).
- Candidates to measure:
  1. Current `int.band(a, b)` (Elixir).
  2. Word forms `int.bit_and(a, b)`, `bit_or`, `bit_xor`, `bit_not`, `shift_left`, `shift_right` (close to Gleam `bitwise_and` and Roc `bitwiseAnd`).
  3. Operators `a & b`, `a | b`, `a ^ b`, `~a` (after R-L0-39 frees `^`; same as Rust/Go/Python/JS/Swift). Shifts remain functions (`shift_left`, `shift_right`) because `>>` is function composition.
- Decision (round twelve): retain current functions (1), making them `Integer` requirements. Round nine chose operators (3), but this was withdrawn because `^` historically meant exponentiation and `>>` composition; reusing these symbols for other meanings violates R-L0-43. Making only `&` and `|` operators leaves XOR and shifts as functions, an inconsistent halfway arrangement. The four function-name spellings have no majority (R-int-05), so retain the current spellings.
- Issue: almide-dojo#65

<a id="r-int-11-bit_width-を-bit_length-に改名する---採用"></a>

### R-int-11 Rename `bit_width` to `bit_length` — ○ Accepted

- Current: `int.bit_width(n)` returns the number of bits needed to represent a value (Go `bits.Len64`, Python `bit_length`). Swift `bitWidth` is the type's width (always 64 for `Int`): same name, different value. Inferring from Swift and using `int.bit_width(n)` for type width silently gives the wrong value.
- Proposal: rename to `bit_length` and make it an `Integer` requirement (also on the eight sized types and `bigint`; for `bigint`, like Python `int.bit_length` and Go `big.Int.BitLen`).
- (d) Rename and addition. `@deprecated` in epoch 15; remove in epoch 16.
- Issue: #4004

<a id="r-int-12-n-bit-の演算を型の幅の演算に置き換える---採用"></a>

### R-int-12 Replace N-bit operations with type-width operations — ○ Accepted

- Current: `int.wrap_add(a, b, bits)`, `wrap_sub`, `wrap_mul`, `rotate_left(a, n, bits)`, `rotate_right` treat `Int` as an N-bit number. Fixed-width types determine width from the type, so the shape conflicted when making these `FixedWidthInteger` requirements.
- Proposal: make all five `FixedWidthInteger` requirements; remove `bits` and operate at type width (`int`: 64 bits; `int32.wrap_add(a, b)`: 32 bits; like Swift `&+`, `&-`, `&*` and Rust `wrapping_add`, `rotate_left`). For N-bit truncation, compose with `int.wrap(n, bits)` (`int.wrap(int.wrap_add(a, b), 12)`). Keep `int.wrap` exclusive to `int`.
- (d) Semantic change. `almide fix` rewrites `int.wrap_add(a, b, bits)` to `int.wrap(int.wrap_add(a, b), bits)`. `rotate_*` with widths other than 8/16/32/64 cannot be rewritten, so provide a hint.
- Issue: #4004

<a id="r-int-13-床方向の剰余の名前は-modulofloor_divmin_value--1-は-abort---採用r-int-03-を直す"></a>

### R-int-13 Name floored remainder `modulo`; abort on `floor_div(min_value, -1)` — ○ Accepted (corrects R-int-03)

- Findings (while implementing wave 1B, #4045–#4048):
  - `mod` is a visibility keyword (`mod fn f() = …`, `TokenType::Mod`). `fn mod(` cannot be parsed; neither `int.mod` nor `int8.mod` can be written.
  - R-int-03 had said “`floor_div(min_value, -1)` wraps like `Int` arithmetic,” but `Int` division `/` aborts with "integer overflow" there on both native and wasm (measured 2026-10-23).
- Proposal:
  - Name floored remainder `modulo` (`int`, eight integer types, `bigint`). Do not remove `mod` from keywords to use it as a function name: distinguishing declaration-start `mod fn` from expression `x.mod(…)` contextually would change syntax.
  - Make `floor_div(min_value, -1)` abort like `/` (all nine integer types). Not silently wrapping integer overflow is the premise of the “integer overflow” row in the [safety table](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/security-by-construction.md). Use `wrap_*` when wrapping is desired.
- L0 basis: R-L0-00 (spellings from compared languages), R-L0-14 (avoid different meanings under one name: align endpoints of `/` and `floor_div`).
- (a) Remainder names: Gleam `int.modulo`, Elixir `Integer.mod`, Kotlin `mod`, Rust `rem_euclid`. `modulo` matches Gleam. `MIN / -1`: Rust `div_euclid` panics in debug, Python's arbitrary precision does not overflow, Kotlin `floorDiv` wraps.
- (c) LLMs tend to write `int.mod`. Calls to `int.mod` get a “use `int.modulo`” diagnostic (E002 hint).
- (d) Name change (an unshipped function, so no user-code impact) and one reconsidered boundary behavior.
- Issue: #4045

<a id="l3-関数表"></a>

## L3: Function table

The current 72 functions in `int` and 13 additions (including `compare` and [L2-bigint](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-bigint.md)'s `to_bigint`).
| Function | Signature (current, mut shown) | Main spellings in other languages | Verdict | Proposal |
|---|---|---|---|---|
| `to_string` | `int.to_string(n: Int) -> String` | Rust `to_string` / Go `strconv.FormatInt` / Python `str` / JS `toString` / Gleam `to_string` (F) / Elixir `to_string` (F) / OCaml `to_string` (F) / Kotlin `toString` / Swift `String(x)` | Retain | — |
| `to_hex` | `int.to_hex(n: Int) -> String` | Rust `format!("{:x}")` / Go `strconv.FormatInt(n,16)` / Python `hex` / JS `toString(16)` / Kotlin `toHexString` (F) / Gleam `to_base16` (F) | Retain | R-int-04（64-bit pattern as hex; signed radix conversion uses `to_string_radix`） |
| `parse` | `int.parse(s: String) -> Result[Int, String]` | Rust `parse` / Go `strconv.Atoi` / Gleam `parse` (F) / Elixir `parse` (F) / OCaml `of_string_opt` / Kotlin `toIntOrNull` / Swift `Int(s)` | Retain | — |
| `from_hex` | `int.from_hex(s: String) -> Result[Int, String]` | Rust `from_str_radix(s,16)` / Go `ParseInt(s,16,64)` / Python `int(s,16)` / Gleam `base_parse(s,16)` (F) / Elixir `parse(s,16)` (F) | Remove | R-int-04（`int.parse_radix(s, 16)` as the sole replacement） |
| `to_float` | `int.to_float(n: Int) -> Float` | Rust `as f64` / Gleam `to_float` (F) / OCaml `to_float` (F) / Kotlin `toDouble` (F) | Remove（→ `float.from`） | R-L0-40 |
| `bits_to_float` | `int.bits_to_float(bits: Int) -> Float` | Rust `f64::from_bits` (F) / Go `math.Float64frombits` / Kotlin `Double.fromBits` / Swift `Double(bitPattern:)` | Remove | R-float-02（`float.from_bits`） |
| `bits_to_f32` | `int.bits_to_f32(bits: Int) -> Float` | Rust `f32::from_bits` / Go `math.Float32frombits` / Kotlin `Float.fromBits` | Remove | R-float-02（`float32.from_bits` → `float32.to_float`） |
| `abs` | `int.abs(n: Int) -> Int` | Rust `abs` / Go — / Python `abs` / JS `Math.abs` / Gleam `absolute_value` (F) / Elixir `abs` / OCaml `abs` (F) / Kotlin `absoluteValue` (F) / Swift `abs` | Retain | R-math-01（`math.abs` is the removed counterpart） |
| `min` | `int.min(a: Int, b: Int) -> Int` | Rust `min` / Go `min` / Python `min` / JS `Math.min` / Gleam `min` (F) / OCaml `min` (F) / Kotlin `minOf` / Swift `min` | Retain | R-math-01（`math.min` is the removed counterpart） |
| `max` | `int.max(a: Int, b: Int) -> Int` | Rust `max` / Go `max` / Python `max` / JS `Math.max` / Gleam `max` (F) / OCaml `max` (F) / Kotlin `maxOf` / Swift `max` | Retain | R-math-01（`math.max` is the removed counterpart） |
| `clamp` | `int.clamp(n: Int, lo: Int, hi: Int) -> Int` | Rust `clamp` / Gleam `clamp` (F) / Kotlin `coerceIn` (F) / Swift — | Retain | — |
| `is_even` | `int.is_even(n: Int) -> Bool` | Gleam `is_even` (F) / Elixir `is_even` (F) / Swift `isMultiple(of: 2)` / others — | Retain | — |
| `is_odd` | `int.is_odd(n: Int) -> Bool` | Gleam `is_odd` (F) / Elixir `is_odd` (F) / others — | Retain | — |
| `band` | `int.band(a: Int, b: Int) -> Int` | Gleam `bitwise_and` (F) / Elixir `Bitwise.band` / OCaml `logand` (F) / Kotlin `and` (F) / Rust・Go・Python・JS・Swift operators `&` | Retain | R-int-05 ×(no rename) |
| `bor` | `int.bor(a: Int, b: Int) -> Int` | Gleam `bitwise_or` (F) / Elixir `Bitwise.bor` / OCaml `logor` (F) / Kotlin `or` (F) / other five languages operators `\|` | Retain | R-int-05 × |
| `bxor` | `int.bxor(a: Int, b: Int) -> Int` | Gleam `bitwise_exclusive_or` (F) / Elixir `Bitwise.bxor` / OCaml `logxor` (F) / Kotlin `xor` (F) / others operators `^` | Retain | R-int-05 × |
| `bnot` | `int.bnot(a: Int) -> Int` | Gleam `bitwise_not` (F) / Elixir `Bitwise.bnot` / OCaml `lognot` (F) / Kotlin `inv` (F) / others operators `!`/`~` | Retain | R-int-05 × |
| `bshl` | `int.bshl(a: Int, n: Int) -> Int` | Gleam `bitwise_shift_left` (F) / Elixir `Bitwise.bsl` / OCaml `shift_left` (F) / Kotlin `shl` (F) / others operators `<<` | Retain | R-int-05 × |
| `bshr` | `int.bshr(a: Int, n: Int) -> Int` | Gleam `bitwise_shift_right` (F) / Elixir `Bitwise.bsr` / OCaml `shift_right` (F) / Kotlin `shr` (F) / others operators `>>` | Retain | R-int-05 × |
| `count_leading_zeros` | `int.count_leading_zeros(n: Int) -> Int` | Rust `leading_zeros` / Go `bits.LeadingZeros64` / OCaml `leading_zeros` (F) / Kotlin `countLeadingZeroBits` (F) / Swift `leadingZeroBitCount` | Retain | — |
| `count_trailing_zeros` | `int.count_trailing_zeros(n: Int) -> Int` | Rust `trailing_zeros` / Go `bits.TrailingZeros64` / OCaml `trailing_zeros` (F) / Kotlin `countTrailingZeroBits` (F) / Swift `trailingZeroBitCount` | Retain | — |
| `pop_count` | `int.pop_count(n: Int) -> Int` | Rust `count_ones` / Go `bits.OnesCount64` / Python `bit_count` / OCaml `popcount` (F) / Elixir `popcount` (F) / Kotlin `countOneBits` (F) / Swift `nonzeroBitCount` | Rename → `count_ones` | R-int-07 |
| `bit_reverse` | `int.bit_reverse(n: Int) -> Int` | Rust `reverse_bits` / Go `bits.Reverse64` / others — | Retain | — |
| `byte_swap` | `int.byte_swap(n: Int) -> Int` | Rust `swap_bytes` / Go `bits.ReverseBytes64` / Swift `byteSwapped` | Retain | — |
| `bit_width` | `int.bit_width(n: Int) -> Int` | Go `bits.Len64` / Python `bit_length` / OCaml `unsigned_bitsize` (F) / Swift `bitWidth`(different meaning) | Rename → `bit_length` | R-int-11 |
| `log2_floor` | `int.log2_floor(n: Int) -> Int` | Rust `ilog2` / others — | Retain | — |
| `log2_ceil` | `int.log2_ceil(n: Int) -> Int` | others —（Rust derives from `next_power_of_two`） | Retain | — |
| `next_power_of_two` | `int.next_power_of_two(n: Int) -> Int` | Rust `next_power_of_two` / others — | Retain | — |
| `prev_power_of_two` | `int.prev_power_of_two(n: Int) -> Int` | Kotlin `takeHighestOneBit` (F) / others — | Retain | — |
| `wrap_add` | `int.wrap_add(a: Int, b: Int) -> Int` | Rust `wrapping_add` / Swift `&+` / JS `BigInt.asUintN` / others — | Semantic change（`bits` removed; wrap at 64 bits; for N bits combine with `int.wrap`） | R-int-12 |
| `wrap_mul` | `int.wrap_mul(a: Int, b: Int) -> Int` | Rust `wrapping_mul` / Swift `&*` / others — | Semantic change（`bits` removed; wrap at 64 bits; for N bits combine with `int.wrap`） | R-int-12 |
| `rotate_right` | `int.rotate_right(a: Int, n: Int) -> Int` | Rust `rotate_right` / Go `bits.RotateLeft64(x,-k)` / Kotlin `rotateRight` (F) | Semantic change（`bits` removed; rotate at 64 bits） | R-int-12 |
| `rotate_left` | `int.rotate_left(a: Int, n: Int) -> Int` | Rust `rotate_left` / Go `bits.RotateLeft64` / Kotlin `rotateLeft` (F) | Semantic change（`bits` removed; rotate at 64 bits） | R-int-12 |
| `to_u32` | `int.to_u32(a: Int) -> Int` | JS `x >>> 0` / Rust `as u32 as i64` / others — | Remove | R-int-02（`int.wrap(x, 32)`。#3851 E） |
| `to_u8` | `int.to_u8(a: Int) -> Int` | Rust `as u8` / others — | Remove | R-int-02（`int.wrap(x, 8)`。#3851 E） |
| `to_int8` | `int.to_int8(n: Int) -> Int8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove（→ `int8.from`） | R-L0-40 |
| `to_int16` | `int.to_int16(n: Int) -> Int16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove（→ `int16.from`） | R-L0-40 |
| `to_int32` | `int.to_int32(n: Int) -> Int32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove（→ `int32.from`） | R-L0-40 |
| `to_uint8` | `int.to_uint8(n: Int) -> UInt8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove（→ `uint8.from`） | R-L0-40 |
| `to_uint16` | `int.to_uint16(n: Int) -> UInt16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove（→ `uint16.from`） | R-L0-40 |
| `to_uint32` | `int.to_uint32(n: Int) -> UInt32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove（→ `uint32.from`） | R-L0-40 |
| `to_uint64` | `int.to_uint64(n: Int) -> UInt64` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove（→ `uint64.from`） | R-L0-40 |
| `to_float32` | `int.to_float32(n: Int) -> Float32` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | Remove（→ `float32.from`） | R-L0-40 |
| `to_float64` | `int.to_float64(n: Int) -> Float64` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | Remove（→ `float64.from`） | R-L0-40 |
| `to_int64` | `int.to_int64(n: Int) -> Int64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` etc. / Go `T(x)` | Remove（→ `int64.from`） | R-L0-40 |
| `from_int8` | `int.from_int8(n: Int8) -> Int` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` etc. / Go `T(x)` | Remove | R-sized-01（`int8.to_int` is the new location.R-L0-07） |
| `from_int16` | `int.from_int16(n: Int16) -> Int` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` etc. / Go `T(x)` | Remove | R-sized-01（`int16.to_int` is the new location.R-L0-07） |
| `from_int32` | `int.from_int32(n: Int32) -> Int` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` etc. / Go `T(x)` | Remove | R-sized-01（`int32.to_int` is the new location.R-L0-07） |
| `from_int64` | `int.from_int64(n: Int64) -> Int` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` etc. / Go `T(x)` | Remove | R-sized-01（`int64.to_int` is the new location.R-L0-07） |
| `from_uint8` | `int.from_uint8(n: UInt8) -> Int` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` etc. / Go `T(x)` | Remove | R-sized-01（`uint8.to_int` is the new location.R-L0-07） |
| `from_uint16` | `int.from_uint16(n: UInt16) -> Int` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` etc. / Go `T(x)` | Remove | R-sized-01（`uint16.to_int` is the new location.R-L0-07） |
| `from_uint32` | `int.from_uint32(n: UInt32) -> Int` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` etc. / Go `T(x)` | Remove | R-sized-01（`uint32.to_int` is the new location.R-L0-07） |
| `from_uint64` | `int.from_uint64(n: UInt64) -> Int` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove | R-sized-01（`uint64.to_int` is the new location.R-L0-07） |
| `to_int8_checked` | `int.to_int8_checked(n: Int) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / other six languages — | Remove（→ `int8.from_checked`） | R-L0-40 |
| `to_int16_checked` | `int.to_int16_checked(n: Int) -> Option[Int16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / other six languages — | Remove（→ `int16.from_checked`） | R-L0-40 |
| `to_int32_checked` | `int.to_int32_checked(n: Int) -> Option[Int32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / other six languages — | Remove（→ `int32.from_checked`） | R-L0-40 |
| `to_uint8_checked` | `int.to_uint8_checked(n: Int) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / other six languages — | Remove（→ `uint8.from_checked`） | R-L0-40 |
| `to_uint16_checked` | `int.to_uint16_checked(n: Int) -> Option[UInt16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / other six languages — | Remove（→ `uint16.from_checked`） | R-L0-40 |
| `to_uint32_checked` | `int.to_uint32_checked(n: Int) -> Option[UInt32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / other six languages — | Remove（→ `uint32.from_checked`） | R-L0-40 |
| `to_uint64_checked` | `int.to_uint64_checked(n: Int) -> Option[UInt64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / other six languages — | Remove（→ `uint64.from_checked`） | R-L0-40 |
| `to_float32_checked` | `int.to_float32_checked(n: Int) -> Option[Float32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / other six languages — | Remove（→ `float32.from_checked`） | R-L0-40 |
| `to_int8_saturating` | `int.to_int8_saturating(n: Int) -> Int8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / other seven languages — | Remove（→ `int8.from_saturating`） | R-L0-40 |
| `to_int16_saturating` | `int.to_int16_saturating(n: Int) -> Int16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / other seven languages — | Remove（→ `int16.from_saturating`） | R-L0-40 |
| `to_int32_saturating` | `int.to_int32_saturating(n: Int) -> Int32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / other seven languages — | Remove（→ `int32.from_saturating`） | R-L0-40 |
| `to_uint8_saturating` | `int.to_uint8_saturating(n: Int) -> UInt8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / other seven languages — | Remove（→ `uint8.from_saturating`） | R-L0-40 |
| `to_uint16_saturating` | `int.to_uint16_saturating(n: Int) -> UInt16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / other seven languages — | Remove（→ `uint16.from_saturating`） | R-L0-40 |
| `to_uint32_saturating` | `int.to_uint32_saturating(n: Int) -> UInt32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / other seven languages — | Remove（→ `uint32.from_saturating`） | R-L0-40 |
| `to_uint64_saturating` | `int.to_uint64_saturating(n: Int) -> UInt64` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / other seven languages — | Remove（→ `uint64.from_saturating`） | R-L0-40 |
| `from_uint64_checked` | `int.from_uint64_checked(n: UInt64) -> Option[Int]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / other six languages — | Remove | R-sized-01（`uint64.to_int_checked`） |
| `from_uint64_saturating` | `int.from_uint64_saturating(n: UInt64) -> Int` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / other seven languages — | Remove | R-sized-01（`uint64.to_int_saturating`） |
| `min_value` | `int.min_value() -> Int` | Rust `i64::MIN` / Go `math.MinInt64` / OCaml `min_int` (F) / Kotlin `MIN_VALUE` (F) / Swift `.min` | Retain | — |
| `max_value` | `int.max_value() -> Int` | Rust `i64::MAX` / Go `math.MaxInt64` / OCaml `max_int` (F) / Kotlin `MAX_VALUE` (F) / Swift `.max` | Retain | — |
| `wrap` | （Add）`int.wrap(n: Int, bits: Int) -> Int` | Rust `as uN` family / JS `BigInt.asUintN(bits, n)` / others — | Add | R-int-02 |
| `wrap_sub` | （Add）`int.wrap_sub(a: Int, b: Int) -> Int` | Rust `wrapping_sub` / Swift `&-` / others — | Add | R-int-02、R-int-12 |
| `pow` | （Add）`int.pow(base: Int, exp: Int) -> Int` | Rust `pow` / Gleam `power`（Float）(F) / Elixir `Integer.pow` (F) / Python `**` | Add | R-math-01 |
| `sign` | （Add）`int.sign(n: Int) -> Int` | Rust `signum` / Kotlin `sign` (F) / JS `Math.sign` / Swift `signum()` | Add | R-math-01 |
| `modulo` | （Add）`int.modulo(a: Int, b: Int) -> Int` | Python `%` / Gleam `modulo` (F) / Elixir `Integer.mod` (F) / Kotlin `mod` (F) / Rust `rem_euclid` | Add | R-int-03 |
| `floor_div` | （Add）`int.floor_div(a: Int, b: Int) -> Int` | Python `//` / Gleam `floor_divide` (F) / Elixir `Integer.floor_div` (F) / Kotlin `floorDiv` (F) / OCaml `fdiv` (F) | Add | R-int-03 |
| `to_string_radix` | （Add）`int.to_string_radix(n: Int, radix: Int) -> String` | Go `FormatInt(n, base)` / JS `toString(radix)` / Gleam `to_base_string` (F) / Elixir `to_string(n, base)` (F) / Kotlin `toString(radix)` / Swift `String(n, radix:)` | Add | R-int-04 |
| `parse_radix` | （Add）`int.parse_radix(s: String, radix: Int) -> Result[Int, String]` | Rust `from_str_radix` / Go `ParseInt(s, base, 64)` / Python `int(s, base)` / Gleam `base_parse` (F) / Elixir `parse(s, base)` (F) / Kotlin `toIntOrNull(radix)` / Swift `Int(s, radix:)` | Add | R-int-04 |
| `compare` | （Add）`int.compare(a: Int, b: Int) -> Int` | Rust `cmp` / Go `cmp.Compare` / Gleam `compare`（Order）(F) / OCaml `compare` (F) / Kotlin `compareTo` (F) / Swift `<` | Add | R-sized-02 |
| `from` | （Add）`int.from[S: Numeric](x: S) -> Int` | Rust `From`・`as` / Swift `init(_:)` / Python `int(x)` | Add | R-L0-40 |
| `from_checked` | （Add）`int.from_checked[S: Numeric](x: S) -> Int?` | Rust `TryFrom` / Swift `init(exactly:)` | Add | R-L0-40 |
| `from_saturating` | （Add）`int.from_saturating[S: Numeric](x: S) -> Int` | Swift `init(clamping:)` / Rust `as`(from floating point) | Add | R-L0-40 |
| `from_wrapping` | （Add）`int.from_wrapping[S: Integer](x: S) -> Int` | Swift `init(truncatingIfNeeded:)` / Rust `as` | Add | R-L0-40 |
| `bshr_logical` | （Add）`int.bshr_logical(a: Int, n: Int) -> Int` | OCaml `shift_right_logical` (F) / Kotlin `ushr` (F) / JS `>>>` | Add | R-int-06 |

