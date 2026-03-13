# JSON Builder API [ACTIVE]

## Problem

The current `json.from_string`, `json.from_int`, etc. API is verbose for constructing JSON objects:

```almide
let obj = json.from_map(Map.from_list([
  ("name", json.from_string("Alice")),
  ("age", json.from_int(30)),
  ("address", json.from_map(Map.from_list([
    ("city", json.from_string("Tokyo"))
  ])))
]))
```

Every value needs manual wrapping. LLMs frequently forget wrappers or misbalance parentheses.

## Research Summary

Studied Rust (serde_json), Go (encoding/json), Kotlin (kotlinx.serialization), Swift (Codable), Elixir (Jason).

- **Best for LLMs**: Elixir (maps ARE JSON) and Rust (`json!` macro) — zero ceremony
- **Best typed approach without macros**: Kotlin's `buildJsonObject { put("k", v) }` builder DSL
- Almide has no macros, no dynamic types, no builder-receiver lambdas → need a different approach

## Proposed API

Short-named constructors + `json.object` for key-value pairs:

```almide
let person = json.object([
  ("name",    json.s("Alice")),
  ("age",     json.i(30)),
  ("active",  json.b(true)),
  ("address", json.object([
    ("city",  json.s("Tokyo")),
    ("zip",   json.s("100-0001"))
  ])),
  ("tags",    json.array([json.s("dev"), json.s("almide")])),
  ("score",   json.f(98.5)),
  ("notes",   json.null())
])
```

### Function Signatures

```
// Construction
json.object    : (List[(String, Json)]) -> Json
json.array     : (List[Json]) -> Json
json.s         : (String) -> Json
json.i         : (Int) -> Json
json.f         : (Float) -> Json
json.b         : (Bool) -> Json
json.null      : () -> Json
json.to_string : (Json) -> String

// Access / parsing (existing or straightforward)
json.parse      : (String) -> Result[Json, String]
json.get        : (Json, String) -> Option[Json]
json.get_string : (Json) -> Option[String]
json.get_int    : (Json) -> Option[Int]
json.get_float  : (Json) -> Option[Float]
json.get_bool   : (Json) -> Option[Bool]
json.get_list   : (Json) -> Option[List[Json]]
```

## Design Rationale

1. **One canonical way** — `json.s/i/f/b` replace `json.from_string/from_int/from_float/from_bool`. No aliases.
2. **7 tokens to memorize** — `object`, `array`, `s`, `i`, `f`, `b`, `null`. Minimal branching for LLMs.
3. **Short names reduce noise** — `json.s("x")` vs `json.from_string("x")` saves 8 chars per value. 10-field object → 80 chars saved.
4. **Structural nesting** — `json.object([...])` inside `json.object([...])`. Code structure mirrors JSON structure. No builder, no mutation.
5. **Deprecate `from_*`** — "one canonical way" principle. `json.s` is unambiguous in context.

## Alternatives Rejected

- **Builder DSL** (Kotlin-style): Requires mutable builder + receiver lambdas Almide doesn't support
- **Auto-coercion** (`String|Int → Json`): Would need union types or implicit conversions
- **Map literal sugar**: Language-level change for a library concern

## Implementation

1. Add `json.object`, `json.s`, `json.i`, `json.f`, `json.b` to `stdlib/defs/json.toml`
2. Implement in `emit_rust/core_runtime.txt` and `emit_ts_runtime.rs`
3. Deprecate `json.from_string`, `json.from_int`, `json.from_float`, `json.from_bool`
4. Add tests in `spec/stdlib/json_test.almd`
