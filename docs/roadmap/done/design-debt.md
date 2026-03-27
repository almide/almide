<!-- description: Design debt including partial TOML dispatch and anonymous records -->
<!-- done: 2026-03-18 -->
# Design Debt

## 1. Partial gen_generated_call connection

**Status:** TOML template dispatch only connected for list/string/map/int/float/math/result/option. Remaining modules (fs, http, json, etc.) call `almide_rt_*` directly
**Problem:** When adding new modules, TOML `&`/`&*`/`.to_vec()` conventions and runtime function signature mismatches reoccur
**Fix approach:** Extend `lower_stdlib_call`'s `use_template` to all modules. Align each module's runtime signatures with TOML conventions
**Estimate:** 2 days (check and fix runtime signatures per module)

## 2. Anonymous record design

**Status:** `AlmdRec0<T0, T1>` — assigns generics by sorted field name order. Fixed this time but the fundamental design is fragile
**Problem:** Anonymous records with the same field count but different names don't share the same `AlmdRec` name (correct), but the generics parameter correspondence is implicit
**Fix approach:** Make anonymous records concrete types like `struct AlmdRec_age_name { age: i64, name: String }` (no generics). Each field combination becomes one concrete struct
**Estimate:** 1 day

## 3. Deeper borrow analysis

**Status:** Connected but with limited effect. Only function parameter borrow decisions
**Problem:** Excessive cloning of variables in loops, field access, and nested calls
**Fix approach:**
- Phase 1: inter-procedural escape analysis (use callee borrow info) — fixpoint loop code already exists
- Phase 2: loop-aware clone (clone once outside loop, reference inside loop)
- Phase 3: field-level borrow (no clone needed if `obj.field` is non-heap)
**Estimate:** Phase 1: 2 days, Phase 2: 3 days, Phase 3: 2 days
