# WASM Validation: Last 1 Compile Failure

## Status: Rust 153/153, WASM 1 compile failure (type_system_test), 8 skipped

## The Problem

`match doubled { Just(v) => assert_eq(v, 10) }` で:
- scan: `v` の local を i32 で宣言（VarId A の VarTable 型 = TypeVar → i32）
- emit: `v` を i64.load で読む（emit_load_at が subject_type_args から Int を解決）
- local type ≠ load type → validation error

## Root Cause: VarId Mismatch in Lowering

lowering が RecordPattern/Constructor pattern で `define_var("v")` → VarId(A) を生成。
body 内の `v` が `lookup_var("v")` → 同じスコープから VarId(A) を取得 **するはず**。

しかし実際には異なる VarId が使われている。原因:
1. lowering が2回走る（auto-derive 等で同じ body が複数回 lower される）
2. スコープ管理のバグで lookup が異なる VarId を返す
3. checker の ExprId → VarId マッピングが lowering と不一致

## 理想の最終系

**VarTable を codegen の型源として使わない。** 全ての IrExpr が `.ty` に concrete 型を持ち、
codegen は `.ty` のみを信頼する。VarTable は名前表示用のメタデータ。

```
scan_pattern: local 型 = pattern の subject_ty + 構造的位置から導出
emit_pattern: load 型 = pattern の subject_ty + 構造的位置から導出（既に実装済み）
→ 同じ情報源 → 構造的に一致
```

## Fix Plan

### Step 1: VarId 不一致の特定

lowering で `match doubled { Just(v) => ... }` の:
- pattern `Just(v)` の VarId = ?
- body 内 `v` の VarId = ?
- 一致するか、異なるか

debug ログを lower/expressions.rs と lower/statements.rs に入れて特定。

### Step 2: 不一致の原因修正

VarId が異なる場合:
- lowering の scope management を修正して一致させる
- OR: scan_pattern を VarTable 不依存にする（subject_ty + 構造的位置のみ）

### Step 3: codegen hack 層の除去

VarId 不一致が解消されたら:
- name-based VarId fallback in Var emit → 削除
- BinOp VarTable fallback → 削除
- scan_pattern VarTable fallback → 削除
- emit_eq type fallback → 削除

### Step 4: IR Validation

mono 後に:
```rust
for each function:
  for each expr:
    assert!(!has_typevar(expr.ty))
  for each VarId in var_map:
    assert!(!has_typevar(var_table[id].ty))
```
