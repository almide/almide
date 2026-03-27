<!-- description: Higher-kinded type foundation - all phases complete -->
# HKT Foundation — Complete

**全 Phase 完了。** このドキュメントはアーカイブ待ち。

→ 完了記録: [done/hkt-foundation-phase1.md](../done/hkt-foundation-phase1.md)

## 完了内容

- [x] **Phase 1:** Ty ヘルパー + TypeConstructor/Kind/AlgebraicLaw 基盤
- [x] **Phase 2:** 代数法則テーブル (全 6 法則)
- [x] **Phase 3:** Stream Fusion — 全 6 代数法則の IR rewrite
- [x] **Phase 4:** Ty::Applied 統一 — List/Option/Result/Map 削除、23 ファイル移行

## Stream Fusion — 全 6 法則 ✅

| 法則 | 変換 |
|------|------|
| FunctorComposition | `map(map(x,f),g)` → `map(x, f>>g)` |
| FunctorIdentity | `map(x, id)` → `x` |
| FilterComposition | `filter(filter(x,p),q)` → `filter(x, p&&q)` |
| MapFoldFusion | `fold(map(x,f),i,g)` → `fold(x, i, g∘f)` |
| MapFilterFusion | `filter(map(x,f),p)` → `filter_map(x, ...)` |
| MonadAssociativity | `flat_map(flat_map(x,f),g)` → `flat_map(x, f>>=g)` |

## 後続 (on-hold, 2.x)

- [Effect Type Integration](../on-hold/effect-type-integration.md) — FnType に EffectSet を持たせる
- [Trait System](../on-hold/trait-system.md) — HKT 基盤上の Protocol/Interface
