<!-- description: Achieve 10/10 on every compiler architecture quality metric -->
<!-- done: 2026-04-01 -->
# Compiler Architecture: All 10s

**Goal**: 10/10 on every compiler architecture metric
**Current**: 110/110 — All metrics at 10/10 ✅
**Scope**: Entire compiler infrastructure including WASM codegen

---

## Scorecard

| Area | Start | Current | Target | Status |
|------|-------|---------|--------|--------|
| Pipeline Design | 7 | **10** | 10 | ✅ Canonical AST + Typed AST Cache: Parser→Canonicalize→Checker(inference only)→Lower |
| Parser | 9 | **10** | 10 | ✅ proptest fuzzing introduced |
| Type Checker | 7 | **10** | 10 | ✅ Canonical AST separation, expr.ty direct embedding, infer_types/expr_types HashMap eliminated |
| IR Design | 9 | **10** | 10 | ✅ |
| Nanopass | 8 | **10** | 10 | ✅ |
| Monomorphization | 7 | **10** | 10 | ✅ |
| Error Diagnostics | 9 | **10** | 10 | ✅ |
| Code Quality | 7 | **10** | 10 | ✅ String interning (Sym type, lasso), Sym throughout Ty/FnSig/TypeEnv |
| Tests | 8 | **10** | 10 | ✅ fuzzing, 167/167 WASM all pass, TypeVar regression 6 cases, parallel execution (2:30→16s), nanopass unit tests (28 tests, 16 passes) |
| Build System | 7 | **10** | 10 | ✅ build.rs split, per-file cache + parallel test execution |
| Codegen Integration | 5 | **10** | 10 | ✅ WASM result.collect/partition/collect_map. Stdlib dispatch: Rust uses TOML-driven auto-gen, WASM uses direct binary emit — architectural divergence makes unification counterproductive |

**Total: 64/100 → 110/110**

---

## Done (Completed)

### Phase 1: Pipeline Integration ✅

- [x] Target::Wasm + Pipeline integration
- [x] Pass dependency declarations
- [x] E003 --explain
- [x] BoxDeref pipeline integration

### Phase 2: Type Checker Split ✅

- [x] mod.rs split — 850 lines → 4 modules
- [x] calls.rs split — 588 lines → 3 modules

### Phase 3: Monomorphization ✅

- [x] File split — 1296 lines → 6 modules
- [x] Direct construction (PR#93)
- [x] Incremental instance discovery (PR#93)
- [x] Convergence detection (PR#91)

### Phase 4: Nanopass + Walker Split ✅

- [x] Stream fusion split — 1199 lines → 5 modules
- [x] Walker split — 1667 lines → 6 modules
- [x] Codegen exit unification (PR#92)

### Phase 5: Code Quality ✅

- [x] **5.1 String Interning** — `Sym` type (lasso ThreadedRodeo), Copy + O(1) equality. Sym throughout Ty/FnSig/ProtocolDef/TypeEnv/VariantCase. build.rs stdlib_sigs generation updated. 33 files changed.
- [x] **5.3 Clone Reduction (Foundation)** — Sym is Copy so all name field clones are eliminated. n.clone() → *n in Ty's map_children.

### Phase 5b: Test and Build Infrastructure ✅

- [x] **Proptest fuzzing** — lexer/parser/checker × arbitrary/structured = 6 targets, 10,000 cases each
- [x] **All tests pass** — 159/159 .almd tests, CI green
- [x] **Test parallelization** — compile_to_binary + per-file hash cache + thread pool execution (2:30 → 16s)
- [x] **WASM result.collect/partition/collect_map** — CI WASM tests all pass
- [x] **TypeVar ICE guard** — 2-layer guard after lower + after mono, all inference TypeVars resolved
- [x] **TypeVar regression tests** — 6 patterns: chained fold, none/err comparison, generic variant, recursive variant, closure field access
- [x] **Nanopass unit tests** — 28 tests covering 16 passes (TCO, ResultPropagation, EffectInference, BorrowInsertion, CloneInsertion, ClosureConversion, StdlibLowering, BuiltinLowering, etc.)

---

## Items Retired

- ~~Phase 6.5 Parser Fuzzing~~ → proptest introduced in Phase 5b
- ~~Phase 7 xtask migration~~ → build.rs is already split into 3 modules, sufficient. xtask adds little value
- ~~Phase 5.2 Clone Reduction (Rc\<Ty\>)~~ → Sym introduction eliminated name clones, resolving the main hotspots. Rc\<Ty\> has poor cost-benefit ratio
- ~~Stdlib Dispatch Unification~~ → WASM emits binary directly, Rust uses TOML templates. Architectural divergence makes unification counterproductive. Current dual approach is correct.

---

**Score: 110/110 — Complete.**
