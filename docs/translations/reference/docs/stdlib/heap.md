# heap

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](heap.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](heap.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](heap.zh-CN.md)

> This is a non-normative reader copy of the [pinned original](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/heap.md), not a new authority, generator output or automatically synchronized document. Original sources and machine consumers are unchanged. Historical claims and examples have not been independently revalidated here.


A priority queue, `Heap[A]`: the least element comes out first. Auto-imported.
Pure — identical on both targets.

The order is set by the empty constructor: `heap.new()` uses the natural order
(the one `list.sort` uses), `heap.new_by(f)` orders by the key `f(x)`, and
`heap.new_with(cmp)` by a comparator (negative puts `a` first). `from_list`
uses the natural order. Equal elements come out in the order they went in.
`push`, `pop` and `clear` change the heap in place (`mut`); `pop` and `peek` on
an empty heap give `none`. `to_list` is the pop order.

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

The greatest first, and a key with ties broken by push order:

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



## Signature index (16 functions)

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


