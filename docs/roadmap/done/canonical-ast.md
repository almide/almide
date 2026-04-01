<!-- description: Introduce Canonical AST phase to separate name resolution from type checking -->
<!-- done: 2026-04-01 -->
# Canonical AST

Source AST → Canonicalize → Checker(inference only) の 2 段パイプラインに分離。

## 実装内容

### canonicalize モジュール
- `canonicalize::protocols::register_builtin_protocols(env)` — 7 builtin protocol 登録
- `canonicalize::registration::register_decls(env, diags, decls, prefix)` — 全登録関数を free function 化
- `canonicalize::resolve::resolve_type_expr(te, types)` — 型式解決の単一ソース
- `canonicalize::canonicalize_program(program, modules)` — public API

### Checker の責務縮小
- `Checker::from_env(env)` — canonicalize 済み TypeEnv から構築
- `infer_program()` — 推論のみ（import/登録なし）
- `infer_module()` — モジュール body の推論（canonicalize 直接呼び出し）
- `Checker::new()`, `check_program()`, `register_module()` 削除
- `check/registration.rs` (thin delegation) 削除

## 解決した問題
- ✅ Checker が import 解決・モジュール登録・型推論を兼務 → 完全分離
- ✅ `resolve_type_expr` の 3 箇所重複 → canonicalize/resolve.rs に統一
- ✅ Checker なしで TypeEnv を構築可能（LSP/incremental の前提条件）
