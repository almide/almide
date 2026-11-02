<a id="als--言語意味論semantics"></a>

# ALS — Language Semantics

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](semantics.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](semantics.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](semantics.zh-CN.md)

> Non-normative reader translation of the [pinned original](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/semantics.md). Normative status and the contract references belong to the canonical original, not this translation. This is a commit-pinned snapshot, not generator input or an automatically synchronized document. Original sources and machine consumers remain unchanged. Examples, measurements, target parity, and conformance claims have not been independently executed or revalidated here.

> Last updated: 2026-08-13

Observational norms for patterns, records, variants, effect-fn, generics, and accumulator loops.
Each section is referenced by the contract ledger's `spec` key; verification tests are supplied by each contract's evidence (spec/wasm_cross fixtures).

<a id="als-m1-パターンマッチの束縛規範"></a>

## ALS-M1 Binding norms for pattern matching

Option/Result construction and matching, nested constructor patterns, tuple patterns that test variant constructors, and nested subpatterns in let destructuring **bind every leaf as declared**. Matching executes the first matching arm from top to bottom. Bindings of heap payloads (such as String) can be read as borrows without transferring ownership. Nested forms whose arm bodies are themselves variant matches also bind payloads at every level in `let` bindings (positions that branch_lift lifts into helper functions because of heap values).

**Constructor patterns name cases that the subject has (static norm)**:
`some`/`none` destructure Option, `ok`/`err` destructure Result, and user variants match only their own cases. A pattern naming a case that the subject does not have is rejected **at check time with E048**, identically on both targets (type checking precedes target branching). Four cells: cross-family mismatches in both directions, a built-in carrier against a user variant, and a user case from another variant (or carrier). Before #1341, only scalar subjects were rejected, so `match list.get(xs, 0) { ok(a) => … }` (`list.get` returns `T?`) survived to codegen with its payload bound as `Ty::Unknown`. On native this produced a `[COMPILER BUG]` banner (**invalid Rust output** for a mismatched user case); on wasm it hit the executable-subset wall. This applies only to subjects whose structure is determined; types still under inference and opaque aliases represented as `Named` are silently allowed through.
Tests: `spec/wasm_cross/nested_variant_match_bind.almd`, `tests/diagnostics/e048-cross-family-variant-pattern/`, `tests/diagnostics/e048-foreign-ctor-case/`.
Contracts: C-044, C-070, C-073, C-091, C-113, C-114, C-269.

<a id="als-m2-レコード意味論"></a>

## ALS-M2 Record semantics

A record spread update `{ ...r, f: v }` creates a **new value** (the original is unchanged, following ALS-C5 value semantics). Field representation is identical whether inferred or explicit. Generic fields are sized according to their instantiated type at construction. Phantom type parameters do not affect observable representation. Cross-module monomorphization has the same observations.
Contracts: C-046, C-072, C-078, C-092, C-123.

<a id="als-m3-変種型adt"></a>

## ALS-M3 Variant types (ADTs)

User-defined variants have independent namespaces (coexisting with standard names such as `Box`). Mutually recursive variants, variants with anonymous-record payloads, and construction inside a module with observation from outside all behave as declared, independently of the target.
Contracts: C-043, C-076, C-079, C-093.

<a id="als-m4-effect-fn-の脱糖規範"></a>

## ALS-M4 Desugaring norms for effect fn

> **Consistency note (non-normative, 2026-10-31)**: The auto-`?` description below records an old implementation. Adopted [ADR-0008](../../../../../adr/0008-explicit-propagation-only.md) and C-217 record the removal of implicit propagation; C-345 records preservation of Result in argument positions. For current usage, see [effect-fn-call-semantics](../../../../../specs/effect-fn-call-semantics.md) and `spec/lang/explicit_propagation_test.almd`. The C-064 contract body still contains the old auto-`?` description. This note does not rewrite that contract; it identifies an inconsistency to be resolved on the judge side.

Automatic insertion of `?` on can-err calls inside effect fn **does not depend on binding form** (the same rule applies to let / tail / argument positions). Auto-`?` in constructor positions follows the same rule. Err from `!` propagates to the function boundary even from a `for` loop body. Tail self-recursion is **converted to a loop with O(1) stack usage** (no overflow even at depth 10^6), except for an explicit `err($var)` rewrapping tail—the form for which C-069 deliberately declines loop rewriting on the v1 leg.
Contracts: C-064, C-068, C-069, C-119, C-135.

<a id="als-m5-ジェネリクスと推論"></a>

## ALS-M5 Generics and inference

Empty `map.from_list`/`set.from_list` resolve their element types from use sites. Generic functions inside lambdas with inferred parameters, UFCS protocol calls, and HOF parameters in nested lambdas retain inference links. `+` over a type parameter follows the norms of its actual type (string/list/number). Default arguments referring to preceding parameters are evaluated on each call. The payload type of `unwrap_or` is also resolved from the default side. Combinators lower to valid code on both targets even with Float instantiation.
Contracts: C-080, C-081, C-082, C-089, C-094, C-097, C-126, C-127, C-142, C-145, C-151.

<a id="als-m6-蓄積ループの規範"></a>

## ALS-M6 Norms for accumulator loops

`var` accumulation (scalars, owned handles, mutual dependencies, and mixed mutation), list iteration and materialization of call-result elements, tail-recursive accumulation forms, and let-bound heap branches inside loops are observationally consistent with sequential semantics (the result of evaluating one iteration at a time, in order).
Contracts: C-102, C-104, C-105, C-117, C-174.

<a id="als-m7-識別子の独立性"></a>

## ALS-M7 Identifier independence

Function and variable names remain usable even when they collide with reserved words of the target language (such as Rust keywords). Names must not leak into observations because of code-generation requirements.
Contracts: C-088.

<a id="als-m8-演算子の全域規範"></a>

## ALS-M8 Operator norms across all operand forms

Comparison and equality operators produce the same results for all operand forms (literals, variables, fields, call results, and compound values). The literal `-9223372036854775808` (i64::MIN) is representable and must not be folded to 0.
Contracts: C-083, C-099.

<a id="als-m9-静的検査unit-変異子"></a>

## ALS-M9 Static checking (Unit mutators)

An expression that **reassigns the result of a Unit-returning in-place mutator to a non-Unit binding** (`xs = list.push(xs, x)`) is rejected with **E001**. The mutator operates in place, and writing its “return value” back is detected as a type mismatch. A plain `let x = list.push(…)` merely binds Unit and receives only an unused warning (measured in 0.59.1).
Contracts: C-057.

<a id="als-m10-分岐からの-heap-束縛"></a>

## ALS-M10 Heap bindings from branches

A heap value bound from an if/match arm (`let s = if c then "a" else make()`) is the selected arm's value itself; observing an empty value or another arm's value is non-conforming. The same applies to piping into a lambda with a block body.
Contracts: C-106, C-115.

<a id="als-m11-unwrap-の脱糖"></a>

## ALS-M11 Desugaring unwrap

> **Context clarification (non-normative)**: The following describes the Result propagation contexts covered by C-108. It does not reinterpret all plain unwraps, such as those inside `test` (the stop-on-failure behavior distinguished by C-222), or Option's `!`, as returning Err. For usage by context, see [result-option-effect](../../../../../specs/result-option-effect.md).

`!` (unwrap) and let-unwrap follow a single rule in every position (let / tail / argument / nested): “extract the Ok payload; on Err, propagate it as the function's Err.”
Contracts: C-108.

<a id="als-m12-heap-要素リスト操作の一般性"></a>

## ALS-M12 Generality of heap-element list operations

Parameter passing, join/len/indexing/iteration, and classification/transformation operations on List[String]/List[Value] (including zip_with element-type routing) preserve element ownership and are observationally consistent with native.
Contracts: C-045, C-100, C-101, C-141, C-147, C-148.

<a id="als-m13-mut-パラメータの-in-place-変異"></a>

## ALS-M13 In-place mutation of mut parameters

In-place mutation through a `mut` parameter is reflected in the caller's `var` (an exception to ALS-C5). Callers may pass only `var` bindings; `let`, non-`mut` parameters, and temporaries are E032. Only a declared `mut` opens this exception: standard-library in-place mutators (`list.push`/`pop`/`clear`, `map.clear`, `string.push`/`clear`, and the `bytes` writer family) also declare their receivers `mut`. There is no path that changes the caller's value through a non-`mut` parameter (the plain-parameter path for `Bytes` was closed at dialect epoch 5, C-132). This also holds for containers that **reallocate** (List / String / Bytes capacity growth), and writes back to the caller's slot at every call position, including value-returning callees, nested expression positions, and record-field arguments (`f(b.items, …)`). The actual argument is copied at call time and written back after the call: if the callee reads the module-level `var` that is the root of the actual argument, it sees the pre-call value (C-132).
Contracts: C-061, C-110, C-132, C-136.

<a id="als-m14-整数リテラルの型域静的規範"></a>

## ALS-M14 Type ranges of integer literals (static norm)

An integer literal must be **a value representable by its contextual type**. An unrepresentable literal is a **checker error (E024)** on both targets; it is neither silently folded to 0 nor allowed to fail to build on just one target. There are three kinds of deviation, each requiring a different fix:

- **Magnitude** — the value is too large for the width (for example, `i64::MAX + 1` for `Int`). Fix it with a smaller literal or a wider type.
- **Sign** — a negative literal for an unsigned type (for example, `-5` for `UInt64`). This is not a magnitude issue: unsigned types contain no negative values. Remove `-` or use a signed type of the same width (`UInt64` → `Int64`). `-0` conforms because it is `0`.
- **Carrier (resolved)** — values in the upper half of UInt64, such as `18446744073709551615`, formerly produced E024. The unsigned lane arrived in 0.37.0 (C-179 and the SUPERSEDED section of C-173), and these values are now accepted, operated on, and printed as unsigned. The remaining deviations are the two above (magnitude and sign).

Contracts: C-173, C-179.

<a id="als-m15-effect-fn-型スロット"></a>

## ALS-M15 effect fn type slots

Prefixing a function type with `effect` makes it an **effect slot**: `effect (A) -> B` (or `effect fn(A) -> B`). A lambda checked against an effect slot receives effect-fn body discipline: effect calls are allowed, and `!` propagates into that lambda's own failure channel. The slot has **one runtime shape**, the effect carrier `(A) -> Result[B, String]`: a pure lambda's value tail is wrapped in `ok(...)`, while a lambda using `!` stays as it is. The caller (HOF) receives the same shape with either spelling.

- Effect slots accept **both** pure and fallible lambdas.
- Calling an effect-fn-typed **value** is itself an effect call and produces the carrier `Result[B, String]` (E006 from a pure context).
- In a plain `(A) -> B` slot without `effect`, fallible lambdas within the Cell 1 subset of #1108 2b-iii are routed to the generated `__fallible__` twin and **executed** (C-221, `fallible_user_hof_test.almd`). Outside that subset, E005 still applies.
- Deviations (such as missing ok-wrapping on only one target or differing err messages) are detected by C-221 fixtures.


## Reader note on source limitations

The source itself marks ALS-M4’s automatic propagation description as an old implementation and identifies the unresolved C-064 inconsistency. This translation preserves both that description and its limiting note; it does not restore implicit propagation. ALS-M11 applies to the specified Result propagation context, not every unwrap. ALS-M15’s Cell 1 subset, generated twin and out-of-subset E005 remain source-scoped claims, not an independently checked statement about every current HOF. The 0.59.1 measurement, v1 tail-recursion exception, unsigned-carrier history, and inference/opaque-alias exemptions are retained with their original bounds.
