# list

List operations. auto-imported.

### `list.len(xs: List[A]) -> Int`

Return the number of elements in a list.

```almd
list.len([1, 2, 3]) // => 3
```

### `list.get(xs: List[A], i: Int) -> Option[A]`

Get the element at index i, or none if out of bounds.

```almd
list.get([10, 20, 30], 1) // => some(20)
```

### `list.get_or(xs: List[A], i: Int, default: A) -> A`

Get the element at index i, or return a default value.

```almd
list.get_or([1, 2], 5, 0) // => 0
```

### `list.set(xs: List[A], i: Int, val: A) -> List[A]`

Return a new list with the element at index i replaced.

```almd
list.set([1, 2, 3], 1, 99) // => [1, 99, 3]
```

### `list.swap(xs: List[A], i: Int, j: Int) -> List[A]`

Return a new list with elements at indices i and j swapped.

```almd
list.swap([1, 2, 3], 0, 2) // => [3, 2, 1]
```

### `list.sort(xs: List[A]) -> List[A]`

Sort a list in ascending order.

```almd
list.sort([3, 1, 2]) // => [1, 2, 3]
```

### `list.reverse(xs: List[A]) -> List[A]`

Reverse the order of elements.

```almd
list.reverse([1, 2, 3]) // => [3, 2, 1]
```

### `list.contains(xs: List[A], x: A) -> Bool`

Check if a list contains an element.

```almd
list.contains([1, 2, 3], 2) // => true
```

### `list.enumerate(xs: List[A]) -> List[(Int, A)]`

Pair each element with its index.

```almd
list.enumerate(["a", "b"]) // => [(0, "a"), (1, "b")]
```

### `list.zip(xs: List[A], ys: List[B]) -> List[(A, B)]`

Combine two lists into a list of pairs.

```almd
list.zip([1, 2], ["a", "b"]) // => [(1, "a"), (2, "b")]
```

### `list.flatten(xss: List[List[T]]) -> List[T]`

Flatten a list of lists into a single list.

```almd
list.flatten([[1, 2], [3]]) // => [1, 2, 3]
```

### `list.take(xs: List[A], n: Int) -> List[A]`

Take the first n elements.

```almd
list.take([1, 2, 3, 4], 2) // => [1, 2]
```

### `list.drop(xs: List[A], n: Int) -> List[A]`

Drop the first n elements.

```almd
list.drop([1, 2, 3, 4], 2) // => [3, 4]
```

### `list.unique(xs: List[A]) -> List[A]`

Remove duplicate elements, preserving first occurrence.

```almd
list.unique([1, 2, 1, 3]) // => [1, 2, 3]
```

### `list.index_of(xs: List[A], x: A) -> Option[Int]`

Find the first index of an element, or none.

```almd
list.index_of([10, 20, 30], 20) // => some(1)
```

### `list.last(xs: List[A]) -> Option[A]`

Get the last element, or none if empty.

```almd
list.last([1, 2, 3]) // => some(3)
```

### `list.chunk(xs: List[A], n: Int) -> List[List[A]]`

Split a list into chunks of size n.

```almd
list.chunk([1, 2, 3, 4, 5], 2) // => [[1, 2], [3, 4], [5]]
```

### `list.sum(xs: List[Int]) -> Int`

Sum all integers in a list.

```almd
list.sum([1, 2, 3]) // => 6
```

### `list.product(xs: List[Int]) -> Int`

Multiply all integers in a list.

```almd
list.product([2, 3, 4]) // => 24
```

### `list.first(xs: List[A]) -> Option[A]`

Get the first element, or none if empty.

```almd
list.first([1, 2, 3]) // => some(1)
```

### `list.is_empty(xs: List[A]) -> Bool`

Check if a list is empty.

```almd
list.is_empty([]) // => true
```

### `list.min(xs: List[A]) -> Option[A]`

Find the minimum element, or none if empty.

```almd
list.min([3, 1, 2]) // => some(1)
```

### `list.max(xs: List[A]) -> Option[A]`

Find the maximum element, or none if empty.

```almd
list.max([3, 1, 2]) // => some(3)
```

### `list.join(xs: List[String], sep: String) -> String`

Join a list of strings with a separator.

```almd
list.join(["a", "b", "c"], "-") // => "a-b-c"
```

### `list.map(xs: List[A], f: Fn[A] -> B) -> List[B]`

Apply a function to each element, returning a new list.

```almd
[1, 2, 3].map(fn(x) => x * 2) // => [2, 4, 6]
```

### `list.filter(xs: List[A], f: Fn[A] -> Bool) -> List[A]`

Keep elements that satisfy a predicate.

```almd
[1, 2, 3, 4].filter(fn(x) => x > 2) // => [3, 4]
```

### `list.find(xs: List[A], f: Fn[A] -> Bool) -> Option[A]`

Find the first element matching a predicate.

```almd
[1, 2, 3].find(fn(x) => x > 1) // => some(2)
```

### `list.any(xs: List[A], f: Fn[A] -> Bool) -> Bool`

Check if any element satisfies a predicate.

```almd
[1, 2, 3].any(fn(x) => x > 2) // => true
```

### `list.all(xs: List[A], f: Fn[A] -> Bool) -> Bool`

Check if all elements satisfy a predicate.

```almd
[2, 4, 6].all(fn(x) => x % 2 == 0) // => true
```

### `list.sort_by(xs: List[A], f: Fn[A] -> B) -> List[A]`

Sort by a key-extraction function (not a comparator). f extracts the sort key from each element.

```almd
["bb", "a", "ccc"].sort_by((s) => string.len(s)) // => ["a", "bb", "ccc"]
```

### `list.flat_map(xs: List[A], f: Fn[A] -> List[B]) -> List[B]`

Map each element to a list and flatten the results.

```almd
[1, 2].flat_map(fn(x) => [x, x * 10]) // => [1, 10, 2, 20]
```

### `list.filter_map(xs: List[A], f: Fn[A] -> Option[B]) -> List[B]`

Map and filter in one pass: keep only some values.

```almd
["1", "x", "3"].filter_map(fn(s) => int.parse(s)) // => [1, 3]
```

### `list.take_while(xs: List[A], f: Fn[A] -> Bool) -> List[A]`

Take elements from the front while a predicate holds.

```almd
[1, 2, 3, 1].take_while(fn(x) => x < 3) // => [1, 2]
```

### `list.drop_while(xs: List[A], f: Fn[A] -> Bool) -> List[A]`

Drop elements from the front while a predicate holds.

```almd
[1, 2, 3, 1].drop_while(fn(x) => x < 3) // => [3, 1]
```

### `list.count(xs: List[A], f: Fn[A] -> Bool) -> Int`

Count elements that satisfy a predicate.

```almd
[1, 2, 3, 4].count(fn(x) => x > 2) // => 2
```

### `list.partition(xs: List[A], f: Fn[A] -> Bool) -> (List[A], List[A])`

Split a list into two: elements matching and not matching a predicate.

```almd
[1, 2, 3, 4].partition(fn(x) => x % 2 == 0) // => ([2, 4], [1, 3])
```

### `list.reduce(xs: List[A], f: Fn[A, A] -> A) -> Option[A]`

Reduce a list by combining elements pairwise. Returns none if empty.

```almd
[1, 2, 3].reduce(fn(a, b) => a + b) // => some(6)
```

### `list.group_by(xs: List[A], f: Fn[A] -> B) -> Map[B, List[A]]`

Group elements by a key function into a map.

```almd
["hi", "hey", "bye"].group_by(fn(s) => string.char_at(s, 0))
```

### `list.range(start: Int, end: Int) -> List[Int]`

Create a list of integers from start (inclusive) to end (exclusive).

```almd
list.range(1, 5) // => [1, 2, 3, 4]
```

### `list.slice(xs: List[A], start: Int, end: Int) -> List[A]`

Extract a sublist from start to end index.

```almd
list.slice([1, 2, 3, 4, 5], 1, 4) // => [2, 3, 4]
```

### `list.insert(xs: List[A], i: Int, val: A) -> List[A]`

Insert an element at index i, shifting elements right.

```almd
list.insert([1, 3], 1, 2) // => [1, 2, 3]
```

### `list.remove_at(xs: List[A], i: Int) -> List[A]`

Remove the element at index i.

```almd
list.remove_at([1, 2, 3], 1) // => [1, 3]
```

### `list.find_index(xs: List[A], f: Fn[A] -> Bool) -> Option[Int]`

Find the first index where a predicate holds.

```almd
[10, 20, 30].find_index(fn(x) => x > 15) // => some(1)
```

### `list.update(xs: List[A], i: Int, f: Fn[A] -> A) -> List[A]`

Return a new list with the element at index i transformed by f.

```almd
list.update([1, 2, 3], 1, fn(x) => x * 10) // => [1, 20, 3]
```

### `list.repeat(val: A, n: Int) -> List[A]`

Create a list with a value repeated n times.

```almd
list.repeat(0, 3) // => [0, 0, 0]
```

### `list.scan(xs: List[A], init: B, f: Fn[B, A] -> B) -> List[B]`

Like fold, but returns all intermediate accumulator values.

```almd
[1, 2, 3].scan(0, fn(acc, x) => acc + x) // => [1, 3, 6]
```

### `list.intersperse(xs: List[A], sep: A) -> List[A]`

Insert a separator between each element.

```almd
list.intersperse([1, 2, 3], 0) // => [1, 0, 2, 0, 3]
```

### `list.windows(xs: List[A], n: Int) -> List[List[A]]`

Return sliding windows of size n.

```almd
list.windows([1, 2, 3, 4], 2) // => [[1, 2], [2, 3], [3, 4]]
```

### `list.dedup(xs: List[A]) -> List[A]`

Remove consecutive duplicates.

```almd
list.dedup([1, 1, 2, 2, 1]) // => [1, 2, 1]
```

### `list.zip_with(xs: List[A], ys: List[B], f: Fn[A, B] -> C) -> List[C]`

Combine two lists element-wise using a function.

```almd
list.zip_with([1, 2], [10, 20], fn(a, b) => a + b) // => [11, 22]
```

### `list.fold(xs: List[A], init: B, f: Fn[B, A] -> B) -> B`

Reduce a list from left with an initial accumulator.

```almd
[1, 2, 3].fold(0, fn(acc, x) => acc + x) // => 6
```

### `list.take_end(xs: List[A], n: Int) -> List[A]`

Take the last N elements.

```almd
list.take_end([1, 2, 3, 4], 2) // => [3, 4]
```

### `list.drop_end(xs: List[A], n: Int) -> List[A]`

Drop the last N elements.

```almd
list.drop_end([1, 2, 3, 4], 2) // => [1, 2]
```

### `list.unique_by(xs: List[A], f: Fn[A] -> K) -> List[A]`

Remove duplicates by key function, preserving first occurrence.

```almd
list.unique_by(["aa", "ab", "ba"], (s) => string.get(s, 0))
```

### `list.shuffle(xs: List[A]) -> List[A]`

Return a randomly shuffled copy of the list.

```almd
list.shuffle([1, 2, 3, 4])
```

### `list.window(xs: List[A], n: Int) -> List[List[A]]`

Sliding window of size N over the list.

```almd
list.window([1, 2, 3, 4], 2) // => [[1, 2], [2, 3], [3, 4]]
```

### `list.push(xs: List[A], x: A) -> Unit`

Append an element in place. Requires var binding.

```almd
list.push(xs, 42)
```

### `list.pop(xs: List[A]) -> Option[A]`

Remove and discard the last element in place. Requires var binding.

```almd
list.pop(xs)
```

### `list.clear(xs: List[A]) -> Unit`

Remove all elements in place. Requires var binding.

```almd
list.clear(xs)
```
