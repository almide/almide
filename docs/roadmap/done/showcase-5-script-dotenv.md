<!-- description: Showcase: dotenv file loader and missing-key checker -->
# Showcase 5: dotenv Loader (Script)

**領域:** Script / 設定管理
**目的:** .env ファイル読み込み + 環境変数チェック。option + guard の実用例。

## 仕様

```
almide run showcase/dotenv-check.almd -- .env .env.example
```

- `.env` ファイルをパースして key=value の Map に
- `.env.example` と比較して不足キーを報告
- コメント (`#`) と空行をスキップ
- `guard` で早期リターン

## 使う機能

- `fs.read_text`, `string.lines`, `string.split`
- `map.set`, `map.contains`, `map.keys`
- `option.unwrap_or`, `option.is_none`
- `guard ... else`
- `list.filter`, `list.each`
- `string.trim`, `string.starts_with`

## 成功基準

- [ ] Tier 1 (Rust) で動作
- [ ] Tier 2 (TS/Deno) で動作
- [ ] 40行以内
- [ ] README に使い方記載
