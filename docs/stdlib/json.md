# json

The JSON **format**: parsing and printing of `Value` trees. import json.

The data model itself lives on `value.*` (auto-imported) — constructors
(`value.null` / `value.int` / `value.string` / `value.object` / …) and
accessors (`value.as_*` / `value.get` / `value.as_object`) and paths
(`value.root` / `value.key` / `value.get_path`) — see
[value.md](./value.md). The old `json.*` spellings of those operations were retired aliases —
dropped after their one-release E040 deprecation window; the migration map
is in [Renamed operations](#renamed-operations) at the bottom.

A path built with `value.root` / `value.key` / `value.index` has type `value.Path`;
annotate helpers over paths with that type.

### `json.parse(text: String) -> Result[Value, String]`

Parse a JSON string into a Value.

```almd run
import json

fn main() -> Unit = {
  let v = json.parse("{\"name\": \"Alice\"}")
  match v {
    ok(j) => println(json.stringify(j)),
    err(e) => println("parse error: ${e}"),
  }
}
```
```output
{"name":"Alice"}
```

The grammar is RFC 8259 plus I-JSON (RFC 7493), read **strictly** (ALS-T3,
C-400 / C-401): the whole text is one value between JSON whitespace (space,
tab, LF, CR), and every other input is an `err` with one text per case, the
same on every target. `N` is the character index where the problem starts.

| Input | Error |
|---|---|
| `"ad\ud800min"` (a surrogate that is not a high+low pair) | `json: lone surrogate at pos N` |
| `"ad\qmin"` (an escape other than `\" \\ \/ \b \f \n \r \t \u`) | `json: unknown escape at pos N` |
| `"\u12"` (`\u` without four hex digits) | `json: invalid unicode escape at pos N` |
| a raw U+0000–U+001F inside a string | `json: control character in string at pos N` |
| U+FDD0–U+FDEF, U+xFFFE, U+xFFFF (escaped or raw) | `json: noncharacter at pos N` |
| `"abc` | `json: unterminated string at pos N` |
| `{"a":1,"a":2}` | `json: duplicate key at pos N` |
| `[1,2] [3]`, `{} garbage` | `json: trailing data at pos N` |
| `{x: 1}` | `json: unquoted key at pos N` |
| `[1,]`, `{"a":1,}` | `json: trailing comma at pos N` |
| `{"a" 1}` | `json: expected ':' at pos N` |
| `[1 2]` / `{"a":1 "b":2}` | `json: expected ',' or ']' at pos N` / `json: expected ',' or '}' at pos N` |
| `01`, `1.`, `-`, `1e` | `json: invalid number at pos N` |

Nothing is dropped or guessed: a lone surrogate or an unknown escape is never
deleted (`ad\ud800min` does not become `admin`), and a repeated key is never
resolved first-wins or last-wins, so a parser in front of the program cannot
read a different document from the same bytes.

```almd run
import json

fn main() -> Unit = {
  for text in ["{\"user\":\"ad\\ud800min\"}", "{\"user\":\"alice\",\"user\":\"admin\"}", "[1,2] [3]"] {
    match json.parse(text) {
      ok(j) => println(json.stringify(j)),
      err(e) => println(e),
    }
  }
}
```
```output
json: lone surrogate at pos 11
json: duplicate key at pos 16
json: trailing data at pos 6
```

### `json.stringify(v: Value) -> String`

Convert a Value to a JSON string. A non-finite Float (NaN, +inf, -inf) has no
JSON spelling and is written as `null`, so the output always parses back; the
same rule holds for `json.stringify_pretty`.

```almd run
import json

type Person: Codec = { name: String, age: Int }

fn main() -> Unit = {
  let person = Person { name: "Alice", age: 30 }
  println(json.stringify(person.encode()))
}
```
```output
{"name":"Alice","age":30}
```

### Typed decode: `T.decode` and `T.decode_strict`

A `: Codec` record type `T` derives `T.decode` and `T.decode_strict` (and
`T.encode`). `T.decode` ignores keys it does not know; `T.decode_strict`
refuses the first one with `unknown key '<k>'` before reading any field. A
field marked `@skip_decode` is never read from input (it keeps its default)
and is still encoded; `@skip` is neither read nor written. Untrusted input is
best decoded into a type holding only what the client may set (CHEATSHEET,
JSON Rule 5).

```almd run
import json

type User: Codec = { name: String, @skip_decode is_admin: Bool = false }

fn main() -> Unit = {
  let body = json.parse("{\"name\": \"m\", \"is_admin\": true}") ?? value.null()
  match User.decode(body) {
    ok(u) => println("${u.name} admin=${u.is_admin}"),
    err(e) => println(e),
  }
  match User.decode_strict(body) {
    ok(u) => println("${u.name} admin=${u.is_admin}"),
    err(e) => println(e),
  }
}
```
```output
m admin=false
unknown key 'is_admin'
```

### `json.stringify_pretty(j: Value) -> String`

Convert a Json value to a pretty-printed JSON string with indentation. A non-finite
Float is written as `null`, as in `json.stringify`.

```almd run
import json

fn main() -> Unit = {
  let j = json.parse("{\"name\": \"Alice\", \"tags\": [\"a\", \"b\"]}") ?? value.null()
  println(json.stringify_pretty(j))
}
```
```output
{
  "name": "Alice",
  "tags": [
    "a",
    "b"
  ]
}
```

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (3 functions)

```
// Value or err; lenient, trailing text ignored; nesting over 1000 is err.
// @since 0.5.0 or earlier
json.parse(text: String) -> Result[Value, String]

// Compact JSON; a Float 1.0 prints as 1, NaN/inf as null.
// @since 0.5.0 or earlier
json.stringify(v: Value) -> String

// JSON with 2-space indent; [] and {} inline, NaN/inf null.
// @since 0.5.0 or earlier
json.stringify_pretty(v: Value) -> String
```

<!-- END GENERATED SIGNATURE INDEX -->

## Renamed operations

Dropped in #1078 after a one-release deprecation window (E040, v0.53.1 –
v0.53.4). A retired spelling is an ordinary E002 undefined-function error now.

| retired | survivor |
|---|---|
| `json.null` / `json.object` / `json.array` | `value.null` / `value.object` / `value.array` |
| `json.from_string` / `json.from_int` / `json.from_bool` / `json.from_float` | `value.string` / `value.int` / `value.bool` / `value.float` |
| `json.as_string` … `json.as_array` (returned `Option`) | `value.as_string(v)?` … `value.as_array(v)?` — `?` converts the `Result` to the same `Option` |
| `json.get(j, k)` (returned `Option`) | `value.get(j, k)` |

Note one behavioral tightening: the dropped `json.as_int` widened a Float
value to Int; `value.as_int` does not (`err("expected Int")`) — the widening
direction lives on `value.as_float`, which accepts an Int.
