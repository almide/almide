# value

Dynamic value manipulation. auto-imported.

### `value.get(v: Value, key: String) -> Result[Value, String]`

Get a field from a Value object by key. Returns err if missing.

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

Convert a Value to a JSON-like string representation.
