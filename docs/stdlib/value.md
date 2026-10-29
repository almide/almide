# value

The dynamic `Value` **data model**: constructors and accessors.
auto-imported. The `json` module is the format over it
(`json.parse` / `json.stringify`) — see [json.md](./json.md).

### `value.as_string(v: Value) -> Result[String, String]`

Extract a String from a Value. Returns err if not a Str.

### `value.as_int(v: Value) -> Result[Int, String]`

Extract an Int from a Value. Returns err if not an Int.

### `value.as_float(v: Value) -> Result[Float, String]`

Extract a Float from a Value. Returns err if not a Float.

### `value.as_bool(v: Value) -> Result[Bool, String]`

Extract a Bool from a Value. Returns err if not a Bool.

### `value.as_array(v: Value) -> Result[List[Value], String]`

Extract a List[Value] from a Value. Returns err if not an Array.

### `value.string(s: String) -> Value`

Create a Value from a String.

### `value.int(n: Int) -> Value`

Create a Value from an Int.

### `value.float(f: Float) -> Value`

Create a Value from a Float.

### `value.bool(b: Bool) -> Value`

Create a Value from a Bool.

### `value.object(pairs: List[(String, Value)]) -> Value`

Create a Value object from a list of key-value pairs. No object holds a key
twice (C-401, #4112): a key the list repeats keeps the position of its first
occurrence and the value of its last (C-380), so `value.get`,
`value.as_object` and every serializer see one entry —
`json.stringify(value.object([("role", value.string("user")), ("role", value.string("admin"))]))`
is `{"role":"admin"}`, never a document with `role` twice.

### `value.array(items: List[Value]) -> Value`

Create a Value array from a list of Values.

### `value.null() -> Value`

Create a null Value.

### `value.pick(v: Value, keys: List[String]) -> Value`

Pick specific keys from an Object, discarding the rest. A value that is not
an object has none of the keys, so it picks to `{}` (C-401): an allowlist over
a value that is not an object lets nothing through.

### `value.omit(v: Value, keys: List[String]) -> Value`

Remove specific keys from an Object.

### `value.merge(a: Value, b: Value) -> Value`

Merge two Objects. Keys from b override keys from a.

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (29 functions)

```
// Wraps xs as an Array Value.
// @since 0.6.0 or earlier
value.array(xs: List[Value]) -> Value

// Array items; err if v is another kind.
// @since 0.6.0 or earlier
value.as_array(v: Value) -> Result[List[Value], String]

// Bool payload; err otherwise (no truthiness).
// @since 0.6.0 or earlier
value.as_bool(v: Value) -> Result[Bool, String]

// Float payload (Int widens); err otherwise.
// @since 0.6.0 or earlier
value.as_float(v: Value) -> Result[Float, String]

// Int payload; err on Float, never truncated.
// @since 0.6.0 or earlier
value.as_int(v: Value) -> Result[Int, String]

// Str payload; err otherwise (no coercion).
// @since 0.6.0 or earlier
value.as_string(v: Value) -> Result[String, String]

// Value holding the Bool b.
// @since 0.6.0 or earlier
value.bool(b: Bool) -> Value

// Value holding the Float f.
// @since 0.6.0 or earlier
value.float(f: Float) -> Value

// Value holding the Int n.
// @since 0.6.0 or earlier
value.int(n: Int) -> Value

// Shallow merge, b's keys win; b if not both objects.
// @since 0.6.0 or earlier
value.merge(a: Value, b: Value) -> Value

// The JSON null Value.
// @since 0.6.0 or earlier
value.null() -> Value

// Wraps the pairs xs as an Object; a repeated key keeps its first position, last value.
// @since 0.6.0 or earlier
value.object(xs: List[(String, Value)]) -> Value

// v minus the listed keys; non-objects as-is.
// @since 0.6.0 or earlier
value.omit(v: Value, keys: List[String]) -> Value

// Only the listed keys of v; a non-object picks to {}.
// @since 0.6.0 or earlier
value.pick(v: Value, keys: List[String]) -> Value

// Value holding the String s.
// @since 0.6.0 or earlier
value.string(s: String) -> Value

// Object pairs in order; err if v is another kind.
// @since unreleased
value.as_object(v: Value) -> Result[List[(String, Value)], String]

// Value at key; none if absent or v is not an Object.
// @since unreleased
value.get(v: Value, key: String) -> Option[Value]

// String at key; none if absent or another kind.
// @since unreleased
value.get_string(v: Value, key: String) -> Option[String]

// Int at key; none if absent or not an Int (a Float is never truncated).
// @since unreleased
value.get_int(v: Value, key: String) -> Option[Int]

// Float at key (an Int widens); none if absent or another kind.
// @since unreleased
value.get_float(v: Value, key: String) -> Option[Float]

// Bool at key; none if absent or another kind.
// @since unreleased
value.get_bool(v: Value, key: String) -> Option[Bool]

// Array items at key; none if absent or another kind.
// @since unreleased
value.get_array(v: Value, key: String) -> Option[List[Value]]

// Object pairs at key; none if absent or another kind.
// @since unreleased
value.get_object(v: Value, key: String) -> Option[List[(String, Value)]]

// The empty path: the whole value.
// @since unreleased
value.root() -> Path

// p extended by the object key name.
// @since unreleased
value.key(p: Path, name: String) -> Path

// p extended by the array index i; a negative i is out of range.
// @since unreleased
value.index(p: Path, i: Int) -> Path

// Value at p, or none if a step misses.
// @since unreleased
value.get_path(v: Value, p: Path) -> Option[Value]

// v with x at p; missing keys are created, a missed index leaves v as is; err when the result would nest past 1000.
// @since unreleased
value.set_path(v: Value, p: Path, x: Value) -> Result[Value, String]

// v minus the entry at p; unchanged if absent; null for the root.
// @since unreleased
value.remove_path(v: Value, p: Path) -> Value
```

<!-- END GENERATED SIGNATURE INDEX -->
