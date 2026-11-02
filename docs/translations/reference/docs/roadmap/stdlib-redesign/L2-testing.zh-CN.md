> 原文描述：标准库重设计 L2/L3 — testing：保留十个断言；明确函数族（Option/Result 的两个方向、Int 严格比较、字符串包含）并设置门禁
<a id="l2l3-testing"></a>

# L2/L3：testing

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-testing.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-testing.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-testing.zh-CN.md)

> 非规范性的固定读者快照。以原文为准，不自动同步，未独立重新验证实现、测量或安全性。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-testing.md)
> 原文保留先前仅限 Int 的严格比较规则和 R-testing-01，同时 R-testing-02 与末表将定义域扩大至所有有序类型。保留两个历史层次，不暗中合并为单一当前 API 主张。保留中止断言的 stock host-capability／新实例边界。此处未独立重新验证实现或跨语言比较。


> 最后更新：2026-10-21
> 前提：[L0](../../../../../roadmap/stdlib-redesign/L0-conventions.md)、[L1](../../../../../roadmap/stdlib-redesign/L1-categories.md)（R-L1-29：不移动 testing）
> 来源：Rust https://doc.rust-lang.org/std/macro.assert.html · Go https://pkg.go.dev/testing · Python https://docs.python.org/3/library/unittest.html#assert-methods · TS/JS https://nodejs.org/api/assert.html · Gleam https://gleam-stdlib.hexdocs.pm/ （没有；gleeunit 是外部库）· Elixir https://hexdocs.pm/ex_unit/ExUnit.Assertions.html · OCaml https://ocaml.org/manual/latest/api/Stdlib.html （只有 `assert`）· Kotlin https://kotlinlang.org/api/core/kotlin-test/ · Swift https://developer.apple.com/documentation/xctest

<a id="l2-役割の比較"></a>

## L2：角色比较

| 角色 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 真值、相等 | 内置 `assert`、`assert_eq`、`assert_ne` | `assert!`、`assert_eq!` | `t.Errorf` | `assertTrue`、`assertEqual` | `assert.ok`、`equal` | — | `assert`、`refute` | `assert` | `assertTrue`、`assertEquals` | `XCTAssertEqual` |
| 接近（浮点数） | `assert_approx` | — | — | `assertAlmostEqual` | — | — | `assert_in_delta` | — | `assertEquals(…, tolerance)` | `XCTAssertEqual(accuracy:)` |
| 大小 | `assert_gt`、`assert_lt` | — | — | `assertGreater`、`assertLess`（＋`Equal`） | — | — | — | — | — | `XCTAssertGreaterThan`（＋`OrEqual`） |
| 包含 | `assert_contains`（String） | — | — | `assertIn` | `match` | — | `assert x =~` | — | `assertContains` | — |
| none / some | `assert_none`、`assert_some` | — | — | `assertIsNone`、`IsNotNone` | — | — | — | — | `assertNull`、`assertNotNull` | `XCTAssertNil`、`NotNil` |
| ok / err | `assert_ok`、`assert_err` | `is_ok`+`assert!` | — | — | `doesNotReject`、`rejects` | — | `{:ok, _} = ` | — | `assertFails` | `XCTAssertNoThrow`、`ThrowsError` |
| 确认中止 | `assert_aborts` | `#[should_panic]` | — | `assertRaises` | `throws` | — | `assert_raise` | — | `assertFails` | `XCTAssertThrowsError` |
| 快照 | `assert_snapshot` | — | — | — | Jest `toMatchSnapshot`（外部） | — | — | — | — | — |

<a id="完全性の規則"></a>

### 完整性规则

- 和类型家族：Option 与 Result 的两个方向（`some`/`none`、`ok`/`err`）。当前门禁 `tests/testing_assert_family_gate_test.rs` 负责维护。
- 大小家族：仅 `Int` 的严格比较 {`gt`, `lt`}。不提供带等号的（`ge`/`le`）或 `Float` 比较。既可写作 `assert(a >= b)`，且九种语言中只有 Python 和 Swift 两种有带等号的版本。`Float` 由 `assert_approx` 负责。
- 包含：仅 `String`。列表用 `assert(list.contains(xs, x))`。
- 名称：返回类型是 `Unit`，因此不属于 R-L0-09 的对象。

<a id="l2-の判定"></a>

### L2 判定

- 缺失角色（至少 5/9）：无（“真值、相等”为内置）。
- 重复／规则违反：无。
- 家族缺口：大小家族不含 `ge`/`le` 是有意的 → 在 R-testing-01 中作为家族规则写入门禁。

<a id="案"></a>

## 方案

<a id="r-testing-01-大小と含むの族の規則をゲートに書く---採用関数は変えない"></a>

### R-testing-01 将大小与包含家族的规则写入门禁 — ○ 采纳（不更改函数）

- 内容：在 `tests/testing_assert_family_gate_test.rs` 中补充并说明理由：大小家族为 {`assert_gt`, `assert_lt`}（`Int`、严格），`assert_contains` 仅限 `String`。不添加 `assert_ge`、`assert_le`（× 不采纳，见上述完整性规则）。
- 依据：CLAUDE.md“以矩阵处理家族”、R-L0-00（九种语言中没有多数）。
- (a) 只有 Python、Swift 有带等号的版本。
- (b) 全部是纯 Almide，在 native 与 wasm 上相同。按 ADR-0029，`assert_aborts` 在新实例中运行闭包（stock 上为 host-capability，台账中有对应行）。
- (c) 家族形状一旦确定，LLM 猜写 `assert_ge` 时，就能提示 `assert(a >= b)`（使用与 `removed_spellings.rs` 相同的机制，添加“不存在的拼写”提示）。
- (d) 无变更（仅门禁和诊断提示）。
- issue：#3949

<a id="r-testing-02-assert_gtassert_lt-を順序のある型すべてで使えるようにする---採用"></a>

### R-testing-02 让 `assert_gt`、`assert_lt` 可用于所有有序类型 — ○ 采纳

- 目前：`fn assert_gt(a: Int, b: Int) -> Unit`。所有有序类型都有 `compare`（R-L0-16），大小断言却仅限 `Int`。检查 `Float`、`Duration`、`String` 的大小会出现 E005。
- 方案：`fn testing.assert_gt[T](a: T, b: T) -> Unit`，`assert_lt` 同样处理。顺序与 `<` 相同。
- 依据的 L0 规则：R-L0-16。与扩展 `list.binary_search` 的 R-list-15 一样，是“仅扩大定义域”的改动。
- (a) Python `assertGreater`、Swift `XCTAssertGreaterThan` 接受所有可比较类型。
- (d) 语义变更（仅扩大定义域，既有调用不变）。R-testing-01 的家族（{`assert_gt`, `assert_lt`}，不提供 `ge`/`le`）保持不变。
- issue：#3980

<a id="l3-関数表"></a>

## L3：函数表

| 函数 | 签名（当前，显示 mut） | 其他语言的主要拼写 | 判定 | 方案 |
|---|---|---|---|---|
| `testing.assert_aborts` | `fn assert_aborts[T](expected: String, f: () -> T) -> Unit` | Python `assertRaises` / Elixir `assert_raise` / Kotlin `assertFails` | 保持 | — |
| `testing.assert_contains` | `fn assert_contains(haystack: String, needle: String) -> Unit` | Python `assertIn` / Kotlin `assertContains` | 保持 | R-testing-01 |
| `testing.assert_approx` | `fn assert_approx(a: Float, b: Float, tolerance: Float) -> Unit` | Python `assertAlmostEqual` / Elixir `assert_in_delta` / Swift `accuracy:` | 保持 | — |
| `testing.assert_gt` | `fn assert_gt[T](a: T, b: T) -> Unit` | Python `assertGreater` / Swift `XCTAssertGreaterThan` | 语义变更（扩大至全部有序类型） | R-testing-02 |
| `testing.assert_lt` | `fn assert_lt[T](a: T, b: T) -> Unit` | Python `assertLess` / Swift `XCTAssertLessThan` | 语义变更（扩大至全部有序类型） | R-testing-02 |
| `testing.assert_some` | `fn assert_some[A](opt: A?) -> Unit` | Python `assertIsNotNone` / Kotlin `assertNotNull` | 保持 | — |
| `testing.assert_none` | `fn assert_none[A](opt: A?) -> Unit` | Python `assertIsNone` / Kotlin `assertNull` / Swift `XCTAssertNil` | 保持 | — |
| `testing.assert_ok` | `fn assert_ok[A, B](result: Result[A, B]) -> Unit` | Swift `XCTAssertNoThrow` | 保持 | — |
| `testing.assert_err` | `fn assert_err[A, B](result: Result[A, B]) -> Unit` | Kotlin `assertFails` / Swift `XCTAssertThrowsError` | 保持 | — |
| `testing.assert_snapshot` | `fn assert_snapshot(actual: String, expected: String) -> Unit` | Jest `toMatchSnapshot`（外部） | 保持 | — |
