# WASM Validation: Last 1 Compile Failure

## Status: Rust 153/153, WASM 1 compile failure (type_system_test), 8 skipped (Codec)

## Root Cause (narrowed down)

func 45 (`test "generic variant map"`) で `match doubled { Just(v) => assert_eq(v, 10) }` の
pattern binding が `i32_load offset=4` を出す。`i64_load` であるべき。

- `emit_load_at(Int, 4)` は `i64_load` を正しく出す（ログ確認済み）
- `i32_load(4)` は emit_load_at からではない（ログ確認済み）
- control.rs に直接 `i32_load(4)` を出すコードはない
- scan_pattern は VarTable=Int → I64 で local を正しく宣言（ログ確認済み）

**仮説**: `find_variant_tag_by_ctor("Just", subject_ty)` が None を返し、
Constructor handler の修正コードに入らず、旧 else case（body 直接 emit）に落ちる。
body 内の `v` が name-based fallback で別の i32 local にマップ。

## Next Debug Step

func 45 の Constructor handler entry にログ:
1. `find_variant_tag_by_ctor` の返り値
2. `ctor_name`
3. `subject_ty` の正確な値
4. handler の if/else どちらに入ったか

## Completed (this branch)

- Codec 8件: wasm:skip
- Union-Find resolve_ty: Record/OpenRecord の再帰追加
- mono discover: type_args + ret_ty から binding 推論
- mono rewrite: type_args + ret_ty から binding 推論
- 未使用 generic 関数の削除
- fold acc_local の型選択
- emit_eq の concrete type dispatch
- Constructor pattern: scan + emit で subject_type_args による型解決

Total: 12 → 1 compile failure
