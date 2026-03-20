# WASM Validation Error Fixes [ACTIVE]

## Vision

Almide の型推論を Union-Find ベースに昇華させ、TypeVar leak を構造的に不可能にする。
hack 層を全て除去し、正しさが構造から自明なコンパイラにする。

## Current: Rust 153/153, WASM 14 compile failures (21/73 pass)

---

## Phase 1: Union-Find 型推論（TypeVar leak の構造的消滅）

### 1-1: UnionFind 構造体の導入

**File**: `src/check/types.rs`

`HashMap<TyVarId, Ty>` を `UnionFind` に置換する。

```rust
pub struct UnionFind {
    parent: Vec<u32>,      // parent[i] = i なら root
    rank: Vec<u8>,         // union by rank
    ty: Vec<Option<Ty>>,   // root に具体型が付く（None = 未解決）
}

impl UnionFind {
    fn fresh(&mut self) -> TyVarId          // 新しい TypeVar を割り当て
    fn find(&mut self, id: TyVarId) -> TyVarId  // path compression 付き root 探索
    fn union(&mut self, a: TyVarId, b: TyVarId) // 等価クラスの合併
    fn bind(&mut self, id: TyVarId, ty: Ty)     // root に具体型を束縛
    fn resolve(&self, id: TyVarId) -> Option<Ty> // find → ty[root]
}
```

**なぜ Union-Find か**:
- `union(?1, ?2)` → `find(?2)` → `?1` の具体型。情報は消えない
- 順序非依存（可換・結合的）。constraint 処理順の問題が構造的に消滅
- path compression で O(α(n)) ≈ O(1)

### 1-2: Checker の solutions を UnionFind に置換

**File**: `src/check/mod.rs`

変更箇所（全て mod.rs 内に集中）:

| 現在のパターン | Union-Find |
|---------------|------------|
| `solutions: HashMap<TyVarId, Ty>` | `uf: UnionFind` |
| `solutions.insert(id, ty)` | `uf.bind(id, ty)` or `uf.union(id_a, id_b)` |
| `solutions.get(&id)` | `uf.resolve(id)` |
| `solutions.clone()` (fixpoint) | `uf.snapshot()` |
| `solutions != prev` (fixpoint) | `uf != prev` |
| `fresh_var()` → `Ty::TypeVar(format!("?{}", id))` | `uf.fresh()` → `TyVarId` |

### 1-3: unify_infer の書き直し

```rust
fn unify_infer(&mut self, a: &Ty, b: &Ty) -> bool {
    match (self.uf.as_inference_var(a), self.uf.as_inference_var(b)) {
        (Some(ia), Some(ib)) => { self.uf.union(ia, ib); true }
        (Some(ia), None)     => { self.uf.bind(ia, b.clone()); true }
        (None, Some(ib))     => { self.uf.bind(ib, a.clone()); true }
        (None, None)         => // 構造的 unify (Applied, Fn, Tuple 等)
    }
}
```

- propagation hack: 削除
- fixpoint iteration: 不要になる可能性大（union が順序非依存なので）
  - 残すなら safety net として。不要なら `solve_until_stable` → 単純な1pass に

### 1-4: resolve_vars の書き直し

`HashMap` を引数に取る `resolve_vars(ty, &solutions)` を `UnionFind` ベースに。

```rust
pub fn resolve_ty(ty: &Ty, uf: &UnionFind) -> Ty {
    match ty {
        Ty::TypeVar(name) if is_inference_var(ty).is_some() => {
            let id = parse_id(name);
            uf.resolve(id).unwrap_or_else(|| ty.clone())
        }
        Ty::Applied(id, args) => Ty::Applied(id.clone(), args.iter().map(|a| resolve_ty(a, uf)).collect()),
        Ty::Fn { params, ret } => Ty::Fn {
            params: params.iter().map(|p| resolve_ty(p, uf)).collect(),
            ret: Box::new(resolve_ty(ret, uf)),
        },
        // ...
        _ => ty.clone(),
    }
}
```

### 1-5: hack 層の除去

Union-Find 導入で不要になるもの:

| 削除対象 | ファイル | 理由 |
|----------|---------|------|
| `resolve_lambda_param_ty` | emit_wasm/mod.rs | TypeVar→Int デフォルト不要 |
| `default_unresolved_vars` | check/types.rs | dead code |
| propagation hack in `unify_infer` | check/mod.rs | union が順序非依存 |
| `solve_until_stable` (fixpoint) | check/mod.rs | 1pass で収束 |
| `ty_to_valtype` catch-all `_ => I32` | emit_wasm/values.rs | panic に昇格 |

### 1-6: IR validation

lowering 後に assert:

```rust
fn assert_no_inference_vars(expr: &IrExpr) {
    if let Ty::TypeVar(name) = &expr.ty {
        if name.starts_with('?') {
            panic!("inference variable {} leaked into IR at {:?}", name, expr.span);
        }
    }
    // 再帰的に子を検査
}
```

### 検証

- Rust 153/153 pass
- WASM compile failures: 14 → 7前後（TypeVar leak 系が全消滅）
- grade-report regression なし

---

## Phase 2: Lambda env typed load/store

**Phase 1 完了後に実施。**

Lambda body が env から capture 変数を読む際、型に応じた load 命令を使う。

- `LambdaInfo.captures` の型情報を参照
- `i32.load` 一律 → `emit_load_at(capture_ty, offset)` に変更
- **検証**: default_fields_test, type_system_test, generics_test の validation pass

---

## Phase 3: Codec WASM or skip

Phase 1-2 完了後に判断。残りが Codec 系のみなら skip を検討。

---

## Touchpoint Map (Phase 1 の変更対象)

| ファイル | 変更内容 |
|---------|---------|
| `src/check/types.rs` | UnionFind 構造体追加、resolve_vars 書き直し |
| `src/check/mod.rs` | solutions→uf 置換、unify_infer 書き直し、hack 削除 |
| `src/check/infer.rs` | resolve_vars 呼び出し更新（20箇所） |
| `src/check/calls.rs` | resolve_vars 呼び出し更新 |
| `src/codegen/emit_wasm/mod.rs` | resolve_lambda_param_ty 削除 |
| `src/codegen/emit_wasm/values.rs` | ty_to_valtype catch-all → panic |
| `src/lower/mod.rs` | IR validation 追加 |
