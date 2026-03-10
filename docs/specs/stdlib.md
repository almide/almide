# Standard Library Specification

> Verified by `exercises/stdlib-test/` (24 test files).

---

## Overview

The Almide stdlib is split into two categories:

| Category | Modules | Implementation |
|---|---|---|
| **Hardcoded** | string, list, map, int, float, math, fs, process, env, io, json, http, random, regex | Type signatures in `src/stdlib.rs`, codegen in `src/emit_rust/calls.rs` |
| **Bundled** | args, path, time, encoding, hash, term | `.almd` source files in `stdlib/`, embedded via `include_str!` |

Hardcoded modules have type signatures defined in `lookup_sig()` and runtime implementations in `*_runtime.txt` files. Bundled modules are written in Almide itself and compiled like user code.

### Effect Functions

Functions marked `effect` perform I/O or side effects. They can only be called from `effect fn` contexts. In effect context, `Result[T, E]` return values are auto-unwrapped (the `?` operator is inserted by the compiler).

### UFCS (Uniform Function Call Syntax)

Most stdlib functions support dot syntax: `x.trim()` is rewritten to `string.trim(x)`. When a method name is ambiguous across modules (e.g. `len` exists in string, list, and map), the compiler resolves by receiver type at compile time.

---

## 1. string

String manipulation functions. All are pure.

| Function | Signature | Description |
|---|---|---|
| `trim` | `(s: String) -> String` | Remove leading and trailing whitespace |
| `trim_start` | `(s: String) -> String` | Remove leading whitespace |
| `trim_end` | `(s: String) -> String` | Remove trailing whitespace |
| `split` | `(s: String, sep: String) -> List[String]` | Split string by separator |
| `join` | `(list: List[String], sep: String) -> String` | Join list of strings with separator |
| `len` | `(s: String) -> Int` | Character length of string |
| `contains` | `(s: String, sub: String) -> Bool` | Check if string contains substring |
| `contains?` | `(s: String, sub: String) -> Bool` | Alias for `contains` |
| `starts_with?` | `(s: String, prefix: String) -> Bool` | Check if string starts with prefix |
| `ends_with?` | `(s: String, suffix: String) -> Bool` | Check if string ends with suffix |
| `to_upper` | `(s: String) -> String` | Convert to uppercase |
| `to_lower` | `(s: String) -> String` | Convert to lowercase |
| `replace` | `(s: String, from: String, to: String) -> String` | Replace all occurrences |
| `replace_first` | `(s: String, from: String, to: String) -> String` | Replace first occurrence |
| `to_int` | `(s: String) -> Result[Int, String]` | Parse string as integer |
| `to_float` | `(s: String) -> Result[Float, String]` | Parse string as float |
| `to_bytes` | `(s: String) -> List[Int]` | Convert to UTF-8 byte list |
| `from_bytes` | `(bytes: List[Int]) -> String` | Construct string from UTF-8 bytes |
| `char_at` | `(s: String, i: Int) -> Option[String]` | Get character at index |
| `slice` | `(s: String, start: Int, end: Int) -> String` | Substring (end is optional at call site) |
| `lines` | `(s: String) -> List[String]` | Split by newlines |
| `chars` | `(s: String) -> List[String]` | Split into individual characters |
| `index_of` | `(s: String, needle: String) -> Option[Int]` | First index of substring |
| `last_index_of` | `(s: String, needle: String) -> Option[Int]` | Last index of substring |
| `repeat` | `(s: String, n: Int) -> String` | Repeat string n times |
| `reverse` | `(s: String) -> String` | Reverse character order |
| `count` | `(s: String, sub: String) -> Int` | Count occurrences of substring |
| `pad_left` | `(s: String, n: Int, ch: String) -> String` | Pad on left to width n with character ch |
| `pad_right` | `(s: String, n: Int, ch: String) -> String` | Pad on right to width n with character ch |
| `strip_prefix` | `(s: String, prefix: String) -> Option[String]` | Remove prefix if present |
| `strip_suffix` | `(s: String, suffix: String) -> Option[String]` | Remove suffix if present |
| `is_empty?` | `(s: String) -> Bool` | Check if string is empty |
| `is_digit?` | `(s: String) -> Bool` | Check if all characters are digits |
| `is_alpha?` | `(s: String) -> Bool` | Check if all characters are alphabetic |
| `is_alphanumeric?` | `(s: String) -> Bool` | Check if all characters are alphanumeric |
| `is_whitespace?` | `(s: String) -> Bool` | Check if all characters are whitespace |

> Verified by `exercises/stdlib-test/stdlib-test.almd`, `stdlib_v2_test.almd`, `stdlib_phase5_test.almd`, `stdlib_phase6_test.almd`, `ufcs_test.almd`.

---

## 2. list

List manipulation functions. All are pure.

| Function | Signature | Description |
|---|---|---|
| `len` | `(xs: List[T]) -> Int` | Number of elements |
| `get` | `(xs: List[T], i: Int) -> Option[T]` | Get element at index |
| `get_or` | `(xs: List[T], i: Int, default: T) -> T` | Get element at index or return default |
| `first` | `(xs: List[T]) -> Option[T]` | First element |
| `last` | `(xs: List[T]) -> Option[T]` | Last element |
| `map` | `(xs: List[T], f: (T) -> U) -> List[U]` | Transform each element |
| `filter` | `(xs: List[T], f: (T) -> Bool) -> List[T]` | Keep elements matching predicate |
| `filter_map` | `(xs: List[T], f: (T) -> Option[U]) -> List[U]` | Filter and transform in one pass |
| `find` | `(xs: List[T], f: (T) -> Bool) -> Option[T]` | First element matching predicate |
| `fold` | `(xs: List[T], init: U, f: (U, T) -> U) -> U` | Left fold with initial value |
| `reduce` | `(xs: List[T], f: (T, T) -> T) -> Option[T]` | Fold without initial value |
| `each` | `(xs: List[T], f: (T) -> Unit) -> Unit` | Iterate for side effects |
| `any` | `(xs: List[T], f: (T) -> Bool) -> Bool` | True if any element matches |
| `all` | `(xs: List[T], f: (T) -> Bool) -> Bool` | True if all elements match |
| `sort` | `(xs: List[T]) -> List[T]` | Sort in natural order |
| `sort_by` | `(xs: List[T], f: (T) -> U) -> List[T]` | Sort by key function |
| `reverse` | `(xs: List[T]) -> List[T]` | Reverse element order |
| `contains` | `(xs: List[T], x: T) -> Bool` | Check if list contains element |
| `index_of` | `(xs: List[T], x: T) -> Option[Int]` | First index of element |
| `unique` | `(xs: List[T]) -> List[T]` | Remove duplicate elements |
| `flatten` | `(xss: List[List[T]]) -> List[T]` | Flatten nested list one level |
| `flat_map` | `(xs: List[T], f: (T) -> List[U]) -> List[U]` | Map then flatten |
| `enumerate` | `(xs: List[T]) -> List[{Int, T}]` | Pair each element with its index |
| `zip` | `(a: List[T], b: List[U]) -> List[{T, U}]` | Pair elements from two lists |
| `take` | `(xs: List[T], n: Int) -> List[T]` | First n elements |
| `drop` | `(xs: List[T], n: Int) -> List[T]` | Drop first n elements |
| `take_while` | `(xs: List[T], f: (T) -> Bool) -> List[T]` | Take while predicate holds |
| `drop_while` | `(xs: List[T], f: (T) -> Bool) -> List[T]` | Drop while predicate holds |
| `chunk` | `(xs: List[T], n: Int) -> List[List[T]]` | Split into chunks of size n |
| `partition` | `(xs: List[T], f: (T) -> Bool) -> {List[T], List[T]}` | Split into matching and non-matching |
| `group_by` | `(xs: List[T], f: (T) -> K) -> Map[K, List[T]]` | Group elements by key function |
| `count` | `(xs: List[T], f: (T) -> Bool) -> Int` | Count elements matching predicate |
| `sum` | `(xs: List[Int]) -> Int` | Sum of integer list |
| `product` | `(xs: List[Int]) -> Int` | Product of integer list |
| `min` | `(xs: List[T]) -> Option[T]` | Minimum element |
| `max` | `(xs: List[T]) -> Option[T]` | Maximum element |
| `join` | `(xs: List[String], sep: String) -> String` | Join string list with separator |
| `is_empty?` | `(xs: List[T]) -> Bool` | Check if list is empty |

> Verified by `exercises/stdlib-test/stdlib-test.almd`, `stdlib_v2_test.almd`, `stdlib_phase5_test.almd`, `stdlib_phase6_test.almd`, `ufcs_test.almd`.

---

## 3. map

Immutable map (dictionary) operations. All are pure. Maps use `Map[K, V]` type.

| Function | Signature | Description |
|---|---|---|
| `new` | `() -> Map[K, V]` | Create an empty map |
| `get` | `(m: Map[K, V], key: K) -> Option[V]` | Look up value by key |
| `get_or` | `(m: Map[K, V], key: K, default: V) -> V` | Look up value or return default |
| `set` | `(m: Map[K, V], key: K, value: V) -> Map[K, V]` | Return new map with key set |
| `remove` | `(m: Map[K, V], key: K) -> Map[K, V]` | Return new map with key removed |
| `contains` | `(m: Map[K, V], key: K) -> Bool` | Check if key exists |
| `keys` | `(m: Map[K, V]) -> List[K]` | List of all keys |
| `values` | `(m: Map[K, V]) -> List[V]` | List of all values |
| `entries` | `(m: Map[K, V]) -> List[{K, V}]` | List of key-value pairs |
| `len` | `(m: Map[K, V]) -> Int` | Number of entries |
| `merge` | `(a: Map[K, V], b: Map[K, V]) -> Map[K, V]` | Merge two maps (b overrides a) |
| `from_list` | `(xs: List[T], f: (T) -> {K, V}) -> Map[K, V]` | Build map from list with key function |
| `from_entries` | `(entries: List[{K, V}]) -> Map[K, V]` | Build map from key-value pairs |
| `map_values` | `(m: Map[K, V], f: (V) -> U) -> Map[K, U]` | Transform all values |
| `filter` | `(m: Map[K, V], f: (K, V) -> Bool) -> Map[K, V]` | Keep entries matching predicate |
| `is_empty?` | `(m: Map[K, V]) -> Bool` | Check if map is empty |

> Verified by `exercises/stdlib-test/stdlib-test.almd`, `stdlib_v2_test.almd`, `stdlib_phase6_test.almd`.

---

## 4. int

Integer operations. All are pure.

| Function | Signature | Description |
|---|---|---|
| `to_string` | `(n: Int) -> String` | Convert integer to string |
| `to_hex` | `(n: Int) -> String` | Convert integer to hexadecimal string |
| `parse` | `(s: String) -> Result[Int, String]` | Parse string as integer |
| `parse_hex` | `(s: String) -> Result[Int, String]` | Parse hexadecimal string as integer |
| `abs` | `(n: Int) -> Int` | Absolute value |
| `min` | `(a: Int, b: Int) -> Int` | Minimum of two integers |
| `max` | `(a: Int, b: Int) -> Int` | Maximum of two integers |
| `clamp` | `(n: Int, lo: Int, hi: Int) -> Int` | Clamp value to range [lo, hi] |
| `band` | `(a: Int, b: Int) -> Int` | Bitwise AND |
| `bor` | `(a: Int, b: Int) -> Int` | Bitwise OR |
| `bxor` | `(a: Int, b: Int) -> Int` | Bitwise XOR |
| `bnot` | `(a: Int) -> Int` | Bitwise NOT |
| `bshl` | `(a: Int, n: Int) -> Int` | Bitwise shift left |
| `bshr` | `(a: Int, n: Int) -> Int` | Bitwise shift right |
| `wrap_add` | `(a: Int, b: Int, bits: Int) -> Int` | Wrapping addition (fixed-width) |
| `wrap_mul` | `(a: Int, b: Int, bits: Int) -> Int` | Wrapping multiplication (fixed-width) |
| `rotate_right` | `(a: Int, n: Int, bits: Int) -> Int` | Bitwise rotate right (fixed-width) |
| `rotate_left` | `(a: Int, n: Int, bits: Int) -> Int` | Bitwise rotate left (fixed-width) |
| `to_u32` | `(a: Int) -> Int` | Mask to unsigned 32-bit range |
| `to_u8` | `(a: Int) -> Int` | Mask to unsigned 8-bit range |

> Verified by `exercises/stdlib-test/bitwise_test.almd`, `stdlib-test.almd`.

---

## 5. float

Floating-point operations. All are pure.

| Function | Signature | Description |
|---|---|---|
| `to_string` | `(n: Float) -> String` | Convert float to string |
| `to_int` | `(n: Float) -> Int` | Truncate float to integer |
| `from_int` | `(n: Int) -> Float` | Convert integer to float |
| `parse` | `(s: String) -> Result[Float, String]` | Parse string as float |
| `round` | `(n: Float) -> Float` | Round to nearest integer |
| `floor` | `(n: Float) -> Float` | Round down |
| `ceil` | `(n: Float) -> Float` | Round up |
| `abs` | `(n: Float) -> Float` | Absolute value |
| `sqrt` | `(n: Float) -> Float` | Square root |
| `min` | `(a: Float, b: Float) -> Float` | Minimum of two floats |
| `max` | `(a: Float, b: Float) -> Float` | Maximum of two floats |
| `clamp` | `(n: Float, lo: Float, hi: Float) -> Float` | Clamp value to range [lo, hi] |

> Verified by `exercises/stdlib-test/bmi_test.almd`, `stdlib-test.almd`.

---

## 6. math

Mathematical functions. All are pure.

| Function | Signature | Description |
|---|---|---|
| `min` | `(a: Int, b: Int) -> Int` | Minimum of two integers |
| `max` | `(a: Int, b: Int) -> Int` | Maximum of two integers |
| `abs` | `(n: Int) -> Int` | Absolute value |
| `pow` | `(base: Int, exp: Int) -> Int` | Integer exponentiation |
| `pi` | `() -> Float` | Pi constant (3.14159...) |
| `e` | `() -> Float` | Euler's number (2.71828...) |
| `sin` | `(x: Float) -> Float` | Sine |
| `cos` | `(x: Float) -> Float` | Cosine |
| `tan` | `(x: Float) -> Float` | Tangent |
| `log` | `(x: Float) -> Float` | Natural logarithm |
| `exp` | `(x: Float) -> Float` | Exponential (e^x) |
| `sqrt` | `(x: Float) -> Float` | Square root |

> Verified by `exercises/stdlib-test/math_random_time_test.almd`.

---

## 7. fs

Filesystem operations. All are `effect` functions.

| Function | Signature | Description |
|---|---|---|
| `read_text` | `(path: String) -> Result[String, IoError]` | Read file as text |
| `read_bytes` | `(path: String) -> Result[List[Int], IoError]` | Read file as byte list |
| `read_lines` | `(path: String) -> Result[List[String], IoError]` | Read file as list of lines |
| `write` | `(path: String, content: String) -> Result[Unit, IoError]` | Write text to file |
| `write_bytes` | `(path: String, bytes: List[Int]) -> Result[Unit, IoError]` | Write bytes to file |
| `append` | `(path: String, content: String) -> Result[Unit, IoError]` | Append text to file |
| `exists?` | `(path: String) -> Bool` | Check if path exists |
| `is_dir?` | `(path: String) -> Bool` | Check if path is a directory |
| `is_file?` | `(path: String) -> Bool` | Check if path is a file |
| `mkdir_p` | `(path: String) -> Result[Unit, IoError]` | Create directory (and parents) |
| `remove` | `(path: String) -> Result[Unit, IoError]` | Remove file or empty directory |
| `copy` | `(src: String, dst: String) -> Result[Unit, IoError]` | Copy file |
| `rename` | `(src: String, dst: String) -> Result[Unit, IoError]` | Rename/move file |
| `list_dir` | `(path: String) -> Result[List[String], IoError]` | List directory entries |
| `walk` | `(dir: String) -> Result[List[String], IoError]` | Recursively list all files |
| `stat` | `(path: String) -> Result[{size: Int, is_dir: Bool, is_file: Bool, modified: Int}, IoError]` | File metadata |

> Verified by `exercises/stdlib-test/fs_process_test.almd`, `fs_walk_stat_test.almd`.

---

## 8. process

Process execution. All are `effect` functions.

| Function | Signature | Description |
|---|---|---|
| `exec` | `(cmd: String, args: List[String]) -> Result[String, String]` | Run command and return stdout |
| `exec_status` | `(cmd: String, args: List[String]) -> Result[{code: Int, stdout: String, stderr: String}, String]` | Run command and return full result |
| `exit` | `(code: Int) -> Unit` | Exit process with status code |
| `stdin_lines` | `() -> Result[List[String], String]` | Read all lines from stdin |

> Verified by `exercises/stdlib-test/env_process_test.almd`, `fs_process_test.almd`.

---

## 9. env

Environment and system access. All are `effect` functions.

| Function | Signature | Description |
|---|---|---|
| `args` | `() -> List[String]` | Command-line arguments |
| `get` | `(name: String) -> Option[String]` | Get environment variable |
| `set` | `(name: String, value: String) -> Unit` | Set environment variable |
| `cwd` | `() -> Result[String, String]` | Current working directory |
| `unix_timestamp` | `() -> Int` | Current UNIX timestamp in seconds |
| `millis` | `() -> Int` | Current time in milliseconds |
| `sleep_ms` | `(ms: Int) -> Unit` | Sleep for given milliseconds |

> Verified by `exercises/stdlib-test/env_process_test.almd`.

---

## 10. io

Standard I/O operations. All are `effect` functions.

| Function | Signature | Description |
|---|---|---|
| `print` | `(s: String) -> Unit` | Print string to stdout (no newline) |
| `read_line` | `() -> String` | Read one line from stdin |
| `read_all` | `() -> String` | Read all of stdin |

Note: `println` and `eprintln` are built-in effect functions (not module-scoped).

> Verified by `exercises/stdlib-test/io_test.almd`.

---

## 11. json

JSON parsing and construction. All are pure except as noted.

| Function | Signature | Description |
|---|---|---|
| `parse` | `(text: String) -> Result[Json, String]` | Parse JSON string |
| `stringify` | `(j: Json) -> String` | Serialize to compact JSON string |
| `stringify_pretty` | `(j: Json) -> String` | Serialize to pretty-printed JSON |
| `get` | `(j: Json, key: String) -> Option[Json]` | Get value by key from JSON object |
| `get_string` | `(j: Json, key: String) -> Option[String]` | Get string value by key |
| `get_int` | `(j: Json, key: String) -> Option[Int]` | Get integer value by key |
| `get_float` | `(j: Json, key: String) -> Option[Float]` | Get float value by key |
| `get_bool` | `(j: Json, key: String) -> Option[Bool]` | Get boolean value by key |
| `get_array` | `(j: Json, key: String) -> Option[List[Json]]` | Get array value by key |
| `keys` | `(j: Json) -> List[String]` | List keys of JSON object |
| `to_string` | `(j: Json) -> Option[String]` | Extract string from Json value |
| `to_int` | `(j: Json) -> Option[Int]` | Extract integer from Json value |
| `from_string` | `(s: String) -> Json` | Wrap string as Json value |
| `from_int` | `(n: Int) -> Json` | Wrap integer as Json value |
| `from_float` | `(n: Float) -> Json` | Wrap float as Json value |
| `from_bool` | `(b: Bool) -> Json` | Wrap boolean as Json value |
| `from_map` | `(m: Map[String, Json]) -> Json` | Convert map to Json object |
| `from_entries` | `(entries: List[{String, Json}]) -> Json` | Build Json object from entries |
| `null` | `() -> Json` | Json null value |
| `array` | `(items: List[Json]) -> Json` | Build Json array |

> Verified by `exercises/stdlib-test/stdlib-test.almd`.

---

## 12. http

HTTP server and client. All are `effect` functions. Type signatures are handled directly in codegen (not in `lookup_sig`).

| Function | Signature | Description |
|---|---|---|
| `serve` | `(port: Int, handler: (HttpRequest) -> HttpResponse) -> Unit` | Start HTTP server on port |
| `response` | `(status: Int, body: String) -> HttpResponse` | Create HTTP response |
| `json` | `(status: Int, body: String) -> HttpResponse` | Create JSON HTTP response |
| `with_headers` | `(status: Int, body: String, headers: Map[String, String]) -> HttpResponse` | Create response with custom headers |
| `get` | `(url: String) -> Result[String, String]` | HTTP GET request |
| `post` | `(url: String, body: String) -> Result[String, String]` | HTTP POST request |

---

## 13. random

Random number generation. All are `effect` functions.

| Function | Signature | Description |
|---|---|---|
| `int` | `(min: Int, max: Int) -> Int` | Random integer in range [min, max] |
| `float` | `() -> Float` | Random float in [0, 1) |
| `choice` | `(xs: List[T]) -> Option[T]` | Random element from list |
| `shuffle` | `(xs: List[T]) -> List[T]` | Randomly shuffle list |

> Verified by `exercises/stdlib-test/math_random_time_test.almd`.

---

## 14. regex

Regular expression operations. All are pure.

| Function | Signature | Description |
|---|---|---|
| `match?` | `(pat: String, s: String) -> Bool` | Check if pattern matches anywhere in string |
| `full_match?` | `(pat: String, s: String) -> Bool` | Check if pattern matches entire string |
| `find` | `(pat: String, s: String) -> Option[String]` | Find first match |
| `find_all` | `(pat: String, s: String) -> List[String]` | Find all matches |
| `replace` | `(pat: String, s: String, rep: String) -> String` | Replace all matches |
| `replace_first` | `(pat: String, s: String, rep: String) -> String` | Replace first match |
| `split` | `(pat: String, s: String) -> List[String]` | Split by pattern |
| `captures` | `(pat: String, s: String) -> Option[List[String]]` | Extract capture groups |

> Verified by `exercises/stdlib-test/regex_test.almd`.

---

## 15. encoding

Hex and Base64 encoding/decoding. Bundled stdlib module (`stdlib/encoding.almd`). All are pure.

| Function | Signature | Description |
|---|---|---|
| `hex_encode` | `(bytes: List[Int]) -> String` | Encode byte list as hex string |
| `hex_decode` | `(s: String) -> List[Int]` | Decode hex string to byte list |
| `hex_encode_string` | `(s: String) -> String` | Hex-encode a string's bytes |
| `hex_decode_string` | `(s: String) -> String` | Decode hex string back to string |
| `base64_encode` | `(bytes: List[Int]) -> String` | Encode byte list as Base64 |
| `base64_decode` | `(s: String) -> List[Int]` | Decode Base64 to byte list |
| `base64_encode_string` | `(s: String) -> String` | Base64-encode a string |
| `base64_decode_string` | `(s: String) -> String` | Decode Base64 back to string |

Internal helpers (`hex_char_val`, `b64_char_val`) are not exported.

> Verified by `exercises/stdlib-test/encoding_test.almd`.

---

## 16. hash

Cryptographic hash functions. Bundled stdlib module (`stdlib/hash.almd`). All are pure.

| Function | Signature | Description |
|---|---|---|
| `sha256` | `(data: String) -> String` | SHA-256 hash (hex digest) |
| `sha1` | `(data: String) -> String` | SHA-1 hash (hex digest) |
| `md5` | `(data: String) -> String` | MD5 hash (hex digest) |

All internal helpers (padding, block processing, byte conversion) use `local fn` visibility and are not exported.

> Verified by `exercises/stdlib-test/hash_test.almd`.

---

## 17. path

File path manipulation. Bundled stdlib module (`stdlib/path.almd`). All are pure.

| Function | Signature | Description |
|---|---|---|
| `join` | `(base: String, child: String) -> String` | Join two path segments |
| `dirname` | `(p: String) -> String` | Parent directory |
| `basename` | `(p: String) -> String` | File name component |
| `extension` | `(p: String) -> Option[String]` | File extension (without dot) |
| `stem` | `(p: String) -> String` | File name without extension |
| `is_absolute?` | `(p: String) -> Bool` | Check if path is absolute |
| `normalize` | `(p: String) -> String` | Resolve `.` and `..` segments |

> Verified by `exercises/stdlib-test/path_test.almd`, `path_v2_test.almd`.

---

## 18. time

Date/time operations. Bundled stdlib module (`stdlib/time.almd`). Uses `env.unix_timestamp`, `env.millis`, `env.sleep_ms` internally.

### Types

```
type TimeParts = {year: Int, month: Int, day: Int, hour: Int, minute: Int, second: Int}
```

### Functions

| Function | Signature | Description |
|---|---|---|
| `now` | `() -> Int` | Current UNIX timestamp (effect) |
| `millis` | `() -> Int` | Current time in milliseconds (effect) |
| `sleep` | `(ms: Int) -> Unit` | Sleep for milliseconds (effect) |
| `time_parts` | `(ts: Int) -> TimeParts` | Decompose timestamp to date/time parts (UTC) |
| `year` | `(ts: Int) -> Int` | Extract year from timestamp |
| `month` | `(ts: Int) -> Int` | Extract month (1-12) from timestamp |
| `day` | `(ts: Int) -> Int` | Extract day (1-31) from timestamp |
| `hour` | `(ts: Int) -> Int` | Extract hour (0-23) from timestamp |
| `minute` | `(ts: Int) -> Int` | Extract minute (0-59) from timestamp |
| `second` | `(ts: Int) -> Int` | Extract second (0-59) from timestamp |
| `weekday` | `(ts: Int) -> Int` | Day of week (0=Monday, 6=Sunday) |
| `to_iso` | `(ts: Int) -> String` | Format as ISO 8601 string (UTC) |
| `from_parts` | `(y: Int, m: Int, d: Int, h: Int, min: Int, s: Int) -> Int` | Construct timestamp from parts |

> Verified by `exercises/stdlib-test/math_random_time_test.almd`.

---

## 19. term

ANSI terminal formatting. Bundled stdlib module (`stdlib/term.almd`). All are pure.

### Foreground Colors

| Function | Signature | Description |
|---|---|---|
| `red` | `(s: String) -> String` | Red text |
| `green` | `(s: String) -> String` | Green text |
| `yellow` | `(s: String) -> String` | Yellow text |
| `blue` | `(s: String) -> String` | Blue text |
| `magenta` | `(s: String) -> String` | Magenta text |
| `cyan` | `(s: String) -> String` | Cyan text |
| `white` | `(s: String) -> String` | White text |
| `gray` | `(s: String) -> String` | Gray text |

### Background Colors

| Function | Signature | Description |
|---|---|---|
| `bg_red` | `(s: String) -> String` | Red background |
| `bg_green` | `(s: String) -> String` | Green background |
| `bg_yellow` | `(s: String) -> String` | Yellow background |
| `bg_blue` | `(s: String) -> String` | Blue background |

### Styles

| Function | Signature | Description |
|---|---|---|
| `bold` | `(s: String) -> String` | Bold text |
| `dim` | `(s: String) -> String` | Dim text |
| `italic` | `(s: String) -> String` | Italic text |
| `underline` | `(s: String) -> String` | Underlined text |
| `strikethrough` | `(s: String) -> String` | Strikethrough text |

### 256-Color and Utility

| Function | Signature | Description |
|---|---|---|
| `color` | `(s: String, code: Int) -> String` | Apply 256-color foreground |
| `bg_color` | `(s: String, code: Int) -> String` | Apply 256-color background |
| `reset` | `() -> String` | ANSI reset sequence |
| `strip` | `(s: String) -> String` | Remove all ANSI escape sequences |

> Verified by `exercises/stdlib-test/term_test.almd`.

---

## 20. args

Command-line argument parsing. Bundled stdlib module (`stdlib/args.almd`). All are `effect` (call `env.args()` internally).

| Function | Signature | Description |
|---|---|---|
| `raw` | `() -> List[String]` | All raw arguments (including program name) |
| `flag?` | `(name: String) -> Bool` | Check if `--name` or `-n` flag is present |
| `option` | `(name: String) -> Option[String]` | Get value of `--name value` or `--name=value` |
| `option_or` | `(name: String, fallback: String) -> String` | Get option value or fallback |
| `positional` | `() -> List[String]` | Non-flag arguments (excluding program name) |
| `positional_at` | `(i: Int) -> Option[String]` | Get positional argument at index |

> Verified by `exercises/stdlib-test/args_test.almd`.

---

## Test Reference

All behaviors above are verified by executable tests:

| File | Covers |
|---|---|
| `exercises/stdlib-test/stdlib-test.almd` | string, list, map, json, int, float core functions |
| `exercises/stdlib-test/stdlib_v2_test.almd` | string, list, map extended functions |
| `exercises/stdlib-test/stdlib_phase5_test.almd` | Additional list/string operations |
| `exercises/stdlib-test/stdlib_phase6_test.almd` | map.filter, map.from_entries, list.group_by, etc. |
| `exercises/stdlib-test/ufcs_test.almd` | UFCS dot syntax for string, list, map |
| `exercises/stdlib-test/bitwise_test.almd` | int bitwise operations |
| `exercises/stdlib-test/fs_process_test.almd` | fs, process |
| `exercises/stdlib-test/fs_walk_stat_test.almd` | fs.walk, fs.stat |
| `exercises/stdlib-test/env_process_test.almd` | env, process |
| `exercises/stdlib-test/math_random_time_test.almd` | math, random, time |
| `exercises/stdlib-test/regex_test.almd` | regex |
| `exercises/stdlib-test/io_test.almd` | io |
| `exercises/stdlib-test/encoding_test.almd` | encoding |
| `exercises/stdlib-test/hash_test.almd` | hash |
| `exercises/stdlib-test/path_test.almd` | path |
| `exercises/stdlib-test/path_v2_test.almd` | path (normalize, stem, is_absolute?) |
| `exercises/stdlib-test/term_test.almd` | term |
| `exercises/stdlib-test/args_test.almd` | args |
