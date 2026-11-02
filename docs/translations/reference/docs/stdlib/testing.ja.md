# testing

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](testing.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](testing.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](testing.zh-CN.md)



> [固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/testing.md) の非規範的な読者向け訳です。自動生成・自動同期されません。署名・コードとそのコメントは原文通りで、API 説明の訳を外側に併記します。この翻訳では例や実装を独立に実行検証していません。


テスト用のアサーション。import testing。

### `testing.assert_aborts[T](expected: String, f: () -> T) -> Unit`

`f` が `expected` を含むテキストで中止することを検査します。プログラムを新しく開始する子プロセス（native）または新規インスタンス（wasm）で `f` だけを実行するため、構造化された Almide の中止 — `panic`、範囲外の添字、ゼロ除算、アサーション失敗 — が対象となり、呼び出し後もテストは続きます。`f` は呼び出し箇所に直接書く lambda で、テスト側の値を何も捕捉してはなりません（E098）。呼び出しは `test` ブロック直下に置く必要があります（E099）。子の出力はアサーション失敗時だけ表示します。生のホスト panic、構造化されていない Wasm trap、中止ではない終了は、Almide の中止として認めません。

```almd check
import testing

fn nth(xs: List[Int], i: Int) -> Int = xs[i]

test "reading past the end aborts" {
  testing.assert_aborts("index out of bounds", () => nth([1, 2, 3], 5))
  testing.assert_aborts("PANIC: oh no", () => panic("oh no"))
  assert_eq(nth([1, 2, 3], 0), 1)
}
```

### `testing.assert_contains(haystack: String, needle: String) -> Unit`

文字列が部分文字列を含むことを検査します。

```almd check
import testing

test "substring present" {
  testing.assert_contains("hello world", "world")
}
```

### `testing.assert_approx(a: Float, b: Float, tolerance: Float) -> Unit`

二つの浮動小数点数が許容誤差の範囲でほぼ等しいことを検査します。

```almd check
import testing

test "close enough" {
  testing.assert_approx(3.14, 3.14159, 0.01)
}
```

### `testing.assert_gt(a: A, b: A) -> Unit`

a が b より大きいことを検査します。

```almd check
import testing

test "greater" {
  testing.assert_gt(10, 5)
}
```

### `testing.assert_lt(a: A, b: A) -> Unit`

a が b より小さいことを検査します。

```almd check
import testing

test "less" {
  testing.assert_lt(3, 7)
}
```

### `testing.assert_some(opt: Option[A]) -> Unit`

Option が some（none ではない）であることを検査します。

```almd check
import testing

test "option is some" {
  testing.assert_some(some("value"))
}
```

### `testing.assert_ok(result: Result[A, E]) -> Unit`

Result が ok（err ではない）であることを検査します。

```almd check
import testing

test "result is ok" {
  let result: Result[String, String] = ok("success")
  testing.assert_ok(result)
}
```

### `testing.assert_snapshot(actual: String, expected: String) -> Unit`

`actual` が呼び出し箇所に記述したスナップショットと等しいことを検査します。期待値はリテラル自体であり、別添ファイルはありません。受け入れ操作はその場でリテラルを書き換えます。`""` から始めて `almide test --update-snapshots <file>` を実行すると、得られた値を第二引数へ書き戻します（複数行なら heredoc）。その後の変化は通常の `almide test` で差分と受け入れ用のヒントを表示して失敗し、`--update-snapshots` を使うと再び書き換えます。CI モード（`--ci` または `CI=true`）では一切書き込みません。新規または変化したスナップショットは CI では失敗するため、コードと同様にコミットしてレビューします。

```almd check
import testing

fn render(xs: List[Int]) -> String = xs |> list.map((x) => "item ${x}") |> list.join("\n")

test "single line" {
  testing.assert_snapshot("hello", "hello")
}

test "multi line, written back as a heredoc" {
  testing.assert_snapshot(render([1, 2]), """
    item 1
    item 2
    """)
}
```

受け入れ操作で書き換えられるよう、第二引数は通常の文字列リテラル（補間なし）でなければなりません。不一致の場合は、すべてのターゲットで構造化ブロック `Error: snapshot mismatch` / `at:` / `expected:` / `found:` と終了コード 1 により実行を中止します（契約 C-336）。



## シグネチャ索引 (10 個の関数)

```
// Fails unless f, run alone in a child, aborts with text containing expected.
// @since 0.68.0
testing.assert_aborts(expected: String, f: () -> A) -> Unit

// Fails unless needle is in haystack; "" passes.
// @since 0.6.0 or earlier
testing.assert_contains(haystack: String, needle: String) -> Unit

// Fails unless abs(a - b) < tolerance (strict).
// @since 0.6.0 or earlier
testing.assert_approx(a: Float, b: Float, tolerance: Float) -> Unit

// Fails unless a > b; equal fails.
// @since 0.6.0 or earlier
testing.assert_gt(a: A, b: A) -> Unit

// Fails unless a < b; equal fails.
// @since 0.6.0 or earlier
testing.assert_lt(a: A, b: A) -> Unit

// Fails when o is none.
// @since 0.6.0 or earlier
testing.assert_some(o: Option[A]) -> Unit

// Fails when r is an err.
// @since 0.6.0 or earlier
testing.assert_ok(r: Result[A, B]) -> Unit

// Fails when o is some.
// @since 0.54.0
testing.assert_none(o: Option[A]) -> Unit

// Fails when r is ok.
// @since 0.54.0
testing.assert_err(r: Result[A, B]) -> Unit

// Fails unless actual == expected; --update-snapshots rewrites.
// @since 0.62.0
testing.assert_snapshot(actual: String, expected: String) -> Unit
```



## API 説明の参考訳

- `testing.assert_aborts(expected: String, f: () -> A) -> Unit` — 子で単独実行した f が expected を含むテキストで中止しなければ失敗。
- `testing.assert_contains(haystack: String, needle: String) -> Unit` — haystack に needle が含まれなければ失敗。空文字列 "" は合格。
- `testing.assert_approx(a: Float, b: Float, tolerance: Float) -> Unit` — abs(a - b) < tolerance（厳密な不等号）でなければ失敗。
- `testing.assert_gt(a: A, b: A) -> Unit` — a > b でなければ失敗。等しい場合も失敗。
- `testing.assert_lt(a: A, b: A) -> Unit` — a < b でなければ失敗。等しい場合も失敗。
- `testing.assert_some(o: Option[A]) -> Unit` — o が none の場合に失敗。
- `testing.assert_ok(r: Result[A, B]) -> Unit` — r が err の場合に失敗。
- `testing.assert_none(o: Option[A]) -> Unit` — o が some の場合に失敗。
- `testing.assert_err(r: Result[A, B]) -> Unit` — r が ok の場合に失敗。
- `testing.assert_snapshot(actual: String, expected: String) -> Unit` — actual == expected でなければ失敗。--update-snapshots で書き換え。
