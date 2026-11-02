> Source description: Stdlib redesign L2/L3 — testing: the ten assertions stay; the families (both polarities of Option/Result, strict Int comparison, string containment) are stated and gated
<a id="l2l3-testing"></a>

# L2/L3: testing

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-testing.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-testing.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-testing.zh-CN.md)

> Non-normative pinned reader snapshot. The original remains authoritative; this copy is not automatically synchronized and has not independently revalidated implementations, measurements, or safety.
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-testing.md)
> The original keeps its earlier Int-only strict-comparison rule and R-testing-01 while R-testing-02 and the final table widen the domain to all ordered types. Both historical layers are retained, not silently reconciled as a single current API claim. The abort assertion retains its stock host-capability/fresh-instance boundary. No implementation or cross-language comparison is independently revalidated here.


> Last updated: 2026-10-21
> Prerequisites: [L0](../../../../../roadmap/stdlib-redesign/L0-conventions.md), [L1](../../../../../roadmap/stdlib-redesign/L1-categories.md) (R-L1-29: do not move testing)
> Sources: Rust https://doc.rust-lang.org/std/macro.assert.html · Go https://pkg.go.dev/testing · Python https://docs.python.org/3/library/unittest.html#assert-methods · TS/JS https://nodejs.org/api/assert.html · Gleam https://gleam-stdlib.hexdocs.pm/ (absent; gleeunit is external) · Elixir https://hexdocs.pm/ex_unit/ExUnit.Assertions.html · OCaml https://ocaml.org/manual/latest/api/Stdlib.html (only `assert`) · Kotlin https://kotlinlang.org/api/core/kotlin-test/ · Swift https://developer.apple.com/documentation/xctest

<a id="l2-役割の比較"></a>

## L2: Comparing roles

| Role | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| Boolean truth and equality | Built-in `assert`, `assert_eq`, `assert_ne` | `assert!`, `assert_eq!` | `t.Errorf` | `assertTrue`, `assertEqual` | `assert.ok`, `equal` | — | `assert`, `refute` | `assert` | `assertTrue`, `assertEquals` | `XCTAssertEqual` |
| Approximate equality (floating point) | `assert_approx` | — | — | `assertAlmostEqual` | — | — | `assert_in_delta` | — | `assertEquals(…, tolerance)` | `XCTAssertEqual(accuracy:)` |
| Ordering | `assert_gt`, `assert_lt` | — | — | `assertGreater`, `assertLess` (plus `Equal`) | — | — | — | — | — | `XCTAssertGreaterThan` (plus `OrEqual`) |
| Containment | `assert_contains` (String) | — | — | `assertIn` | `match` | — | `assert x =~` | — | `assertContains` | — |
| none / some | `assert_none`, `assert_some` | — | — | `assertIsNone`, `IsNotNone` | — | — | — | — | `assertNull`, `assertNotNull` | `XCTAssertNil`, `NotNil` |
| ok / err | `assert_ok`, `assert_err` | `is_ok`+`assert!` | — | — | `doesNotReject`, `rejects` | — | `{:ok, _} = ` | — | `assertFails` | `XCTAssertNoThrow`, `ThrowsError` |
| Check that execution aborts | `assert_aborts` | `#[should_panic]` | — | `assertRaises` | `throws` | — | `assert_raise` | — | `assertFails` | `XCTAssertThrowsError` |
| Snapshot | `assert_snapshot` | — | — | — | Jest `toMatchSnapshot` (external) | — | — | — | — | — |

<a id="完全性の規則"></a>

### Completeness rules

- Sum-type families: both polarities of Option and Result (`some`/`none`, `ok`/`err`). The current gate `tests/testing_assert_family_gate_test.rs` protects this.
- Ordering family: only strict `Int` comparisons {`gt`, `lt`}. No equality-inclusive (`ge`/`le`) or `Float` comparisons. They can be written as `assert(a >= b)`, and only Python and Swift, two of nine languages, have equality-inclusive variants. `Float` is covered by `assert_approx`.
- Containment: `String` only. For lists, use `assert(list.contains(xs, x))`.
- Names: the return type is `Unit`, so R-L0-09 does not apply.

<a id="l2-の判定"></a>

### L2 verdict

- Missing roles (at least 5/9): none (“boolean truth and equality” are built in).
- Duplicates / rule violations: none.
- Family gaps: the absence of `ge`/`le` in the ordering family is intentional → encode it as a family rule in the gate under R-testing-01.

<a id="案"></a>

## Proposals

<a id="r-testing-01-大小と含むの族の規則をゲートに書く---採用関数は変えない"></a>

### R-testing-01 Encode ordering and containment family rules in the gate — ○ Accepted (no function changes)

- Content: add to `tests/testing_assert_family_gate_test.rs`, with reasons, that the ordering family is {`assert_gt`, `assert_lt`} (`Int`, strict) and `assert_contains` is `String` only. Do not add `assert_ge` or `assert_le` (× Rejected; completeness rules above).
- Rationale: CLAUDE.md “families as matrices,” R-L0-00 (no majority across nine languages).
- (a) Only Python and Swift have equality-inclusive variants.
- (b) All are pure Almide and the same on native and wasm. As ADR-0029 specifies, `assert_aborts` runs a closure in a fresh instance (a host-capability on stock; recorded in the ledger).
- (c) Once the family shape is fixed, if an LLM guesses `assert_ge`, a hint can show `assert(a >= b)` (add hints for “nonexistent spellings” using the same mechanism as `removed_spellings.rs`).
- (d) No change (only the gate and diagnostic hints).
- issue: #3949

<a id="r-testing-02-assert_gtassert_lt-を順序のある型すべてで使えるようにする---採用"></a>

### R-testing-02 Make `assert_gt` and `assert_lt` usable for every ordered type — ○ Accepted

- Now: `fn assert_gt(a: Int, b: Int) -> Unit`. All ordered types have `compare` (R-L0-16), yet ordering assertions accept only `Int`. Checking the ordering of `Float`, `Duration`, or `String` results in E005.
- Proposal: `fn testing.assert_gt[T](a: T, b: T) -> Unit`, and likewise for `assert_lt`. Ordering is the same as `<`.
- Supporting L0 rule: R-L0-16. Like R-list-15's widening of `list.binary_search`, this “only widens the domain.”
- (a) Python's `assertGreater` and Swift's `XCTAssertGreaterThan` accept all comparable types.
- (d) Semantic change (only widens the domain; existing calls do not change). The R-testing-01 family ({`assert_gt`, `assert_lt`}, no `ge`/`le`) stays the same.
- issue: #3980

<a id="l3-関数表"></a>

## L3: Function table

| Function | Signature (current, mut shown) | Main spellings in other languages | Verdict | Proposal |
|---|---|---|---|---|
| `testing.assert_aborts` | `fn assert_aborts[T](expected: String, f: () -> T) -> Unit` | Python `assertRaises` / Elixir `assert_raise` / Kotlin `assertFails` | Keep | — |
| `testing.assert_contains` | `fn assert_contains(haystack: String, needle: String) -> Unit` | Python `assertIn` / Kotlin `assertContains` | Keep | R-testing-01 |
| `testing.assert_approx` | `fn assert_approx(a: Float, b: Float, tolerance: Float) -> Unit` | Python `assertAlmostEqual` / Elixir `assert_in_delta` / Swift `accuracy:` | Keep | — |
| `testing.assert_gt` | `fn assert_gt[T](a: T, b: T) -> Unit` | Python `assertGreater` / Swift `XCTAssertGreaterThan` | Semantic change (widen to all ordered types) | R-testing-02 |
| `testing.assert_lt` | `fn assert_lt[T](a: T, b: T) -> Unit` | Python `assertLess` / Swift `XCTAssertLessThan` | Semantic change (widen to all ordered types) | R-testing-02 |
| `testing.assert_some` | `fn assert_some[A](opt: A?) -> Unit` | Python `assertIsNotNone` / Kotlin `assertNotNull` | Keep | — |
| `testing.assert_none` | `fn assert_none[A](opt: A?) -> Unit` | Python `assertIsNone` / Kotlin `assertNull` / Swift `XCTAssertNil` | Keep | — |
| `testing.assert_ok` | `fn assert_ok[A, B](result: Result[A, B]) -> Unit` | Swift `XCTAssertNoThrow` | Keep | — |
| `testing.assert_err` | `fn assert_err[A, B](result: Result[A, B]) -> Unit` | Kotlin `assertFails` / Swift `XCTAssertThrowsError` | Keep | — |
| `testing.assert_snapshot` | `fn assert_snapshot(actual: String, expected: String) -> Unit` | Jest `toMatchSnapshot` (external) | Keep | — |
