Description: Historical incumbent-v0 parity plan with explicit limits on tested and witnessed guarantees
<a id="v1--v0-parity--the-completion-plan"></a>

# V1 → V0 Parity — the completion plan

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](v1-v0-parity.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](v1-v0-parity.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](v1-v0-parity.zh-CN.md)

> Non-normative reader snapshot of the pinned source; original remains authoritative. No automatic synchronization, independent implementation validation or execution of quoted instructions. Historical status, plans and comparative claims retain their source scope.
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-v0-parity.md)

> Source caveat: this July 2026 completion plan is historical, not a current support inventory or release approval. Position A universal-correctness/byte-identity claims are explicitly corrected by the source introduction; finite tests and scoped witnesses do not prove all semantics. B-3 records scalar libm exp and 7/7 completion while the retained older D1 design assumes SIMD and seven pending functions; the older ordering and counts are not reconciled. Later closure work supersedes parts of the backlog. Medical-grade and flight-quality language is the source design ambition, not certification. The quoted debugging command and next-session instructions were not executed, and no result was remeasured.


<a id="historical-completion-plan-and-evidence-correction"></a>

## Historical completion plan and evidence correction

This July 2026 plan compares the incumbent MIR leg with the now-retired v0
leg. Its blocker counts and next-session instructions are historical, not
today's structural-emitter queue. Later closure work is recorded in
[closure architecture v2](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/closure-architecture-v2.md).

The ship-gate text below overstates the evidence when it says every accepted
program is necessarily correct or byte-identical. A finite differential
corpus and accepted ownership/name/capability witnesses do not prove all
program semantics. Read [proven versus trusted](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/proven-vs-trusted.md)
for that distinction. Preserve the recorded successes and partial coverage;
do not infer universal correctness or reopen every old blocker from this plan.

> Filed 2026-07-04. A completion plan to bring v1 (MIR trust-spine) to **v0 parity**.
> Each phase has a **measurable exit gate** and **precise resumption point** so
> /goal can drive it phase by phase. This “completion” roadmap sits above
> v1-backlog.md (the individual bug ledger).

<a id="v100-ship-gate出荷条件--100-北極星のサブセット"></a>

## v1.0.0 SHIP GATE (release conditions — a subset of the 100% north star)

**“Make every wall 0” is the north star, not the shipping condition.** The gate
changes depending on what v1.0.0 claims to be:

<a id="位置づけ-a-検証済みサブセットのコンパイラv100--今すぐ出荷可能"></a>

### Position A — v1.0.0 as “a compiler for a verified subset” → **can ship now**
The v1 core (honest wall) is already established: accepted programs are necessarily
correct and rejections clean. Zero known miscompiles, native⇄wasm byte-identical
for every accepted program + PCC (ownership∧names∧caps) proven, 3-way oracle green.
Walls are explicitly “unsupported,” not “bugs.” Shipping is possible under this promise.

<a id="位置づけ-b-v0-の完全な置き換えv100--下の-14-が致命的ブロッカー"></a>

### Position B — v1.0.0 as “a complete v0 replacement” → #1–4 below are critical blockers
| # | Blocker | Status |
|---|---|---|
| **B-1** | **Dynamic dispatch (Phase C)** — first-class closures + `.method()` (most of method/computed ~79 + heap-result match tail 74). The practical core determining whether normally written code compiles. **Largest blocker**. | Not started |
| **B-2** | **Derived Codec `.decode()`** (Camp-4 heap-Ok `?`-bind). **In progress — precise diagnosis complete; next-session handoff exists**. See “B-2 handoff” below. | value.field done / desugar remains |
| **B-3** | **nn end-to-end (fast-exp family)**. The wasm oracle proved to be scalar libm exp (= already self-hosted math.exp), not SIMD. **All 7/7 open and byte-identical** (softmax_rows / gelu / swiglu_gate / rope_rotate / multi_head_attention / masked_multi_head_attention / from_q1_0_bytes). The nn matrix stack is fully self-hosted and byte-verified. **✅ Effectively closed** (remaining nn unlinked sites are only 4 fft enumerate_h/zip_h helpers, a non-inference route). | ✅ 7/7 |
| **B-4** | **Native target** — broken v0-native matrix codegen (handoff). Only if native is a shipping target. | Externally blocked |

**Phase E (complete caps proofs) and G (contract coverage) are unnecessary for v1.0.0**
— part of the north star (DONE criteria below), not the shipping gate.
Recommended progress: **finish B-3 → B-2 → B-1** (by proximity and practical impact).

<a id="完成の定義done-の判定基準--100-北極星"></a>

## Definition of completion (DONE criteria = the 100% north star)

v1 parity with v0 means **simultaneously** satisfying the following (all mechanically
decidable with existing tools):

1. **walls = 0**: `classify_corpus` reports 0 `walled real (lowering)`
   (the five structural native-FFI sites remain excluded). All 4556 spec functions + entire org corpus.
2. **Complete output parity**: `proofs/output-parity.sh` reports MISMATCH=0 / RUNERR=0,
   and the baseline covers every runnable fixture (byte-identical for all programs v0 runs).
3. **PCC invariant**: `proofs/corpus-wall.sh` maintains ACCEPT for all three properties
   (ownership ∧ names ∧ caps), without regression in any phase.
4. **3-way oracle green**: `wasm_runtime_interp_oracle::interp_cross_target_spec`
   has native==wasm==interp consensus on all fixtures (abstain ledger converges to 0).
5. **Org byte verification on both targets**: BYTE_VERIFIED in `scripts/org-trust-status.sh`
   passes on both native + wasm for every org repository.
6. **Native target recovery**: `almide run` (v0-native) passes all specs including matrix cases.

> Position after the 2026-07-04 session: spec in-profile **4374/4556 (96%)**,
> real walls **469**, nn walls **0**, parity baseline 180, PCC ACCEPT (three properties), 3-way green.
> Zero known miscompiles (honest walls only).
> **Session results**: Phase A complete (+38 in-profile; guard miscompile moved from wall to correct lowering)
> + progress on major Phase B buckets (Option[heap] field match / let-bound custom-variant match
> / List[record-ctor variant] literals — +14 in-profile total). Real walls **520→469**.
> **The biggest remaining lever is Phase C (method/computed dispatch)**: derived Codec
> `.decode()`/`.encode()` methods and first-class closures account for most of heap-result
> match tail 74 and method/computed 79 sites. An architectural area rated “large” by the roadmap.

<a id="-phase-a-完了2026-07-04"></a>

## ✅ Phase A completed (2026-07-04)

`desugar_guard` (mod_p6): bottom-up recursion structures `guard cond else E; rest`
into `if cond then { rest } else E`. It opens **function early return** (`else err(…)`
becomes tail heap/scalar-result-if) and **loop continue** (`else continue` → `else ()`
removes the continue node, admitting scalar loops). break remains walled for now
(requires genuine early loop exit). It is call-count-invariant, preserving `mir == ir`;
desugaring is also applied on the count side (classify_corpus). The guard-else bucket
**39→0**, in-profile **4322→4360**. Old “wall” pins became checks that desugaring fires,
and end-to-end pins were added.

<a id="phase-b-進行中--heap-result-の位置網羅"></a>

## Phase B in progress — positional coverage for heap results

- ✅ **Match with Option[heap] / Result[heap] field subject**
  (`match u.email { some(e) => "…${e}…", none => u.name }`): track the borrowed field's
  variant handle (materialized_options/results + heap_elem_lists) and perform heap-payload borrow binding.
- ✅ **Let-bound custom-variant heap-result match** (`let nm = match s.shape { Circle(_) =>
  "circle", … }; "${nm}…"`): `desugar_match_to_if` previously declined because it only
  handled literals; `wrap_match_arms` (push continuation into each arm and retain Match)
  opens it. Bucket **17→2**.
- ✅ **List[record-ctor variant] literals** (`[Click { x, y }, KeyPress { key }, Close]`):
  add `IrExprKind::Record { name: Some(ctor) }` to the list-literal builder's element gate,
  materialize with `try_lower_variant_ctor`. Bucket **29→24**.
- Remaining subcases: heap-result match tail (74, mostly `.decode()` methods = Phase C),
  bindings of default-field variant constructors (omitting Group's `items = []`), compound
  interpolation (Map/heterogeneous tuple `${…}` — Phase D self-hosting).

<a id="phase-a設計メモ原文保持-制御フローの完成言語機能の唯一の欠落"></a>

## Phase A (design note; original retained) — completing control flow (the only missing language feature)

**A1. Model early-return / guard-else** (39 sites).
Structure `guard cond else E; rest` into `if cond then { rest } else { E }`
(reuse block-tail heap-result-if machinery and fold remaining statements into then).
`return` expressions follow the same route.
- **Exit gate**: `guard-else early return` bucket → 0. Guarded `Result[String,String]`
  ok/err tails (currently held in backlog T4-7) open automatically too.
- **Resumption point**: change `lower_stmt`'s `IrStmtKind::Guard` (currently an honest wall)
  and `lower_body_into`'s statement loop to split then/else upon detecting Guard.
- **Weight**: medium–large (structural change to core lowering). **Take special care with parity regressions**.

<a id="phase-b--heap-result-の位置網羅最大バケット-130-sites"></a>

## Phase B — heap-result positional coverage (largest bucket, ~130 sites)

v0 returns heap-result match/if/lambda in “every position.” v1 adds builders position
by position, leaving holes.

- **B1** Tail / let-bound positions of `heap-result match` (77 + 17).
- **B2** Remaining positions of `heap-result if/lambda` (9).
- **B3** `List[heap] literal` (29) + `string interpolation` (30) as call arguments.
- **Exit gate**: these buckets → 0.
- **Resumption point**: heap-result-arm builders in `tail.rs` / `control_p4.rs`, argument
  materialization in `calls_p2.rs`. Continue this session's record-ctor / ArrV consumer work.
- **Weight**: medium (extend established mechanisms/patterns).

<a id="phase-c--動的ディスパッチfirst-class-クロージャ"></a>

## Phase C — dynamic dispatch (first-class closures)

v0 fully supports this via closure conversion. v1 only handles what it can inline-defunctionalize.

- **C1** `method/computed heap call` (61 + 11) — opaque calls to passed-around closures.
- **C2** Returning `heap-result Lambda` (9).
- **Exit gate**: closure-passing programs lower (`unresolvable method/computed` bucket → 0).
- **Resumption point**: extend `funcref_value_of` / `CallIndirect`, materialize the closure
  environment's value model. Connect with docs/roadmap/done/closure-architecture-v2.md.
- **Weight**: large (design for putting closure-env ownership into PCC).

<a id="phase-d--stdlib-self-host-の完成"></a>

## Phase D — complete stdlib self-hosting

Not self-hosted = wall. Currently 211 `.almd` / 171 registered. Remaining holes:

- **D1 Seven fast-exp functions** (`softmax_rows` / `gelu` / `swiglu_gate` /
  `multi_head_attention` ×2 / `rope_rotate` / `from_q1_0_bytes`). **Direct nn end-to-end
  blockers**. v0 sums almide-kernel SIMD fast-exp **in lane order**, so bit-exact transfer
  requires exp_pd_{wasm,neon} polynomials (six-term Horner) + two-lane partial sums + libm exp
  for the odd tail. **Medical grade: build ULP differential tests before starting**;
  no casual implementation. Reference `crates/almide-kernel/src/silu.rs::exp_pd_neon`,
  precedent `stdlib/math_exp.almd`.
- **D2 Effectful bricks** (`fan.*` concurrency / `http.*` / `net.*` / `datetime.parse_iso` /
  `env.os` / `env.temp_dir` / `fs.stat` / `process.env`). Each needs a WASI prim + capability
  declaration + admitted-effectful registration. Start with those openable like fs.read_bytes
  (this session). Fan nondeterminism separately requires a capability-proof design decision (→ Phase E).
- **D3 Remaining pure combinators** (rich-element `list.zip`/`enumerate`, unsupported
  Map/Set representations, etc.). In ROI order.
- **Exit gate**: nn unlinked → 0 (seven via D1, five via D3). effectful-27 set functional.
- **Weight**: D1 large (precision), D2 medium×n (per brick), D3 small–medium.

<a id="phase-e--capability--effect-の完成"></a>

## Phase E — complete capability / effect support

- Design for putting `fan.*` (concurrency/nondeterminism) into the caps proof model.
  Set capability types for `http`/`net`/`process`. Absorb effectful-27-blueprint.md.
- **Exit gate**: effectful stdlib walls (`needs a declared capability`) → 0.
- **Weight**: large (extension of the proof mechanism).

<a id="phase-f--native-ターゲット-parity"></a>

## Phase F — native-target parity

- **F1 (handoff) Repair v0-native matrix codegen**: mismatch between enum conversion of
  `bridge::AlmideMatrix` and vendored glue (old Vec API). Transitional flat-ABI/burn rework.
  **Hand off to the owning session**, or decide to take it into this roadmap.
- **F2 Productionize v1 MIR→Rust native emit** (the flight-profile keystone).
- **Exit gate**: `almide run` passes all specs including matrix; native==wasm bytes.
- **Weight**: F1 medium (repair existing breakage), F2 large (flight-quality foundation).

<a id="phase-g--契約計測の-ratchet"></a>

## Phase G — contract / measurement ratchets

- File ledger contracts for matrix.mul (ascending k) / from_bytes OOB (all-zero matrix)
  (**requires completed F1** — byte verification is impossible without a native oracle).
- Bring coverage to target (control_p5 defunc engine's 43% is the largest unvisited area).
- **Exit gate**: new C-NNN + fixture, `check-contracts.sh` passes.

<a id="推奨着手順依存--レバレッジ"></a>

## Recommended order (dependencies + leverage)

**A → B → D1 → C → D2/D3 → E → F → G**.

- **A (early return)** is the only language-feature hole and a prerequisite for B/D tail materialization, so first.
- **B** has established patterns and the largest count → large early wall reduction.
- **D1 (fast-exp)** is the only nn end-to-end blocker; advance it as an independent lane
  alongside precision-test preparation (do not wait for other phases).
- **C (closures)** is a major architectural mountain, after B.
- **F1** repairs broken v0 and precedes G. Its order may shift with concurrent sessions.

<a id="goal-の切り方提案"></a>

## How to split /goal (proposal)

Phase-sized goals are easy to run. Examples:
- “Complete Phase A (guard-else early return), reduce the `guard-else` bucket to 0.
  Keep all gates green.”
- “Cover Phase B heap-result match (tail/let/arg), reducing three buckets to 0.”
- “Self-host the seven Phase D1 fast-exp functions with ULP differential tests, reducing nn unlinked by seven.”

Each goal's completion can be checked mechanically through the exit gates above
(the relevant `classify_corpus` bucket count + cross-cutting battery green).

<a id="b-2-handoff次セッション再開点--2026-07-05"></a>

## B-2 handoff (next-session resumption point — 2026-07-05)

<a id="状況"></a>

### Situation
- ✅ Self-hosted `value.field` (Object tag check, byte-identical, committed).
- ✅ **Downstream lowering is correct**: `effect fn dec(v) = { let fv=value.field(v,k)!; let x=value.as_T(fv)!; …; ok(R{…}) }`
  (separate-bind form) produces **valid wasm and identical bytes even with multiple fields**
  (scratch: man5/man6 `dec`).
- ❌ **Derived `Basic.decode` (plain fn, `?`) is invalid wasm** (`type mismatch: expected i32, found i64`).

<a id="精密診断ここが肝"></a>

### Precise diagnosis (the key point)
A debugging **desugared-IR dump is implemented** (committed, env-gated):
```
ALMIDE_DBG_DESUGAR_FN=<fn名> [ALMIDE_DBG_DESUGAR_RAW=1] almide/render_program <file>   # eprintln に desugared IR
```
The quoted command's Japanese placeholders/comments mean “function name” and “desugared IR to eprintln.”
`crates/almide-mir/src/lower/desugar.rs::dump_ir` / `dump_desugared_ir`, called by
`lower_function_all_impl` (mod.rs).

**Finding**: in man6 (one file containing both `type Basic: Codec` and handwritten
effect-fn `dec`), the **desugared IR of `dec` (valid) and `Basic.decode` (invalid)
is identical except for VarId numbers** (confirmed by `ALMIDE_DBG_DESUGAR_RAW` diff).
Nevertheless **MIR differs** (`Basic.decode` has two extra locals; i32/i64 switch
around v91 — observable in wat local declarations).

→ **Identical desugared IR lowers to different MIR**. Differences at function level:
  - `dec` has `is_effect: true`; `Basic.decode` has `is_effect: false` (plain fn).
  - Derived param `_v: Value` has `ParamBorrow::Own` (handwritten uses borrow). However,
    current `bind_params` (mod_p3) ignores `ParamBorrow` and treats all heap params as borrowed,
    so this alone should not differ.
  - **Actual lowering uses `desugar_heap_branches(func.body)` → TCO → `lower_body_into`**,
    unlike the dump's `desugar_all` entry. **Investigate this next**: pre_tco's
    `desugar_heap_branches` may create different intermediate forms for the two functions;
    dump the actual desugared body at `lower_body_into` and compare with `desugar_all`.

<a id="次の一手順に"></a>

### Next moves (in order)
1. **Dump the body on the actual lowering path**: produce the same `dump_ir` at entry
   to `lower_body_into` (after desugaring), diff `dec` vs `Basic.decode`. Since the existing
   dump goes via `desugar_all`, the difference from pre_tco should appear here.
2. The correct desugar-lift form is **man5/man6 `dec`'s separate-bind nested-match**
   (`subj: vN` (already lifted), not `subj: Call(value.as_int, [Try(...)])`). Generate this
   for derive too. The previous attempt (Try recognition in take_or_recurse + outer unwrap
   recursion + callarg-unwrap before effect-unwrap) opened single/effect cases, but
   **only multi-field plain-fn `?` remained invalid wasm** (reverted; see git-history diff).
   Reapply after #1 eliminates the reason “identical desugared IR produces different MIR.”
3. **Strict honest-wall discipline**: never commit invalid wasm. Require validation
   via `render + wasmtime` for every attempt.
