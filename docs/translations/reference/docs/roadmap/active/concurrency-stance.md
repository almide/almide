Original description metadata: The concurrency model decision and the fan-family fixes it settles
<a id="concurrency-stance-deterministic-data-parallelism"></a>

# Concurrency stance: deterministic data-parallelism

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](concurrency-stance.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](concurrency-stance.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](concurrency-stance.zh-CN.md)

> Non-normative reader snapshot pinned to the original commit; the original remains authoritative. No automatic synchronization or independent execution, implementation verification, proof checking or measurement. Historical commands and design instructions are documentation, not executed instructions.
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/concurrency-stance.md)

> Source caveat: this is the recorded 0.44 decision and July/August 2026 investigation, not a freshly verified universal concurrency guarantee. The opening strict list-order observable-equivalence principle coexists with the later acknowledged stdout/stderr interleaving exception. The original buffering plan was not needed for the measured trap case and was left outside this Unit for printing siblings, although the final implementation-order list still includes it. The old race impossibility claim is qualified by the August 1 deterministic-fuel amendment. The wasm32 thread assertion is historical target context, not a universal statement about all WebAssembly configurations. Counts, timings, diagnostics and closure claims retain their measured source scope; files/network effects are outside the stated ledger observations. No implementation, fixture, benchmark or contract was independently run.

> Answer to issue [#1000](https://github.com/almide/almide/issues/1000). Ladder row 0.44.
> This decision derives the correct resolution of all 4 open `fan` issues (#1023 / #1024 / #1025 / #1026).

<a id="決定"></a>

## Decision

**Almide's concurrency model is deterministic data parallelism.**

`fan` is **scheduling syntax, not semantic syntax**. Execution may be parallel, but the program's observable behavior (stdout / stderr / exit code / return value) is defined as **strictly identical to sequential evaluation in list order**.

Anything to which list-order meaning cannot be assigned does not enter the language.

Do not adopt structured concurrency (goroutine / channel equivalents).

<a id="なぜこれ以外にないのか"></a>

## Why there is no other choice

The decisive point is not choosing a new policy. **We decided which of two incompatible existing promises to discard.**

`docs/SPEC.md` says opposite things about the same feature.

- §13.1 “real threads on native, sequential in declaration order on wasm — **results are identical either way**”
- §13.2 “`fan.race(thunks)` — **first to complete wins**”

“Same results whichever execution is used” and “the faster one wins” cannot both hold. The former requires independence from timing; the latter makes timing itself the result.

And the former cannot be discarded. Native ⇄ wasm byte identity is the very reason the contract ledger exists; all 196 contracts in `docs/contracts/contracts.toml` rest on this equivalence. Because wasm32 has no threads, timing-dependent syntax **cannot in principle be implemented** on the wasm side. The same reason led to removing `fan.timeout` in 0.29.0 — it was “the only stdlib surface that was not a function of the program and input.”

Moreover, this language's mission is to be the language LLMs write most accurately: modification survival rate. Code whose output changes on every run with the same input breaks that measurement itself. **For Almide, determinism is not a performance characteristic but part of the definition of correctness.**

So discard “the faster one wins.”

<a id="4-件への帰結"></a>

## Consequences for the 4 issues

<a id="1024-fanrace--撤去済2026-07-31develop-にマージ"></a>

### #1024 `fan.race` — **Removed (2026-07-31, merged into develop)**

Completed: checker E027 tombstone; remove the `fan.race` clause from C-004's title and statement; remove the head-Err clause and `spec/wasm_cross/fan_race_err.almd` from C-005; update SPEC.md §9.8/§13.2, README and DESIGN; add diagnostic fixture `e027-fan-race-removed`; migrate 6 call sites. Contract gate green (198 contracts / flagged 0), spec 324 + examples 11 files green.

**Migration could not be mechanical substitution.** Whereas `fan.race(ts)` ≡ `ts[0]()`, `fan.any` behaves differently when thunk[0] fails (any seeks the next Ok). Mechanically replacing a head-Err case produces **a passing test that asserts the wrong thing**. We read what each file pinned before migrating:

- `fan_race_test.almd` — deleted because race-specific (replaced by the E027 fixture)
- `fan_race_any_wasm.almd` → `fan_any_wasm.almd` — drop race cases, retain any
- `fan_value_regression_test.almd` — move “Result bindings pass through `??`/`==`” to any; for “2 capturing thunks share 1 list (E0308),” **the two-thunk shape is the point**, so use `fan.settle` (do not duplicate the any test below)
- `fan_pure_thunks` / `fan_var_thunk_list` / `fan_deterministic` — drop race lines or move to `fan.any`, which follows the same lowering path

A **tombstone chain** was also found: `e027-fan-timeout-removed/fixed.almd` pointed to `fan.race` as the migration destination for `fan.timeout` in 0.29.0, and stopped compiling with this removal. **A tombstone's migration destination must be a live surface** — a hint pointing to a dead surface silently breaks at the next removal.

<a id="1024元の分析"></a>

### #1024 (original analysis)

The implementation is already correct in list order. C-004 is correct too. Only SPEC.md is lying.

But do not stop at “fix the documentation.” Reading the IR shows that `desugar_fan.rs::rewrite_race_head` **replaces `fan.race([t0, t1, …])` directly with `t0`**. Other thunks are not even executed. In other words, `fan.race(ts)` is **an empty wrapper completely equivalent** to `ts[0]()`.

Under the deterministic model, no meaning can be assigned to the name `race`; even if one could, it would only duplicate an existing surface.
(**2026-08-01 revision**: this statement remains true **without a metric**. When fuel supplies a deterministic metric comparable across branches, a race can be defined whose observations are strictly identical to list-order sequential evaluation. Design: [logical-time-async.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/logical-time-async.md). This stance's principle, “anything to which list-order meaning cannot be assigned does not enter the language,” remains preserved there.) Treat it like `fan.timeout` — **an E027-family check-time tombstone + migration hint** (call `ts[0]()` directly, or use `fan.any` to skip failures). Do not keep both.

**Implementation**: replace the checker's `race` arm with an E027 tombstone. Remove the `fan.race` clause from C-004's title and statement (`fan.any`/`map`/`settle` unchanged), remove the head-Err clause and `spec/wasm_cross/fan_race_err.almd` from C-005, update SPEC.md §9.8/§13.2, README and DESIGN, and add diagnostic fixture `e027-fan-race-removed`. Contract gate green (198 contracts / flagged 0).

One **tombstone chain** emerged during removal: `fixed.almd` in `e027-fan-timeout-removed` (written in 0.29.0 as the migration destination for `fan.timeout`) used `fan.race` and stopped compiling with this removal. **A tombstone's migration destination must be a live surface** — a hint pointing to a dead surface silently breaks at the next removal. Updated it to `fan.any`, and also removed `fan.race` from the `fan.timeout` hint text.

Keep `fan.any` (first Ok in list order). Skipping failures has a nonduplicated meaning.

<a id="1023-err-は兄弟をキャンセルしない--完了2026-07-31"></a>

### #1023 Err does not cancel siblings — **Complete (2026-07-31)**

Remove “siblings are cancelled” from SPEC.md §9.9 / §13.1.

Under the deterministic model, **cancellation must not be implemented**. How far a cancelled sibling's side effects have executed depends on timing, breaking determinism. `fan` always joins all siblings; the block value is **the first `Err` in list order**.

“A nonterminating sibling causes a permanent hang” is **the specification, not a bug**, in this stance. A program that does not terminate under sequential evaluation does not terminate under parallel evaluation either. State that explicitly.

**Implementation**: remove cancellation descriptions from SPEC.md §9.8 / §13.1 / §13.3; explicitly specify join-all and list-order Err. The implementation was correct from the beginning — measurement with a program where both siblings fail shows native and wasm both report `Error: fail:first` (the list-order head). The program makes “first to fail” differ from “first in list order,” so a wall-clock rule would answer differently. Pinned with **C-199** + fixture `spec/wasm_cross/fan_block_err_list_order.almd` (199 contracts / flagged 0).

<a id="1026-trap-した兄弟がプロセスを落とす--完了2026-07-31ただし設計は測定に否定された"></a>

### #1026 A trapping sibling brings down the process — **Complete (2026-07-31), but measurement refuted the design**

This is a real hole. C-004 pins only result order, with an exception inside its statement: **side-effect interleaving is wall-clock on native and sequential on wasm**. The ledger ratchet does not count such “exceptions inside statements,” leaving an uncounted tear in the equivalence claim.

Favor the ideal. **Buffer stdout / stderr for each native arm, then flush in list order after fan resolves**. Wasm already runs sequentially in list order, so this changes native to match wasm. Results:

- C-004's EXCEPTION clause can retire, strengthening the ledger's equivalence claim
- Trap semantics can be defined — if arm k traps, output from arms 0..k-1 is flushed, but output from k onward is not. Identical on both targets
- A new `fan` × trap contract can be written for this converged behavior

The price is streaming (output from long-running arms is invisible until the end). Determinism takes precedence. The ledger observes stdout / stderr / exit code, so file writes and networking are outside this claim — a structural boundary, not a hidden exception.

**Correction from measurement (2026-07-31)**: the buffering design above rested on the **assumption** that trap cases diverged. Actual measurement showed **trap cases already converge** — even with a sibling sleeping for 1.5 seconds, both targets abort at ~0 seconds, with empty stdout, stderr `Error: division by zero`, and exit 1. Thus #1026 needed only a contract, not an implementation. Pinned with **C-200** + fixture `spec/wasm_cross/fan_sibling_trap.almd`.

Buffering remains an answer to **a different question**: in the “both siblings print” case addressed by C-004's EXCEPTION clause, native output interleaves by wall-clock while wasm is sequential. That work is independent of traps and outside this Unit. Good that we did not begin a large implementation on an assumption — measuring before deciding was right.

<a id="1025-兄弟が同じ可変コレクションを-alias-できる--1027-の修正で閉じた"></a>

### #1025 Siblings can alias the same mutable collection — **Closed by the #1027 fix**

This alone is broken regardless of the concurrency model. The frontend accepts it, native ICEs with rustc E0382, and wasm silently discards changes. E008 correctly rejects `var` captures, while **the same danger through call arguments is completely unchecked**.

In the deterministic model, two arms touching the same mutable binding have no definable meaning (assigning list-order meaning makes it sequential execution, with no meaning to the parallelism).

**Investigation on 2026-07-31 found the root one level below fan.** #1025 was reported as a fan alias issue, but fan is not even necessary:

```almide
effect fn pusher(xs: List[Int], n: Int) -> Result[Int, String] = { xs.push(n); ok(n) }
let shared: List[Int] = []
let a = pusher(shared, 1)!
let b = pusher(shared, 2)!
// native: len=2 / wasm: len=0
```

The retained example comment reports native len=2 versus wasm len=0.

The UFCS form `xs.push(n)` bypasses mut-parameter checking (writing the same body as `list.push(xs, n)` correctly rejects it with E007). The cause: `check/calls_ufcs.rs` passes `validate_mut_args` an argument list without the receiver — after UFCS desugaring the receiver is argument 0, exactly the index 0 mut parameter of `list.push(mut xs, x)`.

This is neither a wall nor a trap: it is **cross-target output divergence in a checker-accepted program = wrong bytes**, precisely the class for which the contract ledger exists. **Fix [#1027](https://github.com/almide/almide/issues/1027) before #1025**. **Fixed 2026-07-31** — where `check_call_target_builtin_ufcs` knows the resolved key, it calls `validate_ufcs_mut_args` with the receiver at argument 0. The repro above now produces E007 on both targets, while valid `mut` receivers continue working. 324 spec files green + diagnostic fixture `e007-ufcs-mut-receiver`.

And **#1025 closed with the #1027 fix alone**. Only two paths lead to sibling aliasing in fan, and both are already blocked:

- Destructive modification through UFCS without a `mut` declaration on the argument → **E007** (#1027 fix)
- Argument has a `mut` declaration → caller must use a `var` binding → `var` capture inside fan is **E008** (already existed)

Running the issue's repro as-is gives E007 on both targets; the `mut`-declared version gives E008 on both. No new fan-specific rule was needed — all that was needed was enforcing mutability equally across syntax, and the absence of that was the only hole. The stance (deterministic data parallelism) agrees with this conclusion: a program where two arms touch the same mutable binding cannot be written in the first place. Once mutability is enforced, #1025 reduces to “two arms pass the same `mut` argument,” decidable from declarations alone.

<a id="実装順序044-unit"></a>

## Implementation order (0.44 Unit)

1. **#1027 — missing UFCS mut-parameter check (wrong bytes, highest priority)**
2. #1025 — fan sibling aliasing (after #1027, decidable from declarations alone)
3. #1024 — `fan.race` tombstone + SPEC.md correction
4. #1023 — remove cancellation descriptions from SPEC.md; pin list-order Err with a fixture
5. #1026 — buffer arm output + flush in list order, retire C-004 EXCEPTION, add trap contract
6. Collapse interp's `fan.race` / `fan.any` abstentions if possible (expand the third adjudication tier)

<a id="この決定が閉じないもの"></a>

## What this decision does not close

This decision is **revisitable** if structured concurrency becomes necessary in the future. The condition is to decide the wasm-side answer (sequential fallback, a wall, or flagged divergence) in a contract for each syntax first. The decision says there is no reason to pay that cost now, not that it is “impossible.”
