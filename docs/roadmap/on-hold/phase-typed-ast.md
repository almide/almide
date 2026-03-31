<!-- description: Phase type parameters for type-safe compiler pipeline transitions -->
# Phase-Typed AST

コンパイラのフェーズ遷移を型パラメータで表現し、各フェーズで利用可能な情報を型レベルで保証する。

## 参考

- **Roc**: `can/abilities.rs` — `ResolvePhase` トレイトで Pending → Resolved を型安全に遷移
  ```rust
  pub trait ResolvePhase {
      type MemberType;
  }
  pub struct Pending;
  pub struct Resolved;

  pub struct AbilityMemberData<Phase: ResolvePhase> {
      pub parent_ability: Symbol,
      pub typ: Phase::MemberType,
  }
  ```
  Pending フェーズでは型が未確定、Resolved フェーズでは確定済み。間違ったフェーズのデータにアクセスするとコンパイルエラー。

## 設計の方向性

Canonical AST の導入と組み合わせて：

```rust
struct Expr<Phase> {
    kind: ExprKind,
    ty: Phase::Type,      // Parsed: (), Checked: Ty
    module: Phase::Module, // Parsed: String, Canonical: ModuleId
}

struct Parsed;   // ty = (), module = raw string
struct Canon;    // ty = (), module = ModuleId (resolved)
struct Typed;    // ty = Ty, module = ModuleId

impl Phase for Parsed { type Type = (); type Module = String; }
impl Phase for Canon  { type Type = (); type Module = ModuleId; }
impl Phase for Typed  { type Type = Ty; type Module = ModuleId; }
```

- パーサーは `Expr<Parsed>` を返す
- Canonicalize は `Expr<Canon>` を返す
- 型チェッカーは `Expr<Typed>` を返す
- lowering は `Expr<Typed>` だけを受け取る（型なし AST を渡すとコンパイルエラー）
