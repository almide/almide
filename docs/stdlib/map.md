# map

Map (dictionary) operations. auto-imported.

### `map.new() -> Map[K, V]`

Create an empty map.

```almd run
fn main() -> Unit = {
  let m: Map[String, Int] = map.new()
  println("${map.len(m)}")
  println("${map.is_empty(m)}")
}
```
```output
0
true
```

### `map.get(m: Map[K, V], key: K) -> Option[V]`

Get a value by key. Returns none if the key doesn't exist.

```almd run
fn main() -> Unit = {
  let m = map.from_list([("name", "Alice")])
  match map.get(m, "name") {
    some(v) => println(v),
    none => println("missing"),
  }
  match map.get(m, "age") {
    some(v) => println(v),
    none => println("missing"),
  }
}
```
```output
Alice
missing
```

### `map.get_or(m: Map[K, V], key: K, default: V) -> V`

Get a value by key, returning a default if the key doesn't exist.

```almd run
fn main() -> Unit = {
  let m = map.from_list([("name", "Alice")])
  println(map.get_or(m, "name", "unknown"))
  println(map.get_or(m, "email", "unknown"))
}
```
```output
Alice
unknown
```

### `map.set(m: Map[K, V], key: K, value: V) -> Map[K, V]`

Return a new map with the key set to value. Immutable — does not modify the original.

```almd run
fn main() -> Unit = {
  let m: Map[String, String] = map.new()
  let m2 = map.set(m, "name", "Alice")
  println("${m}")
  println("${m2}")
}
```
```output
[:]
["name": "Alice"]
```

### `map.contains(m: Map[K, V], key: K) -> Bool`

Check if a key exists in the map.

```almd run
fn main() -> Unit = {
  let m = map.from_list([("name", "Alice")])
  println("${map.contains(m, "name")}")
  println("${map.contains(m, "age")}")
}
```
```output
true
false
```

### `map.remove(m: Map[K, V], key: K) -> Map[K, V]`

Return a new map with the key removed. Immutable — does not modify the original.

```almd run
fn main() -> Unit = {
  let m = map.from_list([("keep", 1), ("temp", 2)])
  let m2 = map.remove(m, "temp")
  println("${m}")
  println("${m2}")
}
```
```output
["keep": 1, "temp": 2]
["keep": 1]
```

### `map.keys(m: Map[K, V]) -> List[K]`

Get all keys as a list, in insertion order (the order the entries were first inserted; overwriting a key keeps its position).

```almd
map.keys(m)
```

### `map.values(m: Map[K, V]) -> List[V]`

Get all values as a list.

```almd run
fn main() -> Unit = {
  let m = map.from_list([("a", 1), ("b", 2), ("c", 3)])
  println("${map.values(m)}")
}
```
```output
[1, 2, 3]
```

### `map.len(m: Map[K, V]) -> Int`

Get the number of key-value pairs in the map.

```almd run
fn main() -> Unit = {
  let m = map.from_list([("a", 1), ("b", 2), ("c", 3)])
  println("${map.len(m)}")
}
```
```output
3
```

### `map.entries(m: Map[K, V]) -> List[(K, V)]`

Get all key-value pairs as a list of tuples, in insertion order.

```almd
map.entries(m)
```

### `map.merge(a: Map[K, V], b: Map[K, V]) -> Map[K, V]`

Merge two maps. Keys in the second map override keys in the first.

```almd run
fn main() -> Unit = {
  let base = map.from_list([("a", 1), ("b", 2)])
  let overrides = map.from_list([("b", 20), ("c", 3)])
  println("${map.merge(base, overrides)}")
}
```
```output
["a": 1, "b": 20, "c": 3]
```

### `map.is_empty(m: Map[K, V]) -> Bool`

Check if the map has no entries.

```almd run
fn main() -> Unit = {
  let m: Map[String, Int] = map.new()
  println("${map.is_empty(m)}")
  println("${map.is_empty(map.from_list([("a", 1)]))}")
}
```
```output
true
false
```

### `map.from_list(pairs: List[(K, V)]) -> Map[K, V]`

Create a map from a list of (key, value) pairs.

```almd run
fn main() -> Unit = {
  let m = map.from_list([("a", 1), ("b", 2)])
  println("${m}")
  println("${map.len(m)}")
}
```
```output
["a": 1, "b": 2]
2
```

### `map.map(m: Map[K, V], f: Fn[V] -> B) -> Map[K, B]`

Transform all values in the map using a function, keeping keys unchanged.

```almd run
fn main() -> Unit = {
  let m = map.from_list([("a", 1), ("b", 2), ("c", 3)])
  let doubled = map.map(m, (v) => v * 2)
  println("${map.get(doubled, "a") ?? 0} ${map.get(doubled, "b") ?? 0} ${map.get(doubled, "c") ?? 0}")
  println(int.to_string(map.len(doubled)))
}
```
```output
2 4 6
3
```

### `map.filter(m: Map[K, V], f: Fn[K, V] -> Bool) -> Map[K, V]`

Return a new map containing only entries where the predicate returns true.

```almd run
fn main() -> Unit = {
  let m = map.from_list([("a", 1), ("b", 0), ("c", 3)])
  println("${map.filter(m, (k, v) => v > 0)}")
}
```
```output
["a": 1, "c": 3]
```

### `map.fold(m: Map[K, V], init: A, f: Fn[A, K, V] -> A) -> A`

Accumulate over all entries with an initial value.

```almd run
fn main() -> Unit = {
  let scores = map.from_list([("alice", 90), ("bob", 75), ("carol", 82)])
  println("${map.fold(scores, 0, (acc, k, v) => acc + v)}")
}
```
```output
247
```

### `map.any(m: Map[K, V], f: Fn[K, V] -> Bool) -> Bool`

Check if any entry satisfies the predicate.

```almd run
fn main() -> Unit = {
  let scores = map.from_list([("alice", 90), ("bob", 75), ("carol", 82)])
  println("${map.any(scores, (k, v) => v >= 90)}")
  println("${map.any(scores, (k, v) => v >= 100)}")
}
```
```output
true
false
```

### `map.all(m: Map[K, V], f: Fn[K, V] -> Bool) -> Bool`

Check if all entries satisfy the predicate.

```almd run
fn main() -> Unit = {
  let scores = map.from_list([("alice", 90), ("bob", 75), ("carol", 82)])
  println("${map.all(scores, (k, v) => v > 0)}")
  println("${map.all(scores, (k, v) => v >= 80)}")
}
```
```output
true
false
```

### `map.count(m: Map[K, V], f: Fn[K, V] -> Bool) -> Int`

Count entries that satisfy the predicate.

```almd run
fn main() -> Unit = {
  let scores = map.from_list([("alice", 90), ("bob", 75), ("carol", 82)])
  println("${map.count(scores, (k, v) => v >= 80)}")
}
```
```output
2
```

### `map.find(m: Map[K, V], f: Fn[K, V] -> Bool) -> Option[(K, V)]`

Find the first entry matching the predicate. Returns Option[(K, V)].

```almd run
fn main() -> Unit = {
  let scores = map.from_list([("alice", 90), ("bob", 75), ("carol", 82)])
  match map.find(scores, (k, v) => v >= 90) {
    some((k, v)) => println("${k}: ${v}"),
    none => println("no match"),
  }
  match map.find(scores, (k, v) => v >= 100) {
    some((k, v)) => println("${k}: ${v}"),
    none => println("no match"),
  }
}
```
```output
alice: 90
no match
```

### `map.update(m: Map[K, V], key: K, f: Fn[V] -> V) -> Map[K, V]`

Update the value at a key using a function. Key must exist.

```almd run
fn main() -> Unit = {
  let scores = map.from_list([("alice", 90), ("bob", 75)])
  println("${map.update(scores, "alice", (v) => v + 10)}")
}
```
```output
["alice": 100, "bob": 75]
```

### `map.insert(m: Map[K, V], key: K, value: V) -> Unit`

Insert a key-value pair in place. Requires var binding.

```almd run
fn main() -> Unit = {
  var m: Map[String, String] = map.new()
  map.insert(m, "name", "Alice")
  println("${m}")
}
```
```output
["name": "Alice"]
```

### `map.delete(m: Map[K, V], key: K) -> Unit`

Remove a key in place. Requires var binding.

```almd check
fn main() -> Unit = {
  var m = map.from_list([("keep", 1), ("temp", 2)])
  map.delete(m, "temp")
  println("${m}")
}
```

### `map.clear(m: Map[K, V]) -> Unit`

Remove all entries in place. Requires var binding.

```almd check
fn main() -> Unit = {
  var m = map.from_list([("a", 1), ("b", 2)])
  map.clear(m)
  println("${map.len(m)}")
  println("${map.is_empty(m)}")
}
```

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (25 functions)

```
map.new() -> Map[K, V]
map.get(m: Map[K, V], key: K) -> Option[V]
map.get_or(m: Map[K, V], key: K, default: V) -> V
map.set(m: Map[K, V], key: K, value: V) -> Map[K, V]
map.contains(m: Map[K, V], key: K) -> Bool
map.remove(m: Map[K, V], key: K) -> Map[K, V]
map.keys(m: Map[K, V]) -> List[K]
map.values(m: Map[K, V]) -> List[V]
map.len(m: Map[K, V]) -> Int
map.entries(m: Map[K, V]) -> List[(K, V)]
map.merge(a: Map[K, V], b: Map[K, V]) -> Map[K, V]
map.is_empty(m: Map[K, V]) -> Bool
map.from_list(pairs: List[(K, V)]) -> Map[K, V]
map.map(m: Map[K, V], f: (V) -> B) -> Map[K, B]
map.filter(m: Map[K, V], f: (K, V) -> Bool) -> Map[K, V]
map.fold(m: Map[K, V], init: A, f: (A, K, V) -> A) -> A
map.any(m: Map[K, V], f: (K, V) -> Bool) -> Bool
map.all(m: Map[K, V], f: (K, V) -> Bool) -> Bool
map.count(m: Map[K, V], f: (K, V) -> Bool) -> Int
map.find(m: Map[K, V], f: (K, V) -> Bool) -> Option[(K, V)]
map.update(m: Map[K, V], key: K, f: (V) -> V) -> Map[K, V]
map.upsert(m: Map[K, V], key: K, init: V, f: (V) -> V) -> Map[K, V]
map.insert(m: Map[K, V], key: K, value: V) -> Unit
map.delete(m: Map[K, V], key: K) -> Unit
map.clear(m: Map[K, V]) -> Unit
```

<!-- END GENERATED SIGNATURE INDEX -->
