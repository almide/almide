# Codec Remaining [ACTIVE]

Phase 0-2 完了。残りの機能。

## Now

### Variant encode/decode (Tagged)
```almide
type Shape: Codec = Circle(radius: Float) | Rect(w: Float, h: Float)
// Circle(3.0) → {"Circle": {"radius": 3.0}}
```
- auto-derive で Variant の match → encode/decode 生成
- Tagged 形式 (externally tagged) がデフォルト

### json.decode[T](text) convenience
```almide
let p = json.decode[Person](text)?
// → json.parse(text)? |> Person.decode
```
- checker: 型引数から戻り値型を推論
- lowerer: 展開 (既に lowerer 側の実装はある、checker 型引数解決が残り)

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
