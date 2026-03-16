# Stability Contract [ACTIVE — 1.0 Phase II]

> Go 1 compatibility promise: "every Go program that compiles today compiles forever."
> Rust editions: syntax evolution without breaking existing code.
> Python 2→3: silent semantic changes nearly killed the language.

## 概要

1.0 リリース前に、安定性の契約を定義・実装する。

## edition フィールド

- [ ] `almide.toml` に `edition = "2026"` フィールド追加
- [ ] コンパイラが edition を読み取り、将来の構文変更を edition でゲート
- [ ] デフォルト edition: 省略時は最新

## 破壊的変更ポリシー

- [ ] ポリシー文書作成:
  - post-1.0 の breaking change は compile error + migration hint のみ
  - silent な挙動変更は絶対禁止
  - API 削除は 2 minor version の deprecation warning 後
- [ ] コア型 API 監査: String, List, Map, Result, Option のシグネチャを凍結

## Rejected Patterns リスト

- [ ] 明示的に採用しない機能のリスト作成 (Ruby の教訓)
  - `while` キーワード (do { guard } で代替)
  - `??` 演算子 (unwrap_or で代替)
  - operator overloading
  - mutable by default
  - null / nil
  - implicit type conversion
  - multiple ways to do the same thing

## hidden operations 文書化 (Zig の教訓)

- [ ] clone 自動挿入の条件と挙動
- [ ] auto-`?` 挿入の条件
- [ ] runtime embedding の仕組み
