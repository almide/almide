# heap

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](heap.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](heap.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](heap.zh-CN.md)



> 原文：[heap.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/heap.md)。这是供读者参考的非规范性固定快照，不参与自动生成或自动同步。签名、代码示例和 API 索引注释保持原文，说明译文附于文末。本次翻译未独立复核测量、行为或示例。

优先队列 `Heap[A]`：最小元素先出。自动导入。pure，两个目标行为相同。

顺序由空构造器设置：`heap.new()` 使用自然顺序（`list.sort` 使用的顺序），`heap.new_by(f)` 按键 `f(x)` 排序，`heap.new_with(cmp)` 按比较器排序（负值让 `a` 在前）。`from_list` 使用自然顺序。相等元素按进入顺序取出。`push`、`pop` 和 `clear` 原地修改堆（`mut`）；空堆的 `pop` 和 `peek` 返回 `none`。`to_list` 使用 pop 顺序。

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

最大元素先出，以及键相等时按 push 顺序决定的例子：

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

## 固定快照 API 索引 (16 个函数)

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



## API 说明译文（按上述签名顺序）

- `heap.new() -> Heap[A]` — 空堆；最小元素先出。
- `heap.new_by(f: (A) -> K) -> Heap[A]` — 按键 f(x) 排序的空堆；最小键先出。
- `heap.new_with(cmp: (A, A) -> Int) -> Heap[A]` — 按 cmp 排序的空堆；负值让 a 在前。
- `heap.from_list(xs: List[A]) -> Heap[A]` — 由 xs 构成的自然顺序堆；相等元素按列表顺序取出。
- `heap.push(mut h: Heap[A], x: A) -> Unit` — 原地添加 x。
- `heap.pop(mut h: Heap[A]) -> Option[A]` — 移除并返回下一个出堆元素；空时为 none。
- `heap.clear(mut h: Heap[A]) -> Unit` — 原地移除 h 的所有元素；保留排序规则。
- `heap.peek(h: Heap[A]) -> Option[A]` — pop 将返回的下一个元素；空时为 none。
- `heap.len(h: Heap[A]) -> Int` — 元素数量；空堆为 0。
- `heap.is_empty(h: Heap[A]) -> Bool` — 当且仅当 h 没有元素时为 true。
- `heap.to_list(h: Heap[A]) -> List[A]` — 按 pop 顺序排列的元素。
- `heap.contains(h: Heap[A], x: A) -> Bool` — 有元素 == x 时为 true。
- `heap.any(h: Heap[A], f: (A) -> Bool) -> Bool` — 某个元素的 f 成立时为 true；空时为 false。
- `heap.all(h: Heap[A], f: (A) -> Bool) -> Bool` — 每个元素的 f 成立时为 true；空时为 true。
- `heap.count(h: Heap[A], f: (A) -> Bool) -> Int` — f 成立的元素数量。
- `heap.filter(h: Heap[A], f: (A) -> Bool) -> Heap[A]` — f 成立的元素组成排序规则相同的堆；相等元素保持进入顺序。
