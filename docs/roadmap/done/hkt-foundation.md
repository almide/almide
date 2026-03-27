<!-- description: Higher-kinded type foundation - all phases complete -->
<!-- done: 2026-03-20 -->
# HKT Foundation — Complete

**All phases complete.** This document is awaiting archival.

→ Completion record: [done/hkt-foundation-phase1.md](../done/hkt-foundation-phase1.md)

## Completed

- [x] **Phase 1:** Ty helpers + TypeConstructor/Kind/AlgebraicLaw foundation
- [x] **Phase 2:** Algebraic law table (all 6 laws)
- [x] **Phase 3:** Stream Fusion — IR rewrite for all 6 algebraic laws
- [x] **Phase 4:** Ty::Applied unification — removed List/Option/Result/Map, migrated 23 files

## Stream Fusion — All 6 Laws ✅

| Law | Transformation |
|-----|----------------|
| FunctorComposition | `map(map(x,f),g)` → `map(x, f>>g)` |
| FunctorIdentity | `map(x, id)` → `x` |
| FilterComposition | `filter(filter(x,p),q)` → `filter(x, p&&q)` |
| MapFoldFusion | `fold(map(x,f),i,g)` → `fold(x, i, g∘f)` |
| MapFilterFusion | `filter(map(x,f),p)` → `filter_map(x, ...)` |
| MonadAssociativity | `flat_map(flat_map(x,f),g)` → `flat_map(x, f>>=g)` |

## Follow-up (on-hold, 2.x)

- [Effect Type Integration](../on-hold/effect-type-integration.md) — Add EffectSet to FnType
- [Trait System](../on-hold/trait-system.md) — Protocol/Interface on the HKT foundation
