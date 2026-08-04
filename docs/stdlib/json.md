# json

The JSON **format**: parsing and printing of `Value` trees. import json.

The data model itself lives on `value.*` (auto-imported) — constructors
(`value.null` / `value.int` / `value.str` / `value.object` / …) and
accessors (`value.as_*` / `value.field` / `value.keys`) — see
[value.md](./value.md). The old `json.*` spellings of those operations were retired aliases —
dropped after their one-release E040 deprecation window; the migration map
is in [Renamed operations](#renamed-operations) at the bottom.

`JsonPath` (the opaque handle of `json.root()` / `json.field(...)`) resolves
in user annotations whenever `json` is imported — bare or `json.JsonPath`.

### `json.parse(text: String) -> Result[Value, String]`

Parse a JSON string into a Value.

```almd
let v = json.parse("{\"name\": \"Alice\"}")
```

### `json.stringify(v: Value) -> String`

Convert a Value to a JSON string.

```almd
json.stringify(person.encode())
```

### `json.stringify_pretty(j: Value) -> String`

Convert a Json value to a pretty-printed JSON string with indentation.

```almd
json.stringify_pretty(j)
```

### `json.get_string(j: Value, key: String) -> Option[String]`

Get a string value by key. Returns none if key doesn't exist or value is not a string.

```almd
json.get_string(j, "name")
```

### `json.get_int(j: Value, key: String) -> Option[Int]`

Get an integer value by key. Returns none if key doesn't exist or value is not an integer.

```almd
json.get_int(j, "age")
```

### `json.get_float(j: Value, key: String) -> Option[Float]`

Get a float value by key. Returns none if key doesn't exist or value is not a number.

```almd
json.get_float(j, "price")
```

### `json.get_bool(j: Value, key: String) -> Option[Bool]`

Get a boolean value by key. Returns none if key doesn't exist or value is not a boolean.

```almd
json.get_bool(j, "active")
```

### `json.get_array(j: Value, key: String) -> Option[List[Value]]`

Get an array value by key. Returns none if key doesn't exist or value is not an array.

```almd
json.get_array(j, "items")
```

### `json.root() -> JsonPath`

Create a root JSON path for traversal.

```almd
json.root()
```

### `json.field(path: JsonPath, name: String) -> JsonPath`

Extend a JSON path with a field name.

```almd
json.field(json.root(), "user")
```

### `json.index(path: JsonPath, i: Int) -> JsonPath`

Extend a JSON path with an array index.

```almd
json.index(json.field(json.root(), "items"), 0)
```

### `json.get_path(j: Value, path: JsonPath) -> Option[Value]`

Get a value at a JSON path. Returns none if path doesn't exist.

```almd
json.get_path(j, json.field(json.root(), "name"))
```

### `json.set_path(j: Value, path: JsonPath, value: Value) -> Result[Value, String]`

Set a value at a JSON path. Returns error if path is invalid.

```almd
json.set_path(j, json.field(json.root(), "name"), json.s("Bob"))
```

### `json.remove_path(j: Value, path: JsonPath) -> Value`

Remove a value at a JSON path. Returns the Json with the value removed.

```almd
json.remove_path(j, json.field(json.root(), "temp"))
```

### `json.to_map(j: Value) -> Option[Map[String, String]]`

Convert a JSON object to a Map[String, String]. Values are stringified. Returns none if not an object.

```almd
let m = json.to_map(obj) ?? map.new()
```

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (15 functions)

```
json.parse(text: String) -> Result[Value, String]
json.stringify(v: Value) -> String
json.stringify_pretty(j: Value) -> String
json.get_string(j: Value, key: String) -> Option[String]
json.get_int(j: Value, key: String) -> Option[Int]
json.get_float(j: Value, key: String) -> Option[Float]
json.get_bool(j: Value, key: String) -> Option[Bool]
json.get_array(j: Value, key: String) -> Option[List[Value]]
json.root() -> JsonPath
json.field(path: JsonPath, name: String) -> JsonPath
json.index(path: JsonPath, i: Int) -> JsonPath
json.get_path(j: Value, path: JsonPath) -> Option[Value]
json.set_path(j: Value, path: JsonPath, value: Value) -> Result[Value, String]
json.remove_path(j: Value, path: JsonPath) -> Value
json.to_map(j: Value) -> Option[Map[String, String]]
```

<!-- END GENERATED SIGNATURE INDEX -->

## Renamed operations

Dropped in #1078 after a one-release deprecation window (E040, v0.53.1 –
v0.53.4). A retired spelling is an ordinary E002 undefined-function error now.

| retired | survivor |
|---|---|
| `json.null` / `json.object` / `json.array` / `json.keys` | `value.null` / `value.object` / `value.array` / `value.keys` |
| `json.from_string` / `json.from_int` / `json.from_bool` / `json.from_float` | `value.str` / `value.int` / `value.bool` / `value.float` |
| `json.as_string` … `json.as_array` (returned `Option`) | `value.as_string(v)?` … `value.as_array(v)?` — `?` converts the `Result` to the same `Option` |
| `json.get(j, k)` (returned `Option`) | `value.field(j, k)?` |
| `value.get(j, k)` | `value.field(j, k)` |

Note one behavioral tightening: the dropped `json.as_int` widened a Float
value to Int; `value.as_int` does not (`err("expected Int")`) — the widening
direction lives on `value.as_float`, which accepts an Int.
