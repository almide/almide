<!-- description: Fix 33 CLAUDE.md rule violations found by crate patrol audit -->
# Crate Responsibility Violations

2026-04-06 のパトロール監査で 33 件の CLAUDE.md ルール違反を 6 crate で検出。

## Must Fix (correctness)

### ~~1. almide-ir: fold.rs に 14 ノードの再帰漏れ~~ DONE

### ~~2. almide-codegen: walker に target-specific チェック 5 件~~ DONE

`pass_rust_lowering.rs` に push 最適化 + borrow index lift + List[Fn] Rc wrapping を統合。map_err は template の `when_attr` に移行。`IrExprKind::RcWrap` ノード新設。mut prefix は `mut_param_prefix` template 化。Walker の target チェック 5→0。

## Should Fix (consistency)

### ~~3. almide-optimize: DCE/propagation が top_lets を処理しない~~ DONE

### ~~4. almide-optimize: mono の型上書き問題~~ DONE

- `fix_body_match_ty()` に If 分岐の再帰を追加
- ForIn VarTable 順序は false positive（fixed substitution map で順序不問）

### ~~5. almide-tools: ABI レイアウト不整合~~ DONE

### ~~6. almide-base: Sym の Ord が O(n) 文字列比較~~ Won't fix

String 比較は intentional。record field の出力順序が Sym Ord に依存しており、interned ID に変更すると生成コードが非決定的になりコンパイル失敗する。Ord impl にドキュメントコメントを追加済み。

### ~~6. almide-base: Sym の Ord が O(n) 文字列比較~~ Won't fix

String 比較は intentional。record field の出力順序が Sym Ord に依存しており、interned ID に変更すると生成コードが非決定的になりコンパイル失敗する。Ord impl にドキュメントコメントを追加済み。

## Accepted (pragmatic debt — documented, won't fix)

### 7. almide-base: diagnostic に表示ロジック混在

ANSI カラー、source 行表示、JSON 出力が foundation crate に。分離するとクレート数が増え依存管理が複雑化。実害なし。

### 8. almide-frontend: checker と lowering の pipe desugar 重複

checker の `infer_pipe` が lowering の `lower_pipe` と重複。checker が pipe の意味論（引数挿入、postfix 演算子解除）を理解しないと正しい型推論ができないため構造的に不可避。

### ~~9. almide-frontend: lowering 内の型推論フォールバック~~ Accepted

chained UFCS call (e.g. `items |> list.map(f) |> list.join(",")`) で checker の constraint propagation が中間型を Unknown にする構造的問題。lowering の `infer_stdlib_return_type()` フォールバックは全 stdlib module をカバーする安定ワークアラウンド。checker 修正は regression リスク高。コメントに architectural compromise として記載済み。

### ~~10. almide-tools: ModuleInterface.version が常に None~~ DONE

`extract_with_version()` 追加。almide.toml の package.version を反映。

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
