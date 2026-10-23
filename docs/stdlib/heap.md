# heap

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

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

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

## Type index (1 types)

```
// A priority queue; the least element by its order comes out first.
// @since unreleased
type heap.Heap = { slots: List[(A, Int)], next: Int, cmp: (A, A) -> Int }
```

<!-- END GENERATED SIGNATURE INDEX -->
