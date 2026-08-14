# rot-eradication — the decay map and its reference-backed cures

**Status**: active (constitution written 2026-08-15, after the result-family
arc and the #1418 campaign; every claim below is backed by a measurement or an
incident from those sessions).
**Survey evidence**: `../almide-references/RESEARCH.md` (Surveys 1–3:
call-result layout, fresh-variable discipline, coalescing-sugar elimination,
payload-bind knowledge, early-return-in-value-position, codegen inclusion
sets — all with file:line citations into rustc/Swift/Zig/Roc/Lean/Koka/Grain
clones).

## The laws (what every surveyed compiler agrees on)

1. **Types choose layout; names choose code.** (Survey 1, 8/8.) Landed as the
   result-family arc.
2. **Fresh ids come from a monotonic context-owned allocator, never a subtree
   scan.** (Survey 2, 4/4.) Landed as the band allocator.
3. **Sugar is eliminated once, into the generic machinery; payload classes
   change the test emitted, never the path taken.** (Survey 2 — Roc's `??`
   desugar is character-identical to ours.)
4. **The per-value knowledge IS the value's type/layout, attached at birth;
   projections derive child knowledge by a total function; "untracked" does
   not exist.** (Survey 3a — Roc `Local{layout_idx}`, rustc `Place→Ty`.)
5. **Shape and ownership are separate passes.** (Survey 3a — Roc's match
   lowering is total and layout-driven; ARC decides borrow/incref later.)
6. **A diverging arm drops the merge continuation: duplicate it into ≤1
   surviving branch, else reify it as a local join point.** (Survey 3b —
   Koka UnReturn; our tail-duplication desugar is already the duplication
   half.)
7. **The codegen inclusion set is computed FROM the emitted-call artifact by
   transitive walk, never from a parallel prediction.** (Survey 3c — rustc's
   collector walks the same MIR codegen walks; Roc deletes everything the
   emitted LIR does not reach and the backend compiles the whole store.)

## The rot map → cures

### R1 — the `??` route zoo coupling web (ACTIVE ROT; #1418)
Three failure modes measured in one night: name-table nudges destabilize the
normal path (reverted experiment); the caps counter statically predicts route
helper calls (#1079 coupling); speculative attempts poisoned tracking sets
(fixed by `speculate`). **Cure (law 3): move the route BODIES into the
value-match machinery as its payload-class arms, then delete the chain, the
admission gate, the counter credits, and the `ALMIDE_QQ_DESUGAR` experiment
pass in one slice.** Instruments already landed: match-first inversion
(scalar: 214 match / 1 fallback), `ALMIDE_QQ_NO_ROUTES` deletion-readiness
probe (25 → 2: fs_metadata_family = R6's link-set class; unwrap_or_heap_payload
= a subject-classification residue the route-body migration subsumes).

### R2 — the per-value tracking side sets (#1414)
~20 ValueId-keyed sets; "untracked" as implicit state; insertions
unrewindable (the `speculate` snapshot is the bandage, not the cure); one
bind/subject asymmetry already leaked (fixed). **Cure (laws 4+5): a per-value
shape attached at value creation (the Roc `Local{layout_idx}` shape — for us,
`ValueId → ValueShape` assigned by `fresh_value`-adjacent constructors),
projections derive child shapes totally, and the DROP/ownership routing moves
to a later pass over the same shapes. `result_family` is the first total
piece; the seed-once entry point is the migration vehicle.** Deletes the
`speculate` clone cost and the wall class "match over an UNTRACKED subject"
by making the state unrepresentable.

### R3 — value-position propagation arms (#1421, WRONG-CODE)
`match r { ok(v) => v, err(_) => f()! }` in value position emits an invalid
merge (accepted-but-wrong; the guard in the `??` inversion isolates it).
**Cure (law 6): the Koka lattice — a propagating arm consumes no merge; with
≤1 surviving arm push the continuation into it (the tail-duplication
machinery we already have), else reify the continuation as a synthesized
join fn.** Lifts the `fallback_propagates` guard and closes the last scalar
route fallback. The load-bearing half of this cure — a frame-targeted
`Op::Return` so a propagating arm CAN diverge — is now its own arc:
[return-op-eradication.md](./return-op-eradication.md), which also deletes
the `!` position-desugar zoo (~9 rows in `BRANCH_PASSES`) that exists only
because the op is missing.

### R4 — the link-set divergence (env.cwd class; unfiled until now)
Which self-host bodies get compiled disagreed with the calls the lowering
emitted when a lowering path changed. **Cure (law 7): derive the self-host
inclusion set by walking the LOWERED functions' emitted call names to a
fixpoint (the artifact codegen consumes), delete any parallel prediction,
and add the render backstop: zero unresolved internal symbols = a structural
gate, not a wall message.**

### R5 — residual name registries (purity, NEVER_ERR/AUTO_WRAP, router
special cases)
Same genus as the razed family tables; currently held by tests. **Cure
(Survey 1's Swift/Lean pattern): a name-derived fact is folded into the
signature/registry ONCE at a boundary; downstream never re-derives by name.
Registry-derivation of the materialized set (the result-family arc's open
tail) is the template.** Opportunistic — each registry migrates when touched.

### R6 — the harness reports walls as "panicked"
Cost a full diagnostic detour (12 "panics" were honest walls). **Cure: the
cross-target harness distinguishes wall/build-fail/run-fail/diverge in its
report line.** Small, immediate.

### R7 — include!-splicing blinds rust-analyzer
The most safety-critical crate has no working IDE diagnostics (all-night
false E0308s / unlinked-file noise). **Cure: either real `mod` files (the
line-ceiling policy can count per-module instead of per-file) or a workspace
rust-analyzer config acknowledging the splice roots.** Tooling decision, low
risk, high review-bandwidth payoff.

## Landed (2026-08-15, the night of the constitution)

- **R1 COMPLETE** — #1418 closed. The route zoo is DELETED (+148/−1054): match
  machinery is the only `??` path; the admission gate, operand resolvers,
  counter credits, and the experiment pass are gone; three byte tests pin the
  deletion. Readiness drove 25 → 0 via `speculate`, typed seeding, subject-ANF
  at both payload widths, and the merge-ownership convention. The closure and
  fallible-carrier route BODIES moved inside the path as payload-class cases.
- **R3 first half** — the wrong-code acceptance is closed (the value-position
  scalar match DECLINES propagating arms; every known producer rides the
  `(match …)!` statement rewrite, now covering both payload classes). The
  join-point lowering that would ACCEPT those arms in value position remains
  #1421's body of work.
- **R2 seed-once milestone** — `seed_variant_value_shape` is the single typed
  entry for variant read-shape + drop-route seeding across all positions; the
  per-position drift class is closed. The shape-at-birth endgame remains.
- **R6 symptom noted** — the readiness probe's blind spot (unit tests and
  spec/lang live outside the wasm_cross corpus) compounded the wall-vs-panic
  reporting confusion; both go together when R6 is picked up.

## Order and gates

R3 → R1 (R3 unblocks the last scalar fallback; R1's migration then empties
the readiness probe) → R4 (its gate makes R1's deletion slice safe to verify)
→ R2 (the deep cure, now with the seams R1 exposed) → R5/R6/R7 opportunistic.
Every phase rides the existing gates (parity corpus, verify-trust, ratchets)
plus its own instrument: R1 = `ALMIDE_QQ_NO_ROUTES` unexpected-count (target
0) and `ALMIDE_DBG_QQ` fallback count (target 0); R2 = side-set count (target:
the sets deleted); R4 = the unresolved-symbol render gate (target: exists and
0).
