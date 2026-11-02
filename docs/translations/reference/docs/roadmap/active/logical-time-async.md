Historical description: The async grammar: fuel as the logical clock, deterministic race, oracle tier.
<a id="logical-time-async--the-async-grammar-design"></a>

# Logical-Time Async — the async grammar design

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](logical-time-async.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](logical-time-async.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](logical-time-async.zh-CN.md)

> Non-normative fixed-source historical design reader. The pinned original is authoritative and unchanged; no automatic synchronization, new implementation or independent benchmark/proof rerun. Original race/thunk proposals, the later block form, the superseded optional budget and current fan.min/timeout references remain separate layers; consult the source’s SPEC/ADR authorities for the current contract. Novelty, uniqueness, deterministic behavior and zero global instrumentation overhead are the source design’s claims, not independently established guarantees here. Paper/Lean/model-check claims do not prove the entire current runtime, Duration oracle selection or invariance across meter versions and lowering changes. Stage plans, purity restrictions, stack exclusions and oracle boundaries remain as written. Preserved code sketches are historical, not executable authorization. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/logical-time-async.md)

> Original design charter: [async-inception.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/async-inception.md). This document records the rationale and correction history of the logical-time design. The authoritative current surface and clock contract are [SPEC §13](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/SPEC.md#13-concurrency-fan) and [ADR-0028](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0028-fan-selection-names-and-cost-contract.md). The `race` below refers to the original logical-cost selection model; it does not prove Duration oracle selection or the entire current execution system.
>
> Answers to the four questions in [deterministic-bounds.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/deterministic-bounds.md): budget allocation, winner selection, cancellation, and effect isolation. Revises exactly one conclusion of [concurrency-stance.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/concurrency-stance.md) (#1000). This is a design document; implementation proceeds in the order of the staging section.
>
> **Proof ledger**: [logical-time-proofs.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/logical-time-proofs.md) fixes the validity of these semantics at three layers (paper theorems / Lean kernel-check / exhaustive model checking). It also records three corrections arising from proof work (marked below).

<a id="設計テーゼ"></a>

## Design thesis

**Wall time does not determine observations. The language's time basis is logical time (fuel). Environmental time enters only as a declared oracle input.**

In one line:

> Run Verse's `race` on EVM gas time, put losers outside the observation window, and make that hold within the native ⇄ wasm observational-equivalence contract.

All the components have precedents — deterministic interruption by fuel (Wasmtime fuel / EVM gas), logical time and declaration-order tiebreaks (Verse / Lingua Franca / Esterel), determinism through restrictions (Par monad / LVars). What is missing is the composition: **we have not found a language defining fuel-metric race, including loser non-observation and nested budgets, within a cross-target observational-equivalence contract**. Because `fan` is compiler-known syntax rather than a library, the compiler can own all three of budget allocation, winner selection, and effect visibility. That is the entire rationale of this design and the claim it puts forward as a 2026 language specification.

<a id="表面--追加は-2-つキーワードはゼロ"></a>

## Surface — two additions, zero keywords

> **Shape revision**: the surface shape (functions taking thunk arguments) was replaced by the block form in [fan-v2.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/fan-v2.md) — `fan.bounded(compute.ms(100)) { body }` / `fan.race { arm; arm }`. All semantics in this document (fuel, lockstep, trap windows, nesting, CM-1) continue to apply unchanged.
>
> **Addendum (2026-08-01, later superseded)**: race's budget was made **optional** at that time — selection needs no budget; the budget is only a divergence guard (specialization to n = ∞; termination is treated as in the existing fan {} specification). Audit: [ticks-interface-audit.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/ticks-interface-audit.md). Current `fan.min` requires a clock under ADR-0028; that optional form is not the current API.

```almide
fan.bounded(compute.ms(100)) { body }   // Result[T, String] — n fuel 以内に完了すれば Ok
fan.race(compute.ms(500)) { arm; arm }     // Result[T, String] — 各枝に n fuel、最小消費の成功が勝つ
```

The original oracle-tier proposal (now `fan.timeout(duration.ms(n)) { body }`; the following thunk form and tombstone state are historical sketches):

```almide
fan.timeout(ms, thunk)        // 環境相対。R_Ω 契約クラス。それまで tombstone 維持
```

<a id="fan-族は選択メトリクスの代数になる"></a>

### The fan family becomes an “algebra of selection metrics”

| Syntax | Selection rule | Basis of determinism |
|---|---|---|
| `fan { }` / `fan.map` | All (list-order join-all, first Err) | C-004 / C-199 |
| `fan.any` | Success with **minimum index** | List-order scan |
| `fan.race(d)` | Successful **lexicographic minimum (spend, index)** | fuel + source-order tiebreak |
| `fan.settle` | No selection (collect all) | List order |

`any` minimizes index; `race` minimizes fuel and breaks ties by index. The family has a single axis — “what to minimize” — and closes as a surface rather than a collection of point additions.

**Completeness rule** (stated in a form suitable for the matrix gate): the fuel dimension reaches every combinator through composition with `fan.bounded` (for example, `fan.map(xs, (x) => fan.bounded(compute.ms(100)) { f(x) })`). **Only** `fan.race` integrates fuel as a builtin because its selection rule **consumes** the metric (compares spend across arms). User composition cannot express cross-arm comparison. Omitting dedicated forms for `bounded × {map, any, settle}` is intentional; gate the rule itself.

<a id="なぜ予算に単位を必須にするか"></a>

### Why budgets must carry units

The lesson of 0.29.0 was that no human reads the 1000 in `fan.timeout(1000)` as anything other than milliseconds. `fan.race(1000, ts)` retains the same risk. Requiring a label puts the unit name at every call site and kills bare-integer unit misreadings through types (final form in [ADR-0001](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0001-deterministic-time-units.md)).

**Correction (2026-08-01)**: the first draft cited `fan.map(xs, limit: 16, f)` as a precedent for labeled arguments; this was **wrong**. `limit:` is not implemented (the checker requires exactly two arguments for fan.map), and the language has no labeled-argument syntax — the status table (✅) in fan-concurrency-next.md was stale. The label comes from the block grammar in [fan-v2.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/fan-v2.md): a fan head is syntax, not a function call, so it can accept unit-bearing literals directly without introducing a general labeled-argument mechanism.

<a id="なぜ-fanrace-の名を再利用するか"></a>

### Why reuse the name `fan.race`

The reason for removal in 0.42.0 was “under a deterministic model there is no meaning that can be assigned to the name race.” **Without a metric, that remains true.** Fuel supplies a metric, giving race a deterministic meaning for the first time. Two reasons not to change the name:

1. **The LLM prior.** A model trying to write concurrent fastest-selection will invariably write `fan.race`. A new name (such as `fan.least`) fights the prior — a step backward for MSR.
2. **The signature rejects the old form.** Old `fan.race(thunks)` has one argument; the new form has two, including a unit-bearing budget. Rewrite the E027 tombstone from “removed” to “signature migration hint” (add a unit-bearing budget; semantics select the least-spend winner). This also satisfies the rule that a tombstone's migration destination must be a live surface.

<a id="意味論"></a>

## Semantics

<a id="論理時計--charge-event-と-cm-1"></a>

### Logical clock — charge events and CM-1

- **Charge sites** are basic-block entries in shared MIR. Static cost is the number of semantic ops in the block. Variable-cost intrinsics (list concatenation, string concatenation, construction of size n, etc.) add **dynamic charges** (`1 + サイズ比例項`) within the site. Here the preserved Japanese literal means 1 plus a size-proportional term.
- **Attach charges when MIR is born, before optimization; every later pass preserves the annotations.** Even when a renderer peephole fuses ops, their charges remain as a sum. Thus:
  - Annotation preservation is a requirement of this design, not a proof of optimization invariance of the whole implementation. The current cost contract is limited to a meter version; inlining/lowering changes may change costs or winners (ADR-0028).
  - RC increments/decrements, MakeUnique, moves, and ANF bookkeeping are **free** (products of optimization, not source-semantic costs). This has the same shape as defining EVM gas independently of JIT speed.
- **The cost table is a versioned semantic object** (`CM-1`) in the contract ledger. Changing constants is a semantic change and may alter which arm wins or what exhausts. Versioning is therefore not decorative. Draft v1 constants: all op weights 1, calls cost 1 + callee body, variable-cost intrinsics cost `1 + ⌈size/16⌉`. **Finalize the constants in the same PR as Stage 2 implementation + fixtures.** Only the shape is fixed here (linear in size, constants managed in the ledger).
- **Arithmetic**: budgets are `Int`; internal counters use saturating arithmetic. A charge c exhausts when `remaining < c`. `n <= 0` exhausts at the first charge (total semantics without a validation branch).

<a id="中断点の統一原理"></a>

### Unifying principle for interruption points

**Charge sites are the language's only interruption points.** Execution between sites is atomic. Fuel syntax reads counters at sites; future oracle syntax (`fan.timeout`) reads the environment at the same sites. The only difference between deterministic and oracle tiers is whether an interruption point reads a function of the program (fuel) or environmental input (clock). Both tiers share the locations of interruption points, and these locations are identical on both targets.

<a id="fanboundedcomputems100--body-"></a>

### `fan.bounded(compute.ms(100)) { body }`

- The thunk is **pure** (effect constraint: no effect fn calls or oracle effects). The syntax itself is usable only inside effect fn under the existing fan rule (keep a single rule).
- Meaning: if the thunk's spent fuel `s` satisfies `s <= n`, return `Ok(値)`; otherwise return `Err` with the ledger-defined message. The Japanese value in the preserved literal denotes the returned value. A thunk returning Result follows the existing auto-wrap contract (as race/any/settle do).
- **Traps propagate.** Bounded is not speculation. Division by zero aborts the program as usual (consistent with the philosophy of C-200).
- Exhaustion at this bounded/selection interface has a String-error contract. That does not restrict Result error types throughout the language to String. Check messages within the scope of the corresponding contracts and fixtures.

<a id="fanracecomputems500--arm-arm---lockstep-定義"></a>

### `fan.race(compute.ms(500)) { arm; arm }` — lockstep definition

**Definition (lockstep)**: every arm advances by 1 fuel per logical tick. The arm that first completes (returns a successful value) at tick `s` wins. Events in the same tick resolve in source order (list-index order). A race runs for at most `n` ticks.

**Characterization (the form used by implementations)**: let each arm j terminate at `end_j = min(spend_j, trap_j, n)` (the earliest of completion, trap, or arm-budget exhaustion). Then:

- **Winner** = lexicographic minimum `(spend_j, j)` among completed arms. If none exists, return the ledger-defined `Err`.
- Arms whose value is `Err`, and exhausted arms, are not candidates (symmetric to `any` skipping failures; race is the **successful** least-cost result).

Completion tick in lockstep equals the arm's spend, so the two definitions agree. Lockstep is the picture told to users (as in Verse, “source order when simultaneous”); least-spend is the equation with the scheduler removed. **This agreement is precisely the content of “race without physical time.”**

**Trap visibility window — decisive-event rule** (*correction 2026-08-01, arising from proof work*: the original window rule defined only cases with a winner, leaving no-winner + trap cases undefined. Unify them under this single rule):

> Order Complete / Trap terminal events in merge order (lexicographic (cumulative fuel, arm index)). **The first decisive event is the sole arbiter.** Complete makes that arm the winner; Trap aborts the program with that trap; no decisive event yields `Err(exhausted)`.

The initial window rule (`t_j < s*`, or `t_j = s*` and `j <` winner index) follows as a corollary for the winner-present case. Traps outside the window are treated as never having occurred (the arm disappears at the decisive event's tick). Speculation does not silently swallow bugs (a trap inside the window always aborts), while losers after winner determination disappear entirely. The difference from C-200 (fan{} sibling-trap propagation) is not syntax but **the guarantee being bought: join-all needs everything, race needs only one**. That difference directly becomes the difference in trap rules.

**Cancellation is an optimization.** Observations consist only of the chosen arm value, the trap window, and (under nesting) spent fuel. Native is free to choose when to stop loser threads. Sound pruning: given the current best `(s*, i*)`, arm `j < i*` needs to run only up to budget `min(n, s*)`, and arm `j > i*` only to `min(n, s* - 1)` (source-earlier arms win ties). These caps exactly cover the trap visibility window: traps reached within the cap are visible; those beyond are not. Pruning's semantic invariance follows by construction.

<a id="予算の入れ子--evm-式-min-cap"></a>

### Nested budgets — EVM-style min-cap

The effective budget of nested `fan.bounded` / `fan.race` is `min(自身の n, 外側の remaining)` (the preserved literal means the minimum of its own n and the outer remaining budget). Inner charges simultaneously debit all outer budgets too, because it is the same execution. If the inner computation exhausts under an outer-derived cap, the outer continues with remaining ~0 and exhausts at its next charge. No region-tagged propagation mechanism is needed — everything is arithmetic over a shared charge trace, and equality across targets follows from trace equality. This has the same shape as the EVM subcall gas cap (EIP-150).

**Consumption charged by race to the outside** (*correction 2026-08-01, arising from proof work*: the initial `Σ_j min(end_j, s*)` overcounted at the boundary — the time = s\* charge of arms with indices after the winner follows the winner's completion in merge order and never happens. Also, “an atomic charge at the race site” cannot define the adjudication point when the outer budget expires inside the race):

> The outer region observes the race's **occurred stream** — charge events preceding the decisive event in merge order (+ the winner's own charge) — **streamed directly in merge order** as consumption. If the outer remaining budget expires midway, the outer becomes Exhausted at that point (race abandoned). If the race completes, total consumption is the sum of the stream.

This is a function of per-arm traces, and a pruned implementation can reconstruct it exactly using only data revealed by capped execution (the cap covers every event in the visible window — Lean `cap_admits_window`). **Debit semantic consumption from the counter, not the work the implementation physically performed** — the same “semantic cost” principle that makes RC free.

<a id="効果隔離の梯子"></a>

### Effect-isolation ladder

- **Rung 0 (v1)**: race / bounded arms are pure, checked directly by existing purity machinery. Typical uses: algorithm portfolios (exact solutions vs heuristics, parsing with two grammars, SAT-style portfolios). On native, parallel execution with pruning also improves wall time.
- **Rung 1 (later)**: allow arms with stdout/stderr only, **flushing only the winner's output after resolution** (discard losers). This is the same mechanism as arm-output buffering deferred in #1026, sharing components with retirement of C-004's EXCEPTION section.
- **Permanently disallowed**: oracle effects (fs / http / process / random / env) in race arms are compile errors. **In an I/O race, the environment chooses the winner**, so it cannot in principle belong to the deterministic tier. The diagnostic says: “racing I/O is environment-dependent — restructure, or use the oracle-tier fan.timeout (Stage 4).”

Almide's effect discipline (var capture E008, mut arguments E007, single error channel) already divides effects needing isolation into {pure | output | oracle}. Much of what Verse attempted to solve using transactional effects **does not exist here in the first place, by typing**.

<a id="deterministic-bounds-の-4-設問への回答"></a>

## Answers to the four deterministic-bounds questions

| Question | Answer | Deciding factor |
|---|---|---|
| 1. Budget allocation | **Per-branch: n for every arm** (not deterministic partitioning of a shared counter) | Partitioning makes adding an arm change existing arms' budgets. The moment an LLM adds a third arm, the first starts exhausting — directly breaking modification survival. Per-branch localizes arm semantics |
| 2. Winner selection | **lockstep ≡ lexicographic minimum (spend, index)** | Removes the scheduler from the definition. The only form satisfying both Verse UX (source order when simultaneous) and EVM determinism |
| 3. Cancellation | **Optimization; observations are chosen value, trap window, consumption only** | Pruning caps (`s*` / `s* - 1`) coincide with the trap window, so pruning preserves semantics |
| 4. Effect isolation | **v1 pure; Rung 1 transactional output; oracle permanently rejected** | Existing effect discipline handles most isolation through types |

<a id="型付け診断表面変更"></a>

## Typing, diagnostics, and surface changes

- `fan.race(compute.ms(500)) { arm; arm } -> Result[T, String]`, `fan.bounded(compute.ms(100)) { body } ->
  Result[T, String]`. Thunk auto-wrap follows the existing race/any/settle contract.
- New diagnostics: (a) effect call in race/bounded arm → “branches must be pure” + the oracle hint above; (b) bare integer → unit hint; (c) empty thunk list → compile error as with the old race.
- **E027 revision**: from “removed” to “signature migration.” The destination becomes a live surface. Leave the `fan.timeout` tombstone hint unchanged (directing to the host boundary) until Stage 4.
- Add `fan.race` / `fan.bounded` to the unknown-fan-fn Available list (in the implementation PR).

<a id="クロスターゲット-lowering"></a>

## Cross-target lowering

- **Meter only within bounded regions.** Specialize the call graph reachable from the region into fuel variants (an extra dimension of the same mechanism as mono). Global metering overhead is zero. Explicitly leave treatment of metered slots in closure tables unresolved until implementation (the design answer is “closures that can flow into a region have a metered variant”; cost is proportional to the size of code raced).
- **wasm (sequential)**: list-order scan. Execute each arm under a cap while updating best `(s*, i*)`. **Record** intermediate traps as `(t_j, j)` **and continue scanning**; after every arm's end is known, evaluate visibility and rethrow (a later arm may produce a smaller `s*`, putting the trap outside the window; do not abort immediately).
- **native (parallel)**: one thread per arm, traps caught and recorded through checked lowering (a loser may physically reach a trap before the winner is determined, so delay it and check the window after selection). Caps shrink as best is updated.
- Both compute the same observations (chosen value, trap window, consumption). These are what fixtures compare.

<a id="契約とゲート"></a>

## Contracts and gates

1. Add **CM-1** as a versioned object in the ledger. Changes to cost constants = semantic changes.
2. New contracts (numbers assigned in the implementation PR): bounded exhaustion equivalence, race winner equivalence, trap visibility window, nested min-cap and race consumption. Each gets a `spec/wasm_cross` fixture comparing three items: `result` + `consumed_fuel` + **charge-event trace** (equal totals alone miss ordering divergence, as deterministic-bounds noted).
3. **Charge-trace preservation validator**: checks that every charge event reaches emit exactly once through lowering; an object of the same kind as ownership certificates. Fixtures are falsifiers; the validator is proof-shaped evidence.
4. Property tests over generated MIR (trace preservation for random MIR).

<a id="既知の境界隠さず書く"></a>

## Known boundaries (stated openly)

- **Stack**: fuel also limits recursion depth through charges (calls charge too), but with huge budgets, differences in native / wasm stack limits may be encountered before fuel exhaustion. This is a boundary already present throughout the language; this design neither improves nor worsens it. Explicitly outside the contract's observation scope.
- **Spent fuel is not public**: do not return `spend` as a value (doing so would make CM constant revisions change values of every program). The cost model affects observations only through “who wins” and “whether exhaustion occurs”; fixtures compare internally through emit-time Σ-probes (an existing evidence class).
- **Single error channel**: distinguishing exhaustion depends on message constants. Richer error types are a language-wide question, not something to solve point-wise.

<a id="系譜との対応--何を借り何が新しいか"></a>

## Relationship to predecessors — what is borrowed, what is new

| Lineage | Borrowed | Not borrowed / difference |
|---|---|---|
| Verse `race` | Winner-selection UX, “source order when simultaneous,” structured cancellation | Simulation time derives from wall-clock, not deterministic computational cost. Transactional effects only in limited form at Rung 1. Based on Epic documentation (recheck tie rules at implementation) |
| EVM gas / EIP-150 | Deterministic out-of-gas, nested min-cap, cost table = semantics | EVM has no race (only sequential transactions) |
| Wasmtime fuel | fuel vs epoch distinction (deterministic / wall-clock tiers) | CM-1 metering points and costs are an independent versioned contract. This comparison alone proves neither whole-implementation optimization invariance nor superiority over other methods |
| Lingua Franca / Esterel | Logical time, declaration-order resolution at identical logical times | Logical-time units are events, not computational cost |
| Par monad / LVars | Determinism through restrictions; naming quasi-determinism as an honest fallback | Do not address fuel, selection, or cancellation |
| Kahn process networks | (Future) theoretical foundation for deterministic channels | Outside this design; see “Horizons” below |

**The composition is new**: defining fuel-metric race + loser observation window + nested budgets together with a two-backend observational-equivalence contract and its validation apparatus (trace validator, three-part fixtures).

<a id="msr--この文法がllm-が最も正確に書けるに効く形"></a>

## MSR — how this grammar serves “LLMs write it most correctly”

- **Only one decision point is added**: “Is this speculation?” Race for speculation, bounded for a sequential bound. There are still no async/await/Future/task handles and zero new keywords.
- **Per-branch budgets** exist for modification survival. Adding, deleting, or reordering arms does not change other arms' budgets. Source-order tiebreaks fix outcomes in a way visible in diffs.
- **Unit-bearing literals** eliminate the class of bare-integer unit misreadings through types.
- **The same on every run**: tests containing race do not become flaky. Concurrent syntax fits directly into benchmarks, snapshots, and the 3-way oracle. The stance's wording that determinism is part of the definition of correctness, not a performance property, extends directly to race.

<a id="concurrency-stancemd-の改訂点1-点のみ"></a>

## Revision to concurrency-stance.md (exactly one point)

The stance's “under a deterministic model there is no meaning assignable to the name race” **remains true without a metric**. When, and only when, fuel supplies a metric, a race can be defined with observations strictly identical to sequential evaluation in list order (the scan above is that sequential evaluation). The stance principle “do not admit what cannot be given list-order meaning” is **preserved** — this design applies that principle rather than making an exception.

<a id="将来の地平本設計の範囲外方向だけ記す"></a>

## Future horizons (outside this design; direction only)

A language with a logical clock can define **deterministic concurrency** (not parallelism) on top: logical sleep, deterministic select in tick order, deterministic interleaving via round-robin at charge points, Kahn-network-style deterministic channels. If structured concurrency is revisited, this clock is its foundation. This makes concrete, for the first time, a route meeting the stance's condition “can revisit, but contract first.”

<a id="ステージングdeterministic-bounds-の-5-段に対応"></a>

## Staging (matching the five stages in deterministic-bounds)

1. **CM-1 + charge-trace preservation**: attach charges to MIR; both renderers preserve traces; validator and three-part comparison fixtures. A divergence of one unit or one position halts the program.
2. **`fan.bounded`**: sequential pure; finalize CM-1 constants and register them in the ledger in the same PR.
3. **`fan.race`**: lockstep semantics, pruning, trap windows, nested consumption. Revise E027.
4. **Oracle tier**: R_Ω contract, reintroduce `fan.timeout` (read the environment at charge points), absorb C-189.
5. **AARA over MIR**: omit metering on calls whose proven upper bound fits the budget.

<a id="反証条件"></a>

## Falsifiers

- Charge traces cannot be preserved through lowering (renderer peepholes are the main suspect). Stage 1 detects this and invalidates Stage 2 onward.
- Metering overhead is unacceptable even when region-limited. → Coarsen bounded-region granularity (combine block-level charges) or bring Stage 5 AARA forward.
- Code size of fuel specialization is impractical. → Fall back to a diagnostic restricting raced code to self-contained functions.
- Pure-only race has little real use. → Bring Rung 1 (transactional output) forward; if usefulness remains low, reassess race as sugar for “comparing N bounded computations.”

Reader gloss for the preserved Japanese code comments: bounded returns Ok if it completes within n fuel; race gives each arm n fuel and selects the successful arm with least consumption. The historical timeout sketch is environment-relative, belongs to contract class R_Ω, and retains its tombstone until that layer is available.
