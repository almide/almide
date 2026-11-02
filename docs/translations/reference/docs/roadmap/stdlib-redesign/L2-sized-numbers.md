> Description (fixed history): Stdlib redesign L2/L3 — int8…uint64, float32, float64: one conversion matrix across twelve carriers, source-side conversions, parse/compare/to_string on every carrier, IEEE cells on every float carrier.
<a id="l2l3-固定幅の数int8-int16-int32-int64-uint8-uint16-uint32-uint64-float32-float64"></a>

# L2/L3: Sized numbers (int8 int16 int32 int64 uint8 uint16 uint32 uint64 float32 float64)

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-sized-numbers.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-sized-numbers.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-sized-numbers.zh-CN.md)

> Non-normative historical redesign snapshot. The pinned original remains authoritative and unchanged; no automatic synchronization, fresh official retrieval or independent execution. (F), measurements, counts and comparative implementation assertions are the original author’s historical reports. Preserve the early source-side to_ direction versus later R-L0-40 destination-side from table; wrapping/saturating plain conversions versus later abort rules; canceled removal of saturating cells; rejected arithmetic and predicates versus subsequent protocol requirements. Early function counts need not match later appended rows. The source’s general claims about integer is_nan and unchanged rounding after widening are retained, not independently validated or asserted as universal numerical guarantees. Japanese literal placeholders mean type, destination, source and integer. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md)

> Last updated: 2026-10-21
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md) / [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md) (R-L1-20: do not merge modules).
> Related: [L2-int](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-int.md) / [L2-float](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-float.md)
> Sources: Rust https://doc.rust-lang.org/std/primitive.i8.html and https://doc.rust-lang.org/std/convert/trait.TryFrom.html / Go https://go.dev/ref/spec#Conversions and https://pkg.go.dev/strconv / Python https://docs.python.org/3/library/struct.html / TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/BigInt/asIntN / Gleam https://gleam-stdlib.hexdocs.pm/gleam/int.html (F) / Elixir https://elixir.hexdocs.pm/Integer.html (F) / OCaml https://ocaml.org/manual/latest/api/Int32.html / Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin/-long/ (F) / Swift https://developer.apple.com/documentation/swift/binaryinteger/init(truncatingifneeded:)

(F) = spellings checked by retrieving official pages on 2026-10-21. Swift `init(truncatingIfNeeded:)` / `init(clamping:)` / `init(exactly:)` and Kotlin `Double.toInt()` saturation were already (F)-checked in [facts/naming-conventions.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/facts/naming-conventions.md) §11.

<a id="l2-役割の比較"></a>

## L2: Role comparison

| Role | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| Sized types | 8 modules + `float32` `float64` | `i8`〜`u64` `f32` `f64` | `int8`〜`uint64` `float32` | — (`struct` type codes only) | Typed arrays / `BigInt.asIntN` | — | — | `Int32` `Int64` | `Byte`〜`ULong` `Float` | `Int8`〜`UInt64` `Float` |
| Lossy conversions (unsuffixed; Almide aborts under R-L0-32, wrapping uses R-L0-34 `_wrapping`) | `to_<型>` | `as` | `T(x)` | — | `asIntN` | — | — | `of_int` | `toByte()` etc. | `init(truncatingIfNeeded:)` (F) |
| none out of range | `to_<型>_checked` | `try_from` | — | — | — | — | — | Base `to_int_exn` family | — | `init(exactly:)` (F) |
| Clamp out-of-range to endpoints | `to_<型>_saturating` | Integers — (only arithmetic `saturating_*`), float→int `as` | — | — | — | — | — | — | `coerceIn` | `init(clamping:)` (F) |
| Default `Int` / `Float` destination | `int.from_<型>`、`float.from_float32/64` (destination side) | `i64::from` / `as` | `int64(x)` | — | — | — | — | `Int32.to_int` | `toLong()` | `Int(x)` |
| To string | `to_string` | `to_string` | `FormatInt` | — | — | — | — | `Int32.to_string` | `toString` | `String(x)` |
| From string | — | `parse::<i8>` | `ParseInt(s, 10, 8)` | — | — | — | — | `Int32.of_string_opt` | `toByteOrNull` | `Int8(s)` |
| Comparison | Operators only | `cmp` | `cmp.Compare` | — | — | — | — | `Int32.compare` | `compareTo` | `<` |
| Lower/upper bounds | Integers `min_value` `max_value`, floats — | `MIN` `MAX` | `math.MaxInt8` etc. | — | — | — | — | `Int32.min_int` | `MIN_VALUE` | `.min` `.max` |
| Bit patterns (floats) | — | `f32::to_bits` `from_bits` | `Float32bits` | `struct` | `DataView` | — | — | `Int32.bits_of_float` | `toBits` | `bitPattern` |

<a id="完全性の規則変換の行列"></a>

### Completeness rules (conversion matrix)

Twelve carriers: `Int`, `Int8`, `Int16`, `Int32`, `Int64`, `UInt8`, `UInt16`, `UInt32`, `UInt64`, `Float`, `Float32`, `Float64`. Modules respectively `int`, `int8` … `uint64`, `float`, `float32`, `float64`.

1. **Direction** (R-L0-07): source modules own `to_<先>`. No destination-side `from_<元>` (remove `int.from_int8`, `float.from_int`, `float.from_float32`, `float.from_float64`).
2. **plain**: every ordered pair of distinct carriers (12 × 11 = 132) has `to_<先>`.
3. **`_checked`**: only potentially lossy pairs where source values may not be exactly representable. Integer→integer when ranges do not fit; integer→float when source significant bits exceed mantissa (24 for Float32, 53 for Float64/Float); always float→integer; Float64/Float→Float32. No lossless pairs (always-some functions are noise).
4. **`_saturating`**: only integer→integer range-narrowing pairs. Float→integer `to_<先>` already saturates (Rust `as`, Kotlin `toInt()`, NaN → 0), so no `_saturating` (R-sized-04). No integer→float/float→float saturation because overflow has value `inf`.
5. **Every carrier**: `to_string`, `parse` (`Result[T, String]`, err out of range), `compare` (`-> Int`, IEEE totalOrder for floats).
6. **Integer carriers**: `min_value`, `max_value`.
7. **Floating carriers** (`float`, `float32`, `float64`): `to_bits`, `from_bits`, `infinity`, `nan`, `epsilon`, `max_value`, `min_value`.
8. **Intentional omissions**: sized arithmetic functions (`abs`, `min`, `max`, `pow`, bit operations, etc.). Sized types are for storage/boundaries; operators (`+` `-` `*` `/` `%` `<`) are available. For other computation, use `to_int` then `int` functions on `Int`.

Under these rules, 52 matrix cells are missing and eight extra (below).

| Module | Missing cells (R-sized-01) | Extra cells (R-sized-04) |
|---|---|---|
| `int` | `to_float_checked` `to_float64_checked` | — |
| `int8` `int16` `uint8` `uint16` | `to_int` `to_float` | — |
| `int32` `uint32` | `to_int` `to_float` `to_float32_checked` | — |
| `int64` | `to_int` `to_float` `to_float_checked` `to_float32_checked` `to_float64_checked` | — |
| `uint64` | `to_int` `to_int_checked` `to_int_saturating` `to_float` `to_float_checked` `to_float32_checked` `to_float64_checked` | — |
| `float` | `to_int_checked` | `to_int8_saturating` … `to_uint64_saturating` (8) |
| `float32` | `to_int` `to_float` and for nine integer types: `_checked` (`to_int_checked` `to_int8_checked` … `to_uint64_checked`) | — |
| `float64` | `to_int` `to_float` `to_float32_checked` and for nine integer types: `_checked` | — |

Current gate (`check_numeric_conversion_matrix` in `src/cli/docs_gen.rs`, #956) examines only nine integer carriers. Excluding the three floats explains why `float32`/`float64` have no `_checked` functions.

<a id="l2-の判定"></a>

### L2 verdict

- Direction duplication: ten `int.from_<型>` and three `float.from_*` functions → R-sized-01 (same issue as R-int-01/R-float-01).
- Family holes: 52 cells above → R-sized-01; `parse` (10 modules), `compare` (12 modules) → R-sized-02; seven float cells (14 across float32/float64) → R-sized-03.
- Synonyms: eight `float.to_<整数>_saturating` functions → R-sized-04.
- No module merger: R-L1-20 (R-sized-05 rejected).

<a id="案"></a>

## Proposals

<a id="r-sized-01-変換の行列を-12-個の運び手に広げ向きを元の型の側にそろえる---採用"></a>

### R-sized-01 Extend conversion matrix to twelve carriers, align on source modules — ○ Accepted

- Content: add 52 cells above; remove `int.from_int8` … `int.from_uint64`, `int.from_uint64_checked`, `int.from_uint64_saturating`, `float.from_int`, `float.from_float32`, `float.from_float64`. Same rules as existing cells: plain integer conversions wrap; float→integer truncates toward zero and saturates, NaN zero; `_checked` returns none if not exactly representable (integer→float rounding yields none, like current `int.to_float32_checked`).
- Sixth-round note: “plain integers wrap” was the adoption-time rule. R-L0-32 changed unsuffixed out-of-range to abort; wrapping moved to R-L0-34 `_wrapping`.
- Supporting L0 rules: R-L0-07, R-L0-08 (fill matrix holes), R-L0-01.
- (a) Swift's three initializers `truncatingIfNeeded:`/`exactly:`/`clamping:` are closest (F). Rust has `as`/`TryFrom`/`From`. Other seven languages have one or two forms, no majority (retain suffix matrix under L0 R-L0-08).
- (b) Pure computation. New cells can be composed in Almide in `stdlib/<型>_convert.almd`, e.g. `float32.to_int16_checked(x) = float.to_int16_checked(float32.to_float(x))`, without intrinsics. Verify equal values on both targets using existing `spec/stdlib/sized_conversion_matrix_test.almd` boundary rows.
- (c) Currently `int8.to_int16` exists but `int8.to_int` does not; `float32.to_int8` exists but `float32.to_int8_checked` does not. Every matrix hole defeats an LLM's rule-based inferred spelling.
- (d) Add 52/remove 13. Deprecations such as `@deprecated(since=15, use="int8.to_int")`, removal epoch 16. One-to-one `almide fix`: `int.from_<T>(x)` → `<T>.to_int(x)`, `int.from_uint64_checked(x)` → `uint64.to_int_checked(x)`, `float.from_int(x)` → `int.to_float(x)`, `float.from_float32(x)` → `float32.to_float(x)`. Counts (spec/tests/docs/stdlib): eight `int.from_<型>` total 70/38/47/362, `from_uint64_checked` 9/2/7/1, `from_uint64_saturating` 4/2/4/0, `float.from_int` 24/11/11/0, `from_float32` 4/2/6/29, `from_float64` 7/0/6/23. Stdlib counts are sized-module bodies' own calls; migrate to zero. Rewrite CHEATSHEET:1068 “Widening is `int.from_<src>(x)`”.
- Gate: add `float`/`float32`/`float64` to `check_numeric_conversion_matrix`; broaden loss test from range to range or mantissa bits. Replace `Int` column's `from_<s>` expectations with `<s>.to_int`; require absence of `int.from_*`/`float.from_*`. Add floating rows to semantics `sized_conversion_matrix_test.almd`.
- Issue: #3935.

<a id="r-sized-02-すべての運び手に-parse-と-compare-を置く---採用"></a>

### R-sized-02 Add `parse` and `compare` to every carrier — ○ Accepted

- Content: `int8.parse(s: String) -> Result[Int8, String]` … `float64.parse` (10 functions; integers use `int.parse` grammar, err out of range; floats use `float.parse` grammar). Add `<mod>.compare(a: T, b: T) -> Int` to all twelve carriers (negative/zero/positive; floats IEEE 754 totalOrder, matching `list.sort`).
- Supporting L0 rules: R-L0-16 (ordered types have `compare`), R-L0-06 item 4 (fallible construction returns Option/Result), R-L0-02 (module of returned type), R-L0-06 (`parse` failures have reasons, so Result).
- (a) All five sized-type languages Rust/Go/OCaml/Kotlin/Swift parse strings per type.
- (b) Pure computation, compose `int.parse` and `_checked`.
- (c) Replace two-step `int.parse(s)! |> int.to_int8_checked` with one function; two steps lose “out of range” reason.
- (d) Add 22, usage count zero.
- Gate: require `to_string`/`parse`/`compare` on every carrier.
- Issue: #3902.

<a id="r-sized-03-float32float64-に浮動小数点の-7-セルを置く---採用"></a>

### R-sized-03 Add seven float cells to `float32`/`float64` — ○ Accepted

- Content: `float32.to_bits(x: Float32) -> Int` (low 32 bits), `float32.from_bits(bits: Int) -> Float32` (reads low 32 bits), `float32.infinity()`, `nan()`, `epsilon()` (2^-23), `max_value()`, `min_value()`. Same seven for `float64` (`to_bits` 64 bits). `float` side: [R-float-02](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-float.md)/[R-float-03](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-float.md).
- Supporting L0 rules: completeness rule 7 (R-L0-08 matrix), R-L0-07 item 2 (constructing bits belongs to destination `from_bits`).
- (a) Rust/Go/Kotlin/Swift have per-type 32-bit float bits/constants.
- (b) Pure computation. `float32.from_bits` succeeds removed `int.bits_to_f32`, using the same intrinsic reading low 32 bits as binary32.
- (c) Current trap: code reading GGUF-style f32 weights cannot find `int.bits_to_f32` by name.
- (d) Add 14, usage zero (5/3/5/1 uses of `int.bits_to_f32` migrate).
- Gate: check seven cells in R-sized-01's gate.
- Issue: #3910.

<a id="r-sized-04-floatto_整数_saturating-8-関数を撤去する---取り消しr-l0-32"></a>

### R-sized-04 Remove eight `float.to_<整数>_saturating` functions — × Canceled (R-L0-32)

- Cancellation (fifth round): R-L0-32 makes unsuffixed `float.to_int8` etc. abort out of range; `_saturating` is no longer a synonym, but the only cell with current saturation. Retain eight functions, add `float.to_int_saturating` for nine.
- Content: remove `float.to_int8_saturating` … `float.to_uint64_saturating`, since `float.to_int8` etc. already saturate identically.
- Supporting L0 rule: R-L0-01 counts public functions calling another public function once as synonyms. Currently allowed by eight rows in `tests/stdlib_synonym_gate_test.rs:47-59`; removal shrinks that allowlist by eight.
- (a) Rust `as` and Kotlin `toInt()` each have only one saturating float→integer spelling.
- (c) Two same-result spellings invite mistaken inference that `to_int8` wraps rather than saturates. Completeness rule 4's “plain float→integer saturates” is a more accurate rule.
- (d) Remove 8. `@deprecated(since=15, use="float.to_int8")`, etc., removal epoch 16. `almide fix` simply drops `_saturating`. Counts: `to_int32_saturating` 5/0/3/0, `to_uint16_saturating` 4/0/3/0; other six 0/0/3/0.
- Gate: float→integer `_saturating` must not exist; delete eight synonym allowlist rows.
- Issue: #3935.

<a id="r-sized-05-固定幅のモジュールをまとめる算術関数を足す---不採用算術の部分は--r-sized-10-で見直し"></a>

### R-sized-05 Merge sized modules/add arithmetic functions — × Rejected (arithmetic revisited by R-sized-10)

- Content: do not merge modules (R-L1-20), nor add arithmetic such as `int8.abs`/`uint32.max` (completeness rule 8).
- Supporting L0 rules: R-L0-02 (subject type's module; Almide has no methods, thus per-type modules), R-L0-01 (do not multiply identical `Int` computations tenfold).
- (c) One arithmetic pattern for sized types: `to_int`, then `int.*`. Full `to_int` matrix (R-sized-01) gives every type the same spelling.
- Issue: none.

<a id="r-sized-06-整数の運び手すべてに-parse_radixto_string_radix-を置く---採用"></a>

### R-sized-06 Add `parse_radix`/`to_string_radix` to every integer carrier — ○ Accepted

- Current: only `int`/`bigint` have `parse_radix(s, radix)`/`to_string_radix(n, radix)`.
- Proposal: add `fn <T>.parse_radix(s: String, radix: Int) -> Result[<T>, String]` and `fn <T>.to_string_radix(x: <T>, radix: Int) -> String` to eight `int8`–`uint64` modules (16 functions), same grammar/radix range as `int.parse_radix`.
- Supporting L0 rule: same reason as R-sized-02 (two stages lose “out of range” reason).
- New evidence against rejected R-sized-05: rule 8's `to_int` then `int.*` fails for `uint64` ≥2^63. Measured: `int.from_hex("ffffffffffffffff")` errors; `int.max(int.from_uint64(uint64.max_value()), 1)` returns `1`.
- (a) Five languages read/write unsigned 64-bit with radix: Rust `u64::from_str_radix` (F), Go `ParseUint`/`FormatUint`, Kotlin `toULong(radix)`, Swift `UInt64(s, radix:)`, Python `int(s, 16)`.
- (c) Hex 64-bit hashes are common; LLMs write `uint64.parse_radix(s, 16)`. Current alternatives silently lose values.
- (d) Add 16.
- Completeness: integer carriers (`int`/`bigint`/eight sized) all have `parse`/`to_string`/`parse_radix`/`to_string_radix`.
- Gate: add two columns to numeric conversion matrix; round-trip `uint64` `max_value` at radix 2/16/36.
- Issue: #3972.

<a id="r-sized-07-float32float64-に-is_nan-などの判定を足す---不採用-r-sized-08-で見直し"></a>

### R-sized-07 Add predicates such as `is_nan` to `float32`/`float64` — × Rejected (→ revisited by R-sized-08)

- Reason: `float32.to_float` is lossless, so rule 8's `to_float` then `float.*` works. Adding predicates repeats boundary decisions about also adding `floor`/`sqrt`. Instead add E002 guidance (R-L0-19): `float32.to_float(x) |> float.is_nan`.
- Issue: none.

<a id="r-sized-08-integerfloatingpoint-を宣言し選択と分類の関数を全型にそろえる---採用r-sized-07-を見直し"></a>

### R-sized-08 Declare `Integer`/`FloatingPoint`, align selection/classification across types — ○ Accepted (revisits R-sized-07)

- Current: comparison/selection/classification missing by type. `int.min`/`max`/`clamp` exist but not `uint64.min` (upper `UInt64` half cannot move to `Int`, so `to_int` then `int.min` cannot substitute). `float32.nan`/`infinity` exist but not `float32.is_nan`. Two external reviews successively flagged this.
- Proposal: declare R-L0-31's two protocols and fill cells.
  - `Integer` (`int` + eight types): `compare`, `min`, `max`, `clamp`, `parse`, `parse_radix`, `to_string`, `to_string_radix`, `min_value`, `max_value`. Add eight types' `min`/`max`/`clamp` (24 functions).
  - `FloatingPoint` (`float`/`float32`/`float64`): `compare`, `min`, `max`, `clamp`, `parse`, `to_string`, `is_nan`, `is_infinite`, `is_finite`, `nan`, `infinity`, `epsilon`, `min_value`, `max_value`. Add two types' `min`/`max`/`clamp`/`is_nan`/`is_infinite`/`is_finite` (12 functions).
  - Boundary: selection/comparison/classification close within a type, so are per-type. Computation (`abs`/`floor`/`round`/`sqrt`, etc.) still uses `to_float` then `float.*` (remaining R-sized-07 reason; computing at higher precision then converting back does not change rounded results).
- Supporting L0 rules: R-L0-31, R-L0-12.
- (a) Swift `BinaryInteger` (`min`/`max` generic), Rust integer types and `f32` all have `min`/`max`/`clamp`/`is_nan`.
- (d) Add 36.
- Issue: #3989.

<a id="r-sized-09-float32float64-に-to_int_saturating-を足す---採用"></a>

### R-sized-09 Add `to_int_saturating` to `float32`/`float64` — ○ Accepted

- Current: R-L0-32 added `float.to_int_saturating`, but only `Int` destinations for `float32`/`float64` were missing (`to_int`/`to_int_checked`/`to_int64_saturating` exist).
- Proposal: add `float32.to_int_saturating(x: Float32) -> Int`, `float64.to_int_saturating(x: Float64) -> Int`.
- Supporting L0 rule: R-L0-32 completeness (three lossy conversions from floats).
- (d) Add two functions.
- Completeness: matrix gate checks all 27 float-three × integer-nine cells have unsuffixed/`_checked`/`_saturating`.
- Issue: #3995.

<a id="r-sized-10-整数-8-型に-integerfixedwidthinteger-の要件をそろえる---採用r-sized-05-を見直し"></a>

### R-sized-10 Complete `Integer`/`FixedWidthInteger` requirements for eight integers — ○ Accepted (revisits R-sized-05)

- Current: `int` has `floor_div`/`modulo`/`pow`/`is_even`/`is_odd`, bit operations `band`/`bor`/`bxor`/`bnot`/`bshl`/`bshr`, `count_leading_zeros`/`count_trailing_zeros`/`count_ones`/`byte_swap`/`abs`/`sign`; `int8`–`uint64` do not.
- Proposal: requirements of [protocol hierarchy](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/protocol-view.md) on all eight types.
  - `Integer`: `floor_div(a, b)`/`modulo(a, b)` (`floor_div(min_value, -1)` aborts like `/`, R-int-13), `pow(base, exp: Int)`, `is_even`, `is_odd`, `band`, `bor`, `bxor`, `bnot`, `bshl(a, n: Int)`, `bshr(a, n: Int)`. `bshr` preserves sign on signed types, zero-fills unsigned (like Swift `>>`).
  - `FixedWidthInteger`: `count_leading_zeros`, `count_trailing_zeros`, `count_ones`, `byte_swap` (count within type width; `int8.count_leading_zeros(1)` is 7).
  - `SignedInteger` (`int8`–`int64`): `abs`, `sign`.
  - Out-of-range results follow the type's `+`/`*` behavior.
- Revisiting R-sized-05 rejection: “do not duplicate `Int` computation tenfold” no longer applies: R-L0-37 supplies one protocol default implementation, not per-type code. Also `to_int` then `int.*` fails for `uint64` ≥2^63 (R-sized-06 measurements).
- Intentional omissions: no `int` N-bit operations `wrap_add(a, b, bits)`/`rotate_left(a, n, bits)`/`bit_width`/`bshr_logical`. Sized types fix width in type, so same-name forms would differ (protocol-view.md “What this hierarchy decides”).
- (a) Swift `BinaryInteger`/`FixedWidthInteger` give this family to every integer type; Rust likewise (`div_euclid`/`rem_euclid`/`pow`/`count_ones`/`leading_zeros`/`swap_bytes`).
- (d) Add 128 (eight types ×15, four signed ×2).
- Issue: #4000.

<a id="r-sized-11-float32float64-に-floatingpoint-の要件をそろえる---採用r-sized-08-の線引きを見直し"></a>

### R-sized-11 Complete `FloatingPoint` requirements for `float32`/`float64` — ○ Accepted (revisits R-sized-08 boundary)

- Current: `float` has `abs`/`floor`/`ceil`/`round`/`trunc`/`signum`, unlike `float32`/`float64`. R-sized-08 said computation uses `to_float` then `float.*`.
- Proposal: give both types protocol `Signed`'s `abs` and `FloatingPoint`'s `floor`/`ceil`/`round`/`trunc`/`signum`; same semantics as `float` counterparts (`round` rounds away from zero).
- Revisit rationale: concern was repeatedly deciding how far to add. Protocol requirements now set the boundary. Math functions such as `sqrt` remain in `math`, not `FloatingPoint` requirements.
- (a) Swift `FloatingPoint` has `rounded`/`magnitude`; Rust `f32` has `abs`/`floor`/`ceil`/`round`/`trunc`/`signum`.
- (d) Add 12.
- Issue: #4000.

<a id="l3-関数表"></a>

## L3: Function table

<a id="l3-int8"></a>

### L3: `int8`

20 current functions and 4 additions.

| Function | Signature (current, mut shown) | Main spellings in other languages | Verdict | Proposal |
|---|---|---|---|---|
| `to_int16` | `int8.to_int16(x: Int8) -> Int16` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` etc. / Go `T(x)` | Remove (→ `int16.from`) | R-L0-40 |
| `to_int32` | `int8.to_int32(x: Int8) -> Int32` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` etc. / Go `T(x)` | Remove (→ `int32.from`) | R-L0-40 |
| `to_int64` | `int8.to_int64(x: Int8) -> Int64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` etc. / Go `T(x)` | Remove (→ `int64.from`) | R-L0-40 |
| `to_uint8` | `int8.to_uint8(x: Int8) -> UInt8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `uint8.from`) | R-L0-40 |
| `to_uint16` | `int8.to_uint16(x: Int8) -> UInt16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `uint16.from`) | R-L0-40 |
| `to_uint32` | `int8.to_uint32(x: Int8) -> UInt32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `uint32.from`) | R-L0-40 |
| `to_uint64` | `int8.to_uint64(x: Int8) -> UInt64` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `uint64.from`) | R-L0-40 |
| `to_float32` | `int8.to_float32(x: Int8) -> Float32` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | Remove (→ `float32.from`) | R-L0-40 |
| `to_float64` | `int8.to_float64(x: Int8) -> Float64` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | Remove (→ `float64.from`) | R-L0-40 |
| `to_string` | `int8.to_string(x: Int8) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | Keep | — |
| `to_uint8_checked` | `int8.to_uint8_checked(x: Int8) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `uint8.from_checked`) | R-L0-40 |
| `to_uint8_saturating` | `int8.to_uint8_saturating(x: Int8) -> UInt8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `uint8.from_saturating`) | R-L0-40 |
| `to_uint16_checked` | `int8.to_uint16_checked(x: Int8) -> Option[UInt16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `uint16.from_checked`) | R-L0-40 |
| `to_uint16_saturating` | `int8.to_uint16_saturating(x: Int8) -> UInt16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `uint16.from_saturating`) | R-L0-40 |
| `to_uint32_checked` | `int8.to_uint32_checked(x: Int8) -> Option[UInt32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `uint32.from_checked`) | R-L0-40 |
| `to_uint32_saturating` | `int8.to_uint32_saturating(x: Int8) -> UInt32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `uint32.from_saturating`) | R-L0-40 |
| `to_uint64_checked` | `int8.to_uint64_checked(x: Int8) -> Option[UInt64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `uint64.from_checked`) | R-L0-40 |
| `to_uint64_saturating` | `int8.to_uint64_saturating(x: Int8) -> UInt64` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `uint64.from_saturating`) | R-L0-40 |
| `min_value` | `int8.min_value() -> Int8` | Rust `MIN` / Go `math.Min…` / Kotlin `MIN_VALUE` / Swift `.min` | Keep | — |
| `max_value` | `int8.max_value() -> Int8` | Rust `MAX` / Go `math.Max…` / Kotlin `MAX_VALUE` / Swift `.max` | Keep | — |
| `parse` |  (Add) `int8.parse(s: String) -> Result[Int8, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` etc. / Swift `T(s)` | Add | R-sized-02 |
| `compare` |  (Add) `int8.compare(a: Int8, b: Int8) -> Int` | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | Add | R-sized-02 |
| `floor_div` |  (Add) `int8.floor_div(a: Int8, b: Int8) -> Int8` | Rust `div_euclid` / Swift — / Python `//` | Add | R-sized-10 |
| `modulo` |  (Add) `int8.modulo(a: Int8, b: Int8) -> Int8` | Rust `rem_euclid` / Python `%` | Add | R-sized-10 |
| `pow` |  (Add) `int8.pow(base: Int8, exp: Int) -> Int8` | Rust `pow` / Swift — | Add | R-sized-10 |
| `is_even` |  (Add) `int8.is_even(n: Int8) -> Bool` | Swift `isMultiple(of: 2)` / Kotlin — | Add | R-sized-10 |
| `is_odd` |  (Add) `int8.is_odd(n: Int8) -> Bool` | Swift `!isMultiple(of: 2)` | Add | R-sized-10 |
| `count_leading_zeros` |  (Add) `int8.count_leading_zeros(n: Int8) -> Int` | Rust `leading_zeros` / Swift `leadingZeroBitCount` | Add | R-sized-10 |
| `count_trailing_zeros` |  (Add) `int8.count_trailing_zeros(n: Int8) -> Int` | Rust `trailing_zeros` / Swift `trailingZeroBitCount` | Add | R-sized-10 |
| `count_ones` |  (Add) `int8.count_ones(n: Int8) -> Int` | Rust `count_ones` / Swift `nonzeroBitCount` | Add | R-sized-10 |
| `byte_swap` |  (Add) `int8.byte_swap(n: Int8) -> Int8` | Rust `swap_bytes` / Swift `byteSwapped` | Add | R-sized-10 |
| `abs` |  (Add) `int8.abs(n: Int8) -> Int8` | Rust `abs` / Swift `abs` | Add | R-sized-10 |
| `sign` |  (Add) `int8.sign(n: Int8) -> Int` | Rust `signum` / Swift `signum()` | Add | R-sized-10 |
| `from` |  (Add) `int8.from[S: Numeric](x: S) -> Int8` | Rust `From` / `as` / Swift `init(_:)` / Python `int(x)` | Add | R-L0-40 |
| `from_checked` |  (Add) `int8.from_checked[S: Numeric](x: S) -> Int8?` | Rust `TryFrom` / Swift `init(exactly:)` | Add | R-L0-40 |
| `from_saturating` |  (Add) `int8.from_saturating[S: Numeric](x: S) -> Int8` | Swift `init(clamping:)` / Rust `as` (from floating-point) | Add | R-L0-40 |
| `from_wrapping` |  (Add) `int8.from_wrapping[S: Integer](x: S) -> Int8` | Swift `init(truncatingIfNeeded:)` / Rust `as` | Add | R-L0-40 |
| `wrap_add` |  (Add) `int8.wrap_add(a: Int8, b: Int8) -> Int8` | Swift `&+` / Rust `wrapping_add` | Add | R-int-12 |
| `wrap_sub` |  (Add) `int8.wrap_sub(a: Int8, b: Int8) -> Int8` | Swift `&-` / Rust `wrapping_sub` | Add | R-int-12 |
| `wrap_mul` |  (Add) `int8.wrap_mul(a: Int8, b: Int8) -> Int8` | Swift `&*` / Rust `wrapping_mul` | Add | R-int-12 |
| `rotate_left` |  (Add) `int8.rotate_left(a: Int8, n: Int) -> Int8` | Rust `rotate_left` / Go `bits.RotateLeft` | Add | R-int-12 |
| `rotate_right` |  (Add) `int8.rotate_right(a: Int8, n: Int) -> Int8` | Rust `rotate_right` | Add | R-int-12 |
| `bit_length` |  (Add) `int8.bit_length(n: Int8) -> Int` | Python `bit_length` / Go `bits.Len` | Add | R-int-11 |
| `band` |  (Add) `int8.band(a: Int8, b: Int8) -> Int8` | Elixir `band` / Rust / Go / Swift operators `&` | Add | R-sized-10 |
| `bor` |  (Add) `int8.bor(a: Int8, b: Int8) -> Int8` | Elixir `bor` / Rust / Go / Swift operators `\|` | Add | R-sized-10 |
| `bxor` |  (Add) `int8.bxor(a: Int8, b: Int8) -> Int8` | Elixir `bxor` / Rust / Go / Swift operators `^` | Add | R-sized-10 |
| `bnot` |  (Add) `int8.bnot(a: Int8) -> Int8` | Elixir `bnot` / Swift `~` | Add | R-sized-10 |
| `bshl` |  (Add) `int8.bshl(a: Int8, n: Int) -> Int8` | Elixir `bsl` / operators `<<` | Add | R-sized-10 |
| `bshr` |  (Add) `int8.bshr(a: Int8, n: Int) -> Int8` | Elixir `bsr` / operators `>>` | Add | R-sized-10 |

<a id="l3-int16"></a>

### L3: `int16`

22 current functions and 4 functions to add.

| Function | Signature (current, mut shown) | Main spellings in other languages | Verdict | Proposal |
|---|---|---|---|---|
| `to_int8` | `int16.to_int8(x: Int16) -> Int8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `int8.from`) | R-L0-40 |
| `to_int32` | `int16.to_int32(x: Int16) -> Int32` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` etc. / Go `T(x)` | Remove (→ `int32.from`) | R-L0-40 |
| `to_int64` | `int16.to_int64(x: Int16) -> Int64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` etc. / Go `T(x)` | Remove (→ `int64.from`) | R-L0-40 |
| `to_uint8` | `int16.to_uint8(x: Int16) -> UInt8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `uint8.from`) | R-L0-40 |
| `to_uint16` | `int16.to_uint16(x: Int16) -> UInt16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `uint16.from`) | R-L0-40 |
| `to_uint32` | `int16.to_uint32(x: Int16) -> UInt32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `uint32.from`) | R-L0-40 |
| `to_uint64` | `int16.to_uint64(x: Int16) -> UInt64` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `uint64.from`) | R-L0-40 |
| `to_float32` | `int16.to_float32(x: Int16) -> Float32` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | Remove (→ `float32.from`) | R-L0-40 |
| `to_float64` | `int16.to_float64(x: Int16) -> Float64` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | Remove (→ `float64.from`) | R-L0-40 |
| `to_string` | `int16.to_string(x: Int16) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | Keep | — |
| `to_int8_checked` | `int16.to_int8_checked(x: Int16) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `int8.from_checked`) | R-L0-40 |
| `to_int8_saturating` | `int16.to_int8_saturating(x: Int16) -> Int8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `int8.from_saturating`) | R-L0-40 |
| `to_uint8_checked` | `int16.to_uint8_checked(x: Int16) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `uint8.from_checked`) | R-L0-40 |
| `to_uint8_saturating` | `int16.to_uint8_saturating(x: Int16) -> UInt8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `uint8.from_saturating`) | R-L0-40 |
| `to_uint16_checked` | `int16.to_uint16_checked(x: Int16) -> Option[UInt16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `uint16.from_checked`) | R-L0-40 |
| `to_uint16_saturating` | `int16.to_uint16_saturating(x: Int16) -> UInt16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `uint16.from_saturating`) | R-L0-40 |
| `to_uint32_checked` | `int16.to_uint32_checked(x: Int16) -> Option[UInt32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `uint32.from_checked`) | R-L0-40 |
| `to_uint32_saturating` | `int16.to_uint32_saturating(x: Int16) -> UInt32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `uint32.from_saturating`) | R-L0-40 |
| `to_uint64_checked` | `int16.to_uint64_checked(x: Int16) -> Option[UInt64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `uint64.from_checked`) | R-L0-40 |
| `to_uint64_saturating` | `int16.to_uint64_saturating(x: Int16) -> UInt64` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `uint64.from_saturating`) | R-L0-40 |
| `min_value` | `int16.min_value() -> Int16` | Rust `MIN` / Go `math.Min…` / Kotlin `MIN_VALUE` / Swift `.min` | Keep | — |
| `max_value` | `int16.max_value() -> Int16` | Rust `MAX` / Go `math.Max…` / Kotlin `MAX_VALUE` / Swift `.max` | Keep | — |
| `parse` |  (Add) `int16.parse(s: String) -> Result[Int16, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` etc. / Swift `T(s)` | Add | R-sized-02 |
| `compare` |  (Add) `int16.compare(a: Int16, b: Int16) -> Int` | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | Add | R-sized-02 |
| `floor_div` |  (Add) `int16.floor_div(a: Int16, b: Int16) -> Int16` | Rust `div_euclid` / Swift — / Python `//` | Add | R-sized-10 |
| `modulo` |  (Add) `int16.modulo(a: Int16, b: Int16) -> Int16` | Rust `rem_euclid` / Python `%` | Add | R-sized-10 |
| `pow` |  (Add) `int16.pow(base: Int16, exp: Int) -> Int16` | Rust `pow` / Swift — | Add | R-sized-10 |
| `is_even` |  (Add) `int16.is_even(n: Int16) -> Bool` | Swift `isMultiple(of: 2)` / Kotlin — | Add | R-sized-10 |
| `is_odd` |  (Add) `int16.is_odd(n: Int16) -> Bool` | Swift `!isMultiple(of: 2)` | Add | R-sized-10 |
| `count_leading_zeros` |  (Add) `int16.count_leading_zeros(n: Int16) -> Int` | Rust `leading_zeros` / Swift `leadingZeroBitCount` | Add | R-sized-10 |
| `count_trailing_zeros` |  (Add) `int16.count_trailing_zeros(n: Int16) -> Int` | Rust `trailing_zeros` / Swift `trailingZeroBitCount` | Add | R-sized-10 |
| `count_ones` |  (Add) `int16.count_ones(n: Int16) -> Int` | Rust `count_ones` / Swift `nonzeroBitCount` | Add | R-sized-10 |
| `byte_swap` |  (Add) `int16.byte_swap(n: Int16) -> Int16` | Rust `swap_bytes` / Swift `byteSwapped` | Add | R-sized-10 |
| `abs` |  (Add) `int16.abs(n: Int16) -> Int16` | Rust `abs` / Swift `abs` | Add | R-sized-10 |
| `sign` |  (Add) `int16.sign(n: Int16) -> Int` | Rust `signum` / Swift `signum()` | Add | R-sized-10 |
| `from` |  (Add) `int16.from[S: Numeric](x: S) -> Int16` | Rust `From` / `as` / Swift `init(_:)` / Python `int(x)` | Add | R-L0-40 |
| `from_checked` |  (Add) `int16.from_checked[S: Numeric](x: S) -> Int16?` | Rust `TryFrom` / Swift `init(exactly:)` | Add | R-L0-40 |
| `from_saturating` |  (Add) `int16.from_saturating[S: Numeric](x: S) -> Int16` | Swift `init(clamping:)` / Rust `as` (from floating-point) | Add | R-L0-40 |
| `from_wrapping` |  (Add) `int16.from_wrapping[S: Integer](x: S) -> Int16` | Swift `init(truncatingIfNeeded:)` / Rust `as` | Add | R-L0-40 |
| `wrap_add` |  (Add) `int16.wrap_add(a: Int16, b: Int16) -> Int16` | Swift `&+` / Rust `wrapping_add` | Add | R-int-12 |
| `wrap_sub` |  (Add) `int16.wrap_sub(a: Int16, b: Int16) -> Int16` | Swift `&-` / Rust `wrapping_sub` | Add | R-int-12 |
| `wrap_mul` |  (Add) `int16.wrap_mul(a: Int16, b: Int16) -> Int16` | Swift `&*` / Rust `wrapping_mul` | Add | R-int-12 |
| `rotate_left` |  (Add) `int16.rotate_left(a: Int16, n: Int) -> Int16` | Rust `rotate_left` / Go `bits.RotateLeft` | Add | R-int-12 |
| `rotate_right` |  (Add) `int16.rotate_right(a: Int16, n: Int) -> Int16` | Rust `rotate_right` | Add | R-int-12 |
| `bit_length` |  (Add) `int16.bit_length(n: Int16) -> Int` | Python `bit_length` / Go `bits.Len` | Add | R-int-11 |
| `band` |  (Add) `int16.band(a: Int16, b: Int16) -> Int16` | Elixir `band` / Rust / Go / Swift operators `&` | Add | R-sized-10 |
| `bor` |  (Add) `int16.bor(a: Int16, b: Int16) -> Int16` | Elixir `bor` / Rust / Go / Swift operators `\|` | Add | R-sized-10 |
| `bxor` |  (Add) `int16.bxor(a: Int16, b: Int16) -> Int16` | Elixir `bxor` / Rust / Go / Swift operators `^` | Add | R-sized-10 |
| `bnot` |  (Add) `int16.bnot(a: Int16) -> Int16` | Elixir `bnot` / Swift `~` | Add | R-sized-10 |
| `bshl` |  (Add) `int16.bshl(a: Int16, n: Int) -> Int16` | Elixir `bsl` / operators `<<` | Add | R-sized-10 |
| `bshr` |  (Add) `int16.bshr(a: Int16, n: Int) -> Int16` | Elixir `bsr` / operators `>>` | Add | R-sized-10 |

<a id="l3-int32"></a>

### L3: `int32`

24 current functions and 5 functions to add.

| Function | Signature (current, mut shown) | Main spellings in other languages | Verdict | Proposal |
|---|---|---|---|---|
| `to_int8` | `int32.to_int8(x: Int32) -> Int8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `int8.from`) | R-L0-40 |
| `to_int16` | `int32.to_int16(x: Int32) -> Int16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `int16.from`) | R-L0-40 |
| `to_int64` | `int32.to_int64(x: Int32) -> Int64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` etc. / Go `T(x)` | Remove (→ `int64.from`) | R-L0-40 |
| `to_uint8` | `int32.to_uint8(x: Int32) -> UInt8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `uint8.from`) | R-L0-40 |
| `to_uint16` | `int32.to_uint16(x: Int32) -> UInt16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `uint16.from`) | R-L0-40 |
| `to_uint32` | `int32.to_uint32(x: Int32) -> UInt32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `uint32.from`) | R-L0-40 |
| `to_uint64` | `int32.to_uint64(x: Int32) -> UInt64` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `uint64.from`) | R-L0-40 |
| `to_float32` | `int32.to_float32(x: Int32) -> Float32` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | Remove (→ `float32.from`) | R-L0-40 |
| `to_float64` | `int32.to_float64(x: Int32) -> Float64` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | Remove (→ `float64.from`) | R-L0-40 |
| `to_string` | `int32.to_string(x: Int32) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | Keep | — |
| `to_int8_checked` | `int32.to_int8_checked(x: Int32) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `int8.from_checked`) | R-L0-40 |
| `to_int8_saturating` | `int32.to_int8_saturating(x: Int32) -> Int8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `int8.from_saturating`) | R-L0-40 |
| `to_int16_checked` | `int32.to_int16_checked(x: Int32) -> Option[Int16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `int16.from_checked`) | R-L0-40 |
| `to_int16_saturating` | `int32.to_int16_saturating(x: Int32) -> Int16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `int16.from_saturating`) | R-L0-40 |
| `to_uint8_checked` | `int32.to_uint8_checked(x: Int32) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `uint8.from_checked`) | R-L0-40 |
| `to_uint8_saturating` | `int32.to_uint8_saturating(x: Int32) -> UInt8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `uint8.from_saturating`) | R-L0-40 |
| `to_uint16_checked` | `int32.to_uint16_checked(x: Int32) -> Option[UInt16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `uint16.from_checked`) | R-L0-40 |
| `to_uint16_saturating` | `int32.to_uint16_saturating(x: Int32) -> UInt16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `uint16.from_saturating`) | R-L0-40 |
| `to_uint32_checked` | `int32.to_uint32_checked(x: Int32) -> Option[UInt32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `uint32.from_checked`) | R-L0-40 |
| `to_uint32_saturating` | `int32.to_uint32_saturating(x: Int32) -> UInt32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `uint32.from_saturating`) | R-L0-40 |
| `to_uint64_checked` | `int32.to_uint64_checked(x: Int32) -> Option[UInt64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `uint64.from_checked`) | R-L0-40 |
| `to_uint64_saturating` | `int32.to_uint64_saturating(x: Int32) -> UInt64` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `uint64.from_saturating`) | R-L0-40 |
| `min_value` | `int32.min_value() -> Int32` | Rust `MIN` / Go `math.Min…` / Kotlin `MIN_VALUE` / Swift `.min` | Keep | — |
| `max_value` | `int32.max_value() -> Int32` | Rust `MAX` / Go `math.Max…` / Kotlin `MAX_VALUE` / Swift `.max` | Keep | — |
| `parse` |  (Add) `int32.parse(s: String) -> Result[Int32, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` etc. / Swift `T(s)` | Add | R-sized-02 |
| `compare` |  (Add) `int32.compare(a: Int32, b: Int32) -> Int` | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | Add | R-sized-02 |
| `floor_div` |  (Add) `int32.floor_div(a: Int32, b: Int32) -> Int32` | Rust `div_euclid` / Swift — / Python `//` | Add | R-sized-10 |
| `modulo` |  (Add) `int32.modulo(a: Int32, b: Int32) -> Int32` | Rust `rem_euclid` / Python `%` | Add | R-sized-10 |
| `pow` |  (Add) `int32.pow(base: Int32, exp: Int) -> Int32` | Rust `pow` / Swift — | Add | R-sized-10 |
| `is_even` |  (Add) `int32.is_even(n: Int32) -> Bool` | Swift `isMultiple(of: 2)` / Kotlin — | Add | R-sized-10 |
| `is_odd` |  (Add) `int32.is_odd(n: Int32) -> Bool` | Swift `!isMultiple(of: 2)` | Add | R-sized-10 |
| `count_leading_zeros` |  (Add) `int32.count_leading_zeros(n: Int32) -> Int` | Rust `leading_zeros` / Swift `leadingZeroBitCount` | Add | R-sized-10 |
| `count_trailing_zeros` |  (Add) `int32.count_trailing_zeros(n: Int32) -> Int` | Rust `trailing_zeros` / Swift `trailingZeroBitCount` | Add | R-sized-10 |
| `count_ones` |  (Add) `int32.count_ones(n: Int32) -> Int` | Rust `count_ones` / Swift `nonzeroBitCount` | Add | R-sized-10 |
| `byte_swap` |  (Add) `int32.byte_swap(n: Int32) -> Int32` | Rust `swap_bytes` / Swift `byteSwapped` | Add | R-sized-10 |
| `abs` |  (Add) `int32.abs(n: Int32) -> Int32` | Rust `abs` / Swift `abs` | Add | R-sized-10 |
| `sign` |  (Add) `int32.sign(n: Int32) -> Int` | Rust `signum` / Swift `signum()` | Add | R-sized-10 |
| `from` |  (Add) `int32.from[S: Numeric](x: S) -> Int32` | Rust `From` / `as` / Swift `init(_:)` / Python `int(x)` | Add | R-L0-40 |
| `from_checked` |  (Add) `int32.from_checked[S: Numeric](x: S) -> Int32?` | Rust `TryFrom` / Swift `init(exactly:)` | Add | R-L0-40 |
| `from_saturating` |  (Add) `int32.from_saturating[S: Numeric](x: S) -> Int32` | Swift `init(clamping:)` / Rust `as` (from floating-point) | Add | R-L0-40 |
| `from_wrapping` |  (Add) `int32.from_wrapping[S: Integer](x: S) -> Int32` | Swift `init(truncatingIfNeeded:)` / Rust `as` | Add | R-L0-40 |
| `wrap_add` |  (Add) `int32.wrap_add(a: Int32, b: Int32) -> Int32` | Swift `&+` / Rust `wrapping_add` | Add | R-int-12 |
| `wrap_sub` |  (Add) `int32.wrap_sub(a: Int32, b: Int32) -> Int32` | Swift `&-` / Rust `wrapping_sub` | Add | R-int-12 |
| `wrap_mul` |  (Add) `int32.wrap_mul(a: Int32, b: Int32) -> Int32` | Swift `&*` / Rust `wrapping_mul` | Add | R-int-12 |
| `rotate_left` |  (Add) `int32.rotate_left(a: Int32, n: Int) -> Int32` | Rust `rotate_left` / Go `bits.RotateLeft` | Add | R-int-12 |
| `rotate_right` |  (Add) `int32.rotate_right(a: Int32, n: Int) -> Int32` | Rust `rotate_right` | Add | R-int-12 |
| `bit_length` |  (Add) `int32.bit_length(n: Int32) -> Int` | Python `bit_length` / Go `bits.Len` | Add | R-int-11 |
| `band` |  (Add) `int32.band(a: Int32, b: Int32) -> Int32` | Elixir `band` / Rust / Go / Swift operators `&` | Add | R-sized-10 |
| `bor` |  (Add) `int32.bor(a: Int32, b: Int32) -> Int32` | Elixir `bor` / Rust / Go / Swift operators `\|` | Add | R-sized-10 |
| `bxor` |  (Add) `int32.bxor(a: Int32, b: Int32) -> Int32` | Elixir `bxor` / Rust / Go / Swift operators `^` | Add | R-sized-10 |
| `bnot` |  (Add) `int32.bnot(a: Int32) -> Int32` | Elixir `bnot` / Swift `~` | Add | R-sized-10 |
| `bshl` |  (Add) `int32.bshl(a: Int32, n: Int) -> Int32` | Elixir `bsl` / operators `<<` | Add | R-sized-10 |
| `bshr` |  (Add) `int32.bshr(a: Int32, n: Int) -> Int32` | Elixir `bsr` / operators `>>` | Add | R-sized-10 |

<a id="l3-int64"></a>

### L3: `int64`

26 current functions and 7 functions to add.

| Function | Signature (current, mut shown) | Main spellings in other languages | Verdict | Proposal |
|---|---|---|---|---|
| `to_int8` | `int64.to_int8(x: Int64) -> Int8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `int8.from`) | R-L0-40 |
| `to_int16` | `int64.to_int16(x: Int64) -> Int16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `int16.from`) | R-L0-40 |
| `to_int32` | `int64.to_int32(x: Int64) -> Int32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `int32.from`) | R-L0-40 |
| `to_uint8` | `int64.to_uint8(x: Int64) -> UInt8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `uint8.from`) | R-L0-40 |
| `to_uint16` | `int64.to_uint16(x: Int64) -> UInt16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `uint16.from`) | R-L0-40 |
| `to_uint32` | `int64.to_uint32(x: Int64) -> UInt32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `uint32.from`) | R-L0-40 |
| `to_uint64` | `int64.to_uint64(x: Int64) -> UInt64` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `uint64.from`) | R-L0-40 |
| `to_float32` | `int64.to_float32(x: Int64) -> Float32` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | Remove (→ `float32.from`) | R-L0-40 |
| `to_float64` | `int64.to_float64(x: Int64) -> Float64` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | Remove (→ `float64.from`) | R-L0-40 |
| `to_string` | `int64.to_string(x: Int64) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | Keep | — |
| `to_int8_checked` | `int64.to_int8_checked(x: Int64) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `int8.from_checked`) | R-L0-40 |
| `to_int8_saturating` | `int64.to_int8_saturating(x: Int64) -> Int8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `int8.from_saturating`) | R-L0-40 |
| `to_int16_checked` | `int64.to_int16_checked(x: Int64) -> Option[Int16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `int16.from_checked`) | R-L0-40 |
| `to_int16_saturating` | `int64.to_int16_saturating(x: Int64) -> Int16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `int16.from_saturating`) | R-L0-40 |
| `to_int32_checked` | `int64.to_int32_checked(x: Int64) -> Option[Int32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `int32.from_checked`) | R-L0-40 |
| `to_int32_saturating` | `int64.to_int32_saturating(x: Int64) -> Int32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `int32.from_saturating`) | R-L0-40 |
| `to_uint8_checked` | `int64.to_uint8_checked(x: Int64) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `uint8.from_checked`) | R-L0-40 |
| `to_uint8_saturating` | `int64.to_uint8_saturating(x: Int64) -> UInt8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `uint8.from_saturating`) | R-L0-40 |
| `to_uint16_checked` | `int64.to_uint16_checked(x: Int64) -> Option[UInt16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `uint16.from_checked`) | R-L0-40 |
| `to_uint16_saturating` | `int64.to_uint16_saturating(x: Int64) -> UInt16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `uint16.from_saturating`) | R-L0-40 |
| `to_uint32_checked` | `int64.to_uint32_checked(x: Int64) -> Option[UInt32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `uint32.from_checked`) | R-L0-40 |
| `to_uint32_saturating` | `int64.to_uint32_saturating(x: Int64) -> UInt32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `uint32.from_saturating`) | R-L0-40 |
| `to_uint64_checked` | `int64.to_uint64_checked(x: Int64) -> Option[UInt64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `uint64.from_checked`) | R-L0-40 |
| `to_uint64_saturating` | `int64.to_uint64_saturating(x: Int64) -> UInt64` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `uint64.from_saturating`) | R-L0-40 |
| `min_value` | `int64.min_value() -> Int64` | Rust `MIN` / Go `math.Min…` / Kotlin `MIN_VALUE` / Swift `.min` | Keep | — |
| `max_value` | `int64.max_value() -> Int64` | Rust `MAX` / Go `math.Max…` / Kotlin `MAX_VALUE` / Swift `.max` | Keep | — |
| `parse` |  (Add) `int64.parse(s: String) -> Result[Int64, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` etc. / Swift `T(s)` | Add | R-sized-02 |
| `compare` |  (Add) `int64.compare(a: Int64, b: Int64) -> Int` | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | Add | R-sized-02 |
| `floor_div` |  (Add) `int64.floor_div(a: Int64, b: Int64) -> Int64` | Rust `div_euclid` / Swift — / Python `//` | Add | R-sized-10 |
| `modulo` |  (Add) `int64.modulo(a: Int64, b: Int64) -> Int64` | Rust `rem_euclid` / Python `%` | Add | R-sized-10 |
| `pow` |  (Add) `int64.pow(base: Int64, exp: Int) -> Int64` | Rust `pow` / Swift — | Add | R-sized-10 |
| `is_even` |  (Add) `int64.is_even(n: Int64) -> Bool` | Swift `isMultiple(of: 2)` / Kotlin — | Add | R-sized-10 |
| `is_odd` |  (Add) `int64.is_odd(n: Int64) -> Bool` | Swift `!isMultiple(of: 2)` | Add | R-sized-10 |
| `count_leading_zeros` |  (Add) `int64.count_leading_zeros(n: Int64) -> Int` | Rust `leading_zeros` / Swift `leadingZeroBitCount` | Add | R-sized-10 |
| `count_trailing_zeros` |  (Add) `int64.count_trailing_zeros(n: Int64) -> Int` | Rust `trailing_zeros` / Swift `trailingZeroBitCount` | Add | R-sized-10 |
| `count_ones` |  (Add) `int64.count_ones(n: Int64) -> Int` | Rust `count_ones` / Swift `nonzeroBitCount` | Add | R-sized-10 |
| `byte_swap` |  (Add) `int64.byte_swap(n: Int64) -> Int64` | Rust `swap_bytes` / Swift `byteSwapped` | Add | R-sized-10 |
| `abs` |  (Add) `int64.abs(n: Int64) -> Int64` | Rust `abs` / Swift `abs` | Add | R-sized-10 |
| `sign` |  (Add) `int64.sign(n: Int64) -> Int` | Rust `signum` / Swift `signum()` | Add | R-sized-10 |
| `from` |  (Add) `int64.from[S: Numeric](x: S) -> Int64` | Rust `From` / `as` / Swift `init(_:)` / Python `int(x)` | Add | R-L0-40 |
| `from_checked` |  (Add) `int64.from_checked[S: Numeric](x: S) -> Int64?` | Rust `TryFrom` / Swift `init(exactly:)` | Add | R-L0-40 |
| `from_saturating` |  (Add) `int64.from_saturating[S: Numeric](x: S) -> Int64` | Swift `init(clamping:)` / Rust `as` (from floating-point) | Add | R-L0-40 |
| `from_wrapping` |  (Add) `int64.from_wrapping[S: Integer](x: S) -> Int64` | Swift `init(truncatingIfNeeded:)` / Rust `as` | Add | R-L0-40 |
| `wrap_add` |  (Add) `int64.wrap_add(a: Int64, b: Int64) -> Int64` | Swift `&+` / Rust `wrapping_add` | Add | R-int-12 |
| `wrap_sub` |  (Add) `int64.wrap_sub(a: Int64, b: Int64) -> Int64` | Swift `&-` / Rust `wrapping_sub` | Add | R-int-12 |
| `wrap_mul` |  (Add) `int64.wrap_mul(a: Int64, b: Int64) -> Int64` | Swift `&*` / Rust `wrapping_mul` | Add | R-int-12 |
| `rotate_left` |  (Add) `int64.rotate_left(a: Int64, n: Int) -> Int64` | Rust `rotate_left` / Go `bits.RotateLeft` | Add | R-int-12 |
| `rotate_right` |  (Add) `int64.rotate_right(a: Int64, n: Int) -> Int64` | Rust `rotate_right` | Add | R-int-12 |
| `bit_length` |  (Add) `int64.bit_length(n: Int64) -> Int` | Python `bit_length` / Go `bits.Len` | Add | R-int-11 |
| `band` |  (Add) `int64.band(a: Int64, b: Int64) -> Int64` | Elixir `band` / Rust / Go / Swift operators `&` | Add | R-sized-10 |
| `bor` |  (Add) `int64.bor(a: Int64, b: Int64) -> Int64` | Elixir `bor` / Rust / Go / Swift operators `\|` | Add | R-sized-10 |
| `bxor` |  (Add) `int64.bxor(a: Int64, b: Int64) -> Int64` | Elixir `bxor` / Rust / Go / Swift operators `^` | Add | R-sized-10 |
| `bnot` |  (Add) `int64.bnot(a: Int64) -> Int64` | Elixir `bnot` / Swift `~` | Add | R-sized-10 |
| `bshl` |  (Add) `int64.bshl(a: Int64, n: Int) -> Int64` | Elixir `bsl` / operators `<<` | Add | R-sized-10 |
| `bshr` |  (Add) `int64.bshr(a: Int64, n: Int) -> Int64` | Elixir `bsr` / operators `>>` | Add | R-sized-10 |

<a id="l3-uint8"></a>

### L3: `uint8`

14 current functions and 4 functions to add.

| Function | Signature (current, mut shown) | Main spellings in other languages | Verdict | Proposal |
|---|---|---|---|---|
| `to_int8` | `uint8.to_int8(x: UInt8) -> Int8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `int8.from`) | R-L0-40 |
| `to_int16` | `uint8.to_int16(x: UInt8) -> Int16` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` etc. / Go `T(x)` | Remove (→ `int16.from`) | R-L0-40 |
| `to_int32` | `uint8.to_int32(x: UInt8) -> Int32` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` etc. / Go `T(x)` | Remove (→ `int32.from`) | R-L0-40 |
| `to_int64` | `uint8.to_int64(x: UInt8) -> Int64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` etc. / Go `T(x)` | Remove (→ `int64.from`) | R-L0-40 |
| `to_uint16` | `uint8.to_uint16(x: UInt8) -> UInt16` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` etc. / Go `T(x)` | Remove (→ `uint16.from`) | R-L0-40 |
| `to_uint32` | `uint8.to_uint32(x: UInt8) -> UInt32` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` etc. / Go `T(x)` | Remove (→ `uint32.from`) | R-L0-40 |
| `to_uint64` | `uint8.to_uint64(x: UInt8) -> UInt64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` etc. / Go `T(x)` | Remove (→ `uint64.from`) | R-L0-40 |
| `to_float32` | `uint8.to_float32(x: UInt8) -> Float32` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | Remove (→ `float32.from`) | R-L0-40 |
| `to_float64` | `uint8.to_float64(x: UInt8) -> Float64` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | Remove (→ `float64.from`) | R-L0-40 |
| `to_string` | `uint8.to_string(x: UInt8) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | Keep | — |
| `to_int8_checked` | `uint8.to_int8_checked(x: UInt8) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `int8.from_checked`) | R-L0-40 |
| `to_int8_saturating` | `uint8.to_int8_saturating(x: UInt8) -> Int8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `int8.from_saturating`) | R-L0-40 |
| `min_value` | `uint8.min_value() -> UInt8` | Rust `MIN` / Go `math.Min…` / Kotlin `MIN_VALUE` / Swift `.min` | Keep | — |
| `max_value` | `uint8.max_value() -> UInt8` | Rust `MAX` / Go `math.Max…` / Kotlin `MAX_VALUE` / Swift `.max` | Keep | — |
| `parse` |  (Add) `uint8.parse(s: String) -> Result[UInt8, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` etc. / Swift `T(s)` | Add | R-sized-02 |
| `compare` |  (Add) `uint8.compare(a: UInt8, b: UInt8) -> Int` | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | Add | R-sized-02 |
| `floor_div` |  (Add) `uint8.floor_div(a: UInt8, b: UInt8) -> UInt8` | Rust `div_euclid` / Swift — / Python `//` | Add | R-sized-10 |
| `modulo` |  (Add) `uint8.modulo(a: UInt8, b: UInt8) -> UInt8` | Rust `rem_euclid` / Python `%` | Add | R-sized-10 |
| `pow` |  (Add) `uint8.pow(base: UInt8, exp: Int) -> UInt8` | Rust `pow` / Swift — | Add | R-sized-10 |
| `is_even` |  (Add) `uint8.is_even(n: UInt8) -> Bool` | Swift `isMultiple(of: 2)` / Kotlin — | Add | R-sized-10 |
| `is_odd` |  (Add) `uint8.is_odd(n: UInt8) -> Bool` | Swift `!isMultiple(of: 2)` | Add | R-sized-10 |
| `count_leading_zeros` |  (Add) `uint8.count_leading_zeros(n: UInt8) -> Int` | Rust `leading_zeros` / Swift `leadingZeroBitCount` | Add | R-sized-10 |
| `count_trailing_zeros` |  (Add) `uint8.count_trailing_zeros(n: UInt8) -> Int` | Rust `trailing_zeros` / Swift `trailingZeroBitCount` | Add | R-sized-10 |
| `count_ones` |  (Add) `uint8.count_ones(n: UInt8) -> Int` | Rust `count_ones` / Swift `nonzeroBitCount` | Add | R-sized-10 |
| `byte_swap` |  (Add) `uint8.byte_swap(n: UInt8) -> UInt8` | Rust `swap_bytes` / Swift `byteSwapped` | Add | R-sized-10 |
| `from` |  (Add) `uint8.from[S: Numeric](x: S) -> UInt8` | Rust `From` / `as` / Swift `init(_:)` / Python `int(x)` | Add | R-L0-40 |
| `from_checked` |  (Add) `uint8.from_checked[S: Numeric](x: S) -> UInt8?` | Rust `TryFrom` / Swift `init(exactly:)` | Add | R-L0-40 |
| `from_saturating` |  (Add) `uint8.from_saturating[S: Numeric](x: S) -> UInt8` | Swift `init(clamping:)` / Rust `as` (from floating-point) | Add | R-L0-40 |
| `from_wrapping` |  (Add) `uint8.from_wrapping[S: Integer](x: S) -> UInt8` | Swift `init(truncatingIfNeeded:)` / Rust `as` | Add | R-L0-40 |
| `wrap_add` |  (Add) `uint8.wrap_add(a: UInt8, b: UInt8) -> UInt8` | Swift `&+` / Rust `wrapping_add` | Add | R-int-12 |
| `wrap_sub` |  (Add) `uint8.wrap_sub(a: UInt8, b: UInt8) -> UInt8` | Swift `&-` / Rust `wrapping_sub` | Add | R-int-12 |
| `wrap_mul` |  (Add) `uint8.wrap_mul(a: UInt8, b: UInt8) -> UInt8` | Swift `&*` / Rust `wrapping_mul` | Add | R-int-12 |
| `rotate_left` |  (Add) `uint8.rotate_left(a: UInt8, n: Int) -> UInt8` | Rust `rotate_left` / Go `bits.RotateLeft` | Add | R-int-12 |
| `rotate_right` |  (Add) `uint8.rotate_right(a: UInt8, n: Int) -> UInt8` | Rust `rotate_right` | Add | R-int-12 |
| `bit_length` |  (Add) `uint8.bit_length(n: UInt8) -> Int` | Python `bit_length` / Go `bits.Len` | Add | R-int-11 |
| `band` |  (Add) `uint8.band(a: UInt8, b: UInt8) -> UInt8` | Elixir `band` / Rust / Go / Swift operators `&` | Add | R-sized-10 |
| `bor` |  (Add) `uint8.bor(a: UInt8, b: UInt8) -> UInt8` | Elixir `bor` / Rust / Go / Swift operators `\|` | Add | R-sized-10 |
| `bxor` |  (Add) `uint8.bxor(a: UInt8, b: UInt8) -> UInt8` | Elixir `bxor` / Rust / Go / Swift operators `^` | Add | R-sized-10 |
| `bnot` |  (Add) `uint8.bnot(a: UInt8) -> UInt8` | Elixir `bnot` / Swift `~` | Add | R-sized-10 |
| `bshl` |  (Add) `uint8.bshl(a: UInt8, n: Int) -> UInt8` | Elixir `bsl` / operators `<<` | Add | R-sized-10 |
| `bshr` |  (Add) `uint8.bshr(a: UInt8, n: Int) -> UInt8` | Elixir `bsr` / operators `>>` | Add | R-sized-10 |

<a id="l3-uint16"></a>

### L3: `uint16`

18 current functions and 4 functions to add.

| Function | Signature (current, mut shown) | Main spellings in other languages | Verdict | Proposal |
|---|---|---|---|---|
| `to_int8` | `uint16.to_int8(x: UInt16) -> Int8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `int8.from`) | R-L0-40 |
| `to_int16` | `uint16.to_int16(x: UInt16) -> Int16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `int16.from`) | R-L0-40 |
| `to_int32` | `uint16.to_int32(x: UInt16) -> Int32` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` etc. / Go `T(x)` | Remove (→ `int32.from`) | R-L0-40 |
| `to_int64` | `uint16.to_int64(x: UInt16) -> Int64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` etc. / Go `T(x)` | Remove (→ `int64.from`) | R-L0-40 |
| `to_uint8` | `uint16.to_uint8(x: UInt16) -> UInt8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `uint8.from`) | R-L0-40 |
| `to_uint32` | `uint16.to_uint32(x: UInt16) -> UInt32` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` etc. / Go `T(x)` | Remove (→ `uint32.from`) | R-L0-40 |
| `to_uint64` | `uint16.to_uint64(x: UInt16) -> UInt64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` etc. / Go `T(x)` | Remove (→ `uint64.from`) | R-L0-40 |
| `to_float32` | `uint16.to_float32(x: UInt16) -> Float32` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | Remove (→ `float32.from`) | R-L0-40 |
| `to_float64` | `uint16.to_float64(x: UInt16) -> Float64` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | Remove (→ `float64.from`) | R-L0-40 |
| `to_string` | `uint16.to_string(x: UInt16) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | Keep | — |
| `to_int8_checked` | `uint16.to_int8_checked(x: UInt16) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `int8.from_checked`) | R-L0-40 |
| `to_int8_saturating` | `uint16.to_int8_saturating(x: UInt16) -> Int8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `int8.from_saturating`) | R-L0-40 |
| `to_int16_checked` | `uint16.to_int16_checked(x: UInt16) -> Option[Int16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `int16.from_checked`) | R-L0-40 |
| `to_int16_saturating` | `uint16.to_int16_saturating(x: UInt16) -> Int16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `int16.from_saturating`) | R-L0-40 |
| `to_uint8_checked` | `uint16.to_uint8_checked(x: UInt16) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `uint8.from_checked`) | R-L0-40 |
| `to_uint8_saturating` | `uint16.to_uint8_saturating(x: UInt16) -> UInt8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `uint8.from_saturating`) | R-L0-40 |
| `min_value` | `uint16.min_value() -> UInt16` | Rust `MIN` / Go `math.Min…` / Kotlin `MIN_VALUE` / Swift `.min` | Keep | — |
| `max_value` | `uint16.max_value() -> UInt16` | Rust `MAX` / Go `math.Max…` / Kotlin `MAX_VALUE` / Swift `.max` | Keep | — |
| `parse` |  (Add) `uint16.parse(s: String) -> Result[UInt16, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` etc. / Swift `T(s)` | Add | R-sized-02 |
| `compare` |  (Add) `uint16.compare(a: UInt16, b: UInt16) -> Int` | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | Add | R-sized-02 |
| `floor_div` |  (Add) `uint16.floor_div(a: UInt16, b: UInt16) -> UInt16` | Rust `div_euclid` / Swift — / Python `//` | Add | R-sized-10 |
| `modulo` |  (Add) `uint16.modulo(a: UInt16, b: UInt16) -> UInt16` | Rust `rem_euclid` / Python `%` | Add | R-sized-10 |
| `pow` |  (Add) `uint16.pow(base: UInt16, exp: Int) -> UInt16` | Rust `pow` / Swift — | Add | R-sized-10 |
| `is_even` |  (Add) `uint16.is_even(n: UInt16) -> Bool` | Swift `isMultiple(of: 2)` / Kotlin — | Add | R-sized-10 |
| `is_odd` |  (Add) `uint16.is_odd(n: UInt16) -> Bool` | Swift `!isMultiple(of: 2)` | Add | R-sized-10 |
| `count_leading_zeros` |  (Add) `uint16.count_leading_zeros(n: UInt16) -> Int` | Rust `leading_zeros` / Swift `leadingZeroBitCount` | Add | R-sized-10 |
| `count_trailing_zeros` |  (Add) `uint16.count_trailing_zeros(n: UInt16) -> Int` | Rust `trailing_zeros` / Swift `trailingZeroBitCount` | Add | R-sized-10 |
| `count_ones` |  (Add) `uint16.count_ones(n: UInt16) -> Int` | Rust `count_ones` / Swift `nonzeroBitCount` | Add | R-sized-10 |
| `byte_swap` |  (Add) `uint16.byte_swap(n: UInt16) -> UInt16` | Rust `swap_bytes` / Swift `byteSwapped` | Add | R-sized-10 |
| `from` |  (Add) `uint16.from[S: Numeric](x: S) -> UInt16` | Rust `From` / `as` / Swift `init(_:)` / Python `int(x)` | Add | R-L0-40 |
| `from_checked` |  (Add) `uint16.from_checked[S: Numeric](x: S) -> UInt16?` | Rust `TryFrom` / Swift `init(exactly:)` | Add | R-L0-40 |
| `from_saturating` |  (Add) `uint16.from_saturating[S: Numeric](x: S) -> UInt16` | Swift `init(clamping:)` / Rust `as` (from floating-point) | Add | R-L0-40 |
| `from_wrapping` |  (Add) `uint16.from_wrapping[S: Integer](x: S) -> UInt16` | Swift `init(truncatingIfNeeded:)` / Rust `as` | Add | R-L0-40 |
| `wrap_add` |  (Add) `uint16.wrap_add(a: UInt16, b: UInt16) -> UInt16` | Swift `&+` / Rust `wrapping_add` | Add | R-int-12 |
| `wrap_sub` |  (Add) `uint16.wrap_sub(a: UInt16, b: UInt16) -> UInt16` | Swift `&-` / Rust `wrapping_sub` | Add | R-int-12 |
| `wrap_mul` |  (Add) `uint16.wrap_mul(a: UInt16, b: UInt16) -> UInt16` | Swift `&*` / Rust `wrapping_mul` | Add | R-int-12 |
| `rotate_left` |  (Add) `uint16.rotate_left(a: UInt16, n: Int) -> UInt16` | Rust `rotate_left` / Go `bits.RotateLeft` | Add | R-int-12 |
| `rotate_right` |  (Add) `uint16.rotate_right(a: UInt16, n: Int) -> UInt16` | Rust `rotate_right` | Add | R-int-12 |
| `bit_length` |  (Add) `uint16.bit_length(n: UInt16) -> Int` | Python `bit_length` / Go `bits.Len` | Add | R-int-11 |
| `band` |  (Add) `uint16.band(a: UInt16, b: UInt16) -> UInt16` | Elixir `band` / Rust / Go / Swift operators `&` | Add | R-sized-10 |
| `bor` |  (Add) `uint16.bor(a: UInt16, b: UInt16) -> UInt16` | Elixir `bor` / Rust / Go / Swift operators `\|` | Add | R-sized-10 |
| `bxor` |  (Add) `uint16.bxor(a: UInt16, b: UInt16) -> UInt16` | Elixir `bxor` / Rust / Go / Swift operators `^` | Add | R-sized-10 |
| `bnot` |  (Add) `uint16.bnot(a: UInt16) -> UInt16` | Elixir `bnot` / Swift `~` | Add | R-sized-10 |
| `bshl` |  (Add) `uint16.bshl(a: UInt16, n: Int) -> UInt16` | Elixir `bsl` / operators `<<` | Add | R-sized-10 |
| `bshr` |  (Add) `uint16.bshr(a: UInt16, n: Int) -> UInt16` | Elixir `bsr` / operators `>>` | Add | R-sized-10 |

<a id="l3-uint32"></a>

### L3: `uint32`

22 current functions and 5 functions to add.

| Function | Signature (current, mut shown) | Main spellings in other languages | Verdict | Proposal |
|---|---|---|---|---|
| `to_int8` | `uint32.to_int8(x: UInt32) -> Int8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `int8.from`) | R-L0-40 |
| `to_int16` | `uint32.to_int16(x: UInt32) -> Int16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `int16.from`) | R-L0-40 |
| `to_int32` | `uint32.to_int32(x: UInt32) -> Int32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `int32.from`) | R-L0-40 |
| `to_int64` | `uint32.to_int64(x: UInt32) -> Int64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` etc. / Go `T(x)` | Remove (→ `int64.from`) | R-L0-40 |
| `to_uint8` | `uint32.to_uint8(x: UInt32) -> UInt8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `uint8.from`) | R-L0-40 |
| `to_uint16` | `uint32.to_uint16(x: UInt32) -> UInt16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `uint16.from`) | R-L0-40 |
| `to_uint64` | `uint32.to_uint64(x: UInt32) -> UInt64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` etc. / Go `T(x)` | Remove (→ `uint64.from`) | R-L0-40 |
| `to_float32` | `uint32.to_float32(x: UInt32) -> Float32` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | Remove (→ `float32.from`) | R-L0-40 |
| `to_float64` | `uint32.to_float64(x: UInt32) -> Float64` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | Remove (→ `float64.from`) | R-L0-40 |
| `to_string` | `uint32.to_string(x: UInt32) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | Keep | — |
| `to_int8_checked` | `uint32.to_int8_checked(x: UInt32) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `int8.from_checked`) | R-L0-40 |
| `to_int8_saturating` | `uint32.to_int8_saturating(x: UInt32) -> Int8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `int8.from_saturating`) | R-L0-40 |
| `to_int16_checked` | `uint32.to_int16_checked(x: UInt32) -> Option[Int16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `int16.from_checked`) | R-L0-40 |
| `to_int16_saturating` | `uint32.to_int16_saturating(x: UInt32) -> Int16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `int16.from_saturating`) | R-L0-40 |
| `to_int32_checked` | `uint32.to_int32_checked(x: UInt32) -> Option[Int32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `int32.from_checked`) | R-L0-40 |
| `to_int32_saturating` | `uint32.to_int32_saturating(x: UInt32) -> Int32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `int32.from_saturating`) | R-L0-40 |
| `to_uint8_checked` | `uint32.to_uint8_checked(x: UInt32) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `uint8.from_checked`) | R-L0-40 |
| `to_uint8_saturating` | `uint32.to_uint8_saturating(x: UInt32) -> UInt8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `uint8.from_saturating`) | R-L0-40 |
| `to_uint16_checked` | `uint32.to_uint16_checked(x: UInt32) -> Option[UInt16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `uint16.from_checked`) | R-L0-40 |
| `to_uint16_saturating` | `uint32.to_uint16_saturating(x: UInt32) -> UInt16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `uint16.from_saturating`) | R-L0-40 |
| `min_value` | `uint32.min_value() -> UInt32` | Rust `MIN` / Go `math.Min…` / Kotlin `MIN_VALUE` / Swift `.min` | Keep | — |
| `max_value` | `uint32.max_value() -> UInt32` | Rust `MAX` / Go `math.Max…` / Kotlin `MAX_VALUE` / Swift `.max` | Keep | — |
| `parse` |  (Add) `uint32.parse(s: String) -> Result[UInt32, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` etc. / Swift `T(s)` | Add | R-sized-02 |
| `compare` |  (Add) `uint32.compare(a: UInt32, b: UInt32) -> Int` | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | Add | R-sized-02 |
| `floor_div` |  (Add) `uint32.floor_div(a: UInt32, b: UInt32) -> UInt32` | Rust `div_euclid` / Swift — / Python `//` | Add | R-sized-10 |
| `modulo` |  (Add) `uint32.modulo(a: UInt32, b: UInt32) -> UInt32` | Rust `rem_euclid` / Python `%` | Add | R-sized-10 |
| `pow` |  (Add) `uint32.pow(base: UInt32, exp: Int) -> UInt32` | Rust `pow` / Swift — | Add | R-sized-10 |
| `is_even` |  (Add) `uint32.is_even(n: UInt32) -> Bool` | Swift `isMultiple(of: 2)` / Kotlin — | Add | R-sized-10 |
| `is_odd` |  (Add) `uint32.is_odd(n: UInt32) -> Bool` | Swift `!isMultiple(of: 2)` | Add | R-sized-10 |
| `count_leading_zeros` |  (Add) `uint32.count_leading_zeros(n: UInt32) -> Int` | Rust `leading_zeros` / Swift `leadingZeroBitCount` | Add | R-sized-10 |
| `count_trailing_zeros` |  (Add) `uint32.count_trailing_zeros(n: UInt32) -> Int` | Rust `trailing_zeros` / Swift `trailingZeroBitCount` | Add | R-sized-10 |
| `count_ones` |  (Add) `uint32.count_ones(n: UInt32) -> Int` | Rust `count_ones` / Swift `nonzeroBitCount` | Add | R-sized-10 |
| `byte_swap` |  (Add) `uint32.byte_swap(n: UInt32) -> UInt32` | Rust `swap_bytes` / Swift `byteSwapped` | Add | R-sized-10 |
| `from` |  (Add) `uint32.from[S: Numeric](x: S) -> UInt32` | Rust `From` / `as` / Swift `init(_:)` / Python `int(x)` | Add | R-L0-40 |
| `from_checked` |  (Add) `uint32.from_checked[S: Numeric](x: S) -> UInt32?` | Rust `TryFrom` / Swift `init(exactly:)` | Add | R-L0-40 |
| `from_saturating` |  (Add) `uint32.from_saturating[S: Numeric](x: S) -> UInt32` | Swift `init(clamping:)` / Rust `as` (from floating-point) | Add | R-L0-40 |
| `from_wrapping` |  (Add) `uint32.from_wrapping[S: Integer](x: S) -> UInt32` | Swift `init(truncatingIfNeeded:)` / Rust `as` | Add | R-L0-40 |
| `wrap_add` |  (Add) `uint32.wrap_add(a: UInt32, b: UInt32) -> UInt32` | Swift `&+` / Rust `wrapping_add` | Add | R-int-12 |
| `wrap_sub` |  (Add) `uint32.wrap_sub(a: UInt32, b: UInt32) -> UInt32` | Swift `&-` / Rust `wrapping_sub` | Add | R-int-12 |
| `wrap_mul` |  (Add) `uint32.wrap_mul(a: UInt32, b: UInt32) -> UInt32` | Swift `&*` / Rust `wrapping_mul` | Add | R-int-12 |
| `rotate_left` |  (Add) `uint32.rotate_left(a: UInt32, n: Int) -> UInt32` | Rust `rotate_left` / Go `bits.RotateLeft` | Add | R-int-12 |
| `rotate_right` |  (Add) `uint32.rotate_right(a: UInt32, n: Int) -> UInt32` | Rust `rotate_right` | Add | R-int-12 |
| `bit_length` |  (Add) `uint32.bit_length(n: UInt32) -> Int` | Python `bit_length` / Go `bits.Len` | Add | R-int-11 |
| `band` |  (Add) `uint32.band(a: UInt32, b: UInt32) -> UInt32` | Elixir `band` / Rust / Go / Swift operators `&` | Add | R-sized-10 |
| `bor` |  (Add) `uint32.bor(a: UInt32, b: UInt32) -> UInt32` | Elixir `bor` / Rust / Go / Swift operators `\|` | Add | R-sized-10 |
| `bxor` |  (Add) `uint32.bxor(a: UInt32, b: UInt32) -> UInt32` | Elixir `bxor` / Rust / Go / Swift operators `^` | Add | R-sized-10 |
| `bnot` |  (Add) `uint32.bnot(a: UInt32) -> UInt32` | Elixir `bnot` / Swift `~` | Add | R-sized-10 |
| `bshl` |  (Add) `uint32.bshl(a: UInt32, n: Int) -> UInt32` | Elixir `bsl` / operators `<<` | Add | R-sized-10 |
| `bshr` |  (Add) `uint32.bshr(a: UInt32, n: Int) -> UInt32` | Elixir `bsr` / operators `>>` | Add | R-sized-10 |

<a id="l3-uint64"></a>

### L3: `uint64`

26 current functions and 9 functions to add.

| Function | Signature (current, mut shown) | Main spellings in other languages | Verdict | Proposal |
|---|---|---|---|---|
| `to_int8` | `uint64.to_int8(x: UInt64) -> Int8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `int8.from`) | R-L0-40 |
| `to_int16` | `uint64.to_int16(x: UInt64) -> Int16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `int16.from`) | R-L0-40 |
| `to_int32` | `uint64.to_int32(x: UInt64) -> Int32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `int32.from`) | R-L0-40 |
| `to_int64` | `uint64.to_int64(x: UInt64) -> Int64` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `int64.from`) | R-L0-40 |
| `to_uint8` | `uint64.to_uint8(x: UInt64) -> UInt8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `uint8.from`) | R-L0-40 |
| `to_uint16` | `uint64.to_uint16(x: UInt64) -> UInt16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `uint16.from`) | R-L0-40 |
| `to_uint32` | `uint64.to_uint32(x: UInt64) -> UInt32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` etc. / Go `T(x)` | Remove (→ `uint32.from`) | R-L0-40 |
| `to_float32` | `uint64.to_float32(x: UInt64) -> Float32` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | Remove (→ `float32.from`) | R-L0-40 |
| `to_float64` | `uint64.to_float64(x: UInt64) -> Float64` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | Remove (→ `float64.from`) | R-L0-40 |
| `to_string` | `uint64.to_string(x: UInt64) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | Keep | — |
| `to_int8_checked` | `uint64.to_int8_checked(x: UInt64) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `int8.from_checked`) | R-L0-40 |
| `to_int8_saturating` | `uint64.to_int8_saturating(x: UInt64) -> Int8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `int8.from_saturating`) | R-L0-40 |
| `to_int16_checked` | `uint64.to_int16_checked(x: UInt64) -> Option[Int16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `int16.from_checked`) | R-L0-40 |
| `to_int16_saturating` | `uint64.to_int16_saturating(x: UInt64) -> Int16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `int16.from_saturating`) | R-L0-40 |
| `to_int32_checked` | `uint64.to_int32_checked(x: UInt64) -> Option[Int32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `int32.from_checked`) | R-L0-40 |
| `to_int32_saturating` | `uint64.to_int32_saturating(x: UInt64) -> Int32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `int32.from_saturating`) | R-L0-40 |
| `to_int64_checked` | `uint64.to_int64_checked(x: UInt64) -> Option[Int64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `int64.from_checked`) | R-L0-40 |
| `to_int64_saturating` | `uint64.to_int64_saturating(x: UInt64) -> Int64` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `int64.from_saturating`) | R-L0-40 |
| `to_uint8_checked` | `uint64.to_uint8_checked(x: UInt64) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `uint8.from_checked`) | R-L0-40 |
| `to_uint8_saturating` | `uint64.to_uint8_saturating(x: UInt64) -> UInt8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `uint8.from_saturating`) | R-L0-40 |
| `to_uint16_checked` | `uint64.to_uint16_checked(x: UInt64) -> Option[UInt16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `uint16.from_checked`) | R-L0-40 |
| `to_uint16_saturating` | `uint64.to_uint16_saturating(x: UInt64) -> UInt16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `uint16.from_saturating`) | R-L0-40 |
| `to_uint32_checked` | `uint64.to_uint32_checked(x: UInt64) -> Option[UInt32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / 6 other languages — | Remove (→ `uint32.from_checked`) | R-L0-40 |
| `to_uint32_saturating` | `uint64.to_uint32_saturating(x: UInt64) -> UInt32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 7 other languages — | Remove (→ `uint32.from_saturating`) | R-L0-40 |
| `min_value` | `uint64.min_value() -> UInt64` | Rust `MIN` / Go `math.Min…` / Kotlin `MIN_VALUE` / Swift `.min` | Keep | — |
| `max_value` | `uint64.max_value() -> UInt64` | Rust `MAX` / Go `math.Max…` / Kotlin `MAX_VALUE` / Swift `.max` | Keep | — |
| `parse` |  (Add) `uint64.parse(s: String) -> Result[UInt64, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` etc. / Swift `T(s)` | Add | R-sized-02 |
| `compare` |  (Add) `uint64.compare(a: UInt64, b: UInt64) -> Int` | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | Add | R-sized-02 |
| `floor_div` |  (Add) `uint64.floor_div(a: UInt64, b: UInt64) -> UInt64` | Rust `div_euclid` / Swift — / Python `//` | Add | R-sized-10 |
| `modulo` |  (Add) `uint64.modulo(a: UInt64, b: UInt64) -> UInt64` | Rust `rem_euclid` / Python `%` | Add | R-sized-10 |
| `pow` |  (Add) `uint64.pow(base: UInt64, exp: Int) -> UInt64` | Rust `pow` / Swift — | Add | R-sized-10 |
| `is_even` |  (Add) `uint64.is_even(n: UInt64) -> Bool` | Swift `isMultiple(of: 2)` / Kotlin — | Add | R-sized-10 |
| `is_odd` |  (Add) `uint64.is_odd(n: UInt64) -> Bool` | Swift `!isMultiple(of: 2)` | Add | R-sized-10 |
| `count_leading_zeros` |  (Add) `uint64.count_leading_zeros(n: UInt64) -> Int` | Rust `leading_zeros` / Swift `leadingZeroBitCount` | Add | R-sized-10 |
| `count_trailing_zeros` |  (Add) `uint64.count_trailing_zeros(n: UInt64) -> Int` | Rust `trailing_zeros` / Swift `trailingZeroBitCount` | Add | R-sized-10 |
| `count_ones` |  (Add) `uint64.count_ones(n: UInt64) -> Int` | Rust `count_ones` / Swift `nonzeroBitCount` | Add | R-sized-10 |
| `byte_swap` |  (Add) `uint64.byte_swap(n: UInt64) -> UInt64` | Rust `swap_bytes` / Swift `byteSwapped` | Add | R-sized-10 |
| `from` |  (Add) `uint64.from[S: Numeric](x: S) -> UInt64` | Rust `From` / `as` / Swift `init(_:)` / Python `int(x)` | Add | R-L0-40 |
| `from_checked` |  (Add) `uint64.from_checked[S: Numeric](x: S) -> UInt64?` | Rust `TryFrom` / Swift `init(exactly:)` | Add | R-L0-40 |
| `from_saturating` |  (Add) `uint64.from_saturating[S: Numeric](x: S) -> UInt64` | Swift `init(clamping:)` / Rust `as` (from floating-point) | Add | R-L0-40 |
| `from_wrapping` |  (Add) `uint64.from_wrapping[S: Integer](x: S) -> UInt64` | Swift `init(truncatingIfNeeded:)` / Rust `as` | Add | R-L0-40 |
| `wrap_add` |  (Add) `uint64.wrap_add(a: UInt64, b: UInt64) -> UInt64` | Swift `&+` / Rust `wrapping_add` | Add | R-int-12 |
| `wrap_sub` |  (Add) `uint64.wrap_sub(a: UInt64, b: UInt64) -> UInt64` | Swift `&-` / Rust `wrapping_sub` | Add | R-int-12 |
| `wrap_mul` |  (Add) `uint64.wrap_mul(a: UInt64, b: UInt64) -> UInt64` | Swift `&*` / Rust `wrapping_mul` | Add | R-int-12 |
| `rotate_left` |  (Add) `uint64.rotate_left(a: UInt64, n: Int) -> UInt64` | Rust `rotate_left` / Go `bits.RotateLeft` | Add | R-int-12 |
| `rotate_right` |  (Add) `uint64.rotate_right(a: UInt64, n: Int) -> UInt64` | Rust `rotate_right` | Add | R-int-12 |
| `bit_length` |  (Add) `uint64.bit_length(n: UInt64) -> Int` | Python `bit_length` / Go `bits.Len` | Add | R-int-11 |
| `band` |  (Add) `uint64.band(a: UInt64, b: UInt64) -> UInt64` | Elixir `band` / Rust / Go / Swift operators `&` | Add | R-sized-10 |
| `bor` |  (Add) `uint64.bor(a: UInt64, b: UInt64) -> UInt64` | Elixir `bor` / Rust / Go / Swift operators `\|` | Add | R-sized-10 |
| `bxor` |  (Add) `uint64.bxor(a: UInt64, b: UInt64) -> UInt64` | Elixir `bxor` / Rust / Go / Swift operators `^` | Add | R-sized-10 |
| `bnot` |  (Add) `uint64.bnot(a: UInt64) -> UInt64` | Elixir `bnot` / Swift `~` | Add | R-sized-10 |
| `bshl` |  (Add) `uint64.bshl(a: UInt64, n: Int) -> UInt64` | Elixir `bsl` / operators `<<` | Add | R-sized-10 |
| `bshr` |  (Add) `uint64.bshr(a: UInt64, n: Int) -> UInt64` | Elixir `bsr` / operators `>>` | Add | R-sized-10 |

<a id="l3-float32"></a>

### L3: `float32`

10 current functions and 20 functions to add.

| Function | Signature (current, mut shown) | Main spellings in other languages | Verdict | Proposal |
|---|---|---|---|---|
| `to_int8` | `float32.to_int8(x: Float32) -> Int8` | Rust `as` (saturating) / Kotlin `toInt()` (saturating)(F) / Swift `init(_:)` (trap) / Go `T(x)` | Remove (→ `int8.from`) | R-L0-40 |
| `to_int16` | `float32.to_int16(x: Float32) -> Int16` | Rust `as` (saturating) / Kotlin `toInt()` (saturating)(F) / Swift `init(_:)` (trap) / Go `T(x)` | Remove (→ `int16.from`) | R-L0-40 |
| `to_int32` | `float32.to_int32(x: Float32) -> Int32` | Rust `as` (saturating) / Kotlin `toInt()` (saturating)(F) / Swift `init(_:)` (trap) / Go `T(x)` | Remove (→ `int32.from`) | R-L0-40 |
| `to_int64` | `float32.to_int64(x: Float32) -> Int64` | Rust `as` (saturating) / Kotlin `toInt()` (saturating)(F) / Swift `init(_:)` (trap) / Go `T(x)` | Remove (→ `int64.from`) | R-L0-40 |
| `to_uint8` | `float32.to_uint8(x: Float32) -> UInt8` | Rust `as` (saturating) / Kotlin `toInt()` (saturating)(F) / Swift `init(_:)` (trap) / Go `T(x)` | Remove (→ `uint8.from`) | R-L0-40 |
| `to_uint16` | `float32.to_uint16(x: Float32) -> UInt16` | Rust `as` (saturating) / Kotlin `toInt()` (saturating)(F) / Swift `init(_:)` (trap) / Go `T(x)` | Remove (→ `uint16.from`) | R-L0-40 |
| `to_uint32` | `float32.to_uint32(x: Float32) -> UInt32` | Rust `as` (saturating) / Kotlin `toInt()` (saturating)(F) / Swift `init(_:)` (trap) / Go `T(x)` | Remove (→ `uint32.from`) | R-L0-40 |
| `to_uint64` | `float32.to_uint64(x: Float32) -> UInt64` | Rust `as` (saturating) / Kotlin `toInt()` (saturating)(F) / Swift `init(_:)` (trap) / Go `T(x)` | Remove (→ `uint64.from`) | R-L0-40 |
| `to_float64` | `float32.to_float64(x: Float32) -> Float64` | Rust `as` / Kotlin `toFloat()` / Swift `init(_:)` | Remove (→ `float64.from`) | R-L0-40 |
| `to_string` | `float32.to_string(x: Float32) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | Keep | — |
| `parse` |  (Add) `float32.parse(s: String) -> Result[Float32, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` etc. / Swift `T(s)` | Add | R-sized-02 |
| `compare` |  (Add) `float32.compare(a: Float32, b: Float32) -> Int` (IEEE totalOrder) | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | Add | R-sized-02 |
| `to_bits` |  (Add) `float32.to_bits(x: Float32) -> Int` | Rust `to_bits` (F) / Go `math.Float32bits` / Kotlin `toBits` | Add | R-sized-03 |
| `from_bits` |  (Add) `float32.from_bits(bits: Int) -> Float32` | Rust `from_bits` (F) / Go `math.Float32frombits` / Kotlin `fromBits` | Add | R-sized-03 |
| `infinity` |  (Add) `float32.infinity() -> Float32` | Rust `INFINITY` / Swift `.infinity` | Add | R-sized-03 |
| `nan` |  (Add) `float32.nan() -> Float32` | Rust `NAN` / Swift `.nan` | Add | R-sized-03 |
| `epsilon` |  (Add) `float32.epsilon() -> Float32` | Rust `EPSILON` / Swift `.ulpOfOne` | Add | R-sized-03 |
| `max_value` |  (Add) `float32.max_value() -> Float32` | Rust `MAX` / Go `math.MaxFloat32` / Kotlin `MAX_VALUE` | Add | R-sized-03 |
| `min_value` |  (Add) `float32.min_value() -> Float32` | Rust `MIN` / Swift `-.greatestFiniteMagnitude` | Add | R-sized-03 |
| `abs` |  (Add) `float32.abs(x: Float32) -> Float32` | Rust `abs` / Swift `magnitude` | Add | R-sized-11 |
| `floor` |  (Add) `float32.floor(x: Float32) -> Float32` | Rust `floor` / Swift `rounded(.down)` | Add | R-sized-11 |
| `ceil` |  (Add) `float32.ceil(x: Float32) -> Float32` | Rust `ceil` / Swift `rounded(.up)` | Add | R-sized-11 |
| `round` |  (Add) `float32.round(x: Float32) -> Float32` | Rust `round` / Swift `rounded()` | Add | R-sized-11 |
| `trunc` |  (Add) `float32.trunc(x: Float32) -> Float32` | Rust `trunc` / Swift `rounded(.towardZero)` | Add | R-sized-11 |
| `signum` |  (Add) `float32.signum(x: Float32) -> Float32` | Rust `signum` | Add | R-sized-11 |
| `from` |  (Add) `float32.from[S: Numeric](x: S) -> Float32` | Rust `From` / `as` / Swift `init(_:)` / Python `int(x)` | Add | R-L0-40 |
| `from_checked` |  (Add) `float32.from_checked[S: Numeric](x: S) -> Float32?` | Rust `TryFrom` / Swift `init(exactly:)` | Add | R-L0-40 |

<a id="l3-float64"></a>

### L3: `float64`

10 current functions and 21 functions to add.

| Function | Signature (current, mut shown) | Main spellings in other languages | Verdict | Proposal |
|---|---|---|---|---|
| `to_int8` | `float64.to_int8(x: Float64) -> Int8` | Rust `as` (saturating) / Kotlin `toInt()` (saturating)(F) / Swift `init(_:)` (trap) / Go `T(x)` | Remove (→ `int8.from`) | R-L0-40 |
| `to_int16` | `float64.to_int16(x: Float64) -> Int16` | Rust `as` (saturating) / Kotlin `toInt()` (saturating)(F) / Swift `init(_:)` (trap) / Go `T(x)` | Remove (→ `int16.from`) | R-L0-40 |
| `to_int32` | `float64.to_int32(x: Float64) -> Int32` | Rust `as` (saturating) / Kotlin `toInt()` (saturating)(F) / Swift `init(_:)` (trap) / Go `T(x)` | Remove (→ `int32.from`) | R-L0-40 |
| `to_int64` | `float64.to_int64(x: Float64) -> Int64` | Rust `as` (saturating) / Kotlin `toInt()` (saturating)(F) / Swift `init(_:)` (trap) / Go `T(x)` | Remove (→ `int64.from`) | R-L0-40 |
| `to_uint8` | `float64.to_uint8(x: Float64) -> UInt8` | Rust `as` (saturating) / Kotlin `toInt()` (saturating)(F) / Swift `init(_:)` (trap) / Go `T(x)` | Remove (→ `uint8.from`) | R-L0-40 |
| `to_uint16` | `float64.to_uint16(x: Float64) -> UInt16` | Rust `as` (saturating) / Kotlin `toInt()` (saturating)(F) / Swift `init(_:)` (trap) / Go `T(x)` | Remove (→ `uint16.from`) | R-L0-40 |
| `to_uint32` | `float64.to_uint32(x: Float64) -> UInt32` | Rust `as` (saturating) / Kotlin `toInt()` (saturating)(F) / Swift `init(_:)` (trap) / Go `T(x)` | Remove (→ `uint32.from`) | R-L0-40 |
| `to_uint64` | `float64.to_uint64(x: Float64) -> UInt64` | Rust `as` (saturating) / Kotlin `toInt()` (saturating)(F) / Swift `init(_:)` (trap) / Go `T(x)` | Remove (→ `uint64.from`) | R-L0-40 |
| `to_float32` | `float64.to_float32(x: Float64) -> Float32` | Rust `as` / Kotlin `toFloat()` / Swift `init(_:)` | Remove (→ `float32.from`) | R-L0-40 |
| `to_string` | `float64.to_string(x: Float64) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | Keep | — |
| `parse` |  (Add) `float64.parse(s: String) -> Result[Float64, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` etc. / Swift `T(s)` | Add | R-sized-02 |
| `compare` |  (Add) `float64.compare(a: Float64, b: Float64) -> Int` (IEEE totalOrder) | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | Add | R-sized-02 |
| `to_bits` |  (Add) `float64.to_bits(x: Float64) -> Int` | Rust `to_bits` (F) / Go `math.Float64bits` / Kotlin `toBits` | Add | R-sized-03 |
| `from_bits` |  (Add) `float64.from_bits(bits: Int) -> Float64` | Rust `from_bits` (F) / Go `math.Float64frombits` / Kotlin `fromBits` | Add | R-sized-03 |
| `infinity` |  (Add) `float64.infinity() -> Float64` | Rust `INFINITY` / Swift `.infinity` | Add | R-sized-03 |
| `nan` |  (Add) `float64.nan() -> Float64` | Rust `NAN` / Swift `.nan` | Add | R-sized-03 |
| `epsilon` |  (Add) `float64.epsilon() -> Float64` | Rust `EPSILON` / Swift `.ulpOfOne` | Add | R-sized-03 |
| `max_value` |  (Add) `float64.max_value() -> Float64` | Rust `MAX` / Go `math.MaxFloat64` / Kotlin `MAX_VALUE` | Add | R-sized-03 |
| `min_value` |  (Add) `float64.min_value() -> Float64` | Rust `MIN` / Swift `-.greatestFiniteMagnitude` | Add | R-sized-03 |
| `parse_radix` | `int8.parse_radix(s: String, radix: Int) -> Result[Int8, String]` (Proposal) | Rust `from_str_radix` / Go `ParseInt` / `ParseUint` | Add | R-sized-06 |
| `to_string_radix` | `int8.to_string_radix(x: Int8, radix: Int) -> String` (Proposal) | Rust `format!("{:x}")` / Go `FormatInt` / `FormatUint` | Add | R-sized-06 |
| `parse_radix` | `int16.parse_radix(s: String, radix: Int) -> Result[Int16, String]` (Proposal) | Rust `from_str_radix` / Go `ParseInt` / `ParseUint` | Add | R-sized-06 |
| `to_string_radix` | `int16.to_string_radix(x: Int16, radix: Int) -> String` (Proposal) | Rust `format!("{:x}")` / Go `FormatInt` / `FormatUint` | Add | R-sized-06 |
| `parse_radix` | `int32.parse_radix(s: String, radix: Int) -> Result[Int32, String]` (Proposal) | Rust `from_str_radix` / Go `ParseInt` / `ParseUint` | Add | R-sized-06 |
| `to_string_radix` | `int32.to_string_radix(x: Int32, radix: Int) -> String` (Proposal) | Rust `format!("{:x}")` / Go `FormatInt` / `FormatUint` | Add | R-sized-06 |
| `parse_radix` | `int64.parse_radix(s: String, radix: Int) -> Result[Int64, String]` (Proposal) | Rust `from_str_radix` / Go `ParseInt` / `ParseUint` | Add | R-sized-06 |
| `to_string_radix` | `int64.to_string_radix(x: Int64, radix: Int) -> String` (Proposal) | Rust `format!("{:x}")` / Go `FormatInt` / `FormatUint` | Add | R-sized-06 |
| `parse_radix` | `uint8.parse_radix(s: String, radix: Int) -> Result[UInt8, String]` (Proposal) | Rust `from_str_radix` / Go `ParseInt` / `ParseUint` | Add | R-sized-06 |
| `to_string_radix` | `uint8.to_string_radix(x: UInt8, radix: Int) -> String` (Proposal) | Rust `format!("{:x}")` / Go `FormatInt` / `FormatUint` | Add | R-sized-06 |
| `parse_radix` | `uint16.parse_radix(s: String, radix: Int) -> Result[UInt16, String]` (Proposal) | Rust `from_str_radix` / Go `ParseInt` / `ParseUint` | Add | R-sized-06 |
| `to_string_radix` | `uint16.to_string_radix(x: UInt16, radix: Int) -> String` (Proposal) | Rust `format!("{:x}")` / Go `FormatInt` / `FormatUint` | Add | R-sized-06 |
| `parse_radix` | `uint32.parse_radix(s: String, radix: Int) -> Result[UInt32, String]` (Proposal) | Rust `from_str_radix` / Go `ParseInt` / `ParseUint` | Add | R-sized-06 |
| `to_string_radix` | `uint32.to_string_radix(x: UInt32, radix: Int) -> String` (Proposal) | Rust `format!("{:x}")` / Go `FormatInt` / `FormatUint` | Add | R-sized-06 |
| `parse_radix` | `uint64.parse_radix(s: String, radix: Int) -> Result[UInt64, String]` (Proposal) | Rust `from_str_radix` / Go `ParseInt` / `ParseUint` | Add | R-sized-06 |
| `to_string_radix` | `uint64.to_string_radix(x: UInt64, radix: Int) -> String` (Proposal) | Rust `format!("{:x}")` / Go `FormatInt` / `FormatUint` | Add | R-sized-06 |
| `min` |  (Add) `int8.min(a: Int8, b: Int8) -> Int8` | Rust `min` / Swift `min` | Add | R-sized-08 |
| `max` |  (Add) `int8.max(a: Int8, b: Int8) -> Int8` | Rust `max` / Swift `max` | Add | R-sized-08 |
| `clamp` |  (Add) `int8.clamp(x: Int8, lo: Int8, hi: Int8) -> Int8` | Rust `clamp` | Add | R-sized-08 |
| `min` |  (Add) `int16.min(a: Int16, b: Int16) -> Int16` | Rust `min` / Swift `min` | Add | R-sized-08 |
| `max` |  (Add) `int16.max(a: Int16, b: Int16) -> Int16` | Rust `max` / Swift `max` | Add | R-sized-08 |
| `clamp` |  (Add) `int16.clamp(x: Int16, lo: Int16, hi: Int16) -> Int16` | Rust `clamp` | Add | R-sized-08 |
| `min` |  (Add) `int32.min(a: Int32, b: Int32) -> Int32` | Rust `min` / Swift `min` | Add | R-sized-08 |
| `max` |  (Add) `int32.max(a: Int32, b: Int32) -> Int32` | Rust `max` / Swift `max` | Add | R-sized-08 |
| `clamp` |  (Add) `int32.clamp(x: Int32, lo: Int32, hi: Int32) -> Int32` | Rust `clamp` | Add | R-sized-08 |
| `min` |  (Add) `int64.min(a: Int64, b: Int64) -> Int64` | Rust `min` / Swift `min` | Add | R-sized-08 |
| `max` |  (Add) `int64.max(a: Int64, b: Int64) -> Int64` | Rust `max` / Swift `max` | Add | R-sized-08 |
| `clamp` |  (Add) `int64.clamp(x: Int64, lo: Int64, hi: Int64) -> Int64` | Rust `clamp` | Add | R-sized-08 |
| `min` |  (Add) `uint8.min(a: UInt8, b: UInt8) -> UInt8` | Rust `min` / Swift `min` | Add | R-sized-08 |
| `max` |  (Add) `uint8.max(a: UInt8, b: UInt8) -> UInt8` | Rust `max` / Swift `max` | Add | R-sized-08 |
| `clamp` |  (Add) `uint8.clamp(x: UInt8, lo: UInt8, hi: UInt8) -> UInt8` | Rust `clamp` | Add | R-sized-08 |
| `min` |  (Add) `uint16.min(a: UInt16, b: UInt16) -> UInt16` | Rust `min` / Swift `min` | Add | R-sized-08 |
| `max` |  (Add) `uint16.max(a: UInt16, b: UInt16) -> UInt16` | Rust `max` / Swift `max` | Add | R-sized-08 |
| `clamp` |  (Add) `uint16.clamp(x: UInt16, lo: UInt16, hi: UInt16) -> UInt16` | Rust `clamp` | Add | R-sized-08 |
| `min` |  (Add) `uint32.min(a: UInt32, b: UInt32) -> UInt32` | Rust `min` / Swift `min` | Add | R-sized-08 |
| `max` |  (Add) `uint32.max(a: UInt32, b: UInt32) -> UInt32` | Rust `max` / Swift `max` | Add | R-sized-08 |
| `clamp` |  (Add) `uint32.clamp(x: UInt32, lo: UInt32, hi: UInt32) -> UInt32` | Rust `clamp` | Add | R-sized-08 |
| `min` |  (Add) `uint64.min(a: UInt64, b: UInt64) -> UInt64` | Rust `min` / Swift `min` | Add | R-sized-08 |
| `max` |  (Add) `uint64.max(a: UInt64, b: UInt64) -> UInt64` | Rust `max` / Swift `max` | Add | R-sized-08 |
| `clamp` |  (Add) `uint64.clamp(x: UInt64, lo: UInt64, hi: UInt64) -> UInt64` | Rust `clamp` | Add | R-sized-08 |
| `min` |  (Add) `float32.min(a: Float32, b: Float32) -> Float32` | Rust `min` / Swift `min` | Add | R-sized-08 |
| `max` |  (Add) `float32.max(a: Float32, b: Float32) -> Float32` | Rust `max` / Swift `max` | Add | R-sized-08 |
| `clamp` |  (Add) `float32.clamp(x: Float32, lo: Float32, hi: Float32) -> Float32` | Rust `clamp` | Add | R-sized-08 |
| `is_nan` |  (Add) `float32.is_nan(x: Float32) -> Bool` | Rust `is_nan` / Swift `.isNaN` | Add | R-sized-08 |
| `is_infinite` |  (Add) `float32.is_infinite(x: Float32) -> Bool` | Rust `is_infinite` / Swift `.isInFinite` | Add | R-sized-08 |
| `is_finite` |  (Add) `float32.is_finite(x: Float32) -> Bool` | Rust `is_finite` / Swift `.isFinite` | Add | R-sized-08 |
| `min` |  (Add) `float64.min(a: Float64, b: Float64) -> Float64` | Rust `min` / Swift `min` | Add | R-sized-08 |
| `max` |  (Add) `float64.max(a: Float64, b: Float64) -> Float64` | Rust `max` / Swift `max` | Add | R-sized-08 |
| `clamp` |  (Add) `float64.clamp(x: Float64, lo: Float64, hi: Float64) -> Float64` | Rust `clamp` | Add | R-sized-08 |
| `is_nan` |  (Add) `float64.is_nan(x: Float64) -> Bool` | Rust `is_nan` / Swift `.isNaN` | Add | R-sized-08 |
| `is_infinite` |  (Add) `float64.is_infinite(x: Float64) -> Bool` | Rust `is_infinite` / Swift `.isInFinite` | Add | R-sized-08 |
| `is_finite` |  (Add) `float64.is_finite(x: Float64) -> Bool` | Rust `is_finite` / Swift `.isFinite` | Add | R-sized-08 |
| `abs` |  (Add) `float64.abs(x: Float64) -> Float64` | Rust `abs` / Swift `magnitude` | Add | R-sized-11 |
| `floor` |  (Add) `float64.floor(x: Float64) -> Float64` | Rust `floor` / Swift `rounded(.down)` | Add | R-sized-11 |
| `ceil` |  (Add) `float64.ceil(x: Float64) -> Float64` | Rust `ceil` / Swift `rounded(.up)` | Add | R-sized-11 |
| `round` |  (Add) `float64.round(x: Float64) -> Float64` | Rust `round` / Swift `rounded()` | Add | R-sized-11 |
| `trunc` |  (Add) `float64.trunc(x: Float64) -> Float64` | Rust `trunc` / Swift `rounded(.towardZero)` | Add | R-sized-11 |
| `signum` |  (Add) `float64.signum(x: Float64) -> Float64` | Rust `signum` | Add | R-sized-11 |
| `from` |  (Add) `float64.from[S: Numeric](x: S) -> Float64` | Rust `From` / `as` / Swift `init(_:)` / Python `int(x)` | Add | R-L0-40 |
| `from_checked` |  (Add) `float64.from_checked[S: Numeric](x: S) -> Float64?` | Rust `TryFrom` / Swift `init(exactly:)` | Add | R-L0-40 |
