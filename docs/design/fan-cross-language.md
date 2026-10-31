# Fan against JS, Rust, Python and Go: what each one selects, waits for and stops

> Last updated: 2026-10-18 (the Almide column reflects ADR-0028 as accepted:
> K1 is `??` / `list.find_map`, K4 is `fan.min(compute…)`, and K2 is
> `fan.min(duration…)`, decided and landing in #3788) · the audit
> evidence is pinned at 0746eb689 (see
> [fan-semantics-audit.md](./fan-semantics-audit.md) for the evidence behind
> every Almide cell). Peer semantics are from their reference documentation
> (links at the end), not from names.

This document does not match operations by name: Almide `race` is not JS
`race`, and Almide `any` is not JS `any`. Each operation is described by three
questions:

- **Selects**: which result it returns.
- **Waits for**: what must finish before it returns.
- **Stops**: what it prevents from running, and how.

Two kinds of stopping must not be confused.

- **Dropping**: the runtime stops driving a computation it owns. Examples are a
  Rust future that is no longer polled, a Rayon split that is not taken, or an
  Almide element that is never started.
- **Cancelling external work**: something already started outside the runtime
  is told to stop. Examples are an HTTP request already on the wire, a child
  process, or a write already issued. In every language below this is
  *cooperative*: Go `context`, JS `AbortSignal` and Python `CancelledError` only
  reach code that checks for them. Dropping a future never un-sends a request.

Almide today has no external cancellation at all. Its only stops are "never
start an element" and "cut a pure body at a charge site" (fan.bounded /
fan.timeout / fan.min(compute…) budgets). A cut body cannot have external
effects, because those bodies are checked pure (E006). The decided
`fan.min(duration…)` (#3788) adds cooperative cancellation of the
losing arms at their next charge site or I/O boundary, awaited before the
form returns — the structured kind, as Haskell `race` and Python `TaskGroup`.

## The five selection kinds

| # | Kind | Deterministic? |
|---|---|---|
| K1 | First success **in input order** | yes |
| K2 | First success **in real time** | no |
| K3 | First **completion**, success or failure, in real time | no |
| K4 | Success with the **least logical cost** (tie → input order) | yes, given the cost model |
| K5 | **Collect all** (all results, or all-or-error) | yes for the value; the error choice varies by language |

## Mapping

| Kind | Almide | JS | Rust | Python asyncio | Go |
|---|---|---|---|---|---|
| K1 | `list.find_map(xs, (x) => f(x)?)`, `a() ?? b()` — sequential, not a fan form | — | `Iterator::find_map`; Rayon `find_first` (pure predicate) | — | — |
| K2 | `fan.min(duration.ms(n)) { }` / `(duration…, xs, f)` — decided, #3788 | `Promise.any` | `futures::future::select_ok`; Rayon `find_any` | `as_completed` loop / `wait(FIRST_COMPLETED)` with a success check | hand-written: a channel plus `context` cancel |
| K3 | — (a failed arm is skipped, never selected: see K2) | `Promise.race` | `futures::select!` / `future::select` / `select_all` | `asyncio.wait(return_when=FIRST_COMPLETED)` | first receive from a shared channel |
| K4 | `fan.min(compute.us(n)) { }`, `fan.min(compute.us(n), xs, f)` — the clock argument is required | — | — | — | — |
| K5 all-or-error | `fan.map(xs, f)`, `fan { }` | `Promise.all` | `join_all` over Results / `try_join_all`; Rayon `collect::<Result<_,_>>()` | `gather()`, `TaskGroup` | `errgroup.Group` |
| K5 all results | `fan.settle(xs, f)`, `fan.settle { }` | `Promise.allSettled` | `join_all` | `gather(return_exceptions=True)` | `errgroup` with per-index error slots |

The two names that collided were the trap. **JS `race` is K3, while Almide
`race` was K4. JS `any` is K2, while Almide `any` was K1.** A reader carrying
the JS meaning got a different result on every input where wall-clock order
differed from list order, or from cost order. Dialect epoch 14 closed it:
`fan.race` became `fan.min(clock)` and `fan.any` was removed in favour of the
sequential spellings above (ADR-0028).

## Selects / waits for / stops, per operation

### K1: first success in input order

| | Selects | Waits for | Stops |
|---|---|---|---|
| Almide `list.find_map(xs, (x) => f(x)?)` / `a() ?? b()` (was `fan.any`) | the lowest-index Ok; the `??` fallback if every element fails | until the winner is known, which requires every element *before* it to have failed | starts **no element after the winner** (audit §4, test `any_runs_nothing_after_the_winner…`). Elements before the winner have all run, with their effects kept. The old `fan.any` fs-prefetch exception is not a property of the replacement `list.find_map` loop. |
| Rayon `find_first` | the leftmost element matching the predicate | the whole split tree to settle | splits to the right of a found match are skipped "as soon as possible". Work already running on other threads finishes, so the predicate may run on elements past the answer. Pure predicates only by convention; there is no checker. |

Almide's K1 is sequential-observable: the set of elements that ran is exactly
the prefix up to the winner. Rayon's K1 returns the same answer, but the set
that ran is a timing-dependent superset of it.

### K2: first success in real time

| | Selects | Waits for | Stops |
|---|---|---|---|
| JS `Promise.any` | the first promise to fulfil; an `AggregateError` of all reasons if all reject | the first fulfilment, or all rejections | **nothing**: the other promises keep running, because promises cannot be cancelled. An `AbortController` must be wired in by hand. |
| `futures::future::select_ok` | the first `Ok` future to complete; the last error if all fail | the first `Ok`, or all failures | **drops** the remaining futures, so they are not polled again. External work they already started (a request sent, a spawned task) is not undone. |
| Rayon `find_any` | any matching element, in no particular order | until one is found | skips the remaining splits; running ones finish |
| Python `wait(FIRST_COMPLETED)` loop | the first task whose result passes the check | per round, the first completion | nothing by itself: the caller must `task.cancel()` the rest, which raises `CancelledError` at their next `await` |
| Go | whatever the hand-written code picks | first receive | `cancel()` on a `context` that the workers must check |

### K3: first completion

| | Selects | Waits for | Stops |
|---|---|---|---|
| JS `Promise.race` | the first to settle, fulfilled **or rejected** | the first settlement | nothing (see `Promise.any`) |
| `futures::select!` / `select_all` | the first branch to complete, Ok or Err | the first completion | drops the other branches (`select!`) or hands back the remaining futures (`select_all`) |
| Python `wait(FIRST_COMPLETED)` | the done set | the first completion | nothing; the pending set is returned to the caller |
| Go | first receive | first receive | context cancel, cooperative |

### K4: least logical cost

Only Almide has this. `fan.min` runs every arm, one after another, under
the deterministic charge meter, and picks the Ok with the least spend; ties
go to source/list order (audit §5). Nothing is saved by finding a cheap arm
early: every arm runs to completion, or to the budget. The required Compute clock argument supplies the per-arm budget; an arm cut
at a charge site is excluded. The
arm is pure, so nothing external had started. The answer is the same on
every machine and, since #3778, on every target: both meters read the one
charge table (a loop-free wrapper's entry is exempt, interpolation is free).

The nearest peers are Rayon `min_by_key` over a cost you compute yourself
(this runs everything to completion, so nothing is saved), and search
frameworks with an explicit budget. None of the four ecosystems has a
standard combinator for it.

### K5: collect all

| | Selects (on error) | Waits for | Stops |
|---|---|---|---|
| Almide `fan.map` / `fan { }` | the **lowest-index** Err | **every** element, also after a failure | nothing: every element runs (ADR-0024 D1). The error is chosen by index, not by time. |
| Almide `fan.settle` | every Result | every element | nothing |
| JS `Promise.all` | the **first rejection in time** | the first rejection, or all fulfilments | nothing: the others keep running after the returned promise has rejected |
| JS `Promise.allSettled` | every outcome | every promise | nothing |
| `futures::future::join_all` | every output | every future | nothing |
| `futures::future::try_join_all` | the **first Err in time** | the first Err, or all Ok | **drops** the rest at the first Err |
| Rayon `collect::<Result<Vec<_>,_>>` | *an* Err; which one is unspecified | every running split | stops taking new splits after an Err is seen |
| Python `gather()` | the first exception raised, in time | the first exception | nothing: "other awaitables … won't be cancelled and will continue to run" |
| Python `gather(return_exceptions=True)` | every result or exception, in input order | every awaitable | nothing |
| Python `TaskGroup` | an `ExceptionGroup` of **all** failures | every task, including the ones it cancelled | at the first failure it **cancels** the remaining tasks, delivering `CancelledError` at their next `await` |
| Go `errgroup.Group.Wait` | the **first non-nil error in time** | every `Go` call to return | nothing by itself. `WithContext` cancels the derived context at the first error, and only workers that check `ctx` stop. |
| Go `errgroup.SetLimit(n)` | (same) | (same) | bounds the number of running goroutines: `Go` blocks until a slot frees |

Almide is the only one that chooses the error by input position and runs the
remaining elements anyway. That is what makes `fan.map` byte-identical under
sequential, parallel and wasm execution. The price is that a failure early in
a long list saves no work. The cheatsheet tells the reader to use a `for`
loop with `!` to stop at the first failure.

## What a translator must change

| Coming from | Writing | Almide meaning | Correct Almide |
|---|---|---|---|
| JS `Promise.any` | `fan.any` | E027 | `fan.min(duration.ms(n), mirrors, f)` once #3788 lands (first to succeed in real time, losers cancelled and awaited); until then `list.find_map(mirrors, (m) => f(m)?)` if the earliest in the list is acceptable |
| JS `Promise.race` | `fan.race` | E027 | `fan.min(compute.ms(n)) { … }` for the least compute over pure arms; `fan.min(duration.ms(n)) { … }` (#3788) for the first to finish — a failed arm is skipped, not selected |
| JS `Promise.all` | `fan.map` | runs all, lowest-index error | `fan.map` (same value on success) |
| Python `TaskGroup` | `fan.map` | no cancellation of siblings | `fan.map`; a `for … !` loop if stopping is needed |
| Go `errgroup.WithContext` | `fan.map` | no context; the earliest-index error | `fan.map` |
| Rust `try_join_all` | `fan.map` | nothing dropped | `fan.map` |
| Rayon `find_first` / `Iterator::find_map` | `list.find_map(xs, (x) => f(x)?)` | the same answer; elements after the winner never start | `list.find_map` |
| a timeout around I/O (`asyncio.wait_for`, `context.WithTimeout`) | `fan.timeout { http.get(u) }` | rejected today: the body must be pure (E006) | decided (ADR-0028): the `Duration` clock admits an I/O body, cancelled at its next charge site or I/O boundary and awaited — #3788 |

## References

- JS: MDN, [Promise.all](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise/all), [allSettled](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise/allSettled), [any](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise/any), [race](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise/race), [AbortController](https://developer.mozilla.org/en-US/docs/Web/API/AbortController)
- Rust futures 0.3: [join_all](https://docs.rs/futures/latest/futures/future/fn.join_all.html), [try_join_all](https://docs.rs/futures/latest/futures/future/fn.try_join_all.html), [select](https://docs.rs/futures/latest/futures/future/fn.select.html), [select_all](https://docs.rs/futures/latest/futures/future/fn.select_all.html), [select_ok](https://docs.rs/futures/latest/futures/future/fn.select_ok.html)
- Rayon 1.10: [find_first](https://docs.rs/rayon/1.10.0/rayon/iter/trait.ParallelIterator.html#method.find_first), [find_any](https://docs.rs/rayon/1.10.0/rayon/iter/trait.ParallelIterator.html#method.find_any), [with_min_len](https://docs.rs/rayon/1.10.0/rayon/iter/trait.IndexedParallelIterator.html#method.with_min_len)
- Python 3.12: [asyncio.gather](https://docs.python.org/3/library/asyncio-task.html#asyncio.gather), [TaskGroup](https://docs.python.org/3/library/asyncio-task.html#task-groups), [asyncio.wait](https://docs.python.org/3/library/asyncio-task.html#asyncio.wait), [Task.cancel](https://docs.python.org/3/library/asyncio-task.html#asyncio.Task.cancel)
- Go: [golang.org/x/sync/errgroup](https://pkg.go.dev/golang.org/x/sync/errgroup) (`Wait`, `WithContext`, `SetLimit`)
