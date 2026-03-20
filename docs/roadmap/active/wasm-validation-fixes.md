# WASM Validation Error Fixes [ACTIVE]

## Goal

WASM compile failures (validation error) を 0 にする。

## Current: 14 compile failures, 21/73 pass

## Strategy: Golden Point = `solve_constraints` 固定点反復

TypeVar leak の根本原因は `unify_infer` が `solutions` を上書きし、具体型が消失すること。
個別パターンのパッチ（現在の propagation hack）ではなく、
**constraint solving を解が安定するまで繰り返す**ことで全パターンを一律に解決する。

## Implementation Plan

### Step 1: solve_constraints 固定点反復

**File**: `src/check/mod.rs` `solve_constraints()`

```
Before: constraints を 1回走査
After:  解が変化しなくなるまで繰り返す（上限付き）
```

- `unify_infer` の propagation hack を revert（clean な状態に戻す）
- `solve_constraints` に loop + changed detection を追加
- 上限は 10回程度（通常 2-3回で収束するはず）
- **検証**: Rust 153/153 pass、WASM compile failures 減少

### Step 2: Cleanup — codegen heuristic 撤廃

固定点反復で TypeVar が IR から消えたら:

1. `emit_wasm/mod.rs` の `resolve_lambda_param_ty` を削除
   — TypeVar→Int デフォルトが不要になる
2. `emit_wasm/values.rs` の `ty_to_valtype` catch-all を panic に昇格
   — `_ => Some(ValType::I32)` → `_ => panic!("unexpected type in WASM codegen: {:?}", ty)`
3. `check/types.rs` の `default_unresolved_vars` を削除（dead code）
4. IR validation assert 追加: lowering 後に TypeVar("?N") が残っていたら panic

### Step 3: Lambda env load/store 型対応

**File**: `src/codegen/emit_wasm/mod.rs` lambda body compilation

Lambda body が env から capture 変数を読む際、一律 `i32.load` を使っている。
capture の型に応じて `i64.load` (Int) / `f64.load` (Float) を使う。

- LambdaInfo.captures の型情報を参照
- emit_load_at / emit_store_at を使う

**検証**: generics_test, default_fields_test, type_system_test の validation pass

### Step 4: Codec WASM support（or skip判断）

Codec 生成コードの WASM 対応は大工事。Step 1-3 完了後に判断:
- 残りの compile failure が Codec 系のみになっているか確認
- skip する場合は test に `#[wasm_skip]` 的なマーカーを追加

## Expected Outcome

| Step | Rust | WASM compile failures | WASM pass |
|------|------|-----------------------|-----------|
| 現状 | 153/153 | 14 | 21/73 |
| Step 1 | 153/153 | 14→7前後 | 21→21+ |
| Step 2 | 153/153 | — (correctness) | — |
| Step 3 | 153/153 | 7→4前後 | 21→24+ |
| Step 4 | 153/153 | 4→0 | 24→28+ |

## Non-Goal (this roadmap)

- Runtime trap の修正（map iteration, record destructure, float.to_string 等）
- これらは validation fix 後の別作業
