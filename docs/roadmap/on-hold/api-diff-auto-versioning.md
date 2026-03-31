<!-- description: Automatic semver bump detection via public API diffing -->
# API Diff & Automatic Versioning

パッケージの公開 API を比較して MAJOR/MINOR/PATCH を自動判定する。

## 参考

- **Elm**: `Deps/Diff.hs` — 型シグネチャの構造比較
  - 型変数のリネーミングを考慮した等価判定
  - モジュール追加 = MINOR、型変更 = MAJOR、関数追加 = MINOR
  - `elm diff` コマンドで CLI から実行可能

## ゴール

```bash
almide diff v0.1.0..v0.2.0
# Added: yaml.parse_all (MINOR)
# Changed: yaml.stringify return type String → Result[String, String] (MAJOR)
# Suggested version: v1.0.0
```
