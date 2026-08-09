<!-- description: Self-host linking v2 — retire the shim registry's per-type twins by linking the monomorphized generic bodies the renderer already produces -->
# Self-host linking v2 — link the mono instances, retire the twin matrix

Successor design to the shim era recorded in [[v1-selfhost-machinery]]. That
campaign self-hosted the stdlib onto the wasm leg by hand-writing
splice-context `.almd` shims over the prim floor and linking them by NAME
through `self_host_registry.rs`. It worked — ~380 fns execute byte-for-byte —
and it accumulated a debt shape this document exists to retire.

## The evidence that the successor is already half-built

Two observations, one dynamic and one structural.

**Dynamic — the #1154 wall-escape trace.** For
`result.zip(ok(1.0), ok("abc"))` the emitted module contained BOTH:

- `$almide_rt_result_zip__Float_String_String` — the **correctly typed
  monomorphized instance of the real generic `stdlib/result.almd` body**,
  rendered by the ordinary user-fn pipeline, byte-correct, and **dead**;
- `$result.zip` — the hand-written scalar shim from `result_core.almd`,
  linked by the registry, reading both arguments len-as-tag, and **wrong**
  for the heap-Ok argument (it returned `err("abc")` for two ok inputs).

**Structural — the sibling-linking path already exists and already works.**
`linkable_module_fns` (`pipeline.rs:99`) contributes every bundled pure-Almide
fn *that no registry entry serves* as an ordinary user sibling;
`resolve_user_module_calls` (`pipeline.rs:150`) rewrites their Module calls to
Named calls; `monomorphize_module_fns` specializes one body per TYPE-argument
tuple (`mono/mod.rs:44`, suffix over the type vars — `__fallible_map__Int_String_String`).
`list.split_at`, `list.iterate`, and the seven `__fallible_*` carriers reach
the wasm leg this way TODAY. The registry's `registry_served_names()`
(`pipeline.rs:79`) is literally the exclusion list keeping the shims alive:
a dotted name is shimmed not because it must be, but because it is on the
list. That inversion — correct machinery live for the unregistered, hand
shims intercepting the registered — is the whole case.

## The debt shape being retired

1. **Two tag layouts, re-derived per consumer.** A scalar-Ok Result carries
   its tag at `@4` (len-as-tag); a heap-Ok Result is the 1-slot cap-as-tag
   block (payload `@12` low, tag `@16` — `result_materialize.rs`). Neither
   layout is the problem. The problem is that EVERY router arm re-derives
   which one applies.
2. **The hand-routed twin matrix.** `result_call_name` (`mod_p4_c.rs`) and
   its siblings map combinator × payload-kind × twin (`_h` / `_x` / `_s2h`)
   by hand. Its comment block is a bug ledger: the C-904 silent `ok("")`
   class, the seed-20260718 map misroute (C-151), and #1154's zip — each a
   missed or too-narrow arm in the same matrix. String-suffix twins are
   monomorphization done manually, and manual monomorphization drifts.
3. **The unlinkable class.** 74 call sites across 29 dotted callees wall as
   class-b ("unlinkable stdlib call") because no shim was hand-written for
   that cell — `zlib.*`, `process.*`, `fs.fold_lines*`, `string.split_once`,
   `map.upsert_skv_wall`, `list.split_at`, … Every one of them is a missing
   row in a table that v2 deletes.

## The v2 claim

> A dotted stdlib call whose callee has a pure `.almd` body links through
> the EXISTING sibling path — `linkable_module_fns` + per-type-tuple
> monomorphization — and `registry_served_names()` shrinks to the PRIM
> FLOOR: host intrinsics and the splice-context bodies that read memory
> directly. v2 is not new machinery; it is deleting names from a list, one
> verified family at a time.

Consequences, in order of value:

- The twin matrix and its per-combinator routing arms DELETE. Layout
  selection collapses into the one repr oracle (`repr_of`) the mono
  instance's own lowering already consults.
- The #1154 bug class becomes unrepresentable: there is no name-level
  routing step left to pick the wrong body.
- The class-b unlinkable population shrinks toward the true host boundary
  (fs/process/http effects, allocator, prim floor) instead of "cells nobody
  hand-wrote yet".
- `list.iterate` and the `__fallible_*` carriers stop being special: they
  are generic bodies like any other, linked per instantiation. This is the
  #1134 umbrella's terminal form — the per-shape wall burn-down makes the
  BODIES lowerable; v2 makes the LINKING uniform.

## What stays a shim, on purpose

- The **prim floor** (`prim.load*/store*/alloc*`, handle arithmetic) and
  every splice-context body written against it — they ARE the memory model.
- **Host-effect intrinsics** (fs, process, http, env, random's host seed):
  the callback-crossing and errno-classifying surfaces C-215/C-220 already
  scope as native-first.
- Hand shims that exist for PROVEN performance reasons may stay, but each
  needs a parity gate against its mono instance — a shim without a parity
  gate is the #1154 shape waiting to happen.

## Staged plan, each stage falsifiable

1. **The parity harness first (the falsifier).** For every registry entry
   whose dotted name also has a pure generic `.almd` body: render BOTH the
   shim and the mono instance for the instantiations the corpus exercises,
   and byte-compare execution on generated inputs (the 3-way oracle's
   muscle, pointed inward). Divergence = a live #1154-class bug TODAY —
   worth knowing before any migration. This harness is also the soak gate
   for every later flip.
2. **Family 1: `result.*`** — the matrix that bit twice. Flip call
   resolution to the mono instance per instantiation; delete the `_h`/`_x`
   arms as each combinator flips; the walled `_x` cells (zip-mixed, …)
   become executing code instead of walls. Ratchet: the routing-arm count
   and the class-b callee count both monotonically down, ledgered per
   commit (the C1 discipline).
3. **Families 2..n: `option.* / list.* / map.*`** — same per-family flips,
   ordered by bug history then by unlinkable-cell count.
4. **Registry endgame.** `self_host_registry.rs` shrinks to the prim floor
   + host intrinsics; the twin suffix namespace (`_h`, `_x`, `_s2h`)
   deletes; `v1-selfhost-machinery.md` moves to `done/` as the era record.

## Costs and open decisions

- **Module size.** One mono instance per instantiation vs one shared shim.
  Measured, not guessed: the minigit/onebrc size stats are the scoreboard,
  and DCE already drops unused instances (the dead zip instance proves
  emission happens; the question is only marginal growth when they go
  LIVE). If growth is real, instantiation-collapsing (identical-repr
  instantiations share one instance) is the known lever.
- **Perf.** Shims are hand-tuned prim-floor code; mono instances are
  lowered generic bodies. The perf ratchet (`check-perf-ratio.sh`) gates
  the flip commits like any other change.
- **Layout unification** (open, NOT required by v2): collapsing scalar-Ok
  results into cap-as-tag would delete the dual-layout distinction
  entirely, at a boxing cost on hot scalar paths. v2 deliberately does not
  depend on it — the repr oracle absorbs the duality either way. Decide
  after family 1's measurements.

## What would falsify this

- **The parity harness finds systematic shim↔mono divergence** beyond
  isolated bugs — meaning the mono instances are NOT the trustworthy side,
  and the fix order inverts (fix the generic bodies first).
- **Mono instances of the fallible/HOF families still wall** after the
  #1134 shape burn-down — then linking flips ahead of lowerability and the
  sequencing must follow the burn-down, family by family.
- **Unacceptable size or perf regressions** that instantiation-collapsing
  cannot recover — then the twin matrix survives for the hot families, but
  BEHIND the parity harness, never again ungated.

## Relations — three adjacent recreates, three altitudes

- **[[closure-architecture-v2]]** (exists) owns the closure REPRESENTATION
  layer: one identity, one capture-set, `lift_lambda`'s decline classes.
  The #1134 family-1 walls (fallible_hof/fallible_lambda baseline entries)
  live THERE — a bundled generic body links fine today when its lambda
  argument lifts; the walls fire when the lambda does not. This document
  does not touch that frontier; it removes the layer that decides WHERE a
  dotted call's body comes from.
- Feeds on: #1134's per-shape wall burn-down (makes bodies and lambdas
  lowerable) and #1108 2b-iii (2-mode monomorphization of user HOFs — the
  same mono machinery v2's flips ride).
- Orthogonal to: the identity-layer recreate (#908 / QualifiedRef) — that
  retires NAME-keyed identity; this retires NAME-keyed linking. They
  compose but neither requires the other.
- Supersedes (on completion): the linking half of [[v1-selfhost-machinery]];
  the prim-floor half stays live there.
