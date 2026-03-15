# Codec Remaining [ACTIVE]

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

## Next

### value ユーティリティ
- `value.pick(v, ["name", "age"])` — フィールド抽出
- `value.rename_keys(v, fn)` — キー名変換
- `value.merge(a, b)` — Object 結合

### Codec(naming_strategy)
```almide
type ApiRes: Codec(snake_case) = { userId: String }
// encode → {"user_id": "..."}
```
- type 宣言の Codec 引数パース
- encode 時に naming strategy 適用

### 旧 TOML → runtime crate 移行
- `stdlib/defs/*.toml` の関数を `runtime/rust/src/*.rs` に段階的に移動
- 移動完了した TOML を削除
- 最終的に build.rs の stdlib 生成ロジックを削除

## Future

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
