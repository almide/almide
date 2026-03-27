<!-- description: Activate secondary spans showing declaration sites in error messages -->
# Diagnostic Secondary Spans

**完了日:** 2026-03-25

## 実装内容

セカンダリスパン（エラーの原因となった宣言地点の表示）を活性化。

### 変更点
- `#[allow(dead_code)]` を `with_secondary()`, `at()`, `at_span()` から削除
- `TypeEnv` に `fn_decl_spans: HashMap<Sym, (usize, usize)>` を追加して関数宣言位置を追跡
- `check/registration.rs` の `register_fn_sig()` にスパン情報を渡すように拡張
- `check/infer.rs` で `Let`/`Var` 文の宣言位置を `var_decl_locs` に記録

### セカンダリスパンを使うエラー (3箇所)
- **E006** (effect isolation) — effect fn の定義位置を表示
- **E005** (引数型ミスマッチ) — 関数の定義位置を表示
- **E009** (immutable 再代入) — 変数の宣言位置を表示

### JSON 診断
- `to_json()` に `end_col` と `secondary` フィールドを追加

## 残り → [active/diagnostic-end-col.md](../active/diagnostic-end-col.md)
