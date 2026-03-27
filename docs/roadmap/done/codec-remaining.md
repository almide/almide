<!-- description: Remaining codec features after Phase 0-2 completion -->
<!-- done: 2026-03-15 -->
# Codec Remaining

Phase 0-2 complete. Remaining features.

## Done

### Variant encode (Tagged) ✅
Unit/Tuple/Record variant → encode in `{"CaseName": payload}` format
Variant decode is a stub (returns err) — full decode is Future

### json decode pattern ✅
```almide
match json.parse(text) { ok(v) => Person.decode(v), err(e) => err(e) }
```
`json.decode[T](text)` convenience requires checker type argument resolution → Future

### value utilities ✅
- `value.pick(v, keys)` / `value.omit(v, keys)` — field selection/exclusion
- `value.merge(a, b)` — Object merging
- `value.to_camel_case(v)` / `value.to_snake_case(v)` — key name conversion
- `value.rename_keys(v, fn)` — generic key conversion (runtime internal)

### Naming strategy ✅
```almide
let camel = value.to_camel_case(person.encode())
let text = json.stringify(camel)  // → {"userName": "Alice"}
```
Achieved via function composition. `Codec(snake_case)` syntax is future sugar.

## Future

Legacy TOML → runtime crate migration is in the scope of [Stdlib Runtime Architecture](stdlib-self-hosted-redesign.md). Codec side runs on TOML.

### Structured DecodeError
- `DecodeError { path: List[String], kind: DecodeErrorKind }`
- error path: `"coord.lon"` format

### json.validate[T] / json.repair[T]
- validate: enumerate issues without decoding
- repair: decode while repairing (Safe/Coercive)

### json.describe[T] — JSON Schema
- JSON Schema Draft 2020-12 compatible

### Other formats
- yaml.stringify / yaml.parse
- toml → migrate to Value-based
