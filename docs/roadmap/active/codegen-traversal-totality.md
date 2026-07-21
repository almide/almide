<!-- description: Make a codegen pass forgetting to recurse into a node kind a compile error or CI failure, never a silent native/WASM divergence -->
# Codegen traversal totality

**Goal:** make "a pass forgot to recurse into a node kind" a **compile error or a CI failure**, never
a silent native↔WASM divergence. This converts the largest remaining *asymptotic* divergence class
("keep sweeping, keep finding") into a *static* guarantee by construction.

## How we got here

`DIV2` (PR #347): `xs |> group_by(..)` then `map.keys(g) |> map(k => …get(g,k)…)` then `map.values(g)`
failed to build natively (E0382, borrow of moved `g`); WASM ran fine. Root cause was **not** about
maps or ownership — it was that `CaptureClonePass::transform_expr` hand-rolled
`match &expr.kind { …; _ => {} }` and the `_ => {}` **silently dropped the `Borrow`-wrapped subtree**
(`list.join(&list.map(keys, k => …g…), …)`), so the closure nested under `&(…)` was invisible to
capture analysis and its captured `g` was never pre-cloned.

The fix added the missing wrapper arms. But an audit of all 37 codegen passes shows **this is a class,
not an incident**: ~24 passes hand-roll a `match expr.kind` recursion ending in a non-recursing
`_ => {}` / `other => other`, each a latent divergence waiting for the right nesting. The asymptotic
sweep keeps finding them one at a time. We want to close the class.

## The key insight

Every native-only divergence we have fixed is the same shape: **native asks rustc to re-derive what
the IR already knows, and the emission under-specifies it** — OR (this class) **a pass under-traverses
the IR and silently skips a subtree**. Both are "the pass relied on something instead of carrying the
fact / visiting the node." The static cure is to make the carry/visit *total by construction*.

For traversal specifically: a pass should `match expr.kind` **only for the nodes it transforms**, and
**delegate all other recursion to a single, compile-enforced-exhaustive primitive** — never to a
hand-rolled `_ => {}`.

## The good news: the infrastructure already exists

| Primitive | File | Mutable | Exhaustive (no `_=>`) | Recurses every wrapper child |
|-----------|------|---------|----------------------|------------------------------|
| `IrExpr::map_children` | `almide-ir/src/lib.rs` | by-value | **yes** (documented chokepoint) | yes |
| `IrMutVisitor` / `walk_expr_mut` | `almide-ir/src/visit_mut.rs` | in-place | **yes** | yes |
| `IrVisitor` / `walk_expr` | `almide-ir/src/visit.rs` | read-only | **yes** | yes |
| `substitute_var_in_expr` | `almide-ir/src/substitute.rs` | by-value | yes | yes |
| `fold_expr` | `almide-ir/src/fold.rs` | by-value | **NO — `_ => {}` at L123** | yes (today) |

`IrExpr::map_children` is the gold standard: its doc already says *"All variants are listed explicitly
(no wildcard) so that adding a new `IrExprKind` causes a compile error here — forcing the author to
decide how its children should be traversed."* `free_vars.rs` is the exemplar consumer for a
**context-threading** analysis: it overrides only the binder nodes (`Lambda`/`Block`/`Match`/`ForIn`)
to push/pop a `bound` set and falls through to `walk_expr` for everything else — so it can never drop
a subtree. `pass_concretize_types` and `pass_result_propagation`'s `resolve_err_types` already use
`IrMutVisitor` correctly and are safe.

So we are **~80% there**. The work is to (a) close the one infra hole (`fold.rs`), (b) migrate the
hand-rolling passes onto these primitives, and (c) lock it with a lint so it can't regress.

## Plan

### Phase 0 — infra (small, pure-safety)
- Delete `fold_expr`'s `_ => {}`; reimplement its recursion on `IrExpr::map_children` (or make the
  match exhaustive). No behavior change today; makes variant-addition a compile error.

### Phase 1 — enforcement lint (anti-regression, the "static" lock)

**The enemy is *silence*, not the wildcard.** DIV2 was `_ => {}` — a catch-all that *does nothing* and
silently drops the subtree. A wildcard is fine; a *silent no-op* wildcard is the bug. There are three
catch-all forms; only one is banned:

| Form | Example | Verdict |
|------|---------|---------|
| **Delegate** (correct default recursion) | `_ => walk_expr_mut(self, e)` | ✅ closes the DIV2 class by construction |
| **Loud provisional** (Swift `Never` model) | `_ => unreachable!("…")` / `_ => todo!()` / `panic!` | ✅ traps *at codegen time* on the offending program — surfaces the gap, never ships a divergence. Lets a pass be written incrementally. |
| **Silent no-op** | `_ => {}` / `_ => e` (returns unrecursed) | ❌ the only banned form — drops a subtree silently |

This mirrors the *language's own* `hole`/`todo`: a provisional body typechecks (its type unifies with
context) and renders to `todo!()` (`Never` in Rust — traps loudly if reached). The compiler-internal
walks should obey the same discipline: a not-yet-handled node kind must be *loud* (`unreachable!`) or
*correctly recursed* (`walk_*`), never *silent*.

- Add a CI test (`tests/traversal_lint.rs` or a build-script check) that scans
  `crates/almide-codegen/src/pass_*.rs` and `crates/almide-ir/src/*.rs` for the banned form: a `match`
  on an `IrExprKind` value whose catch-all body is a **silent no-op** (empty `{}`, or returns the node
  without recursing) — while **allowing** catch-alls that delegate to a `walk_*` primitive or diverge
  (`unreachable!`/`todo!`/`panic!`/a `Never`-typed expr). Allow-list the canonical primitives
  themselves. A new silently-dropping walker fails CI; a loud provisional one passes.
- This is what makes the guarantee *static* rather than *snapshot-in-time* — and keeps incremental /
  provisional passes writable (the Swift-`Never` escape hatch), in line with Almide's
  modification-survival-rate mission.

### Phase 2 — migrate the mutating rewriters (prioritized by audit risk)
Each migration is **behavior-subtle** (some catch-alls were intentional non-recursion). Every pass
gets a differential check after migration: `native == wasm` on a spec sweep + the existing suite.

| Tier | Passes | Notes from audit |
|------|--------|------------------|
| **HIGH** | `pass_clone` (CloneInsertion) | omits `IterChain`/`RcWrap` — **a live DIV2-sibling**: clone insertion blind to fused chains. Fix first; it is the other half of DIV2. |
| | `pass_borrow_inference` | 5 hand-rolled walkers, several dropping catch-alls; central to native borrow modes. |
| | `pass_stdlib_lowering` | `rewrite_expr` `other => other` over ~36 variants; "the DIV2 risk is here." |
| | `pass_perceus` | WASM RC; every helper hand-rolled, omits ~all wrappers. |
| **MED** | `anf`, `box_deref`, `capture_clone`, `fan_lowering`, `lambda_type_resolve`, `list_pattern`, `match_subject`, `mut_param_lowering`, `peephole`, `result_propagation` (lift path), `rust_lowering` (idx path), `tco` | each drops some wrapper/collection subtree. `capture_clone` is already partially fixed (#347) but should move to the `IrMutVisitor`+binder-stack pattern so it can't drift again. |
| **LOW / analyses** | `auto_parallel`, `builtin_lowering`, `closure_conversion`, `const_fold`, `effect_inference`, `licm`, `match_lowering`, `matrix_shape_spec` | mostly broad coverage or read-only. **`effect_inference` omitting `IterChain` is a real latent bug** (effects inside a fused chain mis-classified) — verify. Read-only analyses migrate to `IrVisitor`. |
| **DEAD** | `pass_shadow_resolve` | `targets() == None`; never runs. Leave or delete. |

### Phase 3 — differential CI gate (shared with closure-cross-target work)
A well-typed-IR generator + `native` vs `wasm` execution assertion in CI (extending the Perceus
proptest + byte-identical determinism gate) catches any residual traversal/codegen divergence over a
bounded input space — "static" in the continuous-machine-checked sense, the backstop behind the lint.

## Sequencing

Phases 0+1 are low-risk and high-leverage — they install the guarantee. Phase 2 is the bulk and is
behavior-subtle; it should land as **several reviewed PRs by tier**, each differential-checked, not
one mega-diff. Phase 3 is shared infrastructure with `closure-cross-target-completeness.md`.

**First PR proposal:** Phase 0 (`fold.rs`) + Phase 1 (lint) + the HIGH-tier `pass_clone` migration
(the proven DIV2-sibling). This proves the pattern end-to-end and closes a known live bug, while the
lint prevents new ones.
