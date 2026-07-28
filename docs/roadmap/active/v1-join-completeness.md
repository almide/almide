<!-- description: Kill continuation duplication via bind-position joins, and make the proven checkers linear in witness size -->
# v1 join completeness + linear checkers

Status: **Track C landed through C1/C2/C3/C6; Track J landed through
J0/J2a/J3 (2026-07-28). J1 (non-variant widening), J4, J5 and C4/C5 open.**
Three independent code surveys done (join machinery map, duplication
inventory, cert grammar + checker complexity). Born from the 2026-07-27
trust-spine incident.

Measured J results: the incident fixture's 8-chain `main` is RESTORED to one
function and lowers LINEARLY — names witness 231,114B → **4,002B** (~58×),
growth +480B per chained fan.any (was ×3.5 multiplicative). All fan fixtures
byte-identical native/wasm (the #900 pins), corpus walls 0, ratchet clean,
kernel oracle 74s, `cargo test -p almide-mir` 597 green.

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

**J0 — safety bricks (no behavior change). CAPS DONE 2026-07-28.**
`desugar_tuple_empty_list_match` and `desugar_list_pattern_match` (both run
outside the 200k fixpoint cap — the two UNCAPPED duplicators) now carry a
growth cap (+50k nodes over the input; past it the rewrite is DISCARDED and
the match walls honestly, the desugar_heap_branches precedent). The `parse_bc`
second-bar hygiene rejection remains open (cosmetic — the emitter never
produces it and the soundness theorem quantifies over the parse result).

**J1 — bind-position join for non-variant heap types.** Lift the
`lower_bind_heap_if` / `lower_bind_heap_match` wall for String/List/typed
merges using the released-merge-dst credit machinery (the variant-ty precedent,
generalized). The bound merge joins as `IfThen dst`, gets scope-tracked, drops
at scope end → cert line `im`/`id`. Adversarial pass REQUIRED. Gate: corpus
walls stay 0; ownership count may shift (arms go per-arm-balanced) — sound as
long as ACCEPT holds. PARTIAL SLICE LANDED 2026-07-28 with J3: the VARIANT
match-bind join — `lower_bind_heap_match` gained the variant path its `if`
twin already had (the same match-VALUE routers the tail position trusts:
custom-variant / variant-value / Result / Option merges), bind + scope-track +
`seed_variant_param`. No new cert surface (existing tail-trusted routers).

> **ATTEMPT 1 REFUTED (2026-07-28) — do not retry without the two fixes below.**
> A full widening was implemented and killed by the mandatory adversarial
> pass: 2 of 3 independent auditors returned REFUTED with distinct defects, and
> 5 lowering unit tests pinning the duplication route failed. The attempt was
> exactly: drop the `is_variant_ty` gate at both bind sites (`lower_bind_heap_if`
> ran the general merge join for any heap type; `lower_bind_heap_match` gained
> `try_lower_list_match_value` + a `desugar_match_to_if` → `try_lower_heap_result_if`
> fallback), plus a `bind_join_lowerable` predicate in `desugar_branch_b.rs`
> admitting arms drawn from {LitStr, heap Var, ConcatStr, nested such if} under a
> scalar cond, wired into `desugar_let_bound_heap_branch`'s trigger and its
> `rest_branch_binds` counter so those binds stop being duplicated. Nothing of it
> was committed; re-derive from this paragraph plus the two fixes below.
>
> **Defect 1 — live-variable clobber (LATENT, exposed by the rerouting).**
> Admitting a bare-Var else arm let non-shadow binds (`let b = …; let j = if c
> then "A" else b; b + "/" + j`) reach the identity-else accumulator slot fold
> (`try_lower_line_cond_acc`), whose guard checks only that the else arm is an
> owned scope-tracked Var — never that the binder SHADOWS it. The fold rebinds
> `b`'s slot in place, so later reads of `b` return the wrong block: native
> `sB/A` vs wasm `A/A`, exit 0 on both, ownership cert ACCEPT. Verified
> UNREACHABLE on develop (the duplication desugar intercepts first), so nothing
> shipped is broken — but it is a trap under ANY future rerouting. Now carries
> a routing-invariant comment at the fold site and a standing cross-target pin
> (`spec/wasm_cross/heap_result_if_bind_chain.almd::clobber`, two links — at
> four or more the dense branch-lift outlines each bind and hides it).
> FIXED 2026-07-28 (the J1 prerequisite): the fold now carries a LAST-READER
> gate. `LowerCtx::var_read_counts` counts every variable's reads over the
> whole final body (computed beside `cell_vars`, where the body is whole —
> the statement-at-a-time lowering cannot see later reads on its own), and
> `try_lower_line_cond_acc` folds only when the accumulator's whole-body read
> count equals its read count inside THIS bind's value, i.e. this bind is its
> last reader. That is exactly what the source-level shadow rebind provides,
> so stacked shadow accumulators (porta `serialize_opts`) keep folding in
> place; any other binder declines to the ordinary route. The precondition is
> now CHECKED rather than maintained by the accident that
> `desugar_let_bound_heap_branch` intercepts these binds first — which is
> precisely what J1 changes.
>
> **Defect 2 — merge-dst drop class missing for nested heap types (NEW).**
> The widening pushes the merge dst to `live_heap_handles` but registers no
> drop CLASS; only variants got `seed_variant_param`. A `List[String]` merge
> therefore gets a flat `Op::Drop`, freeing the outer block and LEAKING every
> element String — reproduced independently by two auditors (native bounded
> 105MB at N=2M vs wasm 2.88GB unbounded; traps under any fixed
> `--max-memory-size`). The ownership cert ACCEPTs because its `d` cannot
> distinguish a flat from a recursive drop. Pre-change this shape WALLED, so
> the widening converted an honest refusal into a certified leak.
> FIX: register the merge dst's drop class by result type at both join sites
> (mirror `seed_call_module_heap_drop_route` / `register_owned_heap_eq_drop`),
> or restrict the widening to flat-drop-exact types (`String`) until that
> lands.
> NOT SEPARABLE — land it WITH J1, not before (measured 2026-07-28). The
> existing variant join does NOT have this gap: a `Result[List[String],
> String]` merge bind churns 100k times inside a 4MB wasm cap with the right
> value, because `seed_variant_param` already routes its recursive drop; and
> a deeper `Result[List[List[String]], String]` merge WALLS rather than
> lowering. So there is no shape today whose drop class is both missing and
> reachable — registering it now would be untestable dead code, and the
> honest sequencing is to add the call in the same change that admits the
> types needing it, verified by those types' churn probes.
>
> **Process note (kept deliberately):** corpus-wall was GREEN, the walled-real
> ratchet was CLEAN, every pre-existing cross-target fixture passed, and the
> proven checker ACCEPTed both defects. Only the adversarial pass caught them.
> That is the whole argument for keeping it a gate rather than a suggestion.

**J2 — narrow `desugar_let_bound_heap_branch`. J2a DONE 2026-07-28.**
`is_first_ok_chain` (co-located with the chain builder in desugar_fan.rs)
recognizes exactly the shape `first_ok_chain` emits; the tail-duplication's
trigger AND its rest-counter both exclude it, so the chain bind reaches the
variant bind-position join instead of being duplicated (measured ×3.5/call
before the decline). The predicate is deliberately strict — nothing
hand-written trips it — and widens only in step with J1's admission. The
general narrowing (a shared join-admissibility predicate over ordinary
branch binds) still awaits J1; the TCO carve-out (keep duplicating when the
continuation contains a self-recursive tail call) applies to THAT step, not
to the chain shape (a fan chain never contains the outer function's
recursion in its arms — the arms are `ok($x)`/nested-chain only). Ratchet: a
duplication-fire counter over the corpus, checked in like the walled-real
baseline — the count may only SHRINK.

**J3 — fan.any linear re-land. DONE 2026-07-28.** `inline_match_over_any` and
the #900 per-level fresh-binder machinery are DELETED; the match-over-any
shape now folds through `first_ok_chain` (shared with the value position) +
ONE bind + the ORIGINAL match, and the split fixture is re-joined into the
original 8-chain `main` as the standing linearity regression — the
witness-size gate turns any regression back to duplication into a fast red
naming the function. Witness: 231,114B → 4,002B; growth +480B per chained
call.

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
