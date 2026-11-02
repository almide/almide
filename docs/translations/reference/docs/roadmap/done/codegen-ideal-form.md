Description: WASM codegen redesign toward declarative dispatch and explicit symbol resolution.
Historical completion metadata: 2026-04-19.
<a id="codegen-ideal-form"></a>

# Codegen Ideal Form

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](codegen-ideal-form.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](codegen-ideal-form.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](codegen-ideal-form.zh-CN.md)

> Non-normative reader snapshot pinned to the original commit; the original remains authoritative. No automatic synchronization or independent execution, implementation verification, proof checking or measurement. Historical commands and design instructions are documentation, not executed instructions.
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/codegen-ideal-form.md)

> Source caveat: This is the historical April 13–19 design and closure record, not a current implementation audit. Closure includes work split into later milestones: it does not mean every ideal-form goal was implemented. Early harmless-default and remaining-layout-risk descriptions, changing VarTable scope, deferred versus completed Steps, and differing count estimates are retained as separate historical layers. The ideal resolved-symbol/error design differs from the recorded Named-type/panic implementation. The source's unreachable/proven language describes its recorded finite test evidence, not an independently established universal proof. Later dispatch and definition-table unification work remains distinguished from completed work.

> **Arc status (2026-04-19): CLOSED.** All seven items landed or split into their own arcs. See “Final status” below for per-item landing and the outstanding follow-ups that were deliberately scoped out.

Incrementally refactor the codegen accumulated during v0.13 toward its **ideal form**. The bugs exposed by this nn/WASM work—scattered type resolution, test-function name collisions, lifted-closure var_table cross-wiring, and the stdlib emit `list_elem_ty` bug at 20 sites—all stem from **design-level decay**.

<a id="動機"></a>

## Motivation

Every landmine encountered in today's nn WASM work shares one trait: **ad hoc fixes are not being made in the right place**.

| Symptom | Real cause | Symptomatic treatment |
|---|---|---|
| Nested lambda types unresolved | Closure Conversion contains type inference | `has_deep_unresolved`, VarTable priority |
| `p.0 * p.1` becomes `MulInt` | Op lowering fixes BinOp; emit corrects it | `fix_binop_type` |
| `list.slice(row, ...)` copies elements as i32 | `args[0].ty` is stale | Replace 20 sites with `resolve_list_elem` |
| `t.broadcast_add` jumps to a test function | Test-function name prefix omitted during module registration | Add `is_test` check |
| Lifted closure VarId breaks | Boundary management between program.var_table and module.var_table | Keep module lifts in module.functions |
| Unresolved function → runtime `unreachable` | Resolution responsibility is not closed at compile time | (Unresolved) |

Every case is “one more site to fix before it works.” The next recurrence is only a matter of time.

<a id="ターゲット-理想形"></a>

## Target: the ideal form

<a id="1-関数解決を独立パスに-最優先"></a>

### 1. Make function resolution an independent pass (highest priority)

**Current**: WASM emit sees `CallTarget::Module { module, func }` and looks up `almide_rt_{module}_{func}` in `func_map`. If absent, it falls back to a bare name; if that also fails, `emit_stub_call` emits **runtime `unreachable`**. The worst UX: type checking passes, build passes, WASM validation passes, then runtime trap.

**Ideal**:

- `pass_resolve_calls.rs` (new nanopass, applied to both Rust/WASM)
- Walk every `CallTarget::Module` / `CallTarget::Named`
- Canonicalize module names (aliases → actual modules; normalize nn.tensor vs tensor)
- Rewrite to a reference (`Sym`) to the target `IrFunction`, a new variant such as `CallTarget::Resolved { func_ref: Sym }`
- Unresolvable calls are **compile-time errors**, with “Did you mean...?” diagnostics
- Emit only calls known symbols; **delete** `emit_stub_call`

Postcondition: every `CallTarget::Module` / bare `CallTarget::Named` has been rewritten to `Resolved`.

<a id="2-stdlib-関数の宣言駆動化"></a>

### 2. Make stdlib functions declaration-driven

**Current**: `emit_wasm/calls.rs` (1000+ lines), `calls_list.rs` (1000+ lines), `calls_list_closure.rs`, `calls_list_closure2.rs`, `calls_list_helpers.rs`, `calls_map.rs`, `calls_option.rs`, `calls_set.rs`—piles of match statements. The same `elem_ty = self.list_elem_ty(&args[0].ty)` pattern appears at 20+ sites, so fixing one bug required changing 20 sites.

**Ideal**:

- Rust is already TOML-driven (`stdlib/defs/*.toml`). Use the same approach for WASM.
- Each stdlib function = a declarative description:
  ```toml
  [list.slice]
  params = ["list", "int", "int"]
  ret = "list_of[0]"  # 型式
  elem_source = "arg[0]"  # 要素型をどこから取るか
  emit = """
  # alloc: 4 + new_len * elem_size
  # loop: copy [start..end] from src to dst
  ...
  """
  ```
  Original comment translations: type expression; where to obtain the element type. The emit sketch allocates 4 + new_len × elem_size and loops to copy [start..end] from src to dst.
- The emitter is a **generic engine** generating WASM from declarations. Per-stdlib custom code disappears.
- “Where to get the element type” is part of the declaration, preventing wrong choices between `list_elem_ty` and `resolve_list_elem`.

Effect: wholesale removal of emit_wasm calls*.rs (estimated −4000 lines).

<a id="3-irmutvisitor-trait"></a>

### 3. `IrMutVisitor` trait

**Current**: `rewrite_var_ids` is 200 lines of manual match, breaking whenever a new IrExprKind variant is added. The same pattern is scattered across many `pass_*.rs` files.

**Ideal**:

- Add `IrMutVisitor` to `almide-ir`:
  ```rust
  pub trait IrMutVisitor {
      fn visit_expr_mut(&mut self, expr: &mut IrExpr) { walk_expr_mut(self, expr); }
      fn visit_stmt_mut(&mut self, stmt: &mut IrStmt) { walk_stmt_mut(self, stmt); }
      fn visit_pattern_mut(&mut self, p: &mut IrPattern) { walk_pattern_mut(self, p); }
  }
  pub fn walk_expr_mut<V: IrMutVisitor + ?Sized>(v: &mut V, expr: &mut IrExpr) { /* recurse */ }
  ```
- `rewrite_var_ids` becomes 30 lines:
  ```rust
  impl IrMutVisitor for VarIdRewriter {
      fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
          if let IrExprKind::Var { id } = &mut expr.kind {
              if let Some(new) = self.mapping.get(id) { *id = *new; }
          }
          walk_expr_mut(self, expr);
      }
  }
  ```
- Symmetric with the existing read-only `IrVisitor`.

<a id="4-型解決の一元化"></a>

### 4. Unify type resolution

**Current**: type resolution is scattered across layers:
- `LambdaTypeResolve` pass (top-down)
- `propagation.rs` (mono)
- `emit_wasm/closures.rs::resolve_expr_ty` (during emit)
- `emit_wasm/collections.rs::emit_tuple_index` (deciding VarTable vs expr.ty priority)
- `resolve_list_elem` in each `calls_*.rs`

Solve the same “what is the type of `p.0`?” at 3 places. Inconsistency causes bugs.

**Ideal**:

- After `LambdaTypeResolve`, run a **`TypeConcretization` pass** aligning every IrExpr's `.ty` with VarTable.
- Postcondition: no `Ty::Unknown` / `Ty::TypeVar` on executable paths.
- Emit **only trusts** `expr.ty`; no VarTable lookup needed.
- Emit-time type lookup helpers such as `resolve_expr_ty` / `resolve_list_elem` can be removed.

This would end the `is_unresolved_structural()` vs `has_deep_unresolved()` vs `is_unresolved()` nightmare encountered here.

**Remaining gaps (v0.14.7, 21 cases in WASM `ALMIDE_CHECK_IR=1` audit)**:

Running spec/ on WASM with `ALMIDE_CHECK_IR=1` causes 15-21 audit-violation panics. Breakdown:

| Pattern | Example | Count |
|---|---|---|
| Failed backward element-type inference for empty list/collection literals | `fn fan.map empty list: List[Unknown]` / `fn empty for: List[Unknown]` | 10+ |
| Empty Codec auto-derived field | `fn decode_container: List[Unknown]` / `fn derive Codec: empty list field` | 3-4 |
| Undecided inner type in Result/Option ok/some() | `fn safe_div: Result[Unknown, String]` / `fn validate_age` | 2-3 |
| OpenRecord → concrete record unresolved | `fn chain_b__Unknown: OpenRecord { name: String }` | 1 |
| Deep generic context | `fn is_balanced: Option[List[Unknown]]` | 1 |
| Event/Tuple empty payload | `fn extract_click_positions: List[Tuple[Unknown, Unknown]]` | 1 |

By default (no environment variable), defensive fallbacks in WASM emit (`resolve_list_elem`'s 4-stage fallback, `Unknown` → i32 assumption) absorb this, so 203/206 pass. Latent risk: an empty list allocated with i32 elements followed by i64 pushes can cause layout inconsistency, for example. The pattern is merely absent from spec tests; user code may hit it.

**Fix decomposition (sub-phase candidates from 0.14.8 onward)**:

- (a) Infer empty literals backward from consumers: in `fn f(xs: List[Int]) = f([])`, infer `[]: List[Int]`. Checker extension, half a day to 1 day.
- (b) Infer inner types for `ok(x)` / `some(x)` wrappers: derive argument types backward from the wrapper's expected ret_ty. Half a day.
- (c) Empty Codec auto-derived fields: fix empty-list field element types to their defaults in `derive.rs`. 1-2 hours.
- (d) OpenRecord → concrete resolution: tidy checker open → closed resolution paths. Several days.
- (e) Event/Tuple empty payloads: propagate payload types from variant signatures. Half a day.

(a) + (b) + (c) address 17/21; (d) + (e), the remaining 4. Total scope: 1 week.

**Interim stance**: harmless under defaults, so not a release blocker. Raise priority and fix if dojo measurements expose the pattern in user code. Natural work between stdlib-declarative-unification arc steps (both improve WASM emit precision).

<a id="5-vartable-の責務明確化"></a>

### 5. Clarify VarTable responsibilities

**Current**:
- `program.var_table` and `module.var_table` are **different objects**
- Where ClosureConversion puts lifted closures breaks VarId references (fixed today)
- Tests and WASM emit repeatedly decide “which var_table belongs to this function?”

**Ideal** (2 choices):

A) **Region information in VarId**: use `VarId(ModuleId, u32)` and unify lookup as `program.lookup(var_id)`.
   - Advantage: simple argument passing.
   - Disadvantage: larger VarId (u32 → u64 equivalent).

B) **Per-function var_table**: add `IrFunction.var_table`, closed within function scope.
   - Advantage: explicit VarId locality.
   - Disadvantage: cross-function VarId references (closure captures) need rewriting.

Choose one and unify. B better matches “a function is a closed unit.”

<a id="6-test-namespace-完全分離"></a>

### 6. Completely separate test namespaces

**Current**: `fn broadcast_add` and `test "broadcast_add"` have the same `func.name`, colliding at registration. Today's workaround added `is_test` checks at 3 sites.

**Ideal**:
- At parser/lowering time, normalize test names into an always-noncolliding form such as `__test::{name}`.
- Retain `IrFunction.is_test`, but **names are already unique**. No downstream pass needs collision checks.

<a id="7-emit_stub_call-廃止"></a>

### 7. Remove `emit_stub_call`

**Current**: unresolved calls trap with `drop args; unreachable`.
- Validation passes → runtime trap.
- Hard to debug which call was unresolved.
- Evidence that type and function layers are not separated.

**Ideal**: with independent function resolution in #1, **no unresolved calls exist at emit time**. Delete `emit_stub_call` itself.

<a id="実装順-推奨"></a>

## Implementation order (recommended)

1. **#1 Function-resolution pass**—greatest value (runtime trap → compile error), highly independent, foundation for the rest.
2. **#3 IrMutVisitor**—mechanical work, eases other pass refactoring.
3. **#4 TypeConcretization**—easier once #1 and #3 exist.
4. **#5 VarTable integration**—interdependent with #4; do together.
5. **#6 Test namespace**—standalone, anytime.
6. **#2 Declaration-driven stdlib**—largest line reduction, but carefully staged.
7. **#7 Remove emit_stub_call**—byproduct of #1.

<a id="進捗ログ"></a>

## Progress log

- **2026-04-13**: started #1 (Phase 1a).
  - Added `pass_resolve_calls.rs` to Rust/WASM pipelines.
  - Postcondition `Custom(verify_all_calls_resolved)` detects unresolved user-module calls.
  - Phase 1b (IR rewrite) and 1c (delete `emit_stub_call`) are future work.
- **2026-04-13**: started #3.
  - New `crates/almide-ir/src/visit_mut.rs`: `IrMutVisitor` + `walk_expr_mut` / `walk_stmt_mut` / `walk_pattern_mut`.
  - Reduced `pass_closure_conversion.rs::rewrite_var_ids` from 160 to 60 lines.
  - All 183 spec tests + 60 nn WASM tests pass.
- **2026-04-13**: started #4 (ConcretizeTypes pass).
  - New `crates/almide-codegen/src/pass_concretize_types.rs`, bottom-up concretization of all IrExpr.ty.
  - BinOp normalization: correct `AddInt` on Float operands to `AddFloat` at IR level.
  - Effect: **completely remove** `emit_wasm/expressions.rs::fix_binop_type` (20-line correction function).
  - IR verify passes with `ALMIDE_CHECK_IR=1` (previous AddInt/Float mismatch failure).

- **2026-04-13**: extend #4—resolve Call return types + canonical `has_unresolved_deep`.
  - Add `Ty::has_unresolved_deep()` to `almide-types`, remove 2 ad hoc copies.
    - Three variants existed, `is_unresolved()` / `is_unresolved_structural()` / `has_deep_unresolved()`, and misuse caused 3 bugs today.
  - Add **SymbolTable** to ConcretizeTypes: build `(module, func) → ret_ty`.
    - Resolve user-module return types directly (e.g. `tensor.broadcast_add`).
    - Resolve stdlib `list.*` (map/filter/zip/fold/reduce etc.) via type expressions too.
  - Effects:
    - With `ALMIDE_AUDIT_TYPES=1`, 6/7 nn modules have zero warnings (only 6 `Option[Unknown]` cases in gguf remain; no harm).
    - **Simplify** `resolve_list_elem` fallback from 4 stages **to 2** (ConcretizeTypes can now be trusted).
  - All 183 spec + 60 nn WASM tests pass.

- **2026-04-13**: completely remove `resolve_expr_ty` + confirm `emit_stub_call` unreachable.
  - Remove `emit_wasm/closures::resolve_expr_ty` (**90+ lines**, ad hoc emit-time type resolution).
    - Replace 2 outside calls (`closures.rs:62`, `mod.rs:827`) with `expr.ty.clone()`.
    - Because ConcretizeTypes is trustworthy, emit now “only trusts types.”
  - Add `ALMIDE_WASM_STUB_PANIC=1` mode to `emit_stub_call`.
    - A hit causes compile-time panic, for CI.
    - Run **all 183 spec + 60 nn WASM tests** in panic mode → **not a single hit**.
    - Thus stub_call is already unreachable in the current codebase; #7 (stub deletion) can proceed safely.
  - Equivalent of #7 Phase 1 complete: measurements **proved the unexploded bomb was not an unexploded bomb**.

- **2026-04-13**: start #2 (declaration-driven stdlib Phase 1).
  - New `emit_wasm/stdlib_dispatch.rs`; `StdlibOp` enum expresses `Call1` / `Call2` / `Call3` / `FloatUnaryCall` patterns.
  - Move `emit_math_call` / `emit_float_call` / `emit_string_call` to declaration tables:
    - math: sin/cos/tan/log/exp/log10/log2 (7).
    - float: to_string.
    - string: trim/trim_start/trim_end/reverse/len/contains/split/join/count/replace (10).
  - Pattern proof complete; the same dispatcher can extend to all modules (option/map/bytes/value/list etc.).
  - All 183 spec + 60 nn WASM tests pass.

- **2026-04-13**: delete pass.rs dead code + split emit_wasm/calls.rs (4609 → 1144 lines).
  - Delete `pass.rs` stubs `OptionErasurePass` / `CloneInsertionPass` (stub) / `TypeConcretizationPass` (stub) / `StreamFusionPass` (stub) / `ResultPropagationPass` (stub) / `default_pipeline()` (never called).
    - Remove the presentation-fatal falsehood of no-op passes lined up in a pipeline.
    - Remove `TypeConcretizationPass` (Rust stub) / `OptionErasurePass` (Python stub) from pipelines.
  - Split `emit_wasm/calls.rs` (225KB / 4609 lines) by function:
    - `calls_env.rs` (68), `calls_random.rs` (219), `calls_datetime.rs` (481).
    - `calls_http.rs` (332), `calls_fs.rs` (1728), `calls_io.rs` (419), `calls_process.rs` (276).
  - calls.rs becomes 1144 lines, limited to “main dispatch + stub + assert_eq + fan + scalar ops other than env/random/datetime/http/fs/io/process.”
  - All 183 spec + 60 nn WASM tests pass.

<a id="非ゴール"></a>

## Non-goals

- **Redesigning Rust TOML templates** is out of scope (already working).
- **New features** (new stdlib, syntax) are not part of this roadmap—structural refactoring only.
- **Performance optimization**—correctness first.

<a id="測定"></a>

## Measurement

At each stage:
- **All spec/ tests pass** (zero regression).
- **All nn modules pass on both Rust/WASM** (maintain today's baseline).
- No decrease in **almide-dojo MSR**.

<a id="関連"></a>

## Related

- `docs/roadmap/active/whisper-almide.md`—runs on this foundation.
- `docs/roadmap/active/stdlib-defs-runtime-consistency.md`—related to #2.
- `crates/CLAUDE.md`—current design principles.
- `crates/almide-codegen/CLAUDE.md`—current three-layer architecture.

---

<a id="phase-3-arc-v0147--ideal-form-migration-done-2026-04-17"></a>

## Phase 3 Arc (v0.14.7) — Ideal Form Migration [DONE 2026-04-17]

Shipped as v0.14.7. Six ship points (S1 / S2 / S3 / S4 / B / A) closed all 5 items catalogued in `done/bundled-almide-ideal-form.md`. See CHANGELOG `[0.14.7]` for the full patch-layer audit and per-step notes.

Phase 3 focus: **dispatch layer deduplication**. The definition layer (TOML + `runtime/rs` + `emit_wasm/calls_*.rs`) is still triple-written; that scope belongs to the Stdlib Declarative Unification arc—see `active/stdlib-declarative-unification.md`. Beyond that, the egg + MLIR arc picks up at `active/mlir-backend-adoption.md`.

<a id="historical-phase-3-plan-below"></a>

### Historical Phase 3 plan (below)

Shipping bundled-Almide dispatch in 0.14.6 accumulated several patch layers (option/result signature co-dependence, surviving stub_call, mono coverage holes). Of the 5 debts in `bundled-almide-ideal-form.md`, (1)(2)(3) overlap codegen-ideal-form #1 / #4 / #7; (4)(5) are new. Combine them into a 4-step recovery arc.

<a id="step-s1--optionresult-signature-normalization-0147-phase31"></a>

### Step S1 — Option/Result signature normalization `0.14.7-phase3.1`

- Rewrite TOML `Fn[Unit] -> X` as `Fn[] -> X` (2 option.toml sites, none in other modules; investigated).
- Handle empty params in `stdlib_codegen.rs::parse_type`'s `Fn[...]` branch (currently only `fn()` side handled, not `Fn[]`).
- Check each target template correctly renders `Fn[] -> X` (Rust: `impl Fn() -> X`; WASM: closure convention).
- Delete `stdlib/option.almd` and `stdlib/result.almd` (signature override no longer needed).
- Remove option/result from `BUNDLED_MODULES` / `AUTO_IMPORT_BUNDLED` / `get_bundled_source`.
- Confirm `spec/stdlib/coverage_misc_test.almd` (`() => x` caller) passes with new signatures.
- All existing specs pass + smoke tests on both targets.

**Estimate**: 2-3h, 1 commit. Must precede #7 (stub removal), since option/result would block it.

<a id="step-s2--concretizetypes-hard-postcondition-0147-phase32-shipped"></a>

### Step S2 — ConcretizeTypes hard postcondition `0.14.7-phase3.2` *(shipped)*

Complete codegen-ideal-form #4. phase3.1 reached always-on audit + panic escalation through `ALMIDE_CHECK_IR`; phase3.2 completed the **flip to hard**:

- ConcretizeTypes audit in spec/ WASM sweep: 3 classes → 0 (PRs #194–#198).
  - Close 15-21 real cases, including list.zip `Option[Unknown]`, through empty-list elem ty / ResultErr Ok slot / fold acc back-prop / Match subject → pattern bindings / OpenRecord skip.
- Remove `ALMIDE_CHECK_IR` / `ALMIDE_VERIFY_IR` gating in `pass.rs`.
- Always panic on IR verify + Postcondition violations in debug; emit stderr diagnostics (not panic) in release.
- CHANGELOG entry: “`expr.ty` is now trustworthy by contract.”

Remaining WASM lifted-lambda TypeVar comes from a `ClosureConversion` pass-boundary issue, expected to resolve naturally in S3 (pass_resolve_calls Phase 1b-c).

<a id="step-s3--pass_resolve_calls-phase-1b-d--stub-廃止"></a>

### Step S3 — pass_resolve_calls Phase 1b-d + stub removal

Arc to progressively complete codegen-ideal-form #1 / #7.

- **Phase 1b** *(shipped, `0.14.7-phase3.5`)*—rewrite bundled-Almide stdlib fn `CallTarget::Module` → `CallTarget::Named { almide_rt_<m>_<f> }`.
- **Phase 1c** *(shipped, `0.14.7-phase3.2`)*—make `emit_stub_call*` compile-time panics (remove runtime traps).
- **Phase 1d** *(shipped, `0.14.7-phase3.3`)*—completely remove `emit_stub_call_named` / `emit_stub_call` helpers; replace each WASM dispatcher `_` fallback with inline `panic!("[ICE] emit_wasm: no WASM dispatch for ...")`. Already compile-time ICE under S2's flip, so behavior unchanged; diagnostics became module/dispatcher-specific.

Remaining work (Phase 1e+):

- Unify Rust + WASM dispatch entries into one `dispatch_module_call`.
  - Priority: (a) user/bundled fn in IR → user-fn call / (b) TOML → inline emit / (c) neither → compile-time ICE.
- WASM emit match arms such as `emit_list_call` / `emit_int_call` no longer need fallbacks (resolve pass resolves beforehand).

**Estimate**: Phase 1e 4-6h, resolve-pass extension + Rust/WASM dispatch integration. Main axis of the arc.

<a id="step-s5--test-namespace-normalization-shipped-2026-04-19"></a>

### Step S5 — Test namespace normalization *(shipped 2026-04-19)*

Complete codegen-ideal-form #6. `lower_test` now prefixes `IrFunction.name` with `__test_almd_`, removing `is_test` name-collision avoidance from every downstream pass (walker / emit_wasm / module emit):

- Newly expose `almide_ir::TEST_NAME_PREFIX` + `IrFunction::display_name()`.
- `lower/mod.rs::lower_test` allocates a prefixed `Sym`.
- Remove `if func.is_test { format!("__test_…") }` branches in `walker/mod.rs` and `emit_wasm/mod.rs`; use `func.name` directly.
- All downstream specs + WASM tests green (219 + 213).

Retain `is_test` for semantics (mono preservation / auto_unwrap / template selection), but **names are unique upstream**.

<a id="step-s6--vartable-consolidation-deferred-to-activevar-table-unificationmd"></a>

### Step S6 — VarTable consolidation *(deferred to `active/var-table-unification.md`)*

Remaining work from codegen-ideal-form #5: eliminate the two-layer `program.var_table` / `module.var_table` structure itself. This arc already resolved the core pain (lifted closures crossing var_tables) by keeping them module-local in `pass_closure_conversion`, so **split structural integration into an independent arc**. Track in `active/var-table-unification.md`.

<a id="step-s4--monomorphize-coverage-拡張-0147-phase33"></a>

### Step S4 — Extend Monomorphize coverage `0.14.7-phase3.3`

bundled-almide-ideal-form #5. 0.14.6's `monomorphize_module_fns` is narrow (Module call → bundled stdlib generics only). Expand:

- `CallTarget::Method` + paths where UFCS lowers to Module.
- Generic functions in user packages (not stdlib; purely user-defined).
- Cross-module generic chains (bundled A generic calls bundled B generic).

**Estimate**: 2-3h, 1-2 commits. Cleaner after S3 lands (only discover against resolved call targets).

<a id="release-cadence"></a>

### Release cadence

- `0.14.7-phase3.1`: S1 + S2
- `0.14.7-phase3.2`: S3
- `0.14.7-phase3.3`: S4
- `0.14.7`: full release (4 steps complete + dojo regression sweep)

At each `.N`, measure dojo's v0.14.6 baseline + subset; verify no MSR regression before proceeding.

<a id="non-goals"></a>

### Non-goals

- No new stdlib functions in Phase 3 (wait for infrastructure completion).
- Defer vocabulary additions (UFCS / ? chain expansion candidates) to 0.15 onward.
- Dojo task-bank expansion proceeds in parallel on the dojo team.

---

<a id="final-status-2026-04-19"></a>

## Final status (2026-04-19)

| # | Item | Status | Landed |
|---|---|---|---|
| #1 | Resolve-calls independent pass | **done** | v0.14.7-phase3.5 (PR #202) |
| #2 | Stdlib declarative dispatch | **split** | → `active/stdlib-declarative-unification.md` |
| #3 | `IrMutVisitor` | **done** | phase 1a (2026-04-13) |
| #4 | ConcretizeTypes hard postcondition | **done** | v0.14.7-phase3.2 (PR #199) |
| #5 | VarTable unification | **done** | commit `ba50fd7b` → `done/var-table-unification.md` |
| #6 | Test namespace normalization | **done** | commit `496c51e4` |
| #7 | `emit_stub_call` removal | **done** | v0.14.7-phase3.3 |

`#2` remains the only outstanding item—it was rescoped because dispatch-layer deduplication finished here, but the definition layer (TOML + `runtime/rs` + `emit_wasm/calls_*.rs`) is still triple-written and that work lives in its own arc.

<a id="what-this-arc-actually-shipped"></a>

### What this arc actually shipped

- **Call resolution is a pass**, not emit-time guessing. Compile-time ICE on unresolved calls; `emit_stub_call_*` helpers deleted; each WASM dispatcher fallback is inline `panic!("[ICE] ...")` that spec sweeps have proven unreachable.
- **`expr.ty` is trustworthy by contract.** `ConcretizeTypes` is a hard postcondition. Emit-time type re-derivation (`resolve_expr_ty` / `resolve_list_elem`'s 4-segment fallback) is deleted. Debug builds panic on residual `Unknown`; release emits a diagnostic.
- **`IrMutVisitor`** lets VarId-rewriting passes be 30 lines instead of 200; new IR variants do not silently break downstream passes.
- **Test blocks carry `TEST_NAME_PREFIX` upstream**—no per-site `if is_test { format!("__test_…") }` guards in walker / emit_wasm.
- **VarTables are unified**—the first codegen pass drains `IrModule.var_table` into `IrProgram.var_table`. The name-keyed `top_let_globals_by_name` mirror is now backup, not the primary cross-module lookup.

<a id="follow-ups-not-part-of-this-arc"></a>

### Follow-ups (not part of this arc)

- `active/stdlib-declarative-unification.md`—remove definition-layer triple writing.
- `active/mlir-backend-adoption.md`—egg + MLIR compiler-world arc.
- `active/dispatch-unification-plan.md`—unify Rust and WASM stdlib dispatch entries.
