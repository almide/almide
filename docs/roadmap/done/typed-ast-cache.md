<!-- description: Cache type annotations on AST nodes to eliminate re-inference in lowering -->
<!-- done: 2026-04-01 -->
# Typed AST Cache

型情報を AST ノードに直接キャッシュし、lowering での型引き直しを不要にする。

## 実装内容

### Expr 構造の正規化
- `Expr` を flat enum (47 variant × id/span/resolved_type 重複) から `struct { id, span, ty, kind }` + `ExprKind` enum に分離
- dead code だった `resolved_type` / `ResolvedType` フィールドを削除
- 4 つの巨大 match アクセサ (id(), span(), resolved_type(), set_resolved_type()) を削除
- `Expr::new()` コンストラクタ追加
- IrExpr と同じ `#[serde(flatten)]` パターンに統一

### 型の直接埋め込み
- `infer_expr` が `expr.ty = Some(ity)` で AST に直接書く（HashMap 経由なし）
- `solve_constraints` 後、`ast::visit_exprs_mut` で全 Expr の TypeVar を in-place 解決
- `infer_types: HashMap<ExprId, Ty>` 完全除去
- `expr_types: HashMap<ExprId, Ty>` 完全除去
- `lower_program` / `lower_module` から `expr_types` パラメータ除去

### 汎用 AST Visitor
- `ast::visit_exprs_mut()` — 全 Decl/Stmt/Pattern/Expr を網羅的に走査
- resolve walk の手書きによる漏れを構造的に排除

### infer_call の修正
- 元の args/named_args に直接 `infer_expr` を適用（クローンへの二重推論を排除）
- named_args を一時的に positional vec に統合し、check 後に復元

## 解決した問題
- ✅ lowering 時の HashMap lookup + clone が不要に → `expr.ty.clone()`
- ✅ TypeVar が未解決のまま IR に残る ICE が構造的に不可能に
- ✅ ExprId 衝突リスクなし（ExprId をキーにした lookup 自体がない）
- ✅ 47 variant × 3 fields の重複が消滅
