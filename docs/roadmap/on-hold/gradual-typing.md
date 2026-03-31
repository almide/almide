<!-- description: Gradual typing with automatic runtime checks at typed/untyped boundaries -->
# Gradual Typing

typed と untyped コードの境界に自動でランタイムチェックを挿入する。ライブラリは strict、設定/スクリプトは flexible に書ける。

## 参考

- **Nickel**: `typecheck/mod.rs` — Walk モード（動的）と Enforce モード（静的）の 2 モード
  - 型注釈がある関数の内部だけ静的型チェック
  - 境界で自動的にブリッジ契約を挿入
  - 型変数は明示的 `forall` が必要（自動汎化なし）
  - RATIONALE.md: 設定データは untyped（すぐテストされる）、ライブラリは typed（無限の入力パターン）

## 設計の方向性

- 型注釈のない関数は動的チェック（現状の Almide は全て型推論）
- パッケージの public API は型注釈を強制
- 外部パッケージからの呼び出し境界で自動チェック挿入
- `strict` モードフラグで全関数に型注釈を要求（CI 向け）
