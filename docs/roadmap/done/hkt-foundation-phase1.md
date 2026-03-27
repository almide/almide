<!-- description: HKT foundation phases 1-4 with type constructors and algebraic laws -->
# HKT Foundation — Phase 1-4 + Stream Fusion (All 6 Laws)

**完了日:** 2026-03-19
**PR:** #49

## 実装内容

### Phase 1: Ty ヘルパー + TypeConstructor 基盤
- `TypeConstructorId`, `Kind`, `AlgebraicLaw` 型定義
- `TypeConstructorRegistry` — built-in 型 + ユーザー定義型の自動登録
- `Ty::children()`, `map_children()`, `map_children_mut()` — 統一的な型トラバーサル
- `Ty::constructor_id()`, `type_args()`, `constructor_name()`, `is_container()`, `any_child_recursive()`, `all_children_recursive()`
- `Ty::list()`, `option()`, `result()`, `map_of()` スマートコンストラクタ
- `Ty::inner()`, `inner2()`, `is_list()`, `is_option()`, `is_result()`, `is_map()` アクセサ
- IrProgram に `type_registry` フィールド追加、lowering 時に自動登録
- 14 関数の match arm 簡素化 (-120行)

### Phase 2: 代数法則テーブル
- 6 法則: FunctorComposition, FunctorIdentity, FilterComposition, MapFoldFusion, MapFilterFusion, MonadAssociativity
- List: Functor + Filterable + Foldable
- Option: Functor + Monad
- Result: Functor

### Phase 3: Stream Fusion — 全 6 代数法則の IR rewrite
- パイプチェーン検出 (ネスト呼び出し + let-binding チェーン)
- **FunctorComposition**: `map(map(x,f),g)` → `map(x, f>>g)` — 中間 List 消滅
- **FunctorIdentity**: `map(x, id)` → `x` — map 自体が消滅
- **FilterComposition**: `filter(filter(x,p),q)` → `filter(x, p&&q)`
- **MapFoldFusion**: `fold(map(x,f),init,g)` → `fold(x, init, g∘f)` — map 消滅、単一パス
- **MapFilterFusion**: `filter(map(x,f),p)` → `filter_map(x, ...)` — 単一パス
- **MonadAssociativity**: `flat_map(flat_map(x,f),g)` → `flat_map(x, f>>=g)`
- Lambda 合成 + 述語合成 + 変数置換
- ALMIDE_DEBUG_FUSION=1 で分析出力

### Phase 4: Ty::Applied 統一
- `Ty::List`, `Ty::Option`, `Ty::Result`, `Ty::Map` 削除
- `Ty::Applied(TypeConstructorId, Vec<Ty>)` に統一
- 342 構築箇所 → スマートコンストラクタ経由
- ~200 match arms → Applied パターンに移行
- 23 ファイル変更
- build.rs 更新 (generated/ も自動対応)

## 残り (Phase 5-6 → active/hkt-foundation.md)

- Phase 5: コンパイラ内部の Effect 情報付与 (構文変更なし) → 2.x
- Phase 6: Trait 統合 → 2.x

## テスト
- 617+ Rust unit tests (56→617)
- 110/110 almide tests
- 0 warnings
