Original description metadata: Fan v2 design and migration history; current selection surface follows ADR-0028 and SPEC
<a id="fan-v2--execution-policy-design-and-migration-record"></a>

# Fan v2 — execution-policy design and migration record

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](fan-v2.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](fan-v2.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](fan-v2.zh-CN.md)

> Non-normative reader snapshot pinned to the original commit; the original remains authoritative. No automatic synchronization or independent execution, implementation verification, proof checking or measurement. Historical commands and design instructions are documentation, not executed instructions.
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/fan-v2.md)

> Source caveat: the six-head matrix, optional race budget, thunk migration, wave backlog and MSR rationale are historical design records, not a second current grammar or measured survival-rate guarantee. The opening ADR-0028 navigation supersedes old any/race names and optional-clock rules: current min requires Compute, Duration min remains rejected/pending, timeout exists and T1 work is recorded complete. Compute determinism is meter-version scoped, not arbitrary compiler-version invariance; host-relative timeout does not promise host-independent firing decisions. Historical pure/effect and Result requirements, trap windows and min-cap must retain their stated scope. Record-form additions and pending edition migration are separate. Original examples and counts are retained without compiler/test/benchmark execution.

<a id="current-navigation"></a>

## Current navigation

The authoritative surface is [SPEC §13](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/SPEC.md#13-concurrency-fan),
with [ADR-0028](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0028-fan-selection-names-and-cost-contract.md) deciding
selection names and clocks after this design:

- `fan.any` is removed; use `??` / `list.find_map`.
- `fan.race` is renamed `fan.min`; current calls require a Compute clock.
  Duration min is an ADR-0028 design decision pending #3788, rejected by the
  current checker. Compute belongs to the deterministic tier; logical cost is scoped
  to a meter version, not invariant across arbitrary compiler versions.
- `fan.timeout(duration.ms(n)) { body }` exists; it is not a pending tombstone.
- The August T1-1/T1-2 items below were completed in
  [BURNDOWN.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/spike/charge-probe/BURNDOWN.md).
- Current accepted/rejected forms are covered by
  `tests/fan_surface_matrix_test.rs`; the historical six-head matrix below is
  not a second current grammar specification.

<a id="implementation-records"></a>

## Implementation records

> **Implementation status (worktree-stage1-charge-probe, 2026-08-02)**: surface implemented — all block heads (fan{} / any / settle / race / bounded, settle has a tuple contract), mapper any/settle, budgets (compute.* constructors + S3 algebra + min-cap), Err skipping for Result arms, bare Result forms (including native Res carrier), full block body/arms. Fixtures = spec/wasm_cross/fuel_* / time_* / fan_settle_tuple; contracts = C-202..C-207. Unfinished: strict cap + unwind (T1-1), metered-clone (T1-2) — research/spike/charge-probe/BURNDOWN.md is authoritative. When this document's tables disagree, BURNDOWN/contract ledger wins.
>
> **#1388 (2026-10-18)**: implemented unmarked-head record form `fan { a: e1, b: e2 }` — value is an anonymous record, arms run through the same Fan node as tuple form (lowered to tuple form + destructuring + record literal), contract C-386 (serial elision and first Err in source order). Result fields without `!` are E041 (leave room for a “retain as Result” interpretation through settle integration). Unfinished: retiring the `fan.settle { }` tuple head and migrating from tuple form (edition boundary).

> Original charter: [async-inception.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/async-inception.md). What follows preserves introductory design rationale and migration plans, not overriding current grammar/contracts.
>
> Design to fold fan's **entire surface** into one grammar over [logical-time-async.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/logical-time-async.md) (semantics). The [concurrency-stance.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/concurrency-stance.md) decision (deterministic data parallelism) is a premise and does not change.

<a id="導入時の統一テーゼ"></a>

## Unified thesis at introduction

**fan is execution-policy grammar.** Every fan syntax is one cell of

    head（ポリシー） × form（block = 静的 arm 列 | mapper = 動的リスト）

The retained expression means head (policy) × form (block = static arm sequence | mapper = dynamic list).

and no other shapes exist. **Eliminate all** thunks (`() => …`) from the surface.

Before v2, fan mixed 3 calling conventions: blocks contain expression sequences, `fan.map` takes (list, mapper), and `fan.any`/`fan.settle` take thunk lists. Auto-wrap contracts also differ by combinator (“race/any/settle thunks auto-wrap, map mappers do not”). v2 replaces this with **one head × form Cartesian product**.

<a id="導入時の-v2-マトリクス履歴--現行文法ではない"></a>

## v2 matrix at introduction (history, not current grammar)

| head | Selection rule | block form (static arms) | mapper form (dynamic) | Arm effect upper bound | Parallelization |
|---|---|---|---|---|---|
| (unmarked) all | All; first Err in list order | `fan { a; b }` → `(A, B)` | `fan.map(xs, f)` → `List[B]` | Effects allowed | Allowed (observations in list order) |
| settle | No selection; collect all | `fan.settle { a; b }` → `(Result[A], Result[B])` | `fan.settle(xs, f)` → `List[Result[B]]` | Effects allowed | Allowed |
| any | Successful **minimum index** | `fan.any { a; b }` → `T` | `fan.any(xs, f)` → `T` | Effects allowed | Sequential (semantics itself sequential) |
| race | Successful **minimum (spend, index)** | `fan.race { a; b }` → `T` (`fan.race(compute.ms(500)) { … }` is an optional divergence guard) | `fan.race(xs, f)` / `fan.race(compute.ms(500), xs, f)` → `T` (**implemented 2026-08-03, T7-1** — f is a pure 1-argument lambda requiring `Result`; budget per element; ties in list order) | **pure** (Rung 0) | Allowed (with pruning) |
| bounded | Meter one body | `fan.bounded(compute.ms(100)) { body }` → `T` | — (compose with map) | **pure** | — |
| timeout | Environment cuts off (wall clock — oracle tier, [ADR-0001 S8](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0001-deterministic-time-units.md)) | `fan.timeout(duration.s(5)) { body }` → `T` | — | Oracle allowed | Host-relative |

- Block-form arms are **expressions** (same arm contract as `fan {}`: Result auto-unwrap, no capture of outer `var`). any/race arms unify to the same `T`; all/settle are tuples and may be heterogeneous.
- Mapper-form mappers **return Result** (unify with the existing `fan.map` contract; extend #547's diagnostic directing pure mappers to `list.map` to every head).
- bounded / timeout take **one body** (an ordinary block, allowing `let`), not an arm sequence. Explicitly distinguish “N-arm heads” and “body heads” in the grammar table.
- Empty arm sequences and empty bodies are compile errors (inherit old race's empty-list rule).

**Inconsistent auto-wrap becomes a form-level rule**: block arms always follow `fan {}` (auto-unwrap); mappers always follow `fan.map` (Result required). No per-head exceptions remain.

<a id="thunk-全廃が買うもの"></a>

## What eliminating thunks buys

<a id="1-表面から---が消える"></a>

### 1. `() =>` disappears from the surface

```almide
// v2 導入前の v1
fan.any([() => http_fetch(a), () => http_fetch(b)])
fan.any(mirrors |> list.map((m) => () => fetch(m)))

// v2
fan.any { http_fetch(a); http_fetch(b) }
fan.any(mirrors, (m) => fetch(m))
```

The retained example's comments distinguish pre-v2 v1 from v2.

fan-concurrency.md §3.4 “Why Thunks Are Needed” explained that fan.* needed thunks for laziness **because they were functions**. In v2 every head is syntax, so that entire section disappears from the language. Dynamic forms also shrink from two stages, “build then pass a thunk list,” to one, “list + mapper,” making the code strictly shorter.

<a id="2-wasm-の-wall-クラスが-1-つ死ぬ"></a>

### 2. One wasm wall class dies

The then-current dynamic thunk lists (nonliteral) could not represent `List[funcref]` in v1. `desugar_fan.rs` inlines only literals and let-bound literals, dropping genuinely dynamic lists into a purity wall. Mapper form is “data list + one closure,” the same shape as `fan.map`, so **this wall class disappears through syntax change alone**. No semantic sacrifice.

<a id="3-予算引数がラベルを必要としなくなる"></a>

### 3. Budget arguments no longer need labels

The language has **no** labeled-argument syntax (the first logical-time-async.md draft mistakenly assumed precedent for `limit:` — corrected). Initially the design used fan heads being syntax to supply `ticks:` / `ms:` as elements of fan's own grammar, but **[ADR-0001](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0001-deterministic-time-units.md) eliminated the need for labels entirely**: budgets use unit-bearing constructors (`compute.ms(100)` / `duration.s(5)`), with the distinction carried by types. No need for parser head-argument label handling, shrinking v2's surface another step.

```almide
fan.bounded(compute.ms(100)) { body }   // 決定的時計（Compute 型）
fan.timeout(duration.s(5))   { body }   // 壁時計（Duration 型）
fan.bounded(compute.ms(n))   { body }   // 変数もそのまま
fan.bounded(100)                        // 型エラー: expected Compute, found Int
```

Full translation of the retained example's comments: deterministic clock (Compute type); wall clock (Duration type); variables work as-is too; type error: expected Compute, found Int.

The unit-name set is `ns`/`us`/`ms`/`s`/`min`/`h`, included in the gate as a completeness matrix. **Add no new literal syntax** — numeric suffixes such as `100ms` require lexer extension, and expanding lexical rules just for fan's surface is not worth it (if added later, this function form becomes the desugaring destination).

<a id="導入時に整理した各-head-の意味論後続変更は上の-current-navigation-参照"></a>

## Head semantics organized at introduction (see Current navigation above for later changes)

- **all / settle**: unchanged. C-004 / C-199 / C-200 survive with their statements intact. Arm-output buffering (list-order flush, retire C-004 EXCEPTION) is a separate workstream sharing components with Rung 1 of logical-time-async.md.
- **any**: elevate **sequential fallback**, already defined by current desugaring (match chain), to official semantics: “try in list order and **stop** at the first Ok; prior candidates' effects occur, later candidates' effects do not.” any is **fallback syntax**, not concurrency syntax — explicitly state this in the specification (for pure arms only, speculative execution is permitted as an unobservable optimization). When all fail, the Err is the ledger constant (unchanged).
- **race / bounded**: all semantics from logical-time-async.md (lockstep ≡ least-spend, per-branch budgets, trap-visibility window, EVM-style min-cap, CM-1). v2 fixes their surface as block form. **bounded + `??` becomes the primary idiom**:

  ```almide
  let plan = fan.bounded(compute.ms(100)) { optimal_plan(g) } ?? greedy_plan(g)

  let ans = fan.race(compute.s(1)) {
    exact_solve(input)
    heuristic_solve(input)
  } ?? default_answer
  ```

- **timeout (original staged plan)**: deferred until Stage 4 (Path C / R_Ω). Only the reintroduction shape is fixed here: `fan.timeout(duration.s(5)) { body }`; interruption points are charge sites (logical-time-async's unified interruption-point principle), contract class oracle-relative.

<a id="導入時の完備性規則matrix-gate-の設計"></a>

## Introductory completeness rules (matrix-gate design)

1. Every head has block form.
2. Mapper form belongs only to “heads that can produce many arms over homogeneous dynamic data” (all / settle / any / race). bounded / timeout are single-body heads and have none — intentional omission.
3. Budget argument **mandatory** for bounded (bounding is its reason to exist), **optional** for race (selection needs no budget — budgets are only divergence guards; [ticks-interface-audit.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/ticks-interface-audit.md)). timeout takes wall-clock duration. Unmarked fan / map / any / settle have no budget argument because that dimension is reachable through composition with `fan.bounded` (exactly logical-time-async's completeness rule).
4. Adding a head requires a table row with every column filled (selection rule, both form types, effect ceiling, parallelizability) for PR acceptance (API-family matrix principle).

<a id="当初の移行--何が壊れてどれだけか"></a>

## Original migration — what breaks and by how much

**Unchanged**: `fan { }` and `fan.map(xs, f)` — the 2 most-used surfaces do not change a character. Contracts (C-004/C-005/C-199/C-200) survive with statements intact.

**Breaking**: thunk-list forms `fan.any(thunks)` / `fan.settle(thunks)`. Make E0xx migration tombstones (check-time, with rewrite examples). Do not keep them alongside the new forms — two spellings for one meaning are a kind of debt this repo has repeatedly paid.

Blast radius measured: repository usage is **8 spec files, ~60 lines** (stdlib 1 comment, CHEATSHEET 0). Read what each fixture pins before migrating — as learned from removing `fan.race` (#1024), mechanical substitution produces “passing tests that assert the wrong thing.” Update C-005-family statements to new-form spelling (behavior unchanged, so contract claims unchanged).

<a id="却下したものv2-の輪郭は否定側で確定する"></a>

## Rejected options (v2's shape is fixed by exclusions)

| Candidate | Decision | Reason |
|---|---|---|
| `fan.rush` (returns fastest, losers keep running) | **Permanently rejected** | Syntax that keeps leaking losers' effects after selection has no meaning in the deterministic model. An application of “anything to which list-order meaning cannot be assigned does not enter the language” |
| `fan.spawn` / `fan.link` | Future horizon | Unstructured concurrency/channels wait for logical-time-async's “future horizon” (deterministic concurrency) foundation. Not in v2 |
| General labeled arguments | Not introduced | A language-wide calling-convention issue, not smuggled in through fan. ADR-0001 removed even the need for budget labels, so this question disappeared |
| Trailing lambda (`fan.map(xs) { x => … }`) | Not introduced | Merely gives 2 spellings, no added meaning |
| `fan.map(xs, limit: n, f)` | Not included | Never implemented (fan-concurrency-next's ✅ stale; checker fixed at 2 arguments). An observation-preserving scheduling hint needs demonstrated need, then a separate mechanism such as pragmas |
| `fan.all { }` (alias of unmarked form) | Not introduced | Most frequent form should be shortest; aliases are coexistence debt |

<a id="msr--v2-が生存率に効く形"></a>

## MSR — how v2 helps survival rate

- **Shallower decision tree**: “static sequence (block) or data-generated arms (mapper)” × “need all (unmarked/settle), first success (any), cheapest success (race), or just a bound (bounded).” LLM chooses only these 2 × 4 options; every cell's type is readable from the table.
- **Thunk elimination** removes the edit-breakage class “forgot / added an unnecessary `() =>`.”
- **Unified arm contract** (block auto-unwrap, mapper requires Result) lets head-swapping edits (any → race, etc.) survive without relearning type contracts.
- Same result each run remains: v2 has not one nondeterministic cell (timeout is in the oracle tier, separated by contract class).

<a id="当初の実装-wave現在の未完了一覧ではない"></a>

## Original implementation waves (not today's unfinished-work list)

1. **Wave 1 — surface unification (no fuel needed, possible immediately)**: block + mapper forms for any/settle, E0xx tombstones for thunk-list forms, migrate 8 spec files, rewrite SPEC.md §13, CHEATSHEET, formatter, interp bridge, new matrix gate. Parser adds `(head-args)` and `{`/`(` branching after `fan.IDENT`. Also **remove async/await remnants from AST/IR**: lexer no longer has keywords and parser `async_` fixed false (confirmed 2026-08-01), but `ExprKind::Await` / `IrExprKind::Await` / `Decl::Fn`'s `r#async` field and vestigial fmt/interp/optimize arms remain. Upgrade “unwritable in grammar” to “unrepresentable.”
2. **Wave 2 — complete deterministic tier**: `fan.bounded` (logical-time Stage 2) → `fan.race` (Stage 3), in v2 form. E027 revision here too.
3. **Wave 3 — oracle tier**: `fan.timeout(duration.s(5)) { }` (Stage 4).

Wave 1 being independently startable is a key point of v2: surface unification does not wait for fuel semantics implementation.

<a id="反証条件"></a>

## Falsification conditions

- Parser branching on `fan.IDENT ( … ) { … }` conflicts with existing expression grammar (member call + following block ambiguity). → Should not occur while fan is reserved and parsed specially from primary; settle with a fixture at the start of Wave 1.
- A fixture loses its pin in any/settle migration. → Follow #1024's procedure (read each file's pin). 8 files is half a day's work.
- No real use emerges for race mapper form. → Keep the cell declared, implement only block form in Wave 2, and explicitly label “declared, unimplemented” in the matrix gate. (Effective 2026-08-03: `tests/fan_surface_matrix_test.rs` mechanically fixes all cells of 6 heads × 2 forms. **The same day, at user direction, the cell was closed with implementation (T7-1)** — gate has moved race mapper to implemented. bounded / timeout mappers remain absent by design. Update the gate in the same PR when changing a cell.)
- Dojo measurements show the “block or mapper” choice disadvantages LLMs. → Add a fan v2 set to the MSR task bank and measure. Measurement decides whether the design is right.
