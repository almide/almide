# datetime

Date and time. Auto-imported; clock reads are effectful, conversions are pure.

### `datetime.now() -> Int` (effect)

Get the current time as a Unix timestamp (seconds, UTC). An **effect fn**
(#1515): a wall-clock read is nondeterministic, so a pure `fn` cannot call it
(E006) — the caller must be an `effect fn`. Like other effect calls, it
may be written `datetime.now()!`; for this never-error intrinsic the marker
is a no-op and the bare call also yields `Int`. `monotonic_ns` follows the same rule.

```almd check
effect fn stamp() -> Int = datetime.now()

effect fn main() -> Unit = {
  let ts = stamp()!
  println(datetime.to_iso(ts))
}
```

### `datetime.from_parts(y: Int, m: Int, d: Int, h: Int, min: Int, s: Int) -> Result[Int, String]`

Create a timestamp from year, month, day, hour, minute, second (UTC). A field out of range is an `err` naming it, with the same rules and text as `parse_iso`: year 0..=9999, month 1..=12, day 1..=days in that month, hour 0..=23, minute and second 0..=59. Nothing rolls over: February 30th is `err("day out of range: 30")`, not March 2nd.

```almd run
fn main() -> Unit = {
  println(int.to_string(datetime.from_parts(2024, 1, 15, 12, 0, 0) ?? 0))
  match datetime.from_parts(2026, 2, 30, 0, 0, 0) {
    ok(ts) => println(datetime.to_iso(ts)),
    err(e) => println(e),
  }
}
```
```output
1705320000
day out of range: 30
```

### `datetime.parse_iso(s: String) -> Result[Int, String]`

Parse an ISO 8601 date-time into a timestamp: `YYYY-MM-DDTHH:MM:SS` followed
by `Z`, `+HH:MM`, `-HH:MM` or nothing (UTC). The offset is applied — the
result is the UTC instant, so `2024-01-15T10:30:00+09:00` is
`2024-01-15T01:30:00Z`. Every field is ASCII digits of any width; exactly three
date and three time fields; year `0..=9999` (RFC 3339's four-digit year — a
larger year is `"year out of range: <y>"`, never a wrapped instant), month `1..=12`, day `1..=` the month's length
(proleptic Gregorian leap rule), hour `0..=23`, minute and second `0..=59`, and
the offset within `23:59`. A refusal names the field and the value it saw
(`"month out of range: 13"`, `"invalid offset: +24:00"`); a malformed string is
`"invalid datetime format"`, and one without a `T` is
`"expected YYYY-MM-DDTHH:MM:SSZ"`.

```almd check
fn show(r: Result[Int, String]) -> String = match r {
  ok(n) => "ok(${n})",
  err(e) => "err(\"${e}\")",
}

fn main() -> Unit = {
  println(show(datetime.parse_iso("2024-01-15T12:00:00Z")))
  println(show(datetime.parse_iso("not a date")))
}
```

### `datetime.format(ts: Int, pattern: String) -> String`

Format a timestamp using a strftime-style pattern. The following specifiers are
substituted with the zero-padded civil fields; every other character (including a
`%` that is not immediately followed by a recognized specifier) is copied through
verbatim. There is **no** `%%` escape.

| Specifier | Field           | Width |
| --------- | --------------- | ----- |
| `%Y`      | year            | 4     |
| `%m`      | month (01–12)   | 2     |
| `%d`      | day (01–31)     | 2     |
| `%H`      | hour (00–23)    | 2     |
| `%M`      | minute (00–59)  | 2     |
| `%S`      | second (00–59)  | 2     |

```almd run
fn main() -> Unit = {
  let ts = datetime.from_parts(2024, 1, 15, 12, 0, 0) ?? 0
  println(datetime.format(ts, "%Y-%m-%d"))
  println(datetime.format(ts, "%Y-%m-%dT%H:%M:%SZ"))
}
```
```output
2024-01-15
2024-01-15T12:00:00Z
```

The output is byte-identical on the native and wasm targets (contract C-128).
`%Y` is at least four columns, zero-padded, the sign taking one of them — the
same rule as `to_iso`, for any year.

### `datetime.to_iso(ts: Int) -> String`

Format a timestamp as ISO 8601 string.

```almd run
fn main() -> Unit = {
  println(datetime.to_iso(1705320000))
}
```
```output
2024-01-15T12:00:00Z
```

### `datetime.year(ts: Int) -> Int`

Extract the year from a timestamp.

```almd run
fn main() -> Unit = {
  let ts = datetime.from_parts(2024, 1, 15, 12, 30, 45) ?? 0
  println(int.to_string(datetime.year(ts)))
}
```
```output
2024
```

### `datetime.month(ts: Int) -> Int`

Extract the month (1-12) from a timestamp.

```almd run
fn main() -> Unit = {
  let ts = datetime.from_parts(2024, 1, 15, 12, 30, 45) ?? 0
  println(int.to_string(datetime.month(ts)))
}
```
```output
1
```

### `datetime.day(ts: Int) -> Int`

Extract the day of month (1-31) from a timestamp.

```almd run
fn main() -> Unit = {
  let ts = datetime.from_parts(2024, 1, 15, 12, 30, 45) ?? 0
  println(int.to_string(datetime.day(ts)))
}
```
```output
15
```

### `datetime.hour(ts: Int) -> Int`

Extract the hour (0-23) from a timestamp.

```almd run
fn main() -> Unit = {
  let ts = datetime.from_parts(2024, 1, 15, 12, 30, 45) ?? 0
  println(int.to_string(datetime.hour(ts)))
}
```
```output
12
```

### `datetime.minute(ts: Int) -> Int`

Extract the minute (0-59) from a timestamp.

```almd run
fn main() -> Unit = {
  let ts = datetime.from_parts(2024, 1, 15, 12, 30, 45) ?? 0
  println(int.to_string(datetime.minute(ts)))
}
```
```output
30
```

### `datetime.second(ts: Int) -> Int`

Extract the second (0-59) from a timestamp.

```almd run
fn main() -> Unit = {
  let ts = datetime.from_parts(2024, 1, 15, 12, 30, 45) ?? 0
  println(int.to_string(datetime.second(ts)))
}
```
```output
45
```

### `datetime.weekday(ts: Int) -> String`

Get the day of week as a string (Monday-Sunday).

Every civil extractor (`year` / `month` / `day` / `weekday`, and `to_iso` /
`format` over them) floors a negative timestamp to the day it belongs to:
`-1` is `1969-12-31T23:59:59Z`, a Wednesday.

```almd run
fn main() -> Unit = {
  let ts = datetime.from_parts(2024, 1, 15, 12, 30, 45) ?? 0
  println(datetime.weekday(ts))
}
```
```output
Monday
```

### `datetime.add_days(ts: Int, n: Int) -> Int`

Add n days to a timestamp.

```almd run
fn main() -> Unit = {
  let ts = datetime.from_parts(2024, 1, 15, 12, 0, 0) ?? 0
  println(datetime.to_iso(datetime.add_days(ts, 7))) // one week later
}
```
```output
2024-01-22T12:00:00Z
```

### `datetime.add_hours(ts: Int, n: Int) -> Int`

Add n hours to a timestamp.

```almd run
fn main() -> Unit = {
  let ts = datetime.from_parts(2024, 1, 15, 12, 0, 0) ?? 0
  println(datetime.to_iso(datetime.add_hours(ts, 3)))
}
```
```output
2024-01-15T15:00:00Z
```

### `datetime.add_minutes(ts: Int, n: Int) -> Int`

Add n minutes to a timestamp.

```almd run
fn main() -> Unit = {
  let ts = datetime.from_parts(2024, 1, 15, 12, 0, 0) ?? 0
  println(datetime.to_iso(datetime.add_minutes(ts, 30)))
}
```
```output
2024-01-15T12:30:00Z
```

### `datetime.add_seconds(ts: Int, n: Int) -> Int`

Add n seconds to a timestamp.

```almd run
fn main() -> Unit = {
  let ts = datetime.from_parts(2024, 1, 15, 12, 0, 0) ?? 0
  println(datetime.to_iso(datetime.add_seconds(ts, 90)))
}
```
```output
2024-01-15T12:01:30Z
```

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (18 functions)

```
// ts + n * 86400; no DST or calendar logic.
// @since 0.6.0 or earlier
datetime.add_days(ts: Int, n: Int) -> Int

// ts + n * 3600; n may be negative.
// @since 0.6.0 or earlier
datetime.add_hours(ts: Int, n: Int) -> Int

// ts + n * 60; n may be negative.
// @since 0.6.0 or earlier
datetime.add_minutes(ts: Int, n: Int) -> Int

// ts + n; n may be negative.
// @since 0.6.0 or earlier
datetime.add_seconds(ts: Int, n: Int) -> Int

// Calendar day 1..31 (UTC).
// @since 0.6.0 or earlier
datetime.day(ts: Int) -> Int

// Replaces %Y %m %d %H %M %S (UTC); no %% escape.
// @since 0.6.0 or earlier
datetime.format(ts: Int, pattern: String) -> String

// UTC epoch secs; a field out of range is err, as in parse_iso.
// @since 0.6.0 or earlier
datetime.from_parts(y: Int, m: Int, d: Int, h: Int, min: Int, s: Int) -> Result[Int, String]

// UTC hour of day, 0..23.
// @since 0.6.0 or earlier
datetime.hour(ts: Int) -> Int

// UTC minute, 0..59.
// @since 0.6.0 or earlier
datetime.minute(ts: Int) -> Int

// UTC month, 1..12.
// @since 0.6.0 or earlier
datetime.month(ts: Int) -> Int

// Wall-clock Unix seconds (not monotonic).
// @since 0.6.0 or earlier
effect datetime.now() -> Int

// Monotonic ns since first call; compare differences.
// @since 0.15.0 or earlier
effect datetime.monotonic_ns() -> Int

// Wall-clock milliseconds since the Unix epoch (not monotonic).
// @since unreleased
effect datetime.now_ms() -> Int

// Epoch secs of YYYY-MM-DDTHH:MM:SSZ, year 0..=9999; fractions err.
// @since 0.6.0 or earlier
datetime.parse_iso(s: String) -> Result[Int, String]

// UTC second, 0..59.
// @since 0.6.0 or earlier
datetime.second(ts: Int) -> Int

// YYYY-MM-DDTHH:MM:SSZ in UTC.
// @since 0.6.0 or earlier
datetime.to_iso(ts: Int) -> String

// English day name in UTC; 0 is Thursday.
// @since 0.6.0 or earlier
datetime.weekday(ts: Int) -> String

// Calendar year in UTC.
// @since 0.6.0 or earlier
datetime.year(ts: Int) -> Int
```

<!-- END GENERATED SIGNATURE INDEX -->
