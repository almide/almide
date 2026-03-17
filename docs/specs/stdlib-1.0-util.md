# Stdlib 1.0 — Utility & Minor Module Details

> Detailed function-level spec for the 11 utility/IO modules not covered in the main
> stdlib-1.0.md tables (string, int, float, list, map, option, result, json, value).
> Generated from `stdlib/defs/*.toml` definitions as of 2026-03-17.

---

## math (21 functions, pure)

Spec note: stdlib-1.0.md lists math at **19 functions**. The TOML defines **21**.
Extra functions not accounted for in the index: `factorial`, `choose`, `log_gamma`.
These are legitimate combinatorics/statistics additions — the index count should be updated to 21.

| Function | Signature | Description |
|----------|-----------|-------------|
| min | `(a: Int, b: Int) -> Int` | Return the smaller of two integers |
| max | `(a: Int, b: Int) -> Int` | Return the larger of two integers |
| abs | `(n: Int) -> Int` | Return the absolute value of an integer |
| pow | `(base: Int, exp: Int) -> Int` | Raise an integer base to an integer exponent |
| pi | `() -> Float` | Return the mathematical constant pi (3.14159...) |
| e | `() -> Float` | Return Euler's number e (2.71828...) |
| sin | `(x: Float) -> Float` | Return the sine of an angle in radians |
| cos | `(x: Float) -> Float` | Return the cosine of an angle in radians |
| tan | `(x: Float) -> Float` | Return the tangent of an angle in radians |
| log | `(x: Float) -> Float` | Return the natural logarithm (base e) of a float |
| exp | `(x: Float) -> Float` | Return e raised to the given power |
| sqrt | `(x: Float) -> Float` | Return the square root of a float |
| log10 | `(x: Float) -> Float` | Return the base-10 logarithm of a float |
| log2 | `(x: Float) -> Float` | Return the base-2 logarithm of a float |
| sign | `(n: Int) -> Int` | Return the sign of an integer: -1, 0, or 1 |
| fmin | `(a: Float, b: Float) -> Float` | Return the smaller of two floats |
| fmax | `(a: Float, b: Float) -> Float` | Return the larger of two floats |
| fpow | `(base: Float, exp: Float) -> Float` | Raise a float base to a float exponent |
| factorial | `(n: Int) -> Int` | Return the factorial of a non-negative integer |
| choose | `(n: Int, k: Int) -> Int` | Return the binomial coefficient C(n, k) |
| log_gamma | `(x: Float) -> Float` | Return the natural logarithm of the gamma function at x |

### Naming audit

| Flag | Function | Issue |
|------|----------|-------|
| WARN | `fmin` | Prefix `f` for float variants is ad-hoc. `float.min` already exists in the `float` module. Consider whether `math.min_f` or removing in favor of `float.min` is cleaner. Not a spec *violation* per se, but diverges from the "1 verb = 1 meaning" principle since `min` and `fmin` coexist in the same module with different types. |
| WARN | `fmax` | Same concern as `fmin` — float variant prefix is non-standard. |
| WARN | `fpow` | Same concern — `pow` and `fpow` coexist. |
| OK | All others | Conform to spec naming conventions. |

---

## regex (8 functions, pure)

Count matches spec (8).

| Function | Signature | Description |
|----------|-----------|-------------|
| is_match | `(pat: String, s: String) -> Bool` | Check if a pattern matches anywhere in a string |
| full_match | `(pat: String, s: String) -> Bool` | Check if a pattern matches the entire string |
| find | `(pat: String, s: String) -> Option[String]` | Find the first match of a pattern in a string |
| find_all | `(pat: String, s: String) -> List[String]` | Find all non-overlapping matches of a pattern |
| replace | `(pat: String, s: String, rep: String) -> String` | Replace all matches of a pattern with a replacement string |
| replace_first | `(pat: String, s: String, rep: String) -> String` | Replace the first match of a pattern |
| split | `(pat: String, s: String) -> List[String]` | Split a string by a regex pattern |
| captures | `(pat: String, s: String) -> Option[List[String]]` | Extract capture groups from the first match |

### Naming audit

| Flag | Function | Issue |
|------|----------|-------|
| WARN | `is_match` | The `is_*` convention is for Bool predicates, which this follows. However, data-first principle says the first argument should be the data being operated on. Here `pat` (the pattern) is first, not `s` (the string data). This is consistent across all regex functions (pattern-first) and matches the mental model of `regex.is_match(pattern, string)`, but technically violates data-first. This is an accepted module-level exception since `pat` is the "regex data". |
| WARN | `replace` / `replace_first` | Argument order is `(pat, s, rep)` — pattern-first. In `string.replace(s, from, to)` the string is first. This is consistent within the regex module but users must remember the difference. Not a naming violation, but an ordering inconsistency across modules. |
| OK | All others | Conform to spec naming conventions. |

---

## io (3 functions, all effect)

Count matches spec (3).

| Function | Signature | Description |
|----------|-----------|-------------|
| read_line | `effect () -> String` | Read a single line from standard input |
| print | `effect (s: String) -> Unit` | Print a string to stdout without a trailing newline |
| read_all | `effect () -> String` | Read all of standard input as a single string |

### Naming audit

No violations. All names are clear, imperative verbs. No prefix/suffix conflicts.

---

## process (6 functions, all effect)

Count matches spec (6).

| Function | Signature | Description |
|----------|-----------|-------------|
| exec | `effect (cmd: String, args: List[String]) -> Result[String, String]` | Execute a command and return its stdout as a string |
| exit | `effect (code: Int) -> Unit` | Exit the process with the given status code |
| stdin_lines | `effect () -> Result[List[String], String]` | Read all lines from standard input |
| exec_in | `effect (dir: String, cmd: String, args: List[String]) -> Result[String, String]` | Execute a command in a specific working directory |
| exec_with_stdin | `effect (cmd: String, args: List[String], input: String) -> Result[String, String]` | Execute a command with input piped to its stdin |
| exec_status | `effect (cmd: String, args: List[String]) -> Result[{code: Int, stdout: String, stderr: String}, String]` | Execute a command and return exit code, stdout, and stderr |

### Naming audit

| Flag | Function | Issue |
|------|----------|-------|
| WARN | `stdin_lines` | This overlaps with `io.read_line` / `io.read_all` in purpose. The name uses a noun (`stdin_lines`) rather than a verb, which is inconsistent with the imperative naming style of the rest of the module (`exec`, `exit`). Consider `read_stdin_lines` for consistency with verb-first naming. |
| OK | All others | Conform to spec naming conventions. `exec_in`, `exec_with_stdin`, `exec_status` are clear compound names. |

---

## env (9 functions, 8 effect + 1 pure)

Count matches spec (9).

| Function | Signature | Description |
|----------|-----------|-------------|
| unix_timestamp | `effect () -> Int` | Get the current Unix timestamp in seconds |
| args | `effect () -> List[String]` | Get the command-line arguments as a list of strings |
| get | `effect (name: String) -> Option[String]` | Get the value of an environment variable, or none if not set |
| set | `effect (name: String, value: String) -> Unit` | Set an environment variable |
| cwd | `effect () -> Result[String, String]` | Get the current working directory |
| millis | `effect () -> Int` | Get the current time in milliseconds since epoch |
| sleep_ms | `effect (ms: Int) -> Unit` | Sleep for the given number of milliseconds |
| temp_dir | `effect () -> String` | Get the system temporary directory path |
| os | `() -> String` | Get the operating system name (linux, macos, windows) |

### Naming audit

| Flag | Function | Issue |
|------|----------|-------|
| NOTE | `os` | Pure (no `effect` flag in TOML). This is correct since the OS doesn't change at runtime, but the spec index says env is "effect" without noting the exception. |
| WARN | `unix_timestamp` | Overlaps conceptually with `millis`. Both return "current time" in different units. Consider whether one should be in `datetime` instead. Not a naming violation, but a cohesion concern. |
| OK | All others | Conform to spec naming conventions. `get`/`set` follow standard accessor patterns. |

---

## log (8 functions, all effect)

Count matches spec (8).

| Function | Signature | Description |
|----------|-----------|-------------|
| debug | `effect (msg: String) -> Unit` | Log a debug message to stderr |
| info | `effect (msg: String) -> Unit` | Log an info message to stderr |
| warn | `effect (msg: String) -> Unit` | Log a warning message to stderr |
| error | `effect (msg: String) -> Unit` | Log an error message to stderr |
| debug_with | `effect (msg: String, fields: List[(String, String)]) -> Unit` | Log a debug message with structured key-value fields |
| info_with | `effect (msg: String, fields: List[(String, String)]) -> Unit` | Log an info message with structured key-value fields |
| warn_with | `effect (msg: String, fields: List[(String, String)]) -> Unit` | Log a warning message with structured key-value fields |
| error_with | `effect (msg: String, fields: List[(String, String)]) -> Unit` | Log an error message with structured key-value fields |

### Naming audit

No violations. The `*_with` suffix for structured variants is a clear, consistent pattern.

---

## random (4 functions, all effect)

Count matches spec (4).

| Function | Signature | Description |
|----------|-----------|-------------|
| int | `effect (min: Int, max: Int) -> Int` | Generate a random integer between min and max (inclusive) |
| float | `effect () -> Float` | Generate a random float between 0.0 and 1.0 |
| choice | `effect [T](xs: List[T]) -> Option[T]` | Pick a random element from a list, or none if empty |
| shuffle | `effect [T](xs: List[T]) -> List[T]` | Return a randomly shuffled copy of a list |

### Naming audit

| Flag | Function | Issue |
|------|----------|-------|
| NOTE | `int`, `float` | These are type names used as function names, which is unusual. In other modules, `int` and `float` are types. This works because the module qualifier disambiguates (`random.int(1, 10)`), but could confuse LLMs in unqualified contexts. Not a violation of any stated rule. |
| OK | All others | Conform to spec naming conventions. |

---

## crypto (4 functions, all effect)

Spec note: stdlib-1.0.md lists crypto at **3 functions** with examples "sha256/hmac_sha256/hmac_verify".
The TOML defines **4 functions**: `random_bytes`, `random_hex`, `hmac_sha256`, `hmac_verify`.
There is no `sha256` function in the TOML. The spec index is out of date.
The count should be updated to **4**, and the example list should read "random_bytes/random_hex/hmac_sha256/hmac_verify".

| Function | Signature | Description |
|----------|-----------|-------------|
| random_bytes | `effect (n: Int) -> Result[List[Int], String]` | Generate n cryptographically secure random bytes |
| random_hex | `effect (n: Int) -> Result[String, String]` | Generate a random hex string of n bytes (2n hex chars) |
| hmac_sha256 | `effect (key: String, data: String) -> Result[String, String]` | Compute HMAC-SHA256 of data with a key, returning hex digest |
| hmac_verify | `effect (key: String, data: String, signature: String) -> Result[Bool, String]` | Verify an HMAC-SHA256 signature |

### Naming audit

| Flag | Function | Issue |
|------|----------|-------|
| WARN | `random_bytes`, `random_hex` | These are generation functions, not cryptographic operations. The `random_` prefix groups them visually but "generate" is the implied verb. Consider whether these belong in `random` module instead (as crypto-secure variants), or if the current placement is intentional (crypto-grade randomness is a security concern, not a convenience concern). Placement is defensible, but the naming collision with the `random` module is worth noting. |
| WARN | `hmac_sha256` | Argument order is `(key, data)` — the "key" is first, not the "data". The data-first principle would suggest `(data, key)`. However, HMAC convention in most APIs is key-first, so this follows domain convention over Almide convention. Acceptable trade-off. |
| OK | `hmac_verify` | Follows the same key-first convention as `hmac_sha256`. Consistent within the module. |

---

## uuid (6 functions, 2 effect + 4 pure)

Spec note: stdlib-1.0.md lists uuid at **4 functions** with examples "v4/v5/parse/is_valid".
The TOML defines **6 functions**: `v4`, `v5`, `parse`, `is_valid`, `nil`, `version`.
Extra functions not in the spec index: `nil`, `version`.
The count should be updated to **6**.

| Function | Signature | Description |
|----------|-----------|-------------|
| v4 | `effect () -> Result[String, String]` | Generate a random UUID v4 |
| v5 | `effect (namespace: String, name: String) -> Result[String, String]` | Generate a deterministic UUID v5 from namespace and name |
| parse | `(s: String) -> Result[String, String]` | Parse and validate a UUID string |
| is_valid | `(s: String) -> Bool` | Check if a string is a valid UUID |
| nil | `() -> String` | Return the nil UUID (all zeros) |
| version | `(s: String) -> Result[Int, String]` | Extract the version number from a UUID string |

### Naming audit

| Flag | Function | Issue |
|------|----------|-------|
| WARN | `v5` | Marked as `effect = true` in the TOML, but UUID v5 is deterministic (same namespace + name always produces the same UUID). It requires no randomness or I/O. This should be `effect = false`. The spec says "v4 のみ effect", confirming v5 should be pure. |
| NOTE | `parse` | Returns `Result[String, String]` — the Ok value is just the normalized UUID string. This follows the `parse` = fallible convention correctly. However, the Rust template uses `?` (`almide_rt_uuid_parse(&*{s})?`) while `effect = false`, meaning callers in non-effect contexts would get a compile error. This is a codegen concern, not a naming issue. |
| OK | All others | Conform to spec naming conventions. `is_valid` follows `is_*` = Bool predicate. |

---

## testing (7 functions, pure)

Count matches spec (7).

| Function | Signature | Description |
|----------|-----------|-------------|
| assert_throws | `(f: fn() -> Unit, expected: String) -> Unit` | Assert that a function throws an error containing the expected message |
| assert_contains | `(haystack: String, needle: String) -> Unit` | Assert that a string contains a substring |
| assert_approx | `(a: Float, b: Float, tolerance: Float) -> Unit` | Assert two floats are approximately equal within tolerance |
| assert_gt | `(a: Int, b: Int) -> Unit` | Assert that a is greater than b |
| assert_lt | `(a: Int, b: Int) -> Unit` | Assert that a is less than b |
| assert_some | `(opt: Option[String]) -> Unit` | Assert that an Option is some (not none) |
| assert_ok | `(result: Result[String, String]) -> Unit` | Assert that a Result is ok (not err) |

Note: The spec index mentions "assert/assert_eq/assert_ne" but these are **language builtins** (handled by the `test` block), not stdlib functions. The TOML only defines the 7 *extended* assertion helpers listed above.

### Naming audit

| Flag | Function | Issue |
|------|----------|-------|
| WARN | `assert_some` | Parameter type is `Option[String]` — hardcoded to String. Should be generic `Option[T]` to match `assert_ok`. This is a type limitation, not a naming issue. |
| WARN | `assert_ok` | Parameter type is `Result[String, String]` — hardcoded to String/String. Should ideally be generic `Result[T, E]`. Same concern as `assert_some`. |
| OK | All others | Conform to spec naming conventions. `assert_*` prefix is consistent. |

---

## error (3 functions, pure)

Count matches spec (3).

| Function | Signature | Description |
|----------|-----------|-------------|
| context | `[T, E](r: Result[T, E], msg: String) -> Result[T, String]` | Add context message to an error result |
| message | `[T](r: Result[T, String]) -> String` | Extract the error message from a Result, or empty string if ok |
| chain | `(outer: String, cause: String) -> String` | Chain two error messages with a cause separator |

Note: The spec index lists the functions as "message/wrap/chain". The TOML has `context` instead of `wrap`. This is either a spec typo or a rename that happened after the spec was written.

### Naming audit

| Flag | Function | Issue |
|------|----------|-------|
| WARN | `context` vs `wrap` | The spec index says `wrap` but the TOML defines `context`. The name `context` is more descriptive (it adds contextual information to an error), while `wrap` implies wrapping the entire error. `context` is the better name — the spec index should be updated. |
| OK | All others | Conform to spec naming conventions. |

---

## Summary of Spec Index Discrepancies

| Module | Spec count | TOML count | Delta | Details |
|--------|-----------|------------|-------|---------|
| math | 19 | 21 | +2 | `factorial`, `choose`, `log_gamma` not in index |
| crypto | 3 | 4 | +1 | `random_hex` not in index; `sha256` listed but absent from TOML |
| uuid | 4 | 6 | +2 | `nil`, `version` not in index |
| error | 3 (message/wrap/chain) | 3 (context/message/chain) | 0 | `wrap` renamed to `context` in TOML |

All other modules (regex, io, process, env, log, random, testing) match their spec counts exactly.

## Summary of Naming Convention Flags

| Severity | Module | Function(s) | Issue |
|----------|--------|-------------|-------|
| WARN | math | `fmin`, `fmax`, `fpow` | Ad-hoc `f` prefix for float variants; `float` module already has `min`/`max` |
| WARN | regex | all functions | Pattern-first arg order diverges from data-first principle (accepted exception) |
| WARN | process | `stdin_lines` | Noun-form name; verb-first (`read_stdin_lines`) would be more consistent |
| WARN | crypto | `random_bytes`, `random_hex` | Name collision with `random` module; placement is defensible but confusing |
| WARN | crypto | `hmac_sha256` | Key-first arg order diverges from data-first (follows domain convention) |
| WARN | uuid | `v5` | Marked as effect but is deterministic; should be pure |
| WARN | testing | `assert_some`, `assert_ok` | Type params hardcoded to String instead of generic |
| WARN | error | `context` | Spec index says `wrap` but TOML says `context`; spec needs update |
