# Conversion Verbs: `to_*`, `from_*`, `parse`, `stringify`

Analysis of Almide's type-conversion verb patterns across the stdlib.

---

## 1. Current Inventory

### 1.1 `to_*` (output type in name)

| Function | Signature | Semantics |
|----------|-----------|-----------|
| `int.to_string(n)` | `Int -> String` | Infallible numeric rendering |
| `int.to_float(n)` | `Int -> Float` | Infallible widening |
| `int.to_hex(n)` | `Int -> String` | Infallible radix rendering |
| `int.to_u32(a)` | `Int -> Int` | Truncation (mask) |
| `int.to_u8(a)` | `Int -> Int` | Truncation (mask) |
| `float.to_string(n)` | `Float -> String` | Infallible numeric rendering |
| `float.to_int(n)` | `Float -> Int` | Truncation toward zero |
| `float.to_fixed(n, d)` | `(Float, Int) -> String` | Formatted rendering |
| `string.to_upper(s)` | `String -> String` | Case transform |
| `string.to_lower(s)` | `String -> String` | Case transform |
| `string.to_bytes(s)` | `String -> List[Int]` | Encoding conversion |
| `string.to_int(s)` | `String -> Result[Int, String]` | **Parsing** (see 4.1) |
| `string.to_float(s)` | `String -> Result[Float, String]` | **Parsing** (see 4.1) |
| `datetime.to_iso(ts)` | `Int -> String` | Formatting |
| `datetime.to_unix(ts)` | `Int -> Int` | Identity (documentation) |
| `result.to_option(r)` | `Result[A, E] -> Option[A]` | Wrapper conversion |
| `result.to_err_option(r)` | `Result[A, E] -> Option[E]` | Wrapper conversion |
| `value.to_camel_case(v)` | `Value -> Value` | Key transform |
| `value.to_snake_case(v)` | `Value -> Value` | Key transform |
| `json.to_string(j)` | `Value -> Option[String]` | **Extraction** (see 1.5) |
| `json.to_int(j)` | `Value -> Option[Int]` | **Extraction** (see 1.5) |

### 1.2 `from_*` (input type in name)

| Function | Signature | Semantics |
|----------|-----------|-----------|
| `float.from_int(n)` | `Int -> Float` | Infallible widening |
| `string.from_bytes(bs)` | `List[Int] -> String` | Encoding conversion |
| `string.from_codepoint(n)` | `Int -> String` | Encoding conversion |
| `datetime.from_parts(...)` | `(Int*6) -> Int` | Construction |
| `datetime.from_unix(n)` | `Int -> Int` | Identity (documentation) |
| `map.from_entries(es)` | `List[(K,V)] -> Map[K,V]` | Construction |
| `map.from_list(xs, f)` | `(List[A], Fn) -> Map[K,V]` | Construction with transform |
| `json.from_string(s)` | `String -> Value` | Value wrapping |
| `json.from_int(n)` | `Int -> Value` | Value wrapping |
| `json.from_float(n)` | `Float -> Value` | Value wrapping |
| `json.from_bool(b)` | `Bool -> Value` | Value wrapping |
| `json.from_map(m)` | `Map[String,Value] -> Value` | Value wrapping |

### 1.3 `parse` (string-to-type, fallible)

| Function | Signature | Semantics |
|----------|-----------|-----------|
| `int.parse(s)` | `String -> Result[Int, String]` | Decimal string to integer |
| `int.parse_hex(s)` | `String -> Result[Int, String]` | Hex string to integer |
| `float.parse(s)` | `String -> Result[Float, String]` | Numeric string to float |
| `json.parse(text)` | `String -> Result[Value, String]` | JSON text to Value tree |
| `datetime.parse_iso(s)` | `String -> Result[Int, String]` | ISO 8601 string to timestamp |
| `uuid.parse(s)` | `String -> Result[String, String]` | Validate + normalize UUID |

### 1.4 `stringify` (type-to-string, infallible)

| Function | Signature | Semantics |
|----------|-----------|-----------|
| `json.stringify(v)` | `Value -> String` | Value tree to compact JSON |
| `json.stringify_pretty(j)` | `Value -> String` | Value tree to indented JSON |
| `value.stringify(v)` | `Value -> String` | Value to JSON-like string |

### 1.5 `as_*` (type extraction from dynamic Value)

| Function | Module | Signature |
|----------|--------|-----------|
| `as_string` | json, value | `Value -> Option[String]` / `Result[String, String]` |
| `as_int` | json, value | `Value -> Option[Int]` / `Result[Int, String]` |
| `as_float` | json, value | `Value -> Option[Float]` / `Result[Float, String]` |
| `as_bool` | json, value | `Value -> Option[Bool]` / `Result[Bool, String]` |
| `as_array` | json, value | `Value -> Option[List[Value]]` / `Result[List[Value], String]` |

---

## 2. Cross-Language Comparison

### 2.1 Parsing: String to typed value

| Language | Int from string | Float from string | JSON from string |
|----------|----------------|-------------------|-----------------|
| **Rust** | `"42".parse::<i32>()` / `i32::from_str()` | `"3.14".parse::<f64>()` | `serde_json::from_str()` |
| **Go** | `strconv.Atoi()` / `strconv.ParseInt()` | `strconv.ParseFloat()` | `json.Unmarshal()` |
| **Python** | `int("42")` | `float("3.14")` | `json.loads()` |
| **Kotlin** | `"42".toInt()` / `"42".toIntOrNull()` | `"3.14".toDouble()` | `Gson().fromJson()` |
| **Swift** | `Int("42")` (failable init) | `Double("3.14")` | `JSONDecoder().decode()` |
| **TypeScript** | `parseInt("42")` / `Number("42")` | `parseFloat("3.14")` | `JSON.parse()` |
| **Gleam** | `int.parse("42")` | `float.parse("3.14")` | `json.decode()` |
| **Elm** | `String.toInt "42"` | `String.toFloat "3.14"` | `Json.Decode.decodeString` |
| **Almide** | `int.parse("42")` | `float.parse("3.14")` | `json.parse(text)` |

Key observations:

- **`parse` is the dominant verb** for string-to-type conversion in systems languages (Rust, Gleam, Go's `Parse*`). Almide aligns with this.
- Kotlin uses `toInt()` on String -- a `to_*` pattern. This is the minority approach.
- Python and Swift use constructor-style (`int()`, `Int()`). Not applicable to Almide's module-function model.
- `JSON.parse` is **universally understood** across all ecosystems.

### 2.2 Rendering: Typed value to string

| Language | Int to string | Float to string | Value to JSON |
|----------|--------------|-----------------|---------------|
| **Rust** | `i.to_string()` / `format!("{}", i)` | `f.to_string()` | `serde_json::to_string()` |
| **Go** | `strconv.Itoa(i)` / `fmt.Sprintf("%d", i)` | `strconv.FormatFloat()` | `json.Marshal()` |
| **Python** | `str(42)` | `str(3.14)` | `json.dumps()` |
| **Kotlin** | `42.toString()` | `3.14.toString()` | `Gson().toJson()` |
| **Swift** | `String(42)` | `String(3.14)` | `JSONEncoder().encode()` |
| **TypeScript** | `(42).toString()` / `` `${42}` `` | `(3.14).toString()` | `JSON.stringify()` |
| **Gleam** | `int.to_string(42)` | `float.to_string(3.14)` | `json.to_string()` |
| **Almide** | `int.to_string(42)` | `float.to_string(3.14)` | `json.stringify(v)` |

Key observations:

- `to_string` is the standard verb in Rust, Kotlin, Gleam, TypeScript. Almide matches.
- `stringify` is JS/JSON-specific. `json.stringify` is universally understood.
- Gleam uses `json.to_string()` instead of `json.stringify()`. But the JSON ecosystem strongly favors `stringify`.

### 2.3 Inter-type conversion

| Language | Int to Float | Float to Int | Hex to/from Int |
|----------|-------------|-------------|-----------------|
| **Rust** | `i as f64` / `f64::from(i)` | `f as i64` | `format!("{:x}", n)` / `i64::from_str_radix()` |
| **Go** | `float64(i)` | `int(f)` | `fmt.Sprintf("%x", n)` / `strconv.ParseInt(s, 16, 64)` |
| **Python** | `float(42)` | `int(3.14)` | `hex(n)` / `int(s, 16)` |
| **Kotlin** | `i.toDouble()` | `f.toInt()` | `i.toString(16)` / `s.toInt(16)` |
| **Swift** | `Double(i)` | `Int(f)` | `String(n, radix: 16)` / `Int(s, radix: 16)` |
| **Gleam** | `int.to_float(i)` | `float.truncate(f)` | N/A |
| **Almide** | `int.to_float(n)` / `float.from_int(n)` | `float.to_int(n)` | `int.to_hex(n)` / `int.parse_hex(s)` |

Note: `int.to_float(n)` and `float.from_int(n)` are **exact duplicates** that exist in Almide today. They do the same thing. This is addressed in section 4.3.

---

## 3. Pattern Analysis

### 3.1 The `to_*` pattern

**Semantics**: "I have a value of type T. Convert it *to* type U."

The `to_*` verb is used on the **source module**. The conversion's *output* type appears in the name:

```
int.to_string(42)     -- source=Int, output=String, lives in `int` module
float.to_int(3.9)     -- source=Float, output=Int, lives in `float` module
string.to_bytes("Hi") -- source=String, output=List[Int], lives in `string` module
```

**Rule**: `to_*` should be **infallible** (always succeeds) or at worst lossy (truncation). This matches Rust's `Into` trait semantics and Kotlin's `.toInt()` (on numeric types -- where it truncates, not parses).

**Violation**: `string.to_int(s)` and `string.to_float(s)` return `Result`, meaning they are fallible. They are semantically **parsing**, not converting. This breaks the pattern.

### 3.2 The `from_*` pattern

**Semantics**: "Construct a value of type T *from* a value of type U."

The `from_*` verb is used on the **target module**. The conversion's *input* type appears in the name:

```
float.from_int(42)           -- target=Float, input=Int, lives in `float` module
string.from_bytes([72, 105]) -- target=String, input=List[Int], lives in `string` module
map.from_entries(pairs)       -- target=Map, input=List[(K,V)], lives in `map` module
```

**Rule**: `from_*` is a **constructor on the target type**. It may be fallible or infallible, but it is always called on the module that produces the result.

### 3.3 The `parse` pattern

**Semantics**: "Interpret a **string** (text) as a structured value."

```
int.parse("42")                           -- String -> Result[Int, String]
float.parse("3.14")                       -- String -> Result[Float, String]
json.parse('{"name": "Alice"}')           -- String -> Result[Value, String]
datetime.parse_iso("2024-01-15T12:00:00Z") -- String -> Result[Int, String]
uuid.parse("550e8400-...")                 -- String -> Result[String, String]
```

**Rule**: `parse` is always:
1. From **String** (text input)
2. Lives on the **target module** (the type being produced)
3. Returns **Result** (can fail)
4. Implies validation + structural interpretation

### 3.4 The `stringify` pattern

**Semantics**: "Serialize a structured value into a **string** (text)."

```
json.stringify(v)         -- Value -> String
json.stringify_pretty(j)  -- Value -> String
value.stringify(v)        -- Value -> String
```

**Rule**: `stringify` is the **inverse of `parse`** for serialization formats. It is always infallible, producing a String from a structured value. The verb is specific to formats with a canonical text representation (JSON, XML, TOML).

### 3.5 Summary of patterns

| Pattern | Direction | Lives on | Fallible? | Example |
|---------|-----------|----------|-----------|---------|
| `to_*` | source -> target | Source module | No (infallible) | `int.to_string(42)` |
| `from_*` | source -> target | Target module | Either | `float.from_int(42)` |
| `parse` | String -> T | Target module | Yes | `int.parse("42")` |
| `stringify` | T -> String | Format module | No | `json.stringify(v)` |
| `as_*` | Value -> T | Value module | Yes | `json.as_string(j)` |

---

## 4. Identified Problems

### 4.1 `string.to_int` / `string.to_float` vs `int.parse` / `float.parse`

This is the most significant redundancy in the conversion system.

| Source module call | Target module call | Identical semantics? |
|--------------------|--------------------|---------------------|
| `string.to_int(s)` | `int.parse(s)` | Yes -- both `String -> Result[Int, String]` |
| `string.to_float(s)` | `float.parse(s)` | Yes -- both `String -> Result[Float, String]` |

**Problems with `string.to_int`**:

1. **Breaks `to_*` semantics.** Every other `to_*` is infallible, but `string.to_int` returns `Result`. This is misleading -- a user seeing `to_*` expects it to always succeed.
2. **Wrong module placement.** The result type is `Int`, so the function logically belongs on `int`, not `string`. You ask the *target type* to accept a string, not ask the *source type* to become something.
3. **Redundant.** `int.parse(s)` does exactly the same thing and follows the established `parse` pattern shared with `float.parse`, `json.parse`, `uuid.parse`, `datetime.parse_iso`.

**Verdict**: `string.to_int` and `string.to_float` should be deprecated. `int.parse` and `float.parse` are the canonical forms.

**Kotlin counterargument**: Kotlin uses `"42".toInt()` as the primary conversion. But Kotlin's `toInt()` on String is a method on String itself, making it natural in OOP. Almide uses module-qualified calls (`string.to_int(s)` vs `int.parse(s)`), where the target-module pattern is clearer. Additionally, Kotlin now recommends `"42".toIntOrNull()` for safe parsing -- acknowledging that "toInt" on strings is really parsing, not converting.

### 4.2 `int.parse_hex` vs `int.from_hex`

Current: `int.parse_hex(s)` -- `String -> Result[Int, String]`

**Arguments for `parse_hex` (current)**:
- It is parsing a string. It follows the `parse` family: `parse`, `parse_hex`, `parse_iso`.
- The `parse_*` suffix pattern is clear: "parse this string in a specific format."
- Go: `strconv.ParseInt(s, 16, 64)` -- Parse with radix parameter.

**Arguments for `from_hex`**:
- `from_*` is the general "construct from" pattern.
- `from_hex` parallels `from_bytes`, `from_string`, `from_codepoint`, `from_entries`.

**Verdict**: Both patterns have merit. `parse_hex` is more precise (it emphasizes the *parsing* nature -- the input is text). `from_hex` is more consistent with the `from_*` construction pattern. Since `parse_hex` already exists and the input is String (making it genuinely a parse operation), **keep `parse_hex` as primary, add `from_hex` as alias** per the verb-reform-analysis recommendation.

### 4.3 `int.to_float(n)` vs `float.from_int(n)`

Both exist today. They do exactly the same thing: `Int -> Float`.

| Perspective | Function | Pattern | Module |
|-------------|----------|---------|--------|
| Source-centric | `int.to_float(n)` | `to_*` | int |
| Target-centric | `float.from_int(n)` | `from_*` | float |

In Rust, both `Into` (source-centric) and `From` (target-centric) coexist. Implementing `From<A> for B` automatically gives `A.into()`.

**Verdict**: Both are legitimate. In a module-function language like Almide, users will reach for whichever module they are already "thinking in". Keep both. This is not harmful redundancy -- it is **dual-perspective access** to the same operation.

However, for **documentation and teaching**, pick one as canonical:
- **Recommended canonical**: `int.to_float(n)` -- because the UFCS pipeline reads left-to-right: `42 |> int.to_float`. The `to_*` form chains better.
- `float.from_int(n)` remains available for when you are constructing a float and want to express "from int".

### 4.4 `json.to_string` / `json.to_int` vs `json.as_string` / `json.as_int`

The json module has two overlapping extraction patterns:

| Function | Signature | Key lookup? |
|----------|-----------|------------|
| `json.to_string(j)` | `Value -> Option[String]` | No |
| `json.as_string(j)` | `Value -> Option[String]` | No |
| `json.get_string(j, key)` | `(Value, String) -> Option[String]` | Yes |

`json.to_string` and `json.as_string` are identical in behavior. The `as_*` names were added later as aliases.

**Problem**: `json.to_string` is misleading. In every other module, `to_string` means "render this value as a string representation." But `json.to_string(j)` means "extract the string if this JSON value *is* a string." These are completely different operations.

**Verdict**: Deprecate `json.to_string` and `json.to_int` in favor of `json.as_string` and `json.as_int`. The `as_*` verb correctly conveys "treat this dynamic value as type T" (type narrowing / extraction), which is distinct from "convert this value to type T" (`to_*`).

### 4.5 `json.stringify` vs `value.stringify`

Both exist:
- `json.stringify(v)` -- `Value -> String` (JSON serialization)
- `value.stringify(v)` -- `Value -> String` (JSON-like string)

If they produce the same output, one should be deprecated. If `value.stringify` is intended to differ from JSON (e.g., different formatting), the name should be more explicit (e.g., `value.inspect` or `value.debug_string`).

**Verdict**: Keep `json.stringify` as the canonical JSON serialization verb. Rename `value.stringify` to `value.inspect` or keep it only if its output format genuinely differs from JSON.

---

## 5. Key Questions Answered

### Q1: `int.parse` vs `int.from_string` -- which is the standard verb?

**Answer: `int.parse`.**

`parse` is the correct verb because:
1. It emphasizes the **interpretation** of text -- not just type conversion, but structural analysis that can fail.
2. It matches industry consensus: Rust (`str::parse`), Gleam (`int.parse`), Go (`strconv.Parse*`), JS (`JSON.parse`, `parseInt`).
3. `from_string` would suggest a simple, possibly infallible construction -- but string-to-int conversion is inherently fallible and involves validation.
4. `from_string` would collide semantically with `json.from_string(s)`, which wraps a String *inside* a Value (not parsing at all).

### Q2: `json.parse` -- universally understood. Keep?

**Answer: Yes, absolutely.**

`json.parse` is one of the most universally recognized API calls in all of programming. Renaming it would be actively harmful. Every language (JS, Python's `json.loads`, Go's `json.Unmarshal`, Gleam's `json.decode`) has a "parse JSON text" function, and `json.parse` is the most intuitive name.

### Q3: `int.parse_hex` vs `int.from_hex` -- which naming pattern?

**Answer: `parse_hex` as primary, `from_hex` as alias.**

- `parse_hex` correctly signals "parse a *string* in hexadecimal format" -- it is a specialized parse.
- `from_hex` is a reasonable shorthand and follows the `from_*` construction pattern.
- The `parse_*` family (`parse`, `parse_hex`, `parse_iso`) is internally consistent and should be the canonical set.
- Adding `from_hex` as an alias provides discoverability for users who think in `from_*` terms.

### Q4: `string.to_int/to_float` -- redundant with `int.parse/float.parse`?

**Answer: Yes, redundant. Deprecate `string.to_int` and `string.to_float`.**

They violate the `to_*` contract (fallibility) and duplicate the `parse` functions on the target modules. Full rationale in section 4.1.

---

## 6. Canonical Conversion Pattern for Almide

### The Four Verbs

```
to_*        Infallible conversion, called on the SOURCE module.
            "I have X, give me Y."
            int.to_string(42)        -- always works
            float.to_int(3.9)        -- always works (truncates)

from_*      Construction on the TARGET module.
            "Build me a T from this U."
            float.from_int(42)       -- always works
            string.from_bytes(bs)    -- always works
            map.from_entries(pairs)   -- always works

parse       Fallible string interpretation, called on the TARGET module.
            "Read this text as a T."
            int.parse("42")          -- may fail
            json.parse(text)         -- may fail
            datetime.parse_iso(s)    -- may fail

stringify   Infallible serialization to string, called on the FORMAT module.
            "Render this value as text."
            json.stringify(v)        -- always works
```

### The One Rule

**`to_*` never returns Result.** If a conversion from String can fail, it is `parse`, not `to_*`.

### The Hierarchy

When multiple patterns could apply, prefer in this order:

1. **`parse`** for any `String -> Result[T, E]` (string interpretation)
2. **`to_*`** for any infallible `T -> U` on the source module
3. **`from_*`** for any construction on the target module
4. **`stringify`** for any `T -> String` on a format/serialization module
5. **`as_*`** for dynamic type narrowing from `Value` types

### The Auxiliary Verbs

| Verb | Usage | Example |
|------|-------|---------|
| `format` | Parameterized rendering to string | `datetime.format(ts, "%Y-%m-%d")` |
| `to_fixed` | Fixed-precision rendering | `float.to_fixed(3.14, 2)` |
| `to_hex` | Radix rendering | `int.to_hex(255)` |
| `parse_hex` | Radix parsing | `int.parse_hex("ff")` |
| `parse_iso` | Format-specific parsing | `datetime.parse_iso("2024-01-15T...")` |
| `stringify_pretty` | Pretty serialization | `json.stringify_pretty(j)` |

---

## 7. Migration Plan

### Phase 1: Deprecation annotations (pre-1.0)

| Deprecated | Canonical replacement | Reason |
|-----------|----------------------|--------|
| `string.to_int(s)` | `int.parse(s)` | Fallible conversion belongs on target module as `parse` |
| `string.to_float(s)` | `float.parse(s)` | Same reason |
| `json.to_string(j)` | `json.as_string(j)` | `to_string` implies rendering; `as_*` is extraction |
| `json.to_int(j)` | `json.as_int(j)` | Same reason |

### Phase 2: Aliases (pre-1.0)

| New alias | Points to | Reason |
|-----------|-----------|--------|
| `int.from_hex(s)` | `int.parse_hex(s)` | `from_*` discoverability |
| `result.flat_map(r, f)` | `result.and_then(r, f)` | Cross-module symmetry with `list.flat_map` |

### Phase 3: Clarification (1.x)

| Issue | Resolution |
|-------|-----------|
| `value.stringify(v)` | Rename to `value.inspect(v)` or merge with `json.stringify` |
| `json.from_string(s)` | Keep -- it wraps a String as `Value::Str`, it does NOT parse |

### Not changing

| Function | Reason to keep |
|----------|---------------|
| `int.parse(s)` | Industry standard verb for string interpretation |
| `json.parse(text)` | Universally understood |
| `int.to_float(n)` + `float.from_int(n)` | Dual-perspective access, both legitimate |
| `datetime.parse_iso(s)` | Clear format-qualified parse |
| `json.stringify(v)` | Matches JS convention, inverse of `json.parse` |

---

## 8. Verb Decision Matrix

Quick reference for "which verb do I use?"

| I have... | I want... | Use... |
|-----------|-----------|--------|
| `Int` | `String` | `int.to_string(n)` |
| `Int` | `Float` | `int.to_float(n)` |
| `Int` | hex `String` | `int.to_hex(n)` |
| `Float` | `String` | `float.to_string(n)` |
| `Float` | `Int` | `float.to_int(n)` |
| `Float` | fixed `String` | `float.to_fixed(n, d)` |
| `String` | `Int` | `int.parse(s)` |
| `String` | `Float` | `float.parse(s)` |
| `String` | `List[Int]` (bytes) | `string.to_bytes(s)` |
| `List[Int]` (bytes) | `String` | `string.from_bytes(bs)` |
| `String` (JSON text) | `Value` | `json.parse(text)` |
| `Value` | `String` (JSON text) | `json.stringify(v)` |
| `Value` | extract `String` | `json.as_string(j)` |
| `Value` | extract `Int` | `json.as_int(j)` |
| `String` (hex) | `Int` | `int.parse_hex(s)` |
| `String` (ISO date) | timestamp `Int` | `datetime.parse_iso(s)` |
| timestamp `Int` | ISO `String` | `datetime.to_iso(ts)` |
| `Result[A,E]` | `Option[A]` | `result.to_option(r)` |
| `List[(K,V)]` | `Map[K,V]` | `map.from_entries(es)` |
| `Int` (codepoint) | `String` (char) | `string.from_codepoint(n)` |
| `String` (char) | `Int` (codepoint) | `string.codepoint(s)` |
