<!-- description: Kill continuation duplication via bind-position joins, and make the proven checkers linear in witness size -->
# v1 join completeness + linear checkers

Status: **Track C LANDED through C1/C2/C3/C6 (2026-07-28); Track J and C4/C5
open.** Three independent code surveys done (join machinery map, duplication
inventory, cert grammar + checker complexity). Born from the 2026-07-27
trust-spine incident.

Measured C-track results (local M-series, full corpus):
- incident-class 231KB names witness: **>10min (never finished) → 7–9s**
  (sorted merge path; the unsorted FALLBACK stays quadratic by design — the
  64KB per-line gate keeps that class out of CI);
- kernel oracle: **254s → 74s**; corpus-wall.sh total: **5m20s → 2m16s**;
- witness bytes corpus-wide: names.cert −30% (1,351KB → 939KB), max line
  −33%, caps_graph −24% (sort+dedup, set-semantics-sound);
- proof spine + coqchk + claim-drift + gate.sh + `cargo test -p almide-mir`
  (597) all green; `merge_subset_sound` / `subset_check_fast_sound` added to
  the axiom ledger, kernel-checked.

## The incident that named the problem

`457273df` added `fan_any_early_winner.almd` (8 sequential `fan.any` in one
`main`). The fan inliner duplicates the match's ok body — the function's whole
remaining continuation — once per chain level, and sequential calls COMPOUND:
~1,700 copies of the tail, 6,820 heap objects, a **231KB names witness** (corpus
median 88B). The proven checkers are superlinear in a single witness line, so
[names] went 13s → 53min and the kernel oracle parked past 4h; every subsequent
push died CANCELLED (never red) for a full day. Stopgaps landed in `30ea58de`
(fixture split one-case-per-function, `MAX_WITNESS_LINE=65536` gate in
corpus-wall.sh, `timeout-minutes: 100`). This arc removes the two root causes:

1. **The lowering duplicates continuations** — 8 distinct rewrites do it (worst
   measured: ~3.5×/chained call), because a heap branch bound to a let had no
   join story. Two of the duplicators run OUTSIDE the `MAX_DESUGARED_NODES`
   fixpoint and are UNCAPPED.
2. **The proven checkers are superlinear in witness bytes** — Θ(n²) parsing,
   Θ(k²·V̄) subset checks over unary nats, and one genuinely exponential
   reachability fold — so a single large-but-valid witness kills CI.

Every join-free re-encoding was measured before concluding: arm-inlining
~2–3×/call, `let $r = <chain>; match $r` ~3.5×/call (via
`desugar_let_bound_heap_branch`), guarded var-reassign statements ~2×/level
(via `desugar_unit_if_heap_reassign` feeding the same tail-duplication). The
duplication cannot be desugared away; it has to be ADMITTED away.

## The key finding — the join already exists; the wall is admission surface

Three facts from the surveys, all load-bearing:

- **The cert model already supports joins.** `Op::IfThen{cond, dst}` is a
  value-producing merge; per-arm `im` balance is proven (brick 5a,
  `check_bc_unroll_sound`, "Closed under the global context"). Better:
  `certificate_b.rs` already credits a RELEASED merge dst one `i`
  (`ownership_certificate_released_merge_dsts` / `merge_dst_i_credits`), so
  "bind the merge, drop it at scope end" is certifiable TODAY as `im`/`id`.
- **Bind-position joins already work for variant types.** `lower_bind_heap_if`
  admits `is_variant_ty(ty)` (Option/Result binds join, get scope-tracked); the
  wall message "the merged result has no sound scope-end drop in the flat
  certificate" is stale for exactly the machinery that landed after it was
  written. String/List binds are walled by ADMISSION, not by the cert model.
- **The renderers are ahead of the MIR.** wasm renders `IfThen dst` as
  `(if (result i32/i64) …)` and already has a value-producing N-way
  `br_table` join (`render_wasm_switch.rs`, a pure render rewrite); native has
  the if-as-value backpatch join. Nothing below the lowering needs to change.

So the arc is NOT "introduce join points" — it is "widen bind-position join
admission until each duplicating desugar's trigger set is empty, then delete
it", each step corpus-ratcheted.

## Constraints (violating any of these is a design bug)

- **`mir == ir` / desugar-before-both.** `desugar_heap_branches` is shared by
  `lower_body_into`, the `count_ir_calls` caps gate, and pre-TCO. Any trigger
  narrowing must apply identically at all three call sites.
- **TCO consumes tail-duplication.** `mod_c.rs` pre-TCO relies on the
  continuation (including the recursive call) being pushed into arms, yielding
  branched recursion `tco_collect` handles. Until J5, the duplication path must
  stay for continuations containing a self-recursive tail call.
- **CBranch is binary with FLAT arm bodies.** A nested region delimiter in an
  unbalanced arm poisons to the always-rejecting `{i|}` (by design). Join
  widening must keep each object's per-arm events flat and balanced, or defer.
  (Also: `parse_bc` treats a second `|` inside `{}` idempotently — a forged
  `{A|B|C}` parses as `CBranch A (B++C)`. The emitter never produces it; add a
  hygiene rejection when touching the parser anyway.)
- **No new axioms.** `Print Assumptions` = "Closed under the global context" is
  claim-drift-gated. `PrimString` (O(1) string primitives) imports genuine
  `Axiom`s from `PrimStringAxioms.v` — REJECTED for this arc, recorded here so
  nobody re-derives it as a quick win.
- **Adversarial pass on every cert-surface change** (the #49 / elided-call
  precedent): ≥3 independent agents try to construct accept-but-unsafe before
  any J-stage commit that touches certificate generation.

## Track J — join admission (kill the duplication)

**J0 — safety bricks (no behavior change).** Cap the two UNCAPPED duplicators
(`desugar_tuple_empty_list_match`, `desugar_list_pattern_match` — both run
outside the 200k fixpoint cap; same class as the incident). Add the `parse_bc`
second-bar hygiene rejection. Cheap, immediate.

**J1 — bind-position join for non-variant heap types.** Lift the
`lower_bind_heap_if` / `lower_bind_heap_match` wall for String/List/typed
merges using the released-merge-dst credit machinery (the variant-ty precedent,
generalized). The bound merge joins as `IfThen dst`, gets scope-tracked, drops
at scope end → cert line `im`/`id`. Adversarial pass REQUIRED. Gate: corpus
walls stay 0; ownership count may shift (arms go per-arm-balanced) — sound as
long as ACCEPT holds.

**J2 — narrow `desugar_let_bound_heap_branch`.** Where J1 lowers the bind, the
desugar must decline (else it still fires first in the fixpoint and duplicates
anyway — measured on the fan bind-form). TCO carve-out: keep duplicating when
the continuation contains a self-recursive tail call. Ratchet: a
duplication-fire counter over the corpus, checked in like the walled-real
baseline — the count may only SHRINK.

**J3 — fan.any linear re-land.** With J1+J2, the first-Ok chain VALUE
(`match t0 { ok($x) => ok($x), err(_) => <next> }`, innermost
`err("fan.any: all candidates failed")`) + ONE bind + the ORIGINAL match
becomes the linear lowering; `inline_match_over_any` dies, and #900's
per-level fresh-binder machinery dies with it (one binder, bound once).
Re-join the split fixture cases into one function as the regression proof —
the witness-size gate is the ratchet that it STAYS linear.

**J4 — migrate or bound the remaining duplicators.** In inventory order of
blast radius: `desugar_stmt_control_unwrap` (continuation into every arm tail,
uncapped node-wise), `group_option_result_arms` catch-all copies,
`desugar_tuple_variant_match(_deep)` fall-through rows,
`desugar_unit_if_heap_reassign` (retarget: with J1 it can SSA-ify into a
JOINING bind instead of a duplicating one — the shape it manufactures becomes
linear for free). Each: either the join form covers it or it gets a local cap
+ decline-to-wall.

**J5 — TCO on joins, then delete.** Teach `tco_collect` the join form (or
re-derive branched recursion from joins). Then delete the duplication paths and
`MAX_DESUGARED_NODES` — the endgame is a lowering with NO continuation copy,
where the 200k cap is dead code because nothing can approach it.

## Track C — linear checkers (independent, start immediately)

Measured composition of the 231KB wall (n = bytes, k = ids, V̄ = mean id
value, F = functions): Θ(n²) `split_bar` accumulator-append (both backends) +
Θ(n²) extracted `String.sub` destructor (binary only) + Θ(k²) `pnats`
`acc ++ [n]` + Θ(k·V̄) unary-nat digit accumulation + Θ(k²·V̄)
`subset_check`. And `check_prog_cert`'s `reaches` has NO memoization —
Θ(F·b^F) worst case, the sharpest latent cliff in the spine (currently masked
by tiny per-file graphs).

**C1 — emitter sort + dedup (zero proof change). DONE 2026-07-28.**
`subset_prop` is set-membership; `certificate.rs` itself documents "duplicates
are harmless". `sorted_dedup_ids` now serializes `name_witness_string` /
`cap_witness_string` / `transitive_cap_witness_string` /
`program_cap_graph_witness` (callee edges deduped too — they multiply the
un-memoized `reaches` fold). Corpus: names.cert −30%, max line −33%.

**C2 — Θ(n) parsing in Gallina. DONE 2026-07-28.** `pnats` / `split_bar` /
`split_semi` keep their names and signatures but walk `list ascii`
(cons-accumulated, one `rev_append` at the boundary;
`list_ascii_of_string` extracts to one O(n) `List.init`). All four parse
quadratics gone in BOTH backends. Behavior pinned by the in-file Examples +
the 3-way corpus gate.

**C3 — sorted fast path. DONE 2026-07-28.** `sortedb` + `drop_lt` +
`merge_subset` in Subset.v; `subset_check_fast` dispatches, `check_names` /
`check_caps` route through it. The merge's soundness
(`merge_subset_sound`) needs NO sortedness hypothesis — accepted elements are
literally found in a suffix of `sup` — so the trust theorem is unconditional
and sortedness only gates completeness (falls back to `subset_check`; every
pre-existing witness verifies bit-identically). `CapabilityReach.prog_within`
deliberately stays on `subset_check` (reach lists are unsorted until C5).

**C4 — id type `nat` → `N`.** Removes unary-nat digit/eqb costs in both
backends. Witness bytes unchanged; proof-side refactor of
`NameWitness`/`CapWitness`/`Fn` + parsers.

**C5 — `reaches` memoization.** Visited-set BFS (`MSetPositive` — binary
`positive` keys, vm_compute-friendly, already in the stdlib closure) replaces
the fuel-bounded re-expansion. Kills the exponential cliff BEFORE any corpus
file grows a dense call graph.

**C6 — kernel-oracle hygiene. DONE 2026-07-28.** Goals are chunked (500
witnesses per names/caps/tcaps goal, 4,000 ownership LINES per goal — sound:
`check_bc` folds per line, `forallb` distributes over ++), bounding the peak
term and letting coqc GC between goals. `MAX_CERT_TOTAL=8388608` aggregate
gate added beside the per-line gate (current full-corpus total ~1.1MB).
Kernel oracle: 254s → 74s.

## Order and gates

C1 → C6 → J0 are independent quick wins (C1+C6 shrink the kernel oracle's
term size immediately; J0 closes the uncapped-duplicator class). Then C2/C3 as
one proof PR. J1 is the pivot — adversarial pass, then J2's ratchet makes the
progress monotone and visible. J3 is the payoff demo (the incident shape,
linear). C4/C5 and J4/J5 ride behind.

C-track remainder, deferred with reasons (the honesty precedent): **C4**
(nat→N) removes the residual unary-nat arithmetic — the 7–9s still paid on an
incident-CLASS witness — but that class cannot reach CI (64KB gate) and the
refactor spans CallModes/CapabilityReach proof surfaces; do it as its own
focused PR. **C5** (`reaches` memoization) closes a real exponential cliff
that is currently masked (tiny per-file graphs) and, if hit, fails RED by the
100-min workflow timeout rather than zombie-ing — urgent the moment caps
graphs densify, not before.

Every stage gates on: `make verify-trust` green at ~17 min (the incident
taught us to watch the CLOCK, not just the verdict — a green run 3× slower is
a regression), corpus walls = 0, the walled-real and duplication-fire ratchets
only shrinking, and `almide test` + cross-target parity untouched.

## Honesty

- Nothing here changes what is PROVEN about programs — it changes which
  programs the lowering ADMITS (more) and how fast the checkers run (much).
  The trusted base does not grow; C-track explicitly refuses the one lever
  (`PrimString`) that would grow it.
- J1 touches certificate generation — the exact surface where the historical
  accept-but-unsafe gaps lived. That is why the adversarial pass is a gate,
  not a suggestion, and why J1 lands alone, not fused with J2.
- The measured multipliers (2–3.5×/call) and the incident numbers are from
  2026-07-27/28 sessions; re-measure before relying on them after the
  almide-mir file splits settle.
