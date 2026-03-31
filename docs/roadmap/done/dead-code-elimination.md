<!-- description: Dependency-graph-based dead code elimination for smaller WASM binaries -->
<!-- done: 2026-03-31 -->
# Dead Code Elimination

エクスポートされた関数から依存グラフを辿り、到達不能な定義を削除する。WASM バイナリサイズの削減に直結。

## 参考

- **Elm**: `Optimize/Names.hs` — `Tracker` モナドで使用されるグローバル・フィールドアクセスを追跡
  - エクスポートから到達可能な定義だけを残す
  - フィールドアクセス頻度でレコードフィールドの最適化判断
  - コンストラクタの最適化: `Enum`（整数化）、`Unbox`（ラッパー除去）
- **Elm**: `Optimize/DecisionTree.hs` — match 式の冗長ブランチ除去

## 現状

Almide の Rust codegen は `#[allow(dead_code)]` で未使用コードを許容。WASM codegen は全関数を出力。
stdlib ランタイム（Rust）は使用モジュールだけ include する仕組みがあるが、関数レベルの除去はない。

## ゴール

- IR レベルで依存グラフを構築（main/test から辿る）
- 到達不能な関数・型定義を IR から除去
- WASM バイナリサイズの削減（現状 playground の Shape renderer: 5KB → 目標 3KB 以下）
- コンストラクタの最適化（引数なしバリアント → 整数、単一フィールド → unbox）
