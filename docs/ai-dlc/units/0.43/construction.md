# Unit 0.43 — Ledger

> Paired plan: [inception.md](./inception.md) — approved 2026-07-31 under the standing
> full-authority directive (no M0 round-trip; the reasoning stays reviewable here and in the
> plan).
> Rule: a checkbox without evidence (commit SHA / CI run URL) is invalid.
> Bolt N's evidence is recorded at the start of the next iteration, while checking the
> previous run's CI.

## Bolt ledger

| Bolt | What | Done-criteria for this Bolt | Status | Evidence |
|---|---|---|---|---|
| B1 | Inventory every `lower → optimize → mono → ir_link` site | A table with file, line, what it needs from the driver, and whether it wants the verified gates — no site summarized away | done — and it found more than an inventory (see B1 findings below) | Table + the stage-order split, recorded below |
| B2 | The one driver + an IR-accepting renderer entry point | Both land green with NO call site migrated yet | pending | — |
| B3 | Migrate the CLI paths and delete the discard | `spec/wasm_cross` bytes UNCHANGED (not merely passing); the `let _ = (&mut ir_program, …)` line is gone | pending | — |
| B4 | Migrate the non-CLI sites | mir pipeline, both mir examples, interp test harness — each named | pending | — |
| B5 | Structural gate + measurement + close #925 | A second stage-order spelling fails a test; build-time delta recorded with the command used | pending | — |


## B1 findings — the sites are not merely hand-synced, they are not the same sequence

The issue describes "≥6 independent hand-written `lower → optimize → mono → ir_link`
sequences synced by hand". Reading them, the drift is worse than sync drift: **there are two
different orders in the tree**, and they differ in where `ir_link` runs.

| Site | Order | Ships? |
|---|---|---|
| `src/cli/build.rs:416,433,436,439` | lower → **ir_link** → optimize → mono | builds the IR, then DISCARDS it (line 598) |
| `src/cli/commands.rs:395–397` | **ir_link** → optimize → mono | native build path — SHIPS |
| `src/compile_driver.rs:356,361` | mono → … → **ir_link** | — |
| `crates/almide-mir/src/pipeline.rs:328–330` | optimize → mono → **ir_link** | wasm build path — SHIPS |
| `crates/almide-interp/tests/eval_test.rs:39–42` | optimize → mono → **ir_link** | the 3rd oracle |
| `crates/almide-mir/examples/emit_cert_from_source.rs:97` | optimize → mono → **ir_link** | — |

So: **native ships order A (`ir_link` first), wasm ships order B (`ir_link` last), and the
third oracle judges order B.** The cross-target equivalence claim therefore rests on the
position of `ir_link` never mattering, and the independent judge is, by construction, on
wasm's side of that question rather than outside it.

That is a bigger statement than "the frontend runs twice". It does not mean anything is
currently wrong — `spec/wasm_cross` is green, so on the corpus the position does not change
observable behaviour. It means the equivalence is being CARRIED by an untested assumption
instead of by a shared driver.

Consequences for this Unit, decided here rather than discovered in B3:

1. B2's one driver must pick ONE order, and picking it is a real decision, not a refactor.
   Default: order B (`ir_link` last), because it is what the shipped wasm path and the
   verified v1 trust spine already use, and because `ir_link` after mono sees the
   monomorphized call graph rather than a pre-mono one.
2. Before B3 migrates the native path from A to B, build the corpus BOTH ways and diff. If
   any output moves, that difference is a finding with its own issue and contract question —
   it is exactly the class `spec/wasm_cross` cannot see, because both legs would move
   together once they share a driver.
3. B5's structural gate should pin the ORDER, not just the count of call sites. A gate that
   only forbids a second spelling would have been satisfied by today's tree if the two
   spellings had been in one file.

## Notes

- Started while Unit 0.42's B5 is calendar-bound: the green streak needs 2 consecutive
  nightly runs, which no amount of work today can compress. 0.42 stays open and its nightly
  is checked each iteration; 0.43 runs in the gap rather than idling.
- B1 was executed during a 30-minute local fuzz campaign, when rebuilding the compiler would
  have violated the one-binary rule. Read-only inventory work is exactly what that window is
  good for.

## Unit completion

- [ ] Every Bolt done with evidence
- [ ] The evidence satisfies the plan's done-criteria (state which evidence maps to which criterion)
- [ ] Release v0.43.0 (ordinary minor — automatic)

## Retrospective (Try)

(written when the Unit closes)
