> Description (inactive source index metadata): Stdlib redesign L2/L3 — datetime: Int Unix seconds stay the representation; add now_ms as the one millisecond clock, retire the identity and operator wrappers
<a id="l2l3-datetime"></a>

# L2/L3: datetime

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-datetime.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-datetime.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-datetime.zh-CN.md)

> Non-normative historical redesign snapshot; pinned original remains authoritative and unchanged. No automatic synchronization, fresh source retrieval, independent probe or current implementation verification. (F) and measurements refer to the original author’s historical checks. Preserve the old carrying from_parts behavior versus the later Result proposal and the old signature in its table; no silent correction to a present API. The broad seconds-input/only-now_ms statements coexist with monotonic_ns in the original and are not generalized into a unit guarantee. Duration extraction and type decisions belong to this source epoch, not reconciliation with later ADRs. In preserved pseudocode, 単位 means unit. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-datetime.md)

> Last updated: 2026-10-21
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md) / [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md) (R-L1-25: `datetime` owns clocks; time zones are not standard)
> Related: [L2-duration](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-duration.md) / [L2-compute](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-compute.md) / L2-env
> Sources: Rust https://doc.rust-lang.org/std/time/struct.SystemTime.html / Go https://pkg.go.dev/time / Python https://docs.python.org/3/library/datetime.html and https://docs.python.org/3/library/time.html / TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Date / Gleam https://hexdocs.pm/gleam_time/ (official package, not standard) / Elixir https://hexdocs.pm/elixir/DateTime.html / OCaml https://ocaml.org/manual/latest/api/Unix.html / Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.time/-instant/ / Swift https://developer.apple.com/documentation/foundation/date

Placement of time in each language is in [facts/categories.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/facts/categories.md) sections 1.G / 3.3 (Kotlin `kotlin.time.Instant` and WASI clocks have been checked (F)).

Measured (`almide 0.68.0 dev 437b8f127`, same on native and wasm): `datetime.weekday(0)` is `"Thursday"`. `datetime.from_parts(2024, 13, 1, 0, 0, 0)` is `2025-01-01T00:00:00Z` (overflowing months carry over). `datetime.format(0, "%Y %%")` is `"1970 %%"` (does not convert `%%` to `%`). `parse_iso("2024-01-01T09:00:00+09:00")` is `1704067200` (correctly subtracts the offset); fractional seconds yield err. `datetime.from_unix(5)` is `5` (identity function).

<a id="l2-役割の比較"></a>

## L2: Role comparison

| Role | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| Timestamp representation | `Int`（Unix seconds） | `SystemTime` | `time.Time` | `datetime` / `float` seconds | `Date`（ms） | pkg `Timestamp` | `DateTime` | `float`（`Unix.time`） | `Instant` | `Date` |
| Current time (seconds) | `now()` and `env.unix_timestamp()` | `SystemTime::now()` | `time.Now().Unix()` | `time.time()` | — | pkg | `System.os_time(:second)` | `Unix.time` | `Clock.System.now()` | `Date()` |
| Current time (ms) | `env.millis()` | `duration_since(UNIX_EPOCH)` | `UnixMilli()` | `time.time_ns()` | `Date.now()` | pkg | `System.os_time(:millisecond)` | — | `toEpochMilliseconds()` | `timeIntervalSince1970` |
| Monotonic clock | `monotonic_ns()` | `Instant::now()` | `time.Now()`（with monotonic component） | `time.monotonic_ns()` | `performance.now()` | pkg | `System.monotonic_time` | — | `TimeSource.Monotonic` | `ContinuousClock` |
| Construct from components | `from_parts` | — | `time.Date` | `datetime(...)` | `Date.UTC` | pkg | `DateTime.new` | `Unix.mktime` | `LocalDateTime(...)` | `DateComponents` |
| Extract components | `year` `month` `day` `hour` `minute` `second` `weekday` | — | `Year()` etc. | `.year` etc. | `getUTCFullYear()` etc. | pkg | `.year` etc. | `Unix.gmtime` | `.year` etc. | `Calendar.component` |
| Weekday | `weekday`（English name as String） | — | `Weekday()`（number; `String()` for name） | `weekday()`（0 = Monday） | `getUTCDay()`（0 = Sunday） | pkg | `Date.day_of_week`（1 = Monday） | `tm_wday`（0 = Sunday） | `dayOfWeek`（enum） | number（1 = Sunday） |
| Read / write ISO 8601 | `parse_iso` `to_iso` | — | `Parse(RFC3339)` `Format` | `fromisoformat` `isoformat` | `toISOString` `Date.parse` | pkg | `from_iso8601` `to_iso8601` | — | `Instant.parse` `toString` | `ISO8601DateFormatter` |
| Format | `format(ts, "%Y-%m-%d")` | — | `Format("2006-01-02")` | `strftime` | `Intl` | pkg | `Calendar.strftime` | — | `format` | `DateFormatter` |
| Add / subtract time | `add_days` `add_hours` `add_minutes` `add_seconds`、`diff_seconds` | `+ Duration` | `Add` `Sub` | `+ timedelta` | arithmetic | pkg | `DateTime.add` `diff` | arithmetic | `+ Duration` | `addingTimeInterval` |
| Compare before / after | `is_after` `is_before` | `<` | `After` `Before` | `<` | `<` | pkg | `DateTime.compare` | `<` | `<` | `<` |
| Convert to / from Unix seconds | `from_unix` `to_unix`（identity） | `UNIX_EPOCH +` | `time.Unix` `.Unix()` | `fromtimestamp` `timestamp` | `new Date(ms)` `getTime` | pkg | `from_unix` `to_unix` | — | `fromEpochSeconds` `epochSeconds` | `Date(timeIntervalSince1970:)` |
| Add months (calendar arithmetic) | — | — | `AddDate` | — | `setUTCMonth` | pkg | `Date.shift` | — | `plus(DateTimePeriod)` | `Calendar.date(byAdding:)` |
| Time zones | —（UTC and fixed offsets only） | — | `LoadLocation` | `zoneinfo` | `Intl` | — | external db | — | pkg | `TimeZone` |

<a id="完全性の規則"></a>

### Completeness rules

1. **Representation**: timestamps are `Int` Unix seconds (UTC). Only `now_ms()` returns millisecond timestamps; all other `datetime` functions take seconds.
2. **Clock ownership** (R-L1-25): only `datetime` owns wall clocks (seconds/milliseconds) and the monotonic clock. The 3 functions are `now` (seconds), `now_ms` (milliseconds), `monotonic_ns` (nanoseconds; only differences are meaningful); name suffixes indicate units.
3. **Component pairs**: each of the 6 components constructible with `from_parts` (year/month/day/hour/minute/second) has an extractor (`year` … `second`).
4. **Addition family**: `add_<単位>` is closed at 4 units: `days`, `hours`, `minutes`, `seconds`. Do not provide weeks/months/years (months and years require calendar rules, within the L1 decision “calendars and tz belong in packages”).
5. Do not provide functions that do the same computation as operators or identity functions (R-L0-01; a function that merely calls another public function once also counts as a synonym).
6. **Comparison**: timestamps are `Int`, so do not provide `datetime.compare`. Pass the three-way comparator `int.compare` (R-L0-16) directly (`list.sort_with(ts, int.compare)`). Adding one would forward to `int.compare`, violating R-L0-01.

<a id="l2-の判定"></a>

### L2 verdict

- Duplication: `env.unix_timestamp` ≡ `datetime.now` (#3851 B); move `env.millis` to `datetime.now_ms` (R-L1-25) → R-datetime-01 (`env` removals in L2-env). `from_unix` `to_unix` (identity), `is_after` `is_before` `diff_seconds` (same as `>` `<` `-`) → R-datetime-03.
- Representation: create a dedicated timestamp type? → R-datetime-02 (×).
- Missing roles: month addition (5/9, but calendar rules needed) / time zones (× in L1) → do not add.
- Weekday return type → R-datetime-04 (×).

<a id="案"></a>

## Proposals

<a id="r-datetime-01-ミリ秒の現在時刻-datetimenow_ms-を足す---採用"></a>

### R-datetime-01 Add millisecond current time `datetime.now_ms` — ○ Accepted

- Content: `effect fn datetime.now_ms() -> Int` (milliseconds from Unix epoch, wall-clock time). Same body as `env.millis`. Remove `env.millis` and `env.unix_timestamp` in the L2-env proposals, with hints explicitly naming `datetime.now_ms()` and `datetime.now()`, respectively.
- Supporting L0 rules: R-L0-01, R-L0-02 (`datetime` owns clocks), L1 R-L1-25.
- (a) A majority collect clocks in one place (Rust `std::time`, Go `time`, Kotlin `kotlin.time`).
- (b) wasm uses `wasi:clocks`'s `wall-clock`; native uses the OS clock. Keep the current `env.millis` path.
- (c) When measuring elapsed time, LLMs incorrectly subtract `datetime.now` values in seconds and get 0. A millisecond clock in the same module is discoverable by name (docs explain that this is a situation where `monotonic_ns` should be used).
- (d) Addition. Usage count 0 (`env.millis` 11/7/21/3 and llms.txt:388 move here).
- Proposed gate: add a verdict row to ADR-0027 effect inspection. Synonym-gate rule 5 of #3851 F counts the twin with `env.millis` in the allowlist until removal.
- issue: #3851 B (existing)

<a id="r-datetime-02-時刻専用の型instant-などを作る---不採用"></a>

### R-datetime-02 Create a dedicated timestamp type (such as `Instant`) — × Rejected

- Content: do not create one. Keep timestamps as `Int` Unix seconds.
- Supporting L0 rule: R-L0-00. R-L0-14 item 4 now allows changing a type under the same name if old usage becomes a type error. Even so, adding a type changes arguments and return types of all 22 functions, requiring every user handling timestamps as `Int` to rewrite code. It is not worth that cost and losing the benefits in (b)(c) below.
- (a) A majority use a dedicated timestamp type (Rust, Go, Python, JS, Elixir, Kotlin, Swift). Almide instead bridges this difference with names and checks: units are indicated by function suffixes (`now`, `now_ms`, `monotonic_ns`); ADR-0001 separates time quantities (`Compute`, `Duration`) by type, not timestamps (instants).
- (b) `Int` seconds directly fit ADR-0016's determinism commitment and can be emitted directly in JSON.
- (c) With a dedicated type, there is no function to extract a number from `Duration` (ADR-0001 S7), so no way to display the `diff` result in seconds. Current `Int` permits one way to write it.
- Reopening condition: if Dojo measurements observe a milliseconds/seconds mix-up (`datetime.year(datetime.now_ms())`), reintroduce the type proposal as a family with new names.
- issue: none

<a id="r-datetime-03-何もしない関数と演算子の双子を撤去する---採用"></a>

### R-datetime-03 Remove identity functions and operator twins — ○ Accepted

- Content: remove `from_unix` (returns the argument unchanged), `to_unix` (same), `is_after(a, b)` (= `a > b`), `is_before(a, b)` (= `a < b`), `diff_seconds(a, b)` (= `a - b`).
- Supporting L0 rule: R-L0-01 (one spelling per role; merely calling another public function once also counts as a synonym. ADR-0005 desugars operators to standard functions, so a function consisting of one operator is treated likewise).
- (a) In languages with dedicated timestamp types, functions such as Elixir `from_unix` and Kotlin `isAfter` are meaningful. In Almide, where timestamps are `Int`, nothing is converted.
- (c) With `datetime.from_unix(ms)`, an LLM reads “converts units,” hiding an error that uses milliseconds unchanged as seconds. Without the function, the written form exposes that it is `Int`.
- (d) Remove 5. `@deprecated(since=15, use="x")` (`from_unix` / `to_unix`, hint “use the argument directly”), `use="a > b"` (`is_after`), `use="a < b"` (`is_before`), `use="a - b"` (`diff_seconds`), all with `removal_epoch = 16`. `almide fix`: `datetime.from_unix(x)` → `x`, `datetime.to_unix(x)` → `x`, `datetime.is_after(a, b)` → `a > b`, `datetime.is_before(a, b)` → `a < b`, `datetime.diff_seconds(a, b)` → `a - b`. Usage counts (spec/tests/docs/stdlib): `from_unix` 12/0/6/0, `to_unix` 7/0/6/0 (+1 research), `is_after` 3/0/6/0, `is_before` 5/0/6/0, `diff_seconds` 4/0/6/0. Absent from CHEATSHEET / llms.txt.
- Proposed gate: add synonym-gate rules detecting “public functions whose body returns the argument unchanged” and “public functions whose body is a single operator” (allowlist only shrinks).
- issue: #3903

<a id="r-datetime-04-weekday-を数を返す関数にする---不採用"></a>

### R-datetime-04 Make `weekday` return a number — × Rejected

- Content: do not change it. `weekday` continues returning a `String` English weekday name (`"Monday"` …).
- Supporting L0 rule: R-L0-00. Changing the return type to `Int` makes old usage (`== "Monday"`) a type error, so R-L0-14 item 4 permits it. Still, do not change it because numbers bring no benefit as explained in (a).
- (a) Most languages return numbers, but differ on whether counting starts at 0 or 1, and Sunday or Monday (table above). Numbers would be harder to guess correctly.
- (c) Confusing strings and numbers is stopped by type checking, not a silent error.
- issue: none

<a id="r-datetime-05-format-の--を--にする---不採用"></a>

### R-datetime-05 Make `format` convert `%%` to `%` — × Rejected

- Content: do not change. Retain the docs' “there is no `%%` escape” and add it to CHEATSHEET's `format` row.
- Supporting L0 rule: R-L0-14 item 4 (do not change behavior under the same name so old code type-checks yet produces a different result; changing `%%` output does exactly that). There are 0 formats containing `%%` in spec; the harm of diverging from old training-data semantics outweighs the benefit of changing the spelling.
- issue: none

<a id="r-datetime-06-add_単位-の族を行列として宣言し演算子の双子の検出から外す---採用記録とゲートの修正"></a>

### R-datetime-06 Declare the `add_<単位>` family as a matrix and exclude it from operator-twin detection — ○ Accepted (record and gate changes)

- Now: the body of `add_seconds(ts, n)` is `ts + n`. R-datetime-03's proposed gate (detect a public function whose body is a single operator) also catches `add_seconds`. Meanwhile, the completeness rule closes `add_<単位>` at 4 functions.
- Proposal: declare `add_{seconds, minutes, hours, days}` as a unit-axis matrix, treated like R-L0-01's “type-matrix cells do not count as synonyms.” Derive exceptions from the family declaration (1 location), not an allowlist.
- (d) No function changes.
- issue: #3974

<a id="r-datetime-07-from_parts-は範囲外の部品を-err-にする---採用"></a>

### R-datetime-07 Make `from_parts` reject out-of-range components with err — ○ Accepted

- Now: `datetime.from_parts(...) -> Int` carries over out-of-range components (February 30 at 25:61 becomes March 2 at 02:01). `parse_iso` returns err for the same input. These 2 constructors of the same value treat invalid dates oppositely.
- Proposal: use `fn datetime.from_parts(y, m, d, h, min, s) -> Result[Int, String]`, yielding err with the same wording as `parse_iso` (such as “day out of range: 30”).
- Supporting L0 rule: R-L0-06 item 4 (fallible construction returns Result with a reason instead of silently changing to another value).
- (a) Python `datetime()`, Elixir `DateTime.new`, Kotlin `LocalDateTime` reject; Go `time.Date`, JS `Date.UTC`, OCaml `Unix.mktime`, Swift `Calendar` carry over. No majority. Prioritize consistency with Almide's own `parse_iso`.
- (c) On tasks checking whether dates are valid, LLMs use `from_parts` and let invalid dates pass.
- (d) Semantic change. The return type changes, so old code stops with a type error, satisfying R-L0-14.4. `almide fix` adds `!` to calls. Code using `from_parts(y, m + 1, …)` as a workaround for month addition (already rejected) now gets err out of range, so give a hint showing how to write month carry-over explicitly.
- Proposed gate: pin boundary-input results (February 29/30, month 13, hour 24) in spec.
- issue: #3974

<a id="l3-関数表"></a>

## L3: Function table

The current 22 `datetime` functions and 1 addition.

| Function | Signature (current, mut notation) | Main spellings in other languages | Verdict | Proposal |
|---|---|---|---|---|
| `add_days` | `datetime.add_days(ts: Int, n: Int) -> Int` | Go `AddDate(0,0,n)` / Python `+ timedelta(days=n)` / Elixir `DateTime.add(dt, n, :day)` | Unchanged | — |
| `add_hours` | `datetime.add_hours(ts: Int, n: Int) -> Int` | Go `Add(n*time.Hour)` / Python `timedelta(hours=n)` / Kotlin `+ n.hours` | Unchanged | — |
| `add_minutes` | `datetime.add_minutes(ts: Int, n: Int) -> Int` | Go `Add(n*time.Minute)` / Python `timedelta(minutes=n)` | Unchanged | — |
| `add_seconds` | `datetime.add_seconds(ts: Int, n: Int) -> Int` | Go `Add(n*time.Second)` / Elixir `DateTime.add(dt, n)` / Swift `addingTimeInterval` | Unchanged | — |
| `day` | `datetime.day(ts: Int) -> Int` | Go `Day()` / Python `.day` / JS `getUTCDate()` / Elixir `.day` | Unchanged | — |
| `diff_seconds` | `datetime.diff_seconds(a: Int, b: Int) -> Int` | Go `Sub` / Elixir `DateTime.diff` / others use `-` | Remove | R-datetime-03（`a - b`） |
| `format` | `datetime.format(ts: Int, pattern: String) -> String` | Python `strftime` / Elixir `Calendar.strftime` / Go `Format`（example date） | Unchanged | R-datetime-05 |
| `from_parts` | `datetime.from_parts(y: Int, m: Int, d: Int, h: Int, min: Int, s: Int) -> Int` | Go `time.Date` / Python `datetime(...)` / JS `Date.UTC` / Elixir `DateTime.new` | Semantic change (out of range: err; `-> Result[Int, String]`） | R-datetime-07 |
| `from_unix` | `datetime.from_unix(seconds: Int) -> Int` | Elixir `DateTime.from_unix` / Go `time.Unix` / Kotlin `fromEpochSeconds` | Remove | R-datetime-03（the argument itself） |
| `hour` | `datetime.hour(ts: Int) -> Int` | Go `Hour()` / Python `.hour` / JS `getUTCHours()` | Unchanged | — |
| `is_after` | `datetime.is_after(a: Int, b: Int) -> Bool` | Go `After` / Kotlin `>`（Java `isAfter`） / others use `>` | Remove | R-datetime-03（`a > b`） |
| `is_before` | `datetime.is_before(a: Int, b: Int) -> Bool` | Go `Before` / others use `<` | Remove | R-datetime-03（`a < b`） |
| `minute` | `datetime.minute(ts: Int) -> Int` | Go `Minute()` / Python `.minute` / JS `getUTCMinutes()` | Unchanged | — |
| `month` | `datetime.month(ts: Int) -> Int` | Go `Month()` / Python `.month` / JS `getUTCMonth()`（0-based） | Unchanged | — |
| `now` | `effect datetime.now() -> Int` | Python `time.time()` / Go `time.Now().Unix()` / Elixir `System.os_time(:second)` | Unchanged | R-datetime-01（`env.unix_timestamp` retained counterpart） |
| `monotonic_ns` | `effect datetime.monotonic_ns() -> Int` | Python `time.monotonic_ns()` / Rust `Instant::now()` / Elixir `System.monotonic_time` | Unchanged | — |
| `parse_iso` | `datetime.parse_iso(s: String) -> Result[Int, String]` | Python `fromisoformat` / Elixir `DateTime.from_iso8601` / Go `Parse(time.RFC3339, s)` / Kotlin `Instant.parse` | Unchanged | — |
| `second` | `datetime.second(ts: Int) -> Int` | Go `Second()` / Python `.second` / JS `getUTCSeconds()` | Unchanged | — |
| `to_iso` | `datetime.to_iso(ts: Int) -> String` | Python `isoformat` / JS `toISOString` / Elixir `to_iso8601` | Unchanged | — |
| `to_unix` | `datetime.to_unix(ts: Int) -> Int` | Elixir `DateTime.to_unix` / Go `.Unix()` / Python `timestamp()` | Remove | R-datetime-03（the argument itself） |
| `weekday` | `datetime.weekday(ts: Int) -> String` | Go `Weekday()` / Python `weekday()`（number） / Elixir `Date.day_of_week`（number） | Unchanged | R-datetime-04 |
| `year` | `datetime.year(ts: Int) -> Int` | Go `Year()` / Python `.year` / JS `getUTCFullYear()` | Unchanged | — |
| `now_ms` | （Add）`effect datetime.now_ms() -> Int` | JS `Date.now()` / Go `time.Now().UnixMilli()` / Elixir `System.os_time(:millisecond)` / Kotlin `toEpochMilliseconds()` | Add | R-datetime-01 |

<a id="l0-への差し戻し処理済み"></a>

## Returned to L0 (resolved)

- Types of functions accepting quantities of time: retain `Int` (× rejected). Since timestamps are represented as Int Unix seconds (R-datetime-02), keep quantities as `Int` with the unit in the name (`add_seconds`, `sleep_ms`). As in ADR-0001, restrict `Duration` to `fan` budgets and deadlines. Because `Duration` has no extraction function (S7), mixing it into timestamp arithmetic would require adding extraction or having 2 variants of functions by type. Both violate R-L0-01.
