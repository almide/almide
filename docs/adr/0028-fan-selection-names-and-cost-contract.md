# ADR-0028: Replace `fan.any` with `??` / `list.find_map`, rename `fan.race` to `fan.min` with a required clock, admit the real-time clock, and scope the cost contract to a meter version

- **Status**: Accepted (2026-10-18). Q1 was ratified on 2026-10-17 with one
  amendment (no deprecation window); every other question was decided on
  2026-10-18 by comparison with other languages' designs (§Decision). No
  question is open.
- **Date**: 2026-10-17 (proposed; revised the same day after the
  cross-language naming comparison), 2026-10-18 (accepted, with the interim
  names `fan.first_ok` / `fan.min_cost` of the first revision replaced — see
  §Alternatives rejected).
- **Scope**: the selection forms of `fan` (`fan.any`, `fan.race`), their
  diagnostics and documentation; the real-time selection the language did
  not have; the contract of the `Compute` meter that `fan.min` and
  `fan.bounded` select by; and the semantic questions the 2026-10-17 audit
  found (#3778–#3783).
- **Evidence**: [fan-semantics-audit.md](../design/fan-semantics-audit.md)
  (pinned 0746eb689), [fan-cross-language.md](../design/fan-cross-language.md),
  `tests/fan_acceptance_test.rs`, issues #3778–#3784.
- **Lands in**: dialect epoch 14 (`fan-selection-names`,
  `proofs/dialect-epochs.toml`). The real-time clock of `fan.min` and the
  I/O body of `fan.timeout` are the follow-up #3788.

## Context

1. **Almide offered three of the five selection kinds.**

   | Kind | What it selects | Almide before this ADR |
   |---|---|---|
   | K1 | first success in input order | `fan.any` |
   | K2 | first success in real time | – |
   | K3 | first completion, success or failure | – |
   | K4 | least logical cost (tie → input order) | `fan.race` |
   | K5 | collect all | `fan.map` / `fan.settle` / `fan { }` |

   K2 and K3 were absent on purpose. Their answer depends on scheduling, which
   ADR-0024 N7 and SPEC §13.5 (theorem T9) exclude from the deterministic
   tier.
2. **Both names compiled silently under the wrong reading.** A writer who
   expects JS `Promise.race` (K3) got K4 from `fan.race { a(), b() }` over
   pure arms. A writer who expects `Promise.any` (K2) got K1 from
   `fan.any`. Neither case raised a diagnostic. Only the I/O-arm `race` hit
   E006.
3. **The cost value depends on the compiler.** Spend is counted at IR/MIR
   charge sites that the inliner and lowering decide (audit §8). Two
   native routes of one build disagree with each other and with wasm
   (#3778). The promise before this ADR, "same winner/verdict on every target
   and every machine" (CHEATSHEET, C-204/C-205), said nothing about compiler
   versions.
4. **The most common real-world shape had no spelling.** "Ask two servers
   and use whichever answers first" (K2) is what every peer's `race` /
   `any` is for; the cross-language table shows it in JS, Rust, Python, Go,
   Haskell, Kotlin and Swift. Almide's answer was "not expressible".

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
| `find_map` / `find_value` / `firstNotNullOfOrNull` / `collectFirst` | Rust `Iterator::find_map`, OCaml `List.find_map`; Elixir `Enum.find_value`; Kotlin; Scala | **K1** — and in every case a LIST/iterator function, never a concurrency primitive | input order |
| `findFirst` / `find_first` / `find_map_first` | Java `Stream.findFirst`; Rayon `find_first`, `find_map_first` | **K1** | input order |
| `asum` / `<\|>` / `or_else` / `.catch(() => b())` | Haskell `Alternative`; Rust `Result::or_else`; JS | **K1 over two or three candidates**: a lazily-evaluated fallback chain | input order |
| `min_by_key` | Rust `Iterator::min_by_key`: "if several elements are equally minimum, the first element is returned" | **K4's exact rule** (least key, tie → first) | input order on ties |

Results:

- **`race` means "first to finish" in every ecosystem that has the word**,
  and in Haskell it also cancels the loser. Almide `race` neither finished
  first nor cancelled.
- **`any` never means an ordered selection.** It means "no order guarantee"
  (`findAny`, `find_any`, `Promise.any`) or a Bool.
- **The ordered first-success is a list function everywhere**, and over a
  handful of candidates it is a lazy fallback chain. No peer gives it a
  concurrency combinator, because it is sequential by definition.
- K4's rule (least key, ties go to the first) is `min_by_key`'s documented
  contract. The established word for it is **`min`**.
- Almide had done this once before. 0.42.0 removed `fan.race(thunks)`
  because "the name promised timing behaviour the language deliberately does
  not have" (SPEC §fan, C-004). The live `fan.race` head repeated the same
  defect under the same name.

## How other compilers rename a public name

| Compiler | Mechanism | What the user sees |
|---|---|---|
| Swift | `@available(*, deprecated, renamed: "new")` | a warning carrying a one-click fix-it to the new name |
| Kotlin | `@Deprecated(message, ReplaceWith("new(...)"), level)` with levels WARNING → ERROR → HIDDEN across releases | the IDE replaces every use; the old name stays a pointed error before it disappears |
| Rust | `#[deprecated(note)]`, machine-applicable suggestions applied by `cargo fix`; editions (`cargo fix --edition`) for breaking spellings | `rustc` prints the replacement; one command migrates a crate |
| Go | `go fix` rewriters for API moves | one command migrates a module |
| Almide | `@deprecated(since=N, use="…")` on stdlib fns, a `[[deprecation]]` entry in `proofs/dialect-epochs.toml`, `try:` replacements in diagnostics, and E027 tombstones | the same three pieces |

Every peer ends the old name as an error *that still names the replacement*,
never as an unknown identifier. Decision (2026-10-17): Almide skips the
warning window and goes straight to that end state, because the rename is a
one-token mechanical change, the corpus is small, and a silent window would
keep the misreading compiling for one more epoch.

## Logical cost: the contract to state

- **Unit.** One charge unit. It is converted from declared time by the
  versioned calibration constant CM-1 (v0.3 = 3 ns/unit,
  `crates/almide-types/src/time_units.rs`).
- **Charge sites.** Exactly the table in `docs/specs/als/deterministic-time.md`:
  - fn entry, except a loop-free non-recursive callee (C-294);
  - each loop-head evaluation;
  - bulk string concatenation at `1 + len/16`;
  - nothing else.
  #3778 shows that native and wasm each break a different row today. The fix
  is to make both legs follow the table, not to change the table (C below).
- **Budget scope.**
  - `fan.bounded`: the body.
  - `fan.min(compute…) { }`: each arm separately, with the full budget.
  - `fan.min(compute…, xs, f)`: each element separately.
  - An enclosing region min-caps; source cannot nest regions today.
- **Arguments.** In a single-call body such as `{ f(g(x)) }`, `g(x)` ran
  before the meter started (audit §6). E below puts it inside the region.
- **Versioning.** The verdict and the winner are functions of
  (program, inputs, **meter version**). Within one meter version they are the
  same on every target and machine. The meter version changes when the
  charge table, its exemption rule or the calibration constant changes, and
  only then. A change is announced in the release notes with the spend
  ledger's diff. The spend ledger is the gate: a fixture corpus whose
  per-region spend is pinned (`fuel_bounded_boundary.almd`,
  `fuel_dyn_charge.almd`, and the acceptance test's `heavy(1000)` = 1002
  units), and a change in any pinned spend fails CI unless the meter version
  is bumped in the same PR.

## Decision

The rule behind every item: **the name says what is selected; the clock's
TYPE says what it is measured on and which tier it runs in.** `compute.*`
means a pure body and the same answer on every machine; `duration.*` means
the oracle tier — effects allowed, the verdict a recorded host input.

1. **Q1 (2026-10-17): retire the old names at once, no deprecation window.**
   From dialect epoch 14 `fan.race` and `fan.any` are E027 tombstones that
   name the replacement. The parser reports the block heads
   (`crates/almide-syntax/src/parser/fan.rs`, `renamed_fan_head_text`), the
   checker reports the call spellings with the same text
   (`static_dispatch.rs`).
2. **A (2026-10-18): `fan.any` is removed, not renamed.** The first Ok in
   source order is `a() ?? b() ?? fallback`; over a list it is
   `list.find_map(xs, (x) => f(x)?) ?? fallback`. Both already existed; `??`
   evaluates its right side only when the left side failed, and
   `list.find_map` accepts an effect callback — measured on native and wasm
   on 2026-10-18. Because: every peer (Rust `find_map`, OCaml
   `List.find_map`, Elixir `find_value`, Kotlin `firstNotNullOfOrNull`,
   Scala `collectFirst`) keeps this in the list library, and the
   two-candidate form is a lazy fallback chain everywhere (`or_else`,
   `.catch(() => b())`, `<|>`). A fan form for a sequential selection was one
   meaning with two spellings.
3. **Q1 as amended (2026-10-18): `fan.race` becomes `fan.min(clock)`, and
   the clock argument is required.** `fan.min(compute.ms(5)) { a(), b() }`
   and `fan.min(compute.ms(5), xs, f)`; `fan.min { … }` and `fan.min(xs, f)`
   are parse errors. Because: `min` is `min_by_key`'s word for this rule,
   and the mandatory clock prevents reading `fan.min { 3, 5 }` as the
   smallest value. The interim `min_cost` was rejected because the clock
   type already says what is minimised.
4. **H (2026-10-18): the real-time clock is admitted** —
   `fan.min(duration.ms(n)) { … }` / `(duration…, xs, f)`: the arms start
   together, the first to finish with Ok wins, arms may do I/O, **one
   deadline is shared from entry** (not per arm), and the losers are
   cancelled at their next charge site or I/O boundary and awaited before
   the form returns. A loser's performed effects remain, its stdout is
   discarded, only the winner's value is returned. Native runs the arms in
   parallel; wasm runs them in order until it has async (ADR-0024 D8; arm 0
   winning when it succeeds is a legal schedule); ω record/replay pins the
   winning index. Because: this is the shape every peer's `race` / `any`
   serves, and the trade-off (the winner may differ between runs) is the
   one every peer accepts. The structured stop is Haskell `race`, Python
   `TaskGroup`, Kotlin and Swift; JS's leave-it-running is rejected because
   Almide has no unstructured spawn. One shared deadline is what
   `Promise.race([a, b, timer])`, tokio `timeout(dur, select!)`,
   `asyncio.wait(tasks, timeout=…)` and a single Go `ctx` all do; per-arm
   deadlines would multiply the wall time on the sequential wasm leg.
   `fan.timeout(duration…)` admits an I/O body under the same cancellation
   rule. This partly reverses the first draft's "keep K2 and K3 out": K2 is
   in; K3 stays out, because a failed arm is skipped, never selected.
   Follow-up: #3788; until it lands the `Duration` clock is a
   type error on `fan.min`.
5. **G (2026-10-18, ×): `fan.bounded` stays a separate name from
   `fan.timeout`.** The clock-by-type rule is NOT extended to the cut forms.
   Because: every runtime names a fuel/gas limit and a wall deadline
   differently — wasmtime `set_fuel` vs `set_epoch_deadline`, Ethereum "gas
   limit" vs a timeout, tokio `timeout` vs `timeout_at`. Different names for
   different kinds of limit is the peers' design, and "timeout" means wall
   time everywhere.
6. **B (2026-10-18): the cost contract is scoped to a meter version**, as
   stated in §Logical cost. Because: Ethereum reprices gas by hard fork
   (EIP-150, EIP-2929) under a fork id, and wasmtime documents that fuel per
   instruction may change between versions; a promise across versions would
   forbid every inliner change.
7. **C (2026-10-18, #3778): native and wasm follow the charge table.**
   Native stops charging a loop-free wrapper's entry, wasm starts charging
   string concatenation; existing winners and verdicts may move, under the
   meter version. Because: with two implementations of one table, a
   disagreement is a consensus bug (Ethereum clients treat it as the highest
   priority), not a tolerance.
8. **D (2026-10-18, #3780): a `fan { }` whose arm reaches an ordered process
   operation (`env.set`, added to ADR-0024 D3's set) runs sequentially on
   native too.** Because: Rust made `env::set_var` `unsafe` in edition 2024
   and Go's `t.Setenv` panics under `t.Parallel` — the peers refuse the
   combination; Almide keeps its determinism by serialising it, so native
   matches the wasm observation. A check-time refusal remains a possible
   later tightening.
9. **E (2026-10-18): a single-call body's arguments are inside the metered
   region** for `fan.bounded` / `fan.timeout` / `fan.min`. Because: gas and
   wasmtime fuel count every executed instruction; a limit that skips the
   argument's work is not a limit.
10. **F (2026-10-18): the all-fail text of `fan.min` is `fan.min: no branch
    succeeded within budget`.** With the clock required the budget is always
    present, so one text covers "every arm returned Err" and "every arm was
    cut"; `succeeded` says the former case too. (JS `Promise.any` reports
    "All promises were rejected"; the text names the failure, not a budget
    the writer did not set.)

## Alternatives rejected

- **A, keep both names.** Every peer reads `race` as "first to finish" and
  `any` as "unordered". Keeping the names leaves both misreadings silent.
  (This was the first draft's recommendation. It weighed the cost of the
  rename against JS alone, and did not check how far `race` and `any`
  carry the same meaning across ecosystems.)
- **The interim names `fan.first_ok` / `fan.min_cost`** (first revision,
  2026-10-17). `first_ok` was rejected because no peer puts an ordered
  try-in-order in the concurrency library; the selection is sequential and
  already spelled by `??` and `list.find_map`. `min_cost` was rejected
  because the clock type already says what is minimised, so the suffix was
  redundant and closed the door on the `Duration` clock.
- **`fan.min` with an optional clock.** `fan.min { 3, 5 }` reads as the
  smallest value; the argument is the one thing that makes `min` a
  selection over arms.
- **Per-arm deadlines for `fan.min(duration…)`.** No peer has them, and on a
  sequential leg they multiply the total wall time by the arm count.
- **Unify `fan.bounded` into `fan.timeout` by clock type (G).** Rejected per
  Decision 5.
- **Keep K2 out of the language** (the first draft's Q2). The shape is the
  most common use of a race in every peer; the trade-off is one every peer
  accepts; the oracle tier (ω record/replay) already exists for
  `fan.timeout`.
- **JS-style abandonment of the losers.** Almide has no unstructured spawn,
  and a loser running on after the form returned would be one.
- **Freeze the meter across versions.** It would forbid inliner and lowering
  changes in any program with a region.

## Consequences

- **Gains**:
  - Every selection name says what it selects, in the words other
    ecosystems use for it, and a habit carried from another language turns
    into a diagnostic that names the right form.
  - The most common real-world shape ("whichever server answers first")
    gains a spelling, with structured cancellation.
  - One rule — clock type decides the tier — covers `fan.min` and
    `fan.timeout` alike.
  - A user upgrading the compiler can tell from the meter version whether a
    winner may move.
- **Costs**:
  - One epoch entry (14) with three breaks; the examples, CHEATSHEET,
    llms.txt and the Dojo tasks are rewritten.
  - The `fan.any` runtime (nine self-hosted `any_map` cells, the chain
    desugar) is dead code until its removal lands.
  - The spend ledger is one more gate; the meter version is one more number
    in the release notes.
  - `fan.min(duration…)` and the I/O body of `fan.timeout` need a cancellation
    mechanism on native (a cooperative flag checked at charge sites and I/O
    boundaries) and the sequential schedule on wasm.

## Falsifier

The usability evaluation (`docs/design/fan-usability-eval.md`) runs C1, the
old names with D-a/D-c, against C2, the accepted surface. If C2 does not cut
the F1 + F2 + F5 misreading rate by at least a third relative to C1, the
rename is not buying what this ADR claims, and the diagnostics' text is the
first thing to revisit.

## Test plan

| Change | Test | Legs | Status |
|---|---|---|---|
| Q1 / A: old names are E027 naming the replacement | `tests/diagnostics/e027-fan-race-renamed-head`, `e027-fan-any-removed`, the thunk-list cases; `scripts/check-dialect-epochs.sh` sees epoch 14's `breaks` | check | done with the epoch |
| A: the replacements are sequential and lazy | `spec/wasm_cross/fan_any_early_winner.almd` (a `??` chain: later candidates never run), `fan_any_allfail.almd` (every candidate fails → the fallback), `list.find_map` with an effect callback in the mapper fixtures | native, wasm | done with the epoch |
| Q1: `fan.min` requires the clock | parse error for `fan.min { }` and `fan.min(xs, f)`; `tests/fan_surface_matrix_test.rs` | check | done with the epoch |
| F: all-fail text | `fan.min(compute.ms(1)) { err("a"), err("b") }` → `fan.min: no branch succeeded within budget` | 4 legs | done with the epoch |
| C / #3778: meters follow the table | `tests/fan_acceptance_test.rs`: un-ignore `a_loop_free_wrapper_costs_the_same_on_every_leg` and `a_cut_body_that_interpolates_stops_at_the_same_point_on_every_leg` | 4 legs | done (#3778: the native default route reads `meter_plan` and charges `+`, not interpolation; `fuel_wrapper_entry_exempt.almd`, `fuel_interp_uncharged.almd`) |
| B: meter version gate | a script that recomputes every pinned spend and fails unless `METER_VERSION` moved with it | CI | follow-up |
| #3779: trap order in `fan { list.map }` | un-ignore `a_trap_in_a_parallel_list_map_is_the_lowest_index_one`; run it 20 times | native par / seq, wasm | done 2026-10-18 (one timeline per dealt block; `fan_list_map_trap_order.almd`, the acceptance test un-ignored; #3784 closed by the same change) |
| D / #3780: ordered ops make a fan sequential | un-ignore `fan_arms_that_share_process_state_agree_across_targets`; a codegen test that the block with `env.set` renders sequential and one without it does not | 4 legs | follow-up |
| E: single-call arguments metered | `fan.bounded(compute.ns(30)) { id(heavy(100000)) }` is Err on every leg (today Ok) | 4 legs | follow-up |
| H: `fan.min(duration…)` | the winner is the first Ok in real time; a loser's println is discarded; the loser's effect (a written file) remains; the form returns only after the loser stopped; `ALMIDE_OMEGA` replays the winner index on both targets; wasm takes arm 0 | native, wasm | #3788 |
| H: one shared deadline | two arms that each need 60% of the deadline: the second is cut on the sequential leg | native, wasm | #3788 |
| H: `fan.timeout(duration…)` with an I/O body | the body's request is sent, the cut lands at the next boundary, the Err is the deadline's | native, wasm | #3788 |
| #3783 (empty body), #3781 (wasm ω replay), #3782 (Float winner) | landed in #3786 | 4 legs | done |

## Ratification record

| Question | Verdict | Date |
|---|---|---|
| Q1 rename, no deprecation window | ○ | 2026-10-17 |
| Interim names `first_ok` / `min_cost` | × — replaced by A and `fan.min` | 2026-10-18 |
| ① list form of `fan.any` → `list.find_map` | ○ | 2026-10-18 |
| ② `fan.race` → `fan.min`, clock required | ○ | 2026-10-18 |
| ②b admit `fan.min(duration…)` | ○ | 2026-10-18 |
| ②c losers cancelled at the next boundary and awaited; I/O allowed | ○ | 2026-10-18 |
| A block form of `fan.any` → `??` chain | ○ | 2026-10-18 |
| B cost contract scoped to a meter version | ○ | 2026-10-18 |
| C meters follow the table (#3778) | ○ | 2026-10-18 |
| D `env.set` arms run sequentially on native (#3780) | ○ | 2026-10-18 |
| E single-call arguments metered | ○ | 2026-10-18 |
| F all-fail text | ○ (as `no branch succeeded within budget`) | 2026-10-18 |
| G unify `fan.bounded` into `fan.timeout` | × | 2026-10-18 |
| H one shared deadline for the real-time clock | ○ | 2026-10-18 |
| Q7 empty body, Q8 wasm ω replay | ○ — landed in #3786 | 2026-10-18 |

## References

- ADR-0001 (deterministic time units), ADR-0011, ADR-0020, ADR-0022, ADR-0024
- C-004, C-005, C-185, C-199, C-200, C-204, C-205, C-207, C-208, C-294, C-320
- Haskell `async`: [race](https://hackage.haskell.org/package/async/docs/Control-Concurrent-Async.html#v:race) · Java [Stream.findFirst / findAny](https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/util/stream/Stream.html)
- Rayon [find_first / find_any / find_map_first](https://docs.rs/rayon/1.10.0/rayon/iter/trait.ParallelIterator.html) · Rust [Iterator::min_by_key](https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.min_by_key), [Iterator::find_map](https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.find_map), [Result::or_else](https://doc.rust-lang.org/std/result/enum.Result.html#method.or_else) · OCaml [List.find_map](https://ocaml.org/manual/5.2/api/List.html) · Elixir [Enum.find_value](https://hexdocs.pm/elixir/Enum.html#find_value/3)
- Limits by kind: wasmtime [Store::set_fuel](https://docs.rs/wasmtime/latest/wasmtime/struct.Store.html#method.set_fuel) / [set_epoch_deadline](https://docs.rs/wasmtime/latest/wasmtime/struct.Store.html#method.set_epoch_deadline) · tokio [timeout](https://docs.rs/tokio/latest/tokio/time/fn.timeout.html) / [timeout_at](https://docs.rs/tokio/latest/tokio/time/fn.timeout_at.html) · Ethereum gas repricing [EIP-150](https://eips.ethereum.org/EIPS/eip-150), [EIP-2929](https://eips.ethereum.org/EIPS/eip-2929)
- Env + parallelism: Rust [std::env::set_var (unsafe since edition 2024)](https://doc.rust-lang.org/std/env/fn.set_var.html) · Go [testing.T.Setenv](https://pkg.go.dev/testing#T.Setenv) ("cannot be used in parallel tests")
- Shared deadline: tokio `timeout(dur, select!{…})` · Python [asyncio.wait](https://docs.python.org/3/library/asyncio-task.html#asyncio.wait) (`timeout=`) · Go [context.WithTimeout](https://pkg.go.dev/context#WithTimeout)
- Renames: Swift [`@available(renamed:)`](https://docs.swift.org/swift-book/documentation/the-swift-programming-language/attributes/#available) · Kotlin [`@Deprecated` / `ReplaceWith`](https://kotlinlang.org/api/core/kotlin-stdlib/kotlin/-deprecated/) · Rust [`cargo fix`](https://doc.rust-lang.org/cargo/commands/cargo-fix.html)
- MDN [Promise.any](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise/any) / [Promise.race](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise/race)
