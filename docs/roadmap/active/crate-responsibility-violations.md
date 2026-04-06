<!-- description: Fix 33 CLAUDE.md rule violations found by crate patrol audit -->
# Crate Responsibility Violations

2026-04-06 のパトロール監査で 33 件の CLAUDE.md ルール違反を 6 crate で検出。

## Must Fix (correctness)

### ~~1. almide-ir: fold.rs に 14 ノードの再帰漏れ~~ DONE

### ~~2. almide-codegen: walker に target-specific チェック 5 件~~ DONE

`pass_rust_lowering.rs` に push 最適化 + borrow index lift + List[Fn] Rc wrapping を統合。map_err は template の `when_attr` に移行。`IrExprKind::RcWrap` ノード新設。mut prefix は `mut_param_prefix` template 化。Walker の target チェック 5→0。

## Should Fix (consistency)

### ~~3. almide-optimize: DCE/propagation が top_lets を処理しない~~ DONE

### 4. almide-optimize: mono の型上書き問題

- `fix_body_match_ty()` が `expr.ty` を ret_ty で上書きするが内部式の型は再帰更新しない
- ForIn の VarTable 更新が iterable 走査前に実行される

### ~~5. almide-tools: ABI レイアウト不整合~~ DONE

### 6. almide-base: Sym の Ord が O(n) 文字列比較

`Ord` impl が `resolve(*self).cmp(resolve(*other))` で O(n)。interned ID 比較にするか、安定ソートが必要な理由を文書化するか決定が必要。

## Defer (pragmatic debt)

### 7. almide-base: diagnostic に表示ロジック混在

ANSI カラー、source 行表示、JSON 出力が foundation crate に。分離コストが高く実害小。

### 8. almide-frontend: checker と lowering の pipe desugar 重複

checker の `infer_pipe` が lowering の `lower_pipe` と重複するが、checker が pipe 意味論を理解する必要があり構造的に避けにくい。

### 9. almide-frontend: lowering 内の型推論フォールバック

`LowerCtx::expr_ty()` が TypeMap の Unknown を `infer_stdlib_return_type()` で埋める。本来 checker 側で解決すべき。

### 10. almide-tools: ModuleInterface.version が常に None

バージョントラッキング未実装。外部バインディングジェネレータが実用化されるまで不要。

## Stats

| Crate | Violations | Clean? |
|-------|-----------|--------|
| almide-base | 5 | |
| almide-types | 0 | Yes |
| almide-syntax | 0 | Yes |
| almide-lang | N/A | Facade |
| almide-frontend | 2 | |
| almide-ir | 14 | |
| almide-optimize | 4 | |
| almide-codegen | 5 | |
| almide-tools | 3 | |
| **Total** | **33** | |
