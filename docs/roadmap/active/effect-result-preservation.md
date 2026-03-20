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

## 試行した実装案と結果

### 案1: チェッカーの infer_types を上書き ❌
- auto-unwrap 時に `infer_types[expr_id]` を unwrapped 型に変更
- 問題: nanopass の `is_result_call()` が `expr.ty.is_result()` で判定するため、Try 挿入自体が行われなくなる

### 案2: lowerer で型注釈から Bind ty を設定 ❌
- 型注釈ありなら `resolve_type_expr(annotation)` を Bind ty に使う
- 問題: 型注釈なしでも `ir_val.ty` = `Result`（expr_types の値）なので区別できない
- nanopass で Bind ty が Result かどうかで判定しても、全 Bind ty が Result

### 案3 (確定): Bind に型注釈有無フラグを追加
- `IrStmtKind::Bind` に `#[serde(default)] annotated_result: bool` を追加
- lowerer: `Stmt::Let { ty: Some(te), .. }` で `resolve_type_expr(te).is_result()` なら `true`
- nanopass: `annotated_result = true` なら Try を挿入しない
- 25 ファイルの Bind パターンマッチは `..` でフィールド無視可能（既存パスは影響なし）

## 影響範囲

- `src/ir/mod.rs` — Bind に `annotated_result: bool` 追加
- `src/lower/statements.rs` — 型注釈判定
- `src/codegen/pass_result_propagation.rs` — `annotated_result` チェック
- 他の Bind パターンマッチ — `..` で無視（変更不要）

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
