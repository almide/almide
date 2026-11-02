# deque

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](deque.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](deque.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](deque.zh-CN.md)



> 原文：[deque.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/deque.md)。読者向けの非規範的な固定スナップショットです。自動生成・自動同期の対象ではありません。署名・型・`@since` とコード（コメントを含む）は原文のまま保持し、API 説明の訳を末尾に併記しています。コード例・性能・安全性・ターゲット間の一致は、この翻訳作業では独立実行・検証していません。

> 償却 O(1) は各操作の最悪時間が常に一定という意味ではありません。API 索引の同点時の先頭優先、空の場合の値、整数オーバーフロー時の折り返しは原文どおり保持しています。

両端キュー `Deque[A]`。両端での追加・取り出しは償却 O(1) です。自動インポートされます。純粋で、両ターゲットで同一です。

`push` / `pop` は `list.push` / `list.pop` と同様に末尾で、`push_front` / `pop_front` は先頭で動作します。4つともキューをその場で変更し（`mut`）、`clear` も同様です。他の操作はキューを変更しません。空のキューから取り出すと `none` になります。位置・範囲の関数（`get`、`take`、`slice`、`set` など）は同名の `list` 関数と同じ引数を受け取り、同じ結果を返します。列のアルゴリズム（`map`、`fold`、`sort_by`、`min_by` など）は先頭から末尾へ走査します。

```almd run
fn main() -> Unit = {
  var q: Deque[Int] = deque.new()
  deque.push(q, 2)
  deque.push(q, 3)
  deque.push_front(q, 1)
  println("${deque.to_list(q)}")
  println("${deque.pop_front(q)} ${deque.pop(q)} ${deque.len(q)}")
}
```
```output
[1, 2, 3]
some(1) some(3) 1
```

幅優先探索では探索の境界をキューに保持します：

```almd run
fn reachable(edges: Map[Int, List[Int]], start: Int) -> List[Int] = {
  var seen = [start]
  var queue = deque.from_list([start])
  for _ in 0..<100 {
    match deque.pop_front(queue) {
      some(n) => for m in map.get(edges, n) ?? [] {
        if list.contains(seen, m) then ()
        else {
          seen = seen + [m]
          deque.push(queue, m)
        }
      },
      none => (),
    }
  }
  seen
}

fn main() -> Unit = {
  let edges = map.from_list([(1, [2, 3]), (2, [4]), (3, [4]), (4, [])])
  println("${reachable(edges, 1)}")
}
```
```output
[1, 2, 3, 4]
```


<a id="signature-index-60-functions"></a>

## 原文固定スナップショットの API 索引（60関数）

```
// An empty deque.
// @since unreleased
deque.new() -> Deque[A]

// A deque of xs, front to back.
// @since unreleased
deque.from_list(xs: List[A]) -> Deque[A]

// The elements, front to back.
// @since unreleased
deque.to_list(d: Deque[A]) -> List[A]

// Element count; 0 for an empty deque.
// @since unreleased
deque.len(d: Deque[A]) -> Int

// true iff d has no elements.
// @since unreleased
deque.is_empty(d: Deque[A]) -> Bool

// Appends x at the back in place.
// @since unreleased
deque.push(mut d: Deque[A], x: A) -> Unit

// Adds x at the front in place.
// @since unreleased
deque.push_front(mut d: Deque[A], x: A) -> Unit

// Removes and yields the last element; none when empty.
// @since unreleased
deque.pop(mut d: Deque[A]) -> Option[A]

// Removes and yields the first element; none when empty.
// @since unreleased
deque.pop_front(mut d: Deque[A]) -> Option[A]

// The first element, or none when empty.
// @since unreleased
deque.first(d: Deque[A]) -> Option[A]

// The last element, or none when empty.
// @since unreleased
deque.last(d: Deque[A]) -> Option[A]

// Element at index i from the front, or none when out of range.
// @since unreleased
deque.get(d: Deque[A], i: Int) -> Option[A]

// true if some element == x.
// @since unreleased
deque.contains(d: Deque[A], x: A) -> Bool

// Removes every element of d in place.
// @since unreleased
deque.clear(mut d: Deque[A]) -> Unit

// Deque of f(x), front to back; f runs front to back.
// @since unreleased
deque.map(d: Deque[A], f: (A) -> B) -> Deque[B]

// Elements where f holds, order kept.
// @since unreleased
deque.filter(d: Deque[A], f: (A) -> Bool) -> Deque[A]

// Folds f(acc, x) front to back.
// @since unreleased
deque.fold(d: Deque[A], init: B, f: (B, A) -> B) -> B

// true if f holds for some element; false when empty.
// @since unreleased
deque.any(d: Deque[A], f: (A) -> Bool) -> Bool

// true if f holds for every element; true when empty.
// @since unreleased
deque.all(d: Deque[A], f: (A) -> Bool) -> Bool

// Number of elements where f holds.
// @since unreleased
deque.count(d: Deque[A], f: (A) -> Bool) -> Int

// The element nearest the front where f holds, or none.
// @since unreleased
deque.find(d: Deque[A], f: (A) -> Bool) -> Option[A]

// Elements back to front.
// @since unreleased
deque.reverse(d: Deque[A]) -> Deque[A]

// First n elements, as list.take.
// @since unreleased
deque.take(d: Deque[A], n: Int) -> Deque[A]

// All but the first n, as list.drop.
// @since unreleased
deque.drop(d: Deque[A], n: Int) -> Deque[A]

// Last n elements, as list.take_end.
// @since unreleased
deque.take_end(d: Deque[A], n: Int) -> Deque[A]

// All but the last n, as list.drop_end.
// @since unreleased
deque.drop_end(d: Deque[A], n: Int) -> Deque[A]

// Indices start..<end, as list.slice.
// @since unreleased
deque.slice(d: Deque[A], start: Int, end: Int) -> Deque[A]

// Copy with slot i = x, as list.set.
// @since unreleased
deque.set(d: Deque[A], i: Int, x: A) -> Deque[A]

// Copy with x at i, as list.insert.
// @since unreleased
deque.insert(d: Deque[A], i: Int, x: A) -> Deque[A]

// Copy without slot i, as list.remove_at.
// @since unreleased
deque.remove_at(d: Deque[A], i: Int) -> Deque[A]

// (index, element) pairs, index from 0 at the front.
// @since unreleased
deque.enumerate(d: Deque[A]) -> List[(Int, A)]

// Elements paired with ys by position; stops at the shorter.
// @since unreleased
deque.zip(d: Deque[A], ys: List[B]) -> List[(A, B)]

// f over (element, ys element) pairs; stops at the shorter.
// @since unreleased
deque.zip_with(d: Deque[A], ys: List[B], f: (A, B) -> C) -> List[C]

// The somes f gives, front to back.
// @since unreleased
deque.filter_map(d: Deque[A], f: (A) -> Option[B]) -> List[B]

// The first some f gives, or none; f is not called after the hit.
// @since unreleased
deque.find_map(d: Deque[A], f: (A) -> Option[B]) -> Option[B]

// Left fold seeded by the first element; none when empty.
// @since unreleased
deque.reduce(d: Deque[A], f: (A, A) -> A) -> Option[A]

// Running fold values front to back, init excluded.
// @since unreleased
deque.scan(d: Deque[A], init: B, f: (B, A) -> B) -> List[B]

// (elements where f holds, the rest), order kept.
// @since unreleased
deque.partition(d: Deque[A], f: (A) -> Bool) -> (List[A], List[A])

// Key f(x) -> elements; keys in first-seen order.
// @since unreleased
deque.group_by(d: Deque[A], f: (A) -> B) -> Map[B, List[A]]

// Elements in ascending order; Floats in IEEE totalOrder.
// @since unreleased
deque.sort(d: Deque[A]) -> List[A]

// Elements sorted by key f(x), stable; f runs once per element.
// @since unreleased
deque.sort_by(d: Deque[A], f: (A) -> B) -> List[A]

// Elements sorted by cmp, stable: negative puts a first.
// @since unreleased
deque.sort_with(d: Deque[A], cmp: (A, A) -> Int) -> List[A]

// Least element, or none when empty.
// @since unreleased
deque.min(d: Deque[A]) -> Option[A]

// Greatest element, or none when empty.
// @since unreleased
deque.max(d: Deque[A]) -> Option[A]

// Element with the least key f(x); the nearest the front on ties.
// @since unreleased
deque.min_by(d: Deque[A], f: (A) -> K) -> Option[A]

// Element with the greatest key f(x); the nearest the front on ties.
// @since unreleased
deque.max_by(d: Deque[A], f: (A) -> K) -> Option[A]

// Least element by cmp; the nearest the front on ties.
// @since unreleased
deque.min_with(d: Deque[A], cmp: (A, A) -> Int) -> Option[A]

// Greatest element by cmp; the nearest the front on ties.
// @since unreleased
deque.max_with(d: Deque[A], cmp: (A, A) -> Int) -> Option[A]

// Index from the front of the first x where f holds, or none.
// @since unreleased
deque.find_index(d: Deque[A], f: (A) -> Bool) -> Option[Int]

// Longest front run where f holds.
// @since unreleased
deque.take_while(d: Deque[A], f: (A) -> Bool) -> Deque[A]

// The rest after the longest front run where f holds.
// @since unreleased
deque.drop_while(d: Deque[A], f: (A) -> Bool) -> Deque[A]

// Runs of n from the front, as list.chunk.
// @since unreleased
deque.chunk(d: Deque[A], n: Int) -> List[Deque[A]]

// All n-long runs, as list.window.
// @since unreleased
deque.window(d: Deque[A], n: Int) -> List[Deque[A]]

// (first n, the rest), as list.split_at.
// @since unreleased
deque.split_at(d: Deque[A], n: Int) -> (Deque[A], Deque[A])

// Adjacent equal runs collapsed, as list.dedup.
// @since unreleased
deque.dedup(d: Deque[A]) -> Deque[A]

// sep between adjacent elements.
// @since unreleased
deque.intersperse(d: Deque[A], sep: A) -> Deque[A]

// Duplicates removed; the occurrence nearest the front kept.
// @since unreleased
deque.unique(d: Deque[A]) -> Deque[A]

// The first element per distinct key f(x).
// @since unreleased
deque.unique_by(d: Deque[A], f: (A) -> K) -> Deque[A]

// Sum of the elements; 0 when empty; integers wrap on overflow.
// @since unreleased
deque.sum(d: Deque[A]) -> A

// Product of the elements; 1 when empty; integers wrap on overflow.
// @since unreleased
deque.product(d: Deque[A]) -> A
```

## API 説明の訳（上記署名順）

- `deque.new() -> Deque[A]`：空のキュー。
- `deque.from_list(xs: List[A]) -> Deque[A]`：xs のキュー。先頭から末尾の順です。
- `deque.to_list(d: Deque[A]) -> List[A]`：先頭から末尾の要素。
- `deque.len(d: Deque[A]) -> Int`：要素数。空なら0。
- `deque.is_empty(d: Deque[A]) -> Bool`：d に要素がない場合に限り true。
- `deque.push(mut d: Deque[A], x: A) -> Unit`：末尾に x をその場で追加します。
- `deque.push_front(mut d: Deque[A], x: A) -> Unit`：先頭に x をその場で追加します。
- `deque.pop(mut d: Deque[A]) -> Option[A]`：末尾の要素を削除して返します。空なら none。
- `deque.pop_front(mut d: Deque[A]) -> Option[A]`：先頭の要素を削除して返します。空なら none。
- `deque.first(d: Deque[A]) -> Option[A]`：先頭の要素。空なら none。
- `deque.last(d: Deque[A]) -> Option[A]`：末尾の要素。空なら none。
- `deque.get(d: Deque[A], i: Int) -> Option[A]`：先頭から数えた添字 i の要素。範囲外なら none。
- `deque.contains(d: Deque[A], x: A) -> Bool`：いずれかの要素 == x なら true。
- `deque.clear(mut d: Deque[A]) -> Unit`：d の全要素をその場で削除します。
- `deque.map(d: Deque[A], f: (A) -> B) -> Deque[B]`：先頭から末尾への f(x) のキュー。f も先頭から末尾の順に実行します。
- `deque.filter(d: Deque[A], f: (A) -> Bool) -> Deque[A]`：f が成り立つ要素。順序は保持します。
- `deque.fold(d: Deque[A], init: B, f: (B, A) -> B) -> B`：f(acc, x) を先頭から末尾へ畳み込みます。
- `deque.any(d: Deque[A], f: (A) -> Bool) -> Bool`：いずれかの要素で f が成り立てば true。空なら false。
- `deque.all(d: Deque[A], f: (A) -> Bool) -> Bool`：すべての要素で f が成り立てば true。空なら true。
- `deque.count(d: Deque[A], f: (A) -> Bool) -> Int`：f が成り立つ要素数。
- `deque.find(d: Deque[A], f: (A) -> Bool) -> Option[A]`：f が成り立つ先頭に最も近い要素、または none。
- `deque.reverse(d: Deque[A]) -> Deque[A]`：末尾から先頭への要素。
- `deque.take(d: Deque[A], n: Int) -> Deque[A]`：先頭 n 要素。list.take と同じ。
- `deque.drop(d: Deque[A], n: Int) -> Deque[A]`：先頭 n 要素を除くすべて。list.drop と同じ。
- `deque.take_end(d: Deque[A], n: Int) -> Deque[A]`：末尾 n 要素。list.take_end と同じ。
- `deque.drop_end(d: Deque[A], n: Int) -> Deque[A]`：末尾 n 要素を除くすべて。list.drop_end と同じ。
- `deque.slice(d: Deque[A], start: Int, end: Int) -> Deque[A]`：添字 start..<end。list.slice と同じ。
- `deque.set(d: Deque[A], i: Int, x: A) -> Deque[A]`：位置 i = x としたコピー。list.set と同じ。
- `deque.insert(d: Deque[A], i: Int, x: A) -> Deque[A]`：i に x を挿入したコピー。list.insert と同じ。
- `deque.remove_at(d: Deque[A], i: Int) -> Deque[A]`：位置 i を除いたコピー。list.remove_at と同じ。
- `deque.enumerate(d: Deque[A]) -> List[(Int, A)]`：(添字、要素) の組。添字は先頭の0から。
- `deque.zip(d: Deque[A], ys: List[B]) -> List[(A, B)]`：位置で ys と組にします。短い方で終了します。
- `deque.zip_with(d: Deque[A], ys: List[B], f: (A, B) -> C) -> List[C]`：(要素、ys の要素) の組に f を適用。短い方で終了します。
- `deque.filter_map(d: Deque[A], f: (A) -> Option[B]) -> List[B]`：f が返す some を先頭から末尾へ並べます。
- `deque.find_map(d: Deque[A], f: (A) -> Option[B]) -> Option[B]`：f が返す最初の some、または none。一致後は f を呼びません。
- `deque.reduce(d: Deque[A], f: (A, A) -> A) -> Option[A]`：先頭の要素を初期値とした左畳み込み。空なら none。
- `deque.scan(d: Deque[A], init: B, f: (B, A) -> B) -> List[B]`：先頭から末尾への逐次畳み込み値。init は含みません。
- `deque.partition(d: Deque[A], f: (A) -> Bool) -> (List[A], List[A])`：(f が成り立つ要素、それ以外)。順序は保持します。
- `deque.group_by(d: Deque[A], f: (A) -> B) -> Map[B, List[A]]`：キー f(x) → 要素群。キーは初出順。
- `deque.sort(d: Deque[A]) -> List[A]`：昇順の要素。Float は IEEE totalOrder。
- `deque.sort_by(d: Deque[A], f: (A) -> B) -> List[A]`：キー f(x) による安定ソート。f は要素ごとに1回実行。
- `deque.sort_with(d: Deque[A], cmp: (A, A) -> Int) -> List[A]`：cmp による安定ソート。負なら a を先に置きます。
- `deque.min(d: Deque[A]) -> Option[A]`：最小要素。空なら none。
- `deque.max(d: Deque[A]) -> Option[A]`：最大要素。空なら none。
- `deque.min_by(d: Deque[A], f: (A) -> K) -> Option[A]`：キー f(x) が最小の要素。同点なら先頭に最も近いもの。
- `deque.max_by(d: Deque[A], f: (A) -> K) -> Option[A]`：キー f(x) が最大の要素。同点なら先頭に最も近いもの。
- `deque.min_with(d: Deque[A], cmp: (A, A) -> Int) -> Option[A]`：cmp による最小要素。同点なら先頭に最も近いもの。
- `deque.max_with(d: Deque[A], cmp: (A, A) -> Int) -> Option[A]`：cmp による最大要素。同点なら先頭に最も近いもの。
- `deque.find_index(d: Deque[A], f: (A) -> Bool) -> Option[Int]`：f が成り立つ最初の x の先頭からの添字、または none。
- `deque.take_while(d: Deque[A], f: (A) -> Bool) -> Deque[A]`：f が成り立つ先頭からの最長連続部分。
- `deque.drop_while(d: Deque[A], f: (A) -> Bool) -> Deque[A]`：f が成り立つ先頭からの最長連続部分を除いた残り。
- `deque.chunk(d: Deque[A], n: Int) -> List[Deque[A]]`：先頭から n 個ずつの連続部分。list.chunk と同じ。
- `deque.window(d: Deque[A], n: Int) -> List[Deque[A]]`：長さ n のすべての連続部分。list.window と同じ。
- `deque.split_at(d: Deque[A], n: Int) -> (Deque[A], Deque[A])`：(先頭 n 個、残り)。list.split_at と同じ。
- `deque.dedup(d: Deque[A]) -> Deque[A]`：隣接する同値の連続を1つにまとめます。list.dedup と同じ。
- `deque.intersperse(d: Deque[A], sep: A) -> Deque[A]`：隣接要素間に sep。
- `deque.unique(d: Deque[A]) -> Deque[A]`：重複を削除し、先頭に最も近い出現を保持します。
- `deque.unique_by(d: Deque[A], f: (A) -> K) -> Deque[A]`：異なるキー f(x) ごとの最初の要素。
- `deque.sum(d: Deque[A]) -> A`：要素の和。空なら0。整数はオーバーフロー時に折り返します。
- `deque.product(d: Deque[A]) -> A`：要素の積。空なら1。整数はオーバーフロー時に折り返します。
