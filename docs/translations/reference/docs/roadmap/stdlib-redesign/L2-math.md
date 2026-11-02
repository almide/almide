> Description (inactive source index metadata): Stdlib redesign L2/L3 — math: keep only what has no type-module home; move abs/min/max/sign/pow to int and sqrt/fmin/fmax/fpow to float; add the missing inverse trig, hyperbolic, hypot and cbrt
<a id="l2l3-math"></a>

# L2/L3: math

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-math.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-math.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-math.zh-CN.md)

> Non-normative historical redesign snapshot. The pinned original remains authoritative and unchanged; no automatic synchronization, independent rerun or current implementation verification. Preserve the initial sqrt-to-float proposal and later reversal, the initial inverse-hyperbolic exclusion and later additions, and differing usage snapshots. The original counts (remove 9 / current 23 plus 7) coexist with revised decisions and a larger table; the asserted 7/9 and its named examples are not independently reconciled. Numeric inspection and target agreement are source proposals/claims, not new verified results. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-math.md)

> Last updated: 2026-10-21
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md) / [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)
> Related: [L2-int](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-int.md) / [L2-float](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-float.md)
> Sources: Rust https://doc.rust-lang.org/std/primitive.f64.html (F) / Go https://pkg.go.dev/math / Python https://docs.python.org/3/library/math.html / TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Math / Gleam https://gleam-stdlib.hexdocs.pm/gleam/float.html (F) / Elixir https://www.erlang.org/doc/apps/stdlib/math.html / OCaml https://ocaml.org/manual/latest/api/Float.html (F) / Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.math/ / Swift https://developer.apple.com/documentation/foundation/1779833-pow

(F) = spellings verified by fetching the official function lists on 2026-10-21. Gleam's standard library has no trigonometric functions (float has only `power`, `square_root`, `exponential`, `logarithm` (F)).

<a id="l2-役割の比較"></a>

## L2: Role comparison

| Role | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| Constants π / e | `pi()` `e()` | `consts::PI` | `math.Pi` | `math.pi` | `Math.PI` | — | `:math.pi` | `Float.pi` (F) | `PI` `E` | `.pi` |
| Trigonometric functions | `sin` `cos` `tan` `atan` | `sin` etc. (F) | `Sin` etc. | `sin` etc. | `Math.sin` etc. | — | `:math.sin` etc. | `sin` etc. (F) | `sin` etc. | `sin` etc. |
| Inverse trigonometric / `atan2` | `atan` only | `asin` `acos` `atan2` (F) | `Asin` `Acos` `Atan2` | `asin` `acos` `atan2` | `asin` `acos` `atan2` | — | `asin` `acos` `atan2` | `asin` `acos` `atan2` (F) | `asin` `acos` `atan2` | `asin` `acos` `atan2` |
| Hyperbolic functions | `tanh` only | `sinh` `cosh` `tanh` (F) | `Sinh` `Cosh` `Tanh` | Yes | Yes | — | Yes | Yes (F) | Yes | Yes |
| Exponential / logarithmic | `exp` `log` `log10` `log2` | `exp` `ln` `log10` `log2` (F) | `Exp` `Log` etc. | `exp` `log` etc. | `Math.exp` `log` etc. | `exponential` `logarithm` (F) | `exp` `log` etc. | `exp` `log` etc. (F) | `exp` `ln` etc. | `exp` `log` etc. |
| Square root / cube root / hypotenuse | `sqrt`（`float.sqrt` duplicate） | `sqrt` `cbrt` `hypot` (F) | `Sqrt` `Cbrt` `Hypot` | `sqrt` `cbrt` `hypot` | `sqrt` `cbrt` `hypot` | `square_root` (F) | `sqrt` | `sqrt` `cbrt` `hypot` (F) | `sqrt` `cbrt` `hypot` | `sqrt` `cbrt` `hypot` |
| Power | `pow`（Int）`fpow`（Float） | `i64::pow` `f64::powf` (F) | `Pow` | `pow` `**` | `**` | `int.power` `float.power` (F) | `Integer.pow` (F) | `Float.pow` (F) | `pow` | `pow` |
| Absolute value / minimum / maximum / sign (Int) | `abs` `min` `max` `sign`（`int.*` duplicate） | Int methods | `min` `max` | Built-in | `Math.*` | `int.*` (F) | `abs` | `Int.abs` (F) | `abs` `sign` | `abs` |
| Minimum / maximum (Float) | `fmin` `fmax`（`float.min/max` duplicate） | Float methods (F) | `min` `max` | Built-in | `Math.min` | `float.min` (F) | `min` | `Float.min` (F) | `minOf` | `min` |
| Logarithm of gamma | `log_gamma` | `ln_gamma`（nightly）(F) | `Lgamma` | `lgamma` | — | — | — | — | — | `lgamma` |
| Factorial / binomial coefficient | `factorial` `choose` | — | — | `factorial` `comb` | — | — | — | — | — | — |

<a id="完全性の規則"></a>

### Completeness rules

1. **Location** (R-L0-02): `math` holds only functions without a home in a type module: transcendental functions (trigonometric, inverse trigonometric, hyperbolic, exponential, logarithmic, gamma), constants and integer combinatorics (`factorial`, `choose`). Arithmetic with a determined subject type (`abs`, `min`, `max`, `sign`, `pow`, `sqrt`) belongs in `int` or `float`.
2. **Transcendental families**: provide complete trigonometric (`sin` `cos` `tan`), inverse trigonometric (`asin` `acos` `atan` `atan2`) and hyperbolic (`sinh` `cosh` `tanh`) families. Intentionally omit inverse hyperbolic functions (such as `asinh`) (→ reconsidered in R-math-05; add them) (Rust, Python, JS, OCaml, Swift and others have them in their standard libraries, but use cases are rare and they can be written in 1 line from `log` and `sqrt`).
3. All `math` functions take `Float` (the only exceptions are the `Int` arguments of `factorial` and `choose`). Do not use an `f` prefix to distinguish types (`fmin` `fmax` `fpow`).

<a id="l2-の判定"></a>

### L2 verdict

- Duplication (#3851 B): `math.abs` `min` `max` ≡ `int.*`, `math.sqrt` ≡ `float.sqrt`, `math.fmin` `fmax` ≡ `float.min` `max` → R-math-01.
- Location: `math.sign` (Int), `math.pow` (Int), `math.fpow` (Float) have determined subject types → move them to `int.sign`, `int.pow`, `float.pow` in R-math-01. #3851 retained `math.pow`, `fpow`, `sign` because “there is no `int.pow`, so nothing to consolidate,” but the subsequently decided L0 R-L0-02 says “`math` holds only what has no home in a type module,” so change the verdict to moving them.
- Missing roles: inverse trig `asin` `acos` `atan2` (8/9), hyperbolic `sinh` `cosh` (8/9), `hypot` `cbrt` (7/9) → R-math-02.
- Unchanged: `factorial` wrapping above 20 is accommodated by `bigint` in R-L1-03 ([L2-bigint](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-bigint.md)).

<a id="案"></a>

## Proposals

<a id="r-math-01-型の決まる算術を-intfloat-に移す---採用"></a>

### R-math-01 Move type-specific arithmetic to `int` / `float` — ○ Accepted

- Content:
  - Remove (the retained counterpart already exists): `math.abs` → `int.abs`, `math.min` → `int.min`, `math.max` → `int.max`, (`math.sqrt` → `float.sqrt` is canceled by R-math-03), `math.fmin` → `float.min`, `math.fmax` → `float.max`.
  - Move (an addition/removal pair): `math.sign` → `int.sign(n: Int) -> Int`, `math.pow` → `int.pow(base: Int, exp: Int) -> Int`, `math.fpow` → `float.pow(base: Float, exp: Float) -> Float`. Semantics do not change (`int.pow` wraps; negative `exp` aborts; `int.sign(0)` is 0).
- Supporting L0 rules: R-L0-01, R-L0-02.
- (a) 7/9 languages place these in the subject type's module or methods (Rust, Kotlin and Swift use methods; Gleam, Elixir and OCaml use type modules). Gleam has `int.power` and `float.power` per type (F).
- (b) The bodies are the same (identical calculations in `runtime/rs/src/math.rs` and `int.rs` / `float.rs`, #3851 B). For `int.pow` and `float.pow`, reattach the intrinsic and self-host bodies of `math.pow` / `math.fpow` unchanged. Neither target changes.
- (c) With 2 locations such as `int.abs` and `math.abs`, an LLM cannot guess which location is used. The `f` prefix of `fpow` is a spelling in none of the 9 languages; `float.pow` follows predictably from the rule “Float functions are in `float.`.”
- (d) Remove 9, add 3 (`int.sign`, `int.pow`, `float.pow`; tables in L2-int / L2-float). Use `@deprecated(since=15, use="int.abs")` and similar, removal epoch 16. All `almide fix` changes are 1-to-1 (for example, `math.fpow(a, b)` → `float.pow(a, b)`). Usage counts (spec/tests/docs/stdlib): `abs` 13/4/9/0, `min` 7/1/10/0, `max` 7/0/10/0, `sqrt` 22/0/11/2 (+20 research), `fmin` 12/4/10/2, `fmax` 11/0/8/0, `sign` 6/0/6/1, `pow` 20/1/19/3 (+8 research), `fpow` 35/3/12/6. Rewrite examples at llms.txt:413 / 419 / 426.
- Proposed gate: prevent recurrence across `math`↔`int`/`float` using synonym-gate rule 5 of #3851 F (identical bodies across modules). Add the R-L0-G location check (the first argument of public `math` functions is `Float`, with only `factorial` / `choose` excepted) with an allowlist of 0.
- issue: #3851 B (existing; append the changed verdict for `sign`, `pow`, `fpow` to this issue)

<a id="r-math-02-逆三角関数双曲線関数hypotcbrt-を足す---採用"></a>

### R-math-02 Add inverse trigonometric and hyperbolic functions, `hypot`, `cbrt` — ○ Accepted

- Content: `math.asin(x: Float) -> Float`, `math.acos(x: Float) -> Float`, `math.atan2(y: Float, x: Float) -> Float`, `math.sinh(x: Float) -> Float`, `math.cosh(x: Float) -> Float`, `math.hypot(x: Float, y: Float) -> Float`, `math.cbrt(x: Float) -> Float`. Out-of-domain inputs yield NaN (`asin(2.0)`), following existing `math.*` conventions.
- Supporting L0 rules: L1 addition criterion (7–8/9), completeness rule 2. The argument order of `atan2` (y, x) is the same in all 9 languages.
- (a) Rust (F), Go, Python, JS, Elixir (`:math`), OCaml (F), Kotlin, Swift. Only Gleam lacks them.
- (b) Pure computation. Since wasm has no trigonometric instructions, write musl ports in Almide like the existing `math_trig.almd` / `math_atan.almd`, and include them in ADR-0016 numeric-determinism inspection. On native, use the same Almide bodies rather than libm, or verify byte agreement with fixtures.
- (c) An LLM's first choice for angle calculations is `atan2`. Currently, it uses `atan(y / x)` and gets the quadrant wrong.
- (d) Add 7. Usage count 0.
- Proposed gate: add 7 rows to the existing numeric inspection (semantics-manifest rows and inspection ledger), and pin byte agreement between both targets with wasm_cross fixtures.
- issue: #3926

<a id="r-math-03-実数の数学関数は-math-に集めるsqrt-を-math-に戻しfloatsqrt-を撤去---採用r-math-01-の-sqrt-の部分を見直す"></a>

### R-math-03 Collect real-number mathematical functions in `math` (return `sqrt` to `math`, remove `float.sqrt`) — ○ Accepted (revises the `sqrt` part of R-math-01)

- Now: R-math-01 removed `math.sqrt` in favor of `float.sqrt`, while R-math-02 added `cbrt` / `hypot` to `math`. This puts only `sqrt` in `float`, while `cbrt` / `hypot`, trig and exponentials are in `math`.
- Proposal: retain `math.sqrt(x: Float) -> Float` and remove `float.sqrt`. Define the placement boundary in this 1 sentence: “Roles that also exist for `Int` (`abs`, `min`, `max`, `clamp`, `sign`, `pow`) belong in type modules. Mathematical functions specific to real numbers (`sqrt`, `cbrt`, `hypot`, trig, exponentials, logarithms, gamma) belong in `math`. Rounding, predicates, constants, bit patterns and conversions belong in `float` as representation functions.”
- Supporting L0 rules: R-L0-02 (one rule determines placement; replace the L0 wording with this 1 sentence), R-L0-00 item 2, R-L0-01.
- (a) 8 of the 9 languages put `sqrt` and `sin` in the same place, and 6 of those (Go, Python, JS, Elixir, Kotlin, Swift) put them on the math side. No language puts `sqrt` in a different place from `cbrt` / `hypot`.
- (c) In the current proposal, `math.cbrt(x)` passes but `math.sqrt(x)` gets E002. `math.sqrt` is the spelling most readily written by LLMs.
- (d) Remove `float.sqrt` (`@deprecated(since=15, use="math.sqrt")`, epoch 16, 1-to-1 with `almide fix`). Cancel removal of `math.sqrt`. Usage counts are `math.sqrt` 17/0/5/0/18 and `float.sqrt` 14/2/4/1/0 (spec/tests/docs/stdlib/research); this direction requires fewer rewrites.
- Completeness rule: all arguments of public `math` functions are `Float` (except `factorial` / `choose`). Do not put the same name in both `float` and `math`.
- Proposed gate: add a `math`/`float` name-placement table to R-L0-G and compare it with both modules' public names.
- issue: #3972

<a id="r-math-04-expm1log1p-を足す---採用"></a>

### R-math-04 Add `expm1` / `log1p` — ○ Accepted

- Now: `math.exp` / `math.log` exist, but there are no functions to accurately compute `exp(x) - 1` / `log(1 + x)` for `x` near 0. Composition loses significant digits, so this is not a convenience function but a role that cannot be expressed with other functions.
- Proposal: `fn math.expm1(x: Float) -> Float`, `fn math.log1p(x: Float) -> Float`.
- Rationale: the L1 addition criterion (at least 5 languages).
- (a) Rust `exp_m1`/`ln_1p`, Go `Expm1`/`Log1p`, Python `expm1`/`log1p`, JS `Math.expm1`/`log1p`, Kotlin `expm1`/`ln1p`, OCaml `expm1`/`log1p`.
- (b) Pure computation. As in R-math-02, write a musl port in Almide and include it in ADR-0016 numeric-determinism inspection.
- (d) Add 2.
- issue: #3980

<a id="r-math-05-逆双曲線関数-asinhacoshatanh-を足す完全性の規則-2-の見直し---採用"></a>

### R-math-05 Add inverse hyperbolic `asinh` / `acosh` / `atanh` (revise completeness rule 2) — ○ Accepted

- Now: completeness rule 2 said “omit inverse hyperbolic functions because they have few use cases.” However, Rust, Go, Python, JS, Kotlin, OCaml and Swift have them in their standard libraries, meeting the L1 addition criterion (at least 5 languages). There is no reason to apply the criterion to other functions but exempt only these.
- Proposal: `fn math.asinh(x: Float) -> Float`, `fn math.acosh(x: Float) -> Float`, `fn math.atanh(x: Float) -> Float`. Out-of-domain inputs (`x < 1` for `acosh`, `|x| > 1` for `atanh`) return NaN like `math.asin`.
- (b) Same as R-math-04.
- (d) Add 3. Both forward and inverse trigonometric / hyperbolic families become complete.
- Completeness rule (replaces 2): the trigonometric and hyperbolic families each have 3 forward and 3 inverse functions.
- issue: #3980

<a id="l3-関数表"></a>

## L3: Function table

The current 23 `math` functions and 7 additions.

| Function | Signature (current, mut notation) | Main spellings in other languages | Verdict | Proposal |
|---|---|---|---|---|
| `abs` | `math.abs(n: Int) -> Int` | Rust `abs` / Gleam `int.absolute_value` (F) / OCaml `Int.abs` (F) / Kotlin `absoluteValue` (F) | Remove | R-math-01（`int.abs`） |
| `atan` | `math.atan(x: Float) -> Float` | Rust `atan` (F) / Go `Atan` / Python・JS `atan` / OCaml `atan` (F) / Kotlin `atan` | Unchanged | — |
| `choose` | `math.choose(n: Int, k: Int) -> Int` | Python `math.comb` / Others — | Unchanged | — |
| `cos` | `math.cos(x: Float) -> Float` | Rust `cos` (F) / Go `Cos` / Python・JS `cos` / OCaml `cos` (F) | Unchanged | — |
| `e` | `math.e() -> Float` | Rust `consts::E` / Go `math.E` / Python `math.e` / JS `Math.E` / Kotlin `E` | Unchanged | — |
| `exp` | `math.exp(x: Float) -> Float` | Rust `exp` (F) / Go `Exp` / Gleam `exponential` (F) / OCaml `exp` (F) | Unchanged | — |
| `factorial` | `math.factorial(n: Int) -> Int` | Python `math.factorial` / Others — | Unchanged | —（for large values, see [L2-bigint](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-bigint.md)） |
| `fmax` | `math.fmax(a: Float, b: Float) -> Float` | Rust `f64::max` (F) / Go `max` / Gleam `float.max` (F) / OCaml `Float.max` (F) | Remove | R-math-01（`float.max`） |
| `fmin` | `math.fmin(a: Float, b: Float) -> Float` | Rust `f64::min` (F) / Go `min` / Gleam `float.min` (F) / OCaml `Float.min` (F) | Remove | R-math-01（`float.min`） |
| `fpow` | `math.fpow(base: Float, exp: Float) -> Float` | Rust `powf` (F) / Go `Pow` / Gleam `float.power` (F) / OCaml `Float.pow` (F) / Kotlin `pow` | Remove | R-math-01（`float.pow`） |
| `log` | `math.log(x: Float) -> Float` | Rust `ln` (F) / Go `Log` / Python・JS `log` / OCaml `log` (F) / Kotlin `ln` | Unchanged | — |
| `log10` | `math.log10(x: Float) -> Float` | Rust `log10` (F) / Go `Log10` / Python・JS `log10` / OCaml `log10` (F) | Unchanged | — |
| `log2` | `math.log2(x: Float) -> Float` | Rust `log2` (F) / Go `Log2` / Python・JS `log2` / OCaml `log2` (F) | Unchanged | — |
| `log_gamma` | `math.log_gamma(x: Float) -> Float` | Rust `ln_gamma`（nightly）(F) / Go `Lgamma` / Python `lgamma` / Swift `lgamma` | Unchanged | — |
| `max` | `math.max(a: Int, b: Int) -> Int` | Rust `max` / Gleam `int.max` (F) / OCaml `Int.max` (F) | Remove | R-math-01（`int.max`） |
| `min` | `math.min(a: Int, b: Int) -> Int` | Rust `min` / Gleam `int.min` (F) / OCaml `Int.min` (F) | Remove | R-math-01（`int.min`） |
| `pi` | `math.pi() -> Float` | Rust `consts::PI` / Go `math.Pi` / Python `math.pi` / JS `Math.PI` / OCaml `Float.pi` (F) | Unchanged | — |
| `pow` | `math.pow(base: Int, exp: Int) -> Int` | Rust `i64::pow` / Gleam `int.power`（Float）(F) / Elixir `Integer.pow` (F) | Remove | R-math-01（`int.pow`） |
| `sign` | `math.sign(n: Int) -> Int` | Rust `signum` / Kotlin `sign` (F) / JS `Math.sign` | Remove | R-math-01（`int.sign`） |
| `sin` | `math.sin(x: Float) -> Float` | Rust `sin` (F) / Go `Sin` / Python・JS `sin` / OCaml `sin` (F) | Unchanged | — |
| `sqrt` | `math.sqrt(x: Float) -> Float` | Rust `sqrt` (F) / Go `Sqrt` / Gleam `float.square_root` (F) / OCaml `Float.sqrt` (F) | Unchanged | R-math-03（R-math-01 revision） |
| `tan` | `math.tan(x: Float) -> Float` | Rust `tan` (F) / Go `Tan` / Python・JS `tan` / OCaml `tan` (F) | Unchanged | — |
| `tanh` | `math.tanh(x: Float) -> Float` | Rust `tanh` (F) / Go `Tanh` / Python・JS `tanh` / OCaml `tanh` (F) | Unchanged | — |
| `asin` | （Add）`math.asin(x: Float) -> Float` | Rust `asin` (F) / Go `Asin` / Python・JS `asin` / OCaml `asin` (F) / Kotlin `asin` / Swift `asin` | Add | R-math-02 |
| `acos` | （Add）`math.acos(x: Float) -> Float` | Rust `acos` (F) / Go `Acos` / Python・JS `acos` / OCaml `acos` (F) / Kotlin `acos` / Swift `acos` | Add | R-math-02 |
| `atan2` | （Add）`math.atan2(y: Float, x: Float) -> Float` | Rust `y.atan2(x)` (F) / Go `Atan2` / Python・JS `atan2` / OCaml `atan2` (F) / Kotlin `atan2` / Swift `atan2` | Add | R-math-02 |
| `sinh` | （Add）`math.sinh(x: Float) -> Float` | Rust `sinh` (F) / Go `Sinh` / Python・JS `sinh` / OCaml `sinh` (F) | Add | R-math-02 |
| `cosh` | （Add）`math.cosh(x: Float) -> Float` | Rust `cosh` (F) / Go `Cosh` / Python・JS `cosh` / OCaml `cosh` (F) | Add | R-math-02 |
| `hypot` | （Add）`math.hypot(x: Float, y: Float) -> Float` | Rust `hypot` (F) / Go `Hypot` / Python・JS `hypot` / OCaml `hypot` (F) / Kotlin `hypot` | Add | R-math-02 |
| `cbrt` | （Add）`math.cbrt(x: Float) -> Float` | Rust `cbrt` (F) / Go `Cbrt` / Python・JS `cbrt` / OCaml `cbrt` (F) / Kotlin `cbrt` | Add | R-math-02 |
| `expm1` | （Add）`math.expm1(x: Float) -> Float` | Rust `exp_m1` / Go `Expm1` / Python・JS `expm1` | Add | R-math-04 |
| `log1p` | （Add）`math.log1p(x: Float) -> Float` | Rust `ln_1p` / Go `Log1p` / Python・JS `log1p` | Add | R-math-04 |
| `asinh` | （Add）`math.asinh(x: Float) -> Float` | Rust `asinh` / Go `Asinh` / Python・JS `asinh` | Add | R-math-05 |
| `acosh` | （Add）`math.acosh(x: Float) -> Float` | Rust `acosh` / Go `Acosh` / Python・JS `acosh` | Add | R-math-05 |
| `atanh` | （Add）`math.atanh(x: Float) -> Float` | Rust `atanh` / Go `Atanh` / Python・JS `atanh` | Add | R-math-05 |
