> Description (fixed history): Stdlib redesign L2/L3 — float: move conversions to their source, pair to_bits with from_bits, add IEEE constants, trunc, is_finite and pow.
<a id="l2l3-float"></a>

# L2/L3: float

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-float.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-float.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-float.zh-CN.md)

> Non-normative historical redesign snapshot. Pinned original authoritative and unchanged; no automatic synchronization, fresh official-page retrieval or independent numeric/CPU/runtime tests. (F) and versioned probe outcomes are original-author reports. Preserve source-side conversion proposals versus later destination-side table, saturation-removal reversal, float-only sqrt versus table math relocation, and stated 11 additions versus later rows. NaN equality in R-float-09 is an ordering promise, not a new guarantee about the language equality operator. R-float-06 remains deferred; x86-native measurement remains absent. Japanese placeholders denote type/source. Historical totalOrder/canonicalization and per-width payload-loss constraints remain, not current implementation certification. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-float.md)

> Last updated: 2026-10-21
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md) / [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)
> Related: [L2-sized-numbers](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md) (conversion matrix and float32/float64), [L2-math](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-math.md), [L2-int](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-int.md)
> Sources: Rust https://doc.rust-lang.org/std/primitive.f64.html (F) / Go https://pkg.go.dev/math / Python https://docs.python.org/3/library/math.html / TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Number / Gleam https://gleam-stdlib.hexdocs.pm/gleam/float.html (F) / Elixir https://hexdocs.pm/elixir/Float.html / OCaml https://ocaml.org/manual/latest/api/Float.html (F) / Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin/-double/ / Swift https://developer.apple.com/documentation/swift/double

(F) = cells whose spellings were checked by fetching official pages on 2026-10-21 (Rust f64 constants/methods, Gleam float and OCaml Float function lists).

Measured (`almide 0.68.0 dev 437b8f127`, identical native/wasm): `1.0 / 0.0` is `inf`, `float.min(0.0 / 0.0, 1.0)` is `1.0` (ignores NaN), `float.sign(0.0)` is `1.0`, `float.to_int8(0.0 / 0.0)` is `0`. Infinity and NaN have no named forms; one must write `1.0 / 0.0` (NaN: `0.0 / 0.0`). `float.parse` returns only finite values: `"inf"`, `"NaN"`, overflow such as `"1e400"` yield err (#4121, C-413).

<a id="l2-役割の比較"></a>

## L2: Role comparison

| Role | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| To/from strings | `to_string` `parse` | `to_string` `parse` | `FormatFloat` `ParseFloat` | `str` `float` | `toString` `parseFloat` | `to_string` `parse` (F) | `to_string` `parse` | `to_string` `of_string_opt` (F) | `toString` `toDoubleOrNull` | `String` `Double(s)` |
| String with specified decimal places | `to_fixed` | `format!("{:.2}")` | `FormatFloat(f,'f',2,64)` | `f"{x:.2f}"` | `toFixed` | `to_precision`(Float)(F) | `Float.round` | `Printf` | `"%.2f".format` | `String(format:)` |
| To Int (truncate) | `to_int` | `as i64` | `int64(f)` | `int(f)` | `Math.trunc` | `truncate` (F) | `trunc` | `to_int` (F) | `toLong` | `Int(f)` |
| From Int | `from_int`(`int.to_float` twin) | `as f64` | `float64(n)` | `float(n)` | — | `int.to_float` (F) | — | `of_int` (F) | `toDouble` | `Double(n)` |
| Bit-pattern conversion | `to_bits` and `int.bits_to_float` | `to_bits` `from_bits` (F) | `Float64bits` `Float64frombits` | `struct` | `DataView` | — | — | `Int64.bits_of_float` | `toBits` `fromBits` | `bitPattern` `init(bitPattern:)` |
| Rounding (floor/ceil/round) | `floor` `ceil` `round` | `floor` `ceil` `round` (F) | `Floor` `Ceil` `Round` | `math.floor` etc. | `Math.*` | `floor` `ceiling` `round` (F) | `floor` `ceil` `round` | `floor` `ceil` `round` (F) | `floor` `ceil` `round` | `rounded(_:)` |
| Round toward zero (remain Float) | — | `trunc` (F) | `Trunc` | `math.trunc` | `Math.trunc` | `truncate`(returns Int)(F) | — | `trunc` (F) | `truncate` | `rounded(.towardZero)` |
| Absolute/minimum/maximum/range | `abs` `min` `max` `clamp`(`math.fmin`/`fmax` twin) | `abs` `min` `max` `clamp` (F) | `Abs` `min` `max` | `abs` `min` `max` | `Math.*` | `absolute_value` `min` `max` `clamp` (F) | `abs` `min` `max` | `abs` `min` `max` (F) | `absoluteValue` `coerceIn` | `abs` `min` `max` |
| Square root / power | `sqrt`(`math.sqrt` twin); power: `math.fpow` | `sqrt` `powf` (F) | `Sqrt` `Pow` | `math.sqrt` `math.pow` | `Math.sqrt` `Math.pow` | `square_root` `power` (F) | `:math.sqrt` `:math.pow` | `sqrt` `pow` (F) | `sqrt` `pow` | `squareRoot()` `pow` |
| Sign | `sign` | `signum` (F) | — | `math.copysign` | `Math.sign` | — | — | `sign_bit` (F) | `sign` | `sign` |
| NaN/infinity predicates | `is_nan` `is_infinite` | `is_nan` `is_infinite` `is_finite` (F) | `IsNaN` `IsInf` | `isnan` `isinf` `isfinite` | `isNaN` `isFinite` | — | — | `is_nan` `is_infinite` `is_finite` (F) | `isNaN` `isInfinite` `isFinite` | `isNaN` `isInfinite` `isFinite` |
| Infinity/NaN/machine epsilon/maximum | — | `INFINITY` `NAN` `EPSILON` `MAX` `MIN` (F) | `Inf` `NaN` `MaxFloat64` | `math.inf` `math.nan` `sys.float_info` | `Infinity` `NaN` `EPSILON` `MAX_VALUE` | — | — | `infinity` `nan` `epsilon` `max_float` (F) | `POSITIVE_INFINITY` `NaN` `MAX_VALUE` | `.infinity` `.nan` `.ulpOfOne` `.greatestFiniteMagnitude` |
| Fixed-width type conversions | `to_<型>`・`_checked`・`_saturating` and `from_float32`/`from_float64` | `as` | `T(x)` | — | — | — | — | — | `toInt()` etc. | `init(exactly:)` etc. |

<a id="完全性の規則"></a>

### Completeness rules

1. **Conversion matrix** follows [L2-sized-numbers](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md). `float` only owns conversions leaving `Float`; conversions into `Float` are source-type `to_float` (R-L0-07). `Float`-to-integer conversion is lossy, so provide `to_T`/`to_T_checked`. `to_T` itself truncates toward zero and saturates like Rust `as` (NaN → 0), so do not provide `to_T_saturating` (R-L0-01: a function merely calling another public function once is a synonym; R-sized-04). This includes `to_int`.
2. **Floating-point carriers** (`float`, `float32`, `float64`) all have seven cells: `to_bits`, `from_bits`, `infinity`, `nan`, `epsilon`, `max_value`, `min_value`. `min_value` means the smallest finite value (= `-max_value()`), like Rust `MIN`. State on the first doc line that it differs from `Kotlin`/`JS` `MIN_VALUE` (smallest positive number).
3. **No type twins**: place `sqrt`/`min`/`max`/`pow` only in `float` ([R-math-01](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-math.md)).
4. **Ordered types have `compare`** (R-L0-16): `float.compare(a, b) -> Int` (negative/0/positive). IEEE 754 totalOrder (same as `list.sort`: `-0.0 < 0.0`, NaN above positive infinity); differs from `<` (NaN comparisons false) only in handling NaN and `-0.0`. This is one added cell in [R-sized-02](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md).
5. Constants are zero-argument functions (`float.infinity()`), like existing `int.min_value()`/`math.pi()`.

<a id="l2-の判定"></a>

### L2 verdict

- Missing roles: round toward zero `trunc` (8/9), `is_finite` (7/9), infinity/NaN/epsilon/maximum constants (7/9) → R-float-03/R-float-04. Power moves from `math.fpow` → R-math-01.
- Duplicates: `float.from_int` and `int.to_float` (R-L0-07 item 3), `float.from_float32`/`from_float64` and planned `float32.to_float`/`float64.to_float` → R-float-01. `int.bits_to_float` is the inverse of `float.to_bits` but lives in `int` → R-float-02.
- Family gaps: absent `float.to_int_checked` (present for `to_int8`, etc.) → [R-sized-01](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md). Absent `compare` → [R-sized-02](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md).
- Synonyms: eight functions such as `float.to_int8_saturating` merely call `float.to_int8`, etc. once (`tests/stdlib_synonym_gate_test.rs:47-59` allowlists them). Removal under [R-sized-04](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md) was canceled because R-L0-32 makes unsuffixed forms abort.

<a id="案"></a>

## Proposals

<a id="r-float-01-floatfrom_intfrom_float32from_float64-を撤去する---採用"></a>

### R-float-01 Remove `float.from_int`/`from_float32`/`from_float64` — ○ Accepted

- Content: remove three; alternatives `int.to_float` (existing), `float32.to_float`, `float64.to_float` (added by R-sized-01).
- Supporting L0 rules: R-L0-07 items 1/3 (L0 explicitly removes `float.from_int`), R-L0-01.
- (a) Gleam has only `int.to_float`, not `float.from_int` (F). OCaml puts `Float.of_int` on the destination type, but the majority (5/9) use the source side.
- (b) Same intrinsic, unchanged on both targets.
- (c) Both `int.to_float` and `float.from_int` occur in specs, so an LLM writes either; removing one lets hints correct it.
- (d) Remove. `@deprecated(since=15, use="int.to_float")`, etc., removal epoch 16. `almide fix`: `float.from_int(x)` → `int.to_float(x)`, `float.from_float32(x)` → `float32.to_float(x)`, `float.from_float64(x)` → `float64.to_float(x)`. Usage counts: `from_int` 24/11/11/0 (+26 research), `from_float32` 4/2/6/29, `from_float64` 7/0/6/23 (stdlib counts are float32/float64 module bodies). Rewrite llms.txt:420.
- Proposed gate: R-sized-01 gate (include floating-point carriers in the matrix); also check absence of `float.from_<T>`.
- Issue: #3935

<a id="r-float-02-ビット列の変換を-to_bits--from_bits-の対にそろえる---採用"></a>

### R-float-02 Align bit conversion as a `to_bits` / `from_bits` pair — ○ Accepted

- Content: add `float.from_bits(bits: Int) -> Float`, remove `int.bits_to_float`. Remove `int.bits_to_f32` (reads low 32 bits as binary32 and widens to Float), replacing it with `float32.from_bits(bits) |> float32.to_float` (`float32.from_bits`: R-sized-03).
- Supporting L0 rules: R-L0-07 item 2 (construct values from representations through destination-module `from_<元>`), R-L0-01.
- (a) `from_bits` in Rust (F), Kotlin `Double.fromBits`, Swift `init(bitPattern:)`, Go `Float64frombits` all live on the Float side.
- (b) Same intrinsic (`almide_rt_int_bits_to_float`), identical on both targets.
- (c) Seeing `float.to_bits`, an LLM writes `float.from_bits`; currently undefined, and `int.bits_to_float` cannot be found from that name.
- (d) Add 1, remove 2. `@deprecated(since=15, use="float.from_bits")`, removal epoch 16. `almide fix`: `int.bits_to_float(b)` → `float.from_bits(b)`, `int.bits_to_f32(b)` → `float32.to_float(float32.from_bits(b))`. Usage counts: `bits_to_float` 26/10/8/3 (+8 research), `bits_to_f32` 5/3/5/1 (llms.txt:388), `to_bits` 30/4/20/8.
- Proposed gate: include the carriers' seven cells (completeness rule 2) in R-sized-01. Spec checks `from_bits(to_bits(x)) == x` on non-NaN boundary values.
- Issue: #3910

<a id="r-float-03-ieee-の定数-infinitynanepsilonmax_valuemin_value-を足す---採用"></a>

### R-float-03 Add IEEE constants `infinity`/`nan`/`epsilon`/`max_value`/`min_value` — ○ Accepted

- Content: `float.infinity() -> Float`, `float.nan() -> Float` (canonical quiet NaN; via `to_bits`, the same value as `float.to_bits`), `float.epsilon() -> Float` (2^-52), `float.max_value() -> Float`, `float.min_value() -> Float` (= `-max_value()`). Same cells for `float32`/`float64` (R-sized-03).
- Supporting L0 rules: L1 addition criterion (7/9 have names), R-L0-02 (module of returned type), R-L0-08 matrix approach (all integer carriers have `min_value`/`max_value`; align floating carriers too).
- (a) Seven languages: Rust/Go/Python/JS/OCaml/Kotlin/Swift. Gleam and Elixir do not (Elixir cannot represent infinity). `min_value` meanings vary: Rust `MIN` is smallest finite, Kotlin/JS `MIN_VALUE` smallest positive. Almide aligns with integer `min_value`: the smallest value.
- (b) Pure constants. `float.to_bits` already canonicalizes NaN bits, identical across targets (ADR-0016).
- (c) Currently one must write `1.0 / 0.0`; an LLM writes undefined `float.infinity` or `math.inf`.
- (d) Addition; usage count 0.
- Proposed gate: R-sized-01 checks all seven cells; `spec/stdlib/float_consts_test.almd` fixes values (`is_infinite(infinity())`, `is_nan(nan())`, `1.0 + epsilon() > 1.0`).
- Issue: #3910

<a id="r-float-04-trunc-と-is_finite-を足す---採用"></a>

### R-float-04 Add `trunc` and `is_finite` — ○ Accepted

- Content: `float.trunc(n: Float) -> Float` (toward zero: `-0.5` → `-0.0`), `float.is_finite(n: Float) -> Bool` (false for NaN/±inf).
- Supporting L0 rules: L1 addition criterion (`trunc` 8/9, `is_finite` 7/9), R-L0-09 (`is_` for Bool).
- (a) Spelling: Rust/Go/Python/JS/OCaml use `trunc`; Rust/OCaml `is_finite` (JS/Kotlin/Swift unseparated `isFinite`).
- (b) Pure computation; wasm has `f64.trunc`.
- (c) Provides an alternative to the off-by-one mistake of truncating negative numbers with `float.floor`. `is_finite` avoids repeatedly writing `not is_nan(x) and not is_infinite(x)`.
- (d) Addition; usage count 0.
- Proposed gate: spec rows for negatives, `-0.0`, NaN, inf.
- Issue: #3911

<a id="r-float-05-いまの-floatsign-を-floatsignum-に改名する---採用"></a>

### R-float-05 Rename current `float.sign` to `float.signum` — ○ Accepted

- Current: `fn float.sign(n: Float) -> Float` behaves like Rust `signum`: `sign(0.0) = 1.0`, `sign(-0.0) = -1.0` (C-049). By contrast `int.sign(0)`/`bigint.sign` return 0. The same name `sign` handles zero differently by type.
- Proposal: keep behavior, rename to `float.signum`, remove `float.sign`. Only change C-049's function name, not behavior.
- Supporting L0 rules: R-L0-00 item 1 (same verb differs by module), R-L0-14.4 (do not change meaning under the same name; move to a new name).
- (a) JS `Math.sign` (F)/Kotlin `sign` (F) return zero for zero. Only Rust `f64::signum` returns 1.0, spelled `signum`.
- (c) Prevent silent wrong values in `float.sign(dx) * speed` when dx = 0 was meant to stop. Rust's `signum` spelling exposes the behavioral difference.
- (d) Rename. `@deprecated(since=15, use="float.signum")`, remove epoch 16; one-to-one `almide fix`. First change the function name in als C-049 text.
- Completeness rule: the name `sign` returns “0 for 0” for every numeric type.
- Issue: #3972

<a id="r-float-06-floatminmax-の-nan-の扱いを-listminmax-とそろえる--保留"></a>

### R-float-06 Align NaN handling of `float.min`/`max` with `list.min`/`max` — Deferred

- Current: `float.max(NaN, 1.0) = 1.0` (ignores NaN, C-049), `list.max([NaN, 1.0]) = NaN` (totalOrder).
- Reason to defer: neither propagating languages (Go/JS/Kotlin/OCaml) nor ignoring languages (Rust/Swift) reach five; this changes behavior fixed by C-049/C-306. R-L0-14.4 forbids changing only NaN results under the same name, so a new name is needed if changed. To decide, measure how often LLMs encounter NaN-related divergence between `fold(…, float.max)` and `list.max` in almide-dojo (same framework as [almide-dojo#63](https://github.com/almide/almide-dojo/issues/63)).
- Issue: none (file after measurement)

<a id="r-float-07-round-と-to_fixed-の同点の丸めの違いを文書とゲートで固定する---採用関数の面は変えない"></a>

### R-float-07 Fix `round` versus `to_fixed` tie-rounding differences in docs/gates — ○ Accepted (no API surface change)

- Current: `float.round(2.5) = 3.0` (away from zero), `float.to_fixed(2.5, 0) = "2"` (nearest to the exact binary value, ties to even). JS `toFixed` yields `"3"`.
- Proposal: put rounding rules on each function's first doc line; add one float line to CHEATSHEET. Fix `0.5`, `1.5`, `2.5`, `-0.5`, `0.125` under both `round` and `to_fixed` in `spec/stdlib`.
- (d) Documentation/tests only.
- Issue: #3972

<a id="r-float-08-float-の床方向の剰余-floatmod-を足す---不採用"></a>

### R-float-08 Add floor-directed Float remainder `float.mod` — × Rejected

- Reason: only three languages have floor-directed Float remainder: Python `%`, Kotlin `mod`, Gleam `float.modulo`; below the five-language criterion. Go/JS/OCaml/Swift/Elixir use toward-zero remainder.
- Issue: none

<a id="r-float-09-nan-は-1-つの値にそろえ順序の約束を正確に書く---採用"></a>

### R-float-09 Canonicalize NaN to one value and state ordering precisely — ○ Accepted

- Current common promise was “IEEE 754 totalOrder (NaN above positive infinity)”. Under totalOrder, a NaN with its sign bit set precedes negative infinity. Measurements (`almide 0.68.0 dev 437b8f127`, identical native/wasm):
  - Sorting a negative NaN read from bytes (`bytes.read_f64_le` with `00 00 00 00 00 00 f8 ff`) together with `0.0 / 0.0` using `list.sort` placed one first and one last.
  - `float.to_bits`/`bytes.append_f64_le` write NaNs as one positive value (`0x7ff8000000000000`), so negative NaN bits do not round-trip.
  - The sign bit of `0.0 / 0.0` differs by CPU (x86 negative, ARM positive). This Mac (ARM) was positive; x86 native has not yet been measured.
- Proposal: canonicalize NaN to one value (positive quiet NaN, `0x7ff8000000000000`) when produced by operations, `bytes.read_f*`/`get_f*`/`next_f*`, or `float.from_bits`.
  - Ordering promise: “`-0.0 < 0.0`, NaN above positive infinity, NaNs equal to each other”; with one NaN this holds precisely.
  - NaN bits do not round-trip (any read NaN writes as `0x7ff8000000000000`); state this contract.
  - Same treatment as math-function promise (“one fixed NaN value”, ADR-0016).
- Basis: R-L0-00 (one treatment for the same value), ADR-0016.
- (a) JavaScript NaN has one observable value (some engines do not normalize reads via `Float64Array`). Rust `total_cmp` distinguishes signs. Almide promises identical results on both targets/all CPUs, so chooses canonicalization.
- (b) Same ordering across both targets/all CPUs.
- (d) Semantic change (negative NaN and NaN ordering changes). No standard API inspects NaN sign.
- Issue: #3990

<a id="r-float-10-nan-を型の幅ごとに-1-つにしビットの読み書きでも同じにすることを明記する---採用r-float-09-の補い"></a>

### R-float-10 Specify one NaN per width, also in bit I/O — ○ Accepted (supplements R-float-09)

- Current: R-float-09 specified one NaN `0x7ff8000000000000`, but only the 64-bit form. This does not apply to `float32` bit/byte I/O (`float32.to_bits`, `from_bits`, `bytes.append_f32_le`). Nor did the promise state that NaN payloads disappear through `bytes` I/O and `to_bits`.
- Proposal:
  - One NaN per type width: `Float`/`Float64` use `0x7ff8000000000000`; `Float32` uses `0x7fc00000` (both positive quiet NaNs).
  - `to_bits`, `from_bits` and `bytes` operations such as `get_f32_le`, `get_f64_le`, `append_f32_le`, `append_f64_le` also yield this one value; read NaN payloads do not survive.
  - To round-trip bytes containing NaN payloads unchanged (wasm bytes, msgpack, etc.), do not read as floating point; use integer I/O with `get_u32_le`/`get_u64_le`.
- Supporting L0 rule: R-float-09.
- (a) Java `Float.floatToIntBits` also canonicalizes to `0x7fc00000` (`floatToRawIntBits` does not).
- (b) Same on both targets (R-float-09 measurements).
- (d) Documentation finalization, introduced in epoch 15 alongside R-float-09.
- Issue: #3995
<a id="l3-関数表"></a>

## L3: Function table

The 46 current `float` functions and 11 additions (including `compare`).
| Function | Signature (current, mut shown) | Main spellings in other languages | Verdict | Proposal |
|---|---|---|---|---|
| `to_string` | `float.to_string(n: Float) -> String` | Rust `to_string` / Go `strconv.FormatInt` / Python `str` / JS `toString` / Gleam `to_string` (F) / Elixir `to_string` (F) / OCaml `to_string` (F) / Kotlin `toString` / Swift `String(x)` | Retain | — |
| `to_int` | `float.to_int(n: Float) -> Int` | Rust `as`(saturating) / Kotlin `toInt()`(saturating)(F) / Swift `init(_:)`(trap) / Go `T(x)` / Gleam `truncate` (F) / OCaml `to_int` (F) | Remove (→ `int.from`) | R-L0-40 |
| `from_int` | `float.from_int(n: Int) -> Float` | OCaml `Float.of_int` (F) / Gleam: `int.to_float` (F) / Kotlin `toDouble` | Remove | R-float-01(`int.to_float` same; R-L0-07 item 3) |
| `parse` | `float.parse(s: String) -> Result[Float, String]` | Rust `parse` / Go `ParseFloat` / Gleam `parse` (F) / OCaml `of_string_opt` (F) / Kotlin `toDoubleOrNull` / Swift `Double(s)` | Retain | — |
| `to_fixed` | `float.to_fixed(n: Float, decimals: Int) -> String` | JS `toFixed` / Gleam `to_precision`(returns Float)(F) / Kotlin `"%.2f".format` / Rust `format!("{:.2}")` | Retain | — |
| `to_bits` | `float.to_bits(f: Float) -> Int` | Rust `to_bits` (F) / Go `math.Float64bits` / Kotlin `toBits` / Swift `bitPattern` | Retain | R-float-02(`from_bits` paired) |
| `sqrt` | `float.sqrt(n: Float) -> Float` | Rust `sqrt` (F) / Go `math.Sqrt` / Python `math.sqrt` / JS `Math.sqrt` / Gleam `square_root` (F) / OCaml `sqrt` (F) / Kotlin `sqrt` / Swift `squareRoot()` | Remove → `math.sqrt` | R-math-03 |
| `abs` | `float.abs(n: Float) -> Float` | Rust `abs` (F) / Go `math.Abs` / Python `abs` / JS `Math.abs` / Gleam `absolute_value` (F) / OCaml `abs` (F) / Kotlin `absoluteValue` / Swift `abs` | Retain | — |
| `floor` | `float.floor(n: Float) -> Float` | Rust `floor` (F) / Go `math.Floor` / JS `Math.floor` / Gleam `floor` (F) / OCaml `floor` (F) / Kotlin `floor` / Swift `rounded(.down)` | Retain | — |
| `ceil` | `float.ceil(n: Float) -> Float` | Rust `ceil` (F) / Go `math.Ceil` / JS `Math.ceil` / Gleam `ceiling` (F) / OCaml `ceil` (F) / Kotlin `ceil` / Swift `rounded(.up)` | Retain | — |
| `round` | `float.round(n: Float) -> Float` | Rust `round`(away from zero)(F) / Go `math.Round` / JS `Math.round`(toward +∞) / Gleam `round` (F) / OCaml `round` (F) / Kotlin `round`(ties to even) / Swift `rounded()` | Retain | — |
| `min` | `float.min(a: Float, b: Float) -> Float` | Rust `min` (F) / Go `min` / JS `Math.min` / Gleam `min` (F) / OCaml `min` (F) / Kotlin `minOf` / Swift `min` | Retain | R-math-01(`math.fmin` retained counterpart) |
| `max` | `float.max(a: Float, b: Float) -> Float` | Rust `max` (F) / Go `max` / JS `Math.max` / Gleam `max` (F) / OCaml `max` (F) / Kotlin `maxOf` / Swift `max` | Retain | R-math-01(`math.fmax` retained counterpart) |
| `clamp` | `float.clamp(n: Float, lo: Float, hi: Float) -> Float` | Rust `clamp` (F) / Gleam `clamp` (F) / Kotlin `coerceIn` | Retain | — |
| `sign` | `float.sign(n: Float) -> Float` | Rust `signum` (F) / JS `Math.sign` / Kotlin `sign` / OCaml `sign_bit` (F)(Bool) | Rename → `signum`(behavior unchanged) | R-float-05 |
| `is_nan` | `float.is_nan(n: Float) -> Bool` | Rust `is_nan` (F) / Go `math.IsNaN` / Python `math.isnan` / JS `Number.isNaN` / OCaml `is_nan` (F) / Kotlin `isNaN` / Swift `isNaN` | Retain | — |
| `is_infinite` | `float.is_infinite(n: Float) -> Bool` | Rust `is_infinite` (F) / Go `math.IsInf` / Python `math.isinf` / OCaml `is_infinite` (F) / Kotlin `isInfinite` / Swift `isInfinite` | Retain | — |
| `to_int8` | `float.to_int8(n: Float) -> Int8` | Rust `as`(saturating) / Kotlin `toInt()`(saturating)(F) / Swift `init(_:)`(trap) / Go `T(x)` | Remove (→ `int8.from`) | R-L0-40 |
| `to_int16` | `float.to_int16(n: Float) -> Int16` | Rust `as`(saturating) / Kotlin `toInt()`(saturating)(F) / Swift `init(_:)`(trap) / Go `T(x)` | Remove (→ `int16.from`) | R-L0-40 |
| `to_int32` | `float.to_int32(n: Float) -> Int32` | Rust `as`(saturating) / Kotlin `toInt()`(saturating)(F) / Swift `init(_:)`(trap) / Go `T(x)` | Remove (→ `int32.from`) | R-L0-40 |
| `to_uint8` | `float.to_uint8(n: Float) -> UInt8` | Rust `as`(saturating) / Kotlin `toInt()`(saturating)(F) / Swift `init(_:)`(trap) / Go `T(x)` | Remove (→ `uint8.from`) | R-L0-40 |
| `to_uint16` | `float.to_uint16(n: Float) -> UInt16` | Rust `as`(saturating) / Kotlin `toInt()`(saturating)(F) / Swift `init(_:)`(trap) / Go `T(x)` | Remove (→ `uint16.from`) | R-L0-40 |
| `to_uint32` | `float.to_uint32(n: Float) -> UInt32` | Rust `as`(saturating) / Kotlin `toInt()`(saturating)(F) / Swift `init(_:)`(trap) / Go `T(x)` | Remove (→ `uint32.from`) | R-L0-40 |
| `to_uint64` | `float.to_uint64(n: Float) -> UInt64` | Rust `as`(saturating) / Kotlin `toInt()`(saturating)(F) / Swift `init(_:)`(trap) / Go `T(x)` | Remove (→ `uint64.from`) | R-L0-40 |
| `to_float32` | `float.to_float32(n: Float) -> Float32` | Rust `as` / Kotlin `toFloat()` / Swift `init(_:)` | Remove (→ `float32.from`) | R-L0-40 |
| `to_int64` | `float.to_int64(n: Float) -> Int64` | Rust `as`(saturating) / Kotlin `toInt()`(saturating)(F) / Swift `init(_:)`(trap) / Go `T(x)` | Remove (→ `int64.from`) | R-L0-40 |
| `to_float64` | `float.to_float64(n: Float) -> Float64` | Rust `as` / Kotlin `toFloat()` / Swift `init(_:)` | Remove (→ `float64.from`) | R-L0-40 |
| `from_float32` | `float.from_float32(n: Float32) -> Float` | Rust `as` / Kotlin `toFloat()` / Swift `init(_:)` | Remove | R-float-01(`float32.to_float`) |
| `from_float64` | `float.from_float64(n: Float64) -> Float` | Rust `as` / Kotlin `toFloat()` / Swift `init(_:)` | Remove | R-float-01(`float64.to_float`) |
| `to_int8_checked` | `float.to_int8_checked(n: Float) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / other 6 languages — | Remove (→ `int8.from_checked`) | R-L0-40 |
| `to_int16_checked` | `float.to_int16_checked(n: Float) -> Option[Int16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / other 6 languages — | Remove (→ `int16.from_checked`) | R-L0-40 |
| `to_int32_checked` | `float.to_int32_checked(n: Float) -> Option[Int32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / other 6 languages — | Remove (→ `int32.from_checked`) | R-L0-40 |
| `to_int64_checked` | `float.to_int64_checked(n: Float) -> Option[Int64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / other 6 languages — | Remove (→ `int64.from_checked`) | R-L0-40 |
| `to_uint8_checked` | `float.to_uint8_checked(n: Float) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / other 6 languages — | Remove (→ `uint8.from_checked`) | R-L0-40 |
| `to_uint16_checked` | `float.to_uint16_checked(n: Float) -> Option[UInt16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / other 6 languages — | Remove (→ `uint16.from_checked`) | R-L0-40 |
| `to_uint32_checked` | `float.to_uint32_checked(n: Float) -> Option[UInt32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / other 6 languages — | Remove (→ `uint32.from_checked`) | R-L0-40 |
| `to_uint64_checked` | `float.to_uint64_checked(n: Float) -> Option[UInt64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / other 6 languages — | Remove (→ `uint64.from_checked`) | R-L0-40 |
| `to_float32_checked` | `float.to_float32_checked(n: Float) -> Option[Float32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` family / other 6 languages — | Remove (→ `float32.from_checked`) | R-L0-40 |
| `to_int8_saturating` | `float.to_int8_saturating(n: Float) -> Int8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / other 7 languages — | Remove (→ `int8.from_saturating`) | R-L0-40 |
| `to_int16_saturating` | `float.to_int16_saturating(n: Float) -> Int16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / other 7 languages — | Remove (→ `int16.from_saturating`) | R-L0-40 |
| `to_int32_saturating` | `float.to_int32_saturating(n: Float) -> Int32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / other 7 languages — | Remove (→ `int32.from_saturating`) | R-L0-40 |
| `to_int64_saturating` | `float.to_int64_saturating(n: Float) -> Int64` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / other 7 languages — | Remove (→ `int64.from_saturating`) | R-L0-40 |
| `to_uint8_saturating` | `float.to_uint8_saturating(n: Float) -> UInt8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / other 7 languages — | Remove (→ `uint8.from_saturating`) | R-L0-40 |
| `to_uint16_saturating` | `float.to_uint16_saturating(n: Float) -> UInt16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / other 7 languages — | Remove (→ `uint16.from_saturating`) | R-L0-40 |
| `to_uint32_saturating` | `float.to_uint32_saturating(n: Float) -> UInt32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / other 7 languages — | Remove (→ `uint32.from_saturating`) | R-L0-40 |
| `to_uint64_saturating` | `float.to_uint64_saturating(n: Float) -> UInt64` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / other 7 languages — | Remove (→ `uint64.from_saturating`) | R-L0-40 |
| `from_bits` | (Add)`float.from_bits(bits: Int) -> Float` | Rust `f64::from_bits` (F) / Go `math.Float64frombits` / Kotlin `Double.fromBits` / Swift `Double(bitPattern:)` | Add | R-float-02 |
| `trunc` | (Add)`float.trunc(n: Float) -> Float` | Rust `trunc` (F) / Go `math.Trunc` / Python `math.trunc` / JS `Math.trunc` / OCaml `trunc` (F) / Kotlin `truncate` / Swift `rounded(.towardZero)` | Add | R-float-04 |
| `is_finite` | (Add)`float.is_finite(n: Float) -> Bool` | Rust `is_finite` (F) / Python `math.isfinite` / JS `Number.isFinite` / OCaml `is_finite` (F) / Kotlin `isFinite` / Swift `isFinite` | Add | R-float-04 |
| `pow` | (Add)`float.pow(base: Float, exp: Float) -> Float` | Rust `powf` (F) / Go `math.Pow` / Python `math.pow` / JS `Math.pow` / Gleam `power` (F) / OCaml `pow` (F) / Kotlin `pow` / Swift `pow` | Add | R-math-01 |
| `infinity` | (Add)`float.infinity() -> Float` | Rust `INFINITY` (F) / Go `math.Inf(1)` / Python `math.inf` / JS `Infinity` / OCaml `infinity` (F) / Kotlin `POSITIVE_INFINITY` / Swift `.infinity` | Add | R-float-03 |
| `nan` | (Add)`float.nan() -> Float` | Rust `NAN` (F) / Go `math.NaN()` / Python `math.nan` / JS `NaN` / OCaml `nan` (F) / Kotlin `NaN` / Swift `.nan` | Add | R-float-03 |
| `epsilon` | (Add)`float.epsilon() -> Float` | Rust `EPSILON` (F) / Python `sys.float_info.epsilon` / JS `Number.EPSILON` / OCaml `epsilon` (F) / Swift `.ulpOfOne` | Add | R-float-03 |
| `max_value` | (Add)`float.max_value() -> Float` | Rust `MAX` (F) / Go `math.MaxFloat64` / JS `Number.MAX_VALUE` / OCaml `max_float` (F) / Kotlin `MAX_VALUE` / Swift `.greatestFiniteMagnitude` | Add | R-float-03 |
| `min_value` | (Add)`float.min_value() -> Float` | Rust `MIN`(smallest finite value)(F) / Swift `-.greatestFiniteMagnitude` / Note: Kotlin/JS `MIN_VALUE` is smallest positive number | Add | R-float-03 |
| `compare` | (Add)`float.compare(a: Float, b: Float) -> Int`(IEEE totalOrder) | Rust `total_cmp` (F) / Go `cmp.Compare` / Gleam `compare` (F) / OCaml `compare` (F) / Kotlin `compareTo` / Swift `isTotallyOrdered` | Add | R-sized-02 |
| `from` | (Add)`float.from[S: Numeric](x: S) -> Float` | Rust `From`・`as` / Swift `init(_:)` / Python `int(x)` | Add | R-L0-40 |
| `from_checked` | (Add)`float.from_checked[S: Numeric](x: S) -> Float?` | Rust `TryFrom` / Swift `init(exactly:)` | Add | R-L0-40 |
