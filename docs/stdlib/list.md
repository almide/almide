# list

List operations. auto-imported.

### `list.len(xs: List[A]) -> Int`

Return the number of elements in a list.

```almd run
fn main() -> Unit = {
  println("${list.len([1, 2, 3])}")
}
```
```output
3
```

### `list.get(xs: List[A], i: Int) -> Option[A]`

Get the element at index i, or none if out of bounds.

```almd run
fn main() -> Unit = {
  println("${list.get([10, 20, 30], 1)}")
}
```
```output
some(20)
```

### `list.get_or(xs: List[A], i: Int, default: A) -> A`

Get the element at index i, or return a default value.

```almd run
fn main() -> Unit = {
  println("${list.get_or([1, 2], 5, 0)}")
}
```
```output
0
```

### `list.set(xs: List[A], i: Int, val: A) -> List[A]`

Return a new list with the element at index i replaced.

```almd run
fn main() -> Unit = {
  println("${list.set([1, 2, 3], 1, 99)}")
}
```
```output
[1, 99, 3]
```

### `list.swap(xs: List[A], i: Int, j: Int) -> List[A]`

Return a new list with elements at indices i and j swapped.

```almd run
fn main() -> Unit = {
  println("${list.swap([1, 2, 3], 0, 2)}")
}
```
```output
[3, 2, 1]
```

### `list.sort(xs: List[A]) -> List[A]`

Sort a list in ascending order.

```almd run
fn main() -> Unit = {
  println("${list.sort([3, 1, 2])}")
}
```
```output
[1, 2, 3]
```

### `list.reverse(xs: List[A]) -> List[A]`

Reverse the order of elements.

```almd run
fn main() -> Unit = {
  println("${list.reverse([1, 2, 3])}")
}
```
```output
[3, 2, 1]
```

### `list.contains(xs: List[A], x: A) -> Bool`

Check if a list contains an element.

```almd run
fn main() -> Unit = {
  println("${list.contains([1, 2, 3], 2)}")
}
```
```output
true
```

### `list.enumerate(xs: List[A]) -> List[(Int, A)]`

Pair each element with its index.

```almd run
fn main() -> Unit = {
  println("${list.enumerate(["a", "b"])}")
}
```
```output
[(0, "a"), (1, "b")]
```

### `list.zip(xs: List[A], ys: List[B]) -> List[(A, B)]`

Combine two lists into a list of pairs.

```almd run
fn main() -> Unit = {
  println("${list.zip([1, 2], ["a", "b"])}")
}
```
```output
[(1, "a"), (2, "b")]
```

### `list.flatten(xss: List[List[T]]) -> List[T]`

Flatten a list of lists into a single list.

```almd run
fn main() -> Unit = {
  println("${list.flatten([[1, 2], [3]])}")
}
```
```output
[1, 2, 3]
```

### `list.take(xs: List[A], n: Int) -> List[A]`

Take the first n elements.

```almd run
fn main() -> Unit = {
  println("${list.take([1, 2, 3, 4], 2)}")
}
```
```output
[1, 2]
```

### `list.drop(xs: List[A], n: Int) -> List[A]`

Drop the first n elements.

```almd run
fn main() -> Unit = {
  println("${list.drop([1, 2, 3, 4], 2)}")
}
```
```output
[3, 4]
```

### `list.unique(xs: List[A]) -> List[A]`

Remove duplicate elements, preserving first occurrence.

```almd run
fn main() -> Unit = {
  println("${list.unique([1, 2, 1, 3])}")
}
```
```output
[1, 2, 3]
```

### `list.index_of(xs: List[A], x: A) -> Option[Int]`

Find the first index of an element, or none.

```almd run
fn main() -> Unit = {
  println("${list.index_of([10, 20, 30], 20)}")
}
```
```output
some(1)
```

### `list.last(xs: List[A]) -> Option[A]`

Get the last element, or none if empty.

```almd run
fn main() -> Unit = {
  println("${list.last([1, 2, 3])}")
}
```
```output
some(3)
```

### `list.chunk(xs: List[A], n: Int) -> List[List[A]]`

Split a list into chunks of size n.

```almd run
fn main() -> Unit = {
  println("${list.chunk([1, 2, 3, 4, 5], 2)}")
}
```
```output
[[1, 2], [3, 4], [5]]
```

### `list.sum(xs: List[Int]) -> Int`

Sum all integers in a list.

```almd run
fn main() -> Unit = {
  println("${list.sum([1, 2, 3])}")
}
```
```output
6
```

### `list.product(xs: List[Int]) -> Int`

Multiply all integers in a list.

```almd run
fn main() -> Unit = {
  println("${list.product([2, 3, 4])}")
}
```
```output
24
```

### `list.first(xs: List[A]) -> Option[A]`

Get the first element, or none if empty.

```almd run
fn main() -> Unit = {
  println("${list.first([1, 2, 3])}")
}
```
```output
some(1)
```

### `list.is_empty(xs: List[A]) -> Bool`

Check if a list is empty.

```almd run
fn main() -> Unit = {
  println("${list.is_empty([]: List[Int])}")
}
```
```output
true
```

### `list.min(xs: List[A]) -> Option[A]`

Find the minimum element, or none if empty.

```almd run
fn main() -> Unit = {
  println("${list.min([3, 1, 2])}")
}
```
```output
some(1)
```

### `list.max(xs: List[A]) -> Option[A]`

Find the maximum element, or none if empty.

```almd run
fn main() -> Unit = {
  println("${list.max([3, 1, 2])}")
}
```
```output
some(3)
```

### `list.join(xs: List[String], sep: String) -> String`

Join a list of strings with a separator.

```almd run
fn main() -> Unit = {
  println("${list.join(["a", "b", "c"], "-")}")
}
```
```output
a-b-c
```

### `list.map(xs: List[A], f: Fn[A] -> B) -> List[B]`

Apply a function to each element, returning a new list.

```almd run
fn main() -> Unit = {
  println("${[1, 2, 3].map((x) => x * 2)}")
}
```
```output
[2, 4, 6]
```

### `list.filter(xs: List[A], f: Fn[A] -> Bool) -> List[A]`

Keep elements that satisfy a predicate.

```almd run
fn main() -> Unit = {
  println("${[1, 2, 3, 4].filter((x) => x > 2)}")
}
```
```output
[3, 4]
```

### `list.find(xs: List[A], f: Fn[A] -> Bool) -> Option[A]`

Find the first element matching a predicate.

```almd run
fn main() -> Unit = {
  println("${[1, 2, 3].find((x) => x > 1)}")
}
```
```output
some(2)
```

### `list.any(xs: List[A], f: Fn[A] -> Bool) -> Bool`

Check if any element satisfies a predicate.

```almd run
fn main() -> Unit = {
  println("${[1, 2, 3].any((x) => x > 2)}")
}
```
```output
true
```

### `list.all(xs: List[A], f: Fn[A] -> Bool) -> Bool`

Check if all elements satisfy a predicate.

```almd run
fn main() -> Unit = {
  println("${[2, 4, 6].all((x) => x % 2 == 0)}")
}
```
```output
true
```

### `list.sort_by(xs: List[A], f: Fn[A] -> B) -> List[A]`

Sort by a key-extraction function (not a comparator). f extracts the sort key from each element.

```almd run
fn main() -> Unit = {
  println("${["bb", "a", "ccc"].sort_by((s) => string.len(s))}")
}
```
```output
["a", "bb", "ccc"]
```

### `list.flat_map(xs: List[A], f: Fn[A] -> List[B]) -> List[B]`

Map each element to a list and flatten the results.

```almd run
fn main() -> Unit = {
  println("${[1, 2].flat_map((x) => [x, x * 10])}")
}
```
```output
[1, 10, 2, 20]
```

### `list.filter_map(xs: List[A], f: Fn[A] -> Option[B]) -> List[B]`

Map and filter in one pass: keep only some values.

```almd run
fn main() -> Unit = {
  println("${["1", "x", "3"].filter_map((s) => int.parse(s)?)}")
}
```
```output
[1, 3]
```

### `list.take_while(xs: List[A], f: Fn[A] -> Bool) -> List[A]`

Take elements from the front while a predicate holds.

```almd run
fn main() -> Unit = {
  println("${[1, 2, 3, 1].take_while((x) => x < 3)}")
}
```
```output
[1, 2]
```

### `list.drop_while(xs: List[A], f: Fn[A] -> Bool) -> List[A]`

Drop elements from the front while a predicate holds.

```almd run
fn main() -> Unit = {
  println("${[1, 2, 3, 1].drop_while((x) => x < 3)}")
}
```
```output
[3, 1]
```

### `list.count(xs: List[A], f: Fn[A] -> Bool) -> Int`

Count elements that satisfy a predicate.

```almd run
fn main() -> Unit = {
  println("${[1, 2, 3, 4].count((x) => x > 2)}")
}
```
```output
2
```

### `list.partition(xs: List[A], f: Fn[A] -> Bool) -> (List[A], List[A])`

Split a list into two: elements matching and not matching a predicate.

```almd run
fn main() -> Unit = {
  let (evens, odds) = [1, 2, 3, 4].partition((x) => x % 2 == 0)
  println("${evens}")
  println("${odds}")
}
```
```output
[2, 4]
[1, 3]
```

### `list.reduce(xs: List[A], f: Fn[A, A] -> A) -> Option[A]`

Reduce a list by combining elements pairwise. Returns none if empty.

```almd run
fn main() -> Unit = {
  println("${[1, 2, 3].reduce((a, b) => a + b)}")
}
```
```output
some(6)
```

### `list.group_by(xs: List[A], f: Fn[A] -> B) -> Map[B, List[A]]`

Group elements by a key function into a map.

```almd run
fn main() -> Unit = {
  let groups = ["hi", "hey", "bye"].group_by((s) => string.take(s, 1))
  println("${map.get(groups, "h")}")
  println("${map.get(groups, "b")}")
}
```
```output
some(["hi", "hey"])
some(["bye"])
```

### `list.range(start: Int, end: Int) -> List[Int]`

Create a list of integers from start (inclusive) to end (exclusive).

```almd run
fn main() -> Unit = {
  println("${list.range(1, 5)}")
}
```
```output
[1, 2, 3, 4]
```

### `list.slice(xs: List[A], start: Int, end: Int) -> List[A]`

Extract a sublist from start to end index.

```almd run
fn main() -> Unit = {
  println("${list.slice([1, 2, 3, 4, 5], 1, 4)}")
}
```
```output
[2, 3, 4]
```

### `list.insert(xs: List[A], i: Int, val: A) -> List[A]`

Insert an element at index i, shifting elements right.

```almd run
fn main() -> Unit = {
  println("${list.insert([1, 3], 1, 2)}")
}
```
```output
[1, 2, 3]
```

### `list.remove_at(xs: List[A], i: Int) -> List[A]`

Remove the element at index i.

```almd run
fn main() -> Unit = {
  println("${list.remove_at([1, 2, 3], 1)}")
}
```
```output
[1, 3]
```

### `list.find_index(xs: List[A], f: Fn[A] -> Bool) -> Option[Int]`

Find the first index where a predicate holds.

```almd run
fn main() -> Unit = {
  println("${[10, 20, 30].find_index((x) => x > 15)}")
}
```
```output
some(1)
```

### `list.update(xs: List[A], i: Int, f: Fn[A] -> A) -> List[A]`

Return a new list with the element at index i transformed by f.

```almd run
fn main() -> Unit = {
  println("${list.update([1, 2, 3], 1, (x) => x * 10)}")
}
```
```output
[1, 20, 3]
```

### `list.repeat(val: A, n: Int) -> List[A]`

Create a list with a value repeated n times.

```almd run
fn main() -> Unit = {
  println("${list.repeat(0, 3)}")
}
```
```output
[0, 0, 0]
```

### `list.scan(xs: List[A], init: B, f: Fn[B, A] -> B) -> List[B]`

Like fold, but returns all intermediate accumulator values.

```almd run
fn main() -> Unit = {
  println("${[1, 2, 3].scan(0, (acc, x) => acc + x)}")
}
```
```output
[1, 3, 6]
```

### `list.intersperse(xs: List[A], sep: A) -> List[A]`

Insert a separator between each element.

```almd run
fn main() -> Unit = {
  println("${list.intersperse([1, 2, 3], 0)}")
}
```
```output
[1, 0, 2, 0, 3]
```

### `list.windows(xs: List[A], n: Int) -> List[List[A]]`

Return sliding windows of size n.

```almd run
fn main() -> Unit = {
  println("${list.windows([1, 2, 3, 4], 2)}")
}
```
```output
[[1, 2], [2, 3], [3, 4]]
```

### `list.dedup(xs: List[A]) -> List[A]`

Remove consecutive duplicates.

```almd
list.dedup([1, 1, 2, 2, 1]) // => [1, 2, 1]
```

Executable pin of the ADJACENT-duplicates rule (dedup is not distinct —
the trailing `1` survives), gated by `scripts/check-doc-fences.sh`:

```almd run
fn main() -> Unit = {
  let xs = list.dedup([1, 1, 2, 2, 1])
  println(xs |> list.map((x) => int.to_string(x)) |> list.join(","))
  println(int.to_string(list.len(xs)))
}
```
```output
1,2,1
3
```

### `list.zip_with(xs: List[A], ys: List[B], f: Fn[A, B] -> C) -> List[C]`

Combine two lists element-wise using a function.

```almd run
fn main() -> Unit = {
  println("${list.zip_with([1, 2], [10, 20], (a, b) => a + b)}")
}
```
```output
[11, 22]
```

### `list.fold(xs: List[A], init: B, f: Fn[B, A] -> B) -> B`

Reduce a list from left with an initial accumulator.

```almd run
fn main() -> Unit = {
  println("${[1, 2, 3].fold(0, (acc, x) => acc + x)}")
}
```
```output
6
```

### `list.take_end(xs: List[A], n: Int) -> List[A]`

Take the last N elements.

```almd run
fn main() -> Unit = {
  println("${list.take_end([1, 2, 3, 4], 2)}")
}
```
```output
[3, 4]
```

### `list.drop_end(xs: List[A], n: Int) -> List[A]`

Drop the last N elements.

```almd run
fn main() -> Unit = {
  println("${list.drop_end([1, 2, 3, 4], 2)}")
}
```
```output
[1, 2]
```

### `list.unique_by(xs: List[A], f: Fn[A] -> K) -> List[A]`

Remove duplicates by key function, preserving first occurrence.

```almd check
fn main() -> Unit = {
  let firsts = list.unique_by(["aa", "ab", "ba"], (s) => string.get(s, 0))
  println("${firsts}")
}
```

### `list.shuffle(xs: List[A]) -> List[A]`

Return a randomly shuffled copy of the list.

```almd run
fn main() -> Unit = {
  let shuffled = list.shuffle([1, 2, 3, 4])
  println("${list.len(shuffled)}")
  println("${list.sort(shuffled)}")
}
```
```output
4
[1, 2, 3, 4]
```

### `list.window(xs: List[A], n: Int) -> List[List[A]]`

Sliding window of size N over the list.

```almd run
fn main() -> Unit = {
  println("${list.window([1, 2, 3, 4], 2)}")
}
```
```output
[[1, 2], [2, 3], [3, 4]]
```

### `list.push(xs: List[A], x: A) -> Unit`

Append an element in place. Requires var binding.

```almd run
fn main() -> Unit = {
  var xs = [1, 2, 3]
  list.push(xs, 42)
  println("${xs}")
}
```
```output
[1, 2, 3, 42]
```

### `list.pop(xs: List[A]) -> Option[A]`

Remove and discard the last element in place. Requires var binding.

```almd run
fn main() -> Unit = {
  var xs = [1, 2, 3]
  let popped = list.pop(xs)
  println("${popped}")
  println("${xs}")
}
```
```output
some(3)
[1, 2]
```

### `list.clear(xs: List[A]) -> Unit`

Remove all elements in place. Requires var binding.

```almd run
fn main() -> Unit = {
  var xs = [1, 2, 3]
  list.clear(xs)
  println("${xs}")
  println("${list.is_empty(xs)}")
}
```
```output
[]
true
```


## Fallible pipelines — the polymorphic core (ADR-0006)

A callback whose body ends in `!` (or a named `-> T!` fn) instantiates the
core combinator's FALLIBLE form: the call yields `Result[_, String]` and
**short-circuits on the first err** (elements after it are never evaluated).
One name per combinator — the strategy rides the marker:

```almd
effect fn read_all(files: List[String]) -> List[Entry] =
  files |> list.map((f) => read_meta(f)!)!
```

Covers `map` / `filter` / `flat_map` / `filter_map` / `fold` / `find` /
`each`. The removed `list.try_*` twins (one deprecation window at v0.55.0,
gone since v0.56.0) rewrite mechanically — E043 carries the exact form:

```almd
list.try_map(xs, f)      →  list.map(xs, (x) => f(x)!)!
list.try_fold(xs, z, f)  →  list.fold(xs, z, (a, x) => f(a, x)!)!
```

Deliberate omissions unchanged: an erring predicate query is the find-form's
domain (`any`/`all`/`count` never had twins), and `sort_by` has no meaningful
order under an erring key extractor. The end state (empty public surface, the
seven `__fallible_*` internal carriers present and un-nameable from source) is
machine-checked by `tests/list_fallible_family_gate_test.rs`.

When a callback that never errs leaves `E` unconstrained, annotate the result:
`let evens: Result[List[Int], String] = list.filter(xs, (n) => ok(n % 2 == 0)!)`.

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (66 functions)

```
list.len(xs: List[A]) -> Int
list.length(xs: List[A]) -> Int
list.get(xs: List[A], i: Int) -> Option[A]
list.get_or(xs: List[A], i: Int, default: A) -> A
list.set(xs: List[A], i: Int, val: A) -> List[A]
list.swap(xs: List[A], i: Int, j: Int) -> List[A]
list.sort(xs: List[A]) -> List[A]
list.reverse(xs: List[A]) -> List[A]
list.contains(xs: List[A], x: A) -> Bool
list.enumerate(xs: List[A]) -> List[()]
list.zip(xs: List[A], ys: List[B]) -> List[()]
list.flatten(xss: List[List[T]]) -> List[T]
list.take(xs: List[A], n: Int) -> List[A]
list.drop(xs: List[A], n: Int) -> List[A]
list.tail(xs: List[A]) -> List[A]
list.unique(xs: List[A]) -> List[A]
list.index_of(xs: List[A], x: A) -> Option[Int]
list.last(xs: List[A]) -> Option[A]
list.chunk(xs: List[A], n: Int) -> List[List[A]]
list.sum(xs: List[Int]) -> Int
list.product(xs: List[Int]) -> Int
list.first(xs: List[A]) -> Option[A]
list.is_empty(xs: List[A]) -> Bool
list.min(xs: List[A]) -> Option[A]
list.max(xs: List[A]) -> Option[A]
list.join(xs: List[String], sep: String) -> String
list.map(xs: List[A], f: (A) -> B) -> List[B]
list.filter(xs: List[A], f: (A) -> Bool) -> List[A]
list.find(xs: List[A], f: (A) -> Bool) -> Option[A]
list.any(xs: List[A], f: (A) -> Bool) -> Bool
list.all(xs: List[A], f: (A) -> Bool) -> Bool
list.count(xs: List[A], f: (A) -> Bool) -> Int
list.flat_map(xs: List[A], f: (A) -> List[B]) -> List[B]
list.filter_map(xs: List[A], f: (A) -> Option[B]) -> List[B]
list.fold(xs: List[A], init: B, f: (B, A) -> B) -> B
list.sort_by(xs: List[A], f: (A) -> B) -> List[A]
list.take_while(xs: List[A], f: (A) -> Bool) -> List[A]
list.drop_while(xs: List[A], f: (A) -> Bool) -> List[A]
list.partition(xs: List[A], f: (A) -> Bool) -> ()
list.reduce(xs: List[A], f: (A, A) -> A) -> Option[A]
list.group_by(xs: List[A], f: (A) -> B) -> Map[B, List[A]]
list.find_index(xs: List[A], f: (A) -> Bool) -> Option[Int]
list.update(xs: List[A], i: Int, f: (A) -> A) -> List[A]
list.scan(xs: List[A], init: B, f: (B, A) -> B) -> List[B]
list.zip_with(xs: List[A], ys: List[B], f: (A, B) -> C) -> List[C]
list.unique_by(xs: List[A], f: (A) -> K) -> List[A]
list.range(start: Int, end: Int) -> List[Int]
list.slice(xs: List[A], start: Int, end: Int) -> List[A]
list.insert(xs: List[A], i: Int, val: A) -> List[A]
list.remove_at(xs: List[A], i: Int) -> List[A]
list.repeat(val: A, n: Int) -> List[A]
list.intersperse(xs: List[A], sep: A) -> List[A]
list.windows(xs: List[A], n: Int) -> List[List[A]]
list.dedup(xs: List[A]) -> List[A]
list.take_end(xs: List[A], n: Int) -> List[A]
list.drop_end(xs: List[A], n: Int) -> List[A]
list.shuffle(xs: List[A]) -> List[A]
list.window(xs: List[A], n: Int) -> List[List[A]]
list.binary_search(xs: List[Int], target: Int) -> Option[Int]
list.push(xs: List[A], x: A) -> Unit
list.with_capacity(cap: Int) -> List[A]
list.pop(xs: List[A]) -> Option[A]
list.clear(xs: List[A]) -> Unit
list.bundled_probe(n: Int) -> Int
list.split_at(xs: List[T], n: Int) -> ()
list.iterate(seed: T, f: (T) -> T, n: Int) -> List[T]
```

<!-- END GENERATED SIGNATURE INDEX -->
