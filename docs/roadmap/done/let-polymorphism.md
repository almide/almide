<!-- done: 2026-03-16 -->
# Let-Polymorphism (Algorithm W)

## 現状

```almide
let f = (x) => x        // f : fn(?0) -> ?0 (monomorphic)
let a = f(1)            // ?0 = Int に固定
let b = f("hello")      // エラー: ?0 は既に Int
```

Almide は型スキーム (∀α. τ) を持たない。let バインディングは monomorphic。

## 目標

```almide
let f = (x) => x        // f : ∀T. T -> T (polymorphic)
let a = f(1)            // T=Int に instantiate → a: Int
let b = f("hello")      // T=String に instantiate → b: String
```

## 今日の修正で確認済みの基盤

| 基盤 | 状態 |
|------|------|
| `InferTy::Var` + `TyVarId` | ✅ 動作中 |
| `unify_infer` (構造的 unification) | ✅ Named 引数含む |
| `resolve_inference_vars` | ✅ Named 再帰処理あり |
| `from_ty` の TypeVar → Var 変換 | ✅ |
| eager constraint solving | ✅ |

## 追加する3つの機能

### 1. TypeScheme 型

```rust
pub enum TypeScheme {
    Mono(Ty),
    Poly { bound_vars: Vec<TyVarId>, body: Ty },
}
```

### 2. Generalize (let バインディング後)

```rust
fn generalize(ty: &Ty, env_vars: &HashSet<TyVarId>) -> TypeScheme {
    let free = find_free_vars(ty);
    let bound: Vec<TyVarId> = free.difference(&env_vars).collect();
    if bound.is_empty() { TypeScheme::Mono(ty) }
    else { TypeScheme::Poly { bound_vars: bound, body: ty } }
}
```

`check_stmt` の `Let` ケースで、値を推論した後に generalize を呼ぶ。

### 3. Instantiate (変数参照時)

```rust
fn instantiate(scheme: &TypeScheme) -> InferTy {
    match scheme {
        TypeScheme::Mono(ty) => InferTy::from_ty(ty),
        TypeScheme::Poly { bound_vars, body } => {
            let fresh: HashMap<TyVarId, InferTy> = bound_vars.iter()
                .map(|v| (*v, self.fresh_var()))
                .collect();
            substitute_infer(body, &fresh)
        }
    }
}
```

`infer_expr` の `Ident` ケースで、変数を lookup した後に instantiate を呼ぶ。

## 変更ファイル

- `src/check/types.rs` — `TypeScheme` enum 追加
- `src/check/mod.rs` — `generalize`, `instantiate`, `find_free_vars`
- `src/check/infer.rs` — `Ident` で instantiate, `Let` で generalize
- `src/types/env.rs` — `TypeEnv` の scopes を `HashMap<String, TypeScheme>` に

## 影響範囲

今の明示的 generics (`fn id[T](x: T) -> T`) はそのまま動く。
let-polymorphism は**追加機能**。既存テストは壊れない。

## 見積り

3-5日。TAPL Ch.22 の Algorithm W をそのまま実装。
