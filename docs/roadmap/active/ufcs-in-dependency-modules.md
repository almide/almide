<!-- description: UFCS method calls fail type resolution in dependency module context -->
# UFCS Resolution in Dependency Modules

## Problem

依存パッケージ内のモジュールで bare UFCS メソッド呼び出し（`xs.map(fn)`, `xs.join(sep)`）が型解決に失敗する。ローカルでは動くが dependency 経由だと動かない。

```almide
// bindgen/src/bindings/javascript.almd
let names = fields.map((f) => get_str(f, "name"))   // ← ローカルOK、dependency NG
let names = fields |> list.map((f) => get_str(f, "name"))  // ← どちらでもOK
```

## Root Cause

`resolve_unresolved_ufcs`（StdlibLoweringPass 内）は `resolve_module_from_ty(&object.ty, &method)` で型ベースのモジュール解決を行う。dependency モジュールの型推論では `self_module_name` のコンテキスト差異等により一部の変数型が `Unknown` になり、UFCS 解決が失敗する。

フォールバック（`resolve_ufcs_module`）も存在するが、`resolve_unresolved_ufcs` は `rewrite_expr` の後に実行されるため、既に `CallTarget::Method` が別の経路で処理済みのケースではフォールバックに到達しない。

## Impact

- `xs.map(fn)` 等の bare メソッド呼び出しが dependency パッケージ内で使えない
- 回避策: `xs |> list.map(fn)` の明示的パイプ構文を使う（推奨イディオム）
- almide-bindgen は回避策を適用済み

## Fix Direction

1. **型推論の改善**: dependency モジュールの `infer_module` 時に、パッケージルートモジュールの関数型情報が正しく伝播されるようにする
2. **UFCS フォールバックの強化**: `resolve_unresolved_ufcs` で `Unknown` 型のオブジェクトに対しても、引数の個数とメソッド名から stdlib モジュールを推定する
3. **パス順序の見直し**: `rewrite_expr` 内の Method→Module 変換と `resolve_unresolved_ufcs` の実行タイミングの整合性を確認

## Files to Investigate

- `crates/almide-codegen/src/pass_stdlib_lowering.rs` — `resolve_unresolved_ufcs`, `rewrite_expr` の Method 処理
- `crates/almide-frontend/src/check/mod.rs` — `infer_module` の型推論コンテキスト
- `crates/almide-frontend/src/import_table.rs` — dependency モジュールの import 解決
