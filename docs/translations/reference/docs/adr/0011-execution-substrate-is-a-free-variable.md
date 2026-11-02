<a id="adr-0011-the-execution-substrate-is-a-free-variable--and-one-arms-output-is-the-last-hole-in-the-determinism-claim"></a>

# ADR-0011: The execution substrate is a free variable — and one arm's output is the last hole in the determinism claim

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](0011-execution-substrate-is-a-free-variable.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0011-execution-substrate-is-a-free-variable.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](0011-execution-substrate-is-a-free-variable.zh-CN.md)

> Non-normative historical reader snapshot; the unchanged pinned source remains authoritative. No automatic synchronization, independent execution, benchmark rerun, or present safety/equivalence certification. Status remains Proposed with the dated D2a supplement; prospective gains and gates are not claimed as already implemented. The original two-year chronology, old timeout removal versus later oracle-tier forms, and older paths are retained, not reconciled with current releases. Runtime support and exclusivity statements are dated source claims, not newly checked here. Measurements are limited to the stated hardware, versions, repetitions and spike process-launch setup. D1 trap-flush reasoning and broad unconditional-equivalence wording remain original design claims, not universal proof; F1–F6 limitations remain. Linked develop URLs are original mutable references; this reader itself uses the fixed source pin. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0011-execution-substrate-is-a-free-variable.md)

- **Status**: Proposed; **D2 supplemented on 2026-10-04** (§D2a: wasm compute parallelism is per instance, without shared memory — #3003 ruling).
- **Date**: 2026-08-07
- **Scope**: the execution substrate of `fan` (what actually runs concurrently), its selection per target, prerequisites for treating it as a free variable, and correcting the classification of C-004's EXCEPTION.
- **Related**: [async-inception.md](https://github.com/almide/almide/blob/develop/docs/roadmap/active/async-inception.md) (observation charter), [execution-inception.md](https://github.com/almide/almide/blob/develop/docs/roadmap/active/execution-inception.md) (this ADR's charter), [concurrency-stance.md](https://github.com/almide/almide/blob/develop/docs/roadmap/active/concurrency-stance.md) (#1000), [ADR-0001](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0001-deterministic-time-units.md) (deterministic clock).
- **Ledger impact**: the **classification of C-004's EXCEPTION is wrong** (Rationale R1). C-006's “sole stdlib surface” wording is also inaccurate as a consequence.

<a id="context"></a>

## Context

<a id="実測--例外条項を再現した"></a>

### Measurement — Reproducing the Exception Clause

Contract-ledger C-004 has this exception:

> EXCEPTION: side-effect INTERLEAVING inside `fan { }` block arms and `fan.settle`
> thunks is wall-clock on native (both run on real threads) and sequential on wasm —
> they pin their RESULT (tuple / list) order only.

This reads as “output interleaving differs between native and wasm.” **We actually ran it** (2026-08-07, almide 0.56.0 / wasmtime 47.0.2 / 14 cores / arm64 macOS).

```almide
effect fn arm(tag: String, extra: Int) -> Int = {
  let v = spin(6000000 + extra, 1)   // extra は env.args() 由来 — 定数畳み込み不能
  println("${tag}")
  v
}

effect fn main() -> Unit = {
  let n = list.len(env.args()!)
  let (a, b, c, d) = fan { arm("A", n)  arm("B", n)  arm("C", n)  arm("D", n) }
  println("sum=${a + b + c + d}")
}
```

```
native × 10 回 — 出力順:
   2  ACDB
   1  DCAB / DBCA / CBAD / BDCA / BCAD / BACD / ADCB / ACBD
   → 10 回で 9 通り

wasm × 3 回 — 出力順:
   3  ABCD                                    ← 常に arm 順
```

Native and wasm diverge as stated. But an **unstated fact** appeared at the same time.

<a id="見つかった事実--native-は自分自身とも食い違う"></a>

### Finding — Native Disagrees with Itself Too

Nine orders in ten runs. This is not a difference between targets but **observations changing between runs of the same binary with the same inputs**. The ledger has a name for that property. C-006 says:

> Worse than the cross-target divergence: whether the deadline fired depended on
> machine load, so the result was not a function of the program + its inputs
> **even between two native runs** — **the sole stdlib surface violating that property.**

This is precisely why `fan.timeout` was removed in 0.29.0. C-006 says it was the **only** such surface. The measurements above show that statement is inaccurate today. Two printing arms in `fan { }` still ship the same property today.

The oversight is not that there is an exception, but **its classification**. C-004 put it under cross-target differences, a familiar class the ledger handles routinely, including designed differences such as `env.os`. So the clause did not stand out. In reality it was a determinism hole — exactly the class targeted by the 0.29.0 cleanup.

This is the second underestimation of C-004's EXCEPTION. Its end says:

> The block was an undercount here until #915's audit: it spawns per-arm scoped
> threads on native, the same interleaving class as settle.

#915 corrected underestimated **scope**: not only `fan.settle`, but also `fan {}`. This ADR corrects underestimated **class**.

<a id="第-3-のオラクルは-native-の側にいない"></a>

### The Third Oracle Is Not on Native's Side

`crates/almide-interp/src/eval.rs:148` — the reference interpreter's fan is sequential.

> Fan block: evaluate each expr SEQUENTIALLY in source order — the deterministic
> mode both backends collapse to

Of the three-way oracles (native / wasm / interp), **wasm and interp agree on sequential execution; only native diverges**. Two against one; moreover, the outlier requires no debate over which is correct: the sequential side is exactly the observation #1000 defines, “strictly identical to sequential evaluation in list order.”

<a id="基体は誰も決めていない"></a>

### Nobody Chose the Substrate

Why did this happen? **The execution substrate is residue of implementation convenience, not a design decision.**

| Target | Substrate | Origin |
|---|---|---|
| native | OS threads (`std::thread::scope`) | `codegen/templates/rust.toml` `[fan_expr]` / `[fan_effect]` |
| wasm | Fully sequential, inline-expanded arms | `crates/almide-mir/src/lower/desugar_fan.rs` |
| interp | Fully sequential | `crates/almide-interp/src/eval.rs` |

No charter says “OS threads on native, sequential on wasm.” Native uses `std::thread::scope` because Rust had it; wasm is sequential because wasip1 had no threads. **The substrate sat in a charter blank spot, and that blank leaked into observations.**

There are two leak directions. Downward (substrate → observation) is the exception above. Upward (substrate → surface) was also measured: `spec/wasm_cross/fan_race_mapper.almd` finishes normally on wasm but **hits a wall on native** (`op "Prim Handle" in main — outside the rung subset`, exit 1). Charter §3 declares every head × form matrix cell settled, but native sees an empty cell.

(In a two-target comparison of all 23 `spec/wasm_cross/{fan_*,fuel_*}.almd` programs, the other 22 matched stdout and exit code byte-for-byte. One upward leak, zero downward leaks because fixtures avoid placing two printing arms together — that avoidance itself confesses the clause.)

<a id="decision"></a>

## Decision

**The execution substrate is a free variable separated from observation. It is a performance-only knob and must not change even one bit of observation. Obtain wasm concurrency from WASI 0.3 native async, not shared memory or atomics.**

<a id="d1-rung-1arm-単位の出力トランザクションは前提条件であり改善ではない"></a>

### D1. Rung 1, Per-Arm Output Transactions, Is a Prerequisite, Not an Improvement

Each `fan` arm's stdout / stderr enters its own buffer and is flushed in **arm order** at join. Charter §4 pillar 5 already designs this as Rung 1, but this ADR promotes it from “an improvement that comes later” to **the qualification for discussing substrates at all**.

Two reasons, in order of importance:

1. **This is a determinism hole, not performance** (Context). Apply to the same property the same judgment 0.29.0 applied to `fan.timeout`.
2. Ordering cannot be swapped. Parallelizing substrates without Rung 1 **worsens** today's “only native is nondeterministic” into “all targets are nondeterministic.”

Rung 1 must decide three design questions, all by one criterion: matching sequential execution.

- **Relative stdout/stderr order**: independent buffers lose the within-arm `println` → `eprintln` order. Sequential execution interleaves them at fd level, so record **one timeline within an arm**, routing to fds on flush.
- **Flush on trap**: C-200 pins “a trapping sibling goes through unified abort without waiting for in-flight siblings.” Sequentially, outputs of arms preceding the trap are already visible, so **flush completed-arm buffers in arm order before abort**. The C-200 fixture measures “empty stdout” because it has no printing arms, so there is no conflict with the clause (add a fixture).
- **Buffer bound**: an arm printing 1GB buffers 1GB, a cost absent sequentially. The **first arm in arm order may flush immediately**, as no arm precedes it, matching sequential behavior. The worst case therefore becomes “total output of all arms except the first.” If this is insufficient in a real configuration, F4 applies.

<a id="d2-wasm-の並行性は-wasi-03-async-から取る"></a>

### D2. Obtain Wasm Concurrency from WASI 0.3 Async

Use `async func` / `stream<T>` / `future<T>` as substrate, already absorbed into the Component Model's Canonical ABI, with `wasi:io` removed. Neither shared memory nor atomics is needed.

<a id="d2a-補足2026-10-043003-wasm-の計算並列はインスタンス単位で取りメモリは共有しない"></a>

### D2a. Supplement, 2026-10-04, #3003: Wasm Compute Parallelism Is Per Instance, Without Shared Memory

D2 supplies **concurrent I/O**. CPU-bound `fan` chunks do not get faster with Component Model async: it is cooperative concurrency within one instance, and a computing subtask runs to completion. Measured using fannkuchredux (n=10, ten chunks, outputs `73196` / `Pfannkuchen(10) = 38` confirmed equal for all methods, wasmtime 47.0.2, arm64 macOS 14 cores, minimum of five alternating runs):

| Execution method | Wall time |
|---|---:|
| native, threaded `fan` (#2044) | 34.7 ms |
| native, `fan` rewritten sequentially | 178.6 ms |
| wasm, one instance, current | 199.9 ms |
| wasm, p3 component, async ABI | 202.8 ms |
| wasm, separate instance per chunk, sequential | 249.8 ms |
| wasm, separate instance per chunk, ten in parallel | **47.9 ms** |

(The last two rows are spikes. A precompiled module is launched through ten simultaneous `wasmtime run` processes, including all ten process startups.)

Decisions:

1. **The only means of parallel wasm computation is separate instances of the same module running on OS threads.** Each instance remains the same single-threaded module with its own heap. Linear memory is not shared. Emit neither atomics nor shared memory: C-210's `FORBIDDEN = ["relaxed", "atomic", "shared"]`, ownership certificates, and region allocator remain intact (D3 unharmed).
2. Begin with **pure chunks with scalar inputs/outputs**, using the same criterion as native's #2044 threading: no effects or mutable captures; elements, results, and captures are scalars or scalar tuples. A chunk becomes one export; the host calls it across N instances and gathers results in list order. Copy-in/out of heavy data, lists and strings, comes later (#3003 stage 2).
3. Only **hosts that provide it** can execute in parallel, currently the embedded host. There is one artifact. Hosts that do not provide it — plain wasmtime, other WASI runtimes, `to_wasi` / p2 / p3 forms — run sequentially as before. The host chooses the substrate; observations match sequential execution (ADR-0024, C-321).
4. If parallel execution fails, a child-instance trap/abort/exit, discard results and rerun sequentially. Chunks are pure, so retries are unobservable except in time. Sequential execution supplies abort wording and exit code unchanged.

Rejected:

- **wasi-threads + shared memory**: as D3, plus every allocator operation (bump pointer, free list, region-window save/restore) and RC needs atomics or locks, violating C-210 and slowing/enlarging even single-threaded programs. The speed ceiling is the same as this proposal, the same amount of parallel computation, so there is no gain. As of 2026-10-04, wasmtime 47.0.2 still rejects `-S threads=y` at runtime, rechecked.
- **Parallel computation with p3 async subtasks**: the table shows no speedup, 202.8 ms. D2 remains the route for overlapping I/O, prefetching `fan.map(fs.read_text)`, b973a1dd4.

This supplement's falsifier is F6 below.

<a id="d3-wasi-threads--shared-everything-threads-は採らない"></a>

### D3. Do Not Adopt Wasi-Threads or Shared-Everything-Threads

The former is a legacy proposal withdrawn in 2023-08, and wasmtime 47.0.2 **rejects `-S threads=y` at runtime**. The latter is unimplemented in every WASI host runtime (R2).

Consequently `FORBIDDEN = ["relaxed", "atomic", "shared"]` in `crates/almide-mir/tests/deterministic_profile_test.rs` **remains intact**. Do not revise C-210.

<a id="d4-native-の-os-スレッドは維持する"></a>

### D4. Retain Native OS Threads

If substrate differences do not affect observation, choose the fastest substrate per target. That is what free variable means. Making native sequential to match wasm constrains substrate for observation, reversing causality. Parallel execution is explicitly permitted as a performance matter by #1000: “execution may be parallel, but observable behavior is strictly identical to sequential evaluation.”

<a id="d5-基体差分ゲートを新設する"></a>

### D5. Add a Substrate-Difference Gate

Run programs with printing arms on every substrate, native sequential / native threads / wasm sequential / wasm async / interp, **N times each**, mechanically checking stdout, stderr, and exit-code agreement. Repetition across runs is required: the defect is invisible in one run; ten runs revealed nine orders.

<a id="d6-c-004-と-c-006-を訂正する"></a>

### D6. Correct C-004 and C-006

In the PR landing Rung 1, delete C-004's EXCEPTION, lowering the ratchet, and append to C-006's “the sole stdlib surface violating that property” that **output interleaving of `fan {}` / `fan.settle` had existed since 0.24.0 as a second instance of the same class and was closed by this PR**. Correct ledger claims not just when they become true, but also when discovered to have been inaccurate.

<a id="rationale"></a>

## Rationale

<a id="r1-例外条項つきの等価性はこの-repo-の運用と整合しない"></a>

### R1. Equivalence with an Exception Clause Does Not Fit This Repo's Practice

The contract ledger has maintained its `flagged-for-revision` ratchet at zero. C-006 boasts: “This retired the ledger's LAST flagged-for-revision entry: the ratchet stands at zero, so the equivalence claim carries no clause for divergences awaiting a fix.”

The same text admits one exception: platform-reporting `env.os` / `env.temp_dir` (C-189). This carve-out is **by design**, not a repair target.

C-004's EXCEPTION is neither. It is absent from the ratchet and is not a designed carve-out. **It existed unclassified for two years since 0.24.0.** Correctly classified, it should have disappeared in the 0.29.0 cleanup.

<a id="r2-共有メモリで並列にするは-2026-に選択肢として存在しない実測"></a>

### R2. Parallelism through Shared Memory Is Not an Available Option in 2026, Measured

concurrency-stance.md justified wasm's sequential behavior with “wasm32 has no threads.” As of 2026-08, that premise is no longer straightforward: the threads proposal itself is standardized and wasmtime accepts `-W threads` / `-W shared-everything-threads` feature flags. A rejection with a changed basis warrants reconsideration, ADR README rule 2. We reconsidered and ruled it out on hardware.

```
$ wasmtime --version
wasmtime 47.0.2 (90fed3c6a 2026-07-21)

$ wasmtime run -S threads=y t.wasm
Error: the `-Sthreads` flag is no longer supported        ← ヘルプには載っているが拒否される

$ wasmtime run -W shared-everything-threads=y t.wasm
hi                                                        ← flag は通るが提案自体が未実装
```

`-S help` still shows `threads[=y|n] -- Enable support for WASI threading imports
(experimental). Implies preview2=false.` Reading help alone suggests “experimental but usable.” **Execution rejects it.**

Primary information agrees: wasi-threads was withdrawn in 2023-08 in favor of shared-everything-threads, retained as legacy for engines supporting only preview1. Shared-everything-threads is early-stage, with no WASI host-runtime implementation.

**There was no opposing option to defeat.** The C-210/parallelism trade-off does not exist in the 2026 landscape.

<a id="r3-wasi-03-は-atomics-なしに並行-io-をくれる"></a>

### R3. WASI 0.3 Provides Concurrent I/O Without Atomics

WASI 0.3.0 shipped 2026-06-11, introducing native async into the Component Model. `wasi:io` was removed and absorbed into the Canonical ABI, eliminating 0.2's `start-foo` / `finish-foo` / `subscribe` three-step dance. Supported by **Wasmtime 43+**, locally 47.0.2.

Crucially, this is not an instruction-set matter. Concurrency resides in the ABI at component boundaries, without shared linear memory. No emitted atomics/shared memory means C-210 deterministic-profile compliance remains. “Buy parallelism by selling determinism” was a false choice: 2026 wasm can buy without selling.

<a id="r4-憲章-6-の-go-敗北行は決定性を売らずに埋まる"></a>

### R4. The Charter §6 Loss-to-Go Row Can Be Filled Without Selling Determinism

> | Go | Maturity of production async | **The oracle layer is unimplemented; real concurrent I/O races cannot be written** |

The oracle layer needs a contract class treating environment response sequence ω as declared input, already landed in B1, and **a substrate that genuinely overlaps I/O**. D2 supplies the latter: this was the substance of the “matter of time” loss classified by the charter.

<a id="r5-基体非依存性はこの言語だけが機械検査で言える"></a>

### R5. Only This Language Can Mechanically Check and Claim Substrate Independence

Changing parallelism in Go/Rust/JS changes output interleaving. That is not a defect but a design choice making observations a function of schedule. Almide spent two years fixing observation to logical time. **Substrate freedom is the dividend of that investment.**

But it cannot say this today: the measurements above are a counterexample. D1 is the work that buys this sentence.

<a id="alternatives"></a>

## Alternatives

| Proposal | Verdict | Reason |
|---|---|---|
| Adopt wasi-threads and revise C-210 | **Rejected** | Feature does not exist; wasmtime 47.0.2 rejects it at runtime, R2, measured 2026-08-07 |
| Wait for shared-everything-threads | **Deferred to the horizon** | No host-runtime implementation. Reassess D3 if one appears usable without emitting atomics, F3 |
| Make native sequential to remove the exception | **Rejected** | Aligns observation but abandons substrate freedom. Constraining substrate for observation reverses causality, D4. #1000 explicitly permits parallel execution |
| Formalize EXCEPTION as a C-189-style designed carve-out | **Rejected** | `env.os` carve-out is limited to **platform-reporting functions**, whose purpose is divergence. Output interleaving does not aim for divergence; carve-out status merely makes a determinism hole permanent |
| Defer Rung 1 and introduce D2 first | **Rejected** | Worsens divergence from native only to all targets, D1 |
| Status quo | **Rejected** | Leaves C-006 inaccurate and fixes charter claim 2 with an exception clause |

<a id="consequences"></a>

## Consequences

<a id="得るもの"></a>

### Gains

- **The determinism hole closes.** “Observation is a function of (program, inputs)” becomes true including output interleaving.
- C-004's EXCEPTION becomes zero; the equivalence claim loses its exception clause, D6.
- Charter claim 2, native ⇄ wasm observational equivalence, goes from conditional to unconditional.
- A new sixth sentence: **switching substrates does not change observation, mechanically checked**.
- The substrate-difference gate, D5, structurally detects “substrate determines surface” disorders such as the native wall on `fan_race_mapper`.

<a id="払うもの"></a>

### Costs

- **Arm output ceases to appear in real time.** The first arm may flush immediately, D1, but later arms wait for preceding ones to complete. Matching sequential execution is correct by definition, yet long-running arms with progress display feel different, F4.
- Buffer memory cost: worst case is total output of arms other than the first, D1.
- **Componentizing the wasm backend becomes prerequisite work.** Today it emits `wasi_snapshot_preview1` core modules (`render_wasm_p3.rs`). WASI 0.3 async builds on the Component Model; this is not a small migration. This ADR adopts D2 as a **direction decision**, leaving start timing to execution-inception.md.
- More gate runtime: D5 executes the cross-product of every substrate × N repetitions.

<a id="falsifier"></a>

## Falsifier

- **F1**: If switching substrates changes observation in a configuration that Rung 1 cannot close, remove that substrate. If D1 cannot hold, withdraw both D2 and D4 and make every target sequential. Sacrifice substrate freedom to protect observation, never the reverse.
- **F2**: If measured WASI 0.3 async overhead exceeds sequential execution, withdraw only D2 and keep wasm sequential. D1 / D3 / D5 remain regardless of substrate count.
- **F3**: If shared-everything-threads is implemented in a host runtime and becomes usable without overturning C-210 FORBIDDEN, reassess D3. Otherwise retain it.
- **F4**: If real-use complaints show per-arm buffering breaks long-arm progress streaming, **do not offer opt-out**: it restores substrate-dependent observation and invalidates this ADR. Redesign flush-boundary granularity or handle progress through syntax outside `fan`.
- **F5**: If N gate repetitions fail to reproduce the defect, e.g. single-core CI, the gate passes while the hole remains. Redesign checking around **direct substrate selection**, a flag forcing sequential substrate, not repetition count.

- **F6**, D2a: If startup/result-collection costs make per-instance parallelism slower than sequential execution within the eligible pure scalar-I/O chunks, return that shape to sequential execution, narrowing eligibility. If even one configuration changes observations, stdout/stderr/exit code, with parallelism enabled, withdraw D2a and make all wasm sequential, same ordering as F1.

<a id="references"></a>

## References

- [WASI 0.3 · WASI.dev](https://wasi.dev/releases/wasi-p3) — 0.3.0 contents: native async, `async func` / `stream<T>` / `future<T>`.
- [Bytecode Alliance — WASI 0.3 Launched](https://bytecodealliance.org/articles/WASI-0.3) — shipped 2026-06-11, Wasmtime 43+, removal of `wasi:io`.
- [WebAssembly/wasi-threads](https://github.com/WebAssembly/wasi-threads) — legacy proposal, withdrawn 2023-08.
- [WebAssembly/shared-everything-threads](https://github.com/WebAssembly/shared-everything-threads) — successor draft, no host-runtime implementation.
- [Roadmap · WASI.dev](https://wasi.dev/roadmap) — 0.3.x release train and WASI 1.0, late 2026 to early 2027.
- This repo, measured 2026-08-07, almide 0.56.0 / wasmtime 47.0.2 / 14-core arm64 macOS: four-arm `fan {}` produced nine orders in ten native runs versus one order in three wasm runs; both targets compared on 23 `spec/wasm_cross/{fan_*,fuel_*}.almd` programs; `codegen/templates/rust.toml` `[fan_expr]`, `crates/almide-interp/src/eval.rs:148`, `crates/almide-mir/src/render_wasm_p3.rs`.
