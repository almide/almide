<!-- description: Test case specification for codec based on Serde/Codable/Jackson patterns -->
<!-- done: 2026-03-15 -->
# Codec Test Specification

Test case collection based on insights from Swift Codable / Serde / Kotlin serialization / Jackson.
Created to document specifications before implementation.

## 0. Almide design decisions (settled)

| Decision | Determination | Rationale |
|------|------|------|
| missing vs null | **No distinction** (merge) | Option[T] treats both missing and null as none |
| default on missing | **Yes** | Apply field default when missing |
| default on null | **Yes** | Fill null with default just like missing |
| unknown fields | **Ignore** (default) | strict reject is opt-in via JsonOptions |
| alias | **decode only** | encode uses alias (or field name) |
| coercion | **No** (strict) | opt-in via repair API |
| compile-time type priority | **Yes** | Almide has no runtime polymorphism |
| polymorphism | **Tagged required** | Variant Tagged format |
| canonical encode | **field declaration order** | stable |
| encode/decode asymmetry | **allowed** | Can write only one side with manual codec |

## 1. P0: Tests to pass first (20 cases)

### Presence / Null / Default (12 cases)

```
P0-001: required field missing → err("missing field 'id'")
P0-002: required field null → err("expected Int but got Null")
P0-003: optional field missing → none
P0-004: optional field null → none
P0-005: optional field present → some(value)
P0-006: default field missing → default value applied
P0-007: default field null → default value applied
P0-008: default field present → value overrides default
P0-009: type mismatch: id: "1" → err (no coercion)
P0-010: nested required missing → err with field path
P0-011: array element required missing → err with index
P0-012: all fields present → roundtrip success
```

### Unknown / Alias (4 cases)

```
P0-013: unknown field ignored (default mode)
P0-014: alias decode: type_ as "type" → "type" key accepted
P0-015: alias encode: type_ as "type" → "type" key output
P0-016: unknown field rejected (strict mode, future)
```

### Roundtrip (4 cases)

```
P0-017: flat record encode-decode roundtrip
P0-018: nested record encode-decode roundtrip
P0-019: variant encode-decode roundtrip (Tagged)
P0-020: encode then stringify then parse then decode = original
```

## 2. P1: Tests to pass next (20 cases)

### Enum / Variant (6 cases)

```
P1-001: unit variant encode → {"Red": null}
P1-002: tuple variant encode → {"Circle": [3.0]}
P1-003: record variant encode → {"Rect": {"w": 1.0, "h": 2.0}}
P1-004: unknown variant name → err
P1-005: variant payload mismatch → err
P1-006: variant discriminator missing → err
```

### List (4 cases)

```
P1-007: empty list decode → []
P1-008: list of primitives roundtrip
P1-009: list of Codec types roundtrip (List[Weather])
P1-010: list element type mismatch → err with index
```

### Error Quality (6 cases)

```
P1-011: missing field error includes field name
P1-012: type mismatch error includes expected/got types
P1-013: nested error includes full path ("coord.lon")
P1-014: array element error includes index ("weather[2].id")
P1-015: alias error uses wire key name, not field name
P1-016: multiple errors collected (validate mode, future)
```

### Schema Evolution (4 cases)

```
P1-017: old payload (missing new optional field) → decode success
P1-018: new payload (extra unknown field) → decode success (ignore mode)
P1-019: field renamed with alias → old key still accepted
P1-020: new required field added → old payload fails clearly
```

## 3. P2: Advanced tests (15 cases)

### Coercion (repair mode)

```
P2-001: "42" → 42 in repair(Coercive) mode
P2-002: single value → [value] in repair(Coercive) mode
P2-003: 1.5 → 1 rejected (lossy, even in Coercive)
P2-004: repair reports fixes applied
```

### Number Edge Cases

```
P2-005: i64 overflow → err
P2-006: NaN/Infinity → err (JSON spec violation)
P2-007: 1.0 on Int field → err (strict)
P2-008: 1 on Float field → ok (Int → Float promotion)
```

### Map / Dictionary

```
P2-009: Map[String, Int] roundtrip
P2-010: empty Map roundtrip
P2-011: duplicate key → last wins or err (decide)
```

### Performance Boundaries

```
P2-012: 100-level nesting → no stack overflow
P2-013: 10000-element array → reasonable time
P2-014: many unknown fields in strict mode → all reported
P2-015: large string field → no truncation
```

## 4. Open decision points

Decisions that need to be documented for Almide:

| Issue | Options | Recommended |
|------|--------|------|
| duplicate key | last wins / err | **err** (strict default) |
| null on non-Optional, no default | err / ignore | **err** |
| encode Option none | omit field / emit null | **omit** (decided in codec-and-json.md) |
| encode default value | always / omit if == default | **always** (simplicity) |
| error path format | `coord.lon` / `["coord", "lon"]` | **dot notation** |
| unknown enum case preserve | preserve / err | **err** |
| generic T: Codec constraint | compile-time check | **verified during auto-derive generation** (implemented) |
| flatten / inline | support / don't | **don't** (Canonicity — one representation per structure) |

## 5. Not Applicable in Almide

Swift/Serde concepts that do not apply to Almide:

- **class inheritance encode/decode** — Almide has no classes
- **property wrapper** — Almide has no wrapper pattern
- **existential / any Codable** — Almide has no existentials
- **flatten** — Canonicity violation, not supported
- **CodingKeys enum** — replaced by field alias
- **runtime type dispatch** — Almide is compile-time only
- **lazy decode / streaming** — future consideration, currently eager

## 6. Test format

```almide
test "P0-001: required field missing" {
  let input = r'{"name": "Alice"}'  // id missing
  let result = json.parse(input) |> Person.decode
  match result {
    err(e) => assert(string.contains(e, "missing field 'id'"))
    ok(_) => assert(false)
  }
}
```

## Related

- [Codec Implementation Plan](codec-implementation.md) — Implementation design
- [Codec Protocol & JSON](codec-and-json.md) — Original design specification
