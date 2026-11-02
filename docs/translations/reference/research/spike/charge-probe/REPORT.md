<a id="stage-1-charge-trace-preservation--first-empirical-run"></a>

# Stage 1 Charge-Trace Preservation — First Empirical Run

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](REPORT.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](REPORT.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](REPORT.zh-CN.md)

> Non-normative historical reader snapshot of the pinned original. The unchanged original remains authoritative; no automatic synchronization, independent testing, benchmark reruns or proof of current safety/performance. August 2–3 branch instructions, pending merge GO, implementation claims and measurements are historical, not present execution authorization. Old auto-unwrap, optional-clock syntax and paths are not current API guidance. The early 1000→50→3 ns calibration history and later corrections are retained, not combined into one measurement. Stage 1 lists uncomparable native walls yet makes a broad peephole statement; that statement is not new BCE verification. Ledger headline23 versus later added tiers, first-attempt2/6 versus “two initial failures”, and11+2 versus6×2 matrix counts remain source inconsistencies. The timeout-ω slice is not general effect replay, and self-measured Dojo results with between-round fixes are not an independent fixed-compiler benchmark. Full claim4/5 restrictions remain as recorded. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/spike/charge-probe/REPORT.md)
> The closing List[Result] scope note coexists with the earlier tuple-implemented afterword; both source layers are retained rather than reconciled.

Status: **PASSED — 9/9 comparable runs identical on all three points (2026-08-02)**
Spec: [docs/roadmap/active/logical-time-implementation.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/logical-time-implementation.md) §4
Rerun: `ALMIDE_FUEL_PROBE=1 almide run fixtures/<f>.almd [--target wasm]` (with a build from this branch)

<a id="何を検証したか"></a>

## What Was Verified

The sole premise explicitly excluded from proof by logical-time-proofs.md —

> Both renderers preserve the charge events of the shared MIR (same events, same order, once each)

— was implemented and measured. `Op::Charge` was inserted at the IR→MIR boundary (function entries + immediately after every LoopStart, W1 placement), and **(stdout, consumed, trace_hash)** were compared between native (generated Rust + thread-local counter) and wasm (exported mutable globals + guest epilogue printing). The trace is order-sensitive (wrapping `t*1000003+site`, site = function-name FNV + within-function serial number, independent of function order).

<a id="結果"></a>

## Results

| fixture | Path covered | Verdict |
|---|---|---|
| loop | while loop | **MATCH** (1002) |
| recursion | recursive fib(20) | **MATCH** (21892) |
| branch | if/else + match | **MATCH** (602) |
| strings | string-concatenation loop | **MATCH** (102) |
| mutual | 5000 levels of mutual tail recursion | **MATCH** (5002) |
| switch | 12-arm dense match (**br_table path**) | **MATCH** (1002) |
| fusion | arithmetic-heavy (**fuser path**) | **MATCH** (2002) |
| nested | nested double loop | **MATCH** (2602) |
| recursion + `--wasm-opt` | **through Binaryen -Oz** | **MATCH** (21892) |
| list | filter/map with lambda | native wall (v0 fallback) — not comparable |
| bce | list.get loop (BCE path) | native wall — not comparable |

**Zero divergences.** The main suspects, renderer-local peepholes (fuser, br_table conversion, bounds-check-elision, wasm-opt), neither dropped charges nor disturbed their order.

<a id="消費値の意味論的検証数字が正しいことの独立確認"></a>

## Semantic Validation of Consumption (Independent Check of the Numbers)

- fib(20): call-tree size 21891 + main entry 1 = **21892** ✓
- 1000 loop iterations: entry 1 + 1001 passes through the loop head (including the exit check) = **1002** ✓
- is_even(5000): 5001 calls + main 1 = **5002** ✓

<a id="既知の限界stage-1-スコープ外として明示"></a>

## Known Limitations (Explicitly Outside Stage 1 Scope)

1. **Native wall silently loses the probe:** when native v1 walls and falls back to v0, no probe line appears. The full implementation needs wiring that treats fallback detection as probe failure.
2. **Parallel native fan{}:** thread-local counters do not sum consumption on arm threads. Arm serialization or atomic counters + order normalization are needed in Stage 3 (race) — this is where the core deterministic-bounds problem appears.
3. **Trap paths:** direct proc_exit skips the epilogue, so no probe line appears (on both legs).
4. Measurement covers entries and loop heads only (granularity v0). Variable costs (Dyn charge) are not implemented.

<a id="判定"></a>

## Verdict

Charge-trace preservation **holds at this granularity and over this set of paths**. This provides grounds to proceed with the Stage 1 full implementation (spec/wasm_cross fixtures, charge certificate, property tests). No counterexample appeared, but the search space was nine programs, not exhaustive.

<a id="昇格2026-08-02同-branch"></a>

## Promotion (2026-08-02, Same Branch)

The spike findings were locked into gates:

1. **Wall honesty:** native v0 fallback during probing is a hard error (`src/cli/mod.rs`, `render_v1_native_or_fallback`). A silent unmeasured run became structurally impossible.
2. **Static certificate:** `charge_probe::{wasm,native}_charge_sites` + order-sensitive first-occurrence comparison. The Charge claim in `translation_validation::wasm_pattern` was also made site-specific (rejecting by the name of the particular dropped charge).
3. **Permanent gate:** `tests/charge_probe_test.rs` — one test covers dynamic three-point × 8 fixtures + static certificate × 8 + wall honesty × 2 (`cargo test --release
   --test charge_probe_test`, ~2.4s; only the dynamic layer is skipped when wasmtime is absent).

All 605 existing almide-mir lib tests are green (adding Op::Charge was non-breaking).

<a id="stage-2-垂直スライス--fanbounded-が両ターゲットで着地2026-08-02同-branch"></a>

## Stage 2 Vertical Slice — fan.bounded Lands on Both Targets (2026-08-02, Same Branch)

`fan.bounded(compute.ms(100)) { heavy(1000) } ?? fallback` works on native v1 / wasm, demonstrating a **deterministic boundary**:

- **Flip point:** measured consumption of `heavy(1000)` is 1002 charge units (entry 1 + loop head 1001). `compute.us(1001)` is EXHAUST and `compute.us(1002)` is OK — both targets flip at **the same 1µs-granularity point** (boundary.almd, asserted in the gate). Exact agreement between theory and measurement.
- With the probe enabled too, all three points match (including bounded, consumed=503010 / matching trace).
- Four diagnostics fire as specified by ADR-0001: bare Int / mixed-in Duration / non-call body / unknown unit (enumerating the closed set).

<a id="実装形logical-time-implementationmd-からの差分"></a>

### Implementation Shape (Differences from logical-time-implementation.md)

- **Outlining desugar:** `fan.bounded` is outlined into synthetic fn `__almd_bounded_N(budget, args…) -> T` (enter → body call → exit); exit **persists the verdict** ($__b_verdict), which the caller reads as a scalar. `bounded ?? fb` is a **fused form** (entirely scalar If in which no Result value ever exists), solving the native rung's lack of a heap-Result ABI. Bare bounded uses ResultOk/ResultErr nodes (works on wasm; native hits the existing rung wall).
- Budget prims are `PrimKind::{BudgetEnter,BudgetExhausted,BudgetExit}` (scalar prim floor). Min-cap (EIP-150) uses two ops, enter/exit. Fuel decrements from i64::MAX; probe consumed is MAX − fuel.
- `compute.*`/`duration.*` use nominal types in the checker (a firewall) + erasure to Int(ns) in lowering.

<a id="仕様からの-deviation--全件解消2026-08-02-深夜"></a>

### Deviations from the Spec — **All Resolved (Late Night 2026-08-02)**

1. ~~No metered clones~~ → **resolved** (T1-2, de7c2d20): budget-only mode meters only regions + `__fuel` clones; zero charges on non-region paths are mechanically checked in both artifacts. Remaining: on FuncRef reachability, `__lambda_*` is charged globally (tables cannot be duplicated — promoted to a spec note).
2. ~~No saturation/negative-value trap~~ → **resolved** (T3-1/T2-5): constructor guards + S3 saturation algebra, pinned unit-exactly by fixtures.
3. ~~Single-call restriction~~ → **resolved** (T2-1 blocks, T2-2 Result arms, T1-3 bare form).
4. ~~Cannot cut divergence~~ → **resolved** (T1-1): strict cut with check-and-return. A `while true` body is cut by the budget, and the visibility window (until the next charge site) is deterministic (inlining widens the window but equally on both legs; fuel_trap_window pins it).
5. ~~No UFCS diagnostics / S6 gate~~ → **resolved** (T3-2 / T3-3).
6. ~~fan{} × budget undefined~~ → **resolved** (T3-8 ruling: excluded by the type system).

<a id="stage-3-垂直スライス--fanmin-が両ターゲットで着地2026-08-02同-branch"></a>

## Stage 3 Vertical Slice — fan.min Lands on Both Targets (2026-08-02, Same Branch)

`fan.min(budget?) { arm; arm }` works on native v1 / wasm, realizing the **proven lockstep ≡ (spend, index) lex-min semantics**:

- Winner = completed arm with minimum consumption (cheap(7)=~2 units beats heavy(2000)=~2003 units)
- **Ties use source order** (two arms with equal spend → arm 0)
- Budget changes only the candidate set (all eliminated → ledger-constant Err → `??` fallback; filtering → surviving arm)
- **Winner-emergence boundary:** heavy(500) spends 502 units. `compute.us(501)` eliminates all; `compute.us(502)` makes arm 1 the winner — the same 1µs point on both targets (asserted in the gate)
- Probe-enabled runs also match on all three points (consumed 9018 / identical trace)
- For legacy `fan.min(thunks)`, the parser reconstructs the old AST and emits **E027 as a signature-migration hint** (revision as designed; fixtures updated)

<a id="実装形"></a>

### Implementation Shape

- Each arm becomes a metered region through `outline_metered_arm` (the same outliner as Stage 2 bounded). Besides the verdict, BudgetExit **persists spend** ($__b_spend / new prim BudgetSpend), which the caller reads for each arm.
- Winner selection is a **sequential fold of scalar if-values** (candidate = not exhausted; better = no winner or strictly smaller spend — ties naturally use source order). Fused `?? fb` is fully scalar, with zero Result values, and passes the native rung.
- The unbudgeted form uses the i64::MAX sentinel (no divergence guard = the same termination convention as fan {}).

<a id="stage-3-の-deviationstage-2-の-6-件に追加"></a>

### Stage 3 Deviations (In Addition to the Six in Stage 2)

7. ~~Conservative traps~~ → **resolved** (T1-1): a losing trap outside the visibility window is treated as if it never happened (fuel_trap_cut); one inside the window fails identically on both legs (fuel_trap_window).
8. ~~No Err skipping~~ → **resolved** (T2-2, fuel_race_err_skip).

<a id="p0-修正-2-件2026-08-02同-branch"></a>

## Two P0 Fixes (2026-08-02, Same Branch)

1. **Fixed code destruction by fmt:** the formatter wildcard was **deleting code** by replacing `fan.bounded` / `fan.min` with `/* unformatted */` (data-loss class). Added formatting arms: bounded inline, race as a block with one arm per line. Confirmed roundtrip (fmt → reparse green) and idempotence.
2. **Empirically recalibrated CM-1 (v0.1 → v0.2):** the draft 1000ns/unit was **21 times too large** relative to measurement, violating ADR-0001 D5's own declared band (5×). Reference measurement: heavy(1000) = 1002 units took 47.0µs in release → 46.9ns/unit. Pinned to **50ns/unit** (band ratio 1.07). Bounded passes at `compute.us(51)`, and race gets a winner at `compute.us(26)` — the new flip points are also identical across targets (gate updated). Lesson: honestly calling it “ms” depends on a single constant, and a permanent CI D5 calibration gate is required before merge. The constant appears in three places (charge_probe constant / wasm BudgetEnter render / native BUDGET_SHIM); single-sourcing is for the full-implementation PR.
   **Afterword (v0.2 → v0.3):** that 47µs reference measurement was itself contaminated: a 1002-unit execution is µs-scale, not resolvable through ms-scale process spawning. As soon as the D5 gate became permanent (T3-7), it failed at ratio 0.05 (≈18× too large). A min-of-3 measurement of heavy(100M) = 1e8 units (native 0.25s / wasmtime 0.28s — **both agree at ≈2.5ns/unit**) repinned it to **3ns/unit**. Boundary fixtures were strengthened to 1-unit precision with ns constructors: bounded flips at exactly 3006ns (=1002×3) and race at 1506ns (=502×3), simultaneously on both targets. The current determinism claim is that 1ns on the declared clock changes the verdict. Manual review missed both miscalibrations (21× too large → 18× too large); only gates caught both.

<a id="t1-3--native-heap-result-abires-carrier2026-08-02"></a>

## T1-3 — Native Heap-Result ABI (Res Carrier, 2026-08-02)

Bare forms (`let r = fan.bounded(...)` / race, without `??`) now work on native v1. As a byproduct, **general plain functions returning `Result[Int|Bool, String]` + match consumption also work on native v1** (the “advance of the whole rung” predicted by the ledger).

Design: shared lowering represents Result as raw blocks (`Alloc DynListStr` + Handle + address arithmetic + Store/Load, tag@4/payload@12). Native lacks a memory model, so op-by-op correspondence is impossible. The chosen design is a **native-leg-only recognition pass** (`native_result_rewrite.rs`): identify stereotyped materialize/consume windows, rewrite them into five native-only prims (ResMakeOk/ResMakeErrStr/ResTag/ResOkScalar/ResErrStr), and have the renderer map them to Rust `Result<i64, String>` (NTy::Res). **Not one byte changes on the wasm leg** (the pass runs only in the native pipeline).

- Verification alignment: preserve the balance of CallFn Ptr results / DropListStr (born/release unchanged); add Handle-like alias rules for ResErrStr to the verifier; removed result-block Alloc/Consume disappear as pairs. A complete read walker prevents erroneous deletion in the dead-op sweep.
- Bare forms outline “the entire region+wrap with free variables” (reusing T2-1) to the trackable `let r = <直接 CallFn>` form. Using a block-tail call as a match subject is untracked on both legs and walls (demonstrated in the first attempt).
- The 3006/3005ns boundary pin is equivalent even in bare form: wrapping charges not even 1 unit to the region.
- Unsupported (intentional walls retained): Result with String Ok payload (tag/payload window ambiguous with Err), Result-typed parameters, generic Result functions with mono suffixes.

<a id="fan-並列--budget-の裁定t3-82026-08-02"></a>

## Ruling on Parallel fan{} × Budget (T3-8, 2026-08-02)

Resolved the undefined state: “native fan{} uses real threads and fuel counters are thread-local — what happens when combined?” Conclusion: **the type system already excludes the combination**.

- **fan{} inside a metered region:** impossible. The region body is checked in a pure context (can_call_effect=false), while `fan {}` requires an effect context (E007). Therefore a metered region syntactically cannot cross threads, and the thread-local counter is always safe. Checker pin: `t3_8_fan_parallel_inside_metered_region_is_rejected`.
- **Metered region inside a fan{} arm:** defined and deterministic. Region enter/exit is self-contained in its execution context (its own thread counter on native, sequential globals on wasm), and per-arm verdicts have no cross-context observations. Demonstrated on wasm (arm1=Ok / arm2=exhausted). Native walls because fan{}+effect-arm is outside the v1 rung; v0 fallback hits honest rejection for missing budget prims (added in T3-1) — “no route to a wrong answer.” Opening the native v1 fan{} rung is an existing issue outside this branch.
- Note: probe / --time-report consumed reads the main-thread counter, so it excludes arm consumption in native real-thread fan{} (wasm includes it). Semantics (verdict) cannot observe this difference: doing so would require an enclosing region, impossible as above.

<a id="wave-1--fan-v2-表面統一2026-08-02同-branch"></a>

## Wave 1 — fan v2 Surface Unification (2026-08-02, Same Branch)

Resolved the remaining surface inconsistency after race/bounded became block heads (only any/settle still used thunk lists):

- **Block forms:** `a() ?? b()` (was `fan.any { a(); b() }`) / `fan.settle { a(); b() }` — the parser desugars into literal thunk-list ASTs (internal names `__any_block`/`__settle_block`), and frontend lowering normalizes the names. **Entirely unchanged from the checker onward**, immediately working on both targets. fmt re-sugars the internal names into block syntax (roundtrip and idempotence confirmed).
- **Thunk-list spelling tombstones** (E027 signature-migration hints): `fan.any([...])` / `fan.settle([...])` are removed. Two-argument mapper forms have dedicated “declared, unimplemented (Wave 2)” diagnostics. Synthetic nodes are the only producers of legacy forms.
- **Ten migrated files:** 8 spec files (5 wasm_cross + 3 lang) + 2 diagnostic fixed files. Read each pin before migrating; fan_var_thunk_list (#599 var-bound-list pin) lost its target along with the spelling, so was rewritten as a pin for “list-order determinism of mixed arm kinds.” The #514 pin in fan_pure_thunks (pure-arm Ok adapter) survives in block form. Added two new tombstone fixtures.
- **Removed async/await remnants:** deleted `ExprKind::Await` / `IrExprKind::Await` / `Decl::Fn.r#async` / `IrFunction.is_async` and all consumer arms (optimize/codegen/interp/fmt/mir, about 40 files). Strengthened “cannot be written in the grammar” to “cannot be represented in AST/IR.”

The settle block return type in v1 remains the legacy `List[Result]` (the tuple contract in fan-v2.md was added as a deviation item for the full-implementation PR).

> **Afterword (2026-08-03):** The two points above describe Wave 1, and both were subsequently resolved. The settle tuple contract was implemented by T2-4 (real FanSettle node); `let (ra, rb) = fan.settle { a(); b() }` works. For mapper forms, any/settle landed in T2-3, **race in T7-1 (same day, implemented at user direction)**. Every matrix cell is now implemented (11) or absent by design (bounded/timeout mappers, 2); `tests/fan_surface_matrix_test.rs` (every-cell gate for 6 heads × 2 forms) mechanically locks that state. Misfiring thunk migration hints were replaced in T6-1 with dedicated cell-answer diagnostics.

<a id="tier-7--race-mapper-実装2026-08-03ユーザー指示でセル閉鎖"></a>

## Tier 7 — Race Mapper Implementation (2026-08-03, Cell Closed at User Direction)

`fan.min(xs, f)` / `fan.min(budget, xs, f)` were implemented, filling the last declared fan v2 matrix cell. Semantics dynamically extend the block form's same law: arm i = `f(xs[i])` (pure one-argument lambda, Result required — ok competes, err disqualifies itself); index = list order; budget per element; winner = lexicographic minimum (spend, index); all eliminated = ledger-constant Err.

- Implementation: checker directly pins the mapper return to **Result[T, String]** (a free Err variable causes ConcretizeTypes rejection — encountered during implementation, establishing the rule). Lowering is a dynamic fold with while + list.len/get + per-element outlined metered region (outliner split into an IR-body version; lambda parameters become region parameters as free variables).
- Verification: six probes in fixture `fan_race_mapper.almd` (lex-min / ties→first index / err disqualification / all eliminated / 1ns exhaustion / bare auto-`?`) agree on wasm + interp. Native hits an honest wall because list ops are outside the rung floor (@xt-allow tracked — corpus 346 equal + 1 tracked / stale 0). Added mapper clauses to C-205.

<a id="tier-5--oracle-層の完遂2026-08-0203同-branch"></a>

## Tier 5 — Completing the Oracle Layer (2026-08-02/03, Same Branch)

Rather than stopping at the deterministic tier (bounded/race), the same branch landed the oracle layer from inception (ledger T5-1..T5-4f):

- **fan.timeout (T5-1):** cooperatively checks a wall-clock deadline at charge sites (unified interruption-point principle, directly using T1-1 check-and-return cuts). Verdict is ω-relative (R_Ω contract class, C-208 + ALS-D5). The deterministic extremes (ample deadline→Ok / divergence+tiny deadline→Err) are byte-identical on both targets.
- **ω record/replay (T5-2, executable evidence for claim 4):** ω = ordinal of the wall check where the deadline expired. Native records with `ALMIDE_OMEGA_RECORD=1`; `ALMIDE_OMEGA=<n>` **bakes it in at compile time**, so all legs replay without reading the clock. A permanent gate checks byte identity for record(native) → replay(native×2 + wasm).
- **Dojo async task bank + first measurement (T5-4, material for claim 5):** added six fan v2 / deterministic-time tasks to almide-dojo and measured using cli:claude. **6/6 pass (including retries, first-attempt 2/6)**. Every initial failure was a diagnostic gap or real bug, fixed between rounds to converge.

<a id="実測が炙り出して本体側で潰したものdogfood-ループの回収"></a>

### Issues Exposed by Measurement and Fixed in the Main Implementation (Closing the Dogfood Loop)

1. **Three diagnostics** (dojo MH-003..005): misleading thunk spelling “Missing ')'” → migration hint; `import compute` “create a file” → explicit auto-available explanation; unwrap_or on already-unwrapped values → “nothing to unwrap; place `?? <default>` on the producing call.”
2. **Checker hole (T5-4f):** `some(..)/ok(..)/err(..)/none` patterns on scalar subjects silently bound `Unknown` and fell through to backend walls. Changed to a type error + auto-`?` explanation.
3. **Broken wasm validation (T5-4d):** time-type tags of let-bound budgets reached MIR layout, misinterpreted as heap Ptr (the value was already erased to i64). Added Compute/Duration → Int to the MIR newtype eraser. An entire common form (binding budget to a variable) could not compile; the Dojo passing solutions require this fix.
4. **Native leg opened (T5-4e):** enabled use of metered effect functions across calls through five bricks (result_fns blind to lift / NativeSigs likewise / Handle+Die in err-abort windows / definition-side `Ok(..)` wrapping / interpreter timeout-prim routing). Three-way corpus (native == wasm == interp) 75/75 green; added new fixture fuel_var_budget as C-204 evidence.

**Deviation section: empty** (no Tier 5 implementation departed from the spec. Intentional v1 scope restrictions — ω encoding as a single ordinal of the first hit, settle List[Result] — are documented on the specification side in contracts and roadmap, not deviations).
