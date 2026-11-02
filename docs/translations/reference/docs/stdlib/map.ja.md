# map

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](map.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](map.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](map.zh-CN.md)



> 原文：[map.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/map.md)。非規範的な読者向け固定スナップショットです。自動生成・自動同期されません。署名・コード例・API 索引のコメントは原文を保持し、説明の訳を末尾に併記しています。計測・動作・例をこの翻訳作業で独立に再検証していません。

Map（辞書）の操作。自動 import されます。

### `map.new() -> Map[K, V]`

空の map を作ります。

```almd run
fn main() -> Unit = {
  let m: Map[String, Int] = map.new()
  println("${map.len(m)}")
  println("${map.is_empty(m)}")
}
```
```output
0
true
```

### `map.get(m: Map[K, V], key: K) -> Option[V]`

キーで値を取得します。キーが存在しなければ none を返します。

```almd run
fn main() -> Unit = {
  let m = map.from_list([("name", "Alice")])
  match map.get(m, "name") {
    some(v) => println(v),
    none => println("missing"),
  }
  match map.get(m, "age") {
    some(v) => println(v),
    none => println("missing"),
  }
}
```
```output
Alice
missing
```

### `map.set(m: Map[K, V], key: K, value: V) -> Map[K, V]`

キーに value を設定した新しい map を返します。immutable であり、元の map は変更しません。

```almd run
fn main() -> Unit = {
  let m: Map[String, String] = map.new()
  let m2 = map.set(m, "name", "Alice")
  println("${m}")
  println("${m2}")
}
```
```output
[:]
["name": "Alice"]
```

### `map.contains(m: Map[K, V], key: K) -> Bool`

map にキーが存在するかを確認します。

```almd run
fn main() -> Unit = {
  let m = map.from_list([("name", "Alice")])
  println("${map.contains(m, "name")}")
  println("${map.contains(m, "age")}")
}
```
```output
true
false
```

### `map.remove(m: Map[K, V], key: K) -> Map[K, V]`

キーを削除した新しい map を返します。immutable であり、元の map は変更しません。

```almd run
fn main() -> Unit = {
  let m = map.from_list([("keep", 1), ("temp", 2)])
  let m2 = map.remove(m, "temp")
  println("${m}")
  println("${m2}")
}
```
```output
["keep": 1, "temp": 2]
["keep": 1]
```

### `map.keys(m: Map[K, V]) -> List[K]`

全キーを挿入順（各 entry が最初に挿入された順序）のリストとして取得します。キーを上書きしても位置は維持します。

```almd
map.keys(m)
```

### `map.values(m: Map[K, V]) -> List[V]`

全値をリストとして取得します。

```almd run
fn main() -> Unit = {
  let m = map.from_list([("a", 1), ("b", 2), ("c", 3)])
  println("${map.values(m)}")
}
```
```output
[1, 2, 3]
```

### `map.len(m: Map[K, V]) -> Int`

map 内のキーと値の組の数を取得します。

```almd run
fn main() -> Unit = {
  let m = map.from_list([("a", 1), ("b", 2), ("c", 3)])
  println("${map.len(m)}")
}
```
```output
3
```

### `map.to_list(m: Map[K, V]) -> List[(K, V)]`

全キーと値の組を、挿入順の tuple のリストとして取得します。

```almd
map.to_list(m)
```

### `map.merge(a: Map[K, V], b: Map[K, V]) -> Map[K, V]`

二つの map を統合します。二つ目の map のキーが一つ目のキーを上書きします。

```almd run
fn main() -> Unit = {
  let base = map.from_list([("a", 1), ("b", 2)])
  let overrides = map.from_list([("b", 20), ("c", 3)])
  println("${map.merge(base, overrides)}")
}
```
```output
["a": 1, "b": 20, "c": 3]
```

### `map.is_empty(m: Map[K, V]) -> Bool`

map が entry を持たないか確認します。

```almd run
fn main() -> Unit = {
  let m: Map[String, Int] = map.new()
  println("${map.is_empty(m)}")
  println("${map.is_empty(map.from_list([("a", 1)]))}")
}
```
```output
true
false
```

### `map.from_list(pairs: List[(K, V)]) -> Map[K, V]`

(key, value) の組のリストから map を作ります。

```almd run
fn main() -> Unit = {
  let m = map.from_list([("a", 1), ("b", 2)])
  println("${m}")
  println("${map.len(m)}")
}
```
```output
["a": 1, "b": 2]
2
```

### `map.map(m: Map[K, V], f: (V) -> B) -> Map[K, B]`

キーは変えず、関数で map の全値を変換します。

```almd run
fn main() -> Unit = {
  let m = map.from_list([("a", 1), ("b", 2), ("c", 3)])
  let doubled = map.map(m, (v) => v * 2)
  println("${map.get(doubled, "a") ?? 0} ${map.get(doubled, "b") ?? 0} ${map.get(doubled, "c") ?? 0}")
  println(int.to_string(map.len(doubled)))
}
```
```output
2 4 6
3
```

### `map.filter(m: Map[K, V], f: ((K, V)) -> Bool) -> Map[K, V]`

述語が true を返す entry だけを含む新しい map を返します。

```almd run
fn main() -> Unit = {
  let m = map.from_list([("a", 1), ("b", 0), ("c", 3)])
  println("${map.filter(m, (k, v) => v > 0)}")
}
```
```output
["a": 1, "c": 3]
```

### `map.fold(m: Map[K, V], init: A, f: (A, (K, V)) -> A) -> A`

初期値を用いて全 entry を累積します。

```almd run
fn main() -> Unit = {
  let scores = map.from_list([("alice", 90), ("bob", 75), ("carol", 82)])
  println("${map.fold(scores, 0, (acc, (k, v)) => acc + v)}")
}
```
```output
247
```

### `map.any(m: Map[K, V], f: ((K, V)) -> Bool) -> Bool`

述語を満たす entry が一つでもあるか確認します。

```almd run
fn main() -> Unit = {
  let scores = map.from_list([("alice", 90), ("bob", 75), ("carol", 82)])
  println("${map.any(scores, (k, v) => v >= 90)}")
  println("${map.any(scores, (k, v) => v >= 100)}")
}
```
```output
true
false
```

### `map.all(m: Map[K, V], f: ((K, V)) -> Bool) -> Bool`

全 entry が述語を満たすか確認します。

```almd run
fn main() -> Unit = {
  let scores = map.from_list([("alice", 90), ("bob", 75), ("carol", 82)])
  println("${map.all(scores, (k, v) => v > 0)}")
  println("${map.all(scores, (k, v) => v >= 80)}")
}
```
```output
true
false
```

### `map.count(m: Map[K, V], f: ((K, V)) -> Bool) -> Int`

述語を満たす entry を数えます。

```almd run
fn main() -> Unit = {
  let scores = map.from_list([("alice", 90), ("bob", 75), ("carol", 82)])
  println("${map.count(scores, (k, v) => v >= 80)}")
}
```
```output
2
```

### `map.find(m: Map[K, V], f: ((K, V)) -> Bool) -> Option[(K, V)]`

述語を満たす最初の entry を見つけます。Option[(K, V)] を返します。

```almd run
fn main() -> Unit = {
  let scores = map.from_list([("alice", 90), ("bob", 75), ("carol", 82)])
  match map.find(scores, (k, v) => v >= 90) {
    some((k, v)) => println("${k}: ${v}"),
    none => println("no match"),
  }
  match map.find(scores, (k, v) => v >= 100) {
    some((k, v)) => println("${k}: ${v}"),
    none => println("no match"),
  }
}
```
```output
alice: 90
no match
```

### `map.update(m: Map[K, V], key: K, f: (V) -> V) -> Map[K, V]`

既存キーの値を関数で更新します。キーがなければ map を変更せず、関数も呼びません。

```almd run
fn main() -> Unit = {
  let scores = map.from_list([("alice", 90), ("bob", 75)])
  println("${map.update(scores, "alice", (v) => v + 10)}")
}
```
```output
["alice": 100, "bob": 75]
```

### `map.clear(mut m: Map[K, V]) -> Unit`

全 entry をその場で削除します。var 束縛が必要です。

```almd check
fn main() -> Unit = {
  var m = map.from_list([("a", 1), ("b", 2)])
  map.clear(m)
  println("${map.len(m)}")
  println("${map.is_empty(m)}")
}
```



<a id="signature-index-37-functions"></a>

## 固定スナップショットの API 索引 (37 関数)

```
// An empty Map[K, V].
// @since 0.5.0 or earlier
map.new() -> Map[K, V]

// Value for key, or none when absent.
// @since 0.5.0 or earlier
map.get(m: Map[K, V], key: K) -> Option[V]

// Copy with key -> value; an existing key is overwritten and keeps its position.
// @since 0.5.0 or earlier
map.set(m: Map[K, V], key: K, value: V) -> Map[K, V]

// true when key is present.
// @since 0.5.0 or earlier
map.contains(m: Map[K, V], key: K) -> Bool

// Copy without key; unchanged if absent.
// @since 0.5.0 or earlier
map.remove(m: Map[K, V], key: K) -> Map[K, V]

// Every key, in insertion order.
// @since 0.5.0 or earlier
map.keys(m: Map[K, V]) -> List[K]

// Every value, in key insertion order.
// @since 0.5.0 or earlier
map.values(m: Map[K, V]) -> List[V]

// Entry count; 0 for an empty map.
// @since 0.5.0 or earlier
map.len(m: Map[K, V]) -> Int

// (key, value) pairs in insertion order.
// @since 0.5.0 or earlier
map.to_list(m: Map[K, V]) -> List[(K, V)]

// a plus b's entries; b wins on shared keys.
// @since 0.5.0 or earlier
map.merge(a: Map[K, V], b: Map[K, V]) -> Map[K, V]

// true iff m has no entries.
// @since 0.6.0 or earlier
map.is_empty(m: Map[K, V]) -> Bool

// Map of the pairs xs; a repeated key takes the last value.
// @since 0.5.0 or earlier
map.from_list(xs: List[(K, V)]) -> Map[K, V]

// Same keys and order, values replaced by f(v).
// @since 0.6.1 or earlier
map.map(m: Map[K, V], f: (V) -> B) -> Map[K, B]

// Entries where f((k, v)) holds, order kept.
// @since 0.5.0 or earlier
map.filter(m: Map[K, V], f: ((K, V)) -> Bool) -> Map[K, V]

// Folds f(acc, (k, v)) in insertion order.
// @since 0.7.0 or earlier
map.fold(m: Map[K, V], init: A, f: (A, (K, V)) -> A) -> A

// true if some f((k, v)) holds; false when empty.
// @since 0.7.0 or earlier
map.any(m: Map[K, V], f: ((K, V)) -> Bool) -> Bool

// true if every f((k, v)) holds; true when empty.
// @since 0.7.0 or earlier
map.all(m: Map[K, V], f: ((K, V)) -> Bool) -> Bool

// Number of entries where f((k, v)) holds.
// @since 0.7.0 or earlier
map.count(m: Map[K, V], f: ((K, V)) -> Bool) -> Int

// First (k, v) where f holds, or none.
// @since 0.7.0 or earlier
map.find(m: Map[K, V], f: ((K, V)) -> Bool) -> Option[(K, V)]

// Copy with key's value f(v); no-op if absent.
// @since 0.7.0 or earlier
map.update(m: Map[K, V], key: K, f: (V) -> V) -> Map[K, V]

// key -> f(old) if present, else init appended.
// @since 0.57.0
map.upsert(m: Map[K, V], key: K, init: V, f: (V) -> V) -> Map[K, V]

// Removes every entry of m in place.
// @since 0.9.3 or earlier
map.clear(mut m: Map[K, V]) -> Unit

// (index, (key, value)) pairs in insertion order, index from 0.
// @since unreleased
map.enumerate(m: Map[K, V]) -> List[(Int, (K, V))]

// Entries paired with ys by position; stops at the shorter.
// @since unreleased
map.zip(m: Map[K, V], ys: List[B]) -> List[((K, V), B)]

// f over (entry, ys element) pairs; stops at the shorter.
// @since unreleased
map.zip_with(m: Map[K, V], ys: List[B], f: ((K, V), B) -> C) -> List[C]

// The somes f gives over the entries, in insertion order.
// @since unreleased
map.filter_map(m: Map[K, V], f: ((K, V)) -> Option[B]) -> List[B]

// The first some f gives over the entries, or none; f is not called after the hit.
// @since unreleased
map.find_map(m: Map[K, V], f: ((K, V)) -> Option[B]) -> Option[B]

// Left fold of the entries seeded by the first; none when empty.
// @since unreleased
map.reduce(m: Map[K, V], f: ((K, V), (K, V)) -> (K, V)) -> Option[(K, V)]

// Running fold values over the entries, init excluded.
// @since unreleased
map.scan(m: Map[K, V], init: B, f: (B, (K, V)) -> B) -> List[B]

// (entries where f holds, the rest), insertion order kept.
// @since unreleased
map.partition(m: Map[K, V], f: ((K, V)) -> Bool) -> (List[(K, V)], List[(K, V)])

// Key f(entry) -> entries; keys in first-seen order.
// @since unreleased
map.group_by(m: Map[K, V], f: ((K, V)) -> B) -> Map[B, List[(K, V)]]

// Entries sorted by key f(entry), stable; f runs once per entry.
// @since unreleased
map.sort_by(m: Map[K, V], f: ((K, V)) -> B) -> List[(K, V)]

// Entries sorted by cmp, stable: negative puts a first.
// @since unreleased
map.sort_with(m: Map[K, V], cmp: ((K, V), (K, V)) -> Int) -> List[(K, V)]

// Entry with the least f(entry); the first on ties.
// @since unreleased
map.min_by(m: Map[K, V], f: ((K, V)) -> U) -> Option[(K, V)]

// Entry with the greatest f(entry); the first on ties.
// @since unreleased
map.max_by(m: Map[K, V], f: ((K, V)) -> U) -> Option[(K, V)]

// Least entry by cmp; the first on ties.
// @since unreleased
map.min_with(m: Map[K, V], cmp: ((K, V), (K, V)) -> Int) -> Option[(K, V)]

// Greatest entry by cmp; the first on ties.
// @since unreleased
map.max_with(m: Map[K, V], cmp: ((K, V), (K, V)) -> Int) -> Option[(K, V)]
```



## API 説明の訳（上記署名順）

- `map.new() -> Map[K, V]` — 空の Map[K, V]。
- `map.get(m: Map[K, V], key: K) -> Option[V]` — key に対応する値。なければ none。
- `map.set(m: Map[K, V], key: K, value: V) -> Map[K, V]` — key → value としたコピー。既存キーは上書きし、位置を維持します。
- `map.contains(m: Map[K, V], key: K) -> Bool` — key があれば true。
- `map.remove(m: Map[K, V], key: K) -> Map[K, V]` — key を除いたコピー。なければ変更なし。
- `map.keys(m: Map[K, V]) -> List[K]` — 全キーを挿入順で返します。
- `map.values(m: Map[K, V]) -> List[V]` — 全値をキーの挿入順で返します。
- `map.len(m: Map[K, V]) -> Int` — entry 数。空の map では 0。
- `map.to_list(m: Map[K, V]) -> List[(K, V)]` — (key, value) の組を挿入順で返します。
- `map.merge(a: Map[K, V], b: Map[K, V]) -> Map[K, V]` — a に b の entry を追加。共通キーでは b が優先。
- `map.is_empty(m: Map[K, V]) -> Bool` — m に entry がない場合に限り true。
- `map.from_list(xs: List[(K, V)]) -> Map[K, V]` — pairs xs から map を作ります。重複キーは最後の値を採用します。
- `map.map(m: Map[K, V], f: (V) -> B) -> Map[K, B]` — キーと順序はそのまま、値を f(v) に置換します。
- `map.filter(m: Map[K, V], f: ((K, V)) -> Bool) -> Map[K, V]` — f((k, v)) が成立する entry。順序を保持します。
- `map.fold(m: Map[K, V], init: A, f: (A, (K, V)) -> A) -> A` — 挿入順で f(acc, (k, v)) を fold します。
- `map.any(m: Map[K, V], f: ((K, V)) -> Bool) -> Bool` — f((k, v)) が成立するものがあれば true。空なら false。
- `map.all(m: Map[K, V], f: ((K, V)) -> Bool) -> Bool` — 全 f((k, v)) が成立すれば true。空なら true。
- `map.count(m: Map[K, V], f: ((K, V)) -> Bool) -> Int` — f((k, v)) が成立する entry 数。
- `map.find(m: Map[K, V], f: ((K, V)) -> Bool) -> Option[(K, V)]` — f が成立する最初の (k, v)。なければ none。
- `map.update(m: Map[K, V], key: K, f: (V) -> V) -> Map[K, V]` — key の値を f(v) にしたコピー。キーがなければ何もしません。
- `map.upsert(m: Map[K, V], key: K, init: V, f: (V) -> V) -> Map[K, V]` — 存在すれば key → f(old)、なければ init を末尾に追加します。
- `map.clear(mut m: Map[K, V]) -> Unit` — m の全 entry をその場で削除します。
- `map.enumerate(m: Map[K, V]) -> List[(Int, (K, V))]` — (index, (key, value)) の組を挿入順で返します。index は 0 から。
- `map.zip(m: Map[K, V], ys: List[B]) -> List[((K, V), B)]` — entry と ys を位置ごとに組にします。短い方で終了します。
- `map.zip_with(m: Map[K, V], ys: List[B], f: ((K, V), B) -> C) -> List[C]` — (entry, ys の要素) の組に f を適用します。短い方で終了します。
- `map.filter_map(m: Map[K, V], f: ((K, V)) -> Option[B]) -> List[B]` — entry に f を適用して得た some を挿入順で返します。
- `map.find_map(m: Map[K, V], f: ((K, V)) -> Option[B]) -> Option[B]` — entry に f を適用して得た最初の some。なければ none。一致後は f を呼びません。
- `map.reduce(m: Map[K, V], f: ((K, V), (K, V)) -> (K, V)) -> Option[(K, V)]` — 最初の entry を初期値とする entry の左 fold。空なら none。
- `map.scan(m: Map[K, V], init: B, f: (B, (K, V)) -> B) -> List[B]` — entry の fold の途中値。init は含みません。
- `map.partition(m: Map[K, V], f: ((K, V)) -> Bool) -> (List[(K, V)], List[(K, V)])` — (f が成立する entry、それ以外)。挿入順を保持します。
- `map.group_by(m: Map[K, V], f: ((K, V)) -> B) -> Map[B, List[(K, V)]]` — キー f(entry) → entry 群。キーは最初に現れた順序。
- `map.sort_by(m: Map[K, V], f: ((K, V)) -> B) -> List[(K, V)]` — キー f(entry) で entry を安定ソートします。f は entry ごとに一回実行します。
- `map.sort_with(m: Map[K, V], cmp: ((K, V), (K, V)) -> Int) -> List[(K, V)]` — cmp で entry を安定ソートします。負なら a が先。
- `map.min_by(m: Map[K, V], f: ((K, V)) -> U) -> Option[(K, V)]` — f(entry) が最小の entry。同値なら最初のもの。
- `map.max_by(m: Map[K, V], f: ((K, V)) -> U) -> Option[(K, V)]` — f(entry) が最大の entry。同値なら最初のもの。
- `map.min_with(m: Map[K, V], cmp: ((K, V), (K, V)) -> Int) -> Option[(K, V)]` — cmp で最小となる entry。同値なら最初のもの。
- `map.max_with(m: Map[K, V], cmp: ((K, V), (K, V)) -> Int) -> Option[(K, V)]` — cmp で最大となる entry。同値なら最初のもの。
