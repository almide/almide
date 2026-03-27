<!-- description: Remaining codec features after Phase 0-2 completion -->
# Codec Remaining

Phase 0-2 完了。残りの機能。

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
関数合成で実現。`Codec(snake_case)` 構文は Future の sugar。

## Future

旧 TOML → runtime crate 移行は [Stdlib Runtime Architecture](stdlib-self-hosted-redesign.md) のスコープ。Codec 側は TOML で動作中。

### DecodeError 構造化
- `DecodeError { path: List[String], kind: DecodeErrorKind }`
- error path: `"coord.lon"` 形式

### json.validate[T] / json.repair[T]
- validate: decode せずに問題を列挙
- repair: 修復しながら decode (Safe/Coercive)

### json.describe[T] — JSON Schema
- JSON Schema Draft 2020-12 互換

### 他フォーマット
- yaml.stringify / yaml.parse
- toml → Value ベースに移行
