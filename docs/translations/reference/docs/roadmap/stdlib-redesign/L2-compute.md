<a id="l2l3-compute"></a>

# L2/L3: compute

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-compute.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-compute.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-compute.zh-CN.md)

> Non-normative reader translation of the [canonical source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-compute.md), pinned at 35297a612fb29e3f78f6587aba253aa91d51f466. This snapshot is not automatically synchronized. The original remains authoritative. Roadmap decisions, dates, comparisons, implementation and measurement claims are reproduced without independent execution or revalidation; they are not assertions of current release behavior. Source metadata is displayed as a reader note, not active generator input.

> Source description (inactive metadata): Stdlib redesign L2/L3 — compute: the six deterministic compute-time constructors stay as ADR-0001 fixed them; add compare; no counterpart exists in the nine languages

> Last updated: 2026-10-21
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md), [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md) (R-L1-21: do not merge with `duration`), [ADR-0001](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0001-deterministic-time-units.md), [ADR-0028](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0028-fan-selection-names-and-cost-contract.md)
> Related: [L2-duration](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-duration.md)
> Sources: Rust https://doc.rust-lang.org/std/time/struct.Duration.html ・ Go https://pkg.go.dev/time#Duration ・ Python https://docs.python.org/3/library/time.html#time.process_time ・ TS/JS https://tc39.es/proposal-temporal/docs/duration.html ・ Gleam https://hexdocs.pm/gleam_time/ ・ Elixir https://hexdocs.pm/elixir/Process.html#info/2 (reductions) ・ OCaml https://ocaml.org/manual/latest/api/Sys.html (`Sys.time` is CPU time) ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.time/-duration/ ・ Swift https://developer.apple.com/documentation/swift/clock

`Compute` is an amount of “deterministic compute time,” counted by charges in a frozen abstract machine (at function entry and loop heads). The budget verdict depends only on the program and its input and is the same on every target and host. This is the clock read by `fan.bounded` and selected by `fan.min(compute…)`. None of the nine languages has an equivalent ([facts/categories.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/facts/categories.md), section 3.3: BEAM's reduction count and CPU time are close, but neither is a budget type).

<a id="l2-役割の比較"></a>

## L2: Role comparison

| Role | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| Amount of deterministic compute time | `ns` `us` `ms` `s` `min` `h` | — | — | — (`process_time` is CPU time and nondeterministic) | — | — | — (reductions are a count, not a type) | — (`Sys.time` is CPU time) | — | — |
| Add, subtract, integer multiplication, compare | `+` `-` `* Int` `<` (`Ord`) | — | — | — | — | — | — | — | — | — |
| Use as a budget | `fan.bounded(c)`, `fan.min(c, …)` | — | — | — | — | — | — | — | — | — |
| Mixing with wall-clock time | Type error (S3) | — | — | — | — | — | — | — | — | Separate `Instant` for each `Clock` |
| Read out numerically | — (S7) | — | — | — | — | — | — | — | — | — |

<a id="完全性の規則"></a>

### Completeness rules

The same as rules 1–4 in [L2-duration](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-duration.md) (ADR-0001's S2, S3, and S7 require the same surface for the two clocks). Add `compare` alongside `duration` as well.

<a id="l2-の判定"></a>

### L2 verdict

- The absence of counterparts in the nine languages was already adjudicated in L1 R-L1-21: “There is an Almide-specific reason (the determinism promise), so do not follow the majority.”
- Missing cell: `compare` (R-L0-16) → R-compute-01.
- Duplicates: none.

<a id="案"></a>

## Proposals

<a id="r-compute-01-computecompare-を足す---採用"></a>

### R-compute-01 Add `compute.compare` — ○ Accepted

- Content: `compute.compare(a: Compute, b: Compute) -> Int`.
- Supporting L0 rule: R-L0-16 (ordered types have `compare`). Add it together with `duration.compare`, following ADR-0001 S1: “both types have the same operation surface.”
- (b) Pure computation. The charge count is deterministic, so the comparison result is also the same on both targets.
- (c) To sort a list of budgets, write `list.sort_with(cs, compute.compare)`.
- (d) Addition. Usage count 0.
- Proposed gate: the same gate as R-duration-01 (`compare` for standard types that derive `Ord`).
- issue: #3902

<a id="r-compute-02-duration-と統合するまたは読み出し関数を足す---不採用"></a>

### R-compute-02 Merge with `duration`, or add readout functions — × Rejected

- Content: do neither. This follows ADR-0001 (the two clocks are separate, mutually non-convertible types; readout is omitted under S7) and L1 R-L1-21. Making UFCS `n.ms()` an ambiguity error also protects this separation (S2).
- Keep the types separate while declaring their shared shape through the common protocol `TimeAmount` → [L2-duration](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-duration.md) R-duration-03.
- issue: none

<a id="r-compute-03-compute-を共通のプロトコル-timeamount-に従わせる---採用"></a>

### R-compute-03 Make `Compute` conform to the common protocol `TimeAmount` — ○ Accepted

- Content: the other half of [L2-duration](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-duration.md) R-duration-03. Both `Compute` and `Duration` conform to `TimeAmount` (constructors for 6 units and `compare`). Only the clock differs: `Duration` is wall-clock time, whereas `Compute` is deterministic compute time (not OS CPU time, but the same charge count on every host).
- issue: #3964

<a id="l3-関数表"></a>

## L3: Function table

The 6 current functions in `compute` and 1 function to add.

| Function | Signature (current, mut shown) | Main spellings in other languages | Verdict | Proposal |
|---|---|---|---|---|
| `ns` | `compute.ns(n: Int) -> Compute` | — (no counterpart in the nine languages; for wall-clock time, Rust `Duration::from_nanos`) | Keep | — |
| `us` | `compute.us(n: Int) -> Compute` | — (for wall-clock time, Rust `from_micros`) | Keep | — |
| `ms` | `compute.ms(n: Int) -> Compute` | — (for wall-clock time, Rust `from_millis`, Go `time.Millisecond`) | Keep | — |
| `s` | `compute.s(n: Int) -> Compute` | — (for wall-clock time, Rust `from_secs`) | Keep | — |
| `min` | `compute.min(n: Int) -> Compute` | — (for wall-clock time, Rust `from_mins`) | Keep | — |
| `h` | `compute.h(n: Int) -> Compute` | — (for wall-clock time, Rust `from_hours`) | Keep | — |
| `compare` | (addition) `compute.compare(a: Compute, b: Compute) -> Int` | Rust `cmp` / Kotlin `compareTo` | Add | R-compute-01 |

## Source caveats

The six-current/one-to-add table, accepted proposals, protocol conformance, usage count and cross-target determinism claims describe the pinned roadmap, not an independently checked current API or implementation. Compute time remains distinct from both wall-clock duration and OS CPU time.
