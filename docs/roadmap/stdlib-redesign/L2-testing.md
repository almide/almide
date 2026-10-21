<!-- description: Stdlib redesign L2/L3 — testing: the ten assertions stay; the families (both polarities of Option/Result, strict Int comparison, string containment) are stated and gated -->
# L2/L3: testing

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)（R-L1-29: testing は動かさない）
> 出典: Rust https://doc.rust-lang.org/std/macro.assert.html ・ Go https://pkg.go.dev/testing ・ Python https://docs.python.org/3/library/unittest.html#assert-methods ・ TS/JS https://nodejs.org/api/assert.html ・ Gleam https://gleam-stdlib.hexdocs.pm/ （無い。gleeunit は外部）・ Elixir https://hexdocs.pm/ex_unit/ExUnit.Assertions.html ・ OCaml https://ocaml.org/manual/latest/api/Stdlib.html （`assert` だけ）・ Kotlin https://kotlinlang.org/api/core/kotlin-test/ ・ Swift https://developer.apple.com/documentation/xctest

## L2: 役割の比較

| 役割 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 真偽・等しい | 組み込み `assert`・`assert_eq`・`assert_ne` | `assert!`・`assert_eq!` | `t.Errorf` | `assertTrue`・`assertEqual` | `assert.ok`・`equal` | — | `assert`・`refute` | `assert` | `assertTrue`・`assertEquals` | `XCTAssertEqual` |
| 近い（浮動小数点） | `assert_approx` | — | — | `assertAlmostEqual` | — | — | `assert_in_delta` | — | `assertEquals(…, tolerance)` | `XCTAssertEqual(accuracy:)` |
| 大小 | `assert_gt`・`assert_lt` | — | — | `assertGreater`・`assertLess`（＋`Equal`） | — | — | — | — | — | `XCTAssertGreaterThan`（＋`OrEqual`） |
| 含む | `assert_contains`（String） | — | — | `assertIn` | `match` | — | `assert x =~` | — | `assertContains` | — |
| none / some | `assert_none`・`assert_some` | — | — | `assertIsNone`・`IsNotNone` | — | — | — | — | `assertNull`・`assertNotNull` | `XCTAssertNil`・`NotNil` |
| ok / err | `assert_ok`・`assert_err` | `is_ok`+`assert!` | — | — | `doesNotReject`・`rejects` | — | `{:ok, _} = ` | — | `assertFails` | `XCTAssertNoThrow`・`ThrowsError` |
| 止まることを確かめる | `assert_aborts` | `#[should_panic]` | — | `assertRaises` | `throws` | — | `assert_raise` | — | `assertFails` | `XCTAssertThrowsError` |
| スナップショット | `assert_snapshot` | — | — | — | Jest `toMatchSnapshot`（外部） | — | — | — | — | — |

### 完全性の規則

- 和型の族: Option と Result の両方の向き（`some`/`none`、`ok`/`err`）。いまのゲート `tests/testing_assert_family_gate_test.rs` が守っています。
- 大小の族: `Int` の厳密な比較 {`gt`, `lt`} だけ。等号つき（`ge`/`le`）と `Float` の比較は置きません。`assert(a >= b)` で書けるうえ、等号つきを持つのは 9 言語のうち Python と Swift の 2 つだけです。`Float` は `assert_approx` が持ちます。
- 含む: `String` だけ。リストは `assert(list.contains(xs, x))`。
- 名前: 戻り型は `Unit` なので R-L0-09 の対象外です。

### L2 の判定

- 抜けている役割（5/9 以上）: なし（「真偽・等しい」は組み込み）
- 重複・規則違反: なし
- 族の穴: 大小の族に `ge`/`le` が無いのは意図したもの → R-testing-01 で族の規則としてゲートに書く

## 案

### R-testing-01 大小と含むの族の規則をゲートに書く — ○ 採用（関数は変えない）

- 内容: `tests/testing_assert_family_gate_test.rs` に、大小の族が {`assert_gt`, `assert_lt`}（`Int`、厳密）であること、`assert_contains` が `String` だけであることを、理由つきで書き足します。`assert_ge`・`assert_le` は足しません（× 不採用、上の完全性の規則）。
- 根拠: CLAUDE.md「族は行列で」、R-L0-00（9 言語に多数派なし）。
- (a) 等号つきを持つのは Python・Swift だけです。
- (b) どれも純粋な Almide で、native と wasm で同じです。`assert_aborts` は ADR-0029 のとおり、新しいインスタンスで閉包を走らせます（stock では host-capability。台帳の行あり）。
- (c) 族の形が決まっていれば、LLM が `assert_ge` を推測で書いたときに、ヒントで `assert(a >= b)` を示せます（`removed_spellings.rs` と同じ仕組みで「無い綴り」のヒントを足す）。
- (d) 変更なし（ゲートと診断のヒントだけ）。
- issue: #3949

## L3: 関数表

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `testing.assert_aborts` | `fn assert_aborts[T](expected: String, f: () -> T) -> Unit` | Python `assertRaises` / Elixir `assert_raise` / Kotlin `assertFails` | 据え置き | — |
| `testing.assert_contains` | `fn assert_contains(haystack: String, needle: String) -> Unit` | Python `assertIn` / Kotlin `assertContains` | 据え置き | R-testing-01 |
| `testing.assert_approx` | `fn assert_approx(a: Float, b: Float, tolerance: Float) -> Unit` | Python `assertAlmostEqual` / Elixir `assert_in_delta` / Swift `accuracy:` | 据え置き | — |
| `testing.assert_gt` | `fn assert_gt(a: Int, b: Int) -> Unit` | Python `assertGreater` / Swift `XCTAssertGreaterThan` | 据え置き | R-testing-01 |
| `testing.assert_lt` | `fn assert_lt(a: Int, b: Int) -> Unit` | Python `assertLess` / Swift `XCTAssertLessThan` | 据え置き | R-testing-01 |
| `testing.assert_some` | `fn assert_some[A](opt: A?) -> Unit` | Python `assertIsNotNone` / Kotlin `assertNotNull` | 据え置き | — |
| `testing.assert_none` | `fn assert_none[A](opt: A?) -> Unit` | Python `assertIsNone` / Kotlin `assertNull` / Swift `XCTAssertNil` | 据え置き | — |
| `testing.assert_ok` | `fn assert_ok[A, B](result: Result[A, B]) -> Unit` | Swift `XCTAssertNoThrow` | 据え置き | — |
| `testing.assert_err` | `fn assert_err[A, B](result: Result[A, B]) -> Unit` | Kotlin `assertFails` / Swift `XCTAssertThrowsError` | 据え置き | — |
| `testing.assert_snapshot` | `fn assert_snapshot(actual: String, expected: String) -> Unit` | Jest `toMatchSnapshot`（外部） | 据え置き | — |
