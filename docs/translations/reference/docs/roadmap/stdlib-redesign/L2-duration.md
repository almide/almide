<!-- description: Stdlib redesign L2/L3 — duration: the six wall-clock constructors stay as ADR-0001 fixed them; add compare as the function form of its Ord, keep readers and Float constructors omitted -->
<a id="l2l3-duration"></a>

# L2/L3: duration

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-duration.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-duration.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-duration.zh-CN.md)

> Non-normative historical reader snapshot. The pinned original is authoritative; no automatic synchronization. No independent rerun of measurements, proof gates or current implementation checks. Quoted commands are not executed. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-duration.md)
> This dated redesign records decisions and proposed gates, not a new implementation or usage census. R-duration-01/03 acceptance and R-duration-03 current-state wording coexist with the L3 table calling compare an addition; these stages are not silently reconciled. Reader/Float/day omissions and reopening conditions remain; TimeAmount conformance does not merge clocks or permit conversion. Compute counts frozen-machine charges, not measured CPU time.

> Last updated: 2026-10-21
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md), [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md) (R-L1-21: do not merge with `compute`), [ADR-0001](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0001-deterministic-time-units.md)
> Related: [L2-compute](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-compute.md), [L2-datetime](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-datetime.md)
> Sources: Rust https://doc.rust-lang.org/std/time/struct.Duration.html ; Go https://pkg.go.dev/time#Duration ; Python https://docs.python.org/3/library/datetime.html#timedelta-objects ; TS/JS https://tc39.es/proposal-temporal/docs/duration.html ; Gleam https://hexdocs.pm/gleam_time/gleam/time/duration.html (official package); Elixir https://hexdocs.pm/elixir/Duration.html ; OCaml https://ocaml.org/manual/latest/api/Unix.html (no Duration type in the standard library); Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.time/-duration/ ; Swift https://developer.apple.com/documentation/swift/duration

`duration` is a closed surface as specified by ADR-0001: six constructors, `ns` `us` `ms` `s` `min` `h`; negative values deterministically abort; construction overflow saturates; operations are only `+`, `-` (saturating at zero), `* Int`, and same-type comparisons (S3). Numeric readers such as `in_ms`, `Float` constructors and units of days or larger are intentional omissions with reopening conditions in S7. The 2026-10-16 ruling withdrew the proposal to add readers; display uses `Repr`.

<a id="l2-役割の比較"></a>

## L2: Role Comparison

| Role | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| Construct from units | `ns` `us` `ms` `s` `min` `h` | `from_nanos` `from_millis` `from_secs` etc. | `n * time.Millisecond` | `timedelta(milliseconds=n)` | `Temporal.Duration.from` | pkg `duration.milliseconds` | `Duration.new!` | — | `n.milliseconds` | `.milliseconds(n)` |
| Units of days or larger | —(S7) | — | — | `days=` | `days` | — | `day:` | — | `n.days` | — |
| Construct from fractions | —(S7) | `from_secs_f64` | — | `seconds=1.5` | — | — | — | — | `1.5.seconds` | `.seconds(1.5)` |
| Add/subtract/integer multiply | `+` `-` `* Int` | `+` `-` `*` | `+` `-` `*` | `+` `-` `*` | `add` | pkg `add` | `Duration.add` | — | `+` `-` `*` | `+` `-` `*` |
| Compare | `<` `==` etc.(`Ord`) | `<` `cmp` | `<` | `<` | `Duration.compare` | pkg `compare` | `Duration` has no comparison function | — | `<` `compareTo` | `<` |
| Read numerically | —(S7, display via `Repr`) | `as_millis` | `Milliseconds()` | `total_seconds()` | `total` | pkg `to_seconds` | `to_timeout` | — | `inWholeMilliseconds` | `components` |
| Separate quantities of different clocks | `Compute` mixing is a type error | — | — | — | — | — | — | — | — | `Instant` per `Clock` |

<a id="完全性の規則"></a>

### Completeness Rules

1. ADR-0001 S2: all 12 cells of two clocks × six units exist: six in `duration` and six in [compute](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-compute.md). Units form a closed set.
2. ADR-0001 S3: use operators for arithmetic; `Duration` and `Compute` cannot mix, and no conversion functions exist.
3. R-L0-16, ordered types have `compare`: since `Duration` derives `Ord`, provide `duration.compare(a, b) -> Int`, the same shape as `compute.compare`. It is the function form of `<`, not a numeric reader of the quantity, so it is consistent with S7's omission.
4. Intentional omissions, S7 ledger: readers, `Float` constructors, units of days or larger, `T / Int`, `min` and `max`.

<a id="l2-の判定"></a>

### L2 Verdict

- Missing roles: readers (7/9), fractional constructors (4/9), and day units (5/9) are not added because ADR-0001 S7 reopening conditions are not met (R-duration-02 ×).
- Missing cell: `compare`, R-L0-16 → R-duration-01.
- Duplication: none. `duration.ms` and `compute.ms` are distinct values of different types, as #3851's “Dropped” states.

<a id="案"></a>

## Proposals

<a id="r-duration-01-durationcompare-を足す---採用"></a>

### R-duration-01 Add `duration.compare` — ○ Accepted

- Content: `duration.compare(a: Duration, b: Duration) -> Int`, negative for `a < b`, zero if equal, positive for `a > b`.
- L0 basis: R-L0-16, ordered types have `<mod>.compare(a, b) -> Int`, and R-L0-02.
- (a) Rust `Ord::cmp`, Kotlin `compareTo`, Temporal `Duration.compare`.
- (b) Pure computation. Compares two single nanosecond `Int` values, identical on both targets.
- (c) Pass directly to functions accepting comparison functions, such as `list.sort_with(ds, duration.compare)`. The same spelling as `Int`'s `int.compare` makes it discoverable by rule.
- (d) Additive, zero uses. ADR-0001 S2's 12-cell gate, S6-1, counts only constructors and does not change.
- Gate proposal: include `Duration` and `Compute` in the R-L0-16 gate checking that all standard types deriving `Ord` have `compare`.
- Issue: #3902.

<a id="r-duration-02-読み出し関数小数の構築子日の単位を足す---不採用"></a>

### R-duration-02 Add Readers, Fractional Constructors and Day Units — × Rejected

- Content: do not add them. Follow ADR-0001 S7's ledger and the 2026-10-16 withdrawal of the readers proposal.
- Basis: ADR-0001 S7 reopening conditions — an actual use of budgets in computation, Dojo observations of incorrect integer conversion, documented real demand — remain unmet. L1's addition criterion requires that existing rulings cannot cover the need.
- Issue: none.

<a id="r-duration-03-duration-と-compute-を-1-つのプロトコル-timeamount-に従わせる---採用"></a>

### R-duration-03 Have `Duration` and `Compute` Conform to One `TimeAmount` Protocol — ○ Accepted

- Current state: `Duration`, wall-clock time, and `Compute`, deterministic computational time, share six unit constructors, `compare` and operators. But this common surface is enforced only by ADR-0001 S1 prose and duplicated per-type matrix gates, not by a language declaration.
- Proposal: put one protocol in the standard library and have both types conform.
  ```almide
  protocol TimeAmount {
    fn ns(n: Int) -> Self
    fn us(n: Int) -> Self
    fn ms(n: Int) -> Self
    fn s(n: Int) -> Self
    fn min(n: Int) -> Self
    fn h(n: Int) -> Self
    fn compare(a: Self, b: Self) -> Int
  }
  type Duration: Eq, Ord, Repr, TimeAmount
  type Compute: Eq, Ord, Repr, TimeAmount
  ```
  - A single declaration expresses that the types differ only in which clock their quantity belongs to; usage is the same.
  - Types stay separate, R-L1-21 / ADR-0001 S3: mixing is a type error, no conversion functions. The protocol states common shape; it does not permit substituting one type for the other.
  - Constructors and `compare` use the existing module functions, such as `duration.ms` and `compute.compare`, directly as protocol methods. Do not add forwarding functions for the same computation, R-L0-01.
  - Operators `+`, `-`, `* Int`, `<` remain on both types per ADR-0001 S3's matrix. Protocols contain only methods, so these are not included.
  - Users can write one clock-agnostic function for both: `fn half_or_less[T: TimeAmount](a: T, limit: T) -> T`.
- L0 basis: R-L0-01, do not handwrite one role in two places; CLAUDE.md's “derive properties from one declaration.”
- Terminology: `Compute` is not OS-measured CPU time. It counts charges in a frozen abstract machine, counted at function entry and loop heads, and is deterministic across hosts (ADR-0001). It is the other clock paired with wall time.
- (a) Swift's `Clock` protocol lets each clock choose a quantity type conforming to `DurationProtocol` ([DurationProtocol](https://developer.apple.com/documentation/swift/durationprotocol); standard `ContinuousClock` and `SuspendingClock` both use the same `Duration` type). Java's `TemporalAmount`, implemented by `Duration` and `Period`, similarly groups quantity types under one interface. Other languages do not separate clock differences by type.
- (b) Checker-only change; representation remains a single nanosecond Int.
- (c) The correspondence “learn `duration.ms(5)` and write `compute.ms(5)` the same way” becomes visible as a type declaration, not a documentation promise. Protocol checking also prevents functions being added to only one side.
- (d) Additive: one new protocol and declarations on two types. Existing code is unchanged. `TimeAmount` was chosen because it collides with none of the languages' spellings and reads as “a quantity of time.”
- Completeness rule: a type conforming to `TimeAmount` has all six unit constructors plus `compare`, enforced by protocol checking. This protocol check replaces ADR-0001 S2's 12-cell matrix gate.
- Gate proposal: verify both types conform to `TimeAmount` with a generic-function test under `spec/stdlib/`, calling the same generic function on both `Duration` and `Compute`.
- Issue: #3964.

<a id="r-duration-04-min分を-minutes-などの完全な綴りにする---不採用"></a>

### R-duration-04 Rename `min` (Minutes) to a Full Spelling Such as `minutes` — × Rejected

- Reason: ADR-0001 S2 fixes the six unit spellings. Writing `duration.min(a, b)` intending a minimum produces E004 for wrong argument count, not a silently different value.
- Issue: none.

<a id="l3-関数表"></a>

## L3: Function Table

The six existing `duration` functions and one addition.

| Function | Signature (current, mut shown) | Common spellings elsewhere | Verdict | Proposal |
|---|---|---|---|---|
| `ns` | `duration.ns(n: Int) -> Duration` | Rust `from_nanos` / Go `time.Nanosecond` / Kotlin `nanoseconds` / Swift `.nanoseconds` | Retain | — |
| `us` | `duration.us(n: Int) -> Duration` | Rust `from_micros` / Go `time.Microsecond` / Python `microseconds=` / Kotlin `microseconds` | Retain | — |
| `ms` | `duration.ms(n: Int) -> Duration` | Rust `from_millis` / Go `time.Millisecond` / Python `milliseconds=` / Kotlin `milliseconds` / Swift `.milliseconds` | Retain | — |
| `s` | `duration.s(n: Int) -> Duration` | Rust `from_secs` / Go `time.Second` / Python `seconds=` / Kotlin `seconds` / Swift `.seconds` | Retain | — |
| `min` | `duration.min(n: Int) -> Duration` | Rust `from_mins` / Go `time.Minute` / Python `minutes=` / Kotlin `minutes` | Retain | — |
| `h` | `duration.h(n: Int) -> Duration` | Rust `from_hours` / Go `time.Hour` / Python `hours=` / Kotlin `hours` | Retain | — |
| `compare` | (addition) `duration.compare(a: Duration, b: Duration) -> Int` | Rust `cmp` / Kotlin `compareTo` / Temporal `Duration.compare` | Add | R-duration-01 |
