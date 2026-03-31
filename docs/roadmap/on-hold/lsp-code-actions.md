<!-- description: LSP code actions for auto-fix, refactoring, and import management -->
# LSP Code Actions

エディタ上で自動修正・リファクタリングを提供する。

## 参考

- **Gleam**: `code_action.rs` (384KB) — 40+ のコードアクション
  - import 追加/削除
  - unused 変数削除
  - パターン抽出
  - 型注釈追加
- **Nickel**: `analysis.rs` (45KB) — 型とコントラクト情報を使った補完

## ゴール

- import 自動追加（`almide fmt` のエディタ統合版）
- 未使用変数の削除提案
- match パターンの自動補完（全バリアント列挙）
- 関数シグネチャの型注釈追加
