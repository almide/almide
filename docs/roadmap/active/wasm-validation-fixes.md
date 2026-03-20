# Type System Architecture [ACTIVE]

## Vision

Almide の型システムを Kind 体系に基づいた一貫した世界観で構築する。
全ての判断が構造から自明になるコンパイラ。

```
Kind = KType | KArrow Kind Kind
```

この一行が全てを統治する。Type, Type -> Type, (Type -> Type) -> Type, ...
特殊ケースの積み重ねではなく、単一の再帰構造から全てが導かれる。

## Architecture Layers

```
Layer 4: Kind polymorphism     forall k. k -> Type
Layer 3: Higher-order kinds    (Type -> Type) -> Type   ← Fix, MonadTrans
Layer 2: HKT                   Type -> Type             ← List, Option, Functor
Layer 1: Simple types          Type                     ← Int, String, Bool
Layer 0: Values                42, "hello", true
```

Almide は Layer 2 (HKT) を実装中。内部表現は Layer 3+ を自然に許容する設計にする。
Layer 4 (kind polymorphism) は表面言語で必要になるまで露出しないが、内部では禁止しない。

## Current Work: Phase 1 — Union-Find Type Inference

### Status

- Rust: 152/153 pass (grade-report regression — match pattern inference gap)
- WASM: 14 compile failures

### 1-1: UnionFind 構造体 ✅

`check/types.rs` に `UnionFind` を導入。
`HashMap<TyVarId, Ty>` の代入モデルから等価クラスモデルへ。

- `union(a, b)` — 情報を失わない合併。順序非依存
- `bind(id, ty)` — 具体型の束縛。既存束縛があれば caller に返して構造的 unify
- `find(id)` — 代表元の探索
- `occurs(var, ty)` — 無限型防止

### 1-2: Checker 統合 ✅

- `solutions: HashMap` → `uf: UnionFind`
- `resolve_vars` → `resolve_ty`
- `fresh_var` → `uf.fresh()`
- propagation hack / fixpoint iteration → 削除（Union-Find が順序非依存なので不要）

### 1-3: grade-report regression [NEXT]

**問題**: `list.fold(students, ok([]), (acc, student) => { match acc { ok(lines) => ... } })`
lambda body 推論時に `acc` の型が不明（`?68`）。`match acc { ok(x) => ... }` が
`acc = Result[T, E]` の制約を生成しない。

**根本原因**: match pattern `ok(x)` / `err(e)` が subject の型を Result に制約していない。
Union-Find 以前から潜在していたバグが、より厳密な型チェックで顕在化。

**Fix**: `infer.rs` の match 推論で `ok`/`err` パターンを検出したら
subject を `Result[?fresh, ?fresh]` に制約する。

### 1-4: Hack 層の除去

Union-Find + grade-report fix 後に:

| 削除対象 | 理由 |
|---------|------|
| `resolve_lambda_param_ty` (emit_wasm/mod.rs) | TypeVar→Int デフォルト不要 |
| `default_unresolved_vars` (check/types.rs) | dead code |
| `ty_to_valtype` catch-all (emit_wasm/values.rs) | panic に昇格 |
| 旧 `resolve_vars` / `resolve_inner` (check/types.rs) | `resolve_ty` に統合済み |

### 1-5: IR validation

```rust
assert!(!expr.ty.contains_inference_var(), "?N leaked into IR at {:?}", expr.span);
```

## Phase 2: Lambda Env Typed Load/Store

Lambda body が env capture を読み書きする際の型対応。
`LambdaInfo.captures` の型情報を codegen で使う。

## Phase 3: Kind-Aware Type Representation

### 現状の問題

`Ty` enum に Kind 情報がない。全ての型が同じ enum に混在し、
「List は Type -> Type」「Int は Type」という区別がコード上で暗黙。

### 目標

```rust
enum Kind {
    KType,
    KArrow(Box<Kind>, Box<Kind>),
}
```

全ての型定義に Kind を付与。型チェック時に Kind の整合性を検証。

### 具体的なユースケース

1. **Fix / recursion schemes**: `Fix : (Type -> Type) -> Type`
   - AST/IR の generic traversal, fold/unfold の共通化
2. **HKD (Higher-Kinded Data)**: `Person f = { name: f String, age: f Int }`
   - Validation, partial update, DB row を同一定義から派生
3. **Transformer / Effect**: `StateT : Type -> (Type -> Type) -> Type -> Type`
   - Effect system の自然な表現
4. **コンパイラ IR の phase parameterization**:
   - ExprF ann rec — annotation と recursion carrier の分離

### 設計方針

- `Kind = KType | KArrow Kind Kind` — 上限を設けない
- Kind inference を持つ（ユーザーが Kind を書く必要がない）
- 表面言語では Layer 3+ を必要になるまで露出しない
- 内部では禁止しない（一貫した世界観）

## Phase 4: Codec WASM or Skip

Phase 1-2 完了後に判断。

## Invariants (全 Phase 共通)

- Rust 153/153 は絶対に壊さない
- 特殊ケースの追加ではなく、構造の昇華で問題を解決する
- 「動く」で満足しない。構造から正しさが自明になるまで磨く
