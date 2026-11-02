# map

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](map.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](map.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](map.zh-CN.md)



> 原文：[map.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/map.md)。这是供读者参考的非规范性固定快照，不参与自动生成或自动同步。签名、代码示例和 API 索引注释保持原文，说明译文附于文末。本次翻译未独立复核测量、行为或示例。

Map（字典）操作。自动导入。

### `map.new() -> Map[K, V]`

创建空 map。

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

按键获取值。键不存在时返回 none。

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

返回将键设置为 value 的新 map。不可变操作，不修改原 map。

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

检查 map 中是否存在某个键。

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

返回移除该键的新 map。不可变操作，不修改原 map。

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

按插入顺序（各 entry 首次插入的顺序）获取所有键的列表；覆盖键时保留其位置。

```almd
map.keys(m)
```

### `map.values(m: Map[K, V]) -> List[V]`

以列表形式获取所有值。

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

获取 map 中键值对的数量。

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

按插入顺序，以 tuple 列表获取全部键值对。

```almd
map.to_list(m)
```

### `map.merge(a: Map[K, V], b: Map[K, V]) -> Map[K, V]`

合并两个 map。第二个 map 的键覆盖第一个的键。

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

检查 map 是否没有 entry。

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

从 (key, value) 对的列表创建 map。

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

用函数转换 map 中全部值，键保持不变。

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

返回只包含谓词为 true 的 entry 的新 map。

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

从初始值开始，对所有 entry 累积。

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

检查是否有任意 entry 满足谓词。

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

检查所有 entry 是否满足谓词。

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

统计满足谓词的 entry。

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

查找首个匹配谓词的 entry，返回 Option[(K, V)]。

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

用函数更新已有键的值。键不存在时 map 保持不变，也不调用函数。

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

原地移除所有 entry。需要 var 绑定。

```almd check
fn main() -> Unit = {
  var m = map.from_list([("a", 1), ("b", 2)])
  map.clear(m)
  println("${map.len(m)}")
  println("${map.is_empty(m)}")
}
```



<a id="signature-index-37-functions"></a>

## 固定快照 API 索引 (37 个函数)

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



## API 说明译文（按上述签名顺序）

- `map.new() -> Map[K, V]` — 空 Map[K, V]。
- `map.get(m: Map[K, V], key: K) -> Option[V]` — key 对应的值，不存在时为 none。
- `map.set(m: Map[K, V], key: K, value: V) -> Map[K, V]` — 设置 key → value 的副本；已有键被覆盖，但保留位置。
- `map.contains(m: Map[K, V], key: K) -> Bool` — key 存在时为 true。
- `map.remove(m: Map[K, V], key: K) -> Map[K, V]` — 移除 key 的副本；不存在时保持不变。
- `map.keys(m: Map[K, V]) -> List[K]` — 按插入顺序返回所有键。
- `map.values(m: Map[K, V]) -> List[V]` — 按键插入顺序返回所有值。
- `map.len(m: Map[K, V]) -> Int` — entry 数量，空 map 为 0。
- `map.to_list(m: Map[K, V]) -> List[(K, V)]` — 按插入顺序返回 (key, value) 对。
- `map.merge(a: Map[K, V], b: Map[K, V]) -> Map[K, V]` — a 加上 b 的 entry；共有键以 b 为准。
- `map.is_empty(m: Map[K, V]) -> Bool` — 当且仅当 m 没有 entry 时为 true。
- `map.from_list(xs: List[(K, V)]) -> Map[K, V]` — 由 pairs xs 构造 map；重复键采用最后一个值。
- `map.map(m: Map[K, V], f: (V) -> B) -> Map[K, B]` — 键和顺序相同，值替换为 f(v)。
- `map.filter(m: Map[K, V], f: ((K, V)) -> Bool) -> Map[K, V]` — f((k, v)) 成立的 entry，保持顺序。
- `map.fold(m: Map[K, V], init: A, f: (A, (K, V)) -> A) -> A` — 按插入顺序折叠 f(acc, (k, v))。
- `map.any(m: Map[K, V], f: ((K, V)) -> Bool) -> Bool` — 存在 f((k, v)) 成立时为 true；空时为 false。
- `map.all(m: Map[K, V], f: ((K, V)) -> Bool) -> Bool` — 所有 f((k, v)) 成立时为 true；空时为 true。
- `map.count(m: Map[K, V], f: ((K, V)) -> Bool) -> Int` — f((k, v)) 成立的 entry 数量。
- `map.find(m: Map[K, V], f: ((K, V)) -> Bool) -> Option[(K, V)]` — 首个 f 成立的 (k, v)，不存在则为 none。
- `map.update(m: Map[K, V], key: K, f: (V) -> V) -> Map[K, V]` — 将 key 的值设为 f(v) 的副本；key 不存在则不操作。
- `map.upsert(m: Map[K, V], key: K, init: V, f: (V) -> V) -> Map[K, V]` — 存在则 key → f(old)，否则在末尾追加 init。
- `map.clear(mut m: Map[K, V]) -> Unit` — 原地移除 m 的每个 entry。
- `map.enumerate(m: Map[K, V]) -> List[(Int, (K, V))]` — 按插入顺序返回 (index, (key, value)) 对，index 从 0 开始。
- `map.zip(m: Map[K, V], ys: List[B]) -> List[((K, V), B)]` — 按位置将 entry 与 ys 配对，在较短者处停止。
- `map.zip_with(m: Map[K, V], ys: List[B], f: ((K, V), B) -> C) -> List[C]` — 对 (entry, ys 元素) 对应用 f，在较短者处停止。
- `map.filter_map(m: Map[K, V], f: ((K, V)) -> Option[B]) -> List[B]` — 对 entry 应用 f 所得的 some，按插入顺序返回。
- `map.find_map(m: Map[K, V], f: ((K, V)) -> Option[B]) -> Option[B]` — 对 entry 应用 f 所得的首个 some，或 none；命中后不再调用 f。
- `map.reduce(m: Map[K, V], f: ((K, V), (K, V)) -> (K, V)) -> Option[(K, V)]` — 以首个 entry 为初始值对 entry 左折叠；空时为 none。
- `map.scan(m: Map[K, V], init: B, f: (B, (K, V)) -> B) -> List[B]` — 对 entry 折叠的中间值，不包括 init。
- `map.partition(m: Map[K, V], f: ((K, V)) -> Bool) -> (List[(K, V)], List[(K, V)])` — (f 成立的 entry，其余 entry)，保持插入顺序。
- `map.group_by(m: Map[K, V], f: ((K, V)) -> B) -> Map[B, List[(K, V)]]` — 键 f(entry) → entry 组；键按首次出现顺序。
- `map.sort_by(m: Map[K, V], f: ((K, V)) -> B) -> List[(K, V)]` — 按键 f(entry) 稳定排序 entry；每个 entry 调用一次 f。
- `map.sort_with(m: Map[K, V], cmp: ((K, V), (K, V)) -> Int) -> List[(K, V)]` — 按 cmp 稳定排序 entry；负值将 a 放在前面。
- `map.min_by(m: Map[K, V], f: ((K, V)) -> U) -> Option[(K, V)]` — f(entry) 最小的 entry；并列时取首个。
- `map.max_by(m: Map[K, V], f: ((K, V)) -> U) -> Option[(K, V)]` — f(entry) 最大的 entry；并列时取首个。
- `map.min_with(m: Map[K, V], cmp: ((K, V), (K, V)) -> Int) -> Option[(K, V)]` — 按 cmp 最小的 entry；并列时取首个。
- `map.max_with(m: Map[K, V], cmp: ((K, V), (K, V)) -> Int) -> Option[(K, V)]` — 按 cmp 最大的 entry；并列时取首个。
