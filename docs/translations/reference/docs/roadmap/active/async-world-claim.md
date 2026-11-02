Description: Audit of the best-async-of-2026 claim: the axis, the rivals, the five runaway moves.
<a id="the-2026-async-claim--audit-and-the-runaway-plan"></a>

# The 2026 Async Claim — audit and the runaway plan

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](async-world-claim.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](async-world-claim.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](async-world-claim.zh-CN.md)

> Non-normative reader snapshot pinned to the original commit; the original remains authoritative. No automatic synchronization or independent execution, implementation verification, proof checking or measurement. Historical commands and design instructions are documentation, not executed instructions.
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/async-world-claim.md)

> Source caveat: preserve the historical advocacy, competitive hypotheses and superseded plan alongside the later explicit limits; they are not independently verified 2026 rankings, product comparisons or uniqueness evidence. The opening status, later verdict and old future-tense lanes differ: only timeout-event replay is implemented here, not the proposed host-response tape or general record/replay; Duration-clock min is unimplemented. Old fan.race/fuel and timeout(ms:) sketches are retained rather than converted into new syntax. Lean selection-algebra theorems, finite model enumeration, selected cross-target fixtures and 7/7 retry-inclusive dojo outcomes are distinct evidence, not whole-runtime conformance, arbitrary-program determinism or universal bug reproduction. The body’s stronger exclusivity, immutable-structure, cheap-wiring and CI-existence-proof rhetoric is the original's historical argument, not a new guarantee. The old arm-buffering premise conflicts with the separately recorded withdrawal of transactional output buffering; retain it as history, not current I/O isolation. Tape confidentiality and redaction preserve only the stated design intent; unchanged types alone do not prove replay equivalence after redaction. No competitor, compiler, proof, benchmark, command or network experiment was run.

> Charter: [async-inception.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/async-inception.md). This document audits its claims.
>
> An audit of **whether we can say** “the world's best async syntax of 2026,” and five moves to **pull far ahead**. Design: [logical-time-async.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/logical-time-async.md) / [fan-v2.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/fan-v2.md); proofs: [logical-time-proofs.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/logical-time-proofs.md). This document handles the claims—what sentence withstands scrutiny, and what sentence would still be false.

> **Implementation status (2026-08-03, branch `worktree-stage1-charge-probe` — BURNDOWN/contract ledger is authoritative in discrepancies)**: the execution-order section below retains its drafting-time future tense. Current position:
>
> - **Lane 1 complete**: Wave 1 (b817e3dc) → Stage 1 (8ece4153 + 4db109aa) → Stage 2 (4152da7b) → Stage 3 (113feee2). CM-1 v0.3 = 3ns/unit, exact flips at ns-precision boundaries, contracts C-202..C-207 + permanent 3-way corpus—**conditions for enabling claims 1–3 are met**.
> - **Lane 2 B2 complete**: `fan.timeout(duration.ms(n)) { body }` (f8d760ba, C-208 + ALS-D5).
> - **Lane 2 B1 is only the timeout-ω slice**: ω = ordinal of the wall check at which the deadline expired. Capture with `ALMIDE_OMEGA_RECORD=1` → replay baked at compile time with `ALMIDE_OMEGA=<n>`; a permanent gate checks byte equality of record (native) → replay (native + wasm). **The effect-response tape (ω schemas for fs/http/random/process/env + `--record` / `--replay` + redaction) is not started**—claim 4 can be made today if scoped to “deterministic reproduction of timeout events.” Full wording is enabled after B1 completion (the sketch below is the specification).
> - **Lane 3 D is complete on the Almide side**: dojo async bank, 7 tasks **7/7 pass** (cli:claude, retries included—dojo runs/2026-08-02/summary.md). **Enabling claim 5 in full still requires same-model lang-bench(async) comparisons**.
> - **Addendum**: race mapper form is also implemented (T7-1)—every fan v2 matrix cell is settled.
> - **C (KPN)**: not started, as designed (still on the horizon).

<a id="判定先に結論"></a>

## Verdict (conclusion first)

**This ledger cannot establish unconditional “world's best” or worldwide uniqueness. Completing all five moves also does not justify omitting comparison subjects, versions or measurement scope.**

- Can say: there are Lean theorems for the selection algebra and cross-target checks for specified fixtures. The former is not a proof of the whole implementation; the latter is not a proof for all programs.
- Current implementation: Compute-clock `fan.min`, `fan.bounded`, and Duration body-form `fan.timeout` exist. Duration-clock `fan.min` is decided but unimplemented (#3788).
- Timeout-ω replay: `tests/fan_timeout_replay_test.rs` (#3781) checks native/Wasm output equality and differences between cuts at multiple cuts. This supports limited replay fixtures, not proof of recording/replaying arbitrary fs/http/random/process/env responses.
- Cannot yet say: worldwide ranking for overall async superiority or maturity. Without measurements fixing comparison subjects and conditions, do not substitute design or finite fixtures for rankings or all-use guarantees.

<a id="戦場の名指しと勝算"></a>

## Naming the battlefield and the odds

The five sentences are a list of weapons, not a battlefield. Name one battlefield:

> **Concurrency in an era when LLMs write most code.** A place where it is valuable that agent-written concurrent code returns the same result every time and can always be reproduced when it breaks.

We **do not fight** in the world of human-operated fault-tolerant servers (Erlang/Go's battlefield)—the Erlang row in the losses table is a conceded loss. Roles of the five sentences: 1–3 = verifiability weapons; 4 = reproducibility weapon; 5 = scoreboard.

| claim | Odds | Basis | Remaining risk |
|---|---|---|---|
| 1 Machine proof | Already won | 7 theorems, 0 sorry | Alone it will not move the market (assurance of trust, not demand) |
| 2 Observational equivalence | High | Existing infrastructure of 199+ contracts and gates | Stage 1 charge-trace preservation (narrowed to one point) |
| 3 Compute-cost race | Win by default | No language building this has been found | Discovering a predecessor (falsification condition). An academic flag, with little everyday demand |
| 4 record/replay | **Biggest winning route** | Temporal's commercial success demonstrates demand. The moat is structural (nondeterminism flows through one registered surface) | Implementation scope (ω schema, redaction) |
| 5 LLM measurements | Favorable but real risk | Prior lang-bench data (2026-07-15, Almide 100% vs Gleam 19/20) | LLMs have extensive Go/TS async training. Measurements decide whether structurally eliminating error classes can outperform it |

The true rival on this battlefield is not the five names in the table but **“a runtime is enough”** (widespread Temporal-style replay). The counterargument rests on structure: Temporal requires determinism by convention, and violations surface at runtime. **LLMs break conventions. They cannot break structure**—the same logic as MSR's first line. The battlefield's market value is rising: AI coding is a growing market, and attention to deterministic simulation (Antithesis / FoundationDB style) is a tailwind. In short, the odds lie in being first with a structural moat on land that is small now but will appreciate.

<a id="当初検討した差別化の軸世界唯一性の証拠ではない"></a>

## Initially considered differentiation axes (not evidence of worldwide uniqueness)

<a id="軸-1-命令粒度の論理時計"></a>

### Axis 1: Instruction-granularity logical clock

| System | Logical-clock granularity | Can pure computations race each other? |
|---|---|---|
| Verse | event / simulation tick | **Presumably not**—expressions without suspension finish within the same tick, so a pure-computation race always collapses to the first in source order (inference from docs, not checked on a running system) |
| Lingua Franca | timestamp (event tag) | No—time comes from events; computation cost is not on the clock |
| Esterel family | synchronous tick | No (same reason) |
| EVM | **Instruction granularity (gas)** | Has no concurrency syntax to begin with |
| **Almide (design)** | **Instruction granularity (fuel)** | **Yes—the very reason fan.race(fuel:) exists** |

“Running Verse's race on EVM's gas” creates resolution neither has alone. **None of the systems in this table has a race whose winner is determined by computation cost.**

<a id="軸-2-クロスターゲット観測等価の内側での-async"></a>

### Axis 2: Async inside cross-target observational equivalence

Go/Rust/Kotlin/Swift async has single-target semantics, with determinism approximated during testing by external tools (loom, race detector, rr). Almide puts async **inside** the native ⇄ wasm observational-equivalence contract (199+ contracts, with ratchets), and is designed to align even interruption points (charge sites) across both targets. What Swift 6 strict concurrency eliminates is data races (at the type level), not nondeterminism.

<a id="軸-3-実装に先行する機械証明--全数合流ゲート"></a>

### Axis 3: Machine proof **before** implementation + exhaustive confluence gate

- 7 Lean kernel-checked theorems, 0 sorry (uniqueness of selection, preservation of the cap-visible window, confluence)
- Confluence checks over 74,898 configurations × all physical schedules (including overruns from delayed fuel checking)
- A record of proof work finding and fixing 3 design bugs before implementation

loom (Rust), Coyote (.NET), and P (used in production at AWS) **explore schedules at test time**. No prominent example has been found of proving confluence as language semantics and fixing it before shipment. Verse has a core-calculus formalization (Verse calculus, ICFP 2023), but publication of machine proofs for its concurrency/transactional layer has not been confirmed.

<a id="軸-4-最高を外部指標で測る文化"></a>

### Axis 4: A culture of measuring “best” with external metrics

MSR (modification survival rate) and lang-bench measure using the same model and tasks, turning “LLMs can write it most accurately” from a claim into a measurement. The async task bank is move D below.

<a id="負けている軸--正面から認める表"></a>

## Axes where we lose—an explicit admission table

| Rival | Where it wins | Almide's current position |
|---|---|---|
| Erlang/OTP | Fault tolerance (supervision, let-it-crash), 30 years in production | No structural recovery. A trap merely dies deterministically |
| Go | Production async maturity, goroutine usability | Oracle layer unimplemented; cannot write a “real concurrent I/O race” |
| Verse | Transactional effects shipped, in production in Fortnite | Effect isolation only a Rung 0 (pure) design |
| Lingua Franca | Distributed (federated) determinism, accumulated academic work | Distribution out of scope |
| Temporal and other durable execution | Persistent execution through replay exists commercially (a runtime, not a language) | **This is what move B of the five targets** |

Hide this table and call it “best,” and the audit (this repo's academic-rigor norms) fails.

The Lingua Franca row contains a question reviewers will certainly ask: “Wasn't LF first with logical time?” The one-line answer: **LF's clock orders events; Almide's clock ticks inside computation**. LF coordinates C / Python / TS / Rust and does not own reaction bodies (target-language code), so it cannot meter computation portably—metering requires ownership of codegen (the same structure as the record/replay moat). Conversely, LF rightly leads in deterministic distributed coordination. If Almide someday ventures beyond the horizon (KPN channels → distribution), LF there is a source to borrow from, not a rival. Record this as a “horizon loss”—a fourth kind, distinct from Erlang which is not pursued, Go/Verse which are matters of time, and Temporal which is to be taken back.

<a id="ぶっちぎりの五手"></a>

## Five moves to pull far ahead

<a id="a-stage-13-を実装して出荷する"></a>

### A. Implement and ship Stages 1–3

Paper does not compete. Only when fuel, bounded and race run under gates on both backends do axes 1–3 change from “design discussion” into “properties of the language.” **The only prerequisite among the five moves.**

<a id="b-oracle-層をrecordreplayとして設計する--決定打"></a>

### B. Design the oracle layer as “record/replay”—the decisive move

Design Stage 4 not as “bringing timeout back,” but as **language-built-in deterministic record/replay**.

**Claim form**: the deterministic layer established `observe = f(program, input)`. The oracle layer is `observe = f(program, input, ω)`—environment response sequence ω becomes an **explicit input**. Then:

    almide run app.almd --record trace.ω     # ω を採録
    almide run app.almd --replay trace.ω     # 同一実行を再現 — 別ターゲットでも

Original command-comment translations: capture ω; reproduce the same execution—even on a different target.

Replay correctness needs no new theorem—it is function reapplication (a corollary of T9). **Flaky async bugs become reproducible artifacts.**

**Why only Almide gets this cheaply (the moat's structure)**: rivals scatter nondeterminism throughout the language (Go goroutine scheduling itself is nondeterministic, so rr must record syscalls and scheduling. Temporal requires workflow-code determinism **by convention**, discovering violations at runtime). Almide's deterministic layer already fixes everything; nondeterminism passes only through the oracle effects' **narrow registered surface** (fs/http/process/random/env, via the self-host registry). Structurally there is only one place to attach the tape. **B is wiring work for Almide and an architecture change for rivals**—that is the substance of “pulling far ahead.”

**Design sketch** (to become the formal design in the Stage 4 design document):

- Define ω schemas per effect module (http: status/headers/body; fs: contents/errors; random: value sequence; datetime.now: value sequence; process: exit/stdout/stderr). Record **response values, not times**—time enters semantics only in timeout arbitration, recorded as the **discrete event** “at which charge site it expired.”
- Consumption points are charge sites (unified interruption-point principle), so the deterministic layer already fixes ω consumption order. Therefore replay is target-independent—**record native, replay wasm** follows by definition, itself becoming the executable form of the R_Ω contract (executable oracle contract).
- `fan.timeout(ms:)` becomes syntax reading “at which site a timeout event appeared in ω.” The real clock generates the event when recording; the tape plays it back during replay.
- Concurrent I/O in fan{} composes with join-all + arm output buffering (#1026 series)—ω is a response sequence per arm; interleaving is unobservable, so need not be recorded.
- Contract: make “replay with recorded ω reproduces observations byte-for-byte” a C-NNN + fixture. This amounts to keeping an existence proof of R_Ω permanently in CI.
- State the boundary too: ω includes network responses and may be confidential. Design redaction as a schema-side responsibility (preserve value types while hiding contents).

<a id="c-決定的チャネルの地平を名指しでロードマップに載せる"></a>

### C. Explicitly put the deterministic-channel horizon on the roadmap

KPN (Kahn process network) channels over the logical clock + tick-ordered select provide a concrete path satisfying #1000's revisit condition, “contracts before structured concurrency.” Do not pursue Erlang fault tolerance itself (a different product), but this changes the missing “channels” axis from **a design blank into a matter of timing**.

<a id="d-async-版の実測を-dojo-に積む"></a>

### D. Add async measurements to dojo

Measure MSR with a fan v2 task bank (fan-out, fallback, speculation, making computations total with bounded), and publish same-model/same-task lang-bench comparisons with Verse / Go / Rust / TS. Same approach as the lang-bench precedent (2026-07-15, Almide 100% vs Gleam 19/20). **Say “the best async for LLMs” as a measured value.**

<a id="e-claim-の文言を-evidence-つきで固定する"></a>

### E. Fix claim wording together with evidence

Limit external statements to the following evidence scopes. Do not conflate the original 7 theorems, model enumeration, fixtures and Dojo experiments:

1. “The logical-time **selection algebra** has Lean-checked theorems”—not a conformance proof for the entire execution system or all concurrent syntax.
2. “Contract gates check native / wasm observational differences on the target fixtures”—state the finite measurement scope, distinguishing it from observational equivalence for arbitrary programs.
3. “`fan.min` can select on logical cost with the Compute clock”—clock is required; the cost contract is scoped to the meter version. Do not claim worldwide uniqueness.
4. “There are replay checks for the recorded oracle events”—enumerate recording format, target hosts and fixtures; do not guarantee reproduction of all async bugs.
5. “Measure async edit success rates for specified models, tasks and versions”—report only with Dojo comparison conditions and results; do not generalize to most accurate across all languages.
Accompany each claim with its subjects and limits. Even combined, these do not guarantee unconditional world superiority, correctness of arbitrary implementations or reproduction of every bug.

<a id="実行順--2-レーン並列--地平-1-本"></a>

## Execution order—2 parallel lanes + 1 horizon

Resolving dependencies does not give a serial A→B→C. Most of B (B1) is fuel-independent and can run alongside.

**Lane 1 (main route, serial—main worktree)**

1. **fan v2 Wave 1**—block/mapper forms of any/settle + thunk tombstone + migration of 8 spec files + matrix gate. Needs no fuel; can start immediately. Advance work to receive race in v2 form from the outset.
2. **Stage 1: CM-1 + charge-trace preservation**—a whole-program falsifier, so immediately after Wave 1. **Do not build region specialization here**: make a fully instrumented build through a hidden flag (e.g. `--fuel-probe`); first establish three-way fixture comparisons (result / consumed / trace) and the validator. Defer specialization until Stage 2 brings the surface.
3. **Stage 2: fan.bounded**—v2 block form + region specialization + finalize and register CM-1 constants in the ledger (same PR).
4. **Stage 3: fan.race**—pruning cap, delayed trap arbitration, nested streaming, E027 revision. Wire race belt / spike gates as CI acceptance conditions. → **claims 1–3 enabled**.

**Lane 2 (can run in parallel—separate worktree)**

- **B1: record/replay infrastructure**—fuel-independent. The current language is deterministic except for oracle effects, so recording/replaying ω already has meaning on today's surface. Inventory effect surface (enumerate registered-surface oracle effects) → ω schemas → tape format → `--record` / `--replay` → replay-equivalence contract + fixture. → **claim 4 enabled** (even before B2, as “deterministic reproduction of effectful programs”).
- **B2: fan.timeout(ms:)**—read ω events at charge sites. Only this depends on Stage 1, so it joins Lane 2 after step 2 of Lane 1 completes.

**Lane 3 (dojo side)**

- **D: async task bank**—v0 can use the post-Wave-1 surface (fan{} / map / any / settle). Add race / bounded tasks after Stage 3. → **claim 5 enabled**.

**Horizon (do not start)**

- **C: KPN channels**—write only a design document after Stage 3 completes. Implement after meeting #1000's revisit condition (contracts per syntax first).

<a id="反証条件"></a>

## Falsification conditions

- Counterexample to axis 1: find a language or implementation with instruction-granularity deterministic race (including research languages). → Correct the table and withdraw claim 3. Search alongside D's comparisons.
- Inference about Verse pure-computation race is wrong (actual implementation has resolution). → Correct the Verse row in axis 1. The table already labels it an inference.
- B's redaction is impractical (ω is inherently confidential and cannot be shared). → Downgrade replay to a local/CI debugging feature and narrow claim 4.
- D's measurements show Almide async MSR does not beat rivals. → If syntax is at fault, invoke fan v2's falsification conditions (redesign the block/mapper two-way choice); if diagnostics, move to the diagnostics-improvement lane. Measurement wins.
