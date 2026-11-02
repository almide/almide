Original description metadata: How other languages range-check integer literals, and where Almide should land
<a id="integer-literal-domain--cross-language-survey-and-almides-target"></a>

# Integer literal domain — cross-language survey and Almide's target

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](integer-literal-domain.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](integer-literal-domain.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](integer-literal-domain.zh-CN.md)

> Non-normative reader snapshot of the pinned source; original remains authoritative. No automatic synchronization, independent implementation validation or execution of quoted instructions. Historical status, plans and comparative claims retain their source scope.
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/integer-literal-domain.md)

> Source caveat: the measurements, versions and implementation status are the source author’s 2026-07-26 record, not new runs or a current-language survey. Statements about uniqueness, precedents and diagnostic superiority are scoped historical author claims; no MSR improvement is measured here. The temporary accepted UInt64 range is not its declared mathematical range. Pending carrier expansion and old radix limitations are not independently checked for current status. Native int width in the OCaml observation is platform-specific, not a universal width guarantee. Preserved diagnostic blocks include Japanese annotations translated outside the blocks.


While adding C-173 (integer-literal type domains) on 2026-07-26, we discovered that `UInt64` **failed identically on both targets** in the upper half of its declared domain (#872). This records hands-on checks of how other languages handle this and the resulting destination for Almide.

The survey was not recollection: **all cases were run locally**. Versions: Rust 1.94.1 / Go 1.26.1 / Swift 6.1.2 / Zig 0.15.2 / .NET 10.0.105 / Apple clang 17.0.0 / javac 26 / Python 3.11.9 / OCaml 5.4.1.

<a id="実測マトリクス"></a>

## Measured matrix

| Language | `u64 = u64::MAX` | `u64 = -5` | `u64 = -0` | `u64 = u64::MAX+1` |
|---|---|---|---|---|
| Rust | ✅ Correct | ❌ E0600 | ❌ E0600 | ❌ out of range |
| Go | ✅ Correct | ❌ overflows | ✅ `0` | ❌ overflows |
| Swift | ✅ Correct | ❌ overflows into unsigned | ✅ `0` | ❌ |
| Zig | ✅ Correct | ❌ cannot represent | ❌ **ambiguous** | ❌ |
| C# | ✅ Correct | ❌ CS0031 | ✅ `0` | ❌ |
| C | ✅ Correct | ⚠️ **Accepted** (wraps to `…611`) | ✅ `0` | — |
| Java | No such type | No such type | No such type | ❌ Integer too large |
| **Almide (before the fix)** | ⚠️ Accepted as **`0`**, **`-1`** after `/` | ⚠️ **Accepted**, then rustc E0600 | ✅ `0` | ⚠️ Accepted as `0` |
| **Almide (after C-173)** | ❌ E024 carrier | ❌ E024 sign | ✅ `0` | ❌ E024 magnitude |

<a id="軸1--符号なし型に負リテラル"></a>

## Axis 1 — Negative literals for unsigned types

**Every language except C rejects them.** Only C accepts implicit mod 2⁶⁴ wrapping, without even a warning unless `-Wsign-conversion` is enabled (with it, warnings appear: 2 observed). This is a well-known footgun that later languages uniformly closed.

Before the fix, Almide was “C's behavior + rustc failing downstream,” so it **was not even C**.

There are **2 approaches** to explaining rejection.

- **Talk about the operator** (Rust) — `cannot apply unary operator '-' to type 'u64'`. Structurally accurate: `-5` is `Neg::neg(5)` and `u64` has no `Neg` implementation. However, it **does not explain how to fix it at all**.
- **Talk about the value range** (Go / Swift / Zig / C#) — say that the type cannot represent the value. Zig's `type 'u64' cannot represent integer value '-5'` is the most concise.

Almide adopts **the value-range approach**, and additionally **names the signed type of the same width**:

```
error[E024]: integer literal '-5' is out of range for UInt64
  hint: UInt64 is unsigned — it has no negative values at all; drop the '-',
        or use the signed Int64 if the value can go below zero
```

**None of the surveyed languages names the alternative type.** In light of the mission (the language LLMs can write most accurately), making a repair candidate explicit benefits modification survival rate, so this deliberately goes a step beyond the other languages.

<a id="軸2--キャリア問題本丸"></a>

## Axis 2 — The carrier problem (the central issue)

**Almide's situation has no precedent.** The reason is simple:

> A soundly designed language **does not make its internal literal representation narrower than the target type**.

| Strategy | Languages | Measured evidence |
|---|---|---|
| Arbitrary precision | Go, Zig, Python | `(1 << 100) >> 100` returns `1` (intermediate values beyond machine types are preserved). The Go specification requires implementations to **represent integer constants with at least 256 bits** |
| Fixed, but at least as wide as the widest type | Rust (`u128`), C (≥ `uintmax_t`), C#, Swift | `u64::MAX` is exact |
| **Signed carrier only + no unsigned type** | **Java, OCaml** | Provide unsigned interpretation through **operations, not types** (below) |

Java and OCaml matter here. **A signed carrier is not itself a defect.** Both are consistent with their carriers and provide unsigned operations **as functions** (measured):

```
Java    Long.toUnsignedString(-1L)          → 18446744073709551615   ✅
OCaml   Int64.unsigned_div (-1L) 2L         → 9223372036854775807    ✅
OCaml   Printf.printf "%Lu" (-1L)           → 18446744073709551615   ✅
```

OCaml goes further: **native `int` is 63-bit** (because it is tagged; `Sys.int_size = 63`, `max_int = 4611686018427387903`, measured). It does not claim 64 bits and requires explicit use of the `Int64` module when needed. **Do not declare a width you cannot carry** is precisely the stance Almide lacked.

Thus Almide's defect is not “an i64 carrier” but **a mismatch between the declared type and the carrier**. It claims a `UInt64` type but cannot carry its domain. And `uint64.to_string`, which displayed signed values via `int.from_uint64`, did not even provide the “unsigned through operations” that Java/OCaml implement correctly.

<a id="帰結-着地点は-2-つある"></a>

### Consequence: there are 2 destinations

**(1) Widen the carrier (Rust model)** — widen `IrExprKind::LitInt { value: i64 }` to the equivalent of `i128`, add `NTy::U64` and `div_u`/`rem_u`/`lt_u` to MIR, and remove the i64 pivot from `uint64.to_string`. `UInt64` becomes what it declares.

**(2) Drop the unsigned 64-bit “type” (Java / OCaml model)** — stay consistent with an i64 carrier and provide unsigned interpretation through `uint64.*` functions. Cheap, but takes a type away from existing users. **The current state is not even (2)** — `uint64.to_string` displays signed values, falling short of what `Long.toUnsignedString` / `Int64.unsigned_div` provide.

**Recommendation: (1).** Languages with a `UInt64` type take (1) without exception, while Java / OCaml, which take (2), **do not have that type in the first place**. Declaring a type entails responsibility for carrying its domain. If choosing (2), removing the `UInt64` type must be included or the same falsehood remains.

<a id="872-への訂正"></a>

### Correction to #872

Issue #872 said “add `NTy::U64` to MIR,” but that alone **does not fix it**. Literals collapse in the i64 carrier before reaching MIR, so the order is:

1. **Widen the literal representation** (the root cause; like Rust's `LitKind::Int` = `u128`)
2. Unsigned MIR operations (`div_u` / `rem_u` / `lt_u`, selected by operand type)
3. Remove the i64 pivot from `uint64.to_string` / `to_float*`
4. Remove E024's carrier cap and restore the `UInt64` recommendation in E024.md and llms.txt

Doing only 2 first is meaningless without 1.

The temporary cap is “an anomalous state without precedent in other languages,” also providing **a reason not to leave it in place for long**.

<a id="軸3---0-は本当に三分する"></a>

## Axis 3 — `-0` genuinely splits three ways

Only here is the industry divided, requiring a rationale for Almide's choice.

| Behavior | Languages |
|---|---|
| **Accept** as `0` | Go, Swift, C#, C |
| **Reject** (as an operator) | Rust |
| **Reject** (as ambiguous, even for signed types) | Zig |

Zig is distinctive: even for `i64` it says `integer literal '-0' is ambiguous` (measured). Its reason is different: inability to distinguish integer `0` from floating-point `-0.0`.

Rust's rejection is **the natural consequence of choosing the operator approach in axis 1**, not special treatment of `-0`.

**Almide keeps accepting it.** Not because this is the majority, but **because it chose the value-range approach in axis 1**:

> E024's rule asks “Can this type represent this value?” `UInt64` can represent `0`. Rejecting `-0` would therefore contradict the rule itself.

Rejecting it would require switching to the operator approach, discarded in axis 1 (because it does not explain the fix). **Consistency of approach takes precedence over following Rust.**

We briefly considered “a rule without exceptions is easier for LLMs to retain” = rejection, but that mistakes the formulation of the rule. Almide's rule is not “no `-` on unsigned types (exception: 0)” but “no value the type cannot represent”: **there is no exception**.

<a id="軸4--診断の質-値域を書く"></a>

## Axis 4 — Diagnostic quality: state the range

Rust **explicitly states the range in a note**:

```
error: literal out of range for `u64`
  = note: the literal `18446744073709551616` does not fit into the type `u64`
          whose range is `0..=18446744073709551615`
```

Almide's magnitude hint only said “use a literal within the type's range,” **without saying what that range is**. Few readers, human or LLM, can recite the upper bound of `UInt32`. Writing the range is cheap and directly serves the mission.

→ Implemented. E024's magnitude hint outputs the concrete range.

<a id="almide-がこの-survey-から採った--採らなかったもの"></a>

## What Almide adopted / did not adopt from this survey

| Item | Decision | Source |
|---|---|---|
| Reject negative literals for unsigned types | Adopted | All of Rust/Go/Swift/Zig/C# |
| Explain using value ranges | Adopted | Go/Swift/Zig/C# (not Rust's operator approach) |
| Name an alternative signed type | **Original** | No other language does it |
| State a concrete range in the hint | Adopted | Rust |
| Accept `-0` | Adopted | Go/Swift/C# (not Rust/Zig; reason in axis 3) |
| Widen the carrier to at least the widest type | **Incomplete → #872** | All of Rust/Go/Zig/C/C#/Swift |
| Provide functions without an unsigned type | Not adopted | Java (declaring a type means carrying its domain) |

<a id="付録--各言語の実測メッセージ全文"></a>

## Appendix — Complete measured messages for each language

Primary material for designing diagnostics. All were run locally with the versions above.

<a id="符号なし型に負リテラルu64---5"></a>

### Negative literal for an unsigned type (`u64 = -5`)

```
Rust   error[E0600]: cannot apply unary operator `-` to type `u64`
Go     cannot use -5 (untyped int constant) as uint64 value in variable declaration (overflows)
Swift  error: negative integer '-5' overflows when stored into unsigned type 'UInt64'
Zig    error: type 'u64' cannot represent integer value '-5'
C#     error CS0031: Constant value '-5' cannot be converted to a 'ulong'
C      （通る。-Wsign-conversion で警告 2 件。値は 18446744073709551611）
Almide error[E024]: integer literal '-5' is out of range for UInt64
       hint: UInt64 is unsigned — it has no negative values at all; drop the '-',
             or use the signed Int64 if the value can go below zero
```

Translation of the Japanese C line in the preserved block: accepted; 2 warnings with -Wsign-conversion; the value is 18446744073709551611.

Zig is the most concise, Swift the most explanatory. **Only Almide supplies a candidate replacement type.**

<a id="型の域を超える大きさu64--u64max--1"></a>

### Magnitude exceeding the type's domain (`u64 = u64::MAX + 1`)

```
Rust   error: literal out of range for `u64`
       = note: the literal `18446744073709551616` does not fit into the type `u64`
               whose range is `0..=18446744073709551615`
       = note: `#[deny(overflowing_literals)]` on by default
Go     cannot use 18446744073709551616 (untyped int constant) as uint64 value in variable declaration (overflows)
Swift  error: integer literal '18446744073709551616' overflows when stored into 'UInt64'
Zig    error: type 'u64' cannot represent integer value '18446744073709551616'
C#     error CS1021: Integral constant is too large
Java   エラー: 整数が大きすぎます。      ← long のみ。符号なし型が無い
Almide error[E024]: integer literal '18446744073709551616' is out of range for UInt64
       hint: UInt64 would silently fold to 0 here; its range is 0..=9223372036854775807,
             so use a literal within it, or model larger magnitudes as Float (lossy)
             or a parsed string
```

Translation of the Japanese Java line: error: integer too large. Only long; there is no unsigned type.

**Only Rust and Almide explicitly state the range.** The others stop at “too large.” The `UInt64` range Almide displays is not the declared domain but **the range actually accepted** (after #872's cap). Stating a range the compiler does not uphold in a diagnostic is worse than stating no range.

<a id="-0"></a>

### `-0`

```
Rust   error[E0600]: cannot apply unary operator `-` to type `u64`
Zig    error: integer literal '-0' is ambiguous        ← i64 に対しても同じ
Go     0
Swift  0
C#     0
Almide 0
```

Translation of the Japanese Zig annotation: the same also applies to i64.

<a id="関連"></a>

## Related

- Contract C-173 — `docs/contracts/contracts.toml`; ALS-M14 is normative
- #872 — the upper half of `UInt64` (axis 2 of this document determines the repair order)
- #873 — lexer radix literals (`0x` alone silently becomes 0; `0b`/`0o` unsupported)
- `docs/diagnostics/E024.md` — 3 deviations and 3 hints
- `tests/int_literal_domain_test.rs` — fixing the classification in place
