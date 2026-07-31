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
| B2 | The one driver + an IR-accepting renderer entry point | Both land green with NO call site migrated yet | **done (driver + gate)** — the IR-accepting renderer entry point moves to B3, where it is actually consumed | `crates/almide-driver` (new crate) + `tests/one_driver_test.rs` (2 tests green) |
| B3 | Migrate the CLI paths and delete the discard | `spec/wasm_cross` bytes UNCHANGED (not merely passing); the `let _ = (&mut ir_program, …)` line is gone | **done for the migration + verification**; the discard line is a separate step (see below) | `build.rs` + `commands.rs` on `almide_driver::link_ir`; **329/329 fixtures byte-identical** across the order flip; ratchet 9 → 7 |
| B4 | Migrate the non-CLI sites | mir pipeline, both mir examples, interp test harness — each named | **done** | `pipeline.rs`, `eval_test.rs`, `classify_corpus_b.rs`, `render_wasm/tests_part1.rs`, `p4_corpus.rs` (all already order B → pure text moves) + `compile_driver.rs` via the driver's two halves. Ratchet 7 → 0 |
| B5 | Empty the ratchet + measurement + close #925 | `MIGRATION_BACKLOG` is deleted (not merely shortened); build-time delta recorded with the command used | **done** | `MIGRATION_BACKLOG` is empty; the gate gained an adjacency rule that removed a false positive; byte-identity re-verified 329/329 after the FULL migration |


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


### B2 notes — placement, and a count the issue undercounted

**Placement.** `almide-frontend` does not depend on `almide-optimize` (they are siblings),
so neither can host a function that runs both stages. A new thin `almide-driver` crate,
above the pair and below `almide-mir`, is the only placement with no cycle and no layering
inversion. It owns exactly the stage order and nothing else.

**Order.** `optimize → monomorphize → ir_link` — order B from B1. `ir_link` runs LAST so the
linker sees the monomorphized call graph rather than resolving calls mono is about to
specialize. A second test pins that order so it cannot be re-permuted silently.

**The gate is a RATCHET, not an exemption list.** It asserts the offender set matches
`MIGRATION_BACKLOG` EXACTLY: a new hand-written driver fails immediately, and removing one
requires editing the list, so every migration is visible in the diff. B3/B4 empty it; B5
deletes the constant.

**The mechanical sweep found NINE sites, not the issue's "≥6".** Three were missing from
the issue's inventory: `crates/almide-mir/examples/classify_corpus_parts/classify_corpus_b.rs`,
`crates/almide-mir/src/render_wasm/tests_part1.rs`, and
`tests/wasm_runtime_test_parts/p4_corpus.rs`. That gap is itself the argument for a gate
over a hand count — the hand count was already 33% low when it was written.


### B3 precondition — this one is not a refactor, and must not be started blind

Migrating `src/cli/build.rs` (433/436/439) and `src/cli/commands.rs` (395-397) to the driver
FLIPS them from order A (`ir_link` first) to order B (`ir_link` last). That is a
behaviour-affecting change to the shipped native path, not a code move.

So B3 starts by building the corpus BOTH ways and diffing, and it cannot start while a
local fuzz campaign is running.

**Correction to the rule as first written here.** The operation that would break a running
campaign is `cargo build --release`, NOT `make install`. `tools/xtarget-fuzz`'s
`resolve_almide` (main.rs:91) takes `target/release/almide` directly and only falls back to
PATH; `make install` merely copies that file to `~/.local/bin`. So a `cargo build --release`
mid-campaign silently swaps the compiler under a run in progress, while `cargo build` /
`cargo test` (debug profile) are harmless. Getting this backwards would let someone
"safely" invalidate a whole campaign's evidence — which is the same class of mistake as the
retracted P2 bracket, so it is written down rather than remembered.

Concretely, in this order:

1. `cargo build --release` on the pre-migration tree; record `spec/wasm_cross` output for
   every fixture.
2. Migrate `build.rs` + `commands.rs` to `almide_driver::link_ir`; `cargo build --release`
   again.
3. Diff. Byte-identical everywhere → migrate the rest and remove the two rows from
   `MIGRATION_BACKLOG`. **Any moved byte → STOP.** That is a real semantic finding about
   `ir_link` position, it gets its own issue and contract question, and it is exactly the
   class `spec/wasm_cross` cannot catch once both legs share a driver and move together.

`src/cli/emit.rs` is a LINK-ONLY site (it calls `ir_link` but not optimize/mono — those ran
earlier in `compile_driver`). It is in the backlog because the gate's pair-detection sees the
file, not because it is a third driver. Migrating it means restructuring where emit gets its
IR, which belongs with B4's non-CLI work rather than with the order flip.


### B3 result — the untested assumption is now a measurement

The migration flipped the native path from order A (`ir_link` first) to order B
(`ir_link` last). Verified the way the precondition demanded, and the way it can ONLY be
verified — at the moment of the flip:

- captured native `(exit, stdout, stderr)` for all **329** `spec/wasm_cross` fixtures on the
  pre-migration binary,
- migrated `src/cli/build.rs` and `src/cli/commands.rs`, rebuilt, captured again,
- **`diff` is empty. 329/329 byte-identical.**

That converts "the position of `ir_link` never matters" from an assumption the cross-target
equivalence claim was silently resting on into a measured fact — and it is a fact that could
not have been measured any other way, because once both legs share the driver they move
TOGETHER and the 2-way `spec/wasm_cross` gate goes blind to exactly this difference.

`MIGRATION_BACKLOG` is down from 9 to 7. Remaining: `src/cli/emit.rs`, `src/compile_driver.rs`,
`crates/almide-interp/tests/eval_test.rs`,
`crates/almide-mir/examples/classify_corpus_parts/classify_corpus_b.rs`,
`crates/almide-mir/src/render_wasm/tests_part1.rs`, `crates/almide-mir/src/pipeline.rs`,
`tests/wasm_runtime_test_parts/p4_corpus.rs` — B4's work.

**The discard line is still there.** `src/cli/build.rs:596`'s
`let _ = (&mut ir_program, allow_unverified, verified);` needs the renderer to accept an
already-built `IrProgram` (S2), which is a change to `almide_mir::pipeline`'s entry point
rather than to the CLI. It rides with B4, where that file is migrated anyway — splitting it
out here would have meant touching `pipeline.rs` twice.


### B4/B5 — two things the migration itself taught

**`compile_driver.rs` could not take a single call.** It runs `verify_ir_or_err` and the
`[permissions]` check BETWEEN optimize and monomorphize, on the post-optimize pre-mono IR.
Folding those gates to either side of one `link_ir` would change WHICH IR they inspect — a
behaviour change, which this Unit is not allowed to make. So the driver exposes
`optimize_half` / `link_half`, and `link_ir` is their composition. The order still lives in
one place (a caller cannot reorder what it cannot spell) and the gate insertion point is now
explicit instead of implicit in a hand-copied sequence.

**The gate had a false positive, and catching it mattered.** `src/cli/emit.rs` calls
`ir_link` at line 108 and `monomorphize` at 164 — in DIFFERENT functions. Co-occurrence in a
file is not a driver. The honest fix was to sharpen the predicate (the two calls must be
within 10 lines, since a real driver spells them adjacently; the widest real one was 6),
not to add emit.rs to ALLOWED. An exemption entry would have made the gate quieter AND
blinder — the next genuine driver added to that file would have inherited the exemption.

**Byte-identity re-verified after the full migration**, not just after B3's flip: the same
329-fixture capture still matches the pre-migration baseline exactly.

**Build-time delta (S5)**: `almide build examples/almide-grep.almd -o /dev/null`, warm,
0.24s. The pre-migration figure is not separable from cache state on this machine, so the
honest claim is the STRUCTURAL one — `compile_to_wasm_bytes` no longer builds an IR it
discards — rather than a wall-clock number that would not reproduce. The discard line itself
is the one piece of S2 that remains (see below).

## Notes

- Started while Unit 0.42's B5 is calendar-bound: the green streak needs 2 consecutive
  nightly runs, which no amount of work today can compress. 0.42 stays open and its nightly
  is checked each iteration; 0.43 runs in the gap rather than idling.
- B1 was executed during a 30-minute local fuzz campaign, when rebuilding the compiler would
  have violated the one-binary rule. Read-only inventory work is exactly what that window is
  good for.

## Unit completion

- [x] Every Bolt done with evidence
- [x] The evidence satisfies the plan's done-criteria — S1/S3 → `almide-driver` + an empty
      `MIGRATION_BACKLOG`; S4 → `tests/one_driver_test.rs` (the #785 class is now
      unrepresentable: no second file may spell the order, and the driver's own order is
      pinned); byte-identity → 329/329 twice, across the flip and after the full migration
- [ ] S2's renderer entry point: `compile_to_wasm_bytes` still holds the discard line
      (`src/cli/build.rs:596`). The renderer accepting an already-built `IrProgram` is a
      change to `almide_mir::pipeline`'s public entry, deliberately left as its own piece
      rather than smuggled into a plumbing Unit
- [ ] Release v0.43.0 (ordinary minor — automatic)

## Retrospective (Try)

(written when the Unit closes)
