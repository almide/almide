# ADR-0028: Rename `fan.any` to `fan.first_ok` and `fan.race` to `fan.min_cost`, keep the old names as tombstones that name the replacement, and scope the cost contract to a meter version

- **Status**: Proposed
- **Date**: 2026-10-17 (revised the same day: the first draft recommended
  keeping the names. A comparison with other languages' and compilers' naming
  reversed it; see §Naming in other languages)
- **Scope**: the names, diagnostics and documentation of `fan.any` and
  `fan.race`; the contract of the `Compute` meter that `fan.race` and
  `fan.bounded` select by; and the open semantic questions the 2026-10-17
  audit found. This ADR changes no behaviour. Each change it proposes waits on
  a ratification question in §Questions.
- **Evidence**: [fan-semantics-audit.md](../design/fan-semantics-audit.md)
  (pinned 0746eb689), [fan-cross-language.md](../design/fan-cross-language.md),
  `tests/fan_acceptance_test.rs`, issues #3778–#3784.

## Context

1. **Almide offers three of the five selection kinds.**

   | Kind | What it selects | Almide |
   |---|---|---|
   | K1 | first success in input order | `fan.any` |
   | K2 | first success in real time | – |
   | K3 | first completion, success or failure | – |
   | K4 | least logical cost (tie → input order) | `fan.race` |
   | K5 | collect all | `fan.map` / `fan.settle` / `fan { }` |

   K2 and K3 are absent on purpose. Their answer depends on scheduling, which
   ADR-0024 N7 and SPEC §13.5 (theorem T9) exclude from the deterministic
   tier.
2. **Both names compile silently under the wrong reading.** A writer who
   expects JS `Promise.race` (K3) gets K4 from `fan.race { a(), b() }` over
   pure arms. A writer who expects `Promise.any` (K2) gets K1 from
   `fan.any`. Neither case raises a diagnostic. Only the I/O-arm `race` hits
   E006.
3. **The cost value depends on the compiler.** Spend is counted at IR/MIR
   charge sites that the inliner and lowering decide (audit §8). Today, two
   native routes of one build disagree with each other and with wasm
   (#3778). The current promise, "same winner/verdict on every target and
   every machine" (CHEATSHEET, C-204/C-205), says nothing about compiler
   versions.

## Naming in other languages

Each row is a library's documented meaning, sorted by selection kind.

| Name | Where | Selection kind | Order guarantee |
|---|---|---|---|
| `race` | JS `Promise.race` | K3 first to settle, rejection included | none (wall time) |
| `race` | Haskell `async` `race :: IO a -> IO b -> IO (Either a b)` | K3 first to finish; **the loser is cancelled** | none |
| `select` / `select!` | Rust `futures`, `tokio`; Kotlin `select`; Go `select` (random among ready cases, by spec) | K3 | none, or random by design |
| `firstCompletedOf` / `anyOf` / `WhenAny` | Scala `Future`; Java `CompletableFuture.anyOf`; C# `Task.WhenAny` | K3 | none |
| `any` | JS `Promise.any`; Rust `futures::select_ok` (same kind) | K2 | none |
| `findAny` / `find_any` | Java `Stream.findAny`; Rayon `find_any` | any match, **explicitly nondeterministic** | none |
| `any` | Python `any()`, Rust `Iterator::any`, Haskell `any` | a Bool, not a selection | – |
| `findFirst` / `find_first` / `find_map_first` | Java `Stream.findFirst`; Rayon `find_first`, `find_map_first` | **K1** | input order |
| `asum` / `<\|>` | Haskell `Alternative` | **K1** (first success, left to right) | input order |
| `min_by_key` | Rust `Iterator::min_by_key`: "if several elements are equally minimum, the first element is returned" | **K4's exact rule** (least key, tie → first) | input order on ties |

Results:

- **`race` means "first to finish" in every ecosystem that has the word**,
  and in Haskell it also cancels the loser. Almide `race` neither finishes
  first nor cancels.
- **`any` never means an ordered selection.** It means "no order guarantee"
  (`findAny`, `find_any`, `Promise.any`) or a Bool. The ordered selection is
  spelled **`first`** everywhere: `findFirst`, `find_first`,
  `find_map_first`.
- K4's rule (least key, ties go to the first) is `min_by_key`'s documented
  contract. The established word for it is **`min`**.
- Almide has done this once before. 0.42.0 removed `fan.race(thunks)`
  because "the name promised timing behaviour the language deliberately does
  not have" (SPEC §fan, C-004). The live `fan.race` head repeats the same
  defect under the same name.

## How other compilers rename a public name

| Compiler | Mechanism | What the user sees |
|---|---|---|
| Swift | `@available(*, deprecated, renamed: "new")` | a warning carrying a one-click fix-it to the new name |
| Kotlin | `@Deprecated(message, ReplaceWith("new(...)"), level)` with levels WARNING → ERROR → HIDDEN across releases | the IDE replaces every use; the old name stays a pointed error before it disappears |
| Rust | `#[deprecated(note)]`, machine-applicable suggestions applied by `cargo fix`; editions (`cargo fix --edition`) for breaking spellings | `rustc` prints the replacement; one command migrates a crate |
| Go | `go fix` rewriters for API moves | one command migrates a module |
| Almide today | `@deprecated(since=N, use="…")` on stdlib fns (`stdlib/list.almd:18`), a `[[deprecation]]` entry in `proofs/dialect-epochs.toml`, `try:` replacements in diagnostics, and E027 tombstones with a migration hint (the 0.42 `race`) | a warning naming the replacement, then an error that still names it |

Every peer does the same three things:

1. The old name keeps compiling for a window, with a warning that carries
   the exact replacement.
2. A tool or fix-it applies the replacement mechanically.
3. The old name ends as an error *that still names the replacement*, never
   as an unknown identifier.

Almide already has each piece.

## Options for the names

| | A. Keep `any` / `race` | **B. Rename both** | C. Rename `race` only |
|---|---|---|---|
| Spelling | `fan.any`, `fan.race` | **`fan.first_ok` (K1), `fan.min_cost` (K4)** | `fan.any`, `fan.min_cost` |
| Peer precedent | contradicted by every peer row above | `find_map_first`, `findFirst`; `min_by_key` | half |
| A JS / Haskell / Java reader writes the old name | compiles silently with another meaning | warning, then E027, both naming the replacement and its meaning | `any` still silent |
| Compatibility | no change | old names compile with a warning for the window | `race` only |
| Migration | – | the `try:` fix-it rewrites the name; a `[[deprecation]]` entry (`since_epoch` 13, `removal_epoch` 14, the next removal epoch); the window's end in epoch 14's `breaks` | the same, one name |
| End state of the old name | – | a permanent E027 tombstone: "`fan.race` (first to finish) does not exist in Almide: for the least compute use `fan.min_cost`, for the first Ok in order use `fan.first_ok`" | the same, one name |
| Corpus | – | every example, CHEATSHEET, llms.txt, and the Dojo tasks using either name | `race` uses |

**Diagnostics that ship with B**:

- **D-a:** E006 inside `fan.min_cost` names K1/K5 for I/O: "for I/O,
  `fan.first_ok` takes the first Ok in source order, and `fan.map` /
  `fan.settle` collect them all".
- **D-b:** for an unbudgeted all-Err `fan.min_cost`, the text becomes
  "fan.min_cost: every branch failed". The budget wording stays only where a
  budget excluded the arms (Q9).
- **D-c:** the CHEATSHEET glosses state the kind, "first Ok in LIST order"
  and "least compute spent, tie → source order", and say what neither does:
  "not the first to finish".

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

1. **Option B.** Rename `fan.any` → `fan.first_ok` and `fan.race` →
   `fan.min_cost`, in every form: block, mapper and budgeted. Meaning is
   unchanged.
2. Migrate the way Swift, Kotlin and Rust do. During epoch 13 the old names
   compile with a deprecation warning that carries a `try:` replacement.
   In epoch 14 they become E027 tombstones, and those tombstones stay
   permanently and name both replacements and what each selects.
3. Keep K2 and K3 out of the language, and make that visible: the
   tombstone's text and the CHEATSHEET say that "first to finish" does not
   exist and why.
4. State the cost contract as above, including the meter version.
5. Fix #3778–#3783 toward the contracts as already written. #3780 and the
   open points Q5–Q8 wait on their own answers.

## Rationale

- **The comparison is one-sided.** No peer uses `race` for anything but
  "first to finish", and no peer uses `any` for an ordered selection. The
  ordered selection is `first` in Java, Rayon and Haskell's idiom, and
  least-key with first-on-tie is `min_by_key`.
- **A silent misreading is the worst case for the metric.** Under A, a model
  carrying the JS or Haskell meaning writes `fan.race` / `fan.any`, the
  program compiles, and nothing tells it that the meaning differs.
  Modification survival measures whether code does what the writer meant.
  Under B, the same habit produces a diagnostic that names the right form.
  That costs one fix iteration, and it is the cheapest way to learn the rule.
- **The window protects existing code.** During epoch 13 every existing
  program still compiles, and the fix is mechanical: a one-token rename with
  no change in meaning. This is the migration every peer compiler ships.
- **Almide made the same call in 0.42.0** for the same reason, and on the
  same word.

## Alternatives rejected

- **A, keep both names.** Every peer reads `race` as "first to finish" and
  `any` as "unordered". Keeping the names leaves both misreadings silent.
  (This was the first draft's recommendation. It weighed the cost of the
  rename window against JS alone, and did not check how far `race` and `any`
  carry the same meaning across ecosystems.)
- **C, rename `race` only.** `any` is contradicted just as consistently
  (`findAny`, `find_any`, `Promise.any`), and a partial rename leaves a
  family with one intent-revealing name and one misleading name.
- **Other spellings.**
  - `fan.first`: Scala's `firstCompletedOf` reads it as first *in time*.
    `first_ok` names both the order and the Ok.
  - `fan.cheapest`: there is no precedent for it. `min_cost` reuses
    `min_by_key`'s word and tie rule.
  - `fan.select`: this is K3 in Rust, Kotlin and Go.
- **Add K2/K3 (`fan.first_done`).** The answer would depend on scheduling.
  ADR-0024 N7 excludes that, and the wasm leg cannot express it
  deterministically.
- **Make `race` K3 to match JS.** That reverses the 0.42.0 decision, and
  pure arms have no wall-clock meaning that every target agrees on.
- **Freeze the meter across versions.** It would forbid inliner and lowering
  changes in any program with a region.

## Consequences

- **Gains**:
  - Both names say what they select in the words other ecosystems use for
    it.
  - A habit carried from another language turns into a diagnostic that
    names the right form.
  - A user upgrading the compiler can tell from the meter version whether a
    winner may move.
- **Costs**:
  - One deprecation window and one removal-epoch entry.
  - The examples, the CHEATSHEET, llms.txt and the Dojo tasks are rewritten.
  - The spend ledger is one more gate.
  - The meter version is one more number in the release notes.

## Falsifier

The usability evaluation (`docs/design/fan-usability-eval.md`) runs C1, the
old names with D-a/D-c, against C2, the new names. If C2 does not cut the
F1 + F2 + F5 misreading rate by at least a third relative to C1, the rename
is not buying what this ADR claims. Then option A, with its diagnostics,
should be reconsidered before the removal epoch.

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
| Q9: unbudgeted all-fail message | `fan.min_cost { err("a"), err("b") }` → `fan.min_cost: every branch failed`; a budgeted all-excluded race keeps the budget message | 4 legs |
| D-a hint | diagnostic harness case: `fan.min_cost { http.get(a), http.get(b) }` → E006 with the K1/K5 pointers | check |
| Q1 rename window | `fan.race` / `fan.any` in every form: a deprecation warning whose `try:` text is the new name, and the program's output is unchanged; the renamed program prints the same on 4 legs (the acceptance tests, renamed) | check + 4 legs |
| Q1 removal | at epoch 14 both old names are E027 naming both replacements; `scripts/check-dialect-epochs.sh` sees the `[[deprecation]]` → `breaks` pair | check |
| Stop starting after a trap (C-200 / ADR-0024 D6 wording) | a pure `fan.map` whose element 0 traps and whose elements 1..n would print through `eprintln`; nothing after the trap is observable | native par |

## Questions (ratification, one at a time)

Each question takes one ○ or ×. They are independent. When one depends on
another, the dependency is stated in the question.

1. **Q1.** `fan.any` を `fan.first_ok` に、`fan.race` を `fan.min_cost` に改名する（意味は
   不変・全形）。旧名は epoch 13 の間は警告と `try:` 置換つきで通し、epoch 14 で
   置換先を名指す E027 トンボストーンにする（案B）。○×
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
9. **Q9.** 予算なしの race（Q1 が ○ なら `fan.min_cost`）で全腕が Err のときのエラー文言を
   `every branch failed` に変える（予算ありで全腕超過の文言は据え置き）。○×

## References

- ADR-0001 (deterministic time units), ADR-0011, ADR-0020, ADR-0022, ADR-0024
- C-004, C-185, C-199, C-200, C-204, C-205, C-207, C-208, C-294, C-320
- Haskell `async`: [race](https://hackage.haskell.org/package/async/docs/Control-Concurrent-Async.html#v:race) · Java [Stream.findFirst / findAny](https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/util/stream/Stream.html) · [CompletableFuture.anyOf](https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/util/concurrent/CompletableFuture.html#anyOf(java.util.concurrent.CompletableFuture...)) · Scala [Future.firstCompletedOf](https://www.scala-lang.org/api/current/scala/concurrent/Future$.html) · C# [Task.WhenAny](https://learn.microsoft.com/dotnet/api/system.threading.tasks.task.whenany) · Kotlin [select](https://kotlinlang.org/api/kotlinx.coroutines/kotlinx-coroutines-core/kotlinx.coroutines.selects/select.html) · Go [select statements](https://go.dev/ref/spec#Select_statements)
- Rayon [find_first / find_any / find_map_first](https://docs.rs/rayon/1.10.0/rayon/iter/trait.ParallelIterator.html) · Rust [Iterator::min_by_key](https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.min_by_key)
- Renames: Swift [`@available(renamed:)`](https://docs.swift.org/swift-book/documentation/the-swift-programming-language/attributes/#available) · Kotlin [`@Deprecated` / `ReplaceWith`](https://kotlinlang.org/api/core/kotlin-stdlib/kotlin/-deprecated/) · Rust [`#[deprecated]`](https://doc.rust-lang.org/reference/attributes/diagnostics.html#the-deprecated-attribute) and [`cargo fix`](https://doc.rust-lang.org/cargo/commands/cargo-fix.html) · [`go fix`](https://pkg.go.dev/cmd/fix)
- MDN [Promise.any](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise/any) / [Promise.race](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise/race)
