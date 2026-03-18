# Compiler Architecture Cleanup

**優先度:** Medium — 1.0後でもいいが、やるなら早い方がいい
**状態:** 5項目

## 項目

### 1. clone/deref IR化 (ROI: 高)

**現状:** `clone_vars` / `deref_vars` を annotation に入れ、walker が読んで `.clone()` / `(*x)` を出力。
**理想:** ClonePass / DerefPass が IR を直接書き換え (`IrExprKind::Clone { expr }`, `IrExprKind::Deref { expr }`)。walker は annotation 不要。

- [ ] ClonePass: `Var { id }` → `Clone { expr: Var { id } }` に変換
- [ ] DerefPass: lazy/box変数の `Var { id }` → `Deref { expr: Var { id } }` に変換
- [ ] walker から `ann.clone_vars` / `ann.deref_vars` 参照を削除
- [ ] テンプレート `clone_expr` / `deref_var` は IR node rendering として残す

**見積:** ±200行, 中難度

### 2. lower 2パス分離 (ROI: 中)

**現状:** `lower/` が AST→IR 変換と use-count analysis を同時実行。
**理想:** Pass 1: AST→IR (純粋な構造変換), Pass 2: use-count / codegen分析。

- [ ] lower をAST→IR純粋変換に限定
- [ ] use-count analysis を独立pass (UseCountPass) に分離
- [ ] codegen判断ロジックを lower から排除

**見積:** ±500行, 大難度

### 3. checker InferTy/Ty 統一 (ROI: 中)

**現状:** 型推論中は `InferTy` (unification variable付き), 確定後は `Ty`。毎回変換。
**理想:** unified type で推論と確定を同じ型で表現。

- [ ] `InferTy` と `Ty` の統一型設計
- [ ] 変換コスト削減
- [ ] solutions テーブル管理の簡素化

**見積:** ±1000行, 大難度。型システム根幹なので慎重に。

### 4. walker HashMap allocation 削減 (ROI: 小)

**現状:** テンプレート変数を毎回 `HashMap::new()` で作成。
**理想:** 固定struct or arena allocation。

- [ ] 頻出パターン（2-3変数）を struct で表現
- [ ] or SmallVec/ArrayVec ベースの軽量マップ

**見積:** ±100行, 小難度

### 5. build.rs runtime scanner 堅牢化 (ROI: 低)

**現状:** 正規表現で `.rs` ファイルの関数シグネチャをパース。
**理想:** `syn` crate で正確にパース。

- [ ] syn crate 導入
- [ ] 関数シグネチャ抽出を AST ベースに
- [ ] ビルド時間への影響測定（syn は重い）

**見積:** ±200行, 中難度。ビルド時間増のトレードオフ。

## 優先順位

1. **clone/deref IR化** — アーキテクチャ純度。annotation依存をゼロに近づける
2. **walker allocation削減** — 即効性。小さい変更で計測可能な改善
3. **lower 2パス分離** — 保守性。ただし大きい
4. **checker統一** — 理想だが根幹変更。1.x以降推奨
5. **build.rs堅牢化** — 壊れてからでいい
