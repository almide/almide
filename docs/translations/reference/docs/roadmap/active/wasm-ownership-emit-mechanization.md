Description: Ownership-emit incident history and the implemented measurement infrastructure
<a id="wasm-所有権-emit-層の機械化-perceus-drift-の構造的封じ込め"></a>

# Mechanizing the WASM ownership emit layer (structurally containing Perceus drift)

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](wasm-ownership-emit-mechanization.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](wasm-ownership-emit-mechanization.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](wasm-ownership-emit-mechanization.zh-CN.md)

> Non-normative reader snapshot of the pinned source; original remains authoritative. No automatic synchronization, independent implementation validation or execution of quoted instructions. Historical status, plans and comparative claims retain their source scope.
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/wasm-ownership-emit-mechanization.md)

> Source caveat: this is a historical incident investigation and revised plan, not reproduction of current defects or proof of memory safety. The retired v0 description, early OPEN labels, later landed #643 fix and implemented counting infrastructure coexist deliberately. Early root-mechanism hypotheses and the withdrawn get_or fusion are preserved, not promoted to current diagnoses. The September correction changes the emphasis from a missing helper to duplicate implementations. Dates, counts, approximate percentages, comparisons and source assertions about correctness or prevention were not independently remeasured. Quoted tracing/build/CI instructions were not executed; no remediation is authorized by this reader copy.


<a id="historical-diagnosis-and-implemented-measurement"></a>

## Historical diagnosis and implemented measurement

The original `PerceusVerify`/`emit_wasm` description is about the retired v0
emitter, not the current structural Wasm route. The September appendix also
records that the original #643 fix landed; earlier “still OPEN” headings
preserve the diagnosis at that point in the investigation.

The proposed implementation-count infrastructure now exists:
[impl-count-ledger.toml](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/impl-count-ledger.toml).
Its presence does not mean every duplicate implementation has been merged or
that emit-layer memory safety is fully proven. The source-to-witness and
unmodeled-access limits remain described in
[Memory Safety](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/MEMORY-SAFETY.md). Preserve the incident corrections
below rather than treating each earlier hypothesis as a current bug.

Status: active — proposal + precise analysis of #643
Owner: compiler
Related: #643 (wasm rc double-free), #668/#643 history, project_wasm_frees_activation,
project_completeness_roadmap, contract C-066

<a id="問題の本質-perceus-は解析層だけ機械化されている"></a>

## The core problem: Perceus mechanizes only the analysis layer

Almide's WASM Perceus has two layers.

1. **Analysis layer (`pass_perceus.rs`) — mechanized.** Ownership analysis decides
   where to place `RcInc`/`RcDec` nodes in IR. The `PerceusVerify` pass
   mechanically verifies the IR-level inc/dec balance (hard-error, not warn).

2. **Emit layer (`emit_wasm/*.rs`) — handwritten.** Actual memory accesses for
   `UnwrapOr` / `list.get` / record construction / `ToOption` … are handwritten in wasm.
   **Each emitter must itself honor the ownership contract assumed by the analysis layer**,
   but neither types nor a verifier express that contract: only comments and discipline maintain it.

The native target delegates ownership to Rust (= an external oracle), so
`unwrap_or` and similar operations automatically move/clone correctly. WASM
reimplements this by hand, so **holes in that reimplementation cause drift**.
This is exactly why the rt-oracle registry exists (about 72% of cross-target bugs
are “wasm runtime drifts from the native oracle”).

<a id="なぜ検証ゲートが防げないか"></a>

### Why verification gates cannot prevent it

`PerceusVerify` sees **only the IR-level inc/dec balance in (1)**.
Whether the emitter returned an owned pointer with +1 or a borrow (alias) without +1
lies below the IR abstraction and is **invisible** to it. Thus “balanced in IR,
but double-free/leak at runtime” is possible. The some-box of `list.get`
performing `rc_inc` on its element is an emit-layer detail (`calls_list.rs`),
invisible to the verifier.

<a id="643-の精密分析-still-open"></a>

## Precise analysis of #643 (still OPEN)

Reproduction (the some-arm is the cause; the issue-title none-arm hypothesis is wrong):

```almide
let cs = ["a", "b"]
var out: List[String] = []
var i = 0
while i < 2 {
  let nx = list.get(cs, i) ?? ""     // in-bounds = some-arm
  list.push(out, list.slice(cs, i, i + 1) |> list.join(""))  // 後続 alloc
  i = i + 1
}
```

The original code comments mean: in-bounds = some-arm; subsequent allocation.

- native: correct. wasm: double-free sentinel in `__rc_dec` (another dec when rc==0) → `unreachable` trap.
- Isolation (`ALMIDE_WASM_FREES=1` is the default):
  - **some-arm (in-bounds) + subsequent alloc + loop → TRAP**
  - none-arm only (always OOB) → OK
  - Replace `?? ""` with `"z"` → OK
  - Replace the subsequent alloc with `list.push(out, "lit")` → OK (latent)
  - Remove the loop → OK

Loop body in post-Perceus IR (`ALMIDE_DUMP_IR=Perceus`):

```
bind nx = unwrap_or(runtime_call list_get(cs, idx), "")
rc_inc nx                       // VDecl alias-inc (yields_borrowed_alias(UnwrapOr)=true)
  bind t657 = { bind t656=list_slice(cs..); bind t659=list_join(t656); rc_dec t656; t659 }
  runtime_call list_push(out, t657)
  rc_dec t657
assign i = i + 1
rc_dec nx                       // scope-end
...teardown: rc_dec cs; rc_dec out
```

Established facts:

- `nx` (alias-inc + scope-dec) is balanced in IR.
- The some-box of `list.get` does `rc_inc` on the element (SHARE in `calls_list.rs`),
  but **there is no IR node that decrements that box** (the inner of UnwrapOr
  is an intermediate expression, unbound to any variable, so no temp-dec attaches).
- The comment at `pass_perceus.rs:743` explicitly says “UnwrapOr errs toward leaking
  (safer than double-free)” — **double-free therefore violates the design**, and
  an extra dec is entering somewhere.

<a id="確定--非割り当てトレーサで根本機構を特定-2026-06-13"></a>

### Confirmed — a non-allocating tracer identified the root mechanism (2026-06-13)

The initial instrumentation (`__println_int`) **itself calls `__alloc`**, changing
free→reuse patterns and masking corruption (HEISENBUG). Replacing it with a
**non-allocating tracer writing raw bytes from fixed low memory [16..36) through
`fd_write(stderr)`**, and observing returned ptr+size from `__alloc` and
free/double-free in `__rc_dec`, yielded the full r643 timeline:

```
ALLOC 4184/16(cs list) 4208/8(out 空) 4224/4(Some box) 4240/12(slice) ...
FREE 4240(slice) ALLOC 4264/40 ALLOC 4224/12  ← 4224 は #2 で size4・未 free
FREE 4224 ... FREE 4264 FREE 4184 DOUBLE-FREE 4264 → trap
```

The trace labels mean: cs list; empty out; Some box; slice; address 4224 had size4 at #2 and had not been freed.

**Root mechanism**: element slots of `out` start at `ptr + LIST_DATA_OFFSET(8)`.
The data[0] of capacity-0 `out` (4208, size8) **overlaps the header (4216) of the
adjacent Some box (4224)**. The Some box from `list.get(cs,i) ?? d` **leaks without
a dec** (no temp-dec for UnwrapOr's inner intermediate expression), and its
header (size=4) is **overwritten with size≥12**. It then passes the free-list
size check in `__alloc` (`cur.size >= request`), which **hands out the live,
undersized block (4224)** → buffer overflow + double-free in subsequent rc_dec
(rc==0 sentinel).
This is the class “leaking repeated heap temporaries (Some box / slice / join) in
a loop corrupts an adjacent block header, and `__alloc` redistributes a live small block.”

<a id="スケール依存--試した-get_or-融合は不十分--回帰"></a>

### Scale dependence — the attempted get_or fusion is insufficient and regresses

Fusing `list.get(xs,i) ?? d` into `list.get_or(xs,i,d)` (no Some box) makes the
**reported len2 reproduction byte-identical on both targets**, but:

- **Divergence returns at len≥3 / trap at len6** — same-class corruption through
  slice/join temporaries remains (the Some box is only one instance).
- `spec/wasm_cross/alias_combinator_rc` **diverges/traps at the byte gate** — the
  get_or RC path differs from get→unwrap and breaks existing alias RC.

→ **Fusion is therefore not a fix for #643** (withdrawn). The real fix is
mechanizing ownership in the emit layer: (1) reliably dec the Option boxes allocated
by UnwrapOr/getters (or unify on a box-free route), (2) properly release RC of
repeated heap temporaries (slice/join results) per iteration, (3) add the
`cur.rc == 0` invariant to free-list reuse in `__alloc` to prevent redistribution
of live blocks (hardening that turns silent corruption into a clean trap).
Also add “loop × repeated heap temp” churn fixtures in `spec/churn/`.
Speculative patches create new leaks/regressions and are **unmerged**; #643
remains OPEN as §1 of this roadmap.

<a id="関連-591-errorcontext-の-ok-パスが-main-の戻り値を汚染-still-open"></a>

## Related: #591 error.context's OK path contaminates main's return value (still OPEN)

The same class of “the emit layer handwrites ownership/layout beneath IR and drifts.”
`error.context(ok(3), msg)` is correct on native (`3`); wasm produces **exit=1 and
an `Error:` line with enormous whitespace on stderr**, while `m ?? -1` yields `0`
(the value is correctly `3` via `match`). Isolated findings:

- The err path (inner=err in C-098 cases) is normal, native==wasm.
- Only the OK path breaks. `error.context` is `runtime_call almide_rt_error_context`
  in IR; in wasm it reaches `dispatch_runtime_fallback("error","context")` →
  the inline arm of `emit_call` (`("error","context")` in `calls.rs`, handwritten
  `if_i32`: OK passes its input through; err rewraps with `": "`).
- The inline arm has **asymmetric layouts** for the OK result (Result[Int,String],
  payload i64@4, 12B) and err result (handwritten 8B `[tag][str@4]`). The discrepancy
  where `??` reads `0` but match reads `3`, plus main returning Err, suggests that
  the typed-if result is not fully consumed and leaks into main's tail/return.

→ The principled approach is to stop hand-inlining and turn the native oracle
`almide_rt_error_context` (`r.clone().map_err(|e| format!("{ctx}: {e}"))`)
into an **actual wasm runtime routine** and register it with rt-oracle
(the same approach as consolidating `emit_extract_owned` above: converge handwritten
emit code on a single oracle-conforming implementation). The pinning fixture
`spec/wasm_cross/error_context.almd` has not been added (must be created — no fixture
currently pins this claim).

<a id="提案-emit-層の所有権を単一ヘルパーに集約する"></a>

## Proposal: consolidate emit-layer ownership into one helper

Stop scattered handwritten `rc_inc`/payload-load code and consolidate “extract
an owned value from a container/box” into **one emit helper**:

```
fn emit_extract_owned(&mut self, payload_ty: &Ty, offset: u32)
//   load payload at offset; if is_heap_type(payload_ty) { rc_inc }   (ptr 透過)
```

The original comment specifies loading the payload at the offset and incrementing RC if its type is heap-backed (pointer-transparent).

- `UnwrapOr` (Option some / Result ok), `Unwrap`, `Try`, `ToOption`,
  `OptionalChain`, and Option-returning map/list accessors all follow **the same discipline**.
- On paths where the box is decremented (consume-then-drop), extracting the payload
  as a new owned reference requires +1. This contract is consolidated in **one place**,
  structurally preventing partial fixes like #668, where pass-through combinators
  were fixed but UnwrapOr's some-arm was missed.

<a id="提案-静的検証では見えない層を-fuzz--churn-ゲートで塞ぐ"></a>

## Proposal: close layers invisible to static verification with fuzz / churn gates

IR-level `PerceusVerify` cannot see emit-layer borrowing/ownership. Therefore:

- Add churn fixtures in `spec/churn/` for **owned-extract → drop (looping
  `list.get(...) ?? d`, including OOB-default, over containers of heap elements)**;
  compare native output with wasmtime under a wall-clock limit. A single byte
  fixture is insufficient (latent corruption occurs even with correct output).
- If possible, use a differential harness **comparing total rc_inc/rc_dec counts
  between native↔wasm through runtime counters** (observe ownership drift directly below IR).

<a id="段階"></a>

## Stages

1. **Confirm #643 through runtime tracing → one-off fix** (emit layer or
   pass_perceus, consistent with the design intent “prefer a leak”). Add
   `spec/wasm_cross/option_fallback_rc.almd` (not yet added — must be created)
   for regression pinning + contract.
2. Extract `emit_extract_owned` and migrate all payload-extraction sites.
3. Add a `spec/churn/` owned-extract fixture + (future) RC-count differential harness.
4. Document the ownership contract the emit layer must obey in `emit_wasm/CLAUDE.md`.

<a id="却下した案"></a>

## Rejected alternatives

- **Strengthen only analysis**: emit-layer borrow-vs-owned never rises into IR;
  no amount of stricter analysis can detect emit-layer drift.
- **Delegate everything to Rust like native**: impossible because wasm does not
  pass through Rust. That is why ownership discipline must converge on a
  mechanized single helper.

<a id="2026-09-21-再測定-段階-24-は未実行のまま同じ層から-blocker-が出続けている"></a>

## 2026-09-21 remeasurement: stages 2–4 remain unexecuted; blockers keep coming from the same layer

Seven weeks after the diagnosis above (2026-07-30), only stage 1 (the one-off
#643 fix) had been executed. Stage 2's `emit_extract_owned` had zero grep hits;
there was neither the stage 3 churn fixture nor stage 4 contract documentation.
Count what happened in that interval.

**Blocker origins (51 I-unsound / I-divergence / I-miscompile / regression issues
from 2026-08-22 to 09-21, by mechanism)**:

| Mechanism | Count | Examples |
|---|---:|---|
| **RC discipline in wasm emit** (missing inc / guard at consumption sites) | **~12** | #1786 #1820 #2260 #2308 #2317 #2395 #2397 #2398 |
| wasm i32/u32 indices / width truncation | ~7 | #1907 #1909 #2385 #2396 |
| wasm fixed-size buffer / park span | 5 | #1826 #2116 #2117 #2120 |
| Name-key collision (two kinds in one identifier space) | 4 | #1726 #1828 #2369 |
| Missing native borrow-inference spellings | 3 | #1713 #2315 #2377 |
| Recursion depth | 3 | #2291 #2306 #2307 |

The largest bucket is precisely the layer named by this roadmap; **3 of the 4 issues
caught by the 0.63.0-rc3 soak** (#2395 / #2397 / #2398) are here. All three have the
form “the rule was not copied to another implementation of the same operation”:

- #2398: `list.update` — sibling `list.set` is automatically correct through
  `ArgMode::Retain`, but `update` lowers the closure body with `self.lower(...)`, so it has no mode.
- #2397: `list.fold` — the staged copy (`list.rs`) acquired the rule in 10fed0494;
  the fused copy (`list_fuse.rs`), reached only through a `map`/`filter` chain,
  was **written earlier** and never received it.
- #2395: `value.keys` — **both** the wasm side and `value_core.almd` lacked the
  tag check; only interp abstained (through an accidental 0).

<a id="診断の訂正-穴はヘルパーの欠如ではなく一操作に実装が何個あるか"></a>

### Diagnosis correction: the hole is not “no helper” but “how many implementations one operation has”

The 07-30 proposal was to consolidate payload extraction in one helper. That is
correct, but these three issues are not forms a helper can prevent: **the same
operation is implemented multiple times; a rule enters one copy but not the others**.
Count them:

| Operation (`= _` hole in list.almd) | native | Structural-emitter arms | Self-host prim-floor copies |
|---|---:|---:|---:|
| `fold` | 1 intrinsic | **3** (staged / fused / enumerate) | **6** (`list_fold*.almd`, by shape) |
| `sort` | 1 | 1 | 6 |
| `get` | 1 | 2 | 4 |
| `filter` | 1 | 2 | 4 |
| `map` | 1 | 2 | 3 |

`list.fold` has **10 implementations**. Self-host copies are monomorphizations
by shape (`hrec` = heap acc × heap elem, `hsca`, `ols`, `str_hacc` …), handwritten
memory code directly on the prim floor — unlike Koka/Lean/Roc's “one stdlib body
in the language, uniformly processed by Perceus.” Of 3,877 stdlib functions,
**694** are `= _`.

**Why sweeps miss it (demonstrated by #2397)**: ee's closure sweep used the
predicate “lower a closure body and store it in a heap slot,” establishing closure
through **counts of four guard spellings**. Accumulator rebinding stores to a local
and was outside the population; `list_fuse.rs` had none of the guard vocabulary,
so it was registered as empty: “nothing to count.”
**Sweeps counting marker presence cannot see copies older than the marker.
“0 matches in this file” is not a clean sign but a sign of an unconverted copy.**

Population recounted by syntax: 44 `self.lower(<body>, …)` sites. The remaining
unguarded sites are `set.fold` / `map.fold` (`collections_set.rs:577` /
`collections.rs:464`), but both return the accumulator as `Lowered::view`, so callers
do not free it: leak-class, not freed-read (#2408). Nothing in the emitter records
that returning a view is load-bearing.

<a id="段階の書き直し0630-final-の後"></a>

### Rewriting the stages (after 0.63.0 final)

Keep stages 2–4, but put a **counting gate** before them. Order: measure → ratchet → consolidate.

1. **Implementation-count gate** — a `proofs/impl-count-ledger.toml` ledger counting
   triples (native intrinsic, emitter arm, self-host copy) for each `= _` hole,
   with a shrink-only ratchet. CI fails if a new copy has no ledger row. #2397
   should have been visible when “fold emitter arms increased from 2→3.”
2. **Consolidate `list.fold`** — 10 implementations into **one body per target**.
   The two fused/staged arms become one lowering, with fusion in an earlier pass;
   the six self-host shape copies become one type-erased body (`list_fold_hrec.almd`
   already has that form). Each consolidation lowers ledger counts; the alloc
   ledger / size ratchet / run parity attest unchanged behavior.
3. Continue with `update` / `filter_map` / `sort`, applying the same procedure to
   **62 holes in list.almd**.
4. The 07-30 stages 2–4 follow. After consolidation, only one body remains for
   the “single helper” to converge upon.

Rejected: “add fixtures to catch the next missing copy” — a #2397 fixture could
only be written with a chain that fuses; all non-fusing fixtures were green.
Counting, not behavior, catches it.
