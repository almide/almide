# Unit 0.42 — Plan: one driver (the frontend runs once)

- **Aim**: 0.4x arc — the edit loop stops scaling with how many times we happen to re-run the
  frontend, and the six hand-synced driver sequences stop being a drift surface. This is also
  the precondition for the decade's later rows: a query layer (0.47–0.48) and a compile cache
  (0.49) cannot be retrofitted onto six parallel drivers.
- **Issues**: [#925](https://github.com/almide/almide/issues/925)

## In three lines

Every wasm build runs the whole front half twice: `compile_to_wasm_bytes` builds an
`IrProgram`, discards it on the next line, and hands raw SOURCE TEXT to the renderer, which
re-lexes, re-parses, re-typechecks, re-lowers, re-optimizes, re-monomorphizes.
There are also ≥6 independent hand-written `lower → optimize → mono → ir_link` sequences kept
in sync by hand, and #785 is already a recorded bug caused by exactly that divergence.
Done means one driver function with one signature, called from every site, and a renderer
entry point that takes the `IrProgram` it is given.

## Background

Verified against current `develop` (2026-07-31), not taken from the issue text:

- `src/cli/build.rs:578` `compile_to_wasm_bytes` — parse → typecheck → `lower_and_link_wasm_ir`
  → `verify_wasm_ir` → `check_no_native_only_matrix`, then line 598 is literally
  `let _ = (&mut ir_program, allow_unverified, verified);` and the next line calls
  `render_wasm_module(&source_text, …)`. The `IrProgram` is built, gated, and thrown away.
- The comment above that line still describes the v1 renderer as an OPT-IN that falls through
  to v0 on a wall. v0 was retired in #782, so the discard is vestigial: it is not buying the
  fallback it was written for.
- `ir_link` appears in 8 files — `src/compile_driver.rs`, `src/cli/build.rs`,
  `src/cli/emit.rs`, `src/cli/commands.rs`, `crates/almide-mir/src/pipeline.rs`,
  two `crates/almide-mir/examples/`, and `crates/almide-interp/tests/eval_test.rs` —
  which is the ≥6 the issue counts.

The cost has two halves and they are not equally important. The wasted frontend pass is
measurable and annoying. The drift between six drivers is the one that has already produced a
bug (#785), and it is the one that gets worse as the decade adds passes.

## Scope

- S1 One driver: a single function taking (file, options) and returning the linked, verified
  `IrProgram`, with one signature, in one place.
- S2 A renderer entry point that accepts an already-built `IrProgram`, so the wasm path stops
  round-tripping through source text.
- S3 Migrate every call site (CLI build / emit / commands / compile_driver, the mir pipeline,
  both mir examples, the interp test harness) onto S1 + S2.
- S4 Make the #785 class unrepresentable: after S3 there is no second place where the stage
  order can be spelled, and a test or a structural gate pins that.
- S5 Measure and record the build-time delta (the frontend now runs once).

## Out of scope

- The query/incremental layer (0.47–0.48) and the compile cache (0.49). This Unit only makes
  them possible; it does not start them.
- Any change to what the stages DO. If a stage's behaviour changes, that is a separate finding
  with its own contract check — this Unit is a plumbing change and must be output-identical.

## Done-criteria

- `compile_to_wasm_bytes` no longer discards an `IrProgram`, and no build path passes source
  text to the renderer when it already holds the IR.
- Exactly one function in the workspace spells the `lower → optimize → mono → ir_link` order;
  a test or gate fails if a second one appears.
- Every call site listed in S3 is migrated — enumerated in the ledger, not summarized.
- Full CI green, and the byte-identity gates in particular: this is a plumbing change, so
  `spec/wasm_cross` output must be unchanged, not merely "still passing".
- The build-time delta is measured on a fixed program and recorded in the ledger with the
  command used.

## Risks

- **R1 — a hidden behavioural dependence on the second frontend run.** The renderer re-runs
  canonicalize/infer/lower from raw programs; if any of that mutates state the first run left
  behind, feeding it the first run's IR could change output. Absorption: treat byte-identity
  of `spec/wasm_cross` as the gate, and if any fixture's bytes move, STOP and diagnose — a
  moved byte here is a real semantic finding, not a rebase artifact.
- **R2 — the examples and the interp test harness are easy to forget.** They are not on the
  CI hot path the way `src/cli` is. Absorption: S3 enumerates them by path in the ledger, and
  S4's gate is what actually prevents the omission.
- **R3 — scope creep into the query layer.** The moment there is one driver, making it
  incremental looks cheap. It is not, and it is 0.47. Absorption: the out-of-scope line above
  is a hard boundary for this Unit.

## Proposed Bolts

- **B1** — Inventory: enumerate every `lower → optimize → mono → ir_link` site with its file,
  line, and what it needs from the driver (options, whether it wants the verified gates).
  Output is the migration table the later Bolts work down.
- **B2** — Introduce the one driver and the IR-accepting renderer entry point, with no call
  site migrated yet, so it lands green on its own.
- **B3** — Migrate the CLI paths (build / emit / commands / compile_driver) and delete the
  discard. Byte-identity of `spec/wasm_cross` is the acceptance check.
- **B4** — Migrate the non-CLI sites (mir pipeline, both examples, interp test harness).
- **B5** — Land the structural gate that makes a second stage-order spelling fail, measure the
  build-time delta, and close #925.
