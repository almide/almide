# option

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](option.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](option.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](option.zh-CN.md)



> 原文：[option.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/option.md)。読者向けの非規範的な固定スナップショットです。API 索引は原文固定時点の内容であり、この訳は自動生成・自動同期の対象ではありません。署名・型・`@since` とコード例（コメントを含む）は原文のまま保持し、API 説明の訳を末尾に併記しています。コード例はこの翻訳作業では独立実行していません。

> 非推奨化と削除予定は原文固定時点の記述です。即時評価の unwrap_or と、フォールバック時のみ評価する ?? / unwrap_or_else の評価条件の違いを保持しています。

Option 型の操作。自動インポートされます。

### `option.map(o: Option[A], f: (A) -> B) -> Option[B]`

関数を使って内部の値を変換します。none の場合は none を返します。

```almd run
fn show(o: Option[Int]) -> String = match o {
  some(n) => "some(${n})",
  none => "none",
}

fn main() -> Unit = {
  println(show(option.map(some(2), (x) => x * 10)))
}
```
```output
some(20)
```

### `option.flat_map(o: Option[A], f: (A) -> Option[B]) -> Option[B]`

内部の値に Option を返す関数を連結します。入れ子の Option を平坦化します。

```almd run
fn show(o: Option[Int]) -> String = match o {
  some(n) => "some(${n})",
  none => "none",
}

fn main() -> Unit = {
  println(show(option.flat_map(some(5), (x) => if x > 0 then some(x) else none)))
  println(show(option.flat_map(some(-5), (x) => if x > 0 then some(x) else none)))
}
```
```output
some(5)
none
```

### `option.flatten(o: Option[Option[A]]) -> Option[A]`

入れ子の Option を平坦化します。some(some(x)) は some(x) に、some(none) は none になります。

```almd run
fn show(o: Option[Int]) -> String = match o {
  some(n) => "some(${n})",
  none => "none",
}

fn main() -> Unit = {
  println(show(option.flatten(some(some(42)))))
  println(show(option.flatten(some(none))))
}
```
```output
some(42)
none
```

### `option.unwrap_or(o: Option[A], default: A) -> A`

内部の値を取得し、none の場合は既定値を返します。

**非推奨（E052、ADR-0005 D4）** — 即時評価するフォールバックは、`??`（`≡ unwrap_or_else`、既定値はフォールバック経路でのみ評価）との定義上の関係を持たない3番目の表記です。`value ?? default` と書いてください。`almide fix` は呼び出し形式とパイプ段の形式の両方を書き換えます。1つ後の方言移行期間で削除されます。


```almd run
fn main() -> Unit = {
  let n: Int? = none
  println("${n ?? 0}")
  println("${some(7) ?? 0}")
}
```
```output
0
7
```

### `option.unwrap_or_else(o: Option[A], f: () -> A) -> A`

内部の値を取得するか、関数で既定値を計算します。

```almd run
fn main() -> Unit = {
  println("${option.unwrap_or_else(none, () => 42)}")
}
```
```output
42
```

### `option.is_some(o: Option[A]) -> Bool`

Option に値が含まれるか調べます。

```almd run
fn main() -> Unit = {
  println("${option.is_some(some(42))}")
}
```
```output
true
```

### `option.is_none(o: Option[A]) -> Bool`

Option が none か調べます。

```almd run
fn main() -> Unit = {
  println("${option.is_none(none: Option[Int])}")
}
```
```output
true
```

### `option.to_result(o: Option[A], err: E) -> Result[A, E]`

some を ok に、none を指定した E 型のエラー値を持つ err に変換します。

```almd run
fn show(r: Result[Int, String]) -> String = match r {
  ok(n) => "ok(${n})",
  err(e) => "err(\"${e}\")",
}

fn main() -> Unit = {
  println(show(option.to_result(some(42), "missing")))
  println(show(option.to_result(none, "missing")))
}
```
```output
ok(42)
err("missing")
```

### `option.filter(o: Option[A], f: (A) -> Bool) -> Option[A]`

述語を満たせば値を保持し、そうでなければ none を返します。

```almd run
fn show(o: Option[Int]) -> String = match o {
  some(n) => "some(${n})",
  none => "none",
}

fn main() -> Unit = {
  println(show(option.filter(some(5), (x) => x > 3)))
  println(show(option.filter(some(2), (x) => x > 3)))
}
```
```output
some(5)
none
```

### `option.zip(a: Option[A], b: Option[B]) -> Option[(A, B)]`

2つの Option をタプルの Option にまとめます。どちらかが none なら none です。

```almd run
fn show(o: Option[(Int, Int)]) -> String = match o {
  some((a, b)) => "some((${a}, ${b}))",
  none => "none",
}

fn main() -> Unit = {
  println(show(option.zip(some(1), some(2))))
  println(show(option.zip(some(1), none)))
}
```
```output
some((1, 2))
none
```

### `option.or_else(o: Option[A], f: () -> Option[A]) -> Option[A]`

some の場合はその Option を返し、それ以外は関数を呼んで代替値を生成します。

```almd run
fn show(o: Option[Int]) -> String = match o {
  some(n) => "some(${n})",
  none => "none",
}

fn main() -> Unit = {
  println(show(option.or_else(none, () => some(42))))
  println(show(option.or_else(some(1), () => some(42))))
}
```
```output
some(42)
some(1)
```

### `option.to_list(o: Option[A]) -> List[A]`

some(x) を [x] に、none を [] に変換します。

```almd run
fn main() -> Unit = {
  println("${option.to_list(some(42))}")
  println("${option.to_list(none: Option[Int])}")
}
```
```output
[42]
[]
```


## 原文固定スナップショットの API 索引（14関数）

```
// some(f(v)) for some(v); none stays none.
// @since 0.7.0 or earlier
option.map(o: Option[A], f: (A) -> B) -> Option[B]

// f(v) for some(v); none stays none.
// @since 0.7.0 or earlier
option.flat_map(o: Option[A], f: (A) -> Option[B]) -> Option[B]

// Inner option of some(inner); none for none.
// @since 0.7.0 or earlier
option.flatten(o: Option[Option[A]]) -> Option[A]

// v for some(v), else default (always evaluated).
// @since 0.7.0 or earlier
option.unwrap_or(o: Option[A], default: A) -> A   (deprecated — use ??)

// v for some(v), else f(); f runs only for none.
// @since 0.7.0 or earlier
option.unwrap_or_else(o: Option[A], f: () -> A) -> A

// true for some(_), false for none.
// @since 0.7.0 or earlier
option.is_some(o: Option[A]) -> Bool

// true for none, false for some(_).
// @since 0.7.0 or earlier
option.is_none(o: Option[A]) -> Bool

// ok(v) for some(v), err(e) for none.
// @since 0.7.0 or earlier
option.to_result(o: Option[A], e: E) -> Result[A, E]

// o when some(v) and f(v) holds, else none.
// @since 0.7.0 or earlier
option.filter(o: Option[A], f: (A) -> Bool) -> Option[A]

// some((x, y)) when both are some, else none.
// @since 0.7.0 or earlier
option.zip(a: Option[A], b: Option[B]) -> Option[(A, B)]

// o when some, else f(); f runs only for none.
// @since 0.7.0 or earlier
option.or_else(o: Option[A], f: () -> Option[A]) -> Option[A]

// [v] for some(v), [] for none.
// @since 0.7.0 or earlier
option.to_list(o: Option[A]) -> List[A]

// some(values) if no element is none, else none.
// @since 0.15.0 or earlier
option.collect(xs: List[Option[A]]) -> Option[List[A]]

// some of all f(x), or none; f runs on every x.
// @since 0.15.0 or earlier
option.collect_map(xs: List[A], f: (A) -> Option[B]) -> Option[List[B]]
```


## API 説明の訳（上記署名順）

- `option.map(o: Option[A], f: (A) -> B) -> Option[B]`：some(v) には some(f(v))、none は none のまま。
- `option.flat_map(o: Option[A], f: (A) -> Option[B]) -> Option[B]`：some(v) には f(v)、none は none のまま。
- `option.flatten(o: Option[Option[A]]) -> Option[A]`：some(inner) の内側の option。none には none。
- `option.unwrap_or(o: Option[A], default: A) -> A   (deprecated — use ??)`：some(v) には v、それ以外は default（常に評価）。
- `option.unwrap_or_else(o: Option[A], f: () -> A) -> A`：some(v) には v、それ以外は f()。f は none の場合だけ実行します。
- `option.is_some(o: Option[A]) -> Bool`：some(_) には true、none には false。
- `option.is_none(o: Option[A]) -> Bool`：none には true、some(_) には false。
- `option.to_result(o: Option[A], e: E) -> Result[A, E]`：some(v) には ok(v)、none には err(e)。
- `option.filter(o: Option[A], f: (A) -> Bool) -> Option[A]`：some(v) で f(v) が成り立つなら o、それ以外は none。
- `option.zip(a: Option[A], b: Option[B]) -> Option[(A, B)]`：両方が some なら some((x, y))、それ以外は none。
- `option.or_else(o: Option[A], f: () -> Option[A]) -> Option[A]`：some なら o、それ以外は f()。f は none の場合だけ実行します。
- `option.to_list(o: Option[A]) -> List[A]`：some(v) には [v]、none には []。
- `option.collect(xs: List[Option[A]]) -> Option[List[A]]`：none の要素がなければ some(values)、それ以外は none。
- `option.collect_map(xs: List[A], f: (A) -> Option[B]) -> Option[List[B]]`：すべての f(x) を含む some、または none。f はすべての x に対して実行します。
