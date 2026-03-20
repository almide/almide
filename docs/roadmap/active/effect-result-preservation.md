# Effect Result Preservation [ACTIVE]

**優先度:** 1.0 前 — CLI アプリ開発のブロッカー
**発見:** 2026-03-20 MSR ツール開発中

## 問題

`effect fn` 内で effect fn を呼ぶと、Result が自動 unwrap されてエラーハンドリングができない。

```almide
effect fn main() -> Result[Unit, String] = do {
  // ❌ 現在: auto-unwrap で String になる。エラー分岐できない
  let output = process.exec("cmd", [])  // → String (unwrap済み)

  // ✅ やりたいこと: Result を保持してエラーハンドリング
  let res: Result[String, String] = process.exec("cmd", [])
  if result.is_ok(res) then ... else ...
}
```

## 原因

`ResultPropagationPass` (nanopass) が effect fn 内の全ての Result-returning call を `Try { expr }` でラップする。型注釈 `Result[T, E]` があっても区別できない。

- チェッカー: 型注釈があれば auto-unwrap しない ✅
- nanopass: Bind の ty が Result かどうかに関わらず Try を挿入 ❌
- lowerer: Bind ty は expr_types から来る（常に Result） → 型注釈の有無が区別できない

## 設計方針

**型注釈が「私は Result を保持したい」という意図の表明。** ユーザーが `let res: Result[...] = ...` と書いたら auto-unwrap しない。

## 実装案

IR の `IrStmtKind::Bind` に `preserve_result: bool` を追加:

1. **チェッカー → AST**: `Stmt::Let` に型注釈ありフラグを保持（既に `ty: Option<TypeExpr>` がある）
2. **lowerer**: 型注釈ありの let で、注釈型が Result の場合 → `preserve_result = true`
3. **nanopass `ResultPropagationPass`**: `preserve_result = true` の Bind → Try を挿入しない
4. **codegen walker**: `preserve_result = true` の Bind → auto_unwrap を一時無効化

## 影響範囲

- `src/ir/mod.rs` — Bind に `preserve_result` フィールド追加
- `src/lower/statements.rs` — 型注釈から `preserve_result` を設定
- `src/codegen/pass_result_propagation.rs` — `preserve_result` チェック
- `src/codegen/walker.rs` — Result Bind の auto_unwrap スキップ
- 全 nanopass の Bind パターンマッチ更新（フィールド追加のため）

## テスト

```almide
effect fn main() -> Result[Unit, String] = do {
  // auto-unwrap (従来通り)
  let output = process.exec("echo", ["hi"])  // → String
  println(output)

  // Result 保持 (新機能)
  let res: Result[String, String] = process.exec("bad-cmd", [])
  if result.is_ok(res) then println("ok")
  else println("failed gracefully")

  ok(())
}
```
