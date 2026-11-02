<a id="als--決定的時間deterministic-time"></a>

# ALS — Deterministic time

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](deterministic-time.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](deterministic-time.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](deterministic-time.zh-CN.md)

> Non-normative reader snapshot of the [pinned original](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/deterministic-time.md). Normative status and requirements below belong to the original, not this copy. No automatic synchronization; historical implementation, conformance and safety claims have not been independently revalidated for this translation. Code and rule identifiers are preserved.

> Last updated: 2026-08-27

Normalized from [ADR-0001](../../../../../adr/0001-deterministic-time-units.md) (S1–S8) and [SPEC.md §13](../../../../../SPEC.md). Deterministic time is a logical clock measured as “charge unit × CM-1 (a versioned calibration constant)”, not wall-clock time. Every contract in this chapter includes native == wasm observational agreement (stdout / stderr / exit code).

<a id="als-dt1-時間構築子と代数"></a>

## ALS-DT1 Time constructors and algebra

Time quantities are created only by module-qualified constructors over the closed unit set (`ns / us / ms / s / min / h`): `compute.*` = deterministic computation time, `duration.*` = wall-clock time. A bare `Int` is a type error. Negative constructor arguments and negative scale factors cause the deterministic abort of the §13 termination convention (`Error: negative time...` + exit 1); construction overflow saturates at the maximum.
The algebra permits only `T + T` (saturating), `T - T` (saturating at 0), `T × Int` (saturating; abort on a negative factor) and comparisons of like types. `T × T`, mixing clocks and `/` are type errors.
Fixtures: `spec/wasm_cross/time_negative_trap.almd`, `time_negative_scale.almd`,
`time_saturate.almd`, `time_ops_algebra.almd`.

<a id="als-dt2-決定的予算fanbounded"></a>

## ALS-DT2 Deterministic budgets (fan.bounded)

The verdict of `fan.bounded(c) { body }` is a function only of the program and its input: it is Err only when consumed charge units exceed budget units (`ns / CM-1`, rounded down), with the ledger-constant message `fan.bounded: budget exhausted`. The same program switches between Ok ⇄ Err at the same declared nanoseconds on every target and host (exact unit boundaries). Nesting uses min-cap (EIP-150 style — the current release rejects nested regions themselves with E007, so executable evidence for this rule exists only in the reference-evaluator implementation). Bind statements cost 0 charge.

**Charge table (CM-1 v0.3 = 3ns/unit, C-207)**: charge sites are (a) fn entry — 1 unit, except that a loop-free, nonrecursive callee disappears in the shared MIR inliner (including its entry charge: 0 units, C-294); (b) loop-head condition — 1 unit per evaluation (`while` also counts the final false condition: n iterations = n+1 units); (c) bulk string concatenation — `1 + 結果バイト長/16` units (result byte length; result-keyed, C-204 T3-5). Bind statements, straight-line expressions and branches cost 0. The verdict is **check-then-charge**: cut at each site if `spent + cost > budget` (an expression that finishes before reaching its first charge succeeds even with a zero budget). `heavy(1000)` (entry 1 + loop heads 1001) = 1002 units = 3006ns is the calibration example; `spec/wasm_cross/fuel_bounded_boundary.almd` and `fuel_dyn_charge.almd` (252 units, switching at 750/760ns) fix the boundaries to the exact unit.

**Cut bookkeeping (2026-08-25 ruling, C-320)**: a cut caused by budget exhaustion performs the same meter bookkeeping (region exit) as an ordinary exit before leaving. Therefore:

1. An exhausted region's verdict is **always Err**. Ok containing an intermediate or zero value (a stale verdict) is nonconforming.
2. A region's verdict and value **do not depend on other regions** in the same program (meter state does not leak between regions: a preceding region's cut is not observed to change a later region's verdict).
3. Whether a cut fires in a **loop directly inside an arm** or **inside a callee** is unobservable (the same verdict at the same declared nanoseconds). An arm's value is its body block's value, independent of loop form (`for` / `while`).
4. The meter charges only computation in the budgeted body. Runtime machinery materializing observations (string formatting and the `??` fallback path) is neither charged nor cut; fallback after Err and subsequent output always remain sound.

Fixtures: `spec/wasm_cross/fuel_bounded_boundary.almd`, `fuel_block_body.almd`,
`fuel_bare_result.almd`; cut bookkeeping uses `spec/wasm_cross/fuel_cut_in_arm_loop.almd` (0.58.0 violates rules 1–2 identically on both targets: a cut in a directly nested loop yields stale Ok(0), contaminating all subsequent regions. This is a case not revealed by an agreement verdict and is carried by agreement until the reference leg fixes the value. almide/almide#1572).

<a id="als-dt3-決定的-racefanmin"></a>

## ALS-DT3 Deterministic race (fan.min)

The winner is the lexicographic minimum of `(spend, index)` among arms that finish within budget and do not themselves return Err: the cheapest arm wins, with ties broken by source order. Err in a Result arm removes that arm from the candidates (it does not propagate). If all are eliminated, the result is the ledger-constant Err.
Fixtures: `spec/wasm_cross/fuel_race_boundary.almd`, `fuel_race_err_skip.almd`,
`fuel_bare_result.almd`.

<a id="als-dt4-settle-の-tuple-契約"></a>

## ALS-DT4 Tuple contract for settle

The value of `fan.settle { a; b; … }` is an arm-ordered tuple `(Result[A, String], …)`: heterogeneous arms are allowed, plain arms are wrapped in Ok, and an effect arm's Err is captured in that slot (not propagated). Evaluation is deterministic in arm order.
Fixtures: `spec/wasm_cross/fan_settle_tuple.almd`.

<a id="als-dt5-壁時計期限fantimeoutoracle-層"></a>

## ALS-DT5 Wall-clock deadlines (fan.timeout, oracle layer)

`fan.timeout(duration.ms(n)) { body }` **cooperatively checks** the wall-clock deadline **at charge sites** (the unified suspension-point principle: never cut midway through an operation, the same cooperative model as Go contexts). The verdict is relative to ω (R_Ω): which check expires is host input, and **replay with ω fixed reproduces observations byte for byte** (by definition, this also holds when replaying on wasm an ω recorded on native). Only the two ends can be claimed deterministically: a sufficiently large deadline always allows completion, while a divergent body + a tiny deadline always cuts at a charge site.
Fixtures: `spec/wasm_cross/fuel_timeout_ends.almd` (the two ends) + the record/replay gate (`tests/charge_probe_test.rs::timeout_deterministic_ends_and_replay`).

> Source context: the document is dated 2026-08-27 and retains historical fan spellings, a release-specific nested-region rejection, the 0.58.0 defect, and an expected future reference-leg update. These statements are not a newly verified description of current implementation. Its sufficiently-large-deadline claim is retained as written; this copy does not extend it into a completion guarantee for divergent bodies.
