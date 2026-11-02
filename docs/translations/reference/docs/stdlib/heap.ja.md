# heap

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](heap.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](heap.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](heap.zh-CN.md)



> 原文：[heap.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/heap.md)。非規範的な読者向け固定スナップショットです。自動生成・自動同期されません。署名・コード例・API 索引のコメントは原文を保持し、説明の訳を末尾に併記しています。計測・動作・例をこの翻訳作業で独立に再検証していません。

優先度付きキュー `Heap[A]`。最小要素から取り出します。自動 import されます。pure で、両ターゲットで同じ動作です。

順序は空のコンストラクタで決まります。`heap.new()` は自然順（`list.sort` と同じ順）、`heap.new_by(f)` はキー `f(x)` 順、`heap.new_with(cmp)` は比較関数順（負なら `a` が先）です。`from_list` は自然順です。等しい要素は入れた順に出てきます。`push`、`pop`、`clear` はヒープをその場で変更します（`mut`）。空のヒープの `pop` と `peek` は `none` を返します。`to_list` は pop 順です。

```almd run
fn main() -> Unit = {
  var h: Heap[Int] = heap.new()
  for x in [5, 1, 4] {
    heap.push(h, x)
  }
  println("${heap.pop(h)} ${heap.peek(h)} ${heap.to_list(h)}")
}
```
```output
some(1) some(4) [4, 5]
```

最大要素から取り出す例と、同じキーなら push 順で決める例：

```almd run
fn main() -> Unit = {
  var top = heap.new_with((a: Int, b: Int) => if a > b then -1 else if a < b then 1 else 0)
  for x in [3, 9, 1] {
    heap.push(top, x)
  }
  println("${heap.to_list(top)}")
  var jobs = heap.new_by((j: (Int, String)) => j.0)
  heap.push(jobs, (2, "write"))
  heap.push(jobs, (1, "read"))
  heap.push(jobs, (2, "send"))
  println("${heap.to_list(jobs) |> list.map((j) => j.1)}")
}
```
```output
[9, 3, 1]
["read", "write", "send"]
```



<a id="signature-index-16-functions"></a>

## 固定スナップショットの API 索引 (16 関数)

```
// An empty heap; the least element comes out first.
// @since unreleased
heap.new() -> Heap[A]

// An empty heap ordered by key f(x); the least key comes out first.
// @since unreleased
heap.new_by(f: (A) -> K) -> Heap[A]

// An empty heap ordered by cmp; negative puts a first.
// @since unreleased
heap.new_with(cmp: (A, A) -> Int) -> Heap[A]

// A heap of xs in the natural order; equal elements leave in list order.
// @since unreleased
heap.from_list(xs: List[A]) -> Heap[A]

// Adds x in place.
// @since unreleased
heap.push(mut h: Heap[A], x: A) -> Unit

// Removes and yields the first element out; none when empty.
// @since unreleased
heap.pop(mut h: Heap[A]) -> Option[A]

// Removes every element of h in place; the order is kept.
// @since unreleased
heap.clear(mut h: Heap[A]) -> Unit

// The element pop would yield next, or none when empty.
// @since unreleased
heap.peek(h: Heap[A]) -> Option[A]

// Element count; 0 for an empty heap.
// @since unreleased
heap.len(h: Heap[A]) -> Int

// true iff h has no elements.
// @since unreleased
heap.is_empty(h: Heap[A]) -> Bool

// The elements in pop order.
// @since unreleased
heap.to_list(h: Heap[A]) -> List[A]

// true if some element == x.
// @since unreleased
heap.contains(h: Heap[A], x: A) -> Bool

// true if f holds for some element; false when empty.
// @since unreleased
heap.any(h: Heap[A], f: (A) -> Bool) -> Bool

// true if f holds for every element; true when empty.
// @since unreleased
heap.all(h: Heap[A], f: (A) -> Bool) -> Bool

// Number of elements where f holds.
// @since unreleased
heap.count(h: Heap[A], f: (A) -> Bool) -> Int

// The elements where f holds, in a heap of the same order; equal elements
// keep the order they went in.
// @since unreleased
heap.filter(h: Heap[A], f: (A) -> Bool) -> Heap[A]
```



## API 説明の訳（上記署名順）

- `heap.new() -> Heap[A]` — 空のヒープ。最小要素から取り出します。
- `heap.new_by(f: (A) -> K) -> Heap[A]` — キー f(x) 順の空のヒープ。最小キーから取り出します。
- `heap.new_with(cmp: (A, A) -> Int) -> Heap[A]` — cmp 順の空のヒープ。負なら a が先。
- `heap.from_list(xs: List[A]) -> Heap[A]` — xs から自然順のヒープを作ります。等しい要素はリスト順に出ます。
- `heap.push(mut h: Heap[A], x: A) -> Unit` — x をその場で追加します。
- `heap.pop(mut h: Heap[A]) -> Option[A]` — 次に取り出す要素を削除して返します。空なら none。
- `heap.clear(mut h: Heap[A]) -> Unit` — h の全要素をその場で削除します。順序規則は保持します。
- `heap.peek(h: Heap[A]) -> Option[A]` — pop が次に返す要素。空なら none。
- `heap.len(h: Heap[A]) -> Int` — 要素数。空のヒープでは 0。
- `heap.is_empty(h: Heap[A]) -> Bool` — h に要素がない場合に限り true。
- `heap.to_list(h: Heap[A]) -> List[A]` — pop 順の要素。
- `heap.contains(h: Heap[A], x: A) -> Bool` — いずれかの要素が == x なら true。
- `heap.any(h: Heap[A], f: (A) -> Bool) -> Bool` — f が成立する要素があれば true。空なら false。
- `heap.all(h: Heap[A], f: (A) -> Bool) -> Bool` — 全要素で f が成立すれば true。空なら true。
- `heap.count(h: Heap[A], f: (A) -> Bool) -> Int` — f が成立する要素の数。
- `heap.filter(h: Heap[A], f: (A) -> Bool) -> Heap[A]` — f が成立する要素を同じ順序規則のヒープにします。等しい要素は入った順を保持します。
