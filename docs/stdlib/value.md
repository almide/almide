# value

The dynamic `Value` **data model**: constructors and accessors.
auto-imported. The `json` module is the format over it
(`json.parse` / `json.stringify`) — see [json.md](./json.md).

### `value.field(v: Value, key: String) -> Result[Value, String]`

Get a field from a Value object by key. `err("expected Object")` on a
non-Object subject, `err("missing field '<k>'")` when absent. For an
Option instead, apply `?`: `value.field(v, k)?`.

(The retired `value.get` alias of this fn was dropped after its E040
deprecation window — `get` means Option everywhere else: `map.get` /
`list.get`.)

### `value.keys(v: Value) -> List[String]`

All keys of a Value object, `[]` for non-Objects.

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

### `value.str(s: String) -> Value`

Create a Value from a String.

### `value.int(n: Int) -> Value`

Create a Value from an Int.

### `value.float(f: Float) -> Value`

Create a Value from a Float.

### `value.bool(b: Bool) -> Value`

Create a Value from a Bool.

### `value.object(pairs: List[(String, Value)]) -> Value`

Create a Value object from a list of key-value pairs.

### `value.array(items: List[Value]) -> Value`

Create a Value array from a list of Values.

### `value.null() -> Value`

Create a null Value.

### `value.pick(v: Value, keys: List[String]) -> Value`

Pick specific keys from an Object, discarding the rest.

### `value.omit(v: Value, keys: List[String]) -> Value`

Remove specific keys from an Object.

### `value.merge(a: Value, b: Value) -> Value`

Merge two Objects. Keys from b override keys from a.

### `value.to_camel_case(v: Value) -> Value`

Convert Object keys from snake_case to camelCase.

### `value.to_snake_case(v: Value) -> Value`

Convert Object keys from camelCase to snake_case.

### `value.stringify(v: Value) -> String`

Convert a Value to a JSON-like string representation — the same text as `json.stringify`:
a Float prints in its shortest form without a trailing `.0`, and a non-finite Float
(NaN, +inf, -inf) is written as `null`.

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (20 functions)

```
// Wraps items as an Array Value.
// @since 0.6.0 or earlier
value.array(items: List[Value]) -> Value

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

// Wraps pairs as an Object; duplicate keys kept.
// @since 0.6.0 or earlier
value.object(pairs: List[(String, Value)]) -> Value

// v minus the listed keys; non-objects as-is.
// @since 0.6.0 or earlier
value.omit(v: Value, keys: List[String]) -> Value

// Only the listed keys of v; non-objects as-is.
// @since 0.6.0 or earlier
value.pick(v: Value, keys: List[String]) -> Value

// Value holding the String s.
// @since 0.6.0 or earlier
value.str(s: String) -> Value

// Compact JSON; NaN/inf print bare (invalid JSON).
// @since 0.6.0 or earlier
value.stringify(v: Value) -> String

// Top-level keys to camelCase; nested untouched.
// @since 0.6.0 or earlier
value.to_camel_case(v: Value) -> Value

// Top-level keys to snake_case; userID -> user_i_d.
// @since 0.6.0 or earlier
value.to_snake_case(v: Value) -> Value

// Value at key; err if missing or v not an Object.
// @since 0.6.0 or earlier
value.field(v: Value, key: String) -> Result[Value, String]

// Object keys in order; [] for non-objects.
// @since 0.26.0 or earlier
value.keys(v: Value) -> List[String]
```

<!-- END GENERATED SIGNATURE INDEX -->
