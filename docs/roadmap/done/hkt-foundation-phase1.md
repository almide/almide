# HKT Foundation — Phase 1-3

**完了日:** 2026-03-19
**PR:** #49

## 実装内容

### Phase 1: Ty ヘルパー + TypeConstructor 基盤
- `TypeConstructorId`, `Kind`, `AlgebraicLaw` 型定義
- `TypeConstructorRegistry` — built-in 型 + ユーザー定義型の自動登録
- `Ty::children()`, `map_children()`, `map_children_mut()` — 統一的な型トラバーサル
- `Ty::constructor_id()`, `type_args()`, `constructor_name()`, `is_container()`, `any_child_recursive()`, `all_children_recursive()`
- IrProgram に `type_registry` フィールド追加、lowering 時に自動登録
- 14 関数の match arm 簡素化 (-120行)

### Phase 2: 代数法則テーブル
- 6 法則: FunctorComposition, FunctorIdentity, FilterComposition, MapFoldFusion, MapFilterFusion, MonadAssociativity
- List: Functor + Filterable + Foldable
- Option: Functor + Monad
- Result: Functor

### Phase 3: Stream Fusion Nanopass (検出 + 書き換え)
- パイプチェーン検出 (ネスト呼び出し + let-binding チェーン)
- **map+map 融合**: `map(map(x,f),g)` → `map(x, x=>g(f(x)))` — 中間 List 消滅
- **filter+filter 融合**: `filter(filter(x,p),q)` → `filter(x, x=>p(x)&&q(x))`
- **map+fold 融合**: `fold(map(x,f),init,g)` → `fold(x, init, (acc,x)=>g(acc,f(x)))` — map 自体が消滅
- Lambda 合成 (compose_lambdas) + 変数置換 (substitute_var_in_expr)
- ALMIDE_DEBUG_FUSION=1 で分析出力

## 残り (Phase 4-5 → active/hkt-foundation.md に記載)

- Phase 4: Effect 型統合 (Ty レベルに昇格) → 2.x
- Phase 5: Trait 統合 (内部 HKT 上に構築) → 2.x
- Ty enum → Applied 統一リファクタ → 1.x (ヘルパー群が下地として完成)

## テスト
- +28 unit tests (56→84)
- 110/110 almide tests
- 0 warnings
