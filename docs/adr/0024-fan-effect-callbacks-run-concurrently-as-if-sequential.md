# ADR-0024: `fan` runs effect callbacks concurrently on every target, as if sequential — `fan.map` runs every element, the compiler picks the substrate, and no concurrency concept reaches the writer

- **Status**: Accepted. The maintainer accepted both open decisions on
  2026-09-30: (1) the order in which an outside system receives a program's
  requests is part of the environment's answers (ω), not of the program's
  observation; (2) `fan.map` runs every element, and its result is the first
  `Err` in list order. None of it is implemented yet.
  Amended 2026-10-05 (#3383): D3 and D8 for async JS hooks on `--host js`
  (see the amendment section before the references).
- **Date**: 2026-09-30
- **Scope**: the execution substrate of `fan { … }`, `fan.settle { … }`,
  `fan.map(xs, f)` and `fan.settle(xs, f)` on native and on wasm; the
  meaning of `fan.map` after an element fails; what the writer is told when a
  fan runs sequentially; the invariants that keep concurrency out of the
  surface. `fan.any` is out of scope (§8).
- **Closes the design of**: #2594 (`fan.map` runs effect callbacks
  sequentially on native, and the docs say otherwise).
- **Builds on**: [ADR-0011](./0011-execution-substrate-is-a-free-variable.md)
  (the substrate is a free variable; its D1 output transaction becomes step 1
  here), [ADR-0020](./0020-http-serving-and-handler-state.md) §3 (the
  concurrent slot and the executable closure) and §5.5 (native threads, RNG
  reseeding), [ADR-0022](./0022-output-and-abort-builtins-are-admissible-in-a-pure-fn.md)
  (a pure fn may print, so a pure callback can be observed too),
  [ADR-0023](./0023-wasm-http-over-wasi-http.md) (wasm HTTP is
  `wasi:http@0.3` on a p3 component).
- **Charters**: [concurrency-stance.md](../roadmap/active/concurrency-stance.md)
  (#1000, deterministic data parallelism),
  [async-inception.md](../roadmap/active/async-inception.md) §3 (the head ×
  form table) and pillar 3 (cancellation is an optimization, not semantics),
  [execution-inception.md](../roadmap/active/execution-inception.md) (the
  as-if rule).
- **Contracts touched**: C-004, C-005, C-200 (§9). A contract change lands in
  almide/als first.

## 1. Context

### 1.1 What #2594 found

The docs promise one thing and the compiler does another.

- `docs/CHEATSHEET.md` shows `fan.map(urls, (u) => http.get(u))!` as the
  fan-out idiom, and says pure callbacks run "parallel natively".
- `runtime/rs/src/fan.rs` routes every **effect** callback to
  `almide_rt_fan_map`, a sequential loop over an `Rc<dyn Fn>`. Only a pure
  lambda literal over `Send` scalars reaches `almide_rt_fan_map_par` (#2044).
  The comment there names the reason: an `Rc<dyn Fn>` is neither `Send` nor
  `Sync`.

That reason belongs to the closure representation, not to the language. The
block form proves it. On 2026-09-29, a scratch program with three
`fan { … }` arms, each passing a `String` and a `List[String]` to an effect
fn that sleeps 500 ms, finished in **505 ms**. The emitted Rust is an inline
`std::thread::scope` whose arms are `move` closures over owned captures. A
lambda literal passed to `fan.map` can be lowered the same way.

The same measurement showed the hole ADR-0011 documented. `println` in three
arms came out as `arm 1, arm 3, arm 2`, and the order changed between runs.
ADR-0011 D1 (the per-arm output transaction) has not landed.

### 1.2 The substrate today, per head and target

| Surface | native | wasm (p3 component) | interp |
|---|---|---|---|
| `fan { a; b }` | a thread per arm, all arms run, first `Err` in arm order | sequential | sequential |
| `fan.settle { a; b }` | sequential (3015 ms for three 1 s arms, #2594 comment) | sequential | sequential |
| `fan.map(xs, f)`, pure scalar lambda | chunked threads (#2044) | sequential | sequential |
| `fan.map(xs, f)`, effect callback | sequential | sequential; a body that is exactly one `fs.read_text` is prefetched as async subtasks and awaited in list order (b973a1dd4) | sequential |
| `fan.settle(xs, f)` | sequential | sequential | sequential |

Nobody chose this matrix. Each cell is whatever the representation made
easy, which is the condition ADR-0011 called "the substrate lives in the
charter's blank space".

### 1.3 The block and mapper forms disagree after an `Err`

The charter's table gives the unmarked head one rule for both forms:
"all — everything; the first `Err` in list order". The implementations
split:

- `fan { a; b }` runs **every** arm and then reports the first `Err`
  (`crates/almide-wasm/src/fan.rs` header: "the FIRST err aborts (after all
  arms ran)"; native `thread::scope` joins every arm).
- `fan.map` **stops** at the first `Err` (`stdlib/fan_map.almd`: "SHORT-CIRCUIT
  on the FIRST err"; the native `collect()` stops too).

A short-circuit is the one rule that concurrency cannot reproduce without
speculation. A concurrent map has already started element `k+1` when element
`k` fails. Its printed output can be discarded, but its POST has been sent.

### 1.4 What the peers do

| | Result order | On failure | Siblings after a failure | Concurrency cap | Collect-all form |
|---|---|---|---|---|---|
| JS `Promise.all` | input order | the first to fail **in time** | keep running | none built in | `Promise.allSettled` |
| Go `errgroup` | yours to collect | the first to fail in time | the context is cancelled (cooperative) | `SetLimit(n)` | yours to write |
| Python `asyncio.gather` / `TaskGroup` | input order | the first exception | gather: keep running / TaskGroup: cancelled | none built in | `return_exceptions=True` |
| Kotlin `coroutineScope` + `awaitAll` | input order | the first exception | cancelled | a `Semaphore` | `supervisorScope` |
| Swift `TaskGroup`, Trio nursery, Java `StructuredTaskScope` | completion order | the first exception | cancelled | none built in | per library |
| Haskell `mapConcurrently` | input order | the first exception | cancelled | none built in | — |
| Rust `try_join_all` / `buffered(n)` | input order | the first `Err` | cancelled by drop | `buffered(n)` | `join_all` |

Every peer promises the order of results. None promises the order of effects.
In every peer, which failure wins depends on timing. Changing the degree of
parallelism changes what the program prints.

Each ecosystem also paid a price that Almide must not pay (§3):

- **Rust.** `async fn` and `fn` are two colours, and the stdlib and the
  ecosystem split along them. `Send`, `'static` and `Pin` bounds reach the
  writer, often as an error about a type far from the line being edited
  ("future is not `Send`"). Cancellation is `drop` at an `.await`, so
  cancel safety is the writer's job. The executor is a library choice, so
  the ecosystem forked (tokio, async-std).
- **Go.** Goroutines made concurrency cheap to write and data races cheap to
  ship. A goroutine outlives its caller unless the writer arranges otherwise.
- **JS/TS.** A promise nobody awaits fails silently. The browser is one
  thread.
- **Swift 6.** Strict `Sendable` checking made data races compile errors,
  and the migration was a wall of errors about types, not about code the
  writer meant to change.
- **C#.** `async`/`await` reads well, and races still compile.

## 2. Decision

**A `fan` runs concurrently on every target, and its observation is exactly
that of evaluating it sequentially in list order. `fan.map` runs every
element and reports the first `Err` in list order. The compiler chooses the
substrate; the writer never sees a concurrency concept, an error about one,
or a knob for one.**

### D1. The observation is the sequential, run-every-element evaluation

For `fan { … }`, `fan.settle { … }`, `fan.map(xs, f)` and
`fan.settle(xs, f)`, the reference meaning is: evaluate the elements one
after another in list order, **every one of them**, then

- `fan { … }` / `fan.map`: if some element returned `Err`, the result is the
  `Err` of the lowest index; otherwise the results in list order;
- `fan.settle`: every element's `Result`, in list order.

This changes `fan.map`: it no longer stops at the first `Err` (§1.3, the
maintainer's decision 2). The block and mapper forms now have one rule, as
the charter's table always said.

"Observation" is what #1000 defined: stdout, stderr, the exit code and the
returned value.

### D2. The environment's order is ω

When two elements send requests to an outside system, the order in which
that system receives them is part of the environment's answers ω. This is
the class that async-inception's oracle layer (B1) already treats as a
declared input (the maintainer's decision 1). What the program sees of it,
the responses, is an input like any other I/O.

### D3. The compiler chooses the substrate, per fan, by one question

A fan runs **concurrently** unless its executable closure reaches an
**ordered operation**. Then it runs **sequentially**. There is no third
class.

- The executable closure is ADR-0020 §3.2's `X(E)`, reused as it is. Every
  fn-valued argument of the four surfaces is already a concurrent slot
  (ADR-0020 §3.1).
- An **ordered operation** is a stdlib function whose effect on process-local
  state could be read by a sibling element, so that the sibling's result
  depends on order. The stdlib marks it with a parameterless attribute
  (working name `@ordered`, spelled by the implementing PR, the way
  ADR-0020 leaves `@concurrent`). Initial set:
  - every `fs` function that creates, writes, appends, copies, renames or
    removes (`fs.write*`, `fs.append`, `fs.copy`, `fs.rename`,
    `fs.remove*`, `fs.mkdir_p`, `fs.create_temp_*`);
  - every `kv` function (ADR-0020 §4; a read can observe a sibling's write);
  - every `io` read from stdin (which element gets which line depends on
    order);
  - `process` spawning (a child can do any of the above).
- Not ordered: output (`print`, `println`, `eprintln`, covered by D5), `fs`
  reads, `http` client calls (ω, D2), clock reads (already environmental),
  `random` (ADR-0020 §5.5: randomness makes no reproducibility promise, so
  only distinct seeds per thread are required).

Reads race with writes only if some element writes. If one does, the whole
fan is sequential. The rule is one bit per fan, and it is the same bit on
every target.

### D4. Sequential is a note, never an error

Choosing the sequential substrate changes speed, not the observation. The
compiler therefore never rejects a program for it. It emits a **note** (not
a warning, since nothing is wrong) with the witness path in the writer's
terms:

```
note: this fan runs its elements one at a time
  --> src/sync.almd:12:3
   |
12 |   fan.map(items, (it) => save(it))
   |   ^^^^^^^ `save` writes a file (fs.write, src/store.almd:8),
   |           and another element could read it
```

The same applies to anything the compiler cannot move to another thread,
such as a closure value in a list. The note names the value, never `Send`,
`Sync`, a thread, or a lifetime.

### D5. Output is transactional per element (ADR-0011 D1, promoted to a precondition)

Each element's stdout and stderr go to one per-element timeline. They are
flushed in list order when earlier elements have finished. Element 0 streams
directly, since nothing precedes it. A nested fan's elements flush into the
enclosing element's timeline. Stdout and stderr stay one timeline per
element and are split into the two fds at flush (ADR-0011 D1). This covers
pure callbacks too, which may print (ADR-0022).

The concurrent substrate may not be enabled for any surface before this
lands (ADR-0011: "Rung 1 after D2 was rejected").

### D6. Failures: `Err` waits for everyone, a trap waits for everyone below it

- **`Err`.** By D1 every element runs, so the concurrent substrate joins
  every element and picks the lowest-index `Err`. Nothing is speculative.
- **Trap** (division by zero, overflow, out of bounds, `panic`, a failed
  `assert`). Sequentially, a trap in element `k` means elements `0..k` ran
  and nothing after `k` did. The concurrent substrate stops starting
  elements, waits for every element below `k`, and lets the lowest-index
  trap win. It flushes the timelines below that element in order, then the
  trapping element's partial timeline, then aborts through the unified
  main-error path (C-005 / C-200 shape unchanged).
- **Residual.** Elements above the winning trap that had already started may
  have sent requests that the sequential evaluation never sends. Their
  output is discarded, so the observation matches. Their effect on the world
  does not. A trap is a bug-class exit, and closing this would require
  never starting element `k+1` before `k` finishes, which is sequential
  execution. The residual is recorded here and in C-200 rather than hidden.

### D7. No concurrency knob in the surface

There is no `limit:` argument. The async-inception rejection table already
says "`fan.map(xs, limit: n, f)` — not added until the need is
demonstrated". The runtime bounds in-flight elements itself. The initial
bound is ADR-0020's `max_in_flight` default of 64, so a process has one
number. `ALMIDE_FAN_SEQUENTIAL=1` stays as the ablation switch. It is an
environment variable for measurement, not a language feature.

If the search tool (the motivating program) shows a provider rate limit
that 64 cannot absorb, that is the demonstration the table asks for, and a
new ADR decides the spelling.

### D8. Wasm gets its concurrency from WASI 0.3 async, below the IR

On the p3 component, an element's host calls become async-lowered subtasks
driven by a waitable set, the mechanism b973a1dd4 already uses for the
`fs.read_text` prefetch and ADR-0023 uses for `wasi:http@0.3`. An element
that performs several host calls one after another needs the element itself
to suspend between them. The compiler provides that with a state machine or
a stack switch. **That machinery lives below the IR and never appears in a
type, a signature, a diagnostic or the docs** (N1).

Until the general case lands, wasm runs a fan sequentially. The observation
is identical by D1, so this is a speed gap, not a behaviour gap. No shared
memory and no atomics: ADR-0011 D3 and C-210's `FORBIDDEN` stand.

## 3. Invariants: what Almide does not repeat

These are normative. A future change that breaks one needs a new ADR that
supersedes this section.

| # | Invariant | The failure it rules out |
|---|---|---|
| N1 | No `async`, `await`, `Future`, task handle, `Pin` or executor in the surface. Concurrency is the `fan` expression. The effect axis stays `fn` / `effect fn` and gains no colour. | Rust's two colours and doubled ecosystem; the state machine leaking into types |
| N2 | Whether elements may run on other threads is decided by the compiler and is never a writer-facing error. `Send`, `Sync`, `Sendable`, `'static` and "thread" do not appear in diagnostics. | Rust's "future is not `Send`", Swift 6's `Sendable` migration wall |
| N3 | Cancellation is an optimization, not semantics (async-inception pillar 3). No surface cancels an element that has started. `race` and `select` stay removed (E027). | Rust's cancel safety at `.await`, `select!` dropping half-done work |
| N4 | One runtime, chosen by the compiler per target. The writer never picks an executor. | tokio / async-std fork |
| N5 | Every element has finished when the `fan` expression produces its value. Nothing is detached. There is no spawn. | Go's leaked goroutines, JS's unawaited promise |
| N6 | No `var` is reachable from an element (ADR-0020 §3, E008). | Go's and C#'s data races |
| N7 | Changing the substrate never changes the observation, and a gate checks it (ADR-0011 D5), run repeatedly and with the substrate forced (ADR-0011 F5). | Every peer: the output is a function of the schedule |
| N8 | No surface knob for the degree of concurrency (D7) until a demonstrated need, and then one spelling. | `buffered(n)` / `SetLimit` / p-limit, each ecosystem its own |

## 4. Rationale

1. **The peers' failures share one cause: the schedule became something the
   writer has to reason about.** Rust made it typable, Go made it cheap, JS
   made it invisible until it fails. Almide's charter already fixed the
   observation to a sequential evaluation (#1000), so the schedule has
   nothing to leak into. What was missing was the promise's other half: that
   the compiler is then free to schedule, and does.
2. **Run-every-element is what makes D3 a single bit.** With a
   short-circuit, concurrency needs speculation, and speculation is safe
   only for effects that can be undone. That would split callbacks into
   reads, irreversible writes and shared state, inferred through the call
   graph. One edit deep in a helper would then silently change which class a
   fan is in. That is Rust's auto-trait inference with a performance cliff
   instead of an error. Without a short-circuit, the only remaining question
   is "can a sibling observe this element's effect on process state?"
3. **The block form already runs every arm.** D1 makes `fan.map` agree with
   `fan { }` and with the charter's table. It does not introduce a new
   meaning.
4. **The substrate is a free variable (ADR-0011).** A note instead of an
   error follows directly: when the compiler cannot go concurrent, the
   program is still correct, only slower. Refusing it would be the Swift 6
   outcome for no gain in correctness.
5. **The motivating program fits.** A search aggregator calls several
   providers with `fan.settle(providers, (p) => query(p, q))`. `http`
   calls are not ordered, so it runs concurrently on native and on the p3
   component. It does so even for providers queried by POST, because no
   element is started speculatively.

## 5. Alternatives

| Option | Verdict | Reason |
|---|---|---|
| Promise only result order, let effect order vary (the first draft of this ADR's discussion) | **Rejected** | It contradicts #1000 and ADR-0011 D1: the observation would depend on the schedule, which is the determinism hole ADR-0011 closes |
| Keep the short-circuit and speculate only for "undoable" callbacks (reads, GET, output) | **Rejected** | Three classes inferred through the call graph; one edit flips a fan's class invisibly (§4.2) |
| Reject a callback the compiler cannot run concurrently (a compile error, staged as a warning) | **Rejected** | The program is correct. An error would be Rust's `Send` error and Swift 6's migration wall (N2) |
| Fall back to sequential silently | **Rejected** | Correct but undiagnosable. D4's note keeps the reason visible without blocking |
| `fan.map(xs, f, limit: n)` now | **Deferred** | The charter requires a demonstrated need (D7) |
| Add `async fn` / `await` for I/O-bound concurrency | **Rejected** | N1. `fan` already expresses what to overlap |
| Make every value `Send` (Rc→Arc) so any callback can move | **Rejected** | ADR-0020 §5.5 rejected it as a whole-runtime cost. Owned captures and ordered ops cover the need |
| Keep `fan.map` sequential for effect callbacks and fix the docs | **Rejected** | It leaves the motivating program without concurrency, and the substrate chosen by accident (§1.2) |

## 6. Consequences

### Gains

- `fan.map(urls, (u) => http.get(u))` runs concurrently on native, as the
  cheatsheet already claims, and on the p3 component once D8 lands.
- `fan { }` output stops interleaving (D5). That closes ADR-0011's hole and
  removes C-004's EXCEPTION clause.
- One rule for `fan { }` and `fan.map` after an `Err`.
- The invariants N1–N8 are written down, so a later "add `await`" or "add a
  limit knob" must argue against them.

### Costs

- **Behaviour change.** A program whose `fan.map` element fails now also runs
  the elements after it. Their output appears (flushed in order) before the
  abort, and their effects happen. This is a meaning change, so it is a
  dialect epoch (§9).
- **Wasted work after an `Err`.** A map over 1 000 URLs whose first element
  fails still fetches the other 999. A writer who wants to stop at the first
  failure writes a `for` loop with `!`, which says so.
- **Buffering.** An element's output is held until the elements before it
  finish. The worst case is the total output of every element but the first
  (ADR-0011 D1).
- **The trap residual** (D6).
- **Wasm lags in speed** until D8's general case lands.

## 7. Falsifiers

- **F1.** A program whose observation differs between substrates and that D5
  and D6 cannot close. Then that substrate is removed for the affected
  surface (ADR-0011 F1: give up the substrate, never the observation).
- **F2.** The run-every-element cost shows up in real programs, not as a
  hypothetical. For example, the search tool or a dojo corpus hits it where
  a `for` loop is not the natural spelling. Then reopen the short-circuit,
  with a design that does not bring back §4.2's three classes.
- **F3.** The ordered set in D3 is wrong: a stdlib function outside it lets a
  sibling observe order (a missed class), or a function inside it forces
  sequential execution in real programs without cause. Then the set is
  corrected, and the D5-style gate gains the case.
- **F4.** Users need a writer-visible limit (D7).
- **F5.** The D8 machinery cannot be kept below the IR, for example because
  the component model forces an async signature onto user functions. Then
  wasm stays sequential for the affected shapes. N1 is not relaxed.

## 8. Out of scope

- **`fan.any`.** Its early cut is its meaning ("the lowest-index success,
  with sequential fallback"). Concurrency for it needs speculation, which
  §4.2 rejects for `fan.map`. It keeps the sequential substrate. The
  existing read-only prefetch on the p3 component stays, because it abandons
  only reads. A separate decision may revisit it.
- `fan.race` / `fan.bounded` / `fan.timeout`: governed by async-inception.
- `http.serve` concurrency: ADR-0020 §5.5.

## 9. Contracts, epoch, plan

**Contract changes** (almide/als first):

- **C-004.** "`fan.map` runs element fns sequentially in list order" becomes
  "`fan.map` and `fan.settle` behave as a sequential evaluation of every
  element in list order, whatever the substrate". The EXCEPTION clause is
  deleted in the PR that lands D5 (ADR-0011 D6).
- **C-005.** "The first element fn returning Err" is qualified: every
  element runs, and the lowest-index `Err` is the one that surfaces.
- **C-200.** "Does not wait for in-flight siblings" becomes D6: wait for the
  siblings below the trapping element, and state the residual.

**Dialect epoch.** The ledger counts "a construct's meaning changed"
(`proofs/dialect-epochs.toml`). D1 is that, so it takes the next epoch after
ADR-0020's epoch 6. Its `breaks` line: "`fan.map` runs every element even
after one returns `Err`; to stop at the first failure, use a `for` loop with
`!`". The migration impact over `spec/`, `examples/` and the dojo corpus
(programs whose `fan.map` has a failing element followed by an element that
prints or writes) is measured in step 3, before the epoch is cut.

**Plan.**

| Step | Work | Depends on |
|---|---|---|
| 0 | C-004 / C-005 / C-200 rewording in almide/als | — |
| 1 | D5 on native for `fan { }`, `fan.settle { }` and `fan_map_par`; delete C-004's EXCEPTION | 0 |
| 2 | The `@ordered` attribute on the D3 set; the substrate bit computed from ADR-0020's `X(E)`; the D4 note | ADR-0020 step 1 (#2697) |
| 3 | D1 on every leg: `stdlib/fan_map.almd`, the wasm emitters, native, `almide-interp`, the reference evaluator (ADR-0015); impact measurement; the epoch | 0 |
| 4 | Native: lower lambda-literal and named-fn callbacks of `fan.map` / `fan.settle` as owned `move` closures like block arms; `fan.settle { }` onto threads; the in-flight bound (D7); per-thread RNG seeds (ADR-0020 §5.5) | 1, 2, 3 |
| 5 | ADR-0011 D5 gate extended to the four surfaces, run repeatedly and with the substrate forced | 4 |
| 6 | Wasm p3: host calls as async subtasks for single-call elements (generalizing b973a1dd4 to `http.*`), then multi-call elements (D8) | 1, 3, ADR-0023 |
| 7 | Docs: CHEATSHEET fan section, `docs/specs/als/runtime.md` ALS-R3, LLM-facing docs | 4 |

## Amendment 2026-10-05: async JS hooks on `--host js` (#3383)

Accepted by the maintainer's ruling on #3383 (first slice only). #3371 put
"this JS hook answers with a Promise" on the extern
(`@extern(wasm, "js", NAME, returns: promise)`). This amendment says what a
`fan` over such hooks means on the JS host.

**D3, amended.** An extern marked `returns: promise` is a request to a system
outside the process (Workers KV, `fetch`, Cloud Storage). The order in which
that system receives the requests is the environment's ω (D2), the class D3
already gives `http` client calls. So a `returns: promise` extern is **not**
an ordered operation and does not force a fan sequential. A synchronous
extern stays as it was: it is never overlapped.

**D8, amended.** On `--host js`, JSPI (`WebAssembly.Suspending` /
`WebAssembly.promising`) is the substrate, as WASI 0.3 async is on the p3
component. A fan whose every element performs **one** async-hook call on
values it already has is lowered as the p3 prefetch is (`fan.rs`, host ops
40 start / 41 await / 42 abandon), with three imports the glue generates per
hook the fan reaches:

- `start:NAME(args) -> slot`: a plain import. It calls the hook and keeps its
  Promise in a slot.
- `wait()`: the **only** Suspending import. It awaits every started slot and
  settles each as a value or a rejection.
- `take:NAME(slot) -> value`: a plain import. It returns the settled value,
  marshalled as the hook's own return. A rejection is the err of a fallible
  extern and abandons the instance for an infallible one, as a direct call
  does (#3356).

The module starts every element in arm order, suspends once, and takes the
values in arm order. Every element runs (D1), and the first `Err` in arm
order is the result (D6), so the observation is the sequential lowering's
(N7). `fan.any` takes until its first ok. The slots it does not take were
awaited by `wait` (N5) and are dropped unread. The slice is `fan.map`,
`fan.any` and a `fan { }` block. Any other fan on the JS host stays
sequential under D4's note: an element that computes its arguments, calls
a sync hook, or performs several calls one after another. Several calls
need the element to suspend between them, which is the state machine or
stack switch of D8 proper.

Nothing reaches a type, a signature, a diagnostic or the LLM-facing docs (N1).
The exports that reach `wait` are entered through `promising`, as they were
for a direct async call (#3353). The module bytes differ from the
sequential lowering only for a program with such a fan built with
`--host js`; every other build is byte-identical. `ALMIDE_FAN_SEQUENTIAL=1`
(D7) forces the sequential lowering. The gate `spec/wasm_host_js/fan_async_overlap`
builds and runs both lowerings and requires the same output, and asserts that
the overlapped one has every hook in flight at once (the issue measured
510 ms → 50 ms for ten 50 ms reads).

## References

- #1000 / [concurrency-stance.md](../roadmap/active/concurrency-stance.md),
  [async-inception.md](../roadmap/active/async-inception.md) §3 and pillar 3,
  [execution-inception.md](../roadmap/active/execution-inception.md)
- This repository: `runtime/rs/src/fan.rs`, `stdlib/fan_map.almd`,
  `crates/almide-wasm/src/fan.rs`, commit b973a1dd4, `docs/CHEATSHEET.md`
  fan section, `docs/contracts/contracts.toml` C-004 / C-005 / C-200,
  `proofs/dialect-epochs.toml`; scratch measurements of 2026-09-29 (a
  three-arm `fan { }` with heap arguments in 505 ms; interleaved `println`
  across arms)
- [MDN: Promise.all](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise/all),
  [Promise.allSettled](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise/allSettled)
- [Go: errgroup](https://pkg.go.dev/golang.org/x/sync/errgroup),
  [Go: race detector](https://go.dev/doc/articles/race_detector)
- [Python: asyncio task groups and gather](https://docs.python.org/3/library/asyncio-task.html)
- [Kotlin: composing suspending functions / structured concurrency](https://kotlinlang.org/docs/composing-suspending-functions.html)
- [Swift: TaskGroup](https://developer.apple.com/documentation/swift/taskgroup),
  [Swift 6 migration guide](https://www.swift.org/migration/documentation/migrationguide/)
- [Haskell: Control.Concurrent.Async](https://hackage.haskell.org/package/async/docs/Control-Concurrent-Async.html)
- [Rust: tokio::select! and cancellation safety](https://docs.rs/tokio/latest/tokio/macro.select.html)
- [N. J. Smith, Notes on structured concurrency](https://vorpus.org/blog/notes-on-structured-concurrency-or-go-statement-considered-harmful/)
- [B. Nystrom, What color is your function?](https://journal.stuffwithstuff.com/2015/02/01/what-color-is-your-function/)
- [WASI 0.3](https://wasi.dev/releases/wasi-p3)
