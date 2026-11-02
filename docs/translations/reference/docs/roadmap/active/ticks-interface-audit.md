Source description metadata: Audit of ticks against timeout-shaped APIs: the unwritable-number problem, three fixes.

<a id="ticks-interface-audit--is-the-demanded-api-shape-different"></a>

# Ticks interface audit — is the demanded API shape different?

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](ticks-interface-audit.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](ticks-interface-audit.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](ticks-interface-audit.zh-CN.md)

> Non-normative reader snapshot pinned to the original commit; the original remains authoritative. No automatic synchronization or independent execution, implementation verification, proof checking or measurement. Historical commands and design instructions are documentation, not executed instructions.
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/ticks-interface-audit.md)

> Source caveat: the opening supersession notice controls how to read the historical body. Optional race budgets and the rejection of time-like tick units below are preserved, not reinstated as current rules. The source explicitly corrects the latter rejection: deterministic conversion does not break determinism. Proposed flags, Stage 4 interfaces, priority changes and proof-scope claims are not independently validated here. Code examples keep their historical spelling and original Japanese comments.

> **Naming was superseded by [ADR-0001](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0001-deterministic-time-units.md).**
> This audit's “unwritable number” problem and the discussion of tool-based estimation remain, but fix 1's optional race budget is not the current contract. [ADR-0028](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0028-fan-selection-names-and-cost-contract.md) renamed `fan.race` to `fan.min` and made the clock argument mandatory. However, the section rejecting “conversion of time-like units to ticks” was **wrong** — it confused severity (determinism is not broken at all), and ADR-0001 corrected that rejection and adopted time units. What follows is preserved as the record of that time.
>
> Charter: [async-inception.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/async-inception.md). Following the finalization of `ticks:` (be4e7d4c), this records a close examination of the mismatch with other languages' timeout interfaces and whether “the API shape being demanded is actually different.” It concluded with three design fixes, already reflected in the respective documents.

<a id="他言語の形と違和感の正体"></a>

## Other languages' shapes and the real source of discomfort

| System | Shape | Unit | Supplier of the number |
|---|---|---|---|
| Go | `context.WithTimeout(ctx, 5*time.Second)` | time | Human intuition. The deadline **propagates** through context |
| JS | `AbortSignal.timeout(5000)` | time | Human intuition |
| Rust/tokio | `timeout(Duration::from_secs(5), fut)` | time | Human intuition |
| Python | `asyncio.wait_for(coro, timeout=5.0)` | time | Human intuition |
| Erlang | `receive after 5000` | time | Human intuition |
| Verse | `Sleep(5.0)` in a race branch | time | Human intuition (timeout = one race branch) |
| EVM | gas limit | computational work | **Tools** (eth_estimateGas) + margin |
| Wasmtime | fuel | computational work | Empirically calibrated by the **host operator** |

The first 6 rows share “the unit is time” and “intuition works for the number.” Only the bottom 2 rows use computational work, and neither **makes people write numbers by intuition** — tools or operations calibrate them.

This is the source of the discomfort. `ticks:` is not another spelling of timeout but **a different quantity**; time budgets (SLA, responsiveness, cutting off external I/O) belong to the oracle layer's `ms:` (Stage 4) and the host boundary. This division is correct. **The problem is the number, not the unit**:

> Nobody can estimate `ticks: 100_000`. CM-1 is a versioned abstract cost table, for which neither humans nor LLMs have prior intuition. Moreover, the budget is coupled to the body's implementation — refactor `optimal_plan` and 100_000 goes stale. **Making a magic number that goes stale on edits a mandatory argument borders on self-contradiction for a language advocating modification survival.**

<a id="修正-1--race-の-ticks-を任意化する"></a>

## Fix 1 — make race's `ticks:` optional

The race selection rule (the successful result minimizing (spend, index)) **does not require** a budget. The budget's only role is guarding against divergence. A race without a budget is well-defined:

```almide
fan.race { exact(p); heuristic(p) }   // 予算なし — 発明すべき数字がない
```

- Its semantics specialize the existing definition to n = ∞. Run branch 0; its spend becomes the cap thereafter (there is no cap until the first completion appears).
- Termination: if a divergent branch precedes a completing branch (in list order), it hangs. This is **the same risk class** as the existing `fan {}` specification (“hanging if a sibling does not terminate is by design,” #1023), not a new hole. For branch arrangements needing a guard (runaway search, for example), add `ticks:`:

```almide
fan.race(compute.s(1)) { search_a(p); search_b(p) }   // ガード付き
```

- `fan.bounded`'s `ticks:` **remains mandatory** — bounding is why that syntax exists.
- Lean / model theorems take the budget as a parameter and still hold unchanged at n = ∞ (cases involving divergence simply do not reach a finite observation; the confluence claim is a claim about terminating cases).

Effect on MSR: invented constants disappear from the most frequent case (a portfolio of 2–3 branches). An LLM need only write `fan.race { a; b }`; the pressure toward the misuse of “writing time into ticks while intending a timeout” also disappears by not writing a number at all.

<a id="修正-2--数字の供給者はツールとホスト境界"></a>

## Fix 2 — tools and the host boundary supply the numbers

Where a number is needed, write it based on measurement rather than intuition.

1. **Calibration loop**: Stage 1's `--fuel-probe` directly becomes a user-facing measuring instrument — `--ticks-report` measures consumed ticks per region / program, then a margin is applied to obtain a budget. The same operational form as EVM's estimateGas. Recalibrate on a CM-1 revision (semver-major).
2. **Deterministic budget at the host boundary**: `almide run --ticks n` / `almide test --ticks n` — wrap the whole program in a bounded main. **The harness / CI** owns the number; the source contains no constant. Unlike a wall-clock CI timeout, it is **a non-flaky hang killer** (the same input is cut off at the same tick). This is rather the shortest route to a product for the arena of deterministically bounding agent-written code (the arena named by async-world-claim). Implementation is only “wrap main in bounded,” with zero new semantics.

<a id="修正-3--timeout-ではないを明文化する"></a>

## Fix 3 — explicitly state “this is not a timeout”

- Ticks are a computational-work budget, not a time budget. Readers who want time should use the oracle layer's `fan.timeout(ms:)` (Stage 4) or the host boundary (`timeout 5 ./app`). Include this guidance in fan-v2 diagnostics too (the same shape as the existing pure-constraint hint).
- Deadline propagation, which Go context carries **as a value**, is carried **by semantics** in Almide's nested min-cap — the inner region implicitly inherits the outer region's remainder (EIP-150 style). No user-visible cancellation API corresponding to a cancellation token / AbortSignal exists (cancellation is structural and an optimization; it does not appear in observations, so there is nothing to pass).

<a id="単位を時間にしないとだめではへの回答"></a>

## Answer to “don't the units have to be time?”

Since the first 6 rows of the comparison table all use time, this question is inevitable. The answer has two parts.

**The deterministic layer cannot use time — this is a theorem (T9), not a preference.** Wall-clock time is a function of host speed and contradicts “observation = a function of (program, input).” The moment it uses time, native ⇄ wasm observational equivalence, record/replay, and zero flakiness all collapse together — returning to exactly the reason `fan.timeout` was removed in 0.29.0. It is not that we refuse to compromise here; there is nothing to compromise on.

**Answer demand for time with time.** The comparison table shows that mainstream demand (SLA, responsiveness, cutting losses) is time-shaped, and the oracle layer's `fan.timeout(ms:)` accommodates it. As an outcome of this audit, **raise its priority**: B2 is not an “extra” in Lane 2 but a deliverable equal to B1, the official answer to time-shaped demand. Ticks are not a substitute for time but a separate product for those buying **what time cannot buy** (determinism, replay, sandboxing, flake-free CI) — the two-layer division is itself the design's answer to this question.

**An intermediate proposal considered and rejected — convert time-like units to ticks.** Pin a reference rate (e.g. 1ms ≈ N tick on a reference machine) in CM-1, let source use time-like notation such as `budget: 100.ms`, and convert the constant to ticks at compile time. Determinism can be preserved, but an API allowing `100.ms` without matching wall-clock time **replays the misreading accident of `fan.timeout(1000)` in reverse** (what looks like time is read as time — the lesson of 0.29.0). Rejected. However, connect intuition on the tool side: `--ticks-report` **also shows measured time on this machine** alongside ticks (`52,000 ticks (≈0.4ms here)`) — the same solution by which the gas ecosystem connects intuition using estimateGas and prevailing gas/sec rates.

<a id="反証条件"></a>

## Falsification conditions

- Budgetless race hangs are observed as an accident class in dojo / real use → make race's `ticks:` mandatory again and reverse this section of the document with a correction mark.
- `--ticks-report` measurements fluctuate so much with every CM-1 revision that calibration becomes impractical → strengthen the CM-1 stability contract (limits on frequency of constant changes) on the ledger side.
- No demand for host-boundary `--ticks` (nobody uses it) → remove it from the ladder. Conversely, if demand concentrates there, lower the priority of fan.bounded's in-language surface and reallocate effort.

Translation of preserved code comments: the first example says “No budget — no number to invent”; the second says “With a guard.”
