<!-- description: Compiler structural cleanup including clone/deref IR conversion -->
<!-- done: 2026-03-18 -->
# Compiler Architecture Cleanup

**Priority:** Medium — can wait until post-1.0, but sooner is better
**Status:** 5 items

## Items

### ✅ 1. clone/deref IR conversion (complete)

- [x] CloneInsertionPass: `Var { id }` → `Clone { Var { id } }` (heap-type variables)
- [x] BoxDerefPass: `Var { id }` → `Deref { Var { id } }` (box'd pattern bindings)
- [x] Remove `ann.clone_vars` / `ann.deref_vars` references from walker
- [x] Remove `clone_vars` / `deref_vars` fields from annotations

### ✅ 4. Walker HashMap allocation reduction (complete)

- [x] Change `fill_template` to `&[(&str, &str)]`
- [x] Add `render_with()` API
- [x] Migrate all 89 HashMap::new() occurrences to render_with

### 2, 3, 5 → Split into separate roadmaps (post-1.0)
