Original description metadata: Proof ledger for the logical-time async semantics: theorems, Lean core, model gate
<a id="logical-time-async--the-proof-ledger"></a>

# Logical-Time Async — the proof ledger

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](logical-time-proofs.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](logical-time-proofs.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](logical-time-proofs.zh-CN.md)

> Non-normative reader snapshot of the pinned source; original remains authoritative. No automatic synchronization, independent implementation validation or execution of quoted instructions. Historical status, plans and comparative claims retain their source scope.
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/logical-time-proofs.md)

> Source caveat: the nine ledger entries include paper/model arguments; only the stated seven selection-algebra core theorems are reported as Lean kernel-checked. The 74,898 configurations are the union of the listed finite sets, not their larger Cartesian product or all programs. W1/W2 and the source’s charging model are prerequisites; semantic consistency is not renderer conformance. The historical counts and proof-gate reports were not rerun. T9 classifies wall-clock selection into the oracle layer; it neither permanently bans timeout syntax nor proves the runtime. Duration-min remains a design-only item in this snapshot, distinct from the implemented timeout body.


> Charter: [async-inception.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/async-inception.md). This document is its proof ledger.
>
> A proof ledger showing that the semantics in [logical-time-async.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/logical-time-async.md) constitute an asynchronous facility. Evidence has 3 layers:
>
> 1. **Paper theorems** (this document) — claims and proofs over a formal model
> 2. **Lean kernel-check** — 7 core selection-algebra theorems in [crates/almide-race-belt/](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-race-belt) (0 sorry, CI `lean-proofs`)
> 3. **Exhaustive model checking** — 74,898 configurations × all schedules, [research/spike/logical-time-race/](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/spike/logical-time-race) (`run-gate.sh`, 0 divergences)
>
> This document also records **3 corrections** that proof work forced on the design — evidence that proof was not a ritual.

<a id="何を証明し何を証明しないか"></a>

## What is and is not proved

- **Proved**: properties of the semantics itself — uniqueness of selection, schedule independence (confluence), unobservability of pruning and delayed fuel checks, deterministic consumption, finite observability of the bounded fragment, and the necessity of the boundary “syntax selecting the winner by environmental time cannot exist in the deterministic layer.”
- **Not proved**: that the implementation (both renderers) conforms to the semantics. That belongs to the charge-trace validator + `spec/wasm_cross` fixtures and is placed on the **trusted side**, following [proven-vs-trusted](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/proven-vs-trusted.md). These theorems fix “the semantics is consistent”; implementation gates seek to falsify “the implementation reflects that semantics.” Their roles do not overlap.
- **Explicit assumptions** (well-formedness, acceptance conditions when implementing CM-1):
  - **W1**: every CFG cycle and recursive call passes through ≥1 charge site.
  - **W2**: work between adjacent charge sites is finite (with basic-block-entry charging, bounded by block length).

<a id="形式モデル"></a>

## Formal model

- **Branch** = a deterministic charge trace: cost sequence `c₁, c₂, …` and termination (`Complete` | `Trap` | divergence). In the pure fragment (Rung 0), a function of (program, input).
- **Branch end under budget n** `end_j` (check-then-charge: Exhaust when remaining < c): `Complete(t)` | `Trap(t)` | `Exhaust(t)`, where `t` = cumulative consumption.
- **Event** = `(time, idx)` (cumulative consumption, branch index). **Merge order** `≺` is lexicographic on (time, idx) — a scheduler-free statement of lockstep: “1 fuel per tick, source order for ties.”
- **Decisive event** = the minimum Complete / Trap terminal event in merge order. Complete means a winner, Trap means a program trap, and absence means `Err(exhausted)`.
- **Occurred stream** = charge events preceding the decisive event under ≺ (+ the decisive branch's own charges). The outer region observes this sequence as consumption in merge order.

The Rust model (spike) literally implements this definition; Lean directly formalizes the algebra of events and `≺`.

<a id="定理台帳"></a>

## Theorem ledger

<a id="t1--枝の決定性と全域性紙"></a>

### T1 — Branch determinism and totality (paper)

**Claim**: under W1 and W2, branch execution with budget n necessarily reaches `Complete(t) | Trap(t) | Exhaust(t)` (t ≤ n) in at most n charges and finite work; the result is a function of (trace, n).

**Proof**: small-step evaluation in the pure fragment is deterministic. By W1, an infinite execution passes charge sites infinitely often, but an n+1st site passage is impossible (the budget is exhausted). By W2, work between sites is finite. ∎

This is the totality of `fan.bounded`, closing the condition in deterministic-bounds.md's conditional statement “the 2-safety framework applies in the bounded fragment” (T8).

<a id="t2--lockstep--least-spend紙--lean-decisive_subset"></a>

### T2 — lockstep ≡ least-spend (paper + Lean `decisive_subset`)

**Claim**: when the decisive event is Complete, its branch is lexicographically minimal in (spend, index) among completing branches.

**Proof**: a completion event's time equals that branch's spend. Merge order is lexicographic on (time, idx), so if the minimum of all candidates is Complete, it is also minimal in the Complete subset (Lean: apply `decisive_subset` to the completion subset). ∎

<a id="t3--選択の一意性lean-decisive_unique"></a>

### T3 — Uniqueness of selection (Lean `decisive_unique`)

**Claim**: when candidate branch indices are distinct (a branch terminates at most once), the decisive event is unique.

`¬prec` in both directions implies equality of time and idx (omega). Algebraically, there cannot be 2 answers to “who won?” ∎ (kernel-checked)

<a id="t4--cap-の許容性lean-cap_admits_decisive--cap_admits_window"></a>

### T4 — Admissibility of caps (Lean `cap_admits_decisive` / `cap_admits_window`)

**Claim**: a pruning cap (given recorded candidate d', branch k may be cut at `d'.time − (k > d'.idx ? 1 : 0)`) **computed from any candidate** (a provisional best from sequential scanning or a candidate a parallel schedule happened to record first) (a) allows the true decisive branch to run up to its time and (b) preserves every event preceding the decisive event under ≺ — including the trap-visibility window.

∎ (kernel-checked). Corollary: **every schedule run to quiescence records the true decisive event** (the decisive branch cannot park because of a cap, nor Exhaust before termination).

<a id="t5--合流lean-decide_stable--モデル-adv"></a>

### T5 — Confluence (Lean `decide_stable` + model ADV)

**Claim**: an implementation whose records (i) are a subset of the true candidate set, (ii) include the true decisive event, and (iii) answer with the recorded minimum in merge order **always** answers with the true decisive event. Sequential scanning, parallel execution, pruning exactly at the cap, and delayed fuel checks crossing the cap (overrun) all reduce to (i)–(iii).

Lean proves (i)–(iii) ⇒ agreement (`decide_stable`); the model attempted to falsify “the implementation satisfies (i)–(iii)” by enumerating 74,898 configurations × all schedules and found no counterexample. Not just the winner but **consumed fuel and the occurred stream** agree (also checking T6).

<a id="t6--消費量の決定性と再構成可能性紙--モデル"></a>

### T6 — Determinism and reconstructability of consumption (paper + model)

**Claim**: the occurred stream (and therefore the consumption a race imposes on the outside) is a function of the traces, and can be reconstructed exactly using only data revealed by capped execution.

**Proof**: the stream definition is an ≺ filter against the decisive event (unique by T3), hence a function of the traces. Reconstruction: capped execution reveals every event with time ≤ cap; by T4(b), the cap covers every event in the window. Extra revealed events (overrun) are discarded by the ≺ filter. ∎ The model compares agreement as sequences of stream events.

<a id="t7--入れ子の決定性紙--モデル"></a>

### T7 — Determinism of nesting (paper + model)

**Claim**: the outer region consumes the occurred stream in merge order, and if its remaining budget runs out midway, the outer region is Exhausted at that point (the race is abandoned). This decision is a function of (traces, outer remaining budget), and the whole is deterministic by induction on region nesting depth.

**Proof**: the stream is deterministic by T6; comparison with the prefix sum is arithmetic. If the decisive event is Trap, the program traps and consumption is not observed. ∎

<a id="t8--有界断片の有限観測性紙"></a>

### T8 — Finite observability of the bounded fragment (paper)

By T1, every execution of the bounded fragment reaches an observation in finite time. Native ⇄ wasm observational equivalence is therefore falsifiable by finite trace pairs on this fragment — this establishes the inside of the fragment for the claim deterministic-bounds.md limited with “do not call it 2-safety outside the fragment.”

<a id="t9--決定層の不可能性定理紙"></a>

### T9 — Impossibility theorem for the deterministic layer (paper)

**Claim**: syntax that “selects the winner using the environment's wall clock” cannot exist in the deterministic layer.

**Proof**: the deterministic layer is defined by “observations are a function of (program, input).” Wall-clock order is a function of host speed, not of (program, input). As long as the winner is observable (branches return distinguishable values), wall-clock selection contradicts the definition. ∎

Corollary: I/O races and timeouts **necessarily** belong to the oracle layer (the contract class treating the environment as declared input ω and relating it by R_Ω). This is a boundary theorem, not a design preference. Initially deferring `fan.timeout` to Stage 4 was an implementation-order decision; this corollary does not prove a permanent ban on timeout syntax. For the current body form and clock-based distinction of layers, see [SPEC §13](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/SPEC.md#13-concurrency-fan) and [ADR-0028](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0028-fan-selection-names-and-cost-contract.md).

<a id="証明作業が設計に強いた訂正3-件--モデル自身の-1-件"></a>

## Corrections proof work forced on the design (3 + 1 in the model itself)

1. **The no-winner trap rule was undefined.** The first draft only described the visible window as “before the winner-determined tick,” without defining the no-winner case. Unifying it under the decisive-event rule (the minimum Complete / Trap in merge order is the sole arbiter) eliminated this.
2. **The consumption formula Σ min(end_j, s\*) overcounted at the boundary.** For a branch with an index after the winner's, a charge at time = s\* follows the winner's completion in merge order and does not occur. The correct quantity is the sum of the occurred stream (merge-prefix).
3. **Nesting is streaming, not “atomic charging at the race site.”** The outside observes the race's internal charges in merge order, abandoning the race if exhausted midway. Atomic charging cannot define the decision point for “the outer budget runs out inside the race.”
4. One bug in the model itself: the reference-side occurred stream omitted the cost-1 tail of a diverging branch; this was immediately detected and fixed as a consumed discrepancy with SEQ (also demonstrating that divergence actually reaches the detection surface).

Items 1–3 have been reflected in logical-time-async.md with correction markers.

<a id="非同期機能としての十全性adequacy-audit"></a>

## Adequacy as an asynchronous facility (adequacy audit)

| Asynchronous pattern | Mapping to current surface | Evidence / scope |
|---|---|---|
| Concurrent fan-out / join (`Promise.all`) | `fan { }` / `fan.map` (execution may be parallel; observations are in list order) | C-004 / C-199 / C-200 |
| Collect all (`allSettled`) | `fan.settle` | Existing |
| Fallback chain | `??` / `list.find_map` (old `fan.any` removed) | ADR-0028 |
| Speculative / portfolio computation | `fan.min(compute.ms(n))` (clock required) | Selection model T2–T5; implementation conformance separate |
| Computation bound / totalization | `fan.bounded(compute.ms(n))` | Model scope of T1 / T8 |
| I/O race (first response) | `fan.min(duration.ms(n))` is only a design decision (#3788, unimplemented) | T9 boundary; details in ADR-0028 / SPEC |
| Timeout | `fan.timeout(duration.ms(n)) { body }` | Oracle layer, body form; T9 is not an implementation proof |
| streaming / backpressure | Domain of `Flow[T]` (flow-design.md) | Explicitly out of scope |
| fire-and-forget | Permanently rejected (rush / spawn — fan-v2 rejection table) | stance |

Honest summary: core asynchronous uses (concurrent I/O fan-out, collecting all, fallback) can be organized in the deterministic layer. Selective I/O races and timeouts are oracle-layer contracts; timeout bodies are implemented, while Duration min is unimplemented (#3788). Distinguish the layer decision from implementation status. T9 discusses this distinction of layers, not a proof of every current runtime feature.

<a id="小スコープの但し書き"></a>

## Small-scope caveat

The model-checking execution scope is the union of two sets recorded in [GATE.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/spike/logical-time-race/GATE.md):

- branches ≤ 3, charges per branch ≤ 2 (costs 1..2), budget 0..=5
- branches ≤ 2, charges ≤ 3, budget 0..=7
- nested outer bounded budget 0..=9 × additional charge {0, 1}

It did not check the full Cartesian product branches ≤ 3 / charges ≤ 3 / budget ≤ 7. Lean's size-independent theorems cover selection algebra (the theory of events and ≺); the connection “trace → candidate set” (distinct candidate idx, records ⊆ truth, recording the decisive event) was checked by the model for each configuration. Rerunning with expanded scope is possible with a single `run-gate.sh` invocation.

<a id="反証条件"></a>

## Falsification conditions

- CM-1 implementation requires a placement that cannot satisfy W1/W2 (e.g. an optimization requiring a charge-free cycle) → T1 fails; withdraw the totality claim.
- Renderer charge-trace preservation breaks → the theorem set remains intact while the validator / fixtures detect that **the implementation stepped outside the semantics** (the gate's job).
- Out-of-scope counterexample → raise scope constants in `run-gate.sh` to reproduce and add it, correcting the corresponding theorem in this ledger. Keep corrections visible in the style of the section above.
