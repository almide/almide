# Unit 0.44 — Ledger

> Paired plan: [inception.md](./inception.md) — approved 2026-07-31 under the standing
> full-authority directive.
> Rule: a checkbox without evidence (commit SHA / CI run URL) is invalid.

## Bolt ledger

| Bolt | What | Done-criteria for this Bolt | Status | Evidence |
|---|---|---|---|---|
| B1 | Decide the stance, write it up, correct SPEC | The page names which existing promise is kept and which is dropped; SPEC's false claims are gone | done | `docs/roadmap/active/concurrency-stance.md`; SPEC §9.8/§13.1/§13.2/§13.3 corrected. #1000 closed |
| B2 | #1025 — sibling aliasing | The repro is rejected on both targets | done — and the root was NOT fan | Traced to #1027 (UFCS skipped the mut-param check); fixed in `check_call_target_builtin_ufcs`; #1025 closed with no fan-specific rule needed |
| B3 | #1024 — `fan.race` | Removed, not renamed; migrations by intent | done | E027 tombstone; C-004/C-005 updated; 6 call sites migrated; `e027-fan-race-removed` fixture. Merge 3eb47b7f |
| B4 | #1023 — cancellation | SPEC corrected; list-order Err pinned by a fixture | done | C-199 + `spec/wasm_cross/fan_block_err_list_order.almd`. Commit 409d3932 |
| B5 | #1026 — trap exit | Contracted; convergence measured, not assumed | done | C-200 + `spec/wasm_cross/fan_sibling_trap.almd`. Commit 5b2f9f7a |

## Notes — three of the four resolved in a different shape than reported

This Unit's value was mostly in refusing the reported framing.

- **#1025 was not a `fan` problem.** The hazard reproduces with no `fan` block: UFCS method
  calls skipped the `mut`-parameter check entirely, because `check_call_target_builtin_ufcs`
  handed `validate_mut_args` a list the RECEIVER was not in — and after UFCS desugaring the
  receiver IS argument 0, which for `list.push(mut xs, x)` is precisely the `mut` parameter.
  That made it the session's only WRONG-BYTES finding (native `len=2`, wasm `len=0`, on a
  program the checker accepted). Filed and fixed as #1027; #1025 then closed with no new
  rule, because the two routes to it are now both sealed — no `mut` declaration → E007, a
  `mut` declaration → the caller needs a `var` → E008 in a fan block.
- **#1024 was not a documentation bug.** Reading the IR, `desugar_fan.rs::rewrite_race_head`
  replaced `fan.race([t0, t1, …])` with t0's body and never evaluated the rest, so the
  combinator was `thunks[0]()` with a name promising a race. Removed rather than corrected.
- **#1026 refuted this Unit's own design.** The plan proposed per-arm output buffering to
  force convergence on the trap path. Measured with a 1.5s-sleeping sibling, both targets
  abort in ~0s with an empty stdout — it already converges, so only a contract was needed.
  R3's absorption ("measure before writing the fix") is what caught this.

Two process findings worth keeping:

- **A tombstone's migration target must stay a live surface.** Removing `fan.race` broke
  `tests/diagnostics/e027-fan-timeout-removed/fixed.almd`, which had migrated `fan.timeout`
  users TO `fan.race` in 0.29.0. A hint pointing at a dead surface rots silently.
- **A removal's migrations cannot be mechanical.** `fan.race(ts)` ≡ `ts[0]()`, but `fan.any`
  differs when thunk[0] fails. Mechanically rewriting a head-Err test to `fan.any` yields a
  test that passes while asserting something else. Each of the six sites was moved by what
  it actually pins.

## Unit completion

- [x] Every Bolt done with evidence
- [x] The evidence satisfies the plan's done-criteria — S1 → the stance page + #1000;
      S2 → the four closures; S3 → C-199, C-200, C-004/C-005 edits, each with its fixture
- [ ] Release v0.44.0 (ordinary minor — blocked behind 0.42 and 0.43 in ladder order)

## Retrospective (Try)

1. **Measure the reported behaviour before designing the fix.** Two of five Bolts changed
   shape once measured, and one of those would have been a large unnecessary implementation.
2. **When a report names a construct, check whether the construct is required.** #1025 named
   `fan`; the bug had nothing to do with it, and following the name would have produced a
   fan-specific rule that fixed the symptom while the wrong-bytes root stayed live.
3. **A removal is not done when the surface is gone.** Its migration targets, its tombstone
   hints, and every test that used it are part of the removal.
