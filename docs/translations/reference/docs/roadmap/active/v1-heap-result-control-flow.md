<a id="v1-heap-result-ifmatch-execution--design-de-risked-no-coq-change"></a>

# v1 heap-result `if`/`match` execution — design (DE-RISKED: no Coq change)

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](v1-heap-result-control-flow.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](v1-heap-result-control-flow.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](v1-heap-result-control-flow.zh-CN.md)

> Non-normative reader copy/translation of the [canonical source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-heap-result-control-flow.md), pinned at 35297a612fb29e3f78f6587aba253aa91d51f466. This snapshot is not automatically synchronized. The original remains authoritative. Historical status, implementation, proof, audit and measurement claims remain statements of the pinned source; they have not been independently executed or validated here. Pending proposals are not current guarantees. Fences and comments remain verbatim; source metadata is displayed as an inactive note.

> Source description (inactive metadata): Historical first heap-result control-flow slice and later lowering context

<a id="historical-first-slice-later-lowering-exists"></a>

## Historical first slice; later lowering exists

The literal-arm result and “remaining” list below describe the first MIR
heap-result control-flow slice. Later match and non-literal-arm work exists
in [the MIR lowering](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-mir/src/lower/) and is recorded in
[Camp 4](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/camp4-general-heap-match.md). Do not treat this original
next-session recipe as proof that those shapes are still unimplemented.
The current shipping Wasm route is [structural](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/src/wasm_route.rs);
the original certificate reasoning is retained within its modeled scope,
not promoted to a whole-artifact memory-safety theorem.

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

<a id="the-problem"></a>

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

<a id="the-key-insight--no-coq-checker-change-is-needed"></a>

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

<a id="implementation-plan-scoped-literal--direct-call-arms-first"></a>

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

<a id="why-deferred-honesty"></a>

## Why deferred (honesty)

The design is sound and the checker is untouched, but steps 2–4 mutate the
ownership certificate generation and the wasm render — the exact surface where the
two historical accept-but-unsafe gaps (#49 transitive caps, elided-call taint)
were found. That demands the adversarial pass with fresh focus, not an
end-of-long-session implementation. Recording the de-risked design here is the
advance; the implementation + adversarial verification is the next session's
first move.

## Code-comment reading notes

The first event sketch allocates “yes” and “no” separately, then moves out once on dst. The balanced sketch allocates and consumes each object separately; dst is the IfThen result, never an allocation, so it is absent from of, and returning dst emits no second move-out (no double move).

## Source caveats

The historical IMPLEMENTED/SOUND report, remaining-work list, later FIXED allocation issue, and next-session plan coexist in the source. Later lowering is explicitly acknowledged. The retained local certificate reasoning and adversarial-audit report are not a whole-artifact memory-safety theorem or a new audit by this translation. Source instructions about tests, agents and commits are historical plan content, not actions performed here.
