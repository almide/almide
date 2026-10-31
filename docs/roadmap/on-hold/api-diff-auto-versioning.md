<!-- description: Package-facing API diff and version advice beyond the shipped stdlib interface gate -->
# API Diff & Automatic Versioning

## 実装済みの土台

標準ライブラリの公開 API 差分は、既に
[`scripts/check-interface-diff.sh`](../../../scripts/check-interface-diff.sh)
で検査する。生成済みのシグネチャ索引を git ref 間で比較し、
`identical` / `additive` / `breaking` に分類するリリースゲートであり、
明示的に許可されていない breaking change は失敗にする。

[`check-interface-diff-negative.sh`](../../../scripts/check-interface-diff-negative.sh)
は pure / effect 関数の削除・追加を使った負の対照。
この機構は既存だが、一般のパッケージ向け CLI や自動バージョン更新ではない。

## 残る提案

- パッケージの公開 API を利用者向け CLI で比較する。
- 型変数のリネーミングなどを考慮した構造比較の規則を決める。
- 差分から MAJOR/MINOR/PATCH の推奨を示す。シグネチャが同じでも振る舞いは
  変わり得るため、API 差分だけで互換性全体を保証しない。

以下は未実装 CLI の設計例。現在使えるコマンドとして案内しない。

```text
almide diff v0.1.0..v0.2.0
# Added: yaml.parse_all (MINOR)
# Changed: yaml.stringify return type String → Result[String, String] (MAJOR)
# Suggested version: v1.0.0
```

## 設計参考

Elm の `Deps/Diff.hs` / `elm diff`: 型シグネチャの構造比較、型変数名を
考慮した等価判定、モジュール・関数の追加と型変更の分類。
既存の stdlib ゲートとこのパッケージ向け提案は、対象と完成条件を分けて扱う。
