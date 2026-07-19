<!-- description: v1 TCO for self-recursive tail calls to a scalar-state loop — the yaml-parser keystone -->
# v1 TCO — self-recursive tail calls → scalar-state loop (the yaml parser keystone)

**Status: DESIGN (2026-06-19). Soundness-critical → design-first, implement fresh + adversarial, verify against the proven checker before shipping. NEVER ship a checker-rejected witness.**

## Why

`almide/yaml`'s v1 wall (28/74) is dominated by DIRECT self-recursive parsers
(`scan_quote`, `find_colon_at`, `oct_rec`, `bin_rec`, `flow_rec`, `esc_rec`, …).
The heap-result-`if` arm lowering (`lower_heap_result_arm`, control.rs) WALLS a
self-recursive call arm (`if name == self.fn_name { return None }`) because v1
has NO TCO: emitting a real recursive `CallFn` would, on deep input, overflow the
wasm call stack (fail-stop trap) — and **v0 TCOs these** (proved 2026-06-19: a
500k-deep self-recursive `Option[Int]` runs in v0 with no overflow; `pass_tco`'s
`is_tco_candidate` = `can_default_init(ret) ∧ all self-calls in tail position`).
So executing the recursion in v1 without TCO would DIVERGE from v0 on deep input
(v0 loops, v1 traps) — not byte-parity. The guard is CORRECT; the fix is to give
v1 the same TCO v0 has.

## The key simplification

In every yaml self-recursive parser the self-call changes only SCALAR params; the
HEAP params pass through UNCHANGED:

```almide
fn scan_quote(s: String, pos: Int, in_q: Bool) -> Option[Int] =
  if pos >= string.len(s) then none                       // base → result, break
  else { let c = string.get(s, pos) ?? ""
    if in_q then scan_quote(s, pos + 1, not (...))        // recurse: pos,in_q := …  (s unchanged)
    else if c == "#" and … then some(pos)                 // base → result, break
    else scan_quote(s, pos + 1, …) }                      // recurse: pos,in_q := …
```

`s` (the only heap param) is identical in every self-call. Only `pos`/`in_q`
(scalars) change. So TCO here is a SCALAR-STATE loop — exactly the shape
`try_lower_scalar_while` (control.rs:1336) already lowers cert-clean via
`LoopStart`/`LoopBreakUnless`/`LoopEnd` + `Op::SetLocal`, reassigning scalar
loop-carried state. The heap param needs NO reassignment (no heap loop-carried
state → no `in_frame` heap-reassign wall).

## The shape

```
LoopStart                                  ; markers carry no ownership (verify_ownership no-op)
  <recompute per-iteration locals>         ; e.g. `let c = string.get(s, pos) ?? ""` — a per-iter heap temp
  <if base-case-A> { result := <A>; brk }  ; base arm ALLOCs the heap result into `result` local, breaks
  <else if recurse> { pos := pos'; in_q := in_q'; <continue, no break> }
  <drop per-iteration heap temps>          ; `c` freed before the back-edge (per-iteration balance)
LoopEnd
ret = result                               ; the moved-out function result (rc=1)
```

The transform: a self-recursive function whose body is a heap-result `if`/`match`
chain where every self-call (a) is in tail position and (b) changes only scalar
params (heap params bit-identical) becomes the loop above. Base-case arms
(`none`/`some(p)`/`ok`/`err`/literal/concat) set the `result` local and break;
recurse arms `SetLocal` the changed scalars and fall to the back-edge.

## The cert (the soundness-critical part — NEW vs scalar-while)

`try_lower_scalar_while` REQUIRES "no net heap handle escaping the per-iteration
frame." TCO VIOLATES that: the base-case arm's `Alloc` (the function result) MUST
escape — it is moved out after `LoopEnd`. So this is a NEW cert shape:

- Per ITERATION: the recurse path allocs nothing for the result (only `SetLocal`s
  scalars) and frees its transient heap (`c`) before the back-edge → balanced `i…d`.
- The base-case path allocs the result (cert `i`) and the loop breaks; the result
  is `Consume`d (cert `m`) ONCE after `LoopEnd` (the move-out) → `im`, exactly the
  heap-result-`if` arm balance the checker already accepts.
- The heap param `s` is borrowed (no acquire, no drop in the callee) — same as the
  straight-line case.

Claim to VERIFY against the proven checker (the `corpus-wall` ownership fold): the
object set is { the per-iteration transients (each `id` within the loop frame),
the result (`im`, alloc on the taken base-case, moved out after the loop) }. If
the checker REJECTS (e.g. it cannot see the break-once / model-one-iteration for a
heap result escaping a loop), the design is WRONG → revert, do NOT ship; the
fallback is to leave the self-rec guard (yaml stays walled on these) until the
loop-result cert is extended.

## BLOCKER found 2026-06-19: the loop primitives are TOP-TEST ONLY

The MIR loop markers (`LoopStart` / `LoopBreakUnless{cond}` at the TOP /
`LoopEnd` back-edge / `SetLocal`) model a classic `while cond { body }` — ONE
exit, tested at the top, no value. General TCO needs MID-loop break with a
RESULT: a self-recursive function has ≥2 base cases (`none` vs `some(p)`),
interleaved with the recurse arm, each producing a DIFFERENT heap result. That
cannot be expressed as a single top-test while + post-loop result derivation
without rewriting the computation per-function (not a general transform).

So TCO needs a NEW loop primitive: a mid-loop **break that sets the function
result and exits** (e.g. `Op::LoopBreakWith { val }` rendering `(result.set;
br $outer)`), plus the cert reasoning that the result `Alloc` (`i`) escaping the
loop is `Consume`d (`m`) exactly once after `LoopEnd`. This is a MIR + render +
**ownership-cert extension** (a heap value crossing the loop boundary) — strictly
larger and more soundness-critical than reusing the existing top-test markers.
Implement it design-first + adversarial; verify the escaping-result object's
`im` trace is ACCEPTED by the proven checker BEFORE shipping. If the checker
cannot model a heap result escaping a loop, this is a genuine cert frontier
(like Camp 4) → Mob-gate, do not ship rejected.

## Scope / gates

- ONLY self-recursive (direct `name == self.fn_name`); mutual recursion is a
  separate (harder) problem — leave walled.
- ONLY scalar-changing self-calls (heap params bit-identical). A self-call that
  changes a heap param (rebuilds a list/string) needs heap loop-carried state →
  defer (the `in_frame` heap-reassign wall already rejects it).
- desugar-before-both: if implemented as an IR rewrite, `count_ir_calls` must see
  the SAME transform (the self-call disappears — count parity must hold).
- VERIFY every step: `render_program` probe byte-match (shallow AND a deep input,
  to confirm no trap = TCO actually applied) + `corpus-wall` ACCEPT (3 props +
  mir≤ir) + `cargo test -p almide-mir` + `output-parity` baseline no-regress.

## Expected payoff

Unblocks the ~6 direct self-recursive yaml parsers (and any self-recursive
heap-returning function in the corpus). Combined with (2) the non-self-rec
heap-arm shapes (after_colon tuple-with-heap-Value etc.) and (3) the Camp-4
heap-payload variant match (the 4 `looks_numeric`/`is_compound`/… + float.parse
self-host), this is the path to yaml = 0 walls on v1. TCO is the keystone.

## CLEANEST TCO ENTRY POINT, found at the function level 2026-06-19: scan_quote / find_colon_at

These two yaml functions are the most tractable TCO targets (and unblock 2 of the 22 walls directly,
likely oct_rec/bin_rec/flow_rec too — same shape):
```almide
fn scan_quote(s: String, pos: Int, in_q: Bool) -> Option[Int] =
  if pos >= string.len(s) then none
  else { let c = string.get(s, pos) ?? ""
    if in_q then scan_quote(s, pos + 1, not (c == "'" or c == "\""))   // TAIL self-call
    else if c == "#" and ... then some(pos)                            // base: scalar-payload Option
    else scan_quote(s, pos + 1, c == "'" or c == "\"") }               // TAIL self-call
```
SHAPE = the textbook TCO case: **tail self-recursion, SCALAR loop-carried args (pos, in_q), a
SCALAR-payload `Option[Int]` result, base cases `none`/`some(scalar)`**. No heap arg, no heap accumulator.

BUILD (extend the EXISTING loop machinery — control.rs ~1349 scalar-while + ~2027 for-range use
`LoopStart`/`LoopBreakUnless`/`LoopEnd`/`SetLocal`): replace the self-rec GUARD (control.rs:1641
`if name == self.fn_name { return None }`) with a TCO transform for THIS shape — detect a body whose
if/match arms are either (a) a direct self-call with the same fn + scalar-updated args, or (b) a base
expression; emit a `LoopStart` … per-iteration: compute the branch conditions, on a base arm set a
RESULT local (`Op::Alloc` the `none`/`some(pos)` Option — the ONE heap alloc, `i`) + break, on a
self-call arm `SetLocal` the updated scalar args + loop. After `LoopEnd`, return the result local
(`m`). The cert sees one `i` (the result Option at the taken break) + one `m` (return) = balanced; the
scalar SetLocals carry no ownership. VERIFY: byte-match v0 on `scan_quote`/`find_colon` inputs +
corpus-wall + tests. This is a focused but well-scoped brick (a new recursion→loop transform), NOT a
session-end one-liner — but it is the HIGHEST-LEVERAGE next move (TCO unblocks ~6 of the 22 walls).

### CORRECTION (implementation-level, 2026-06-19): TCO needs a NEW MIR primitive, not a loop-reuse

Examined at the op level: scan_quote's base cases (`none`, `some(pos)`) sit MID-BODY (inside the
`if in_q … else if # … else …` chain), NOT at the loop top. The existing loop primitive is a TOP-TEST
`LoopBreakUnless { cond }` (control.rs:1377/2032) — it can only exit at the head of an iteration. So
the recursion→loop transform CANNOT reuse it: it needs a **MID-BODY BREAK-WITH-RESULT** primitive
(`Op::LoopBreak { result: ValueId }` or similar) that exits the loop carrying a heap result computed at
an arbitrary point in the body. That is a NEW MIR op + render (`(br $loopexit)` after setting the
result local, inside a wasm `block`/`loop` with the result threaded) + cert handling (the break's
result `i` must balance against the function `m`; the cert must treat the loop body's per-iteration ops
as balanced, and the break-result as the moved-out return). So the "mid-loop-break-with-result MIR
primitive" named in this doc's title is REAL and REQUIRED — TCO is a focused MIR+render+cert brick, not
a transform over the existing top-test loops. (This is why the self-rec guard at control.rs:1641 stays:
without the new primitive there is no sound lowering, only the deep-trapping recursion.)

### REFINEMENT 2026-06-19: the GATE-VERIFIABLE cert-clean structure (no new primitive, no cert change)

Re-analysis shows TCO is implementable WITHOUT the new `Op::LoopBreak` primitive (and so without a
cert change — which would be UNVERIFIABLE since corpus-wall *uses* the cert). The cert-clean shape:
**a SCALAR-ONLY top-test loop + a POST-LOOP heap-result-if.**
- The loop body updates ONLY the scalar loop-carried args (`pos`, `in_q`) via `SetLocal` — NO heap in
  the body, so each iteration is trivially per-iteration-balanced (the existing cert passes as-is).
- The heap result (`none`/`some(pos)`) is computed AFTER the loop, as an ordinary heap-result-if over
  the FINAL scalar state — balanced normally (`i`+`m`), no Alloc escaping the loop body.
PRECONDITION: the base results must be RECONSTRUCTIBLE from the final scalar state. scan_quote/find_colon_at
satisfy it: `if pos >= len then none else some(pos)`. (A function whose base result needs more than the
final scalars would need the result-in-loop form → the Alloc escapes the loop body → the cert flags it →
that subset still needs the new primitive. So START with the reconstructible subset.)
THE TRANSFORM (the intricate part): from the body's nested if/else where leaves are self-calls
`f(invariant, pos', in_q')` or base exprs, DERIVE (a) the continue-condition = ¬(any base reached), (b)
the loop body = the scalar updates on the self-call paths, (c) the post-loop result = the base-expr
chain re-evaluated on the final scalars. Emit `LoopStart` / `LoopBreakUnless{continue}` / body
`SetLocal`s / `LoopEnd`, then the post-loop heap-result-if. VERIFY: byte-match v0 on scan_quote inputs +
corpus-wall (existing cert) + tests. GATE-VERIFIABLE, bounded (a wrong derivation fails byte-match →
revert) — but a genuinely intricate transform, a focused brick. scan_quote/find_colon_at first (2 walls,
the reconstructible subset); oct_rec/bin_rec/flow_rec/flow_step likely follow the same shape.

### LANDED 2026-06-19 (commit 77c91648): scalar-arg TCO — scan_quote/find_colon_at, yaml 22→19

IMPLEMENTED `try_tco_rewrite` (lower/mod.rs): the result_kind + scalar-loop + post-loop-dispatch
transform, gated to HEAP-result tail-self-rec with SCALAR loop-carried args. VERIFIED: scan_quote
byte-matches v0 (3/99/99/0), corpus-wall ACCEPT, output-parity 61/61, 466 tests. yaml 22→19
(scan_quote + find_colon_at now lower; float.parse recognition earlier took 22→21, TCO 21→19).

REMAINING self-rec walls ALL need a HARDER extension (heap loop-carried args = a heap back-edge merge,
the `while_body_heap_accumulator_walls` class):
- `oct_rec`/`bin_rec`: `match list.first(cs)` leaves (need MATCH-leaf support in tco_collect/rewrite)
  AND `cs: List[String]` is a HEAP loop-carried arg (drop(cs,1) each turn) — out of the scalar gate.
- `flow_rec`/`flow_step`: MUTUAL recursion (flow_rec↔flow_step, not direct self-rec) AND `acc:
  List[String]` heap accumulator (acc+[x]).
So the next TCO brick is HEAP-LOOP-CARRIED: a scalar loop carrying a heap accumulator needs the
accumulator merged across the back-edge (rebind the heap local each iteration, freed once) — the
hard case the scalar-while explicitly rejects today. Plus match-leaf + mutual-rec support. Each is a
further focused extension; the scalar-arg subset (this commit) is the clean foundation.

## SUPERSEDED (for oct_rec/bin_rec) — verified 2026-07-19

This doc's own conclusion above (oct_rec/bin_rec need the "HARDER extension... a heap back-edge
merge" — a new MIR primitive + cert extension, last commit 7b402d6e, 2026-06-19) is **NOT what
actually happened**. [v1-value-model.md](v1-value-model.md)'s "list-iterator TCO" section
(commit 338ff9a5, same day ~4h later, "Add list-iterator TCO: rewrite a shrinking-list heap
recursion to an invariant list plus scalar index (yaml 13 to 11)") shows oct_rec/bin_rec were
cleared via a **DIFFERENT, cert-clean lowering rewrite** (invariant-list + synthetic scalar
index) needing **no cert/checker extension and no new MIR primitive** — verified still present:
`fn try_list_iter_rewrite` in `crates/almide-mir/src/lower/desugar_loop.rs`.

So: this doc's REMAINING section (the "HARDER extension" / heap-loop-carried / new
`Op::LoopBreak`-style primitive) is **closed/superseded for oct_rec/bin_rec** by that mechanism,
not by the primitive this doc concluded was required. The **general** self-recursive-TCO
question this doc raises for OTHER cases (a heap accumulator that genuinely can't be reframed as
an invariant list + scalar index, e.g. `flow_rec`/`flow_step`'s mutual recursion + `acc:
List[String]` accumulator) may still be open — don't overclaim full closure from this note alone.
