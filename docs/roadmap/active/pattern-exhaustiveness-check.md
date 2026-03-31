<!-- description: Static exhaustiveness checking for match expressions -->
# Pattern Exhaustiveness Check

match 式でバリアントの漏れをコンパイル時に検出する。

## 参考

- **Elm**: `Nitpick/PatternMatches.hs` — Maranget のアルゴリズムで decision tree を構築
- **Gleam**: `exhaustiveness.rs` (155KB) — Jules Jacobs のアルゴリズム

## 現状

WASM codegen では網羅性チェックなし。到達しない場合は `unreachable` trap。
Rust ターゲットでは Rust コンパイラが検出するため間接的にカバー。

## ゴール

- 型チェッカーで match の網羅性を検証
- 不足しているパターンをエラーメッセージで列挙
- 到達不能パターンを warning で報告
