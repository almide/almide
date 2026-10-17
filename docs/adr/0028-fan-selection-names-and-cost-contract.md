# ADR-0028: Keep `fan.any` / `fan.race`, make the selection rule visible where the reader is, and scope the cost contract to a meter version

- **Status**: Proposed
- **Date**: 2026-10-17
- **Scope**: the names, diagnostics and documentation of `fan.any` and
  `fan.race`; the contract of the `Compute` meter that `fan.race` and
  `fan.bounded` select by; and the open semantic questions the 2026-10-17
  audit found. No behaviour change is made by this ADR. Each change it
  proposes waits on a ratification question in §Questions.
- **Evidence**: [fan-semantics-audit.md](../design/fan-semantics-audit.md)
  (pinned 0746eb689), [fan-cross-language.md](../design/fan-cross-language.md),
  `tests/fan_acceptance_test.rs`, issues #3778–#3783.

## Context

1. **Almide offers three of the five selection kinds.** The audit sorts every
   peer operation by what it selects:

   | Kind | What it selects | Almide |
   |---|---|---|
   | K1 | first success in input order | `fan.any` |
   | K2 | first success in real time | – |
   | K3 | first completion, success or failure | – |
   | K4 | least logical cost | `fan.race` |
   | K5 | collect all | `fan.map` / `fan.settle` / `fan { }` |

   K2 and K3 are absent on purpose: their answer depends on scheduling, which
   ADR-0024 N7 and SPEC §13.5 (theorem T9) exclude from the deterministic
   tier.
2. **Two names collide with the most widely read API.** JS `Promise.any` is
   K2 and JS `Promise.race` is K3. A model that learned JS reads
   `fan.any { a(), b() }` as "whichever succeeds first", and
   `fan.race { a(), b() }` as "whichever finishes first, even if it failed".
3. **The cost of each misreading is not the same.**
   - **`race`:** the JS use of `race` is a race between I/O calls. That use is
     refused at check time: race arms must be pure (E006), and the hint names
     the rule. Among pure arms, "least compute" is the deterministic version of
     "fastest" (SPEC §13.5), so a JS reader gets the arm they expected to win
     whenever compute tracks wall time. The remaining gap is errors. In JS a
     rejection can win the race; in Almide an Err arm disqualifies itself.
   - **`any`:** a JS reader gets *an* Ok, the earliest in the list. The value
     is acceptable by construction, so a misreading costs latency, never
     correctness.
4. **The cost value depends on the compiler.** Spend is counted at IR/MIR
   charge sites that the inliner and lowering decide (audit §8). Today two
   native routes of one build disagree with each other and with wasm (#3778).
   The current promise is "same winner/verdict on every target and every
   machine" (CHEATSHEET, C-204/C-205). It says nothing about compiler
   versions, so a reader cannot tell whether an upgrade may move a winner. It
   may.

## Options for the names

| | A. Keep `any` / `race` | B. Rename both to intent-revealing names | C. Rename `race` only |
|---|---|---|---|
| Spelling | `fan.any`, `fan.race` | `fan.first_ok` (K1), `fan.cheapest` (K4) | `fan.any`, `fan.cheapest` |
| What a JS reader gets wrong | `any`: latency only. `race`: I/O use is E006 with a hint; an Err never wins | nothing carried over; the names have no JS meaning | `any` as in A |
| Compatibility | no change | every existing `fan.any` / `fan.race` use is a deprecation warning, then an error | `fan.race` uses only |
| Migration | none | `@deprecated`-style warning with a `try:` fix-it (`fan.race` → `fan.cheapest`), a `[[deprecation]]` entry in `proofs/dialect-epochs.toml` (`since_epoch` = 13, `removal_epoch` ≥ 15), and removal in that epoch's `breaks` | the same, for one name |
| Diagnostics | add hints (below) | the deprecation, plus the same hints on the new names | both |
| Dialect epoch | none | a removal epoch | a removal epoch |
| Corpus impact | none | every Dojo task and every example using either name; CHEATSHEET and llms.txt rewritten | race uses only (fewer: race is pure-only) |
| Risk to modification survival | none | a model trained on the old names keeps writing them for the whole window | smaller |

**Diagnostics for option A** (they also apply to B and C):

- **D-a:** E006 inside `fan.race` already says the arms are pure. Extend the
  hint: "fan.race picks the arm with the least deterministic compute; for
  I/O, `fan.any` takes the first Ok in source order, and `fan.map` /
  `fan.settle` collect them all". This is the place a JS-style
  `fan.race { http.get(a), http.get(b) }` lands.
- **D-b:** when every arm of an unbudgeted race returned Err, the error text is
  today `fan.race: no branch completed within budget`, which names a budget
  the program never set. Change it to "fan.race: every branch failed" for the
  unbudgeted case. This is a ledger-constant string change (Q9).
- **D-c:** CHEATSHEET's one-line glosses for `fan.any` and `fan.race`, which
  models read first, name the selection kind in words: "first Ok in LIST
  order (not the first to finish)" and "least compute spent, not the first
  to finish".

## Logical cost: the contract to state

- **Unit.** One charge unit. It is converted from declared time by the
  versioned calibration constant CM-1 (v0.3 = 3 ns/unit,
  `crates/almide-types/src/time_units.rs:9-18`).
- **Charge sites.** Exactly the table in `docs/specs/als/deterministic-time.md`:
  - fn entry, except a loop-free non-recursive callee (C-294);
  - each loop-head evaluation;
  - bulk string concatenation at `1 + len/16`;
  - nothing else.
  #3778 shows that native and wasm each break a different row today. The fix
  is to make both legs follow the table, not to change the table (Q4).
- **Budget scope.**
  - `fan.bounded`: the body.
  - `fan.race { }`: each arm separately, with the full budget.
  - `fan.race(xs, f)`: each element separately.
  - An enclosing region min-caps; source cannot nest regions today.
- **Arguments.** In a single-call body such as `{ f(g(x)) }`, `g(x)` runs
  before the meter starts (audit §6). Whether that is the rule, or whether
  arguments are inside the region, is Q6.
- **Versioning (the new part).** The verdict and the winner are functions of
  (program, inputs, **meter version**). Within one meter version they are the
  same on every target and machine. The meter version changes when the
  charge table, its exemption rule or the calibration constant changes, and
  only then. A change is announced in the release notes with the spend
  ledger's diff. The spend ledger is the gate: a fixture corpus whose
  per-region spend is pinned (`fuel_bounded_boundary.almd`,
  `fuel_dyn_charge.almd`, and the acceptance test's `heavy(1000)` = 1002
  units), and a change in any pinned spend fails CI unless the meter version
  is bumped in the same PR.

## Decision (proposed)

1. **Option A: keep `fan.any` and `fan.race`.** Add D-a, D-b and D-c.
2. Keep K2 and K3 out of the language.
3. State the cost contract as above, including the meter version.
4. Fix #3778–#3783 toward the contracts as already written. #3780 (ordered
   operations in `fan { }`) and the open points Q5–Q8 wait on their own
   answers.

## Rationale

- **A costs nothing and loses little.**
  - **`race`:** the misreading with a wrong answer is the JS I/O race, and
    that is already a compile error at the exact spot. The hint upgrade makes
    the error teach the selection rule.
  - **`any`:** the misreading yields a correct value with worse latency.
  - **B:** removes both misreadings, but makes every existing program and
    every training example of these two names stale for a whole deprecation
    window. That is a direct loss in the metric the language exists for
    (CLAUDE.md, "modification survival rate"). The usability evaluation
    (`docs/design/fan-usability-eval.md`) can measure the size of each loss
    before anything breaks.
- **The versioned cost contract is the only honest version.** The count is
  defined over compiler IR, and passes are disabled program-wide to keep it
  still (LICM, unrolling, accumulator TRE). "Same on every target" is
  achievable, as #3778's fix shows. "Same across every compiler version" is
  not, unless the optimiser is frozen. Saying so is better than a promise the
  next inliner change silently breaks.

## Alternatives rejected

- **Add K2/K3 (`fan.first_done`).** The answer would depend on scheduling.
  ADR-0024 N7 excludes that, and the wasm leg cannot express it
  deterministically. If a host-level need appears, it belongs at the host
  boundary, like `fan.timeout`'s ω, not in the deterministic tier.
- **Make `race` K3 to match JS.** This reverses the 0.42.0 decision
  (SPEC §13.5, history). Pure arms have no wall-clock meaning that every
  target agrees on.
- **Freeze the meter across versions.** That would forbid inliner and
  lowering changes in any program with a region, and the cost would be paid
  by programs that never use `fan.race`.

## Consequences

- **Gains.** No program or training example goes stale. The two
  misreadings get a diagnostic or doc line at the point of use. A user
  upgrading the compiler can tell from the meter version whether a race
  winner may move.
- **Costs.**
  - A JS reader still sees familiar names with unfamiliar meanings; the
    diagnostics and glosses have to carry that.
  - The spend ledger is one more gate.
  - The meter version is one more number in the release notes.

## Falsifier

If the Dojo evaluation in `docs/design/fan-usability-eval.md` shows models
picking `fan.any` / `fan.race` *wrongly*, as if they were K2/K3, in more than
20% of the tasks that need K1/K4, with the D-a/D-c text in context, then
option A has failed. Revisit B or C.

## Test plan for the semantic changes

These are tests for behaviour that does not exist yet. Each one lands with
the change it gates, after that change's question is answered ○.

| Change | Test | Legs |
|---|---|---|
| Q4 / #3778: meters follow the table | `tests/fan_acceptance_test.rs`: un-ignore `a_loop_free_wrapper_costs_the_same_on_every_leg` and `a_cut_body_that_interpolates_stops_at_the_same_point_on_every_leg`; add both shapes to `spec/wasm_cross/fuel_*` with the pinned spend | native default / splice / codegen meter, wasm |
| Meter version gate | a script that recomputes every pinned spend and fails unless `METER_VERSION` moved with it | CI |
| #3779: trap order in `fan { list.map }` | un-ignore `a_trap_in_a_parallel_list_map_is_the_lowest_index_one`; run it 20 times (it is timing-forced, not timing-dependent) | native par / seq, wasm |
| Q5 / #3780: ordered ops make a fan sequential | un-ignore `fan_arms_that_share_process_state_agree_across_targets`; a codegen test that the block with `env.set` renders sequential and one without it stays threaded; the D4 note text | native, wasm |
| Q6: single-call arguments metered | `fan.bounded(compute.ns(30)) { id(heavy(100000)) }` is Err on every leg (today Ok) | 4 legs |
| Q7 / #3783: empty body | `fan.bounded(c) { }` and `fan.timeout(d) { }` are the same parse error on both targets | check, both targets |
| Q8 / #3781: wasm timeout clock + replay | `ALMIDE_OMEGA=10` cuts at the same check on wasm as on native; replace the vacuous fixture with one whose replayed output differs from the live one | native, wasm |
| Q9: unbudgeted all-fail message | `fan.race { err("a"), err("b") }` → `fan.race: every branch failed`; a budgeted all-excluded race keeps the budget message | 4 legs |
| D-a hint | diagnostic harness case: `fan.race { http.get(a), http.get(b) }` → E006 with the K1/K5 pointers | check |
| Stop starting after a trap (C-200 / ADR-0024 D6 wording) | a pure `fan.map` whose element 0 traps and whose elements 1..n would print through `eprintln`; nothing after the trap is observable | native par |

## Questions (ratification, one at a time)

Each question takes one ○ or ×. They are independent. When one depends on
another, the dependency is stated in the question.

1. **Q1.** `fan.any` / `fan.race` の名前は変えずに維持し、診断（E006 のヒント）と
   CHEATSHEET の一行説明で「入力順で最初の Ok」「計算量最小」を明示する（案A）。○×
2. **Q2.** 実時間で最初の成功（K2）と最初の完了（K3）は、今後も言語に入れない。○×
3. **Q3.** `fan.race` の勝者と `fan.bounded` の判定は「同じ meter 版の中では全
   ターゲット・全マシンで同一」と約束し、meter 版はコンパイラ更新で変わりうる
   ものとして版番号と spend 台帳で告知する。○×
4. **Q4.** #3778 は `docs/specs/als/deterministic-time.md` の charge 表に両レッグを
   合わせて直す。native はループ無しラッパーの entry を免除し、wasm は文字列連結を
   課金する。既存プログラムの勝者・判定は変わりうる。○×
5. **Q5.** `fan { }` の腕が順序付き操作（`env.set` など。ADR-0024 D3 の集合に
   `env.set` を足す）に届くとき、native でも逐次に実行する（#3780）。○×
6. **Q6.** `fan.bounded` / `fan.timeout` の本体が単一呼び出しのとき、その引数の
   評価も計量・期限の対象に含める（現状は対象外）。○×
7. **Q7.** 空の `fan.bounded` / `fan.timeout` 本体は、全ターゲットで parse error に
   する（`fan { }` / `fan.settle { }` と同じ扱い、#3783）。○×
8. **Q8.** wasm の `fan.timeout` は単調時計を読み、`ALMIDE_OMEGA` の再生にも対応する
   （#3781）。○×
9. **Q9.** 予算なしの `fan.race` で全腕が Err のときのエラー文言を
   `fan.race: every branch failed` に変える（予算ありで全腕超過の文言は据え置き）。○×

## References

- ADR-0001 (deterministic time units), ADR-0011, ADR-0020, ADR-0022, ADR-0024
- C-004, C-185, C-199, C-200, C-204, C-205, C-207, C-208, C-294, C-320
- MDN [Promise.any](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise/any) / [Promise.race](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise/race)
