# set

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](set.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](set.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](set.zh-CN.md)



> 原文：[set.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/set.md)。非規範的な読者向け固定スナップショットです。自動生成・自動同期されません。署名・コード例・API 索引のコメントは原文を保持し、説明の訳を末尾に併記しています。計測・動作・例をこの翻訳作業で独立に再検証していません。

Set の操作。自動 import されます。

### `set.new() -> Set[A]`

空の set を作ります。

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

値のリストから set を作ります。

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

set に値を追加します。新しい set を返します。

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

set から値を削除します。新しい set を返します。

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

値が set に含まれるか確認します。

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

要素数を返します。

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

set が要素を持たないか確認します。

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

set をリストに変換します。

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

二つの set の和集合を返します。

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

二つの set の共通部分を返します。

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

a に含まれ、b に含まれない要素を返します。

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

どちらか一方の set だけに含まれる要素を返します。

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

a の全要素が b に含まれるか確認します。

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

二つの set に共通要素がないか確認します。

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

述語を満たす要素を保持します。

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

各要素に関数を適用し、新しい set を返します。

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

初期 accumulator を用いて set を畳み込みます。

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

述語を満たす要素が一つでもあるか確認します。

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

全要素が述語を満たすか確認します。

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



## 固定スナップショットの API 索引 (42 関数)

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



## API 説明の訳（上記署名順）

- `set.new() -> Set[A]` — 空の Set[A]。
- `set.from_list(xs: List[A]) -> Set[A]` — xs の重複しない要素を、最初に現れた順で返します。
- `set.insert(s: Set[A], x: A) -> Set[A]` — x を末尾に追加したコピー。すでにあれば変更なし。
- `set.remove(s: Set[A], x: A) -> Set[A]` — x を除いたコピー。なければ変更なし。
- `set.contains(s: Set[A], x: A) -> Bool` — x が要素なら true。
- `set.len(s: Set[A]) -> Int` — 要素数。空の set では 0。
- `set.is_empty(s: Set[A]) -> Bool` — s に要素がない場合に限り true。
- `set.to_list(s: Set[A]) -> List[A]` — 要素を挿入順で返します。
- `set.union(a: Set[A], b: Set[A]) -> Set[A]` — a の要素、その後に b の新しい要素。
- `set.intersection(a: Set[A], b: Set[A]) -> Set[A]` — a の要素のうち b にもあるもの。a の順序。
- `set.difference(a: Set[A], b: Set[A]) -> Set[A]` — a の要素のうち b にないもの。a の順序。
- `set.symmetric_difference(a: Set[A], b: Set[A]) -> Set[A]` — ちょうど一方だけにある要素。a のみの要素、その後に b のみの要素。
- `set.is_subset(a: Set[A], b: Set[A]) -> Bool` — a の全要素が b にあれば true。a が空でも true。
- `set.is_disjoint(a: Set[A], b: Set[A]) -> Bool` — a と b が要素を共有しなければ true。
- `set.filter(s: Set[A], f: (A) -> Bool) -> Set[A]` — f が成立する要素。順序を保持します。
- `set.map(s: Set[A], f: (A) -> B) -> Set[B]` — f(x) の set。f の結果が衝突すると要素数が減ります。
- `set.fold(s: Set[A], init: B, f: (B, A) -> B) -> B` — 挿入順で f(acc, x) を fold します。
- `set.any(s: Set[A], f: (A) -> Bool) -> Bool` — f が成立する要素があれば true。空なら false。
- `set.all(s: Set[A], f: (A) -> Bool) -> Bool` — 全要素で f が成立すれば true。空なら true。
- `set.count(s: Set[A], f: (A) -> Bool) -> Int` — f が成立する要素数。
- `set.find(s: Set[A], f: (A) -> Bool) -> Option[A]` — 挿入順で最初に f が成立する要素。なければ none。
- `set.clear(mut s: Set[A]) -> Unit` — s の全要素をその場で削除します。
- `set.enumerate(s: Set[A]) -> List[(Int, A)]` — (index, member) の組を挿入順で返します。index は 0 から。
- `set.zip(s: Set[A], ys: List[B]) -> List[(A, B)]` — 要素と ys を位置ごとに組にします。短い方で終了します。
- `set.zip_with(s: Set[A], ys: List[B], f: (A, B) -> C) -> List[C]` — (member, ys の要素) の組に f を適用します。短い方で終了します。
- `set.filter_map(s: Set[A], f: (A) -> Option[B]) -> List[B]` — f が返す some を挿入順で返します。
- `set.find_map(s: Set[A], f: (A) -> Option[B]) -> Option[B]` — f が返す最初の some。なければ none。一致後は f を呼びません。
- `set.reduce(s: Set[A], f: (A, A) -> A) -> Option[A]` — 最初の要素を初期値とする左 fold。空なら none。
- `set.scan(s: Set[A], init: B, f: (B, A) -> B) -> List[B]` — 挿入順の fold の途中値。init は含みません。
- `set.partition(s: Set[A], f: (A) -> Bool) -> (List[A], List[A])` — (f が成立する要素、それ以外)。挿入順を保持します。
- `set.group_by(s: Set[A], f: (A) -> B) -> Map[B, List[A]]` — キー f(x) → 要素群。キーは最初に現れた順序。
- `set.sort(s: Set[A]) -> List[A]` — 要素を昇順にします。Float は IEEE totalOrder。
- `set.sort_by(s: Set[A], f: (A) -> B) -> List[A]` — キー f(x) で要素を安定ソートします。f は要素ごとに一回実行します。
- `set.sort_with(s: Set[A], cmp: (A, A) -> Int) -> List[A]` — cmp で要素を安定ソートします。負なら a が先。
- `set.min(s: Set[A]) -> Option[A]` — 最小要素。空なら none。
- `set.max(s: Set[A]) -> Option[A]` — 最大要素。空なら none。
- `set.min_by(s: Set[A], f: (A) -> K) -> Option[A]` — キー f(x) が最小の要素。同値なら最初のもの。
- `set.max_by(s: Set[A], f: (A) -> K) -> Option[A]` — キー f(x) が最大の要素。同値なら最初のもの。
- `set.min_with(s: Set[A], cmp: (A, A) -> Int) -> Option[A]` — cmp で最小となる要素。同値なら最初のもの。
- `set.max_with(s: Set[A], cmp: (A, A) -> Int) -> Option[A]` — cmp で最大となる要素。同値なら最初のもの。
- `set.sum(s: Set[A]) -> A` — 要素の和。空なら 0。整数は overflow で wrap します。
- `set.product(s: Set[A]) -> A` — 要素の積。空なら 1。整数は overflow で wrap します。
