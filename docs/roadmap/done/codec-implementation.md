<!-- description: Three-layer codec implementation: compiler, format library, user code -->
<!-- done: 2026-03-15 -->
# Codec Implementation Plan

## 3-Layer Model

```
Layer 1: Codec (compiler)         T ←→ Value
Layer 2: Format (library)         Value ←→ String/Bytes
Layer 3: User code                T ←→ String (composed via pipes)
```

```
encode: T ──.encode()──▶ Value ──json.stringify──▶ String
                               ──yaml.stringify──▶ String
                               ──toml.stringify──▶ String

decode: String ──json.parse──▶ Value ──T.decode()──▶ Result[T, E]
        String ──yaml.parse──▶ Value ──T.decode()──▶ Result[T, E]

transform: Value ──rename_keys──▶ Value  (naming strategy)
           Value ──set_path──▶ Value     (local operations)
           Value ──json→yaml──▶ Value    (format conversion, no type needed)
```

**Types don't know about formats. Formats don't know about types. Value is the sole contact point.**

## Value type (universal data model)

```almide
// Defined in value module
type Value =
  | Null
  | Bool(Bool)
  | Int(Int)
  | Float(Float)
  | Str(String)
  | Array(List[Value])
  | Object(List[(String, Value)])
```

Variant constructors are qualified under the `value` module:
```almide
value.Null
value.Str("hello")
value.Object([("name", value.Str("Alice"))])
```

Type name collisions (`String`, `Int`, etc.) are resolved by module scope. No `V` prefix needed.

The data models of JSON / YAML / TOML / msgpack all map to this.
TOML datetime is stored as `Str("2024-01-15T10:30:00Z")`; toml.stringify detects ISO 8601 and converts to TOML datetime.

### Value accessor functions (stdlib: value module)

```almide
// Field access (returns Result — chainable with ?)
fn value.field(v: Value, key: String) -> Result[Value, String]
fn value.index(v: Value, i: Int) -> Result[Value, String]

// Type conversion (returns Result)
fn value.as_string(v: Value) -> Result[String, String]
fn value.as_int(v: Value) -> Result[Int, String]
fn value.as_float(v: Value) -> Result[Float, String]
fn value.as_bool(v: Value) -> Result[Bool, String]
fn value.as_array(v: Value) -> Result[List[Value], String]
fn value.as_object(v: Value) -> Result[List[(String, Value)], String]

// Construction shortcuts
fn value.str(s: String) -> Value = Str(s)
fn value.int(n: Int) -> Value = Int(n)
fn value.object(pairs: List[(String, Value)]) -> Value = Object(pairs)
fn value.array(items: List[Value]) -> Value = Array(items)
```

`value.field` returns `err("missing field 'name'")` if the key doesn't exist.
`value.as_string` returns `err("expected String but got Int")` if the type doesn't match.
All return `Result`, so they chain naturally with `?`.

### Obj internal representation

`Obj(List[(String, Value)])` preserves insertion order. Field lookup during decode is linear search O(n).

- Small structs (<=20 fields) — don't worry about it. Most practical JSON objects are this size
- Large structs — convert to `Map[String, Value]` once inside the decode function, then look up
- Manual codec — write by hand when performance matters

## Codec convention

```almide
type Person: Codec = { name: String, age: Int, active: Bool = true }
```

`: Codec` declares that `T.encode` and `T.decode` exist. The compiler auto-derives them:

```almide
// auto-generated:
fn Person.encode(p: Person) -> Value =
  Object([("name", Str(p.name)), ("age", Int(p.age)), ("active", Bool(p.active))])

fn Person.decode(v: Value) -> Result[Person, String] = ...
```

### Nested types — encode

```almide
type Address: Codec = { city: String, zip: String }
type Person: Codec = { name: String, address: Address }

// Person.encode calls Address.encode:
fn Person.encode(p: Person) -> Value =
  Object([("name", Str(p.name)), ("address", Address.encode(p.address))])
```

### Nested types — decode (type dispatch)

Auto-derive inspects field types and selects the appropriate decode function:

```
Field type           → Generated decode code
──────────────      ──────────────────────
String            value.as_string(v)?
Int               value.as_int(v)?
Float             value.as_float(v)?
Bool              value.as_bool(v)?
Named("Address")  Address.decode(v)?       ← Codec guarantee check
List[T]           value.as_arr(v)? |> list.map((x) => T.decode(x)?)
Option[T]         field missing → none, Null → none, other → some(T.decode(v)?)
```

```almide
type Team: Codec = { name: String, leader: Person, members: List[Person] }

// auto-generated decode — all chained with value.field + ?:
fn Team.decode(v: Value) -> Result[Team, String] = {
  let name = value.field(v, "name")? |> value.as_string?
  let leader = value.field(v, "leader")? |> Person.decode?
  let members = value.field(v, "members")? |> value.as_array?
    |> list.map((x) => Person.decode(x))?
  ok(Team { name: name, leader: leader, members: members })
}
```

Decode generation patterns:
- `value.field(v, "key")?` — field retrieval (missing → `err("missing field 'key'"`)
- `|> value.as_string?` — primitive type conversion (type mismatch → `err("expected String but got Int")`)
- `|> Person.decode?` — recursive decode for nested Codec types
- `|> value.as_array? |> list.map(...)` — decode each List element
- `Option[T]` fields — if `value.field` errors then `none`, if successful then `some(decode?)`
- field default — if `value.field` errors then use default value

### Codec constraint verification

When auto-deriving `Team: Codec`, if a field type is Named(Person), then `Person` must also have Codec.

**Verification timing**: Reference `type_conventions` in `generate_auto_derives` (lowerer).

```
error: field 'leader' has type Person which does not derive Codec
  --> app.almd:3
  hint: Add `: Codec` to the type declaration: type Person: Codec = ...
```

No traits or protocols needed. Guaranteed by static checks during auto-derive generation.

### Variant types

```almide
type Shape: Codec = Circle(radius: Float) | Rect(w: Float, h: Float)

// Tagged (default):
// Circle(3.0) → Object([("Circle", Object([("radius", Float(3.0))]))])
```

## Format modules (libraries)

Each format provides a **3-layer API**:

```
Layer 1 (internal):  stringify / parse       — Value ↔ text
Layer 2 (primary):   encode / decode         — T ↔ text (convenience, LLMs use this)
Layer 3 (advanced):  pipe composition        — encode() |> stringify (for customization)
```

### JSON (stdlib)

```almide
// Layer 1: Value ↔ text (implemented by format provider)
fn json.stringify(v: Value) -> String
fn json.stringify_pretty(v: Value) -> String
fn json.parse(text: String) -> Result[Value, String]

// Layer 2: T ↔ text (convenience — LLMs write this)
fn json.encode[T](value: T) -> String =
  T.encode(value) |> json.stringify

fn json.decode[T](text: String) -> Result[T, String] =
  json.parse(text)? |> T.decode

// Usage (what LLMs write):
let text = json.encode(person)
let p = json.decode[Person](input)?
```

### YAML (stdlib or package)

```almide
fn yaml.stringify(v: Value) -> String
fn yaml.parse(text: String) -> Result[Value, String]

// Same convenience pattern:
fn yaml.encode[T](value: T) -> String =
  T.encode(value) |> yaml.stringify

fn yaml.decode[T](text: String) -> Result[T, String] =
  yaml.parse(text)? |> T.decode

// Usage (Person definition unchanged):
let yaml_text = yaml.encode(person)
let p = yaml.decode[Person](yaml_input)?
```

### User-defined formats

```almide
// Implementers only write Layer 1:
fn csv.stringify(v: Value) -> String = ...
fn csv.parse(text: String) -> Result[Value, String] = ...

// Layer 2 convenience follows the same pattern:
fn csv.encode[T](value: T) -> String =
  T.encode(value) |> csv.stringify

fn csv.decode[T](text: String) -> Result[T, String] =
  csv.parse(text)? |> T.decode
```

**Design decision: the convenience boilerplate is intentional.** The 4 lines of `T.encode(value) |> XXX.stringify` are duplicated across all formats. Eliminating this requires higher-order modules or traits, neither of which Almide has. Copying 4 lines is judged not worth the cost of introducing traits.

### Final form of LLM-written code

```almide
type Config: Codec = {
  host: String = "localhost",
  port: Int = 8080,
  debug: Bool = false
}

// Save and load with JSON
let text = json.encode(config)
let loaded = json.decode[Config](text)?

// Same type works with YAML
let yaml_text = yaml.encode(config)
let from_yaml = yaml.decode[Config](yaml_text)?
```

## Format provider implementation guide

To add a format, just write 2 functions for `Value ↔ external representation`. No type knowledge needed.

### Minimal implementation: stringify

```almide
// my_format/mod.almd
fn stringify(v: Value) -> String =
  match v {
    Null          => "null"
    Bool(b)      => if b then "true" else "false"
    Int(n)       => int.to_string(n)
    Float(f)     => float.to_string(f)
    Str(s)       => "\"" ++ escape(s) ++ "\""
    Array(items)  => "[" ++ items |> list.map(stringify) |> string.join(", ") ++ "]"
    Object(pairs) => "{" ++ pairs |> list.map((k, v) => "\"" ++ k ++ "\": " ++ stringify(v)) |> string.join(", ") ++ "}"
  }
```

That alone makes `person.encode() |> my_format.stringify` work. No need to know Person's type definition.

### Minimal implementation: parse

```almide
fn parse(text: String) -> Result[Value, String] = {
  // Tokenize text → recursively build Value
  // Format-specific parser logic
  // Output is always Value type
}
```

### Format-specific options

```almide
type MyFormatOptions = { pretty: Bool = false, indent: Int = 2 }

fn stringify_with(v: Value, opts: MyFormatOptions) -> String = ...

// Usage:
person.encode() |> my_format.stringify_with(MyFormatOptions { pretty: true })
```

Options are arguments on the format side. No impact on the Codec side.

### Binary formats

```almide
// Value ↔ Bytes (instead of String)
fn msgpack.to_bytes(v: Value) -> List[Int] = ...
fn msgpack.from_bytes(b: List[Int]) -> Result[Value, String] = ...

// Usage:
let bytes = person.encode() |> msgpack.to_bytes
let p2 = msgpack.from_bytes(bytes)? |> Person.decode
```

### What format providers need to know

1. **Input is always Value** — handle all cases with pattern matching on 7 variants
2. **Output is always Value** — parse must return Value
3. **No type knowledge needed** — doesn't matter if it's Person or Config
4. **Errors are Result** — return parse failures as `err("message")`
5. **Nesting is recursive** — just recursively process Arr and Obj contents
6. **Testing**: if `json.parse(text)? |> my_format.stringify |> my_format.parse` roundtrips, it's correct

## Consumer use cases

### JSON encode/decode

```almide
type Person: Codec = { name: String, age: Int }

let alice = Person { name: "Alice", age: 30 }

// encode
let json_text = alice.encode() |> json.stringify
// → '{"name":"Alice","age":30}'

// decode
let bob = json.parse(input)? |> Person.decode
```

### Same type with YAML

```almide
// Person definition unchanged

let yaml_text = alice.encode() |> yaml.stringify
// → "name: Alice\nage: 30\n"

let carol = yaml.parse(yaml_input)? |> Person.decode
```

### Format conversion (no types needed)

```almide
// Convert JSON → YAML without going through types
let value = json.parse(json_text)?
let yaml_text = yaml.stringify(value)
```

### naming strategy

```almide
type ApiResponse: Codec = { userId: String, createdAt: String }

// encode uses field names as-is
let v = response.encode()  // Object([("userId", ...), ("createdAt", ...)])

// Insert a Value transformation function when snake_case is needed
let v_snake = v |> value.rename_keys(to_snake_case)
let text = v_snake |> json.stringify
// → '{"user_id":"...","created_at":"..."}'
```

## Generic constraints and Codec

```almide
// Check T.encode existence at mono time
fn json.encode_typed[T](value: T) -> String =
  T.encode(value) |> json.stringify
```

Since Almide has no traits, constraints are guaranteed by function existence checks at monomorphization time.
Error messages are improved using `: Codec` metadata:
- ❌ `T.encode not found`
- ✅ `type Foo is not a Codec. Declare it with type Foo: Codec = ...`

## Implementation status

```
Phase 0: Value type ✅
  └─ Core (auto-import, platform-independent)
  └─ TOML definition + Runtime crate (`runtime/rust/src/value.rs`)
  └─ All functions unified with `almide_rt_` prefix
  └─ No import needed (registered in PRELUDE_MODULES + STDLIB_MODULES)

Phase 1: Codec auto-derive ✅
  └─ Record encode (fields → Value) + decode (Value → Result[T, String])
  └─ Nested (recursive encode/decode)
  └─ Option[T] (missing/null → none) + Default (missing/null → default value)
  └─ List[T] (primitive + Named)
  └─ field alias (name as "key": Type)
  └─ Checker pre-registration (: Codec → auto-register encode/decode FnSig)

Phase 2: json module + Runtime crate ✅
  └─ json.stringify / json.parse — Value-based
  └─ json.encode(t) convenience (Codec dispatch in lowerer)
  └─ Runtime crate `almide_rt` (runtime/rust/) — verifiable via cargo test
  └─ Codegen: stdlib runtime calls with `almide_rt_` prefix
  └─ stdlib E2E: int.to_string, string.to_upper, list.len/map/filter/fold ✅

Phase 2.5: stdlib v2 foundation ✅
  └─ `stdlib/core/{int,string,list}/mod.almd` — @extern declarations + pure Almide
  └─ `stdlib/core/{int,string,list}/extern.rs` — Rust native implementation
  └─ `runtime/rust/` — proper Cargo crate, 12 tests passing
  └─ auto_try / in_effect flag separation (test disables auto-?)
  └─ Named ↔ Record compatible (Ty::compatible extension)
  └─ multi-line Named record parsing (TypeName {\n ...})
  └─ triple-quote raw string r\"""...\"""

Phase 3: Remaining features
  ├─ json.decode[T](text) convenience
  ├─ Variant encode/decode (Tagged)
  ├─ value.pick / value.rename_keys utilities
  └─ Gradual migration from legacy TOML stdlib → runtime crate

Phase 4: yaml/toml module
  └─ yaml.stringify / yaml.parse
  └─ toml.stringify / toml.parse (migrate existing toml module to Value-based)

## Stdlib 3-layer classification

| Layer | import | platform | Codec-related modules |
|---|---|---|---|
| **Core** (implicit auto-import) | Not needed | Independent | `value` (Value type + accessors) |
| **Heavy Pure** (explicit import) | `import json` | Independent | `json`, `yaml`, `toml` (parse/stringify) |
| **Effect/Platform** (explicit import) | `import http` | Dependent | `http` (JSON API responses, etc.) |

Value is Core — `value.str("hello")`, `value.field(v, "name")` work without `import`.
json/yaml/toml are Heavy Pure — `json.encode(person)` works with `import json`.

Phase 5: DecodeError + repair + validate
  └─ Structured errors (path + kind)
  └─ json.validate[T]
  └─ json.repair[T]
  └─ json.describe[T] (JSON Schema)
```

### P0 test status (14/14 ✅)
- required missing/null → err
- optional missing/null/present
- default missing/null/present
- type mismatch → err
- nested missing → err
- roundtrip (flat, nested, JSON stringify/parse)
- unknown field ignored
- WeatherResponse full roundtrip (nested records + List + alias)

### Next steps
1. `json.decode[T](text)` convenience — already expanded in lowerer, needs checker type argument resolution
2. Variant encode/decode — Tagged format (`{"Circle": {"radius": 3.0}}`)
3. `value.pick` / `value.rename_keys` — Value transformation utilities
4. Migrate existing `Json` type API to Value — consolidate `json.get_string` etc. into `value.as_string`

Phase 5: value transformation utilities
  └─ value.rename_keys, value.set_path, value.get_path
```

## Design decision rationale

- **Extensible without traits** — Value serves as the concrete contact point. Concrete, not abstract.
- **Convention-based** — `: Codec` declares ".encode and .decode exist"
- **Function composition** — `encode() |> json.stringify` chains via pipes
- **Formats are outside the language** — json, yaml are just modules. Not built into the language
- **Not JSON-first** — Value is a universal data model. JSON is one of its serializations

## Related roadmaps

| Roadmap | Relationship |
|---------|-------------|
| codec-and-json.md | Original design specification (Json → Value rename planned) |
| derive-conventions (done) | Foundation for convention declarations |
| operator-protocol (done) | Auto-derive mechanism |
| web-framework | Codec integration after Phase 1 completion |
| monomorphization (done) | Foundation for generic Codec |
