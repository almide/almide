> Description (inactive source index metadata): Stdlib redesign L2/L3 — bigint (new module, R-L1-03): the proposed surface of an arbitrary-precision integer that mirrors int, with operators desugared to it and byte-identical results on both targets
<a id="l2l3-bigint新しいモジュール提案する面だけ"></a>

# L2/L3: bigint (a new module; proposed surface only)

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-bigint.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-bigint.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-bigint.zh-CN.md)

> Non-normative historical snapshot; the pinned original remains authoritative and unchanged. No automatic synchronization or independent rerun/current implementation verification. Preserve the original proposal of 26 functions versus the later claim of 24 public functions, initial exclusions versus later accepted bitwise/direct-conversion additions, and internal naming changes. The early mod spelling and later modulo are not reconciled. Numeric determinism and dated measurements are source claims, not new validation. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-bigint.md)

> Last updated: 2026-10-21
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md) / [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md) (R-L1-03: add `bigint`)
> Related: [L2-int](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-int.md) (use the same spellings as `int`) / [L2-sized-numbers](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md) (conversion-matrix rules)
> Sources: Rust — (not in the standard library; https://docs.rs/num-bigint/ is the de facto standard) / Go https://pkg.go.dev/math/big#Int / Python https://docs.python.org/3/library/stdtypes.html#numeric-types-int-float-complex / TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/BigInt / Gleam https://gleam-stdlib.hexdocs.pm/gleam/int.html (F) (Int on the Erlang target is arbitrary precision) / Elixir https://elixir.hexdocs.pm/Integer.html (F) (Integer is arbitrary precision) / OCaml — (Zarith is external: https://github.com/ocaml/Zarith ) / Kotlin https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/math/BigInteger.html (JVM only) / Swift —

Almide's current `Int` is 64 bit and wraps on overflow (measured: `int.max_value() + 1` is `-9223372036854775808`; `math.factorial(21)` also wraps). Factorials, large Fibonacci numbers and combinations silently produce wrong answers.

<a id="l2-役割の比較"></a>

## L2: Role comparison

| Role | Proposal (Almide) | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin (JVM) | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| Type | `BigInt` | — | `big.Int` | `int` | `BigInt` | ◐ `Int`（Erlang） | `Integer` | — | `BigInteger` | — |
| From Int | `int.to_bigint` | — | `NewInt` | (same type) | `BigInt(n)` | — | — | — | `valueOf` | — |
| To Int | `to_int`（`_checked`・`_saturating`） | — | `Int64` `IsInt64` | (same type) | `Number(b)` `asIntN` | — | — | — | `toLong` `longValueExact` | — |
| From / to strings | `parse` `to_string`(also `_radix` variants) | — | `SetString` `String` `Text(base)` | `int(s)` `str` | `BigInt(s)` `toString(r)` | — | `parse` `to_string` (F) | — | `BigInteger(s)` `toString(r)` | — |
| Arithmetic and remainder | `add` `sub` `mul` `div` `rem`(operators) | — | `Add` `Sub` `Mul` `Quo` `Rem` | `+ - * // %` | `+ - * / %` | — | `+ - * div rem` | — | `add` `subtract` `multiply` `divide` `remainder` | — |
| Floor division / modulo | `floor_div` `modulo` | — | `Div` `Mod`（Euclid） | `//` `%` | — | — | `floor_div` `mod` (F) | — | `mod`(nonnegative) | — |
| Negation / absolute value / power | `neg` `abs` `sign` `pow` | — | `Neg` `Abs` `Sign` `Exp` | `-` `abs` `**` | `-` `**` | — | `abs` `pow` (F) | — | `negate` `abs` `signum` `pow` | — |
| Comparison / minimum / maximum | `compare` `min` `max`(operator `<` too) | — | `Cmp` | `<` | `<` | — | `<` | — | `compareTo` `min` `max` | — |
| To Float | `to_float`（`_checked`） | — | `Float64` | `float` | `Number` | — | — | — | `toDouble` | — |

<a id="完全性の規則"></a>

### Completeness rules

1. **Same spellings as `int`**: when `int` has a function with the same role, a `BigInt` function uses the same name and argument order (`abs` `min` `max` `sign` `pow` `compare` `parse` `to_string` `to_string_radix` `parse_radix` `floor_div` `mod`). Align with functions added in the `int` L2 (R-int-03 / R-int-04).
2. **Conversion direction** (R-L0-07): `Int` → `BigInt` uses `int.to_bigint` on the source type; `BigInt` → `Int` uses `bigint.to_int`. Since `BigInt` → `Int` is lossy, provide a triple (`to_int` / `to_int_checked` / `to_int_saturating`); for `BigInt` → `Float`, provide `to_float` and `to_float_checked` (the same as rules 3 / 4 of [L2-sized-numbers](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md)). Do not provide direct conversions with fixed-width types (go through `to_int`).
3. **Operators**: `+` `-` `*` `/` `%` `==` `<` and others desugar to `bigint.add` … (L1 R-L1-03). Like `Int`, `/` and `%` truncate toward 0; division by 0 aborts.
4. **Intentional omissions**: bitwise operations (such as `band`), `gcd` (only Go, Python, Elixir and JVM have it in their standard libraries: 4, short of a majority), square root, implicit conversions with `Int`, and literals such as `123n` (R-bigint-02).

<a id="l2-の判定"></a>

### L2 verdict

- This is a new module. There is no duplication or conflict with existing spellings.
- What it accommodates: leave wrapping in `math.factorial` and `math.choose` ([L2-math](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-math.md)) unchanged, and document that code needing large values should use `bigint`.

<a id="案"></a>

## Proposals

<a id="r-bigint-01-任意精度整数のモジュール-bigint-を足す---採用"></a>

### R-bigint-01 Add the arbitrary-precision integer module `bigint` — ○ Accepted

- Content: the type `BigInt` and the 26 functions in the L3 table below (including `int.to_bigint`).
- Supporting L0 rules: L1 R-L1-03, R-L0-02 (module of the returned type / subject type), R-L0-07 (conversion direction), R-L0-06 (`parse` returns `Result`; `to_int_checked` returns `Option`), R-L0-16 (ordered types have `compare`).
- (a) Six languages have it: Go, Python, JS, Elixir, Kotlin (JVM), and Gleam on the Erlang target.
- (b) Write it in Almide as pure computation (store the sequence of digits in `List[Int]` or `Bytes`). No host functionality is needed. Include results that do not differ by even 1 bit between native and wasm in the ADR-0016 numeric-determinism commitment. Do not use a separate native-only implementation for speed (the ADR-0027 D8 principle: do not make a one-sided path the endpoint).
- (c) For problems such as “factorial of 30” and “the 100th Fibonacci number,” LLMs write as though using Python's `int` and silently fail because `Int` wraps. With `bigint`, CHEATSHEET can say in 1 line, “use `bigint` for factorials above 20.”
- (d) Addition (new module; whether to keep it out of `AUTO_IMPORT_BUNDLED` and require `import bigint` follows the L1 import rule). Usage count: 0.
- Proposed gate: differential tests comparing same-named `int` and `bigint` functions on inputs that fit in `Int` (for example, `bigint.to_int(bigint.add(a, b)) == a + b`, within a non-wrapping range). Pin agreement between both targets with wasm_cross fixtures, and put new C contract numbers in als first. Check completeness rule 1, “same spellings as `int`,” from signatures.
- issue: #3931

<a id="r-bigint-02-123n-のような-bigint-のリテラルを足す---不採用"></a>

### R-bigint-02 Add BigInt literals such as `123n` — × Rejected

- Content: do not add them. Write `bigint.parse("123…")!` and `int.to_bigint(123)`.
- Rationale: changing lexical rules changes the language; ADR-0001 D3 deferred time-unit literals (`100ms`) for the same reason. Only JS has this spelling; it is not the majority either.
- issue: none

<a id="r-bigint-03-int-と-bigint-を混ぜた演算を自動で広げる---不採用"></a>

### R-bigint-03 Automatically widen mixed `Int` and `BigInt` operations — × Rejected

- Content: do not. If one operand of `a + b` is `Int` and the other is `BigInt`, report a type error and suggest `int.to_bigint(a)` in the diagnostic hint.
- Rationale: R-L0-07 (write conversions as named functions). Implicit conversion hides from the reader which type's operation ran (the same reason ADR-0001 separates clocks by type).
- issue: none

<a id="r-bigint-04-bigintaddsubmuldivremneg-を公開しない演算子だけにする---採用"></a>

### R-bigint-04 Do not expose `bigint.add`/`sub`/`mul`/`div`/`rem`/`neg` (operators only) — ○ Accepted

- Now: the proposal exposes 6 functions such as `bigint.add(a, b)` and desugars operators `+ - * / %` and unary `-` to them.
- Proposal: give the 6 functions internal names (such as `__add`), leaving operators as the only public spelling. Keep `floor_div`, `modulo`, `abs` and `pow` public because they also exist in `int`.
- Supporting L0 rules: R-L0-01 (one spelling per role), bigint completeness rule 1 (same spellings as `int`; there is no `int.add` or `int.rem`), R-L0-26 (do not expose operator desugaring targets). The precedent is the `__` names of R-matrix-02.
- (a) Languages that support operator notation (Python, JS, Elixir) do not expose the function spelling. Go's `Add` and JVM's `add` use functions in languages without operator overloading.
- (c) Having both `a + b` and `bigint.add(a, b)` makes generated code fluctuate. The asymmetry of having `bigint.rem` but no `int.rem` leads LLMs to guess `int.rem` and fail.
- (d) There is no compatibility impact because this is a new module. For Java-derived `bigint.add`, show guidance to “write `a + b`” (R-L0-19).
- issue: #3973

<a id="r-bigint-05-bigintclamp-を足す---採用"></a>

### R-bigint-05 Add `bigint.clamp` — ○ Accepted

- Proposal: `fn bigint.clamp(n: BigInt, lo: BigInt, hi: BigInt) -> BigInt`. Like `int.clamp`, abort when `lo > hi`.
- Supporting L0 rule: bigint completeness rule 1 (use the same name when `int` has the same role). `min` and `max` were included but `clamp` alone was missed.
- (a) Rust `Ord::clamp`, Kotlin `coerceIn`, Gleam `int.clamp`.
- (d) Add 1. This is a new module.
- Proposed gate: also check the bigint gate in the direction “anything in `int` that is not in the exclusion table must also exist in bigint.”
- issue: #3972

<a id="r-bigint-06-uint64-と-bigint-を値を失わずに行き来させる---採用"></a>

### R-bigint-06 Convert between `uint64` and `BigInt` without losing values — ○ Accepted

- Now: bigint rule 2 says “do not provide direct conversions with fixed-width types (go through `to_int`),” but `uint64.to_int` wraps values of 2^63 and above to negative numbers (measured: `int.from_uint64(uint64.max_value())` is `-1`).
- Proposal: `fn uint64.to_bigint(x: UInt64) -> BigInt` (lossless) and a triple for `fn bigint.to_uint64(b: BigInt) -> UInt64` (`_checked` / `_saturating`).
- Supporting L0 rules: R-L0-07 (conversions use the source type's `to_<先>` (the retained Japanese placeholder means the destination type)), R-L0-08 (a triple for the lossy direction).
- (a) Go `SetUint64`/`Uint64`/`IsUint64`, JS `BigInt.asUintN(64, b)`.
- (d) Add 4.
- Completeness rule: carriers that lose values when routed through `to_int` (currently only `uint64`) connect directly to `BigInt`.
- issue: #3972

<a id="r-bigint-07-bigint-に-integer-のビット演算を置く---採用"></a>

### R-bigint-07 Give `bigint` the bitwise operations of `Integer` — ○ Accepted

- Now: `bigint` has `Integer` arithmetic (`floor_div`, `modulo`, `pow`, `is_even`, `is_odd`), but not bitwise operations.
- Proposal: provide `band`, `bor`, `bxor`, `bnot`, `bshl`, `bshr` (the same functions as `int`; R-int-10). Treat negative numbers as two's complement with infinitely many leading 1 bits (like Python's `int` and Swift's `BinaryInteger`). `bnot(x)` is `-x - 1`.
- Intentional omissions: do not provide `count_leading_zeros`, `count_trailing_zeros`, `count_ones`, `byte_swap` (these are requirements of `FixedWidthInteger`, with no fixed answer for a type without a width).
- (a) Swift's `BinaryInteger` requires bitwise operations, designed so arbitrary-precision integers also have them. Python's `int`, Go's `big.Int` (`And`, `Or`, `Xor`, `Not`, `Lsh`, `Rsh`), and Java/Kotlin's `BigInteger` (`and`, `or`, `xor`, `not`, `shiftLeft`, `shiftRight`) also have them.
- (d) Add 6.
- issue: #4000

<a id="実装の状況wave-22026-10-24"></a>

## Implementation status (wave 2, 2026-10-24)

- Implemented: the 24 public functions in the L3 table (including `from` and `from_checked`). Written in pure Almide in `stdlib/bigint.almd`, used with `import bigint`. The representation is a sign plus a `List[Int]` of 30 bit limbs in least-significant-first order. Zero is not negative and has no leading zero limbs, so `==` compares values.
- Operators (R-L1-03, R-bigint-04) were also implemented (2026-10-24). `BigInt` is declared as `type BigInt: Numeric, Integer`; `+ - * / %` and unary `-` desugar to module-local `local fn` functions `add`, `sub`, `mul`, `div`, `rem`, `neg`, and `< <= > >=` desugar to comparing `compare(a, b)` with 0 (the mapping of operators to requirements is in one place, `OPERATOR_REQUIREMENTS`). `/` rounds toward 0, and `%` has the dividend's sign, as for `Int`. Desugaring target functions are not public. `list.sum` can also add `BigInt` values. `spec/stdlib/bigint_operators_test.almd` checks both targets.
- `bigint.from` preserves integers and produces the exact value truncated toward 0 for floating-point inputs (even `1e300` loses no digits). `from_checked` yields none for a fractional part, NaN or infinity.

<a id="l3-関数表提案"></a>

## L3: Function table (proposed)

| Function | Signature (proposed) | Main spellings in other languages | Verdict | Proposal |
|---|---|---|---|---|
| `parse` | `bigint.parse(s: String) -> Result[BigInt, String]` | Go `SetString` / Python `int(s)` / JS `BigInt(s)` / Elixir `Integer.parse` (F) | Add | R-bigint-01 |
| `parse_radix` | `bigint.parse_radix(s: String, radix: Int) -> Result[BigInt, String]` | Go `SetString(s, base)` / Python `int(s, base)` / Kotlin `BigInteger(s, radix)` | Add | R-bigint-01 |
| `to_string` | `bigint.to_string(b: BigInt) -> String` | Go `String` / Python `str` / JS `toString` / Elixir `to_string` (F) | Add | R-bigint-01 |
| `to_string_radix` | `bigint.to_string_radix(b: BigInt, radix: Int) -> String` | Go `Text(base)` / JS `toString(radix)` / Kotlin `toString(radix)` | Add | R-bigint-01 |
| `add` | `bigint.add(a: BigInt, b: BigInt) -> BigInt`（`+`） | Go `Add` / Kotlin `add` / others use `+` | Do not add (internal operator desugaring target) | R-bigint-04 |
| `sub` | `bigint.sub(a: BigInt, b: BigInt) -> BigInt`（`-`） | Go `Sub` / Kotlin `subtract` | Do not add (internal operator desugaring target) | R-bigint-04 |
| `mul` | `bigint.mul(a: BigInt, b: BigInt) -> BigInt`（`*`） | Go `Mul` / Kotlin `multiply` | Do not add (internal operator desugaring target) | R-bigint-04 |
| `div` | `bigint.div(a: BigInt, b: BigInt) -> BigInt`（`/`, toward 0; division by 0 aborts） | Go `Quo` / Kotlin `divide` / JS `/` | Do not add (internal operator desugaring target) | R-bigint-04 |
| `rem` | `bigint.rem(a: BigInt, b: BigInt) -> BigInt`（`%`） | Go `Rem` / Kotlin `remainder` / JS `%` | Do not add (internal operator desugaring target) | R-bigint-04 |
| `floor_div` | `bigint.floor_div(a: BigInt, b: BigInt) -> BigInt` | Python `//` / Elixir `Integer.floor_div` (F) | Add | R-bigint-01(same spelling as R-int-03) |
| `modulo` | `bigint.modulo(a: BigInt, b: BigInt) -> BigInt` | Python `%` / Elixir `Integer.mod` (F) / Kotlin `mod` | Add | R-bigint-01(same spelling as R-int-03) |
| `neg` | `bigint.neg(a: BigInt) -> BigInt`(unary `-`） | Go `Neg` / Kotlin `negate` | Do not add (internal operator desugaring target) | R-bigint-04 |
| `abs` | `bigint.abs(a: BigInt) -> BigInt` | Go `Abs` / Python `abs` / Kotlin `abs` | Add | R-bigint-01 |
| `sign` | `bigint.sign(a: BigInt) -> Int` | Go `Sign` / Kotlin `signum` | Add | R-bigint-01 |
| `pow` | `bigint.pow(base: BigInt, exp: Int) -> BigInt`(negative `exp` aborts) | Go `Exp` / Python `**` / Elixir `Integer.pow` (F) / Kotlin `pow` | Add | R-bigint-01 |
| `compare` | `bigint.compare(a: BigInt, b: BigInt) -> Int` | Go `Cmp` / Kotlin `compareTo` | Add | R-bigint-01 |
| `min` | `bigint.min(a: BigInt, b: BigInt) -> BigInt` | Kotlin `min` / Python `min` | Add | R-bigint-01 |
| `max` | `bigint.max(a: BigInt, b: BigInt) -> BigInt` | Kotlin `max` / Python `max` | Add | R-bigint-01 |
| `is_even` | `bigint.is_even(a: BigInt) -> Bool` | Elixir `Integer.is_even` (F) / Kotlin `!testBit(0)` | Add | R-bigint-01(same spelling as `int.is_even`) |
| `is_odd` | `bigint.is_odd(a: BigInt) -> Bool` | Elixir `Integer.is_odd` (F) | Add | R-bigint-01 |
| `clamp` | `bigint.clamp(n: BigInt, lo: BigInt, hi: BigInt) -> BigInt` | Rust `clamp` / Kotlin `coerceIn` | Add | R-bigint-05 |
| `from` | （Add）`bigint.from[S: Numeric](x: S) -> BigInt` | Rust `From`・`as` / Swift `init(_:)` / Python `int(x)` | Add | R-L0-40 |
| `from_checked` | （Add）`bigint.from_checked[S: Numeric](x: S) -> BigInt?` | Rust `TryFrom` / Swift `init(exactly:)` | Add | R-L0-40 |
| `bit_length` | （Add）`bigint.bit_length(n: BigInt) -> Int` | Python `bit_length` / Go `BitLen` | Add | R-int-11 |
| `band` | `bigint.band(a: BigInt, b: BigInt) -> BigInt` | Elixir `band` / Rust・Go・Swift operators `&` | Add | R-bigint-07 |
| `bor` | `bigint.bor(a: BigInt, b: BigInt) -> BigInt` | Elixir `bor` / Rust・Go・Swift operators `\|` | Add | R-bigint-07 |
| `bxor` | `bigint.bxor(a: BigInt, b: BigInt) -> BigInt` | Elixir `bxor` / Rust・Go・Swift operators `^` | Add | R-bigint-07 |
| `bnot` | `bigint.bnot(a: BigInt) -> BigInt` | Elixir `bnot` / Swift `~` | Add | R-bigint-07 |
| `bshl` | `bigint.bshl(a: BigInt, n: Int) -> BigInt` | Elixir `bsl` / operators `<<` | Add | R-bigint-07 |
| `bshr` | `bigint.bshr(a: BigInt, n: Int) -> BigInt` | Elixir `bsr` / operators `>>` | Add | R-bigint-07 |
