<!-- description: Remaining codec features after Phase 0-2 completion -->
<!-- done: 2026-03-15 -->
# Codec Remaining

Phase 0-2 complete. Remaining features.

## Done

### Variant encode (Tagged) ✅
Unit/Tuple/Record variant → `{"CaseName": payload}` 形式で encode
Variant decode は stub (err を返す) — full decode は Future

### json decode パターン ✅
```almide
match json.parse(text) { ok(v) => Person.decode(v), err(e) => err(e) }
```
`json.decode[T](text)` convenience は checker 型引数解決が必要 → Future

### value ユーティリティ ✅
- `value.pick(v, keys)` / `value.omit(v, keys)` — フィールド選択/除外
- `value.merge(a, b)` — Object 結合
- `value.to_camel_case(v)` / `value.to_snake_case(v)` — キー名変換
- `value.rename_keys(v, fn)` — 汎用キー変換 (runtime 内部)

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
- error path: `"coord.lon"` 形式

### json.validate[T] / json.repair[T]
- validate: enumerate issues without decoding
- repair: decode while repairing (Safe/Coercive)

### json.describe[T] — JSON Schema
- JSON Schema Draft 2020-12 compatible

### Other formats
- yaml.stringify / yaml.parse
- toml → migrate to Value-based
