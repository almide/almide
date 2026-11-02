> **削除済み（ADR-0007）**：`result.collect` / `collect_map` はなくなりました。呼び出すと通常の [E002](../../../../diagnostics/E002.md) になります。全エラーの収集には `result.partition` を使います。

# result

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](result.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](result.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](result.zh-CN.md)



> 原文：[result.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/result.md)。読者向けの非規範的な固定スナップショットです。API 索引は原文固定時点の内容であり、この訳は自動生成・自動同期の対象ではありません。署名・型・`@since` とコード例（コメントを含む）は原文のまま保持し、API 説明の訳を末尾に併記しています。コード例はこの翻訳作業では独立実行していません。

> 削除済み・非推奨化・削除予定は原文固定時点の記述です。collect 系の削除と unwrap_or の削除予定を区別し、即時評価と遅延評価の違いを保持しています。

Result 型の操作。自動インポートされます。

### `result.map(r: Result[A, E], f: (A) -> B) -> Result[B, E]`

関数を使って ok の値を変換します。err の場合はそのまま通します。

```almd run
fn show(r: Result[Int, String]) -> String = match r {
  ok(n) => "ok(${n})",
  err(e) => "err(\"${e}\")",
}

fn main() -> Unit = {
  println(show(result.map(ok(2), (x) => x * 10)))
  println(show(result.map(err("fail"), (x) => x * 10)))
}
```
```output
ok(20)
err("fail")
```

### `result.map_err(r: Result[A, E], f: (E) -> F) -> Result[A, F]`

関数を使って err の値を変換します。ok の場合はそのまま通します。

```almd run
fn show(r: Result[Int, String]) -> String = match r {
  ok(n) => "ok(${n})",
  err(e) => "err(\"${e}\")",
}

fn main() -> Unit = {
  println(show(result.map_err(err("fail"), (e) => "wrapped: " + e)))
  println(show(result.map_err(ok(1): Result[Int, String], (e) => "wrapped: " + e)))
}
```
```output
err("wrapped: fail")
ok(1)
```

### `result.flat_map(r: Result[A, E], f: (A) -> Result[B, E]) -> Result[B, E]`

ok の値に Result を返す関数を連結します。入れ子の Result を平坦化します。

```almd run
fn show(r: Result[Int, String]) -> String = match r {
  ok(n) => "ok(${n})",
  err(e) => "err(\"${e}\")",
}

fn main() -> Unit = {
  println(show(result.flat_map(ok(5), (x) => if x > 0 then ok(x) else err("negative"))))
  println(show(result.flat_map(ok(-5), (x) => if x > 0 then ok(x) else err("negative"))))
}
```
```output
ok(5)
err("negative")
```

### `result.unwrap_or(r: Result[A, E], default: A) -> A`

**非推奨（E052、ADR-0005 D4）** — 即時評価するフォールバックは、`??`（`≡ unwrap_or_else`、既定値はフォールバック経路でのみ評価）との定義上の関係を持たない3番目の表記です。`value ?? default` と書いてください。`almide fix` は呼び出し形式とパイプ段の形式の両方を書き換えます。1つ後の方言移行期間で削除されます。


ok の値を取得し、err の場合は既定値を返します。

```almd run
fn main() -> Unit = {
  let r: Result[Int, String] = err("fail")
  println("${r ?? 0}")
  println("${(ok(7): Result[Int, String]) ?? 0}")
}
```
```output
0
7
```

### `result.unwrap_or_else(r: Result[A, E], f: (E) -> A) -> A`

ok の値を取得するか、関数を使ってエラーから既定値を計算します。

```almd run
fn main() -> Unit = {
  println("${result.unwrap_or_else(err("fail"), (e) => string.len(e))}")
}
```
```output
4
```

### `result.is_ok(r: Result[A, E]) -> Bool`

Result が ok か調べます。

```almd run
fn main() -> Unit = {
  println("${result.is_ok(ok(42): Result[Int, String])}")
}
```
```output
true
```

### `result.is_err(r: Result[A, E]) -> Bool`

Result が err か調べます。

```almd run
fn main() -> Unit = {
  println("${result.is_err(err("fail"): Result[Int, String])}")
}
```
```output
true
```

### `result.to_option(r: Result[A, E]) -> Option[A]`

ok を some に、err を none に変換します。エラー値は破棄します。

```almd run
fn show(o: Option[Int]) -> String = match o {
  some(n) => "some(${n})",
  none => "none",
}

fn main() -> Unit = {
  println(show(result.to_option(ok(42): Result[Int, String])))
  println(show(result.to_option(err("fail"): Result[Int, String])))
}
```
```output
some(42)
none
```

### `result.to_err_option(r: Result[A, E]) -> Option[E]`

err を some に、ok を none に変換します。ok の値は破棄します。

```almd run
fn show(o: Option[String]) -> String = match o {
  some(e) => "some(\"${e}\")",
  none => "none",
}

fn main() -> Unit = {
  println(show(result.to_err_option(err("fail"): Result[Int, String])))
  println(show(result.to_err_option(ok(42): Result[Int, String])))
}
```
```output
some("fail")
none
```

### `result.partition(rs: List[Result[T, E]]) -> (List[T], List[E])`

Result のリストを ok の値と err の値に分割します。これは全エラーを収集する基本操作で、削除済みの `result.collect` / `result.collect_map` はこれを使って表します：

```almd run
fn main() -> Unit = {
  println("${result.partition([ok(1), err("x"), ok(2)])}")
}

// result.collect(rs)          → let (oks, errs) = result.partition(rs)
// result.collect_map(xs, f)   → let (oks, errs) = result.partition(list.map(xs, f))
```
```output
([1, 2], ["x"])
```


<a id="signature-index-15-functions"></a>

## 原文固定スナップショットの API 索引（15関数）

```
// ok(f(v)) for ok(v); err passes through.
// @since 0.5.13 or earlier
result.map(r: Result[A, E], f: (A) -> B) -> Result[B, E]

// err(f(e)) for err(e); ok passes through.
// @since 0.5.13 or earlier
result.map_err(r: Result[A, E], f: (E) -> F) -> Result[A, F]

// f(v) for ok(v); err passes through.
// @since 0.6.1 or earlier
result.flat_map(r: Result[A, E], f: (A) -> Result[B, E]) -> Result[B, E]

// v for ok(v), else default (always evaluated).
// @since 0.5.13 or earlier
result.unwrap_or(r: Result[A, E], default: A) -> A   (deprecated — use ??)

// v for ok(v), else f(e); f runs only on err.
// @since 0.5.13 or earlier
result.unwrap_or_else(r: Result[A, E], f: (E) -> A) -> A

// true for ok(_), false for err(_).
// @since 0.6.0 or earlier
result.is_ok(r: Result[A, E]) -> Bool

// true for err(_), false for ok(_).
// @since 0.6.0 or earlier
result.is_err(r: Result[A, E]) -> Bool

// some(v) for ok(v); none for err, e dropped.
// @since 0.5.13 or earlier
result.to_option(r: Result[A, E]) -> Option[A]

// some(e) for err(e); none for ok.
// @since 0.5.13 or earlier
result.to_err_option(r: Result[A, E]) -> Option[E]

// (ok values, err values), each in list order.
// @since 0.9.1 or earlier
result.partition(xs: List[Result[A, E]]) -> (List[A], List[E])

// Inner result of ok(inner); outer err kept.
// @since 0.15.0 or earlier
result.flatten(r: Result[Result[A, E], E]) -> Result[A, E]

// [v] for ok(v), [] for err.
// @since 0.15.0 or earlier
result.to_list(r: Result[A, E]) -> List[A]

// ok((x, y)) if both ok, else the first err.
// @since 0.15.0 or earlier
result.zip(a: Result[A, E], b: Result[B, E]) -> Result[(A, B), E]

// r when ok, else f(e); f runs only on err.
// @since 0.15.0 or earlier
result.or_else(r: Result[A, E], f: (E) -> Result[A, F]) -> Result[A, F]

// err(err_val) if f fails on ok; err kept.
// @since 0.15.0 or earlier
result.filter(r: Result[A, E], f: (A) -> Bool, err_val: E) -> Result[A, E]
```


## API 説明の訳（上記署名順）

- `result.map(r: Result[A, E], f: (A) -> B) -> Result[B, E]`：ok(v) には ok(f(v))、err はそのまま通します。
- `result.map_err(r: Result[A, E], f: (E) -> F) -> Result[A, F]`：err(e) には err(f(e))、ok はそのまま通します。
- `result.flat_map(r: Result[A, E], f: (A) -> Result[B, E]) -> Result[B, E]`：ok(v) には f(v)、err はそのまま通します。
- `result.unwrap_or(r: Result[A, E], default: A) -> A   (deprecated — use ??)`：ok(v) には v、それ以外は default（常に評価）。
- `result.unwrap_or_else(r: Result[A, E], f: (E) -> A) -> A`：ok(v) には v、それ以外は f(e)。f は err の場合だけ実行します。
- `result.is_ok(r: Result[A, E]) -> Bool`：ok(_) には true、err(_) には false。
- `result.is_err(r: Result[A, E]) -> Bool`：err(_) には true、ok(_) には false。
- `result.to_option(r: Result[A, E]) -> Option[A]`：ok(v) には some(v)、err には none。e は破棄します。
- `result.to_err_option(r: Result[A, E]) -> Option[E]`：err(e) には some(e)、ok には none。
- `result.partition(xs: List[Result[A, E]]) -> (List[A], List[E])`：(ok の値、err の値)。各リストは元のリスト順です。
- `result.flatten(r: Result[Result[A, E], E]) -> Result[A, E]`：ok(inner) の内側の result。外側の err は保持します。
- `result.to_list(r: Result[A, E]) -> List[A]`：ok(v) には [v]、err には []。
- `result.zip(a: Result[A, E], b: Result[B, E]) -> Result[(A, B), E]`：両方が ok なら ok((x, y))、それ以外は最初の err。
- `result.or_else(r: Result[A, E], f: (E) -> Result[A, F]) -> Result[A, F]`：ok なら r、それ以外は f(e)。f は err の場合だけ実行します。
- `result.filter(r: Result[A, E], f: (A) -> Bool, err_val: E) -> Result[A, E]`：ok に対して f が成り立たなければ err(err_val)。err は保持します。
