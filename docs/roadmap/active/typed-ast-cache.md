<!-- description: Cache type annotations on AST nodes to eliminate re-inference in lowering -->
# Typed AST Cache

型情報を AST ノードに直接キャッシュし、lowering での型引き直しを不要にする。

## 問題

現在の lowering は `expr_types` (HashMap) と `checker.env` を受け取って IR を生成する。型チェック結果が AST に紐づいていないため：

- lowering 時に型を HashMap から引き直す必要がある
- TypeVar が未解決のまま IR に残る ICE の原因になる
- expr_types のキーが位置ベースで、同一位置の式が複数あると壊れる

## 参考

- **Elm**: `AST/Canonical.hs` — `Can.Annotation` を変数に直接キャッシュ。後段は O(1) で型を取得
- **Gleam**: `TypedModule` に全型情報を格納。immutable で再利用可能
- **Roc**: `AbilityMemberData<Phase>` — フェーズ型パラメータで Pending → Resolved を型安全に遷移

## ゴール

```rust
// Before: 型を外部 HashMap から引く
let ty = expr_types.get(&expr.span).unwrap_or(&Ty::Unknown);

// After: AST ノードに型が貼り付いている
let ty = &expr.ty;  // Canonical AST のフィールド
```

- 各 AST ノードに `ty: Ty` フィールドを追加（Canonical 化後に設定）
- lowering は `expr_types` HashMap を参照しない
- TypeVar の ICE が構造的に不可能になる
