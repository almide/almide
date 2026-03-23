# Stdlib 1.0 — I/O Module Details

> Detailed function-level spec for fs, http, and datetime modules.
> Generated from `stdlib/defs/*.toml` definitions and cross-checked against `stdlib-1.0.md` naming conventions.

---

## fs (24 functions, all effect)

All fs functions perform filesystem I/O. Every function except `temp_dir` is marked `effect = true` in the TOML definition.

| Function | Signature | Description |
|----------|-----------|-------------|
| read_text | `(path: String) -> Result[String, String]` | Read file contents as a UTF-8 string |
| read_bytes | `(path: String) -> Result[List[Int], String]` | Read file contents as a list of bytes |
| write | `(path: String, content: String) -> Result[Unit, String]` | Write a string to a file, creating or overwriting it |
| write_bytes | `(path: String, bytes: List[Int]) -> Result[Unit, String]` | Write a list of bytes to a file |
| append | `(path: String, content: String) -> Result[Unit, String]` | Append a string to a file, creating it if needed |
| mkdir_p | `(path: String) -> Result[Unit, String]` | Create a directory and all parent directories |
| exists | `(path: String) -> Bool` | Check if a file or directory exists |
| read_lines | `(path: String) -> Result[List[String], String]` | Read a file as a list of lines |
| remove | `(path: String) -> Result[Unit, String]` | Delete a file |
| list_dir | `(path: String) -> Result[List[String], String]` | List entries in a directory |
| is_dir | `(path: String) -> Bool` | Check if a path is a directory |
| is_file | `(path: String) -> Bool` | Check if a path is a regular file |
| copy | `(src: String, dst: String) -> Result[Unit, String]` | Copy a file from src to dst |
| rename | `(src: String, dst: String) -> Result[Unit, String]` | Rename or move a file |
| walk | `(dir: String) -> Result[List[String], String]` | Recursively list all files in a directory tree |
| remove_all | `(path: String) -> Result[Unit, String]` | Recursively delete a directory and all its contents |
| file_size | `(path: String) -> Result[Int, String]` | Get file size in bytes |
| temp_dir | `() -> String` | Get the system temporary directory path |
| stat | `(path: String) -> Result[{size: Int, is_dir: Bool, is_file: Bool, modified: Int}, String]` | Get file metadata: size, type, and modification time |
| glob | `(pattern: String) -> Result[List[String], String]` | Find files matching a glob pattern |
| create_temp_file | `(prefix: String) -> Result[String, String]` | Create a temporary file with a given prefix, return its path |
| create_temp_dir | `(prefix: String) -> Result[String, String]` | Create a temporary directory with a given prefix, return its path |
| is_symlink | `(path: String) -> Bool` | Check if a path is a symbolic link |
| modified_at | `(path: String) -> Result[Int, String]` | Get file modification time as Unix timestamp (seconds) |

---

## http (26 functions)

The http module spans four categories: server, response builders (pure), request accessors (pure), and HTTP client (effect). Only 10 of 26 functions are effect functions.

### Server (1 function, effect)

| Function | Signature | Description |
|----------|-----------|-------------|
| serve | `(port: Int, f: Fn[Unknown] -> Unknown) -> Unit` | Start an HTTP server on the given port with a request handler |

### Response Builders (6 functions, pure)

| Function | Signature | Description |
|----------|-----------|-------------|
| response | `(status: Int, body: String) -> Response` | Create a plain text HTTP response with status code |
| json | `(status: Int, body: String) -> Response` | Create a JSON HTTP response with status code |
| with_headers | `(status: Int, body: String, headers: Map[String, String]) -> Response` | Create a response with custom headers |
| redirect | `(url: String) -> Response` | Create a 302 temporary redirect response |
| redirect_permanent | `(url: String) -> Response` | Create a 301 permanent redirect response |
| not_found | `(body: String) -> Response` | Create a 404 Not Found response |

### Response Accessors/Mutators (5 functions, pure)

| Function | Signature | Description |
|----------|-----------|-------------|
| status | `(resp: Response, code: Int) -> Response` | Set the status code on a response |
| body | `(resp: Response) -> String` | Get the body string from a response |
| set_header | `(resp: Response, key: String, value: String) -> Response` | Set a header on a response |
| get_header | `(resp: Response, key: String) -> Option[String]` | Get a header value from a response |
| set_cookie | `(resp: Response, name: String, value: String) -> Response` | Set a cookie on a response |

### Request Accessors (5 functions, pure)

| Function | Signature | Description |
|----------|-----------|-------------|
| req_method | `(req: Request) -> String` | Get the HTTP method of a request (GET, POST, etc.) |
| req_path | `(req: Request) -> String` | Get the URL path of a request |
| req_body | `(req: Request) -> String` | Get the body string of a request |
| req_header | `(req: Request, key: String) -> Option[String]` | Get a header value from a request |
| query_params | `(req: Request) -> Map[String, String]` | Get all query parameters from a request as a map |

### HTTP Client (9 functions, all effect)

| Function | Signature | Description |
|----------|-----------|-------------|
| get | `(url: String) -> Result[String, String]` | Send an HTTP GET request and return the response body |
| get_json | `(url: String) -> Result[Value, String]` | Send a GET request and parse the response as JSON |
| post | `(url: String, body: String) -> Result[String, String]` | Send an HTTP POST request with a body string |
| post_json | `(url: String, body: String) -> Result[Value, String]` | Send a POST request and parse the response as JSON |
| put | `(url: String, body: String) -> Result[String, String]` | Send an HTTP PUT request |
| patch | `(url: String, body: String) -> Result[String, String]` | Send an HTTP PATCH request |
| delete | `(url: String) -> Result[String, String]` | Send an HTTP DELETE request |
| get_with_headers | `(url: String, headers: Map[String, String]) -> Result[String, String]` | Send a GET request with custom headers |
| request | `(method: String, url: String, body: String, headers: Map[String, String]) -> Result[String, String]` | Send a custom HTTP request with method, URL, body, and headers |

---

## datetime (21 functions)

DateTime is represented as `Int` (Unix timestamp in seconds, UTC). Only `now()` is an effect function; all others are pure computations on integers or string parsing.

### Generation (4 functions)

| Function | Signature | Description |
|----------|-----------|-------------|
| now | `() -> Int` | Get the current time as a Unix timestamp (seconds, UTC) -- **effect** |
| from_parts | `(y: Int, m: Int, d: Int, h: Int, min: Int, s: Int) -> Int` | Create a timestamp from year, month, day, hour, minute, second (UTC) |
| parse_iso | `(s: String) -> Result[Int, String]` | Parse an ISO 8601 date string into a timestamp |
| from_unix | `(seconds: Int) -> Int` | Convert a Unix timestamp (identity function for documentation clarity) |

### Format (3 functions)

| Function | Signature | Description |
|----------|-----------|-------------|
| format | `(ts: Int, pattern: String) -> String` | Format a timestamp using a pattern string |
| to_iso | `(ts: Int) -> String` | Format a timestamp as ISO 8601 string |
| to_unix | `(ts: Int) -> Int` | Get the Unix timestamp value (identity function) |

### Part Access (7 functions)

| Function | Signature | Description |
|----------|-----------|-------------|
| year | `(ts: Int) -> Int` | Extract the year from a timestamp |
| month | `(ts: Int) -> Int` | Extract the month (1-12) from a timestamp |
| day | `(ts: Int) -> Int` | Extract the day of month (1-31) from a timestamp |
| hour | `(ts: Int) -> Int` | Extract the hour (0-23) from a timestamp |
| minute | `(ts: Int) -> Int` | Extract the minute (0-59) from a timestamp |
| second | `(ts: Int) -> Int` | Extract the second (0-59) from a timestamp |
| weekday | `(ts: Int) -> String` | Get the day of week as a string (Monday-Sunday) |

### Arithmetic (5 functions)

| Function | Signature | Description |
|----------|-----------|-------------|
| add_days | `(ts: Int, n: Int) -> Int` | Add n days to a timestamp |
| add_hours | `(ts: Int, n: Int) -> Int` | Add n hours to a timestamp |
| add_minutes | `(ts: Int, n: Int) -> Int` | Add n minutes to a timestamp |
| add_seconds | `(ts: Int, n: Int) -> Int` | Add n seconds to a timestamp |
| diff_seconds | `(a: Int, b: Int) -> Int` | Compute the difference in seconds between two timestamps |

### Comparison (2 functions)

| Function | Signature | Description |
|----------|-----------|-------------|
| is_before | `(a: Int, b: Int) -> Bool` | Check if timestamp a is before timestamp b |
| is_after | `(a: Int, b: Int) -> Bool` | Check if timestamp a is after timestamp b |

---

## Naming Convention Compliance

Cross-checked against stdlib-1.0.md naming rules:

### Passing

- **`to_*` infallible**: `to_iso` returns `String`, `to_unix` returns `Int` -- both infallible. PASS.
- **`parse` fallible**: `parse_iso` returns `Result[Int, String]` -- fallible. PASS.
- **`is_*` predicates**: `is_dir`, `is_file`, `is_symlink` (fs), `is_before`, `is_after` (datetime) -- all return `Bool`. PASS.
- **`from_*` target-side construction**: `from_parts`, `from_unix` (datetime) -- construct timestamp on the datetime side. PASS.
- **`get` returns Option**: `get_header` (http) returns `Option[String]`. PASS.
- **data-first**: All fs functions take `path` as first arg. All datetime part accessors take `ts` as first arg. All http response mutators take `resp` as first arg. PASS.

### Flags

1. **`fs.temp_dir` missing `effect = true`** -- This function reads a system property (`std::env::temp_dir()` in Rust). While deterministic on most systems, it is environment-dependent I/O. Every other fs function is marked `effect = true`. The omission appears to be an oversight. Consider adding `effect = true` for consistency with the module's "all effect" classification in the spec index.

2. **`http.exists` pattern gap** -- The fs module has `exists` (returns `Bool`), which follows the predicate convention but does not use the `is_*` prefix. This is acceptable because `is_exists` would be grammatically awkward, and `exists` is an established verb form used as a predicate in most languages. Informational only, not a violation.

3. **`http.json` function name shadows type** -- The function `http.json(status, body)` creates a JSON response. The name `json` is also a module name. This works because `http.json` is always module-qualified, but could cause confusion in documentation. Informational only.

4. **`datetime.parse_iso` vs `parse` convention** -- The spec says `parse` is the canonical name for fallible string interpretation (e.g., `int.parse`, `float.parse`). Here the function is `parse_iso` rather than just `parse`. This is justified because `parse` alone would be ambiguous (datetime strings come in many formats), and `parse_iso` clearly specifies the expected format. Consistent with "1 verb = 1 meaning" -- the `_iso` suffix disambiguates rather than violates. ACCEPTABLE.

### No Violations Found

All three modules conform to the stdlib-1.0.md naming conventions. The only actionable item is the missing `effect = true` on `fs.temp_dir`.
