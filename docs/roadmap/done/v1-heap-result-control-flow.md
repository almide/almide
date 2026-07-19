<!-- description: v1: design for heap-result if/match execution — de-risked, no Coq change required -->
<!-- done: 2026-06-15 -->
# v1 heap-result `if`/`match` execution — design (DE-RISKED: no Coq change)

Status: **`if` with literal arms IMPLEMENTED (commit 126921e6), adversarially
verified SOUND.** `fn label(c) -> String = if c then "yes" else "no"` now executes
byte-matching v0. The design below proved exactly right — NO Coq change, the
existing checker accepts the per-arm `"im"` certificate unchanged. Three
independent adversarial audits found no accept-but-unsafe gap. corpus-wall:
ownership 13121→13129, all three properties ACCEPT.

REMAINING under this slice: heap-result `match` (desugar via `desugar_match_to_if`),
and non-literal arms (direct calls — step 1 below). A SEPARATE pre-existing bug the
audit surfaced — **since FIXED** (render_wasm_p2.rs): String/Bytes blocks are now
sized LIST-COMPATIBLY (`cap = ceil(bytes/ELEM_SIZE)` elements, allocation =
`LIST_HEADER + cap*ELEM_SIZE` — exactly what the `$alloc` reuse check recomputes),
so freed String blocks ARE reclaimed and String-churning loops run in bounded
memory (pinned by the churn tests in render_wasm/tests_part1_b.rs / tests_part2.rs).

## The problem

A heap-RESULT branch returns String/data from its arms:

```almide
fn label(c: Bool) -> String = if c then "yes" else "no"
```

Scalar `if`/`match`/`while`/`for` already EXECUTE (commits a2a9f656 / 0a6db87b /
547b5efc) via the flat markers `Op::IfThen`/`Else`/`EndIf` (+ `LoopStart`/… for
loops). The soundness argument is **per-arm balance**: each arm is internally
balanced, so the cert processes the flat marker stream exactly as the
corpus-proven linearization. Scalar arms carry no heap → trivially balanced.

A heap-result arm breaks this: each arm `Alloc`s its string and the value
**escapes** (it is the return), so the arm is NOT internally balanced. The flat
ownership certificate would then see two arm allocs and one move-out:

```
objA: i          (Alloc "yes")
objB: i          (Alloc "no")
ret  : m  on dst (one move-out)
```

`objA`/`objB` get `i` with no matching `m`/`d` → the checker FAULTS (leak) →
REJECT. That is why the current lowering DEFERS heap-result arms to a single
`Alloc{Opaque}` (sound, but the result is Opaque = not executable).

## The key insight — NO Coq checker change is needed

The kernel-proven checker only FOLDS the per-object event stream (`i`/`a`/`d`/`m`)
and checks each object balances (`certificate.rs::ownership_certificate` emits the
stream; the Coq checker re-verifies it). It already accepts `"im"` (alloc then
move-out). So if the **compiler** emits a move-out PER ARM — restoring per-arm
balance — the existing checker accepts with ZERO proof change:

```
objA: im         (Alloc "yes"; Consume objA)
objB: im         (Alloc "no" ; Consume objB)
dst : (none)     dst is the IfThen RESULT, never an Alloc → not in `of`
                 → func.ret = dst emits NO second `m` (no double-move)
```

Each arm is now `"im"` = internally balanced. The flat linearization of two
balanced arms is balanced. The existing per-arm-balance soundness argument HOLDS
unchanged. Runtime: the wasm `if` runs exactly one arm → one `Alloc` + return
(rc=1, caller owns); the untaken arm never allocates → no leak, no double-free.

This DOWNGRADES the slice from "modify + re-prove the Coq checker" (what the
earlier sessions believed) to "compiler-side cert generation + render, checker
untouched." Much cheaper, but still soundness-core.

## Implementation plan (scoped: literal + direct-call arms first)

1. `try_lower_heap_result_if(cond, then, else_, ty)` in `lower/control.rs`
   (mirrors `try_lower_scalar_if`): gate on `is_heap_ty(ty)`, a scalar-lowerable
   `cond`, and arms that are **simple owned heap producers** — a `LitStr`
   (→ `Op::Alloc{Init::Str}`) or a direct `Call`/`CallFn` returning heap (→ the
   call op, which the cert already scores `i`). Other arm kinds (heap `Var`,
   nested branch, block-with-locals) → return `None` → fall back to today's sound
   Opaque form. A `match` variant desugars through `desugar_match_to_if` exactly
   like the scalar path.
2. Each arm: lower the producer to `objX`, then push `Op::Consume{v: objX}` and
   REMOVE `objX` from `live_heap_handles` (it is moved out as the result, not a
   per-arm drop). Emit `Else{val: Some(objX)}` / `EndIf{val: Some(objX)}` so the
   wasm leaves the handle on the stack for `dst`.
3. `ownership_certificate` (`certificate.rs`): no change needed — `Op::Consume`
   already emits `m`, `Op::Alloc`/heap-`Call` emit `i`, and `IfThen.dst` is never
   inserted into `of` so `func.ret` adds no second `m`. VERIFY this with the
   adversarial pass (the danger is an arm whose producer is NOT a fresh owned
   value — e.g. a borrowed param — which must REJECT, not silently balance; a
   borrowed-param Consume emits `m` at rc 0 → the checker already FAULTS, the
   correct wall, but confirm the gate never admits it).
4. `render_wasm.rs`:
   - `render_wasm_fn`'s `IfThen` arm currently hardcodes `(result i64)`. Make the
     result type follow `dst`'s repr: **i32 when `dst` is heap**, i64 scalar.
   - The dst-repr WRINKLE: `value_reprs_wasm` must classify the `IfThen` `dst` as
     heap. Infer it from the `EndIf`/`Else` `val` repr (the arm handles are i32),
     or thread the repr on the marker. Get this right — a heap dst rendered as an
     i64 local is a type error at best, a silent miscompile at worst.
5. Tests (`render_wasm.rs`): `label(true)="yes"`, `label(false)="no"`,
   byte-matching v0; a call-arm variant; a `match` literal-pattern heap-result.
6. Gates: `cargo test -p almide-mir`, `proofs/corpus-wall.sh` must stay WALL OK /
   3-property ACCEPT (watch the ownership count — heap-result-if corpus fns move
   from Opaque-1-object to per-arm-2-objects, so ownership may RISE; that is
   sound as long as ACCEPT holds and each is `"im"`).
7. **Adversarial soundness pass** (REQUIRED — this is where #49-class
   accept-but-unsafe gaps live): spawn ≥3 independent agents to try to construct a
   heap-result branch that the new lowering makes the checker ACCEPT but that
   leaks or double-frees at runtime. Only commit if they fail to break it.

## Why deferred (honesty)

The design is sound and the checker is untouched, but steps 2–4 mutate the
ownership certificate generation and the wasm render — the exact surface where the
two historical accept-but-unsafe gaps (#49 transitive caps, elided-call taint)
were found. That demands the adversarial pass with fresh focus, not an
end-of-long-session implementation. Recording the de-risked design here is the
advance; the implementation + adversarial verification is the next session's
first move.

## Follow-on work

This design's execution slice (heap-result `if`/`match`) shipped and became the
foundation for two later efforts: [v1-loop-ownership-cert.md](../active/v1-loop-ownership-cert.md)
(loop-body match execution and the porta org-repo conquest built on top of
per-arm-balanced heap-result branches) and
[v1-parser-tco-lever.md](../active/v1-parser-tco-lever.md) (TCO over a match
returning heap-result, which required this slice's per-arm balance argument as
a precondition).
