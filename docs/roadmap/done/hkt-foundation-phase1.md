<!-- description: HKT foundation phases 1-4 with type constructors and algebraic laws -->
<!-- done: 2026-03-19 -->
# HKT Foundation — Phase 1-4 + Stream Fusion (All 6 Laws)

**Completion date:** 2026-03-19
**PR:** #49

## Implementation Details

### Phase 1: Ty Helpers + TypeConstructor Foundation
- `TypeConstructorId`, `Kind`, `AlgebraicLaw` type definitions
- `TypeConstructorRegistry` — auto-registration of built-in types + user-defined types
- `Ty::children()`, `map_children()`, `map_children_mut()` — unified type traversal
- `Ty::constructor_id()`, `type_args()`, `constructor_name()`, `is_container()`, `any_child_recursive()`, `all_children_recursive()`
- `Ty::list()`, `option()`, `result()`, `map_of()` smart constructors
- `Ty::inner()`, `inner2()`, `is_list()`, `is_option()`, `is_result()`, `is_map()` accessors
- Added `type_registry` field to IrProgram, auto-registered during lowering
- Simplified match arms in 14 functions (-120 lines)

### Phase 2: Algebraic Law Table
- 6 laws: FunctorComposition, FunctorIdentity, FilterComposition, MapFoldFusion, MapFilterFusion, MonadAssociativity
- List: Functor + Filterable + Foldable
- Option: Functor + Monad
- Result: Functor

### Phase 3: Stream Fusion — IR Rewrite for All 6 Algebraic Laws
- Pipe chain detection (nested calls + let-binding chains)
- **FunctorComposition**: `map(map(x,f),g)` → `map(x, f>>g)` — intermediate List eliminated
- **FunctorIdentity**: `map(x, id)` → `x` — map itself eliminated
- **FilterComposition**: `filter(filter(x,p),q)` → `filter(x, p&&q)`
- **MapFoldFusion**: `fold(map(x,f),init,g)` → `fold(x, init, g∘f)` — map eliminated, single pass
- **MapFilterFusion**: `filter(map(x,f),p)` → `filter_map(x, ...)` — single pass
- **MonadAssociativity**: `flat_map(flat_map(x,f),g)` → `flat_map(x, f>>=g)`
- Lambda composition + predicate composition + variable substitution
- Analysis output with ALMIDE_DEBUG_FUSION=1

### Phase 4: Ty::Applied Unification
- Removed `Ty::List`, `Ty::Option`, `Ty::Result`, `Ty::Map`
- Unified to `Ty::Applied(TypeConstructorId, Vec<Ty>)`
- 342 construction sites → via smart constructors
- ~200 match arms → migrated to Applied pattern
- 23 files changed
- build.rs updated (generated/ also auto-handled)

## Remaining (Phase 5-6 → active/hkt-foundation.md)

- Phase 5: Effect information annotation inside compiler (no syntax changes) → 2.x
- Phase 6: Trait integration → 2.x

## Tests
- 617+ Rust unit tests (56→617)
- 110/110 almide tests
- 0 warnings
