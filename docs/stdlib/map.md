# map

Map (dictionary) operations. auto-imported.

### `map.new() -> Map[K, V]`

Create an empty map.

```almd
let m = map.new()
```

### `map.get(m: Map[K, V], key: K) -> Option[V]`

Get a value by key. Returns none if the key doesn't exist.

```almd
map.get(m, "name")
```

### `map.get_or(m: Map[K, V], key: K, default: V) -> V`

Get a value by key, returning a default if the key doesn't exist.

```almd
map.get_or(m, "name", "unknown")
```

### `map.set(m: Map[K, V], key: K, value: V) -> Map[K, V]`

Return a new map with the key set to value. Immutable — does not modify the original.

```almd
let m2 = map.set(m, "name", "Alice")
```

### `map.contains(m: Map[K, V], key: K) -> Bool`

Check if a key exists in the map.

```almd
map.contains(m, "name")
```

### `map.remove(m: Map[K, V], key: K) -> Map[K, V]`

Return a new map with the key removed. Immutable — does not modify the original.

```almd
let m2 = map.remove(m, "temp")
```

### `map.keys(m: Map[K, V]) -> List[K]`

Get all keys as a sorted list.

```almd
map.keys(m)
```

### `map.values(m: Map[K, V]) -> List[V]`

Get all values as a list.

```almd
map.values(m)
```

### `map.len(m: Map[K, V]) -> Int`

Get the number of key-value pairs in the map.

```almd
map.len(m)
```

### `map.entries(m: Map[K, V]) -> List[(K, V)]`

Get all key-value pairs as a list of tuples, sorted by key.

```almd
map.entries(m)
```

### `map.merge(a: Map[K, V], b: Map[K, V]) -> Map[K, V]`

Merge two maps. Keys in the second map override keys in the first.

```almd
map.merge(base, overrides)
```

### `map.is_empty(m: Map[K, V]) -> Bool`

Check if the map has no entries.

```almd
map.is_empty(m)
```

### `map.from_list(pairs: List[(K, V)]) -> Map[K, V]`

Create a map from a list of (key, value) pairs.

```almd
map.from_list([("a", 1), ("b", 2)])
```

### `map.map(m: Map[K, V], f: Fn[V] -> B) -> Map[K, B]`

Transform all values in the map using a function, keeping keys unchanged.

```almd
map.map_values(m, fn(v) => v * 2)
```

### `map.filter(m: Map[K, V], f: Fn[K, V] -> Bool) -> Map[K, V]`

Return a new map containing only entries where the predicate returns true.

```almd
map.filter(m, fn(k, v) => v > 0)
```

### `map.fold(m: Map[K, V], init: A, f: Fn[A, K, V] -> A) -> A`

Accumulate over all entries with an initial value.

```almd
map.fold(scores, 0, (acc, k, v) => acc + v)
```

### `map.any(m: Map[K, V], f: Fn[K, V] -> Bool) -> Bool`

Check if any entry satisfies the predicate.

```almd
map.any(scores, (k, v) => v >= 90)
```

### `map.all(m: Map[K, V], f: Fn[K, V] -> Bool) -> Bool`

Check if all entries satisfy the predicate.

```almd
map.all(scores, (k, v) => v > 0)
```

### `map.count(m: Map[K, V], f: Fn[K, V] -> Bool) -> Int`

Count entries that satisfy the predicate.

```almd
map.count(scores, (k, v) => v >= 80)
```

### `map.find(m: Map[K, V], f: Fn[K, V] -> Bool) -> Option[(K, V)]`

Find the first entry matching the predicate. Returns Option[(K, V)].

```almd
map.find(scores, (k, v) => v >= 90)
```

### `map.update(m: Map[K, V], key: K, f: Fn[V] -> V) -> Map[K, V]`

Update the value at a key using a function. Key must exist.

```almd
map.update(scores, "alice", (v) => v + 10)
```

### `map.insert(m: Map[K, V], key: K, value: V) -> Unit`

Insert a key-value pair in place. Requires var binding.

```almd
map.insert(m, "name", "Alice")
```

### `map.delete(m: Map[K, V], key: K) -> Unit`

Remove a key in place. Requires var binding.

```almd
map.delete(m, "temp")
```

### `map.clear(m: Map[K, V]) -> Unit`

Remove all entries in place. Requires var binding.

```almd
map.clear(m)
```
