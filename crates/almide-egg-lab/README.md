# almide-egg-lab

**Feasibility PoC for egg (equality saturation) on a minimal Almide IR subset.**

This crate exists to answer one question before we commit to the
10-month [MLIR Backend + Egg Rewrite Engine](../../docs/roadmap/active/mlir-backend-adoption.md)
arc: *can egg actually express Almide's existing stream-fusion rules,
terminate quickly, and pick the fused form?*

## Verdict

**Yes.** The 5 tests below pass in 1.5s on release build, covering
the three most common fusion shapes plus transitive / cross-rule
composition.

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
- Does not substitute lambdas — uses `compose` / `and-pred` pseudo-ops
  as markers. A later lowering step (Stage 1 of the arc) would
  beta-reduce them into real lambdas.
- Not wired into the main compiler pipeline. No codegen impact,
  deletable with zero blast radius.

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
- **Integration with `almide-ir::IrExpr`.** Translation in both
  directions (`IrExpr` ↔ `RecExpr<AlmideExpr>`) is Stage 1 scope.

## Running

```bash
cargo test -p almide-egg-lab --release
```

## Next step if kept

If this PoC passes review, Stage 1 of the MLIR adoption arc (see
`docs/roadmap/active/mlir-backend-adoption.md`) will:

1. Expand the language to cover the full `IrExpr` enum
2. Replace `lam`/`compose`/`and-pred` pseudo-ops with real lambda
   substitution (custom `Applier`)
3. Add type-parametric rules
4. Port the remaining 4 `pass_stream_fusion` rules
5. Run equality saturation as an opt-in pass before the existing
   Nanopass pipeline

## Next step if rejected

If saturation proves intractable on real Almide programs, we fall
back to GHC RULES-style priority rewriting inside the existing
Nanopass framework. The MLIR adoption arc continues without egg.
This crate gets deleted, no other code changes.
