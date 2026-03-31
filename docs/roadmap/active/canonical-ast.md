<!-- description: Introduce Canonical AST phase to separate name resolution from type checking -->
# Canonical AST

Source AST → Canonical AST → Type Check の 2 段階に分離する。

## 問題

現在 Almide は AST のまま型チェックに入るため、import 解決とチェッカーが絡み合っている。ImportTable リファクタリングが必要になった根本原因。

- チェッカーが import 解決、モジュール登録、エイリアス管理を兼務
- lowering 時にモジュールエイリアスが消失するバグが発生（`import self as lander` 等）
- サブモジュールの暗黙エイリアス（Go 方式）の導入で複雑度が爆発

## 参考

- **Elm**: `AST/Source.hs` → `Canonicalize/Module.hs` → `AST/Canonical.hs`
  - Canonical AST で変数が `VarLocal` / `VarTopLevel` / `VarKernel` / `VarForeign` にタグ付け
  - 全名前が完全修飾済み。型チェッカーは名前解決を一切しない
- **Roc**: `can/module.rs` — `exposed_imports`, `exposed_symbols`, `referenced_values` を Canonical 化で確定

## ゴール

```
Source AST (parser 出力)
    │
    ▼
Canonicalize (名前解決 + import 解決 + UFCS 解決)
    │
    ▼
Canonical AST (全名前が module.func 形式に確定)
    │
    ▼
Type Checker (純粋に型だけを扱う)
    │
    ▼
Typed Canonical AST → Lowering → IR
```

- チェッカーから import/module 解決を完全に分離
- Canonical AST の各ノードにモジュール情報を持たせる
- lowering はエイリアス解決を一切しない（Canonical で完了済み）
