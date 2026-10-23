# deque

A double-ended queue, `Deque[A]`: add and take at both ends in amortized O(1).
Auto-imported. Pure — identical on both targets.

`push` / `pop` work at the back, like `list.push` / `list.pop`, and
`push_front` / `pop_front` at the front. All four change the deque in place
(`mut`); everything else returns a new value. Taking from an empty deque gives
`none`. Positional and range functions (`get`, `take`, `slice`, `set`, …) take
the same arguments and give the same answers as the `list` function of the same
name, and the sequence algorithms (`map`, `fold`, `sort_by`, `min_by`, …) walk
the deque front to back.

```almd run
import deque

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

A breadth-first search keeps its frontier in a deque:

```almd run
import deque

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

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (58 functions)

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
```

## Type index (1 types)

```
// A double-ended queue, front to back.
// @since unreleased
type deque.Deque = { front: List[A], back: List[A] }
```

<!-- END GENERATED SIGNATURE INDEX -->
