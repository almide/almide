# Showcase 1: almide-grep (CLI Tool)

**領域:** CLI tool
**目的:** ファイル検索ツール。fan concurrency + effect fn + regex の実用例。

## 仕様

```
almide run showcase/almide-grep.almd -- "pattern" path/
```

- 引数: 検索パターン (regex) + 対象ディレクトリ
- 再帰的にファイルを走査
- マッチした行を `ファイル名:行番号: 内容` で出力
- `fan.map` で並列ファイル読み込み

## 使う機能

- `effect fn` (fs, io)
- `fan.map` (並列ファイル処理)
- `regex.find_all`
- `guard` (フィルタリング)
- `list.flat_map`, `string.lines`, `string.contains`
- `env.args` (CLI引数)

## 成功基準

- [x] Tier 1 (Rust) で動作
- [x] Tier 2 (TS/Deno) で動作
- [ ] 50行以内
- [ ] README に使い方記載
