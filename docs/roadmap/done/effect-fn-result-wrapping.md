<!-- description: Fix effect fn Rust codegen to wrap return type in Result -->
# Effect fn Result Wrapping [DONE]

**優先度:** 1.0 blocker
**前提:** ResultPropagationPass 既存

## 問題

`effect fn` の Rust codegen が壊れている。`?` 演算子を挿入するが、戻り値型を `Result` に変換していない。

```
effect fn fetch() -> String = do { http.get(url) }

現状:    fn fetch() -> String { (http_get(url))? }          ← rustc エラー
あるべき: fn fetch() -> Result<String, String> { Ok((http_get(url))?) }  ← 正しい
```

CLAUDE.md の設計意図: `effect fn` → `Result<T, String>`, auto `?` propagation

## 影響パターン

| パターン | 現状 | 修正後 |
|---|---|---|
| `effect fn foo() -> String` | `-> String` + `?` ❌ | `-> Result<String, String>` + `Ok()` |
| `effect fn foo() -> Result[T, E]` | `-> Result<T, E>` ✅ | 変更不要 |
| `effect fn main() -> Unit` | `-> ()` + `?` ❌ | `-> Result<(), String>` (Termination trait) |
| `test "..." { ... }` | ✅ | 変更不要 (is_test 別処理) |
| 純粋 fn | ✅ | 変更不要 |

## 設計

**変更箇所:** `src/codegen/pass_result_propagation.rs` の1箇所に集約

条件: `is_effect && !is_test && !ret_ty.is_result()`

1. `func.ret_ty` を `Result<ret_ty, String>` に変換
2. body 末尾を `Ok(expr)` でラップ
3. 既存ロジック (`fn_returns_result=true`) がそのまま動く

### TS ターゲット

TS は `async/await` + `try/catch` で処理。この変換は Rust/WASM のみ。TS にも同じパスが走るならターゲット判定を入れる。

### main() の扱い

特別扱い不要。Rust の `main()` は `Result<(), E: Debug>` を返せる (`Termination` trait)。

### 呼び出し側への波及

- **effect fn → effect fn:** 自動 `?` で連鎖的に正しくなる
- **test → effect fn:** test は `is_test=true` で別処理、Result を match で受ける
- **純粋 fn → effect fn:** チェッカーが E006 で弾く

## 実装進捗

| Phase | 内容 | 状態 |
|---|---|---|
| Phase 1 | ResultPropagationPass で ret_ty 変換 + Ok ラップ (Rust only) | ✅ 完了 |
| Phase 2 | main() が Result<(), String> で動作検証 | ✅ 完了 |
| Phase 3 | チェッカー auto-unwrap (effect fn body が Result → T に照合) | ✅ 完了 |
| Phase 4 | LICM effect 判定: TypeEnv 由来の effect_fn_names に置換 | ✅ 完了 |
| Phase 5 | テスト内 lifted fn 呼び出し → .unwrap() | ✅ 完了 |
| Phase 6 | CI 全通過 (Rust/TS/WASM) | ✅ 完了 |
