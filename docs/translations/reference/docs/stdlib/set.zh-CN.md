# set

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](set.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](set.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](set.zh-CN.md)



> 原文：[set.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/set.md)。这是供读者参考的非规范性固定快照，不参与自动生成或自动同步。签名、代码示例和 API 索引注释保持原文，说明译文附于文末。本次翻译未独立复核测量、行为或示例。

Set 操作。自动导入。

### `set.new() -> Set[A]`

创建空 set。

```almd run
fn main() -> Unit = {
  let s: Set[Int] = set.new()
  println("${set.len(s)}")
  println("${set.is_empty(s)}")
}
```
```output
0
true
```

### `set.from_list(xs: List[A]) -> Set[A]`

从值的列表创建 set。

```almd run
fn main() -> Unit = {
  let s = set.from_list([1, 2, 3])
  println("${s}")
  println("${set.len(s)}")
}
```
```output
set.from_list([1, 2, 3])
3
```

### `set.insert(s: Set[A], x: A) -> Set[A]`

向 set 添加值，返回新的 set。

```almd run
fn main() -> Unit = {
  let s = set.from_list([1, 2, 3])
  let s2 = set.insert(s, 42)
  println("${s}")
  println("${s2}")
}
```
```output
set.from_list([1, 2, 3])
set.from_list([1, 2, 3, 42])
```

### `set.remove(s: Set[A], x: A) -> Set[A]`

从 set 移除值，返回新的 set。

```almd run
fn main() -> Unit = {
  let s = set.from_list([1, 42])
  let s2 = set.remove(s, 42)
  println("${s}")
  println("${s2}")
}
```
```output
set.from_list([1, 42])
set.from_list([1])
```

### `set.contains(s: Set[A], x: A) -> Bool`

检查值是否在 set 中。

```almd run
fn main() -> Unit = {
  let s = set.from_list([1, 42])
  println("${set.contains(s, 42)}")
  println("${set.contains(s, 7)}")
}
```
```output
true
false
```

### `set.len(s: Set[A]) -> Int`

返回元素数量。

```almd run
fn main() -> Unit = {
  let s = set.from_list([1, 2, 3])
  println("${set.len(s)}")
}
```
```output
3
```

### `set.is_empty(s: Set[A]) -> Bool`

检查 set 是否没有元素。

```almd run
fn main() -> Unit = {
  let s: Set[Int] = set.new()
  println("${set.is_empty(s)}")
  println("${set.is_empty(set.from_list([1]))}")
}
```
```output
true
false
```

### `set.to_list(s: Set[A]) -> List[A]`

将 set 转换为列表。

```almd run
fn main() -> Unit = {
  let s = set.from_list([1, 2, 3])
  println("${set.to_list(s)}")
}
```
```output
[1, 2, 3]
```

### `set.union(a: Set[A], b: Set[A]) -> Set[A]`

返回两个 set 的并集。

```almd run
fn main() -> Unit = {
  let a = set.from_list([1, 2, 3])
  let b = set.from_list([2, 3, 4])
  println("${set.union(a, b)}")
}
```
```output
set.from_list([1, 2, 3, 4])
```

### `set.intersection(a: Set[A], b: Set[A]) -> Set[A]`

返回两个 set 的交集。

```almd run
fn main() -> Unit = {
  let a = set.from_list([1, 2, 3])
  let b = set.from_list([2, 3, 4])
  println("${set.intersection(a, b)}")
}
```
```output
set.from_list([2, 3])
```

### `set.difference(a: Set[A], b: Set[A]) -> Set[A]`

返回在 a 中但不在 b 中的元素。

```almd run
fn main() -> Unit = {
  let a = set.from_list([1, 2, 3])
  let b = set.from_list([2, 3, 4])
  println("${set.difference(a, b)}")
}
```
```output
set.from_list([1])
```

### `set.symmetric_difference(a: Set[A], b: Set[A]) -> Set[A]`

返回仅在其中一个 set 而非两者中都有的元素。

```almd run
fn main() -> Unit = {
  let a = set.from_list([1, 2, 3])
  let b = set.from_list([2, 3, 4])
  println("${set.symmetric_difference(a, b)}")
}
```
```output
set.from_list([1, 4])
```

### `set.is_subset(a: Set[A], b: Set[A]) -> Bool`

检查 a 的所有元素是否都在 b 中。

```almd run
fn main() -> Unit = {
  let a = set.from_list([1, 2])
  let b = set.from_list([1, 2, 3])
  println("${set.is_subset(a, b)}")
  println("${set.is_subset(b, a)}")
}
```
```output
true
false
```

### `set.is_disjoint(a: Set[A], b: Set[A]) -> Bool`

检查两个 set 是否没有共同元素。

```almd run
fn main() -> Unit = {
  let a = set.from_list([1, 2])
  let b = set.from_list([3, 4])
  println("${set.is_disjoint(a, b)}")
  println("${set.is_disjoint(a, set.from_list([2, 3]))}")
}
```
```output
true
false
```

### `set.filter(s: Set[A], f: (A) -> Bool) -> Set[A]`

保留满足谓词的元素。

```almd run
fn main() -> Unit = {
  let s = set.from_list([1, 2, 3, 4])
  println("${set.filter(s, (x) => x > 2)}")
}
```
```output
set.from_list([3, 4])
```

### `set.map(s: Set[A], f: (A) -> B) -> Set[B]`

对每个元素应用函数，返回新的 set。

```almd run
fn main() -> Unit = {
  let s = set.from_list([1, 2, 3])
  println("${set.map(s, (x) => x * 2)}")
}
```
```output
set.from_list([2, 4, 6])
```

### `set.fold(s: Set[A], init: B, f: (B, A) -> B) -> B`

从初始 accumulator 开始折叠 set。

```almd run
fn main() -> Unit = {
  let s = set.from_list([1, 2, 3])
  println("${set.fold(s, 0, (acc, x) => acc + x)}")
}
```
```output
6
```

### `set.any(s: Set[A], f: (A) -> Bool) -> Bool`

检查是否有任意元素满足谓词。

```almd run
fn main() -> Unit = {
  let s = set.from_list([1, 2, 3])
  println("${set.any(s, (x) => x > 2)}")
  println("${set.any(s, (x) => x > 3)}")
}
```
```output
true
false
```

### `set.all(s: Set[A], f: (A) -> Bool) -> Bool`

检查是否所有元素都满足谓词。

```almd run
fn main() -> Unit = {
  let s = set.from_list([1, 2, 3])
  println("${set.all(s, (x) => x > 0)}")
  println("${set.all(s, (x) => x > 1)}")
}
```
```output
true
false
```



<a id="signature-index-42-functions"></a>

## 固定快照 API 索引 (42 个函数)

```
// An empty Set[A].
// @since 0.8.0 or earlier
set.new() -> Set[A]

// Distinct elements of xs in first-seen order.
// @since 0.8.0 or earlier
set.from_list(xs: List[A]) -> Set[A]

// Copy with x appended; unchanged if present.
// @since 0.8.0 or earlier
set.insert(s: Set[A], x: A) -> Set[A]

// Copy without x; unchanged if absent.
// @since 0.8.0 or earlier
set.remove(s: Set[A], x: A) -> Set[A]

// true when x is a member.
// @since 0.8.0 or earlier
set.contains(s: Set[A], x: A) -> Bool

// Member count; 0 for an empty set.
// @since 0.8.0 or earlier
set.len(s: Set[A]) -> Int

// true iff s has no members.
// @since 0.8.0 or earlier
set.is_empty(s: Set[A]) -> Bool

// Members in insertion order.
// @since 0.8.0 or earlier
set.to_list(s: Set[A]) -> List[A]

// Members of a, then b's new ones.
// @since 0.8.0 or earlier
set.union(a: Set[A], b: Set[A]) -> Set[A]

// Members of a also in b, in a's order.
// @since 0.8.0 or earlier
set.intersection(a: Set[A], b: Set[A]) -> Set[A]

// Members of a not in b, in a's order.
// @since 0.8.0 or earlier
set.difference(a: Set[A], b: Set[A]) -> Set[A]

// In exactly one set: a-only, then b-only.
// @since 0.9.0 or earlier
set.symmetric_difference(a: Set[A], b: Set[A]) -> Set[A]

// true if all of a is in b; true for empty a.
// @since 0.9.0 or earlier
set.is_subset(a: Set[A], b: Set[A]) -> Bool

// true if a and b share no member.
// @since 0.9.0 or earlier
set.is_disjoint(a: Set[A], b: Set[A]) -> Bool

// Members where f holds, order kept.
// @since 0.9.0 or earlier
set.filter(s: Set[A], f: (A) -> Bool) -> Set[A]

// Set of f(x); shrinks when f results collide.
// @since 0.9.0 or earlier
set.map(s: Set[A], f: (A) -> B) -> Set[B]

// Folds f(acc, x) in insertion order.
// @since 0.9.0 or earlier
set.fold(s: Set[A], init: B, f: (B, A) -> B) -> B

// true if f holds for some member; false if empty.
// @since 0.9.0 or earlier
set.any(s: Set[A], f: (A) -> Bool) -> Bool

// true if f holds for every member; true if empty.
// @since 0.9.0 or earlier
set.all(s: Set[A], f: (A) -> Bool) -> Bool

// Number of members where f holds.
// @since unreleased
set.count(s: Set[A], f: (A) -> Bool) -> Int

// First member, in insertion order, where f holds, or none.
// @since unreleased
set.find(s: Set[A], f: (A) -> Bool) -> Option[A]

// Removes every member of s in place.
// @since unreleased
set.clear(mut s: Set[A]) -> Unit

// (index, member) pairs in insertion order, index from 0.
// @since unreleased
set.enumerate(s: Set[A]) -> List[(Int, A)]

// Members paired with ys by position; stops at the shorter.
// @since unreleased
set.zip(s: Set[A], ys: List[B]) -> List[(A, B)]

// f over (member, ys element) pairs; stops at the shorter.
// @since unreleased
set.zip_with(s: Set[A], ys: List[B], f: (A, B) -> C) -> List[C]

// The somes f gives, in insertion order.
// @since unreleased
set.filter_map(s: Set[A], f: (A) -> Option[B]) -> List[B]

// The first some f gives, or none; f is not called after the hit.
// @since unreleased
set.find_map(s: Set[A], f: (A) -> Option[B]) -> Option[B]

// Left fold seeded by the first member; none when empty.
// @since unreleased
set.reduce(s: Set[A], f: (A, A) -> A) -> Option[A]

// Running fold values in insertion order, init excluded.
// @since unreleased
set.scan(s: Set[A], init: B, f: (B, A) -> B) -> List[B]

// (members where f holds, the rest), insertion order kept.
// @since unreleased
set.partition(s: Set[A], f: (A) -> Bool) -> (List[A], List[A])

// Key f(x) -> members; keys in first-seen order.
// @since unreleased
set.group_by(s: Set[A], f: (A) -> B) -> Map[B, List[A]]

// Members in ascending order; Floats in IEEE totalOrder.
// @since unreleased
set.sort(s: Set[A]) -> List[A]

// Members sorted by key f(x), stable; f runs once per member.
// @since unreleased
set.sort_by(s: Set[A], f: (A) -> B) -> List[A]

// Members sorted by cmp, stable: negative puts a first.
// @since unreleased
set.sort_with(s: Set[A], cmp: (A, A) -> Int) -> List[A]

// Least member, or none when empty.
// @since unreleased
set.min(s: Set[A]) -> Option[A]

// Greatest member, or none when empty.
// @since unreleased
set.max(s: Set[A]) -> Option[A]

// Member with the least key f(x); the first on ties.
// @since unreleased
set.min_by(s: Set[A], f: (A) -> K) -> Option[A]

// Member with the greatest key f(x); the first on ties.
// @since unreleased
set.max_by(s: Set[A], f: (A) -> K) -> Option[A]

// Least member by cmp; the first on ties.
// @since unreleased
set.min_with(s: Set[A], cmp: (A, A) -> Int) -> Option[A]

// Greatest member by cmp; the first on ties.
// @since unreleased
set.max_with(s: Set[A], cmp: (A, A) -> Int) -> Option[A]

// Sum of the elements; 0 when empty; integers wrap on overflow.
// @since unreleased
set.sum(s: Set[A]) -> A

// Product of the elements; 1 when empty; integers wrap on overflow.
// @since unreleased
set.product(s: Set[A]) -> A
```



## API 说明译文（按上述签名顺序）

- `set.new() -> Set[A]` — 空 Set[A]。
- `set.from_list(xs: List[A]) -> Set[A]` — xs 中不同的元素，按首次出现顺序。
- `set.insert(s: Set[A], x: A) -> Set[A]` — 追加 x 的副本；已存在则不变。
- `set.remove(s: Set[A], x: A) -> Set[A]` — 移除 x 的副本；不存在则不变。
- `set.contains(s: Set[A], x: A) -> Bool` — x 是成员时为 true。
- `set.len(s: Set[A]) -> Int` — 成员数量；空 set 为 0。
- `set.is_empty(s: Set[A]) -> Bool` — 当且仅当 s 没有成员时为 true。
- `set.to_list(s: Set[A]) -> List[A]` — 按插入顺序返回成员。
- `set.union(a: Set[A], b: Set[A]) -> Set[A]` — a 的成员，随后是 b 的新增成员。
- `set.intersection(a: Set[A], b: Set[A]) -> Set[A]` — a 中也在 b 中的成员，按 a 的顺序。
- `set.difference(a: Set[A], b: Set[A]) -> Set[A]` — a 中不在 b 中的成员，按 a 的顺序。
- `set.symmetric_difference(a: Set[A], b: Set[A]) -> Set[A]` — 恰好属于一个 set：先仅属于 a 的，再仅属于 b 的。
- `set.is_subset(a: Set[A], b: Set[A]) -> Bool` — a 的全部成员都在 b 中则为 true；空 a 也为 true。
- `set.is_disjoint(a: Set[A], b: Set[A]) -> Bool` — a 与 b 没有共同成员时为 true。
- `set.filter(s: Set[A], f: (A) -> Bool) -> Set[A]` — f 成立的成员，保持顺序。
- `set.map(s: Set[A], f: (A) -> B) -> Set[B]` — f(x) 的 set；f 的结果碰撞时会缩小。
- `set.fold(s: Set[A], init: B, f: (B, A) -> B) -> B` — 按插入顺序折叠 f(acc, x)。
- `set.any(s: Set[A], f: (A) -> Bool) -> Bool` — 存在 f 成立的成员时为 true；空时为 false。
- `set.all(s: Set[A], f: (A) -> Bool) -> Bool` — 每个成员的 f 都成立时为 true；空时为 true。
- `set.count(s: Set[A], f: (A) -> Bool) -> Int` — f 成立的成员数量。
- `set.find(s: Set[A], f: (A) -> Bool) -> Option[A]` — 按插入顺序首个 f 成立的成员，或 none。
- `set.clear(mut s: Set[A]) -> Unit` — 原地移除 s 的每个成员。
- `set.enumerate(s: Set[A]) -> List[(Int, A)]` — 按插入顺序返回 (index, member) 对，index 从 0 开始。
- `set.zip(s: Set[A], ys: List[B]) -> List[(A, B)]` — 按位置将成员与 ys 配对；在较短者处停止。
- `set.zip_with(s: Set[A], ys: List[B], f: (A, B) -> C) -> List[C]` — 对 (member, ys 元素) 对应用 f；在较短者处停止。
- `set.filter_map(s: Set[A], f: (A) -> Option[B]) -> List[B]` — f 返回的 some，按插入顺序。
- `set.find_map(s: Set[A], f: (A) -> Option[B]) -> Option[B]` — f 返回的首个 some，或 none；命中后不再调用 f。
- `set.reduce(s: Set[A], f: (A, A) -> A) -> Option[A]` — 以首个成员为初始值左折叠；空时为 none。
- `set.scan(s: Set[A], init: B, f: (B, A) -> B) -> List[B]` — 按插入顺序折叠的中间值，不包括 init。
- `set.partition(s: Set[A], f: (A) -> Bool) -> (List[A], List[A])` — (f 成立的成员，其余成员)，保持插入顺序。
- `set.group_by(s: Set[A], f: (A) -> B) -> Map[B, List[A]]` — 键 f(x) → 成员组；键按首次出现顺序。
- `set.sort(s: Set[A]) -> List[A]` — 升序成员；Float 使用 IEEE totalOrder。
- `set.sort_by(s: Set[A], f: (A) -> B) -> List[A]` — 按键 f(x) 稳定排序成员；每个成员调用一次 f。
- `set.sort_with(s: Set[A], cmp: (A, A) -> Int) -> List[A]` — 按 cmp 稳定排序成员；负值将 a 放在前面。
- `set.min(s: Set[A]) -> Option[A]` — 最小成员，空时为 none。
- `set.max(s: Set[A]) -> Option[A]` — 最大成员，空时为 none。
- `set.min_by(s: Set[A], f: (A) -> K) -> Option[A]` — 键 f(x) 最小的成员；并列时取首个。
- `set.max_by(s: Set[A], f: (A) -> K) -> Option[A]` — 键 f(x) 最大的成员；并列时取首个。
- `set.min_with(s: Set[A], cmp: (A, A) -> Int) -> Option[A]` — 按 cmp 最小的成员；并列时取首个。
- `set.max_with(s: Set[A], cmp: (A, A) -> Int) -> Option[A]` — 按 cmp 最大的成员；并列时取首个。
- `set.sum(s: Set[A]) -> A` — 元素之和；空时为 0；整数溢出时回绕。
- `set.product(s: Set[A]) -> A` — 元素之积；空时为 1；整数溢出时回绕。
