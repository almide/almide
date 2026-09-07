# set

Set operations. auto-imported.

### `set.new() -> Set[A]`

Create an empty set.

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

Create a set from a list of values.

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

### `set.insert(s: Set[A], value: A) -> Set[A]`

Add a value to the set. Returns a new set.

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

### `set.remove(s: Set[A], value: A) -> Set[A]`

Remove a value from the set. Returns a new set.

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

### `set.contains(s: Set[A], value: A) -> Bool`

Check if a value is in the set.

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

Return the number of elements.

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

Check if the set has no elements.

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

Convert a set to a list.

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

Return the union of two sets.

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

Return the intersection of two sets.

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

Return elements in a that are not in b.

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

Return elements in either set but not both.

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

Check if all elements of a are in b.

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

Check if two sets have no elements in common.

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

### `set.filter(s: Set[A], f: Fn[A] -> Bool) -> Set[A]`

Keep elements that satisfy a predicate.

```almd run
fn main() -> Unit = {
  let s = set.from_list([1, 2, 3, 4])
  println("${set.filter(s, (x) => x > 2)}")
}
```
```output
set.from_list([3, 4])
```

### `set.map(s: Set[A], f: Fn[A] -> B) -> Set[B]`

Apply a function to each element, returning a new set.

```almd run
fn main() -> Unit = {
  let s = set.from_list([1, 2, 3])
  println("${set.map(s, (x) => x * 2)}")
}
```
```output
set.from_list([2, 4, 6])
```

### `set.fold(s: Set[A], init: B, f: Fn[B, A] -> B) -> B`

Reduce a set with an initial accumulator.

```almd run
fn main() -> Unit = {
  let s = set.from_list([1, 2, 3])
  println("${set.fold(s, 0, (acc, x) => acc + x)}")
}
```
```output
6
```

### `set.any(s: Set[A], f: Fn[A] -> Bool) -> Bool`

Check if any element satisfies a predicate.

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

### `set.all(s: Set[A], f: Fn[A] -> Bool) -> Bool`

Check if all elements satisfy a predicate.

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

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (19 functions)

```
set.new() -> Set[A]
set.from_list(xs: List[A]) -> Set[A]
set.insert(s: Set[A], value: A) -> Set[A]
set.remove(s: Set[A], value: A) -> Set[A]
set.contains(s: Set[A], value: A) -> Bool
set.len(s: Set[A]) -> Int
set.is_empty(s: Set[A]) -> Bool
set.to_list(s: Set[A]) -> List[A]
set.union(a: Set[A], b: Set[A]) -> Set[A]
set.intersection(a: Set[A], b: Set[A]) -> Set[A]
set.difference(a: Set[A], b: Set[A]) -> Set[A]
set.symmetric_difference(a: Set[A], b: Set[A]) -> Set[A]
set.is_subset(a: Set[A], b: Set[A]) -> Bool
set.is_disjoint(a: Set[A], b: Set[A]) -> Bool
set.filter(s: Set[A], f: (A) -> Bool) -> Set[A]
set.map(s: Set[A], f: (A) -> B) -> Set[B]
set.fold(s: Set[A], init: B, f: (B, A) -> B) -> B
set.any(s: Set[A], f: (A) -> Bool) -> Bool
set.all(s: Set[A], f: (A) -> Bool) -> Bool
```

<!-- END GENERATED SIGNATURE INDEX -->
