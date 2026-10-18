# Fan semantics audit: what the seven fan forms do today

> Last updated: 2026-10-17 · pinned commit **0746eb689** (develop) · every
> claim cites `file:line` at that commit, a probe in
> [research/fan-audit/probes/](../../research/fan-audit/probes/) that was run
> on native and wasm, or a test in `tests/fan_acceptance_test.rs`.

This audit records the semantics that existed at the pinned commit; it does
not propose any. The decisions it led to are in
[ADR-0028](../adr/0028-fan-selection-names-and-cost-contract.md) (Accepted
2026-10-18), and the comparison with other languages is in
[fan-cross-language.md](./fan-cross-language.md).

> **Status 2026-10-18.** Since dialect epoch 14 the surface below reads
> differently: `fan.any { }` / `fan.any(xs, f)` (§4) are removed — the same
> selection is `a() ?? b() ?? fallback` and `list.find_map(xs, (x) => f(x)?)`,
> both sequential by definition; `fan.race` (§5) is `fan.min(clock)`, with the
> clock argument required, and its all-fail text is `fan.min: no branch
> succeeded within budget`. `fan.min(duration…)` is decided and tracked in
> #3788. The findings §6–§10 and the issues they name stand as
> recorded. #3778 (the meters) and #3780 (ordered arms) are fixed on 2026-10-18: the native default route reads the shared meter plan and charges `+` only, and a fan reaching an `@ordered` stdlib fn runs its arms in order on every target.

Each form was traced in order: syntax → type check → lowering → runtime
(native and wasm, pure and effect). Legs: `almide run` (native), the same
with `ALMIDE_FAN_SEQUENTIAL=1` / `ALMIDE_FAN_COST_OFF=1`, and
`almide run --target wasm`, which runs on the embedded host
(`src/cli/run.rs:511-550`). The interpreter (`crates/almide-interp`) has no
CLI entry, so its behaviour was only read in the source, never run.

## Summary

| Form | Selects | Runs after a failure? | Stops anything? | Native | Wasm |
|---|---|---|---|---|---|
| `fan { a, b }` | tuple of values; lowest-index Err | yes, every arm | no | threads (effect arms too) | in order |
| `fan.map(xs, f)` | list in input order; lowest-index Err | yes, every element | no | pure lambda: pool; effect: in order | in order |
| `fan.settle { }` / `(xs, f)` | every Result | yes | no | in order (lowers to a tuple / `list.map`) | in order |
| `fan.any { }` / `(xs, f)` | the first Ok in input order | stops at the first Ok | later elements never start | in order | in order; fs-read prefetch starts every read |
| `fan.race { }` / `(xs, f)` | the Ok with least compute; tie → input order | yes, every arm | a budgeted arm is cut | in order | in order |
| `fan.bounded(c) { }` | Ok if spend ≤ budget | – | cuts the body | inline | inline |
| `fan.timeout(d) { }` | Ok if no check sees the deadline | – | cuts the body | inline | inline |

Two forms are effect-capable: the block forms and the mapper forms of `fan`,
`map`, `settle` and `any`. Four forms check their body pure (E006): both
`race` forms, `bounded` and `timeout`. Every form needs an effect fn context
(E007).

## 1. `fan { a, b }`

**Path**:
- **Parser:** `crates/almide-syntax/src/parser/compounds.rs:272-310`. It
  rejects `let`/`var` (283), loops (286), `;` (295-296) and an empty block
  (304-306).
- **Checks:** E007 outside an effect fn; E008 for a captured `var`
  (`crates/almide-frontend/src/concurrent_reach.rs:1-21`).
- **Lowering:** an arm with `!` becomes a thunk scope
  (`crates/almide-frontend/src/lower/expressions_fan_arms.rs:30-49`).
- **Native:** `mark_inline_fans` (`crates/almide-codegen/src/pass_fan_lowering.rs:82-120`)
  and `render_fan` (`crates/almide-codegen/src/walker/expressions_fan.rs:4-38`),
  with the templates `fan_expr` / `fan_single` / `fan_inline`
  (`crates/almide-codegen/templates/rust.toml:541-559`) and per-arm output
  timelines (`crates/almide-codegen/src/prelude_fan.rs`).
- **Wasm:** a sequential desugar (`crates/almide-mir/src/lower/desugar_fan.rs:555-680`).

| Item | Behaviour | Evidence |
|---|---|---|
| Success | tuple of the arm values, in arm order | test `results_and_output_follow_input_order…` |
| Error | the **lowest-index** Err, even when a later arm failed first in time | test `the_lowest_index_error…` (`block err A err`); `desugar_fan.rs:636-640` |
| Order | native: arms start together and complete in any order. Results and output are in arm order (timelines flushed in arm order, `prelude_fan.rs:9-15`). Wasm: everything in arm order. | test `results_and_output…` (`end A, end B, end C` although C finished first) |
| After a failure | every arm still runs | test (`ran C`) |
| Cancellation | none. An Err beside a non-terminating arm hangs. A trap waits for the arms below it and not for those above (ADR-0024 D6), and later arms are not prevented from starting (`prelude_fan.rs:92-107` only marks and waits). | audit probes P7–P9 (block form, in the acceptance notes) |
| Side effects | output of every arm is kept on Err. On a trap: lower arms' output and the trapping arm's partial output are kept; higher arms' output is discarded. Process state (`env.set`) is shared: **native and wasm disagree (#3780)**. | probe `A/p19_env_race.almd` |
| Empty / all-fail / divergent | empty is a parse error; all-fail returns the lowest index; divergent hangs | parser 304-306; test |
| Coverage | native threads every block with ≥ 2 arms, except an arm that captures an Rc value (`fan_inline`, sequential) or a one-arm block (`fan_single`). `ALMIDE_FAN_SEQUENTIAL` has **no effect** on blocks. Wasm runs in order, except a pure scalar `list.map` arm, which runs instance-parallel (`crates/almide-wasm/src/fan_par.rs:1-38`). | `expressions_fan.rs:15-20` |

`fan { list.map(xs, f) }` is the block form's data-parallel case:
- **Native:** `almide_rt_list_par_map` (`runtime/rs/src/list.rs:1125-1154`).
  It has **no per-element trap order**: the first trap in time wins (#3779,
  probe `A/p18_trap_order.almd`, ignored test `a_trap_in_a_parallel_list_map…`).
- **Wasm:** host worker instances (`crates/almide-wasm-run/src/host_fan.rs`).
  On a trap it reruns sequentially, so the lowest index wins.

## 2. `fan.map(xs, f)`

**Path**:
- **Check:** `crates/almide-frontend/src/check/static_dispatch.rs:202-283`. The
  callback must return `Result[B, String]`.
- **Native routing:** `crates/almide-codegen/src/pass_rust_lowering_fan.rs:496-507, 577-597`.
  A pure lambda literal over Send-safe types goes to `almide_rt_fan_map_par`
  (`runtime/rs/src/fan.rs:60-106`) with the cost model (`list.rs:303-343`).
  An effect callback goes parallel only when its only effect is a nested fan
  (`Purity::for_fan`, `tests/fan_nested_effect_parallel_test.rs`). Anything
  else uses the sequential `almide_rt_fan_map` (`fan.rs:18-38`).
- **Wasm:** `stdlib/fan_map.almd:1-27`, sequential.

| Item | Behaviour | Evidence |
|---|---|---|
| Success | `ok(list)` in input order | test `results_and_output…` (`map [36, 1050, …]`) |
| Error | the lowest-index Err (`fan.rs:24-35, 110-121`) | test (`err e2`, `err bad3` with index 6 failing first) |
| Order | parallel: block-dealt workers write slots by index. Sequential: start = completion = return = input order. | test |
| After a failure | every element runs (ADR-0024 D1) | test (`run 3, run 4, run 5` after `e2`) |
| Cancellation | none. On a trap, the parallel route still waits for lower elements (`fan.rs:97`). Workers do not stop taking blocks after a trap (`list.rs:643-680`). | — |
| Side effects | effect callbacks run in order on every leg, and every effect is kept | test `effects_already_performed…` |
| Empty / all-fail | `ok([])`; the lowest-index Err | test `empty_input…` |
| Coverage | native pure: parallel; native effect: sequential, except a nested-fan-only callback; wasm: sequential | probe timings in the acceptance notes |

## 3. `fan.settle { a, b }` and `fan.settle(xs, f)`

- **Block:** `lower_fan_settle` (`expressions_fan_arms.rs:5-28`) lowers it to a
  plain **tuple literal**.
- **Mapper:** `crates/almide-frontend/src/lower/calls.rs:65-71` lowers it to
  `list.map(xs, f)`.

Both run sequentially on every target. `almide_rt_fan_settle`
(`runtime/rs/src/fan.rs:184-203`) has no producer.

| Item | Behaviour | Evidence |
|---|---|---|
| Result | a tuple (block) or list (mapper) of every Result; a plain arm is wrapped in `ok` | test `the_lowest_index_error…` (`settle 5`) |
| Order | input order everywhere | — |
| After a failure | every element runs; a **trap** stops everything after it | probes P11/P12 (acceptance notes) |
| Empty | block: parse error; mapper: `[]` | test `empty_input…` |
| Rejected | E027 for the thunk-list `fan.settle([...])`; E007 outside an effect fn | — |

## 4. `fan.any { a, b }` and `fan.any(xs, f)`

**Path**:
- **Parse:** the block becomes `fan.__any_block([() => a, …])`
  (`crates/almide-syntax/src/parser/fan.rs:296-321`).
- **Lowering:** the block is renamed to `any` and the mapper to `any_map`
  (`lower/calls.rs:55-64`).
- **Native:** `almide_rt_fan_any` / `almide_rt_fan_any_map`
  (`runtime/rs/src/fan.rs:148-179`).
- **Wasm:** `stdlib/fan_any.almd` or the accumulator in
  `crates/almide-wasm/src/fan.rs:86-90`. An fs-read callback takes the
  prefetch lowering, `crates/almide-wasm/src/fan.rs:74-85`.

| Item | Behaviour | Evidence |
|---|---|---|
| Success | the first Ok **in input order** (not the first in time) | test `any_runs_nothing_after_the_winner…` |
| All-fail / empty | `err("fan.any: all candidates failed")`, a constant; the elements' own errors are discarded | test `empty_input…` |
| Order | start = completion = return = input order | test (`probe -1, probe -2, probe 5`) |
| After the winner | nothing after the winner starts (`probe 7` never prints) | test |
| Cancellation | unstarted elements are simply never started. Exception: on wasm, a callback that is a file read is **prefetched**, so every read is issued up front and the losers are dropped (op 42 / subtask.cancel). Reads have no visible side effect; the ADR-0024 amendment (`docs/adr/0024-…md:443-445`) permits this. | probe `B/any_fs_import.almd` |
| Side effects | the effects of every element before the winner are kept, in order | test (`ran a, ran b`) |
| Coverage | effect arms and callbacks are allowed on every target; sequential everywhere | `static_dispatch.rs:322-359` |

## 5. `fan.race { a, b }`, `fan.race(xs, f)` and budgeted forms

**Path**:
- **Parse:** `parser/fan.rs:206-273`. Arms are separated by `,` or a newline;
  `;` is rejected (probe `B/semi.almd`).
- **Check:** `crates/almide-frontend/src/check/infer_calls_closures.rs:363-462`.
  Arms and the mapper are checked **pure** in a metered region; the result is
  `Result[T, String]`.
- **Lowering:** `crates/almide-frontend/src/lower/expressions_fan.rs:273-685`, a
  sequential fold that outlines each arm into `__almd_bounded_N(budget, …)`
  and keeps the least `(spend, index)`. The comparison is strict `<` (`:435`,
  `:623`), so ties go to the earlier arm.
- **Dead runtime:** `almide_rt_fan_race` (`runtime/rs/src/fan.rs:123-142`) and
  `rewrite_race_head` (`desugar_fan.rs:305-340`) have no producer; the
  thunk-list call is E027 (`static_dispatch.rs:284-307`).

| Item | Behaviour | Evidence |
|---|---|---|
| Success | the Ok with the least spend; tie → input order | test (`race 205 105 105`, `race map 15`) |
| Err arm | disqualifies itself, even when it is cheaper | test (`race err-skip 5000`) |
| All-fail / empty / all over budget | `err("fan.race: no branch completed within budget")`, also with no budget | test `empty_input…` |
| Order | arms run one after another in input order on every target | `expressions_fan.rs:316` |
| After a winner | every arm runs; a later arm that traps aborts the program | probe `B/race_trap.almd` |
| Cancellation | a budgeted arm is cut at its next charge site and returns a zero value (`render_native_op_families.rs:39-48`, `crates/almide-wasm/src/fuel.rs:114-127`). Nothing is concurrent, so nothing waits. | probe `B/race_diverge_budget.almd` |
| Budget scope | block form: the same budget for **each arm**, evaluated once (`expressions_fan.rs:302-309`). Mapper form: per element (`:541-575`). Without a budget the cap is `i64::MAX` ns. | probe `B/race_map.almd` |
| Side effects | arms are pure (E006). Only `println`, a trap or non-termination can be observed. | test `effects_inside_race…` |
| Coverage | Int and String winners. **Float/List winners do not compile on either target (#3782).** | probes `B/rt_*.almd` |
| Cross-target | **winners can differ (#3778)** | probe `B/race_flip.almd`; ignored test |

An unbudgeted divergent arm (probe `B/race_diverge_nobudget.almd`) finishes
at once on native and runs for years on wasm. Both are cut by the same
`i64::MAX` ns cap; on native LLVM folds the side-effect-free fuel countdown
into its closed form. The answer is the same and only the wall time differs,
so this is not a divergence. A long but finite race agrees on both targets
(probe `B/race_long_nobudget.almd`).

## 6. `fan.bounded(compute.X(n)) { body }`

**Path**:
- **Parse:** `parser/fan.rs:127-159`; the body is a full block.
- **Check:** `infer_calls_closures.rs:331-357`. The body is pure, the budget
  must be `Compute`, and a Result-typed body is an error.
- **Lowering:** `lower/expressions.rs:188-226` and
  `lower/expressions_fan.rs:142-260`, which produce
  `budget_enter → body → budget_exit`.
- **Native:** the v1 MIR render by default; `ALMIDE_FORCE_METER_SPLICE=1` and
  `ALMIDE_FORCE_METER_CODEGEN=1` select the other two routes
  (`src/cli/mod.rs:240-341`). Shims are at `render_native_shims.rs:262-292`.
- **Wasm:** `crates/almide-wasm/src/fuel.rs:16-61, 119-195`.

| Item | Behaviour | Evidence |
|---|---|---|
| Success | `ok(v)` iff spend ≤ `floor(ns / 3)` units | test (`bounded at ok 496551` at 3006 ns; `under` at 3005 ns) |
| Error | `err("fan.bounded: budget exhausted")` | test |
| Order | one body, inline, on the caller's thread | — |
| On exceeding | cut at the next charge site; each enclosing fn returns a zero value up to the region | `fuel.rs:114-118` |
| Side effects | effects are E006. `println` is allowed (ADR-0022) and output before the cut is kept. | test `effects_already_performed…` (9 `iter` lines on every leg) |
| Edge cases | an empty body is `ok` on native and **E082 on wasm (#3783)**; a body with no charge site always finishes, even at a zero budget; source cannot nest regions (E007) | probes `C/e4*.almd`, `C/n1_nocharge.almd`, `C/n3_nest.almd` |
| Single-call arguments | in `{ f(g(x)) }`, `g(x)` runs **before** the meter starts (`walker/expressions_fan.rs:167-168`). This is undocumented and is ADR-0028 Q6. | probe `C/m1_meter_shape.almd` |
| Cross-target | **verdicts can differ (#3778)**: a loop-free wrapper's entry, and string concatenation | probes `C/m2_wrap_diverge.almd`, `C/concat_cut.almd`; ignored tests |

## 7. `fan.timeout(duration.X(n)) { body }`

**Path**:
- **Parse:** `parser/fan.rs:163-200`. The legacy `fan.timeout(100, f)` is
  E027 (probe `C/l1_legacy.almd`).
- **Check:** `infer_calls_closures.rs:470-496`. Same rules as `bounded`, with
  `Duration` as the type.
- **Native:** `render_native_shims.rs:164-216`, a monotonic `Instant`.
- **Wasm:** `fuel.rs:65-99`, which reads host op 34, `SystemTime` (#3781).

| Item | Behaviour | Evidence |
|---|---|---|
| Success / Error | Ok iff no charge-site check sees `now ≥ deadline`; otherwise `err("fan.timeout: deadline exceeded")` | probe `C/t1_basic.almd` |
| When checked | only at charge sites (fn entries, loop heads, concat); never during a single operation | `render_native_op_families.rs:50-53` |
| Side effects | effects are E006 (`C/e1_timeout_effect.almd`); prints are kept | test `effects_inside_race…` |
| Repeatability | not promised: the cut point depends on the machine (`C/t2_vary.almd`). Replay with `ALMIDE_OMEGA` works on **native only (#3781)**. | probe `C/t3_replay.almd` |
| Edge cases | same as `bounded`: empty body E082 on wasm, a charge-free body is never stopped | probes `C/e4b_empty_timeout.almd`, `C/n1_nocharge.almd` |

## 8. Logical cost (the `Compute` meter)

- **Unit.** One charge unit is 3 ns: `CM1_NS_PER_CHARGE`, recalibrated
  2026-08-02 and versioned v0.3 (`crates/almide-types/src/time_units.rs:9-18`).
  A budget converts as `units = ns / 3`.
- **Charge sites:**
  - 1 per fn entry;
  - 1 per loop-head evaluation (so n iterations cost n + 1);
  - `1 + len/16` per string concatenation (native default route only, #3778);
  - 1 per closure hop on the codegen meter
    (`crates/almide-mir/src/charge_probe.rs:3-4, 67-103`,
    `crates/almide-codegen/src/pass_meter_charge.rs:10-19`).
  - Stdlib bodies never charge.
  - Wasm and the codegen meter exempt the entries of loop-free, non-recursive
    user fns (`crates/almide-ir/src/meter_plan.rs:5-9, 126-130`); the native
    default route does not (#3778).
- **Budget scope.** `bounded`: the whole body. Race block: each arm gets the
  full budget. Race mapper: each element gets the full budget. Regions
  min-cap with an enclosing region; source cannot nest them.
- **The value depends on the compiler.** Spend is defined over IR/MIR charge
  sites that the inliner and the lowering decide. A compiler update can
  therefore move a race winner or a bounded verdict without any source
  change. Metered programs turn off LICM (`pass_licm.rs:38-43`), unrolling
  (`unroll.rs:8-10`) and accumulator TRE (`emit.rs:125-133`) program-wide to
  hold the count still. Today two native routes of the same build already
  disagree (#3778).

## 9. Effect admission

| Form | Effect arms / callbacks | Diagnostic |
|---|---|---|
| `fan { }`, `fan.settle { }` | allowed | — |
| `fan.map`, `fan.settle(xs, f)`, `fan.any`, `fan.any { }` | allowed (lambda or bare effect fn) | — |
| `fan.race { }`, `fan.race(xs, f)` | refused | E006 `cannot call effect function '…' inside a fan.race region` |
| `fan.bounded`, `fan.timeout` | refused | E006, hint "fan.bounded meters deterministic computation, so its body is PURE" |
| any form outside an effect fn | refused | E007 (blocks); `fan.map` gives an **uncoded** error (`static_dispatch.rs:176-180`) |
| a captured `var` | refused | E008 |

Evidence: test `effects_inside_race_bounded_and_timeout_are_refused`, test
`fan_outside_an_effect_fn_and_a_captured_var_are_refused`, probes `C/a*.almd`.

## 10. Discrepancies

There are three kinds of entry:

- **Fixed here**: a comment or doc said something false, and this PR corrects
  it with no change in behaviour.
- **Issue**: the implementation breaks a promise. These are filed, and
  nothing here changes behaviour.
- **ADR**: there is no promise yet; the question is in ADR-0028.

| # | Where | What it said | What runs | Disposition |
|---|---|---|---|---|
| 1 | `runtime/rs/src/fan.rs:8-10, 123-147` | race returns the first thunk to settle; "parallel + wall-clock nondeterministic"; race/settle keep `thread::scope` | lex-min `(spend, index)` fold in the frontend; `almide_rt_fan_race` / `almide_rt_fan_settle` unreachable | Fixed here (comments) |
| 2 | `crates/almide-mir/src/lower/desugar_fan.rs:1-6` | "`fan.race([…]) ≡ t0`"; any falls back to the last thunk's result | race head unreachable; any returns a constant Err | Fixed here |
| 3 | `static_dispatch.rs:146-147, 189-197, 205-209, 331-333` | "the removed race"; "auto-`?` propagated"; the any mapper "not yet implemented" | only the race *call* is tombstoned; auto-? is E041; the any mapper is live | Fixed here |
| 4 | `runtime/rs/src/fan.rs:41-45`, `pass_rust_lowering_fan.rs:164-165` | "String is declined on purpose"; Rc types stay sequential | String/List/Set/Map are Send-safe and go parallel (`:185-186`) | Fixed here |
| 5 | `docs/SPEC.md` §fan, `docs/specs/effect-system.md:313-317`, C-004 | `fan.race` removed in 0.42.0, only thunk[0] runs; `fan.any(thunks)` | race head and mapper are live; thunk lists are E027 | Fixed here (SPEC, effect-system); C-004 wording → als first (see below) |
| 6 | SPEC, ADR-0001:111 | `;` between race arms | `;` is a parse error | Fixed here (SPEC; ADR-0001 is a decision record, left as is) |
| 7 | SPEC §fan.bounded | "must be a single function call"; "nested budgets min-cap"; `fan.timeout` "future" | a full block body; source cannot nest; timeout exists and is pure | Fixed here |
| 8 | ADR-0024 Status | "None of it is implemented yet" | D1, D5, D6 implemented; D3/D4/step 4 not | Fixed here (status line) |
| 9 | ADR-0024 D7 | `ALMIDE_FAN_SEQUENTIAL=1` is the ablation switch | affects the par twins only, not blocks | Fixed here (note under D7) |
| 10 | `stdlib/fan_any.almd:3-4` | later elements are never evaluated | wasm fs-read prefetch starts every read | Fixed here |
| 11 | `crates/almide-interp/src/eval.rs:146-153` | an Err arm short-circuits | evaluates every arm, then the first Err (read only, not run) | Fixed here |
| 12 | `src/cli_args.rs:41-44` | `--target wasm` runs on the `wasmtime` CLI | runs on the embedded host | Fixed here |
| 13 | CHEATSHEET:691, 696 | "over scalars"; "arms are parallel siblings" over `fan.settle` | Strings/Lists too; settle is sequential | Fixed here |
| 14 | `lower/expressions.rs:270-271`, `expressions_fan.rs:479-480`, `ast.rs:263-268`, `infer_calls_closures.rs:326-330, 361` | "while-scan"; "every arm is a single pure call"; bounded body "a single call" | `ForIn`; expression arms and block bodies accepted | Fixed here |
| 15 | CHEATSHEET:728, C-204, C-205, C-294 | same winner / verdict on every target | native default meter ≠ wasm meter | Issue #3778 |
| 16 | C-200, `list.rs:1141-1142` | a trap waits for the elements below it | native `fan { list.map }` picks the first trap in time | Issue #3779 |
| 17 | ADR-0024 D3, C-004 | a fan reaching an ordered op runs sequentially | no ordered-op analysis; `env.set` races on native | Issue #3780 |
| 18 | C-208, cli.md "replay anywhere" | ω replay on every target | native only; the test is vacuous | Issue #3781 |
| 19 | CHEATSHEET:705-708, SPEC §13.5 | race is generic over `T` | Float/List winners do not compile | Issue #3782 |
| 20 | — | (nothing promised) | empty bounded/timeout body: native ok, wasm E082 | Issue #3783 |
| 21 | — | (nothing promised) | single-call arguments run outside the meter / deadline | ADR-0028 Q6 |
| 22 | ADR-0024 D4 | a note when a fan runs sequentially | no note is emitted | ADR-0028 test plan (diagnostic) |
| 23 | C-200 / ADR-0024 D6 | the substrate "stops starting elements" after a trap | nothing stops starting; only output is gated | ADR-0028 test plan |
| 24 | diagnostics | — | the clock-type mismatch and "plain value" errors have no code; one E006 cascades into E001/E041; `fan.settle(xs, f)` non-Result E001 is garbled | ADR-0028 §Diagnostics |
| 25 | `docs/specs/cli.md`, C-006, C-214 | "fan.timeout does not exist / stayed removed" | the `fan.timeout(d) { }` head exists | C-006 / C-214 wording → als first |

Rows 5, 25 and similar contract-text fixes change the *statement* of an
existing contract. `scripts/check-als-pin.sh` compares statements with the
judge ledger, so those edits go through almide/als first. They are listed
here, and none is made in this PR.
