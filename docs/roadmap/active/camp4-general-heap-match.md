# Camp-4, generalised: make heap-result `match` total instead of recognised

Status: **plan + first landed arc.** Written 2026-08-17 from a fresh outside wall
(almide/dfa) plus a source-level read of the four RC references.

## Landed 2026-08-17 (the #1492 arc) — all gates green (403/403, no cert change)

The L2 hypothesis below held where tested: every fix landed **in lowering only** —
no checker, kernel, or Coq change — which is what L2 predicts (the `CBranch`-era
checker was never the blocker; the gates written under the old FLAT model were).

- **Heap→heap Result payload re-wrap admitted** (`ok(r.node)` after `let r = g(n)!`,
  the #1492 headline): `try_lower_result_variant_ctor`'s Ok arm falls back from the
  ctor builder to `lower_result_str_piece` — the exact composition the record cell
  already used. The piece side was already near-total (`lower_owned_heap_field`).
- **Recursive-payload subjects route through the rc-guarded wrapper drop**:
  `try_lower_result_match_value` now routes a subject whose Ok payload needs a
  recursive drop via `resrec:` (and a scalar-Ok/recursive-Err subject via `optrec:`)
  instead of the flat dec that leaked the payload's fields the moment an arm
  extracted a child instead of moving the payload whole. A whole-payload move is
  unchanged in effect (the recursion decs to 1 and stops) — Lean's
  incs-before-decs derived-value rule, applied at the one site that lacked it.
- **`lower_result_str_piece` got the ctor guard** every other Named-call site had —
  a variant-ctor payload builds its tagged block instead of emitting a dangling
  `CallFn`. Audited the class: 22 user-named CallFn sites, 10 ctor-position probe
  programs, all BUILD+PARITY; the render's unlinked-call wall backstops the rest.
- **Module-level heap globals admitted as heap-result arm values**
  (`if c then DIGIT else …`): the arm's Var case uses `value_or_global` inside a
  per-arm frame, mirroring the owned-heap-field Var arm.
- **Scalar-Ok × record-Err admitted** (`Result[Int, {code, msg}]`): the record twin
  of the variant-Err cell, len-as-tag `optrec:` wrapper.
- **`List[(scalar, …)]` variant ctor fields admitted** (ADT brick 5, the
  `Cls(List[(Int, Int)])` shape): extended in lockstep at all four mirror sites —
  ctor admission, both `needs_recursive_drop` twins, the generator's field free
  (the `__drop_list_str` sweep, List[flat-variant] precedent verbatim), and the
  `LIST_STR_DROP_SRC` linkage gate.

Result on the 10-shape wall corpus (`w01–w10`): **2 → 6 BUILD+PARITY**, then
**7 of 10** with the combinator leg below; the remaining 3 are the Map/Set twins.

## Landed 2026-08-17, second arc: the combinator leg

`result.map` / `map_err` / `flat_map` over a type instantiation with no linked typed
twin now desugar to the equivalent `match` (`desugar_result_combinator_to_match`,
in both the counted-tree and lowering chains) — the reference-compiler architecture
for combinators (rustc's `?`, Swift's library `??`, Roc's canonicalization). The
desugar asks `result_call_name` itself whether the instantiation routes `_x`, so the
twin-availability logic lives in exactly one place: linked twins stay byte-identical,
and a match the lowering cannot express walls exactly as `_x` did. Verified with
lambda AND named-fn `f` arguments, custom record Err types, 403/403.

## The remaining brick: Map/Set typed twins (sized, not started)

`Map[List[Int], _]` / `Map[String, <record>]` / `Set[List[Int]]` wall at the
`_key_wall` / `_x` routes (mod_p4_e.rs:609). This is NOT a lowering gap: the
stdlib self-hosts are per-class prim-level hash tables (`map*.almd` 3,238 lines,
`set*.almd` 1,191 lines, one file per key/value class — `set_str.almd` is the
String twin). A heap-key class needs structural eq+hash over elements, owned-block
drop routing inside the table, registry + router + usage-gate wiring, and — per
CLAUDE.md's family rule — the executable matrix gate stating which cells exist, in
the same PR. A real stdlib arc of its own; do not bolt it onto a lowering session.

Downstream check: `almide/dfa` (a full recursive-descent regex parser with a
`Result[Cursor,_] → Result[Node,_]` spine) now builds to wasm (41KB, v1-verified),
its 50 tests run via wasm, native/wasm byte parity on real inputs.

Supersedes the framing in `v1-value-model.md` §263-308, which pinned Camp-4 to
one instance (`value.as_array : Result[List[Value], String]`) and to a checker
limitation that the code says no longer exists. The instance analysis there is
still good; the *root cause* named there needs re-verification before anything
is built on it.

---

## 1. What walls

A fallible function cannot change its `Result` payload from one heap type to
another. Filed upstream as almide/almide#1492.

```almide
type N = | L(Int) | C(List[N])
type R = { node: N, pos: Int }

fn g(n: Int) -> Result[R, String] = if n > 0 then ok(R { node: L(n), pos: n }) else err("neg")

fn f(n: Int) -> Result[N, String] = { let r = g(n)!; ok(r.node) }   // R -> N
```

```
heap-result `match` outside the executable subset cannot be faithfully returned
in this brick (would move out an empty deferred heap value)
```

`ok(r)` (payload unchanged) builds. `ok(r.pos)` (payload → scalar) builds. Only
heap → heap walls. `match` instead of `!` walls identically; `result.map` has no
wasm definition at all, so all three spellings are closed.

This is not an exotic shape. It is the spine of every staged transform —
source → AST → IR → analysis → output — with each stage fallible. A
recursive-descent parser hits it on the single line that drops the cursor:

```almide
fn parse_alt(cps: List[Int], pos: Int) -> Result[Cursor, String] = ...

fn parse(pattern: String) -> Result[Node, String] = {
  let c = parse_alt(to_codepoints(pattern), 0)!
  ok(c.node)                                       // the only line that walls
}
```

## 2. Where it lives

`lower_tail_heap_match` (`lower/tail_b.rs:355-440`) is a fixed sequence of
hand-written shape recognisers, tried in order, with `Unsupported` as the
fallthrough:

| # | recogniser | admits |
|---|---|---|
| 1 | `tuple_extract_match_index` | single-arm tuple destructure |
| 2 | `try_lower_custom_variant_match` | user ADT subject, heap arms |
| 3 | `try_lower_variant_value_match` | Option/Result subject — **scalar payload only** |
| 4 | `try_lower_result_match_value` | len-as-tag Result subject |
| 5 | `try_lower_option_match_value` | `Option[heap]` subject |
| 6 | `try_lower_list_match_value` | list subject |
| 7 | `try_lower_tuple_refinement_match` | scalar-tuple subject |
| 8 | `desugar_match_to_if` → `try_lower_heap_result_if` | reducible to an if |
| — | else | **wall** |

Scale of the recognised-shape approach:

- **192** distinct `try_lower_*` / `lower_heap_*` recogniser functions under `lower/`
- **3,817** lines across the six files carrying Camp-4 gates
  (`control_p2.rs` 829, `control_p2_b.rs` 656, `control_p3.rs` 460,
  `result_match_value.rs` 246, `tail_b.rs` 813, `desugar_unwrap_b.rs` 813)
- **64,930** lines in `lower/` total

The comments are explicit that this is a frontier being pushed forward one shape
at a time — `tail_b.rs:375` "a heap payload self-gates back to None here = the
true Camp-4 frontier", `control_p2.rs:751` "no single-slot borrow rep yet".

## 3. Diagnosis: three layers, and only one of them is real work

### L1 — architectural: shape and ownership are entangled

Each recogniser must hand-prove its own ownership story, so **each new shape is
new work, and every shape not yet proven is a wall.** `RESEARCH.md` synthesis
point 5 already names this as the anti-pattern absent from all eight surveyed
compilers:

> No compiler has a per-VALUE "tracked/untracked" state. […] Almide's
> `materialized_results` / `heap_elem_lists` ValueId sets are this anti-pattern —
> the wall "match over an UNTRACKED subject" cannot be expressed in any of the eight.

Survey 3a states the alternative directly:

> Shape vs ownership SEPARATED: Roc's match lowering is layout-driven and total;
> borrow-vs-incref is a LATER whole-program ARC pass over the same layouts.
> […] "Untracked → decline the match" exists in NEITHER compiler. **Every
> well-typed scrutinee is matchable by construction.**

### L2 — stale: the recorded root cause names a checker that was replaced

`v1-value-model.md:275` gives the blocker as:

> `verify_ownership` — which processes the if-arms FLAT — see TWO drops of the one
> subject = a false double-free (the checker doesn't model then/else mutual exclusion)

`lib_c.rs:735-742` now says the opposite, in as many words:

> BRANCH JOIN (mirrors the proven checker's `CBranch` rule): each arm of an
> `IfThen`/`Else`/`EndIf` runs from the SAME entry state, and the arms must AGREE
> on every object's leaving count […] Folding the arms FLAT (**the old model**)
> counted BOTH arms' events, silently accepting cross-arm compensation

`OwnershipChecker.v` carries the matching rule and its tests
(`check_line [COp Inc; CBranch [Inc] [Inc]; COp Dec; COp Dec] = true`,
`check_line [CBranch [Inc] [Dec]] = false`).

If that reading is right, **the gates written under the old model were never
revisited**, and some fraction of the 192 recognisers are guarding against a
constraint that no longer exists.

> This claim is exactly the kind the org's own meta-lesson warns about — *"solo
> deep/Coq diagnoses were wrong on every hard wall"*. It is written here as the
> first thing to **measure**, not as a finding to build on. See Stage 0.

### L3 — real: no uniform borrow representation for a heap payload

`control_p2.rs:751`: a non-`String` heap payload "has no single-slot borrow rep
yet". And `Result` has **two** physical layouts today — len-as-tag (37 files) and
cap-as-tag (36 files) — so `Result[Unit, String]` is physically two different
things depending on which stdlib function produced it. Until layout is a total
function of type, no general match lowering can exist, because it cannot know
what it is matching on.

This is Stage A of the adoption plan already recorded in `RESEARCH.md:231`.

## 4. What the references actually do

Read in `almide-references/` at the lines below, not quoted from the memo.

### Koka — `koka/src/Backend/C/Parc.hs`, `parcGuard`

```haskell
return $ \liveInSomeBranch -> ... do
  let dups = S.intersection ownedPvs liveInThisBranch
  drops <- filterM isOwned (S.toList $ liveInSomeBranch \\ liveInThisBranch)
```

Two lines. `liveInSomeBranch` is the union across arms; each arm drops what the
union holds and it does not. Pattern variables become owned or borrowed by
`isOwned` on the *scrutinee* (`ownedPats`), and `inferShapes` records the alias
map from scrutinee to payload — the aliasing that Almide currently treats as a
blocker is a first-class, named concept here.

### Lean 4 — `lean4/src/Lean/Compiler/LCNF/ExplicitRC.lean`

`addPrologForAlt` (:421) is the same rule, plus one ordering constraint Almide
does not currently have:

```lean
-- These are derived values who are no longer kept alive by a (potentially transitive)
-- parent value in this alternative and must thus be incremented. It is crucial that
-- these increments happen before the decrements as the decrements might contain an
-- operation that frees a parent.
```

The `cases` site (:624-653) builds the union explicitly:

```lean
let caseLiveVars := alts.foldl (init := {}) fun acc ⟨_, altLive⟩ => acc.union altLive
modifyLive fun _ => caseLiveVars
```

and `withCtorAlt cs.discr c` registers payload binds as **derived from the
discriminant** — borrowed by default, incremented only where the bind outlives
the parent.

### Roc — `roc/src/lir/arc.zig`

Match lowering is layout-driven and total; ARC is a separate whole-program pass
over the same layouts. Payload reads deliberately do **not** propagate ownership
(borrow + anchor). Each exit emits `state − keep`.

### Grain — `grain/compiler/src/codegen/garbage_collection.re:719-731`

RC over structured wasm, the closest target shape to ours: `MReturn` performs a
full cleanup of every live managed binding **skipping the returned value**.

### The law, 4/4

**Every well-typed scrutinee is matchable.** Ownership is decided afterwards by a
liveness computation whose branch rule is `union ∖ here`. No reference declines a
match because of a payload's representation.

## 5. The key fact: Almide already wrote Koka's rule, in the wrong place

`lower/result_match_value.rs:227-244`:

```rust
fn emit_arm_release_parity(
    &mut self,
    consumed_by_then: &[ValueId],
    consumed_by_else: &[ValueId],
    else_marker_at: usize,
) {
    for x in consumed_by_then {
        if !consumed_by_else.contains(x) { /* drop after THEN */ }
    }
    for x in consumed_by_else {
        if !consumed_by_then.contains(x) { /* drop at the ELSE marker */ }
    }
}
```

That is `liveInSomeBranch \\ liveInThisBranch`, verbatim. The differences from
the references are all structural, not conceptual:

| | Almide today | Koka / Lean |
|---|---|---|
| Scope | inside **one** recogniser | one pass, all branches |
| Arity | two arms | n arms via the union fold |
| Input | hand-collected `consumed_by_*` lists | **computed** liveness per arm |
| Ordering | drops only | **incs before decs** (Lean: a dec may free a parent) |
| Borrows | per-shape gate | `withCtorAlt` / `inferShapes` alias map |

So the work is not to invent the rule. It is to **hoist the rule that is already
written** out of one brick, drive it from computed liveness, and then delete the
gates it was compensating for.

## 6. Plan

### Stage 0 — measure before building (no production code)

The entire plan below is conditional on L2. Establish it empirically first.

1. Build a wall corpus: every shape currently rejected by
   `lower_tail_heap_match` and `try_lower_variant_value_match`, one minimal
   `.almd` per shape, each with an expected runtime output.
2. For each gate that cites the FLAT-arm model, remove **only that gate**, and
   run the full gate set (corpus-wall ACCEPT ×3 props, byte-verify, cargo test,
   output parity). Record which shapes now pass unchanged.
3. Publish the count. If a meaningful fraction passes, Stage 3 shrinks
   drastically and may partly precede Stages 1-2.

Deliverable: a table of `shape → still-blocked? → by what`. Nothing is merged.

**Do not do this solo.** The recorded org meta-lesson is that solo deep/Coq
diagnoses were wrong on every hard wall, and that an independent fleet map plus
full-gate verification found the sound answer each time. Run Stage 0 that way.

### Stage 1 — one type, one layout

Unify the len-as-tag and cap-as-tag `Result` representations onto one canonical
layout, so `layout = f(type)` becomes true. This is `RESEARCH.md:231` Stage A,
sized there at ~70 files. Add the single-slot payload borrow that
`control_p2.rs:751` says is missing — **one** borrow rep for any heap payload,
not one per payload type.

Gate: no change in accepted programs. This stage is pure representation.

### Stage 2 — hoist the union rule into a pass

A MIR pass that runs after lowering and before `verify_ownership`:

1. Per branch arm, compute the live set (Lean `withCollectLiveVars`).
2. `caseLive = ⋃ altLive`.
3. Per arm: `dec` for `caseLive ∖ altLive` where owned and not borrowed;
   `inc` for borrowed values whose bind outlives the parent —
   **incs emitted before decs**.
4. Payload binds are borrowed by default and derived from the subject
   (`withCtorAlt`), so the alias that blocks the current lowering is modelled
   rather than avoided.

This is the pass Almide's `CBranch` checker rule was always waiting for: the
checker demands the arms agree, and this is the pass that **makes** them agree.

Gate: every currently-accepted program still accepts, byte-identical where the
old path already emitted parity drops; `emit_arm_release_parity` becomes dead and
is deleted.

### Stage 3 — make the lowering total, delete the recognisers

With layout total (Stage 1) and ownership handled downstream (Stage 2),
`lower_tail_heap_match` becomes layout-driven dispatch with no `Unsupported`
fallthrough. Retire recognisers in dependency order, each behind the full gate
set, deleting its Camp-4 comment with it.

Expected deletion: most of the 3,817 lines in the six gate files, plus whatever
Stage 0 shows was already dead. `RESEARCH.md:434` separately estimates ~2,800
lines for the nine `!` position passes, which the same primitive retires.

## 7. Relationship to Survey 4's `Op::Return`

`RESEARCH.md:422` (Survey 4e) recommends a Zig-shaped frame-targeted
`Op::Return { val }`. That is a **different** wall — the nine position-specific
`!` desugars — and it is complementary, not a substitute:

- `Op::Return` fixes *early exit in value position*.
- This plan fixes *matching on a heap subject and producing a heap result*.

Both converge on the same verifier work (`diverged` on a branch frame, exit
obligation at `Return`), so Stage 2's pass should be designed to accept a
diverging arm from the start.

## 8. What would falsify this plan

- **Stage 0 shows the gates are still all load-bearing.** Then L2 is wrong, the
  June analysis stands, and Stage 1's borrow rep carries the whole cost.
- **The union rule cannot be expressed over Almide's structured, certificate-
  carrying MIR** without a kernel change. Koka and Lean both emit into
  unstructured IRs; Grain is the structured-wasm reference and should be read in
  full before Stage 2 is committed.
- **Layout unification breaks byte-verification.** Stage 1 touches the emitted
  bytes for every `Result`-returning stdlib function; if the certificates cannot
  be re-established, the staging order has to invert.

## 9. Provenance

- Wall found writing [almide/dfa](https://github.com/almide/dfa) (a multi-pattern
  DFA matcher targeting wasm), filed as almide/almide#1492.
- References read at: `koka/src/Backend/C/Parc.hs` (parcGuard),
  `lean4/src/Lean/Compiler/LCNF/ExplicitRC.lean:421,624`,
  `roc/src/lir/arc.zig`,
  `grain/compiler/src/codegen/garbage_collection.re:719`.
- Prior art in-tree: `RESEARCH.md` synthesis + Surveys 3a and 4,
  `v1-value-model.md` §263-308.
