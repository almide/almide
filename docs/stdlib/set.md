# set

Set operations. auto-imported.

### `set.new() -> Set[A]`

Create an empty set.

```almd
set.new()
```

### `set.from_list(xs: List[A]) -> Set[A]`

Create a set from a list of values.

```almd
set.from_list([1, 2, 3])
```

### `set.insert(s: Set[A], value: A) -> Set[A]`

Add a value to the set. Returns a new set.

```almd
set.insert(s, 42)
```

### `set.remove(s: Set[A], value: A) -> Set[A]`

Remove a value from the set. Returns a new set.

```almd
set.remove(s, 42)
```

### `set.contains(s: Set[A], value: A) -> Bool`

Check if a value is in the set.

```almd
set.contains(s, 42)
```

### `set.len(s: Set[A]) -> Int`

Return the number of elements.

```almd
set.len(s)
```

### `set.is_empty(s: Set[A]) -> Bool`

Check if the set has no elements.

```almd
set.is_empty(s)
```

### `set.to_list(s: Set[A]) -> List[A]`

Convert a set to a list.

```almd
set.to_list(s)
```

### `set.union(a: Set[A], b: Set[A]) -> Set[A]`

Return the union of two sets.

```almd
set.union(a, b)
```

### `set.intersection(a: Set[A], b: Set[A]) -> Set[A]`

Return the intersection of two sets.

```almd
set.intersection(a, b)
```

### `set.difference(a: Set[A], b: Set[A]) -> Set[A]`

Return elements in a that are not in b.

```almd
set.difference(a, b)
```

### `set.symmetric_difference(a: Set[A], b: Set[A]) -> Set[A]`

Return elements in either set but not both.

```almd
set.symmetric_difference(a, b)
```

### `set.is_subset(a: Set[A], b: Set[A]) -> Bool`

Check if all elements of a are in b.

```almd
set.is_subset(a, b)
```

### `set.is_disjoint(a: Set[A], b: Set[A]) -> Bool`

Check if two sets have no elements in common.

```almd
set.is_disjoint(a, b)
```

### `set.filter(s: Set[A], f: Fn[A] -> Bool) -> Set[A]`

Keep elements that satisfy a predicate.

```almd
set.filter(s, fn(x) => x > 2)
```

### `set.map(s: Set[A], f: Fn[A] -> B) -> Set[B]`

Apply a function to each element, returning a new set.

```almd
set.map(s, fn(x) => x * 2)
```

### `set.fold(s: Set[A], init: B, f: Fn[B, A] -> B) -> B`

Reduce a set with an initial accumulator.

```almd
set.fold(s, 0, fn(acc, x) => acc + x)
```

### `set.any(s: Set[A], f: Fn[A] -> Bool) -> Bool`

Check if any element satisfies a predicate.

```almd
set.any(s, fn(x) => x > 2)
```

### `set.all(s: Set[A], f: Fn[A] -> Bool) -> Bool`

Check if all elements satisfy a predicate.

```almd
set.all(s, fn(x) => x > 0)
```
