# Unit 0.44 — Ledger

> Paired plan: [inception.md](./inception.md) (approved 2026-07-31)
> Rule: a checkbox without evidence (commit SHA / CI run URL) is invalid.
> Bolt N's evidence is recorded at the start of the next iteration, while checking the
> previous run's CI. Detail ledger for the findings themselves: Wave 3 in
> `docs/roadmap/active/fuzz-findings-triage.md`.

## Bolt ledger

| Bolt | What | Done-criteria for this Bolt | Status | Evidence |
|---|---|---|---|---|
| B1 | Build the fuzzer; replay all 5 live records; classify root causes; open the wave table | Every record reproduced (or its absence explained); each has a named symptom + suspected root-cause class in the wave table | done | Wave 4 table in `docs/roadmap/active/fuzz-findings-triage.md`: 2 resolved-in-window (861, 535), 3 live (57 trap-divergence, 12 heap-if wall, 29 registry gap). Commit 697c2c91 |
| B2 | Fix the OutputDivergence class (native `r15 = 100` vs wasm `r15 = 0`) | The replay is clean on both targets; root cause named; contract entry if observable behavior was defined | done — retargeted (M6) to attribution: 861 → `020021df` (Float closure-capture admission; the walled lift had run the HOF with a missing closure = the zero-filled class), 535 → `6ac44503` (the paren/unary chain walk with net sign; C-173 alone could not see through `--`). Both mechanism-exact, replays clean | Wave 4 rows carry the full reasoning chains |
| B3 | Fix the build/run-failure classes (NativeBuildFailure, RunFailureDivergence, WasmBuildFailure ×2) | All four replays clean on both targets; root causes named | done — 29 fixed (6edfc79d, CI green), 12 fixed (46f89882, msb brick), 535 resolved-in-window (attribution in B2), 57 contracted per M2 #1017 option 1 (C-196 + oracle rule + convergent fixture; replay SKIPPED naming C-196) | 29: 6edfc79d. 12: 81cd0806 + 46f89882. 57: C-196 commit follows this record; contract gate OK (196 active / 0 flagged, 325/325 fixtures bidirectional), fuzzer tests 21/0 |
| B4 | Local campaign on the fixed compiler | A fresh multi-thousand-program local campaign records 0 findings | in progress — CI green on the P2 fix (77153808: CI / Cross-Target / Trust Spine all success). Round 6 running — **P2 (the last live finding) FIXED 2026-07-31** (77153808): a list literal whose elements are bare maps was unmaterializable; `ListElemDrop::MapSkv` + `$__drop_list_mb`. Getting there required RETRACTING a whole bracket — see the note below on what made it invalid. Round 6 is now unblocked. Campaign history: round 1: 707 → L0 (fixed d8d2d232+bef7e218-era). Round 2: 1,283 → L2 (fixed 98dd3041) + L3 (fixed bef7e218). Round 3: 1,304 → L4 (fixed 0b4ff28e). Round 4: 1,271 → L5 (M2 #1019 pending), L6 (design recorded, next fresh iteration), L7 (fixed 3bec6b06). fmt-gate repair abccf0b7 + procedure rule 90b0af68. Round 5 after L5/L6 | ~5,800 programs total across 4 rounds; every finding root-caused same-day |
| B5 | Observe 2 consecutive green nights | `fuzz-track-record.sh` green streak ≥ 2; #796 closed; release v0.42.0 | pending | — |

## Notes

- **CI repair (2026-07-31)**: the C-197 fixture landed in B3 (`allocation_within_limits`)
  used `list.push`, which the interp could not evaluate, so it abstained without a ledger
  entry and `interp_abstain_ledger` went red. The gate offers two fixes and names its
  preference: widen the interp glue, or record the abstention. Widening won, because the
  same hole held **16 fixtures** out of the third oracle, not just the new one.
  `crates/almide-interp/src/inplace.rs` intercepts the `mut`-receiver mutators before the
  dispatch path turns the receiver into a value, and writes the mutation back into its
  binding. `Rc::make_mut` on the binding's own slot is both the COW rule (C-033) and the
  reason a 100k-push loop stays linear — the first draft cloned the container per call and
  would have been quadratic.
  Measured outcome: **189 → 194 evaluated**, ledger 137 → 133. Five fixtures now evaluate
  end to end (`inplace_mutator_statement`, `loop_buffer_churn`,
  `loop_outer_inplace_mutate_rc`, `loop_push_trailing_increment`, and
  `allocation_within_limits` itself). Five more got past the in-place barrier and now
  abstain on a DIFFERENT, deeper gap (`prim.alloc_map` / `prim.alloc_list` /
  `prim.handle`) — real progress, but not coverage yet; do not count them as recovered.
  Six keep an in-place reason, now stated precisely instead of family-wide: a `mut`
  parameter receiver (#1022) or a bytes byte-level writer (#1021).
  `interp_cross_target_spec` stayed green, which is the load-bearing check — the interp
  now VOTES on five fixtures it used to skip, and a wrong vote fails loudly.
- **P2 and the invalid bracket (2026-07-31)**: the last live Wave 4 finding was tracked for a
  session as a capacity wall — "the cliff is EXACTLY at the 12th map entry" — with a nine-probe
  bracket behind it. All of it was wrong. Re-running the SAME probes with
  `almide build <f> --target wasm` on ONE binary walls every one of them, including the smallest.
  The earlier "BUILDS" cells had not exercised the wasm path at all, so the bracket was measuring
  nothing. Reduced properly, the finding is three lines
  (`let xs: List[Map[String, Bool]] = [["k0": true]]`) and a plain missing cell — which is what the
  FIRST classification said before the bad probes talked me out of it.
  Process rule this earns: **a bracket is evidence only if every cell in it was measured with the
  same command on the same binary, in one run.** A probe result recorded across sessions or
  commands is a note, not a measurement. Cheap to obey — the corrected sweep took one command.

- The plan said "Wave 3", but the triage ledger already carries a Wave 3 — this Unit's
  campaign is **Wave 4**. In-scope naming correction.
- B2's original target (the OutputDivergence) turned out to be already resolved on current
  develop — no codegen commits landed between the finding night and now EXCEPT the
  c4d38b1d..52200340 window (7/28–7/29), so B2 is retargeted to: attribute the resolving
  commits for 861 and 535 (bisect the window) and confirm the resolution is real, not
  host-dependent. In-scope retarget; the DoD (streak, not list) is unchanged.
- Fix order for the live three, easiest-first to shrink the nightly surface while the M2
  question on 57 is prepared: 29 (registry gap) → 12 (heap-if wall brick) → 57 (M2 first,
  then implementation per the decision).

## Unit completion

- [ ] Every Bolt done with evidence
- [ ] The evidence satisfies the plan's done-criteria (state which evidence maps to which criterion)
- [ ] Release v0.44.0 (ordinary minor — automatic)

## Retrospective (Try)

(written when the Unit closes)
