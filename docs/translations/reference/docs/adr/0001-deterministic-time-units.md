<a id="adr-0001-deterministic-budgets-are-written-in-time-units"></a>

# ADR-0001: Deterministic budgets are written in time units

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](0001-deterministic-time-units.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0001-deterministic-time-units.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](0001-deterministic-time-units.zh-CN.md)

> Non-normative historical reader snapshot; unchanged pinned original authoritative. No autosync, independent benchmark/proof rerun or current syntax/feature guarantee. Preserve this August decision, including later-superseded fan forms, planned calibration gates and unit-independent proof claims; these are not evidence that every implementation or host meets them today. Source tensions are retained rather than repaired: “under 2 seconds” versus the displayed 0.514 total; the 54× CPU comparison uses the quoted user-time figure rather than recomputing user+system; frozen relative-cost/error-correction claims versus possible absolute changes; the stated 12-character difference is not recalculated. The source itself says Stage 1 will provide the first calibration measurement. Oracle equality is conditional on fixed ω, not equality of live wall-clock runs. Code fences and quotations retain original language and bytes. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0001-deterministic-time-units.md)

- **Status**: Accepted
- **Date**: 2026-08-01
- **Decision scope**: surface notation for computational budgets in `fan.bounded` / `fan.race`, and the units of the deterministic clock
- **Related**: [async-inception.md](https://github.com/almide/almide/blob/develop/docs/roadmap/active/async-inception.md) (charter), [logical-time-async.md](https://github.com/almide/almide/blob/develop/docs/roadmap/active/logical-time-async.md) (semantics), [fan-v2.md](https://github.com/almide/almide/blob/develop/docs/roadmap/active/fan-v2.md) (grammar)
- **Ratification**: the current form was ratified in the 2026-08-01 discussion of two requirements: “write conventional ms” × “completeness.” The single Duration type was considered and rejected (see Alternatives); the wall-clock completeness mechanism was added as S8.

<a id="前提--時間は-1-つではない"></a>

## Premise — there is more than one kind of “time”

This ADR decides to write budgets in time units. First, we must share the distinction that **even the same “seconds” can measure different things**. Without that distinction, the discussion below is unreadable.

At least two different times can be taken from one execution.

```
プログラム開始 ──────────────────────────────────────────── 終了
              [計算]   [ネット待ち]   [計算]   [ディスク待ち]  [計算]
               30ms      500ms        20ms      200ms        10ms

  壁時計時間 = 760ms   ← 部屋の時計で測った経過。待ち時間も込み
  CPU 時間   =  60ms   ← CPU が実際にこのコードを実行していた分だけ
```

You can check locally. Measuring a process that merely sleeps for 2 seconds and one that only computes for just under 2 seconds with the same `time` gives the following (measured):

```
$ time python3 -c "import time; time.sleep(2)"
0.04s user   0.02s system   2% cpu   2.173 total     ← 壁時計 2.17s / CPU 0.04s

$ time python3 -c "x=0
for i in range(8000000): x+=i"
0.39s user   0.03s system  80% cpu   0.514 total     ← 壁時計 0.51s / CPU 0.39s
```

Above, **wall time is 54 times CPU time**. CPU time does not advance while sleeping. Cloudflare's taxi analogy is apt: the meter keeps running while the driver stops for fuel and a snack. That is wall time; **only the actual driving** is CPU time.

We need the distinction because wall time **changes for reasons outside your code**: an API's slow day, a busy machine, a neighboring process. This is also why Cloudflare chose CPU-time pricing: “CPU time is more predictable and **under your control**… **purely a function of the logic and processing of inputs on outputs to your Worker**.”

Almide's **deterministic clock takes this direction one step further**.

| | Includes waiting | Depends on machine speed | Function of (program, input) |
|---|---|---|---|
| Wall time | **Yes** | Yes | ✗ |
| CPU time | No | **Yes** (shorter on a faster CPU) | ✗ |
| **Deterministic clock** (this design) | No | **No** | **✓** |

CPU time is still machine-dependent. The same code finishes sooner on a faster CPU, so its value is not the same on every machine. The deterministic clock takes the final step of **not measuring**: a frozen abstract-machine cost table (CM-1) defines “how many nanoseconds this op represents,” and executed ops are summed. Because it is not measured, the value is **always the same** on fast or slow machines, native or wasm.

Thus `compute.ms(100)` means not “wait for 100 milliseconds of wall time” but **“perform 100 milliseconds' worth of computation.”** It might finish in 40ms wall time on a fast machine or take 300ms on a slow one. Even then, **it is cut off at the same point on every machine**: that is what “deterministic” means.

“Seconds, but not wall time” is not itself new. POSIX `ulimit -t 60` (kill a process after 60 CPU seconds) has operated for 50 years. The most accurate positioning is that this design adds **machine independence** to that.

<a id="用語集"></a>

## Glossary

| Term | Meaning | Reference |
|---|---|---|
| **Wall clock** | Host real time, the quantity returned by `Date.now()` / `Instant::now()`. Depends on host speed, load and scheduler; **not** a function of (program, input) | General term |
| **Logical clock** | A measure of progress independent of physical time. **Beware the terminology collision**: distributed-systems Lamport clocks count the happens-before partial order, a different concept from this ADR's deterministic counter advanced by the program's own execution | [Lamport 1978](https://lamport.azurewebsites.net/pubs/time-clocks.pdf) (a different concept with the same name) |
| **Deterministic clock** | This design's logical clock. Advances at each charge site according to CM-1. Its value is a function of (program, input), identical across native, wasm and all hosts. Paired with the wall clock; **the fan head declares which one it reads** | [logical-time-async.md](https://github.com/almide/almide/blob/develop/docs/roadmap/active/logical-time-async.md) |
| **charge site** | The **only point** where the deterministic clock advances, placed at shared-MIR basic-block entries, for example. Fuel constructs read the counter here; oracle constructs read the environment at the same point (unified interruption-point principle) | [logical-time-implementation.md](https://github.com/almide/almide/blob/develop/docs/roadmap/active/logical-time-implementation.md) |
| **Lockstep semantics** | The picture for race winner selection: all arms advance the deterministic clock at the same pace, the first to finish wins, ties use source order. The implementation's equivalent characterization is the lexicographic minimum of (consumption, index). **Agreement of these two** is the substance of “race without physical time” | [logical-time-async.md §deterministic event rules](https://github.com/almide/almide/blob/develop/docs/roadmap/active/logical-time-async.md), [Decisive.lean `Ev.prec`](https://github.com/almide/almide/blob/develop/crates/almide-race-belt/AlmideRaceBelt/Decisive.lean) |
| **Versioned abstract cost table** (CM-1) | Assigns a deterministic-clock increment to each MIR op. A **semantic object** in the contract ledger: changing a constant is a semantic change (version bump). EVM's gas schedule is an analogous precedent | [Yellow Paper](https://ethereum.github.io/yellowpaper/paper.pdf) Appendix G |
| **EVM** (Ethereum Virtual Machine) | Ethereum's execution environment. All nodes apply the same gas schedule to the same state transition and reach the same out-of-gas decision to preserve consensus. **The largest-scale production example of a deterministic computational limit** | [Yellow Paper](https://ethereum.github.io/yellowpaper/paper.pdf) / [ethereum.org: gas](https://ethereum.org/developers/docs/gas/) |
| **The race belt's 7 theorems** | Lean 4 theorems in `crates/almide-race-belt/` (0 sorry; kernel-checked in CI `lean-proofs`): decisive-event uniqueness, subset stability, pruning caps cannot hide the decisive event or trap visibility window, confluence. **All are unit-independent** (event times are `Nat`), so this ADR's unit change alters not one character | [Decisive.lean](https://github.com/almide/almide/blob/develop/crates/almide-race-belt/AlmideRaceBelt/Decisive.lean) |
| **fuel** | A **mechanism-side** term implementing the deterministic clock (internal counter, `--fuel-probe`, Wasmtime / EVM lineage). Not exposed as surface vocabulary | [Wasmtime: fuel vs epoch](https://docs.wasmtime.dev/examples-interrupting-wasm.html) |

<a id="context"></a>

## Context

The surface notation for deterministic computational budgets (`fan.bounded` / `fan.race`) needed a decision. The design passed through three options.

1. **`fuel:`** — the name reveals the Wasmtime / EVM lineage. Rejected: the mechanism metaphor leaks into the surface. Fuel is fuel, not a unit.
2. **`ticks:`** — “label = unit name” aligns with `ms:`. The unit of lockstep semantics itself. This was coherent so far.
3. But `ticks: 100_000` has the flaw of **a number one cannot write**.

The third flaw starts this ADR. Absolute ticks depend on CM-1 (the versioned abstract cost table), and neither humans nor LLMs have prior intuition for them. Moreover, the budget couples to the body's implementation: refactoring `optimal_plan` makes `100_000` stale. **Requiring an edit-sensitive magic number as an argument nearly contradicts a language's claim to modification survival.**

Budget errors are **silent in both directions**. Too small always falls back; too large is effectively unlimited. Neither is caught by diagnostics.

<a id="decision"></a>

## Decision

**Write deterministic computational budgets in time units. The type carries the unit; construction uses function calls in the existing syntax (no new literal syntax).**

```almide
fan.bounded(compute.ms(100)) { optimal_plan(g) } ?? greedy_plan(g)   // 決定的時計
fan.race { exact(p); heuristic(p) } ?? fallback(p)                    // 予算なしが基本形
fan.race(compute.ms(500)) { search_a(p); search_b(p) } ?? none        // 発散ガードは任意
fan.timeout(duration.s(5)) { http.get(url) } ?? cached                // 壁時計（oracle 層）
```

The following are also fixed.

<a id="d1-論理時計の単位は時間である"></a>

### D1. The logical clock's unit is time

CM-1 assigns each MIR op a **duration on the frozen Almide abstract machine**. Program consumption is their sum: one duration, a function of (program, input), returning the same value on every host. **What is counted is deterministic (the fuel side); what it is called is time.** These can be separated; D6 is evidence.

<a id="d2-型で分ける単位ではなく"></a>

### D2. Separate by type, not by unit

Deterministic-clock and wall-clock quantities are **different types** (names fixed: `Compute` / `Duration` — S1). Both share ms / s. Heads distinguish them at the call site, but only types can prevent mixing via variables or configuration records. These are **wrapped types**, like Go's `time.Duration`, expressible in the current syntax (a single-case variant `type Compute = Compute(Int)`, or a compiler-known type like `fan`). The implementation PR fixes the representation; **neither requires additional language syntax**.

**A bare integer is a type error.** `fan.bounded(100)` does not compile: “expected Compute, found Int.” Unitless-integer APIs have ample thousandfold incidents, from Go's `time.Sleep(10)` (10 nanoseconds) to Jenkins's “300 seconds → 3.5 days.”

<a id="d3-構築は現行構文の関数呼び出し--新しいリテラル構文は追加しない"></a>

### D3. Construct with existing function-call syntax — no new literal syntax

Module-function names carry the unit. **The module names the clock; the function names the unit.**

```almide
compute.ms(100)     compute.s(2)      compute.us(500)     // 決定的時計 → Compute
duration.s(5)       duration.ms(300)  duration.min(3)     // 壁時計 → Duration
compute.ms(n)                                             // 変数もそのまま
```

The unit-name set is `ns` / `us` / `ms` / `s` / `min` / `h` (ASCII only, not `µs`; minutes use `min`, since `m` is ambiguous). Do not include days or larger units (the same reason Go has no Day). Put this set **in a completeness-matrix gate**: manually maintained surfaces drift, and LLMs invent `msec` or `5m`.

**Do not adopt literal suffixes (`100ms`).** That was the first edition's decision, but adding numeric suffixes to the lexer requires **new language syntax**. Almide currently lacks the mechanism; broadening the entire language's lexical rules just for the `fan` surface is not worth it. Function form costs only more characters; explicit units, type safety and gateability remain identical.

If `100ms` is added later, **these functions are its desugaring target** (`100ms` ≡ `compute.ms(100)`). This decision does not foreclose future sugar. The condition for adding sugar is D3-F below.

<a id="d4-抽象機械は凍結する"></a>

### D4. Freeze the abstract machine

Correct model errors (an op's relative cost was wrong). **Do not follow hardware evolution by rescaling the whole model.** Over 10 years the ratio to wall time will drift severalfold, but that is the same order as machine variation already present today. In exchange, we buy “program meaning never changes.”

<a id="d5-校正はゲートで機械検査し対応幅を宣言する"></a>

### D5. Mechanically check calibration with a gate and declare its range

CI measures the ratio of deterministic ms to measured wall ms and checks the declared band. Document workload classes outside the band. Reports **always display both**:

```
this region: 0.42ms deterministic (≈0.31ms wall here)
```

<a id="d6-比のみが契約絶対値は非契約を初日に宣言する"></a>

### D6. Declare on day one: only ratios are contractual; absolute values are not

Relative costs between ops are contractual; absolute values are not. A CM version bump may move absolute values.

<a id="仕様--時間指定の完全な表面normative"></a>

## Specification — complete time-setting surface (normative)

Turn D1–D6 into a complete surface specification that implementation PRs can transcribe directly. **Completeness follows the repository's matrix principle**: enumerate the existing cells across the surface, declare absent cells as deliberate omissions with reasons and reopening conditions, and inspect the surface with executable gates.

<a id="s1-型"></a>

### S1. Types

| Type | Meaning | Construction module | derive |
|---|---|---|---|
| `Compute` | Deterministic-clock quantity (the premise table's third row) | `compute` | `Eq, Ord, Repr` |
| `Duration` | Wall-clock quantity | `duration` | `Eq, Ord, Repr` |

- **Representation**: nanoseconds internally, one `Int` (i64). Maximum ≈292 years. Same ceiling as Go, but research found incidents caused not by the ceiling but **wraparound** (Swift's immediate-return bug with `sleep(nanoseconds: .max)`) and **D×D** (Go #64420). Eliminate the former with saturating arithmetic (S3), the latter with a type error (S3).
- Both types have **the same operation surface** (duplicate the S2/S3 matrix per type; gates check both).
- Compiler-known type or stdlib `.almd` type is up to the implementation PR. This section fixes the observable specification, which is identical either way.

<a id="s2-構築--2-時計--6-単位の完全-matrix"></a>

### S2. Construction — complete 2-clock × 6-unit matrix

| Unit | `compute.*` | `duration.*` |
|---|---|---|
| `ns` | `compute.ns(n)` | `duration.ns(n)` |
| `us` | `compute.us(n)` | `duration.us(n)` |
| `ms` | `compute.ms(n)` | `duration.ms(n)` |
| `s` | `compute.s(n)` | `duration.s(n)` |
| `min` | `compute.min(n)` | `duration.min(n)` |
| `h` | `compute.h(n)` | `duration.h(n)` |

All 12 cells exist and are gated. Rules:

- Arguments are `Int` only (`Float` versions are deliberately omitted — S7).
- **Negative values deterministically trap** (ledger-constant message, pinned by a fixture matching observations on both targets). Constructed values have invariant ≥0; zero is valid.
- Multiplication overflow during construction (such as `compute.h(大値)`) **saturates**.
- **UFCS form `n.ms()` is an ambiguity error**: both `compute.ms` and `duration.ms` match receiver `Int`. Inference must not choose the clock; a silent default contradicts why this ADR exists. The diagnostic names both candidates and requires qualified form.
- Share the unit-name set with the CLI (S4) from **one source** (the `scripts/lib/contract-classes.txt` approach).

<a id="s3-演算-matrix"></a>

### S3. Operation matrix

`T` is each of `Compute` / `Duration` (the same surface per type):

| Operation | Decision | Reason |
|---|---|---|
| `T + T → T` (saturating) | **Present** — compose budgets/deadlines | |
| `T - T → T` (saturating at zero) | **Present** — remaining amount | Never negative (invariant ≥0) |
| `T * Int → T` / `Int * T → T` (saturating) | **Present** — asymmetric scaling | Go #20757 lesson: “scale by a number, not by a duration” |
| `T * T` | **Type error** | Go #64420's silent 10⁹-fold incident. Go could not express this in types and needed a dedicated linter; we eliminate it with types |
| `T / Int` | Deliberate omission (S7) | Defer rounding semantics |
| `==` / `<`, etc. (within one type) | **Present** (`Ord`) | |
| `Compute ⊕ Duration` (all operations) | **Type error** | Mixed clocks: exactly why Ada used separate types |
| `Int ⊕ T` (except scaling) | **Type error** | Block bare integers (D2) |
| `Compute ↔ Duration` conversion functions | **Do not exist** | There is no semantic bridge between the clocks. The only bridge is side-by-side tooling reports (D5), not values |

<a id="s4-消費者-matrix--どこに何の型が現れるか"></a>

### S4. Consumer matrix — which type appears where

| Surface | Type consumed | Clock | Multiplicity |
|---|---|---|---|
| `fan.bounded(c) { body }` | `Compute` | Deterministic | Required 1 |
| `fan.race { arms }` / `fan.race(c) { arms }` | `Compute` | Deterministic | Optional 0..1 (divergence guard) |
| `fan.race(xs, f)` / `fan.race(c, xs, f)` | `Compute` | Deterministic | Optional 0..1 |
| `fan.timeout(d) { body }` (Stage 4, oracle layer) | `Duration` | Wall (S8) | Required 1 |
| Deadlines on effect surfaces (http/process, etc.; shape is future design, only type is pinned here) | `Duration` | Wall (S8) | — |
| CLI `almide run/test --budget 500ms` | `Compute` (CLI strings use suffix notation for the same 6 units; the CLI is not language syntax, so suffixes are allowed) | Deterministic | Optional |
| Reports (`--time-report`) | Both side by side: `0.42ms deterministic (≈0.31ms wall here)` | Both | D5 |

The clock column corresponds 1:1 with contract-ledger declarations; gate S6-6 checks this surface.

Pin misplacement diagnostic wording in fixtures (`expected Compute, found Duration` plus a head-guidance hint, matching fan-v2-examples/diagnostics.almd).

<a id="s5-端の意味論"></a>

### S5. Boundary semantics

- **Zero budget**: check-then-charge means an expression finishing before a charge succeeds: `fan.bounded(compute.ms(0)) { 42 }` → `Ok(42)`. An expression hitting the first charge exhausts.
- **Negative**: construction-time traps (S2) prevent negative values from reaching consumers.
- **Saturation ceiling**: maximum `Compute` is finite (≈292 years), though infeasible to execute. Treat it separately from race's “no budget = infinite n”: no budget is absence of an argument, whereas the ceiling is a value.

<a id="s6-実行可能ゲート実装-pr-と同一-pr-で-land"></a>

### S6. Executable gates (land in the same PR as implementation)

1. Existence and type signatures of all 12 constructors (S2 matrix surface check).
2. Fixture matching negative-value trap observations across both targets (`spec/wasm_cross`).
3. One diagnostic fixture per type error: bare integer / mixed clocks / `T * T` / UFCS ambiguity.
4. CLI unit-name set = language unit-name set (check generation from one source).
5. Saturation boundary fixtures (overflow construction, `T - T` saturating at zero).
6. **Clock-declaration surface check**: the complete set of surfaces accepting `Compute` / `Duration` matches all S4 clock declarations (= ledger declarations). CI rejects a time-consuming surface without a clock declaration.

<a id="s7-意図的省略の台帳再開条件つき"></a>

### S7. Deliberate-omission ledger (with reopening conditions)

| Omission | Reason | Reopening condition |
|---|---|---|
| Units `d` (day) and above | Meaningless for budgets/deadlines; same reasoning as Go's lack of Day | Recorded real demand |
| `Float` constructors (`duration.s(1.5)`) | Go #20757's float-misuse class; integers suffice in `ms` | Integer-conversion writing mistakes observed in dojo |
| `T / Int` | Defer rounding semantics (truncate/nearest) | Real demand |
| `min` / `max` helpers | Semantics implicitly handles nested min-cap (EIP-150 style) | Demand for explicit budget composition |
| Accessors (reads such as `in_ms(c)`) | `Repr` suffices for display | A real example using budgets in computation |
| `Codec` derive | No use case crossing serialization boundaries | A use case appears |
| Literal suffix `100ms` | D3: lexer-extension trade-off not justified; desugaring target already fixed | D3-F (dojo measurement) |

<a id="s8-壁時計側の完全性--timeout-が従来の-ms-で書けてなお完全である理由"></a>

### S8. Wall-clock completeness — why timeout can use conventional ms and remain complete

`fan.timeout(duration.ms(5000))` and effect-surface deadlines can use customary ms just as general-purpose languages do. Three layers make this consistent with completeness of the deterministic layer. That wall-clock winner/result selection cannot enter the deterministic layer is theorem T9, not a preference.

1. **Clock-declaration matrix (S4 clock column)**: every time-consuming surface declares the clock it reads in the ledger. Gate S6-6 rejects undeclared surfaces. The ledger can always answer “which clock is this ms?”, ensuring clocks do not mix despite sharing units.
2. **Timeout events are part of ω**: expiry is oracle input recording “at which charge site the environment expired the deadline” (unified interruption-point principle: oracle constructs read the environment at the deterministic layer's sites). The contract class is oracle-relative (R_Ω): the claim is relative, “observations agree for every ω.”
3. **Record/replay is executable evidence**: replay with recorded ω reproduces observations byte for byte, including recording on native and replaying on wasm by definition. **Do not claim** cross-target equality of live runs. This is the oracle layer's honest boundary: agreement is claimed “with ω fixed.”

In one sentence: **wall time is not nondeterminism but declared input.** Input can be recorded, recording makes reproduction possible, and reproduction brings it within contracts and gates. Conventional ms interfaces and completeness coexist on this structure.

<a id="rationale"></a>

## Rationale

<a id="決定的だったのは計算予算は実質すでに時間であるという発見"></a>

### The decisive discovery: computational budgets are already effectively time

EIP-2929 Motivation, verbatim:

> "Generally, the main function of gas costs of opcodes is to be **an estimate of the
> time needed to process that opcode**, the goal being for the gas limit to correspond
> to a limit on the time needed to process a block."

Primary evidence rejects the common belief that gas deliberately avoids time. Gas estimates time; only its unit is dimensionless. ewasm is more explicit, fixing conversion to a named 2014 Haswell CPU: “1 second of CPU execution equals to 10 million gas.” Solana internally uses 30 CU/µs, and eBPF's one-million-instruction limit came from “0.1 seconds on a typical x86.”

Go pprof even returns “cpu / nanoseconds” that is actually **sample count × a 10ms constant** (`b.period = 1e9 / hz`, `hz = 100`). It is not reading a clock. Count × calibration constant presented as nanoseconds is already in production, and nobody calls it a lie.

The question is therefore not whether computation may be written as time, but **whether to call an existing time estimate time, or keep dressing it in an abstract-unit name**.

<a id="抽象単位に留まる理由が我々には無い"></a>

### We lack the reasons to retain abstract units

Other systems' reasons for dimensionless units reduce to two, neither applying here.

- **Gas is a currency unit.** Charging multiplies by gas price; the economy requires gas units. Almide has no price dimension.
- **EVM assumes repricing.** Labeling a repeatedly revised empirical relationship ms breaks the unit's meaning at each revision. That is an argument **when rescaling**; D4's freeze answers it. Precedents for freezing are gem5 (`1 Tick == 1 ps`, changing it after fixing it panics) and POSIX (`CLOCKS_PER_SEC` frozen at 10^6; the standard admits actual resolution need not be microseconds).

<a id="抽象単位では直感が育たないことが全系で実証されている"></a>

### Every system demonstrates failure to develop intuition for abstract units

> **Research conclusion: “We found no system in which users succeeded at intuiting the number.”**

- **EVM**: `eth_estimateGas` uses binary search. The 63/64 rule makes required gas nonmonotonic (gas used ≠ required gas limit). MetaMask's automatic setting, falling back to 95% of the block limit when estimation fails, broke UX.
- **NEAR**: in the 200 Tgas era, **9.2% of Aurora's 274,000 transactions failed by hitting the limit**, accounting for 26% of gas usage. At 300 Tgas, failures fell to 1.0%. The reference FT implementation attaches 30 TGas, multiples of actual consumption; **over-allocation became the industry's standard workaround**.
- **eBPF**: “4k limit was confusing to users”; “adding code passes the verifier, deleting code fails.”
- **Earth Engine EECU**: “very difficult to estimate.”
- **F\***: `--fuel` says to work at default 0 and document nonzero values: a design fixing an untunable abstract unit at zero.

All systems use the same remedy: run-and-measure estimators, safety margins, and large defaults. **Retreating into abstract units does not solve the problem.**

<a id="時間単位を表面に出した先例は存在する"></a>

### There are precedents for exposing time units

- **Cloudflare Workers**: the configuration key is literally `"limits": { "cpu_ms": 300000 }`. Wall time has “No limit” (unlimited HTTP duration); only CPU time is capped. Its rationale matches ours verbatim: “CPU time is more predictable and **under your control**… **purely a function of the logic and processing of inputs on outputs to your Worker**.”
- **NEAR**: “we define 10^15 gas to be executed in at most 1s… 1 Tgas corresponds to **1ms execution time**.” Moreover, Runtime Parameter Estimator **mechanically checks** existing parameters against the 1ms = 1 Tgas rule (a D5 precedent).
- **F\* `--z3rlimit`**: calls it deterministic and machine-independent, then says n intuitively counts permitted seconds on a **mythical powerful laptop**; “correspondence is quite good, **it's not perfect**” (D5's honest form).
- **Ada**: `type CPU_Time is private`; seconds as units, but **a distinct type** from `Real_Time.Time` (a D2 precedent).
- **gem5**: displays `simSeconds` / `simTicks` / `hostSeconds` together under separate names (D5's form).

<a id="msr-の観点では時間単位が正しさに効く"></a>

### From the MSR perspective, time units affect correctness

An LLM has never seen a tick budget. `ticks: 100_000` can miss by six orders of magnitude. `100ms` has a prior distribution. Since budget mistakes are silent in both directions, **a guessable number is a correctness issue, not convenience**.

<a id="単位の変更が意味論の変更でないことは機械検査で担保されている"></a>

### Mechanical checks establish that changing units does not change semantics

Neither Lean's `Ev { time : Nat }` nor the exhaustive model's `u64` depends on units at all. Reading ticks as ns changes not one character of the [race belt](https://github.com/almide/almide/tree/develop/crates/almide-race-belt)'s 7 theorems or the 74,898-configuration confluence gate. Wasmtime's named axis (fuel = deterministic / epoch = wall-clock nondeterministic) concerns **what is counted**, not what it is called. Other systems had no reason to separate them; we have evidence they can be separated.

<a id="我々に有利な構造的事実"></a>

### Structural facts in our favor

The real cause of poor estimation is not abstract units but **state-dependent and nonmonotonic costs** (EVM's 63/64 rule, eBPF verifier search). **Almide's costs are monotonic and state-independent**; that pathology is absent from the outset.

<a id="alternatives"></a>

## Alternatives

| Option | Reason for rejection |
|---|---|
| **`fuel:`** | Mechanism metaphor leaks into the surface. Fuel is not a unit |
| **`ticks:`** | Unwritable numbers (Context); zero systems developed intuition for abstract units |
| **Qualified labels such as `compute_ms:`** | Cloudflare qualifies `cpu_ms` because wall time coexists in configuration. Almide's head names the clock (`bounded` / `timeout`), making the label redundant. Types (D2) do the same job more strongly |
| **Postgres-style compromise** (dimensionless default, ms opt-in for a particular environment) | Postgres cost constants are **administrator-tuned configuration**, so both can coexist. This cannot transfer to source-code budgets; two spellings of one quantity create coexistence debt |
| **Define a physical reference machine** (F\* / ewasm) | It ages. ewasm explicitly adjusts every 3 years as CPUs improve; adjustments break existing program meaning (EIP-1884 broke `transfer()` assuming the 2300 gas stipend; an Aragon function crossed the threshold at 1759→2359 gas). Adopt D4's frozen abstract machine |
| **User-extensible literal suffixes** (C++ UDL) | Google C++ Style Guide forbids even standard-library `100ms`, all for **consequences of extensibility**: no namespace qualification, required `using`, third-party `_ms`. Do not build the extension mechanism |
| **Closed language-owned literal suffixes** (CSS, `100ms`) | **Deferred, not rejected.** CSS shows a closed set works, so the design is sound. But numeric lexer suffixes require **new language syntax**; broadening lexical rules just for `fan` is not worth it. Same meaning as function form, which is the future desugaring target (D3) |
| **Constructor functions only** (Rust / Swift) | **Adopted** (D3). Kotlin deprecated concise forms in 1.5 toward static factories, then reverted in 1.6: evidence concise forms are preferred. But Kotlin implements them with existing **extension properties**, needing no new lexical rules. Almide lacks that feature, so the same concision has a different price |
| **Bare integers + documentation** (Go) | Go's `time.Sleep(10)` is 10 nanoseconds. Real incident hudl/fargo #56; staticcheck has SA1004. The type-checked `Duration × Duration` problem (#64420) closed as *not planned*, requiring a dedicated linter. Go's team itself acknowledged in #20757 that Go Duration is more dangerous than Rust/Java/C#/Python. Swift and Zig both recently moved from bare-integer APIs to typed Duration |
| **One Duration type, clock named by consuming syntax** (`ulimit -t` / Cloudflare `cpu_ms`) | **Considered and rejected (2026-08-01 discussion).** Considered even the argument that bounded/race arms are pure with no I/O waiting, so compute and wall time agree within calibration and call-site mixing causes little harm. Rejected because clock mixing through config/variables (`cfg.timeout` into `bounded`) is stoppable only by types: that firewall is the purpose of two types (Ada precedent, D2). The user chose the firewall over shorter vocabulary |

<a id="前回の却下を訂正する"></a>

### Correcting the previous rejection

[ticks-interface-audit.md](https://github.com/almide/almide/blob/develop/docs/roadmap/active/ticks-interface-audit.md) rejected “time-like units converted to ticks (`budget: 100.ms`)” as **replaying the misreading in reverse**. That **confused severity**. The `fan.timeout(1000)` incident meant wall-clock dependence breaks determinism; writing logical time in time units **does not break determinism at all**. Only correspondence to wall time shifts. The earlier decision to put them in the same category was wrong. This ADR supersedes that section.

<a id="consequences"></a>

## Consequences

**Gains**

- Writable numbers: both humans and LLMs have a prior for `100ms`.
- One unit system: deterministic and wall clocks share ms / s vocabulary, distinguished by heads and types.
- Labels disappear from the parser. `fan.bounded(<expr>) { body }` suffices; no head-argument label parsing is needed (fan-v2's surface shrinks).
- **Zero added language syntax.** Adding types and stdlib modules suffices; neither lexer nor parser changes.
- An external anchor for the cost table. A tick table “cannot be wrong,” hence cannot be calibrated; a time table **can be wrong against a reference machine, hence can be corrected**. CM revisions become convergence toward truth rather than arbitrary drift, with mechanically gated calibration (D5).
- Bare integers become type errors; unit-confusion incidents (Gearman's 17-minute timeout, Jenkins's 300 seconds → 3.5 days) are eliminated by syntax.

**Costs**

- **Spread between the label and reality.** Despite being cycle-accurate, gem5 has mean absolute runtime error 13–17% against hardware (ARM servers: MAPE 26–30%). Our weighted-op counts model neither caches, branch prediction nor memory hierarchy, so some workloads may differ severalfold. D5's band declaration and paired reports honestly address the range behind the ms label.
- **Possible misreading.** Someone reading “returns in 100ms” sees 300ms wall time on a slow machine. But **program behavior is identical on all hosts**: only wall-latency expectations break; correctness and determinism remain intact. Its severity differs by two orders of magnitude from `fan.timeout(1000)`'s nondeterminism incident.
- **Length.** `compute.ms(100)` is longer than `100ms`: a 12-character difference at the most frequent call site.
- **Two additional types** (`Compute` / `Duration`). This touches Vocabulary Economy, justified by Ada and D2's incident prevention.

**Unchanged**

- Lean's 7 theorems, the 74,898-configuration confluence gate, T1–T9: all unit-independent.
- Semantics: lockstep, deterministic event rules, trap visibility window, nested min-cap.
- The surfaces of `fan {}` / `fan.map` / `fan.any` / `fan.settle`.

<a id="falsifier"></a>

## Falsifier

- **If calibration-gate measurements show too wide a range** (beyond the declared band, roughly fivefold), remove the ms label and return to a dimensionless name. Given gem5's 13–30%, there is no guarantee yet that ours will stay within severalfold. **The Stage 1 probe produces the first measurement.**
- If misreading becomes an observed incident class (code causes real harm by believing a `fan.bounded` budget is wall time), make D2 type names self-explanatory or return to qualified labels.
- **D3-F**: if function form measurably lowers MSR (dojo shows significant mistakes writing `compute.ms(100)`), add literal suffix `100ms` as sugar. Its desugaring target already is function form, so semantics do not change.
- If following hardware evolution becomes unavoidable (abandoning D4's freeze), supersede this entire ADR: freezing is a precondition for ms notation.

- If negative-value traps prove too harsh in practice (recorded legitimate patterns where computed constructor arguments go negative), revise construction to saturate at zero; S5's invariant survives.
- If UFCS ambiguity errors lower MSR (frequent missing qualifications), consider a short qualified form. Implicit clock selection is not an option.

<a id="references"></a>

## References

**Primary sources**

- [EIP-2929](https://eips.ethereum.org/EIPS/eip-2929) — gas costs are “an estimate of the time needed”
- [EIP-150](https://eips.ethereum.org/EIPS/eip-150), [EIP-1884](https://eips.ethereum.org/EIPS/eip-1884) — repricing and breaking existing contracts
- [ewasm: determining wasm gas costs](https://github.com/ewasm/design/blob/master/determining_wasm_gas_costs.md) — fixed 2014 Haswell, adjustment every 3 years
- [NEAR nomicon: Gas](https://nomicon.io/architecture/gas/index.html) — “1 Tgas = 1ms”
- [NEAR parameter definition](https://near.github.io/nearcore/architecture/gas/parameter_definition.html) — mechanical checking by estimator
- [NEPs discussion #305](https://github.com/near/NEPs/discussions/305) — 9.2% failures
- [Cloudflare Workers limits](https://developers.cloudflare.com/workers/platform/limits/) — `cpu_ms`
- [Cloudflare pricing blog](https://blog.cloudflare.com/workers-pricing-scale-to-zero) — “purely a function of the logic”
- [F\* rlimits](https://github.com/FStarLang/FStar/wiki/rlimits:-Machine-Independent-Resource-Limits-for-Deterministic-Execution) — mythical powerful laptop
- [Wasmtime Store::set_fuel](https://docs.wasmtime.dev/api/wasmtime/struct.Store.html) / [Interrupting Execution](https://docs.wasmtime.dev/examples-interrupting-wasm.html) — fuel and epoch axes
- [gem5 core.cc](https://raw.githubusercontent.com/gem5/gem5/stable/src/sim/core.cc) — frozen `1 Tick == 1 ps`
- [Gutierrez et al., ISPASS 2014](https://tnm.engin.umich.edu/wp-content/uploads/sites/353/2017/12/2014.03.Sources-of-error-in-full-system-simulation.pdf) — gem5's 13–17% error
- [Ada RM D.14](https://ada-lang.io/docs/arm/AA-D/AA-D.14/) — `type CPU_Time is private`
- [POSIX clock()](https://pubs.opengroup.org/onlinepubs/9699919799/functions/clock.html) — frozen `CLOCKS_PER_SEC`
- [PostgreSQL planner cost constants](https://www.postgresql.org/docs/current/runtime-config-query.html) — “arbitrary scale, only relative values matter”
- [Solana block_cost_limits.rs](https://github.com/solana-labs/solana/blob/master/cost-model/src/block_cost_limits.rs) — 30 CU/µs
- [Linux commit c04c0d2](https://github.com/torvalds/linux/commit/c04c0d2b968ac45d6ef020316808ef6c82325a82) — eBPF “1M instructions in 1/10 of a second”

**Language Duration designs**

- [Go time](https://pkg.go.dev/time), [golang/go#20757](https://github.com/golang/go/issues/20757), [#64420](https://github.com/golang/go/issues/64420), [staticcheck SA1004](https://github.com/dominikh/go-tools), [hudl/fargo#56](https://github.com/hudl/fargo/issues/56)
- [Kotlin 1.6 release notes](https://kotlinlang.org/docs/whatsnew16.html) — restoration of extension properties
- [JetBrains/kotlin#4119](https://github.com/JetBrains/kotlin/pull/4119) — deprecation in 1.5
- [Rust pre-RFC: custom suffixes](https://internals.rust-lang.org/t/pre-rfc-custom-suffixes-for-integer-and-float-literals/8029)
- [Google C++ Style Guide](https://google.github.io/styleguide/cppguide.html) — UDL prohibition and reasons
- [CSS Values 4 §time](https://www.w3.org/TR/css-values-4/#time) — closed built-in suffixes
- [SE-0329](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0329-clock-instant-duration.md), [Zig lib/std/Io.zig](https://github.com/ziglang/zig/blob/master/lib/std/Io.zig)

**Internal**

- [race belt](https://github.com/almide/almide/tree/develop/crates/almide-race-belt) — 7 unit-independent theorems
- [logical-time-race spike](https://github.com/almide/almide/tree/develop/research/spike/logical-time-race) — 74,898-configuration confluence gate


### Reader gloss for preserved Japanese diagram and code comments

The first diagram runs from program start to finish: computation 30ms, network wait 500ms, computation 20ms, disk wait 200ms, computation 10ms. Wall time totals 760ms including waits; CPU time totals 60ms of actual code execution. The quoted shell annotations label wall/CPU as 2.17s/0.04s and 0.51s/0.39s respectively; these are the source figures, not recalculated here. In the decision examples, the comments identify a deterministic clock, the budgetless default race form, an optional divergence guard, and wall clock in the oracle layer. The constructor comments distinguish deterministic Compute from wall-clock Duration and show that variables work unchanged. The preserved constructor argument 大値 means a large value.
