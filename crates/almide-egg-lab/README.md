# almide-egg-lab

**Feasibility PoC for egg (equality saturation) on a minimal Almide IR subset.**

This crate exists to answer one question before we commit to the
10-month [MLIR Backend + Egg Rewrite Engine](../../docs/roadmap/active/mlir-backend-adoption.md)
arc: *can egg actually express Almide's existing stream-fusion rules,
terminate quickly, and pick the fused form?*

## Verdict

**Yes.** The toy PoC (5 tests) plus the real-IR bridge (11 tests,
including 5 that cover lift → saturate → **lower** round-trips with
live `VarTable`-backed beta-reduction) pass in ~2.5 s on release
build. Fusion fires on three canonical shapes, saturation converges
inside a small iteration budget, cross-rule composition works
without manual phase ordering, and — most importantly — `compose`
and `and-pred` markers are beta-reduced into real `IrExprKind::Lambda`
nodes with fresh `VarId`s at lower time, so the extracted output is
a well-typed `IrExpr` fragment ready to feed back into the existing
pipeline.

| Test | What it proves |
|---|---|
| `identity_map_collapses` | `map(xs, identity) ≡ xs` — single-rule rewrite |
| `map_map_fuses` | `map(map(xs, f), g) ≡ map(xs, g ∘ f)` — functor law |
| `filter_filter_fuses` | `filter(filter(xs, p), q) ≡ filter(xs, p ∧ q)` — predicate conjunction |
| `triple_map_fuses_transitively` | Three-stage chain folds to one traversal — saturation converges under iter/node budgets |
| `identity_inside_map_chain_eliminates` | Identity elimination **+** map fusion compose **without explicit phase ordering** — the core motivation for choosing egg over priority-ordered rewriters (GHC RULES, MLIR PDL) |

The last test is the key one. In the imperative `pass_stream_fusion`
we have to decide the order: eliminate identity first, then fuse? Or
fuse first, then notice the collapse? Each ordering misses some
programs. With egg, we don't pick — saturation holds both alternatives
in the e-graph and the extractor picks the cheapest.

## Scope (intentionally small)

- Models only the IR shapes needed for the three rules: `map`, `filter`,
  `fold`, `lam`, `compose`, `and-pred`, plus numeric / symbolic atoms.
- **Lambda substitution happens at lower time, not inside saturation.**
  E-graphs cannot represent binders without extra alpha-renaming
  machinery (see egg's `examples/lambda.rs`). Saturation keeps
  `compose` / `and-pred` as zero-cost markers; the single extracted
  best form is then beta-reduced into real `IrExprKind::Lambda`
  nodes with fresh `VarId`s. One-shot allocation is bounded by the
  output size, which sidesteps the e-graph alpha-equivalence issue
  without losing the fresh-id discipline the main IR requires.
- Not wired into the main compiler pipeline. No codegen impact,
  deletable with zero blast radius.

## What's included

- **Toy Language PoC** (`src/lib.rs`, `tests::`): string-parsed
  `AlmideExpr` expressions, three fusion rules, five tests.
- **IrExpr bridge** (`src/bridge.rs`, `tests/bridge_test.rs`): lifts
  `almide_ir::IrExpr` subtrees (list combinators + identity-lambda
  detection) into `RecExpr<AlmideExpr>` and lowers the extracted
  form back into a well-typed `IrExpr`. Eleven tests cover identity
  elimination, map/filter fusion, cross-rule composition, opaque
  pass-through, and the five round-trip cases that verify fresh
  `VarId` allocation, body substitution, and structural shape on the
  lowered output.

## What this does NOT yet answer

Tracked as open questions in the arc document:

- **Type-aware rules.** Real Almide rules depend on element types
  (`List[Int]` vs `List[String]`). egg supports this via `Analysis`
  but we haven't exercised it.
- **Large IR scale.** Tested up to three-stage chains. Stage 1 will
  benchmark on programs of 100–1000 IR nodes to see if saturation
  time stays tolerable.
- **Cost-function calibration for targets.** The current `FusionCost`
  just penalizes loops. Real target-aware costs (Rust iter-collect
  vs WASM manual loop vs GPU offload) are a Stage 3 concern.
- **Lambda substitution inside the e-graph.** `lower`-time
  beta-reduction is sufficient to prove the round-trip works, but
  in-graph substitution (custom `Applier` + alpha-renaming, à la
  `egg/examples/lambda.rs`) would let saturation see through compose
  markers and potentially discover more fusions. Scope for Stage 1+
  once we see which rules need it.

## Running

```bash
cargo test -p almide-egg-lab --release
```

## Next step if kept

If this PoC passes review, Stage 1 of the MLIR adoption arc (see
`docs/roadmap/active/mlir-backend-adoption.md`) will:

1. Expand the language to cover the full `IrExpr` enum
2. Decide whether to keep lower-time beta-reduction or promote
   substitution into the e-graph via a custom `Applier`
3. Add type-parametric rules
4. Port the remaining 4 `pass_stream_fusion` rules
5. Run equality saturation as an opt-in pass before the existing
   Nanopass pipeline

## Next step if rejected

If saturation proves intractable on real Almide programs, we fall
back to GHC RULES-style priority rewriting inside the existing
Nanopass framework. The MLIR adoption arc continues without egg.
This crate gets deleted, no other code changes.
