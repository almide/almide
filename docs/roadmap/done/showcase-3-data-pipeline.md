# Showcase 3: CSV→JSON Pipeline (Data Processing)

**領域:** Data processing
**目的:** CSV読み込み → 変換 → JSON出力。list高階関数 + pipe の実用例。

## 仕様

```
almide run showcase/csv-to-json.almd -- input.csv > output.json
```

- CSV パース (ヘッダー行 + データ行)
- フィルタ/集計/変換をpipeチェーンで
- JSON出力

## 使う機能

- `string.split`, `string.trim`, `string.lines`
- `list.map`, `list.filter`, `list.fold`, `list.group_by`
- `|>` pipe chain
- `int.parse`, `float.parse`
- `json.stringify_pretty`
- `map.from_list`

## 成功基準

- [ ] Tier 1 (Rust) で動作
- [ ] Tier 2 (TS/Deno) で動作
- [ ] 40行以内
- [ ] README に使い方記載
