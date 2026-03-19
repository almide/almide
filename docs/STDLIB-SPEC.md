# Almide Standard Library Specification

Auto-generated from `stdlib/defs/*.toml`. 381 native functions across 22 modules.
Runtime implementation: 381/381 (100%).

## Module Index

### Native Modules (TOML-defined)

| Module | Layer | Functions | Implemented | Status |
|--------|-------|-----------|-------------|--------|
| crypto | platform | 4 | 4/4 | Ready |
| datetime | platform | 21 | 21/21 | Ready |
| env | platform | 9 | 9/9 | Ready |
| error | core | 3 | 3/3 | Ready |
| float | core | 16 | 16/16 | Ready |
| fs | platform | 24 | 24/24 | Ready |
| http | platform | 26 | 4/26 | Partial (4/26) |
| int | core | 21 | 21/21 | Ready |
| io | platform | 3 | 3/3 | Ready |
| json | core | 36 | 36/36 | Ready |
| list | core | 54 | 54/54 | Ready |
| log | core | 8 | 8/8 | Ready |
| map | core | 16 | 16/16 | Ready |
| math | core | 21 | 21/21 | Ready |
| process | platform | 6 | 6/6 | Ready |
| random | platform | 4 | 4/4 | Ready |
| regex | core | 8 | 0/8 | TOML only |
| result | core | 9 | 9/9 | Ready |
| string | core | 41 | 41/41 | Ready |
| testing | core | 7 | 7/7 | Ready |
| uuid | platform | 6 | 6/6 | Ready |
| value | core | 19 | 19/19 | Ready |

### Bundled Modules (pure Almide)

| Module | Functions |
|--------|-----------|
| args | 6 |
| compress | 4 |
| csv | 9 |
| encoding | 10 |
| hash | 3 |
| path | 7 |
| term | 21 |
| time | 20 |
| toml | 14 |
| url | 21 |
| value | 17 |

---

## crypto

Layer: **platform** | 4 functions | 4/4 implemented

### `crypto.random_bytes`

Generate n cryptographically secure random bytes.

```
effect random_bytes(n: Int) -> Result[List[Int], String]
```

Example: `crypto.random_bytes(16) // => ok([42, 17, ...])`

### `crypto.random_hex`

Generate a random hex string of n bytes (2n hex chars).

```
effect random_hex(n: Int) -> Result[String, String]
```

Example: `crypto.random_hex(8) // => ok("a1b2c3d4e5f6a7b8")`

### `crypto.hmac_sha256`

Compute HMAC-SHA256 of data with a key, returning hex digest.

```
effect hmac_sha256(key: String, data: String) -> Result[String, String]
```

Example: `crypto.hmac_sha256("secret", "message") // => ok("...")`

### `crypto.hmac_verify`

Verify an HMAC-SHA256 signature.

```
effect hmac_verify(key: String, data: String, signature: String) -> Result[Bool, String]
```

Example: `crypto.hmac_verify("secret", "message", sig) // => ok(true)`

## datetime

Layer: **platform** | 21 functions | 21/21 implemented

### `datetime.now`

Get the current time as a Unix timestamp (seconds, UTC).

```
effect now() -> Int
```

Example: `let ts = datetime.now()`

### `datetime.from_parts`

Create a timestamp from year, month, day, hour, minute, second (UTC).

```
from_parts(y: Int, m: Int, d: Int, h: Int, min: Int, s: Int) -> Int
```

Example: `datetime.from_parts(2024, 1, 15, 12, 0, 0)`

### `datetime.parse_iso`

Parse an ISO 8601 date string into a timestamp.

```
parse_iso(s: String) -> Result[Int, String]
```

Example: `datetime.parse_iso("2024-01-15T12:00:00Z") // => ok(1705320000)`

### `datetime.from_unix`

Convert a Unix timestamp (identity function for documentation clarity).

```
from_unix(seconds: Int) -> Int
```

Example: `datetime.from_unix(1705320000)`

### `datetime.format`

Format a timestamp using a pattern string.

```
format(ts: Int, pattern: String) -> String
```

Example: `datetime.format(ts, "%Y-%m-%d") // => "2024-01-15"`

### `datetime.to_iso`

Format a timestamp as ISO 8601 string.

```
to_iso(ts: Int) -> String
```

Example: `datetime.to_iso(1705320000) // => "2024-01-15T12:00:00Z"`

### `datetime.to_unix`

Get the Unix timestamp value (identity function).

```
to_unix(ts: Int) -> Int
```

Example: `datetime.to_unix(ts) // => 1705320000`

### `datetime.year`

Extract the year from a timestamp.

```
year(ts: Int) -> Int
```

Example: `datetime.year(ts) // => 2024`

### `datetime.month`

Extract the month (1-12) from a timestamp.

```
month(ts: Int) -> Int
```

Example: `datetime.month(ts) // => 1`

### `datetime.day`

Extract the day of month (1-31) from a timestamp.

```
day(ts: Int) -> Int
```

Example: `datetime.day(ts) // => 15`

### `datetime.hour`

Extract the hour (0-23) from a timestamp.

```
hour(ts: Int) -> Int
```

Example: `datetime.hour(ts) // => 12`

### `datetime.minute`

Extract the minute (0-59) from a timestamp.

```
minute(ts: Int) -> Int
```

Example: `datetime.minute(ts) // => 30`

### `datetime.second`

Extract the second (0-59) from a timestamp.

```
second(ts: Int) -> Int
```

Example: `datetime.second(ts) // => 45`

### `datetime.weekday`

Get the day of week as a string (Monday-Sunday).

```
weekday(ts: Int) -> String
```

Example: `datetime.weekday(ts) // => "Monday"`

### `datetime.add_days`

Add n days to a timestamp.

```
add_days(ts: Int, n: Int) -> Int
```

Example: `datetime.add_days(ts, 7) // one week later`

### `datetime.add_hours`

Add n hours to a timestamp.

```
add_hours(ts: Int, n: Int) -> Int
```

Example: `datetime.add_hours(ts, 3)`

### `datetime.add_minutes`

Add n minutes to a timestamp.

```
add_minutes(ts: Int, n: Int) -> Int
```

Example: `datetime.add_minutes(ts, 30)`

### `datetime.add_seconds`

Add n seconds to a timestamp.

```
add_seconds(ts: Int, n: Int) -> Int
```

Example: `datetime.add_seconds(ts, 90)`

### `datetime.diff_seconds`

Compute the difference in seconds between two timestamps.

```
diff_seconds(a: Int, b: Int) -> Int
```

Example: `datetime.diff_seconds(later, earlier) // => 3600`

### `datetime.is_before`

Check if timestamp a is before timestamp b.

```
is_before(a: Int, b: Int) -> Bool
```

Example: `datetime.is_before(earlier, later) // => true`

### `datetime.is_after`

Check if timestamp a is after timestamp b.

```
is_after(a: Int, b: Int) -> Bool
```

Example: `datetime.is_after(later, earlier) // => true`

## env

Layer: **platform** | 9 functions | 9/9 implemented

### `env.unix_timestamp`

Get the current Unix timestamp in seconds.

```
effect unix_timestamp() -> Int
```

Example: `let ts = env.unix_timestamp()`

### `env.args`

Get the command-line arguments as a list of strings.

```
effect args() -> List[String]
```

Example: `let args = env.args()`

### `env.get`

Get the value of an environment variable, or none if not set.

```
effect get(name: String) -> Option[String]
```

Example: `env.get("HOME") // => some("/Users/alice")`

### `env.set`

Set an environment variable.

```
effect set(name: String, value: String) -> Unit
```

Example: `env.set("MY_VAR", "hello")`

### `env.cwd`

Get the current working directory.

```
effect cwd() -> Result[String, String]
```

Example: `let dir = env.cwd()`

### `env.millis`

Get the current time in milliseconds since epoch.

```
effect millis() -> Int
```

Example: `let ms = env.millis()`

### `env.sleep_ms`

Sleep for the given number of milliseconds.

```
effect sleep_ms(ms: Int) -> Unit
```

Example: `env.sleep_ms(1000) // sleep 1 second`

### `env.temp_dir`

Get the system temporary directory path.

```
effect temp_dir() -> String
```

Example: `let tmp = env.temp_dir()`

### `env.os`

Get the operating system name (linux, macos, windows).

```
os() -> String
```

Example: `env.os() // => "macos"`

## error

Layer: **core** | 3 functions | 3/3 implemented

### `error.context`

Add context message to an error result.

```
context[T, E](r: Result[T, E], msg: String) -> Result[T, String]
```

Example: `error.context(result, "failed to load config")`

### `error.message`

Extract the error message from a Result, or empty string if ok.

```
message[T](r: Result[T, String]) -> String
```

Example: `error.message(err("oops")) // => "oops"`

### `error.chain`

Chain two error messages with a cause separator.

```
chain(outer: String, cause: String) -> String
```

Example: `error.chain("load failed", "file not found") // => "load failed: file not found"`

## float

Layer: **core** | 16 functions | 16/16 implemented

### `float.to_string`

Convert a float to its string representation.

```
to_string(n: Float) -> String
```

Example: `float.to_string(3.14) // => "3.14"`

### `float.to_int`

Truncate a float to an integer (rounds toward zero).

```
to_int(n: Float) -> Int
```

Example: `float.to_int(3.9) // => 3`

### `float.round`

Round a float to the nearest integer value (as Float).

```
round(n: Float) -> Float
```

Example: `float.round(3.6) // => 4.0`

### `float.floor`

Round a float down to the nearest integer value (as Float).

```
floor(n: Float) -> Float
```

Example: `float.floor(3.9) // => 3.0`

### `float.ceil`

Round a float up to the nearest integer value (as Float).

```
ceil(n: Float) -> Float
```

Example: `float.ceil(3.1) // => 4.0`

### `float.abs`

Return the absolute value of a float.

```
abs(n: Float) -> Float
```

Example: `float.abs(-2.5) // => 2.5`

### `float.sqrt`

Return the square root of a float.

```
sqrt(n: Float) -> Float
```

Example: `float.sqrt(9.0) // => 3.0`

### `float.parse`

Parse a string into a float. Returns err if the string is not a valid number.

```
parse(s: String) -> Result[Float, String]
```

Example: `float.parse("3.14") // => ok(3.14)`

### `float.from_int`

Convert an integer to a float.

```
from_int(n: Int) -> Float
```

Example: `float.from_int(42) // => 42.0`

### `float.min`

Return the smaller of two floats.

```
min(a: Float, b: Float) -> Float
```

Example: `float.min(1.5, 2.5) // => 1.5`

### `float.max`

Return the larger of two floats.

```
max(a: Float, b: Float) -> Float
```

Example: `float.max(1.5, 2.5) // => 2.5`

### `float.to_fixed`

Format a float with a fixed number of decimal places.

```
to_fixed(n: Float, decimals: Int) -> String
```

Example: `float.to_fixed(3.14159, 2) // => "3.14"`

### `float.clamp`

Clamp a float to the range [lo, hi].

```
clamp(n: Float, lo: Float, hi: Float) -> Float
```

Example: `float.clamp(15.0, 0.0, 10.0) // => 10.0`

### `float.sign`

Return the sign of a float: -1.0, 0.0, or 1.0.

```
sign(n: Float) -> Float
```

Example: `float.sign(-3.5) // => -1.0`

### `float.is_nan`

Check if a float is NaN (not a number).

```
is_nan(n: Float) -> Bool
```

Example: `float.is_nan(0.0 / 0.0) // => true`

### `float.is_infinite`

Check if a float is positive or negative infinity.

```
is_infinite(n: Float) -> Bool
```

Example: `float.is_infinite(1.0 / 0.0) // => true`

## fs

Layer: **platform** | 24 functions | 24/24 implemented

### `fs.read_text`

Read file contents as a UTF-8 string

```
effect read_text(path: String) -> Result[String, String]
```

Example: `let text = fs.read_text("config.toml")`

### `fs.read_bytes`

Read file contents as a list of bytes

```
effect read_bytes(path: String) -> Result[List[Int], String]
```

Example: `let bytes = fs.read_bytes("image.png")`

### `fs.write`

Write a string to a file, creating or overwriting it

```
effect write(path: String, content: String) -> Result[Unit, String]
```

Example: `fs.write("output.txt", "hello")`

### `fs.write_bytes`

Write a list of bytes to a file

```
effect write_bytes(path: String, bytes: List[Int]) -> Result[Unit, String]
```

Example: `fs.write_bytes("out.bin", [0, 1, 2])`

### `fs.append`

Append a string to a file, creating it if it doesn't exist

```
effect append(path: String, content: String) -> Result[Unit, String]
```

Example: `fs.append("log.txt", "new line\n")`

### `fs.mkdir_p`

Create a directory and all parent directories

```
effect mkdir_p(path: String) -> Result[Unit, String]
```

Example: `fs.mkdir_p("data/cache/images")`

### `fs.exists`

Check if a file or directory exists

```
effect exists(path: String) -> Bool
```

Example: `if fs.exists("config.toml") then ...`

### `fs.read_lines`

Read a file as a list of lines

```
effect read_lines(path: String) -> Result[List[String], String]
```

Example: `let lines = fs.read_lines("data.csv")`

### `fs.remove`

Delete a file

```
effect remove(path: String) -> Result[Unit, String]
```

Example: `fs.remove("temp.txt")`

### `fs.list_dir`

List entries in a directory

```
effect list_dir(path: String) -> Result[List[String], String]
```

Example: `let entries = fs.list_dir("src/")`

### `fs.is_dir`

Check if a path is a directory

```
effect is_dir(path: String) -> Bool
```

Example: `if fs.is_dir("src") then ...`

### `fs.is_file`

Check if a path is a regular file

```
effect is_file(path: String) -> Bool
```

Example: `if fs.is_file("readme.md") then ...`

### `fs.copy`

Copy a file from src to dst

```
effect copy(src: String, dst: String) -> Result[Unit, String]
```

Example: `fs.copy("a.txt", "b.txt")`

### `fs.rename`

Rename or move a file

```
effect rename(src: String, dst: String) -> Result[Unit, String]
```

Example: `fs.rename("old.txt", "new.txt")`

### `fs.walk`

Recursively list all files in a directory tree

```
effect walk(dir: String) -> Result[List[String], String]
```

Example: `let all_files = fs.walk("src/")`

### `fs.remove_all`

Recursively delete a directory and all its contents

```
effect remove_all(path: String) -> Result[Unit, String]
```

Example: `fs.remove_all("build/")`

### `fs.file_size`

Get file size in bytes

```
effect file_size(path: String) -> Result[Int, String]
```

Example: `let size = fs.file_size("data.bin")`

### `fs.temp_dir`

Get the system temporary directory path

```
temp_dir() -> String
```

Example: `let tmp = fs.temp_dir()`

### `fs.stat`

Get file metadata: size, type, and modification time

```
effect stat(path: String) -> Result[{size: Int, is_dir: Bool, is_file: Bool, modified: Int}, String]
```

Example: `let info = fs.stat("file.txt") // {size, is_dir, is_file, modified}`

### `fs.glob`

Find files matching a glob pattern

```
effect glob(pattern: String) -> Result[List[String], String]
```

Example: `let files = fs.glob("src/**/*.almd")`

### `fs.create_temp_file`

Create a temporary file with a given prefix, return its path

```
effect create_temp_file(prefix: String) -> Result[String, String]
```

Example: `let path = fs.create_temp_file("almide-")`

### `fs.create_temp_dir`

Create a temporary directory with a given prefix, return its path

```
effect create_temp_dir(prefix: String) -> Result[String, String]
```

Example: `let dir = fs.create_temp_dir("build-")`

### `fs.is_symlink`

Check if a path is a symbolic link

```
effect is_symlink(path: String) -> Bool
```

Example: `if fs.is_symlink("link") then ...`

### `fs.modified_at`

Get file modification time as Unix timestamp (seconds)

```
effect modified_at(path: String) -> Result[Int, String]
```

Example: `let ts = fs.modified_at("file.txt")`

## http

Layer: **platform** | 26 functions | 4/26 implemented

### `http.serve` (not implemented)

Start an HTTP server on the given port with a request handler

```
effect serve(port: Int, f: Fn[Unknown] -> Unknown) -> Unit
```

Example: `http.serve(3000, (req) => http.response(200, "ok"))`

### `http.response`

Create a plain text HTTP response with status code

```
response(status: Int, body: String) -> Response
```

Example: `http.response(200, "Hello!")`

### `http.json`

Create a JSON HTTP response with status code

```
json(status: Int, body: String) -> Response
```

Example: `http.json(200, json.stringify(data))`

### `http.with_headers`

Create a response with custom headers

```
with_headers(status: Int, body: String, headers: Map[String, String]) -> Response
```

Example: `http.with_headers(200, body, {"Content-Type": "text/html"})`

### `http.redirect` (not implemented)

Create a 302 temporary redirect response

```
redirect(url: String) -> Response
```

Example: `http.redirect("/new-path")`

### `http.redirect_permanent` (not implemented)

Create a 301 permanent redirect response

```
redirect_permanent(url: String) -> Response
```

Example: `http.redirect_permanent("/new-path")`

### `http.not_found`

Create a 404 Not Found response

```
not_found(body: String) -> Response
```

Example: `http.not_found("Page not found")`

### `http.status` (not implemented)

Set the status code on a response

```
status(resp: Response, code: Int) -> Response
```

Example: `http.status(resp, 201)`

### `http.body` (not implemented)

Get the body string from a response

```
body(resp: Response) -> String
```

Example: `let text = http.body(resp)`

### `http.set_header` (not implemented)

Set a header on a response

```
set_header(resp: Response, key: String, value: String) -> Response
```

Example: `http.set_header(resp, "X-Custom", "value")`

### `http.get_header` (not implemented)

Get a header value from a response

```
get_header(resp: Response, key: String) -> Option[String]
```

Example: `let ct = http.get_header(resp, "Content-Type")`

### `http.set_cookie` (not implemented)

Set a cookie on a response

```
set_cookie(resp: Response, name: String, value: String) -> Response
```

Example: `http.set_cookie(resp, "session", "abc123")`

### `http.req_method` (not implemented)

Get the HTTP method of a request (GET, POST, etc.)

```
req_method(req: Request) -> String
```

Example: `let method = http.req_method(req)`

### `http.req_path` (not implemented)

Get the URL path of a request

```
req_path(req: Request) -> String
```

Example: `let path = http.req_path(req)`

### `http.req_body` (not implemented)

Get the body string of a request

```
req_body(req: Request) -> String
```

Example: `let body = http.req_body(req)`

### `http.req_header` (not implemented)

Get a header value from a request

```
req_header(req: Request, key: String) -> Option[String]
```

Example: `let auth = http.req_header(req, "Authorization")`

### `http.query_params` (not implemented)

Get all query parameters from a request as a map

```
query_params(req: Request) -> Map[String, String]
```

Example: `let params = http.query_params(req) // {"page": "1", "q": "test"}`

### `http.get` (not implemented)

Send an HTTP GET request and return the response body

```
effect get(url: String) -> Result[String, String]
```

Example: `let html = http.get("https://example.com")`

### `http.get_json` (not implemented)

Send a GET request and parse the response as JSON

```
effect get_json(url: String) -> Result[Json, String]
```

Example: `let data = http.get_json("https://api.example.com/users")`

### `http.post` (not implemented)

Send an HTTP POST request with a body string

```
effect post(url: String, body: String) -> Result[String, String]
```

Example: `let resp = http.post("https://api.example.com", body)`

### `http.post_json` (not implemented)

Send a POST request and parse the response as JSON

```
effect post_json(url: String, body: String) -> Result[Json, String]
```

Example: `let result = http.post_json(url, json.stringify(payload))`

### `http.put` (not implemented)

Send an HTTP PUT request

```
effect put(url: String, body: String) -> Result[String, String]
```

Example: `let resp = http.put(url, body)`

### `http.patch` (not implemented)

Send an HTTP PATCH request

```
effect patch(url: String, body: String) -> Result[String, String]
```

Example: `let resp = http.patch(url, body)`

### `http.delete` (not implemented)

Send an HTTP DELETE request

```
effect delete(url: String) -> Result[String, String]
```

Example: `let resp = http.delete(url)`

### `http.get_with_headers` (not implemented)

Send a GET request with custom headers

```
effect get_with_headers(url: String, headers: Map[String, String]) -> Result[String, String]
```

Example: `let resp = http.get_with_headers(url, {"Authorization": "Bearer token"})`

### `http.request` (not implemented)

Send a custom HTTP request with method, URL, body, and headers

```
effect request(method: String, url: String, body: String, headers: Map[String, String]) -> Result[String, String]
```

Example: `let resp = http.request("PUT", url, body, headers)`

## int

Layer: **core** | 21 functions | 21/21 implemented

### `int.to_string`

Convert an integer to its decimal string representation.

```
to_string(n: Int) -> String
```

Example: `int.to_string(42) // => "42"`

### `int.to_hex`

Convert an integer to its hexadecimal string representation (lowercase).

```
to_hex(n: Int) -> String
```

Example: `int.to_hex(255) // => "ff"`

### `int.parse`

Parse a decimal string into an integer. Returns err if the string is not a valid integer.

```
parse(s: String) -> Result[Int, String]
```

Example: `int.parse("42") // => ok(42)`

### `int.parse_hex`

Parse a hexadecimal string into an integer. Returns err if the string is not valid hex.

```
parse_hex(s: String) -> Result[Int, String]
```

Example: `int.parse_hex("ff") // => ok(255)`

### `int.abs`

Return the absolute value of an integer.

```
abs(n: Int) -> Int
```

Example: `int.abs(-5) // => 5`

### `int.min`

Return the smaller of two integers.

```
min(a: Int, b: Int) -> Int
```

Example: `int.min(3, 7) // => 3`

### `int.max`

Return the larger of two integers.

```
max(a: Int, b: Int) -> Int
```

Example: `int.max(3, 7) // => 7`

### `int.band`

Bitwise AND of two integers.

```
band(a: Int, b: Int) -> Int
```

Example: `int.band(0b1100, 0b1010) // => 0b1000`

### `int.bor`

Bitwise OR of two integers.

```
bor(a: Int, b: Int) -> Int
```

Example: `int.bor(0b1100, 0b1010) // => 0b1110`

### `int.bxor`

Bitwise XOR of two integers.

```
bxor(a: Int, b: Int) -> Int
```

Example: `int.bxor(0b1100, 0b1010) // => 0b0110`

### `int.bshl`

Bitwise shift left.

```
bshl(a: Int, n: Int) -> Int
```

Example: `int.bshl(1, 3) // => 8`

### `int.bshr`

Bitwise shift right (arithmetic).

```
bshr(a: Int, n: Int) -> Int
```

Example: `int.bshr(8, 2) // => 2`

### `int.bnot`

Bitwise NOT (complement) of an integer.

```
bnot(a: Int) -> Int
```

Example: `int.bnot(0) // => -1`

### `int.wrap_add`

Wrapping addition within a given bit width. Overflow wraps around.

```
wrap_add(a: Int, b: Int, bits: Int) -> Int
```

Example: `int.wrap_add(255, 1, 8) // => 0`

### `int.wrap_mul`

Wrapping multiplication within a given bit width. Overflow wraps around.

```
wrap_mul(a: Int, b: Int, bits: Int) -> Int
```

Example: `int.wrap_mul(16, 16, 8) // => 0`

### `int.rotate_right`

Rotate bits right within a given bit width.

```
rotate_right(a: Int, n: Int, bits: Int) -> Int
```

Example: `int.rotate_right(1, 1, 8) // => 128`

### `int.rotate_left`

Rotate bits left within a given bit width.

```
rotate_left(a: Int, n: Int, bits: Int) -> Int
```

Example: `int.rotate_left(128, 1, 8) // => 1`

### `int.to_u32`

Truncate an integer to an unsigned 32-bit value (mask to 0..4294967295).

```
to_u32(a: Int) -> Int
```

Example: `int.to_u32(300) // => 300`

### `int.to_u8`

Truncate an integer to an unsigned 8-bit value (mask to 0..255).

```
to_u8(a: Int) -> Int
```

Example: `int.to_u8(300) // => 44`

### `int.clamp`

Clamp an integer to the range [lo, hi].

```
clamp(n: Int, lo: Int, hi: Int) -> Int
```

Example: `int.clamp(15, 0, 10) // => 10`

### `int.to_float`

Convert an integer to a floating-point number.

```
to_float(n: Int) -> Float
```

Example: `int.to_float(42) // => 42.0`

## io

Layer: **platform** | 3 functions | 3/3 implemented

### `io.read_line`

Read a single line from standard input

```
effect read_line() -> String
```

Example: `let name = io.read_line()`

### `io.print`

Print a string to stdout without a trailing newline

```
effect print(s: String) -> Unit
```

Example: `io.print("Enter name: ")`

### `io.read_all`

Read all of standard input as a single string

```
effect read_all() -> String
```

Example: `let input = io.read_all()`

## json

Layer: **core** | 36 functions | 36/36 implemented

### `json.parse`

Parse a JSON string into a Value.

```
parse(text: String) -> Result[Value, String]
```

Example: `let v = json.parse("{\"name\": \"Alice\"}")`

### `json.stringify`

Convert a Value to a JSON string.

```
stringify(v: Value) -> String
```

Example: `json.stringify(person.encode())`

### `json.get`

Get a nested value by key. Returns none if key doesn't exist.

```
get(j: Value, key: String) -> Option[Value]
```

Example: `json.get(j, "name")`

### `json.get_string`

Get a string value by key. Returns none if key missing or not a string.

```
get_string(j: Value, key: String) -> Option[String]
```

Example: `json.get_string(j, "name")`

### `json.get_int`

Get an integer value by key. Returns none if key missing or not an integer.

```
get_int(j: Value, key: String) -> Option[Int]
```

Example: `json.get_int(j, "age")`

### `json.get_bool`

Get a boolean value by key. Returns none if key missing or not a boolean.

```
get_bool(j: Value, key: String) -> Option[Bool]
```

Example: `json.get_bool(j, "active")`

### `json.get_array`

Get an array value by key. Returns none if key missing or not an array.

```
get_array(j: Value, key: String) -> Option[List[Value]]
```

Example: `json.get_array(j, "items")`

### `json.keys`

Get all keys of a JSON object as a list of strings.

```
keys(j: Value) -> List[String]
```

Example: `json.keys(j)`

### `json.to_string`

Extract the string value from a Json. Returns none if not a string.

```
to_string(j: Value) -> Option[String]
```

Example: `json.to_string(json.from_string("hello"))`

### `json.to_int`

Extract the integer value from a Json. Returns none if not an integer.

```
to_int(j: Value) -> Option[Int]
```

Example: `json.to_int(json.from_int(42))`

### `json.from_string`

Create a Json string value.

```
from_string(s: String) -> Value
```

Example: `json.from_string("hello")`

### `json.from_int`

Create a Json integer value.

```
from_int(n: Int) -> Value
```

Example: `json.from_int(42)`

### `json.from_bool`

Create a Json boolean value.

```
from_bool(b: Bool) -> Value
```

Example: `json.from_bool(true)`

### `json.null`

Create a Json null value.

```
null() -> Value
```

Example: `json.null()`

### `json.array`

Create a Json array from a list of Json values.

```
array(items: List[Value]) -> Value
```

Example: `json.array([json.i(1), json.i(2)])`

### `json.from_map`

Create a Json object from a Map[String, Value].

```
from_map(m: Map[String, Value]) -> Value
```

Example: `json.from_map(map.set(map.new(), "key", json.s("val")))`

### `json.get_float`

Get a float value by key. Returns none if key missing or not a number.

```
get_float(j: Value, key: String) -> Option[Float]
```

Example: `json.get_float(j, "score")`

### `json.from_float`

Create a Json float value.

```
from_float(n: Float) -> Value
```

Example: `json.from_float(3.14)`

### `json.stringify_pretty`

Convert a Json value to a pretty-printed JSON string with indentation.

```
stringify_pretty(j: Value) -> String
```

Example: `json.stringify_pretty(j)`

### `json.object`

Create a Json object from a list of (key, value) pairs.

```
object(entries: List[(String, Value)]) -> Value
```

Example: `json.object([("name", json.s("Alice")), ("age", json.i(30))])`

### `json.s`

Shorthand for json.from_string. Create a Json string value.

```
s(v: String) -> Value
```

Example: `json.s("hello")`

### `json.i`

Shorthand for json.from_int. Create a Json integer value.

```
i(v: Int) -> Value
```

Example: `json.i(42)`

### `json.f`

Shorthand for json.from_float. Create a Json float value.

```
f(v: Float) -> Value
```

Example: `json.f(3.14)`

### `json.b`

Shorthand for json.from_bool. Create a Json boolean value.

```
b(v: Bool) -> Value
```

Example: `json.b(true)`

### `json.as_string`

Extract string from a Json value (without key lookup). Returns none if not a string.

```
as_string(j: Value) -> Option[String]
```

Example: `json.as_string(j)`

### `json.as_int`

Extract integer from a Json value (without key lookup). Returns none if not an integer.

```
as_int(j: Value) -> Option[Int]
```

Example: `json.as_int(j)`

### `json.as_float`

Extract float from a Json value (without key lookup). Returns none if not a number.

```
as_float(j: Value) -> Option[Float]
```

Example: `json.as_float(j)`

### `json.as_bool`

Extract boolean from a Json value (without key lookup). Returns none if not a boolean.

```
as_bool(j: Value) -> Option[Bool]
```

Example: `json.as_bool(j)`

### `json.as_array`

Extract array from a Json value (without key lookup). Returns none if not an array.

```
as_array(j: Value) -> Option[List[Value]]
```

Example: `json.as_array(j)`

### `json.root`

Create a root JSON path for traversal.

```
root() -> JsonPath
```

Example: `json.root()`

### `json.field`

Extend a JSON path with a field name.

```
field(path: JsonPath, name: String) -> JsonPath
```

Example: `json.field(json.root(), "user")`

### `json.index`

Extend a JSON path with an array index.

```
index(path: JsonPath, i: Int) -> JsonPath
```

Example: `json.index(json.field(json.root(), "items"), 0)`

### `json.get_path`

Get a value at a JSON path. Returns none if path doesn't exist.

```
get_path(j: Value, path: JsonPath) -> Option[Value]
```

Example: `json.get_path(j, json.field(json.root(), "name"))`

### `json.set_path`

Set a value at a JSON path. Returns error if path is invalid.

```
set_path(j: Value, path: JsonPath, value: Value) -> Result[Value, String]
```

Example: `json.set_path(j, json.field(json.root(), "name"), json.s("Bob"))`

### `json.upsert_path`

Set a value at a JSON path, creating intermediate objects as needed.

```
upsert_path(j: Value, path: JsonPath, value: Value) -> Value
```

Example: `json.upsert_path(j, json.field(json.root(), "name"), json.s("Bob"))`

### `json.remove_path`

Remove a value at a JSON path. Returns the Json with the value removed.

```
remove_path(j: Value, path: JsonPath) -> Value
```

Example: `json.remove_path(j, json.field(json.root(), "temp"))`

## list

Layer: **core** | 54 functions | 54/54 implemented

### `list.len`

Return the number of elements in a list.

```
len[A](xs: List[A]) -> Int
```

Example: `list.len([1, 2, 3]) // => 3`

### `list.get`

Get the element at index i, or none if out of bounds.

```
get[A](xs: List[A], i: Int) -> Option[A]
```

Example: `list.get([10, 20, 30], 1) // => some(20)`

### `list.get_or`

Get the element at index i, or return a default value.

```
get_or[A](xs: List[A], i: Int, default: A) -> A
```

Example: `list.get_or([1, 2], 5, 0) // => 0`

### `list.set`

Return a new list with the element at index i replaced.

```
set[A](xs: List[A], i: Int, val: A) -> List[A]
```

Example: `list.set([1, 2, 3], 1, 99) // => [1, 99, 3]`

### `list.swap`

Return a new list with elements at indices i and j swapped.

```
swap[A](xs: List[A], i: Int, j: Int) -> List[A]
```

Example: `list.swap([1, 2, 3], 0, 2) // => [3, 2, 1]`

### `list.sort`

Sort a list in ascending order.

```
sort[A](xs: List[A]) -> List[A]
```

Example: `list.sort([3, 1, 2]) // => [1, 2, 3]`

### `list.reverse`

Reverse the order of elements.

```
reverse[A](xs: List[A]) -> List[A]
```

Example: `list.reverse([1, 2, 3]) // => [3, 2, 1]`

### `list.contains`

Check if a list contains an element.

```
contains[A](xs: List[A], x: A) -> Bool
```

Example: `list.contains([1, 2, 3], 2) // => true`

### `list.enumerate`

Pair each element with its index.

```
enumerate[A](xs: List[A]) -> List[(Int, A)]
```

Example: `list.enumerate(["a", "b"]) // => [(0, "a"), (1, "b")]`

### `list.zip`

Combine two lists into a list of pairs.

```
zip[A, B](xs: List[A], ys: List[B]) -> List[(A, B)]
```

Example: `list.zip([1, 2], ["a", "b"]) // => [(1, "a"), (2, "b")]`

### `list.flatten`

Flatten a list of lists into a single list.

```
flatten[T](xss: List[List[T]]) -> List[T]
```

Example: `list.flatten([[1, 2], [3]]) // => [1, 2, 3]`

### `list.take`

Take the first n elements.

```
take[A](xs: List[A], n: Int) -> List[A]
```

Example: `list.take([1, 2, 3, 4], 2) // => [1, 2]`

### `list.drop`

Drop the first n elements.

```
drop[A](xs: List[A], n: Int) -> List[A]
```

Example: `list.drop([1, 2, 3, 4], 2) // => [3, 4]`

### `list.unique`

Remove duplicate elements, preserving first occurrence.

```
unique[A](xs: List[A]) -> List[A]
```

Example: `list.unique([1, 2, 1, 3]) // => [1, 2, 3]`

### `list.index_of`

Find the first index of an element, or none.

```
index_of[A](xs: List[A], x: A) -> Option[Int]
```

Example: `list.index_of([10, 20, 30], 20) // => some(1)`

### `list.last`

Get the last element, or none if empty.

```
last[A](xs: List[A]) -> Option[A]
```

Example: `list.last([1, 2, 3]) // => some(3)`

### `list.chunk`

Split a list into chunks of size n.

```
chunk[A](xs: List[A], n: Int) -> List[List[A]]
```

Example: `list.chunk([1, 2, 3, 4, 5], 2) // => [[1, 2], [3, 4], [5]]`

### `list.sum`

Sum all integers in a list.

```
sum(xs: List[Int]) -> Int
```

Example: `list.sum([1, 2, 3]) // => 6`

### `list.product`

Multiply all integers in a list.

```
product(xs: List[Int]) -> Int
```

Example: `list.product([2, 3, 4]) // => 24`

### `list.first`

Get the first element, or none if empty.

```
first[A](xs: List[A]) -> Option[A]
```

Example: `list.first([1, 2, 3]) // => some(1)`

### `list.is_empty`

Check if a list is empty.

```
is_empty[A](xs: List[A]) -> Bool
```

Example: `list.is_empty([]) // => true`

### `list.min`

Find the minimum element, or none if empty.

```
min[A](xs: List[A]) -> Option[A]
```

Example: `list.min([3, 1, 2]) // => some(1)`

### `list.max`

Find the maximum element, or none if empty.

```
max[A](xs: List[A]) -> Option[A]
```

Example: `list.max([3, 1, 2]) // => some(3)`

### `list.join`

Join a list of strings with a separator.

```
join(xs: List[String], sep: String) -> String
```

Example: `list.join(["a", "b", "c"], "-") // => "a-b-c"`

### `list.map`

Apply a function to each element, returning a new list.

```
map[A, B](xs: List[A], f: Fn[A] -> B) -> List[B]
```

Example: `[1, 2, 3].map(fn(x) => x * 2) // => [2, 4, 6]`

### `list.filter`

Keep elements that satisfy a predicate.

```
filter[A](xs: List[A], f: Fn[A] -> Bool) -> List[A]
```

Example: `[1, 2, 3, 4].filter(fn(x) => x > 2) // => [3, 4]`

### `list.find`

Find the first element matching a predicate.

```
find[A](xs: List[A], f: Fn[A] -> Bool) -> Option[A]
```

Example: `[1, 2, 3].find(fn(x) => x > 1) // => some(2)`

### `list.any`

Check if any element satisfies a predicate.

```
any[A](xs: List[A], f: Fn[A] -> Bool) -> Bool
```

Example: `[1, 2, 3].any(fn(x) => x > 2) // => true`

### `list.all`

Check if all elements satisfy a predicate.

```
all[A](xs: List[A], f: Fn[A] -> Bool) -> Bool
```

Example: `[2, 4, 6].all(fn(x) => x % 2 == 0) // => true`

### `list.each`

Execute a function for each element (side effects only).

```
each[A](xs: List[A], f: Fn[A] -> Unit) -> Unit
```

Example: `[1, 2, 3].each(fn(x) => println(to_string(x)))`

### `list.sort_by`

Sort by a key function.

```
sort_by[A, B](xs: List[A], f: Fn[A] -> B) -> List[A]
```

Example: `["bb", "a", "ccc"].sort_by(fn(s) => string.len(s)) // => ["a", "bb", "ccc"]`

### `list.flat_map`

Map each element to a list and flatten the results.

```
flat_map[A, B](xs: List[A], f: Fn[A] -> List[B]) -> List[B]
```

Example: `[1, 2].flat_map(fn(x) => [x, x * 10]) // => [1, 10, 2, 20]`

### `list.filter_map`

Map and filter in one pass: keep only some values.

```
filter_map[A, B](xs: List[A], f: Fn[A] -> Option[B]) -> List[B]
```

Example: `["1", "x", "3"].filter_map(fn(s) => string.to_int(s)) // => [1, 3]`

### `list.take_while`

Take elements from the front while a predicate holds.

```
take_while[A](xs: List[A], f: Fn[A] -> Bool) -> List[A]
```

Example: `[1, 2, 3, 1].take_while(fn(x) => x < 3) // => [1, 2]`

### `list.drop_while`

Drop elements from the front while a predicate holds.

```
drop_while[A](xs: List[A], f: Fn[A] -> Bool) -> List[A]
```

Example: `[1, 2, 3, 1].drop_while(fn(x) => x < 3) // => [3, 1]`

### `list.count`

Count elements that satisfy a predicate.

```
count[A](xs: List[A], f: Fn[A] -> Bool) -> Int
```

Example: `[1, 2, 3, 4].count(fn(x) => x > 2) // => 2`

### `list.partition`

Split a list into two: elements matching and not matching a predicate.

```
partition[A](xs: List[A], f: Fn[A] -> Bool) -> (List[A], List[A])
```

Example: `[1, 2, 3, 4].partition(fn(x) => x % 2 == 0) // => ([2, 4], [1, 3])`

### `list.reduce`

Reduce a list by combining elements pairwise. Returns none if empty.

```
reduce[A](xs: List[A], f: Fn[A, A] -> A) -> Option[A]
```

Example: `[1, 2, 3].reduce(fn(a, b) => a + b) // => some(6)`

### `list.group_by`

Group elements by a key function into a map.

```
group_by[A, B](xs: List[A], f: Fn[A] -> B) -> Map[B, List[A]]
```

Example: `["hi", "hey", "bye"].group_by(fn(s) => string.char_at(s, 0))`

### `list.range`

Create a list of integers from start (inclusive) to end (exclusive).

```
range(start: Int, end: Int) -> List[Int]
```

Example: `list.range(1, 5) // => [1, 2, 3, 4]`

### `list.slice`

Extract a sublist from start to end index.

```
slice[A](xs: List[A], start: Int, end: Int) -> List[A]
```

Example: `list.slice([1, 2, 3, 4, 5], 1, 4) // => [2, 3, 4]`

### `list.insert`

Insert an element at index i, shifting elements right.

```
insert[A](xs: List[A], i: Int, val: A) -> List[A]
```

Example: `list.insert([1, 3], 1, 2) // => [1, 2, 3]`

### `list.remove_at`

Remove the element at index i.

```
remove_at[A](xs: List[A], i: Int) -> List[A]
```

Example: `list.remove_at([1, 2, 3], 1) // => [1, 3]`

### `list.find_index`

Find the first index where a predicate holds.

```
find_index[A](xs: List[A], f: Fn[A] -> Bool) -> Option[Int]
```

Example: `[10, 20, 30].find_index(fn(x) => x > 15) // => some(1)`

### `list.update`

Return a new list with the element at index i transformed by f.

```
update[A](xs: List[A], i: Int, f: Fn[A] -> A) -> List[A]
```

Example: `list.update([1, 2, 3], 1, fn(x) => x * 10) // => [1, 20, 3]`

### `list.repeat`

Create a list with a value repeated n times.

```
repeat[A](val: A, n: Int) -> List[A]
```

Example: `list.repeat(0, 3) // => [0, 0, 0]`

### `list.scan`

Like fold, but returns all intermediate accumulator values.

```
scan[A, B](xs: List[A], init: B, f: Fn[B, A] -> B) -> List[B]
```

Example: `[1, 2, 3].scan(0, fn(acc, x) => acc + x) // => [1, 3, 6]`

### `list.intersperse`

Insert a separator between each element.

```
intersperse[A](xs: List[A], sep: A) -> List[A]
```

Example: `list.intersperse([1, 2, 3], 0) // => [1, 0, 2, 0, 3]`

### `list.windows`

Return sliding windows of size n.

```
windows[A](xs: List[A], n: Int) -> List[List[A]]
```

Example: `list.windows([1, 2, 3, 4], 2) // => [[1, 2], [2, 3], [3, 4]]`

### `list.dedup`

Remove consecutive duplicates.

```
dedup[A](xs: List[A]) -> List[A]
```

Example: `list.dedup([1, 1, 2, 2, 1]) // => [1, 2, 1]`

### `list.zip_with`

Combine two lists element-wise using a function.

```
zip_with[A, B, C](xs: List[A], ys: List[B], f: Fn[A, B] -> C) -> List[C]
```

Example: `list.zip_with([1, 2], [10, 20], fn(a, b) => a + b) // => [11, 22]`

### `list.sum_float`

Sum all floats in a list.

```
sum_float(xs: List[Float]) -> Float
```

Example: `list.sum_float([1.5, 2.5]) // => 4.0`

### `list.product_float`

Multiply all floats in a list.

```
product_float(xs: List[Float]) -> Float
```

Example: `list.product_float([2.0, 3.0]) // => 6.0`

### `list.fold`

Reduce a list from left with an initial accumulator.

```
fold[A, B](xs: List[A], init: B, f: Fn[B, A] -> B) -> B
```

Example: `[1, 2, 3].fold(0, fn(acc, x) => acc + x) // => 6`

## log

Layer: **core** | 8 functions | 8/8 implemented

### `log.debug`

Log a debug message to stderr.

```
effect debug(msg: String) -> Unit
```

Example: `log.debug("cache hit")`

### `log.info`

Log an info message to stderr.

```
effect info(msg: String) -> Unit
```

Example: `log.info("server started")`

### `log.warn`

Log a warning message to stderr.

```
effect warn(msg: String) -> Unit
```

Example: `log.warn("disk space low")`

### `log.error`

Log an error message to stderr.

```
effect error(msg: String) -> Unit
```

Example: `log.error("connection failed")`

### `log.debug_with`

Log a debug message with structured key-value fields.

```
effect debug_with(msg: String, fields: List[(String, String)]) -> Unit
```

Example: `log.debug_with("cache hit", [("key", cache_key)])`

### `log.info_with`

Log an info message with structured key-value fields.

```
effect info_with(msg: String, fields: List[(String, String)]) -> Unit
```

Example: `log.info_with("user login", [("user_id", id), ("ip", addr)])`

### `log.warn_with`

Log a warning message with structured key-value fields.

```
effect warn_with(msg: String, fields: List[(String, String)]) -> Unit
```

Example: `log.warn_with("slow query", [("duration_ms", "500")])`

### `log.error_with`

Log an error message with structured key-value fields.

```
effect error_with(msg: String, fields: List[(String, String)]) -> Unit
```

Example: `log.error_with("read failed", [("path", path), ("error", e)])`

## map

Layer: **core** | 16 functions | 16/16 implemented

### `map.new`

Create an empty map.

```
new[K, V]() -> Map[K, V]
```

Example: `let m = map.new()`

### `map.get`

Get a value by key. Returns none if the key doesn't exist.

```
get[K, V](m: Map[K, V], key: K) -> Option[V]
```

Example: `map.get(m, "name")`

### `map.get_or`

Get a value by key, returning a default if the key doesn't exist.

```
get_or[K, V](m: Map[K, V], key: K, default: V) -> V
```

Example: `map.get_or(m, "name", "unknown")`

### `map.set`

Return a new map with the key set to value. Immutable — does not modify the original.

```
set[K, V](m: Map[K, V], key: K, value: V) -> Map[K, V]
```

Example: `let m2 = map.set(m, "name", "Alice")`

### `map.contains`

Check if a key exists in the map.

```
contains[K, V](m: Map[K, V], key: K) -> Bool
```

Example: `map.contains(m, "name")`

### `map.remove`

Return a new map with the key removed. Immutable — does not modify the original.

```
remove[K, V](m: Map[K, V], key: K) -> Map[K, V]
```

Example: `let m2 = map.remove(m, "temp")`

### `map.keys`

Get all keys as a sorted list.

```
keys[K, V](m: Map[K, V]) -> List[K]
```

Example: `map.keys(m)`

### `map.values`

Get all values as a list.

```
values[K, V](m: Map[K, V]) -> List[V]
```

Example: `map.values(m)`

### `map.len`

Get the number of key-value pairs in the map.

```
len[K, V](m: Map[K, V]) -> Int
```

Example: `map.len(m)`

### `map.entries`

Get all key-value pairs as a list of tuples, sorted by key.

```
entries[K, V](m: Map[K, V]) -> List[(K, V)]
```

Example: `map.entries(m)`

### `map.merge`

Merge two maps. Keys in the second map override keys in the first.

```
merge[K, V](a: Map[K, V], b: Map[K, V]) -> Map[K, V]
```

Example: `map.merge(base, overrides)`

### `map.is_empty`

Check if the map has no entries.

```
is_empty[K, V](m: Map[K, V]) -> Bool
```

Example: `map.is_empty(m)`

### `map.from_entries`

Create a map from a list of (key, value) tuples.

```
from_entries[K, V](entries: List[(K, V)]) -> Map[K, V]
```

Example: `map.from_entries([("a", 1), ("b", 2)])`

### `map.from_list`

Create a map by applying a function to each element that returns a (key, value) pair.

```
from_list[A, K, V](xs: List[A], f: Fn[A] -> (K, V)) -> Map[K, V]
```

Example: `map.from_list(["a", "b"], fn(s) => (s, string.len(s)))`

### `map.map_values`

Transform all values in the map using a function, keeping keys unchanged.

```
map_values[K, V, B](m: Map[K, V], f: Fn[V] -> B) -> Map[K, B]
```

Example: `map.map_values(m, fn(v) => v * 2)`

### `map.filter`

Return a new map containing only entries where the predicate returns true.

```
filter[K, V](m: Map[K, V], f: Fn[K, V] -> Bool) -> Map[K, V]
```

Example: `map.filter(m, fn(k, v) => v > 0)`

## math

Layer: **core** | 21 functions | 21/21 implemented

### `math.min`

Return the smaller of two integers.

```
min(a: Int, b: Int) -> Int
```

Example: `math.min(3, 7) // => 3`

### `math.max`

Return the larger of two integers.

```
max(a: Int, b: Int) -> Int
```

Example: `math.max(3, 7) // => 7`

### `math.abs`

Return the absolute value of an integer.

```
abs(n: Int) -> Int
```

Example: `math.abs(-5) // => 5`

### `math.pow`

Raise an integer base to an integer exponent.

```
pow(base: Int, exp: Int) -> Int
```

Example: `math.pow(2, 10) // => 1024`

### `math.pi`

Return the mathematical constant pi (3.14159...).

```
pi() -> Float
```

Example: `math.pi() // => 3.141592653589793`

### `math.e`

Return Euler's number e (2.71828...).

```
e() -> Float
```

Example: `math.e() // => 2.718281828459045`

### `math.sin`

Return the sine of an angle in radians.

```
sin(x: Float) -> Float
```

Example: `math.sin(0.0) // => 0.0`

### `math.cos`

Return the cosine of an angle in radians.

```
cos(x: Float) -> Float
```

Example: `math.cos(0.0) // => 1.0`

### `math.tan`

Return the tangent of an angle in radians.

```
tan(x: Float) -> Float
```

Example: `math.tan(0.0) // => 0.0`

### `math.log`

Return the natural logarithm (base e) of a float.

```
log(x: Float) -> Float
```

Example: `math.log(1.0) // => 0.0`

### `math.exp`

Return e raised to the given power.

```
exp(x: Float) -> Float
```

Example: `math.exp(1.0) // => 2.718281828459045`

### `math.sqrt`

Return the square root of a float.

```
sqrt(x: Float) -> Float
```

Example: `math.sqrt(16.0) // => 4.0`

### `math.log10`

Return the base-10 logarithm of a float.

```
log10(x: Float) -> Float
```

Example: `math.log10(100.0) // => 2.0`

### `math.log2`

Return the base-2 logarithm of a float.

```
log2(x: Float) -> Float
```

Example: `math.log2(8.0) // => 3.0`

### `math.sign`

Return the sign of an integer: -1, 0, or 1.

```
sign(n: Int) -> Int
```

Example: `math.sign(-42) // => -1`

### `math.fmin`

Return the smaller of two floats.

```
fmin(a: Float, b: Float) -> Float
```

Example: `math.fmin(1.5, 2.5) // => 1.5`

### `math.fmax`

Return the larger of two floats.

```
fmax(a: Float, b: Float) -> Float
```

Example: `math.fmax(1.5, 2.5) // => 2.5`

### `math.fpow`

Raise a float base to a float exponent.

```
fpow(base: Float, exp: Float) -> Float
```

Example: `math.fpow(2.0, 0.5) // => 1.4142135623730951`

### `math.factorial`

Return the factorial of a non-negative integer.

```
factorial(n: Int) -> Int
```

Example: `math.factorial(5) // => 120`

### `math.choose`

Return the binomial coefficient C(n, k) = n! / (k! * (n-k)!).

```
choose(n: Int, k: Int) -> Int
```

Example: `math.choose(5, 2) // => 10`

### `math.log_gamma`

Return the natural logarithm of the gamma function at x.

```
log_gamma(x: Float) -> Float
```

Example: `math.log_gamma(5.0) // => 3.178...`

## process

Layer: **platform** | 6 functions | 6/6 implemented

### `process.exec`

Execute a command and return its stdout as a string

```
effect exec(cmd: String, args: List[String]) -> Result[String, String]
```

Example: `let output = process.exec("ls", ["-la"])`

### `process.exit`

Exit the process with the given status code

```
effect exit(code: Int) -> Unit
```

Example: `process.exit(1)`

### `process.stdin_lines`

Read all lines from standard input

```
effect stdin_lines() -> Result[List[String], String]
```

Example: `let lines = process.stdin_lines()`

### `process.exec_in`

Execute a command in a specific working directory

```
effect exec_in(dir: String, cmd: String, args: List[String]) -> Result[String, String]
```

Example: `let output = process.exec_in("/tmp", "pwd", [])`

### `process.exec_with_stdin`

Execute a command with input piped to its stdin

```
effect exec_with_stdin(cmd: String, args: List[String], input: String) -> Result[String, String]
```

Example: `let output = process.exec_with_stdin("cat", [], "hello")`

### `process.exec_status`

Execute a command and return exit code, stdout, and stderr

```
effect exec_status(cmd: String, args: List[String]) -> Result[{code: Int, stdout: String, stderr: String}, String]
```

Example: `let r = process.exec_status("ls", []) // {code, stdout, stderr}`

## random

Layer: **platform** | 4 functions | 4/4 implemented

### `random.int`

Generate a random integer between min and max (inclusive).

```
effect int(min: Int, max: Int) -> Int
```

Example: `random.int(1, 100) // => 42`

### `random.float`

Generate a random float between 0.0 and 1.0.

```
effect float() -> Float
```

Example: `random.float() // => 0.7321`

### `random.choice`

Pick a random element from a list, or none if empty.

```
effect choice[T](xs: List[T]) -> Option[T]
```

Example: `random.choice(["a", "b", "c"]) // => some("b")`

### `random.shuffle`

Return a randomly shuffled copy of a list.

```
effect shuffle[T](xs: List[T]) -> List[T]
```

Example: `random.shuffle([1, 2, 3]) // => [3, 1, 2]`

## regex

Layer: **core** | 8 functions | 0/8 implemented

### `regex.is_match` (not implemented)

Check if a pattern matches anywhere in a string.

```
is_match(pat: String, s: String) -> Bool
```

Example: `regex.is_match("[0-9]+", "abc123") // => true`

### `regex.full_match` (not implemented)

Check if a pattern matches the entire string.

```
full_match(pat: String, s: String) -> Bool
```

Example: `regex.full_match("[0-9]+", "123") // => true`

### `regex.find` (not implemented)

Find the first match of a pattern in a string.

```
find(pat: String, s: String) -> Option[String]
```

Example: `regex.find("[0-9]+", "abc123def") // => some("123")`

### `regex.find_all` (not implemented)

Find all non-overlapping matches of a pattern.

```
find_all(pat: String, s: String) -> List[String]
```

Example: `regex.find_all("[0-9]+", "a1b2c3") // => ["1", "2", "3"]`

### `regex.replace` (not implemented)

Replace all matches of a pattern with a replacement string.

```
replace(pat: String, s: String, rep: String) -> String
```

Example: `regex.replace("[0-9]+", "a1b2", "X") // => "aXbX"`

### `regex.replace_first` (not implemented)

Replace the first match of a pattern.

```
replace_first(pat: String, s: String, rep: String) -> String
```

Example: `regex.replace_first("[0-9]+", "a1b2", "X") // => "aXb2"`

### `regex.split` (not implemented)

Split a string by a regex pattern.

```
split(pat: String, s: String) -> List[String]
```

Example: `regex.split("[,;]", "a,b;c") // => ["a", "b", "c"]`

### `regex.captures` (not implemented)

Extract capture groups from the first match.

```
captures(pat: String, s: String) -> Option[List[String]]
```

Example: `regex.captures("(\\w+)@(\\w+)", "user@host") // => some(["user@host", "user", "host"])`

## result

Layer: **core** | 9 functions | 9/9 implemented

### `result.map`

Transform the ok value using a function. If err, passes through unchanged.

```
map[A, B, E](r: Result[A, E], f: Fn[A] -> B) -> Result[B, E]
```

Example: `result.map(ok(2), fn(x) => x * 10)`

### `result.map_err`

Transform the err value using a function. If ok, passes through unchanged.

```
map_err[A, E, F](r: Result[A, E], f: Fn[E] -> F) -> Result[A, F]
```

Example: `result.map_err(err("fail"), fn(e) => "wrapped: " ++ e)`

### `result.and_then`

Chain a Result-returning function on the ok value. Flattens nested Results.

```
and_then[A, B, E](r: Result[A, E], f: Fn[A] -> Result[B, E]) -> Result[B, E]
```

Example: `result.and_then(ok(5), fn(x) => if x > 0 then ok(x) else err("negative"))`

### `result.unwrap_or`

Get the ok value, or return a default if err.

```
unwrap_or[A, E](r: Result[A, E], default: A) -> A
```

Example: `result.unwrap_or(err("fail"), 0)`

### `result.unwrap_or_else`

Get the ok value, or compute a default from the error using a function.

```
unwrap_or_else[A, E](r: Result[A, E], f: Fn[E] -> A) -> A
```

Example: `result.unwrap_or_else(err("fail"), fn(e) => string.len(e))`

### `result.is_ok`

Check if the Result is ok.

```
is_ok[A, E](r: Result[A, E]) -> Bool
```

Example: `result.is_ok(ok(42))`

### `result.is_err`

Check if the Result is err.

```
is_err[A, E](r: Result[A, E]) -> Bool
```

Example: `result.is_err(err("fail"))`

### `result.to_option`

Convert ok to some, err to none. Discards the error value.

```
to_option[A, E](r: Result[A, E]) -> Option[A]
```

Example: `result.to_option(ok(42))`

### `result.to_err_option`

Convert err to some, ok to none. Discards the ok value.

```
to_err_option[A, E](r: Result[A, E]) -> Option[E]
```

Example: `result.to_err_option(err("fail"))`

## string

Layer: **core** | 41 functions | 41/41 implemented

### `string.trim`

Remove leading and trailing whitespace.

```
trim(s: String) -> String
```

Example: `string.trim("  hello  ") // => "hello"`

### `string.split`

Split a string by separator into a list of substrings.

```
split(s: String, sep: String) -> List[String]
```

Example: `string.split("a,b,c", ",") // => ["a", "b", "c"]`

### `string.join`

Join a list of strings with a separator.

```
join(list: List[String], sep: String) -> String
```

Example: `string.join(["a", "b", "c"], "-") // => "a-b-c"`

### `string.len`

Return the number of characters in a string.

```
len(s: String) -> Int
```

Example: `string.len("hello") // => 5`

### `string.contains`

Check if a string contains a substring.

```
contains(s: String, sub: String) -> Bool
```

Example: `string.contains("hello world", "world") // => true`

### `string.starts_with`

Check if a string starts with a prefix.

```
starts_with(s: String, prefix: String) -> Bool
```

Example: `string.starts_with("hello", "hel") // => true`

### `string.ends_with`

Check if a string ends with a suffix.

```
ends_with(s: String, suffix: String) -> Bool
```

Example: `string.ends_with("hello", "llo") // => true`

### `string.slice`

Extract a substring by start and optional end index.

```
slice(s: String, start: Int, end: Int?) -> String
```

Example: `string.slice("hello", 1, 4) // => "ell"`

### `string.pad_left`

Pad a string on the left to reach a target length.

```
pad_left(s: String, n: Int, ch: String) -> String
```

Example: `string.pad_left("42", 5, "0") // => "00042"`

### `string.to_bytes`

Convert a string to a list of UTF-8 byte values.

```
to_bytes(s: String) -> List[Int]
```

Example: `string.to_bytes("Hi") // => [72, 105]`

### `string.capitalize`

Capitalize the first character of a string.

```
capitalize(s: String) -> String
```

Example: `string.capitalize("hello") // => "Hello"`

### `string.to_upper`

Convert all characters to uppercase.

```
to_upper(s: String) -> String
```

Example: `string.to_upper("hello") // => "HELLO"`

### `string.to_lower`

Convert all characters to lowercase.

```
to_lower(s: String) -> String
```

Example: `string.to_lower("HELLO") // => "hello"`

### `string.to_int`

Parse a string as an integer. Returns err if invalid.

```
to_int(s: String) -> Result[Int, String]
```

Example: `string.to_int("42") // => ok(42)`

### `string.replace`

Replace all occurrences of a substring.

```
replace(s: String, from: String, to: String) -> String
```

Example: `string.replace("aabbcc", "bb", "XX") // => "aaXXcc"`

### `string.char_at`

Get the character at a given index, or none if out of bounds.

```
char_at(s: String, i: Int) -> Option[String]
```

Example: `string.char_at("hello", 1) // => some("e")`

### `string.lines`

Split a string into lines.

```
lines(s: String) -> List[String]
```

Example: `string.lines("a\nb\nc") // => ["a", "b", "c"]`

### `string.chars`

Split a string into individual characters.

```
chars(s: String) -> List[String]
```

Example: `string.chars("abc") // => ["a", "b", "c"]`

### `string.index_of`

Find the first index of a substring, or none if not found.

```
index_of(s: String, needle: String) -> Option[Int]
```

Example: `string.index_of("hello", "ll") // => some(2)`

### `string.repeat`

Repeat a string n times.

```
repeat(s: String, n: Int) -> String
```

Example: `string.repeat("ab", 3) // => "ababab"`

### `string.from_bytes`

Create a string from a list of UTF-8 byte values.

```
from_bytes(bytes: List[Int]) -> String
```

Example: `string.from_bytes([72, 105]) // => "Hi"`

### `string.is_digit`

Check if all characters are ASCII digits.

```
is_digit(s: String) -> Bool
```

Example: `string.is_digit("123") // => true`

### `string.is_alpha`

Check if all characters are alphabetic.

```
is_alpha(s: String) -> Bool
```

Example: `string.is_alpha("abc") // => true`

### `string.is_alphanumeric`

Check if all characters are alphanumeric.

```
is_alphanumeric(s: String) -> Bool
```

Example: `string.is_alphanumeric("abc123") // => true`

### `string.is_whitespace`

Check if all characters are whitespace.

```
is_whitespace(s: String) -> Bool
```

Example: `string.is_whitespace("  ") // => true`

### `string.is_upper`

Check if all characters in the string are uppercase.

```
is_upper(s: String) -> Bool
```

Example: `string.is_upper("ABC") // => true`

### `string.is_lower`

Check if all characters in the string are lowercase.

```
is_lower(s: String) -> Bool
```

Example: `string.is_lower("abc") // => true`

### `string.codepoint`

Return the Unicode codepoint of the first character, or none for empty string.

```
codepoint(s: String) -> Option[Int]
```

Example: `string.codepoint("A") // => some(65)`

### `string.from_codepoint`

Create a single-character string from a Unicode codepoint.

```
from_codepoint(n: Int) -> String
```

Example: `string.from_codepoint(65) // => "A"`

### `string.char_count`

Return the number of Unicode characters (not bytes) in a string.

```
char_count(s: String) -> Int
```

Example: `string.char_count("hello") // => 5`

### `string.pad_right`

Pad a string on the right to reach a target length.

```
pad_right(s: String, n: Int, ch: String) -> String
```

Example: `string.pad_right("hi", 5, ".") // => "hi..."`

### `string.trim_start`

Remove leading whitespace.

```
trim_start(s: String) -> String
```

Example: `string.trim_start("  hello") // => "hello"`

### `string.trim_end`

Remove trailing whitespace.

```
trim_end(s: String) -> String
```

Example: `string.trim_end("hello  ") // => "hello"`

### `string.count`

Count occurrences of a substring.

```
count(s: String, sub: String) -> Int
```

Example: `string.count("banana", "an") // => 2`

### `string.is_empty`

Check if a string is empty.

```
is_empty(s: String) -> Bool
```

Example: `string.is_empty("") // => true`

### `string.reverse`

Reverse the characters in a string.

```
reverse(s: String) -> String
```

Example: `string.reverse("hello") // => "olleh"`

### `string.strip_prefix`

Remove a prefix if present, returning none if not found.

```
strip_prefix(s: String, prefix: String) -> Option[String]
```

Example: `string.strip_prefix("hello", "hel") // => some("lo")`

### `string.strip_suffix`

Remove a suffix if present, returning none if not found.

```
strip_suffix(s: String, suffix: String) -> Option[String]
```

Example: `string.strip_suffix("hello", "llo") // => some("he")`

### `string.replace_first`

Replace the first occurrence of a substring.

```
replace_first(s: String, from: String, to: String) -> String
```

Example: `string.replace_first("aabaa", "a", "X") // => "Xabaa"`

### `string.last_index_of`

Find the last index of a substring, or none if not found.

```
last_index_of(s: String, needle: String) -> Option[Int]
```

Example: `string.last_index_of("abcabc", "bc") // => some(4)`

### `string.to_float`

Parse a string as a float. Returns err if invalid.

```
to_float(s: String) -> Result[Float, String]
```

Example: `string.to_float("3.14") // => ok(3.14)`

## testing

Layer: **core** | 7 functions | 7/7 implemented

### `testing.assert_throws`

Assert that a function throws an error containing the expected message.

```
assert_throws(f: fn() -> Unit, expected: String) -> Unit
```

Example: `testing.assert_throws(fn() => panic("oh no"), "oh no")`

### `testing.assert_contains`

Assert that a string contains a substring.

```
assert_contains(haystack: String, needle: String) -> Unit
```

Example: `testing.assert_contains("hello world", "world")`

### `testing.assert_approx`

Assert two floats are approximately equal within tolerance.

```
assert_approx(a: Float, b: Float, tolerance: Float) -> Unit
```

Example: `testing.assert_approx(3.14, 3.14159, 0.01)`

### `testing.assert_gt`

Assert that a is greater than b.

```
assert_gt(a: Int, b: Int) -> Unit
```

Example: `testing.assert_gt(10, 5)`

### `testing.assert_lt`

Assert that a is less than b.

```
assert_lt(a: Int, b: Int) -> Unit
```

Example: `testing.assert_lt(3, 7)`

### `testing.assert_some`

Assert that an Option is some (not none).

```
assert_some(opt: Option[String]) -> Unit
```

Example: `testing.assert_some(some("value"))`

### `testing.assert_ok`

Assert that a Result is ok (not err).

```
assert_ok(result: Result[String, String]) -> Unit
```

Example: `testing.assert_ok(ok("success"))`

## uuid

Layer: **platform** | 6 functions | 6/6 implemented

### `uuid.v4`

Generate a random UUID v4.

```
effect v4() -> Result[String, String]
```

Example: `uuid.v4() // => ok("550e8400-e29b-41d4-a716-446655440000")`

### `uuid.v5`

Generate a deterministic UUID v5 from namespace and name.

```
effect v5(namespace: String, name: String) -> Result[String, String]
```

Example: `uuid.v5("dns", "example.com")`

### `uuid.parse`

Parse and validate a UUID string.

```
parse(s: String) -> Result[String, String]
```

Example: `uuid.parse("550e8400-e29b-41d4-a716-446655440000") // => ok("...")`

### `uuid.is_valid`

Check if a string is a valid UUID.

```
is_valid(s: String) -> Bool
```

Example: `uuid.is_valid("550e8400-e29b-41d4-a716-446655440000") // => true`

### `uuid.nil`

Return the nil UUID (all zeros).

```
nil() -> String
```

Example: `uuid.nil() // => "00000000-0000-0000-0000-000000000000"`

### `uuid.version`

Extract the version number from a UUID string.

```
version(s: String) -> Result[Int, String]
```

Example: `uuid.version("550e8400-e29b-41d4-a716-446655440000") // => ok(4)`

## value

Layer: **core** | 19 functions | 19/19 implemented

### `value.field`

Get a field from a Value object by key. Returns err if missing.

```
field(v: Value, key: String) -> Result[Value, String]
```

### `value.as_string`

Extract a String from a Value. Returns err if not a Str.

```
as_string(v: Value) -> Result[String, String]
```

### `value.as_int`

Extract an Int from a Value. Returns err if not an Int.

```
as_int(v: Value) -> Result[Int, String]
```

### `value.as_float`

Extract a Float from a Value. Returns err if not a Float.

```
as_float(v: Value) -> Result[Float, String]
```

### `value.as_bool`

Extract a Bool from a Value. Returns err if not a Bool.

```
as_bool(v: Value) -> Result[Bool, String]
```

### `value.as_array`

Extract a List[Value] from a Value. Returns err if not an Array.

```
as_array(v: Value) -> Result[List[Value], String]
```

### `value.str`

Create a Value from a String.

```
str(s: String) -> Value
```

### `value.int`

Create a Value from an Int.

```
int(n: Int) -> Value
```

### `value.float`

Create a Value from a Float.

```
float(f: Float) -> Value
```

### `value.bool`

Create a Value from a Bool.

```
bool(b: Bool) -> Value
```

### `value.object`

Create a Value object from a list of key-value pairs.

```
object(pairs: List[(String, Value)]) -> Value
```

### `value.array`

Create a Value array from a list of Values.

```
array(items: List[Value]) -> Value
```

### `value.null`

Create a null Value.

```
null() -> Value
```

### `value.pick`

Pick specific keys from an Object, discarding the rest.

```
pick(v: Value, keys: List[String]) -> Value
```

### `value.omit`

Remove specific keys from an Object.

```
omit(v: Value, keys: List[String]) -> Value
```

### `value.merge`

Merge two Objects. Keys from b override keys from a.

```
merge(a: Value, b: Value) -> Value
```

### `value.to_camel_case`

Convert Object keys from snake_case to camelCase.

```
to_camel_case(v: Value) -> Value
```

### `value.to_snake_case`

Convert Object keys from camelCase to snake_case.

```
to_snake_case(v: Value) -> Value
```

### `value.stringify`

Convert a Value to a JSON-like string representation.

```
stringify(v: Value) -> String
```
