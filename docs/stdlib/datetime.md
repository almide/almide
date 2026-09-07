# datetime

Date and time. import datetime, effect.

### `datetime.now() -> Int` (effect)

Get the current time as a Unix timestamp (seconds, UTC). An **effect fn**
(#1515): a wall-clock read is nondeterministic, so a pure `fn` cannot call it
(E006) — the caller must be an `effect fn`, where the spelling is unchanged
(the raw Int comes back, no `!`). `monotonic_ns` follows the same rule.

```almd check
effect fn stamp() -> Int = datetime.now()

effect fn main() -> Unit = {
  let ts = stamp()!
  println(datetime.to_iso(ts))
}
```

### `datetime.from_parts(y: Int, m: Int, d: Int, h: Int, min: Int, s: Int) -> Int`

Create a timestamp from year, month, day, hour, minute, second (UTC).

```almd run
fn main() -> Unit = {
  println(int.to_string(datetime.from_parts(2024, 1, 15, 12, 0, 0)))
}
```
```output
1705320000
```

### `datetime.parse_iso(s: String) -> Result[Int, String]`

Parse an ISO 8601 date string into a timestamp.

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

### `datetime.from_unix(seconds: Int) -> Int`

Convert a Unix timestamp (identity function for documentation clarity).

```almd run
fn main() -> Unit = {
  println(int.to_string(datetime.from_unix(1705320000)))
}
```
```output
1705320000
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
  let ts = datetime.from_parts(2024, 1, 15, 12, 0, 0)
  println(datetime.format(ts, "%Y-%m-%d"))
  println(datetime.format(ts, "%Y-%m-%dT%H:%M:%SZ"))
}
```
```output
2024-01-15
2024-01-15T12:00:00Z
```

The output is byte-identical on the native and wasm targets (contract C-128), for
years `0..9999`.

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

### `datetime.to_unix(ts: Int) -> Int`

Get the Unix timestamp value (identity function).

```almd run
fn main() -> Unit = {
  let ts = datetime.from_parts(2024, 1, 15, 12, 0, 0)
  println(int.to_string(datetime.to_unix(ts)))
}
```
```output
1705320000
```

### `datetime.year(ts: Int) -> Int`

Extract the year from a timestamp.

```almd run
fn main() -> Unit = {
  let ts = datetime.from_parts(2024, 1, 15, 12, 30, 45)
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
  let ts = datetime.from_parts(2024, 1, 15, 12, 30, 45)
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
  let ts = datetime.from_parts(2024, 1, 15, 12, 30, 45)
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
  let ts = datetime.from_parts(2024, 1, 15, 12, 30, 45)
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
  let ts = datetime.from_parts(2024, 1, 15, 12, 30, 45)
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
  let ts = datetime.from_parts(2024, 1, 15, 12, 30, 45)
  println(int.to_string(datetime.second(ts)))
}
```
```output
45
```

### `datetime.weekday(ts: Int) -> String`

Get the day of week as a string (Monday-Sunday).

```almd run
fn main() -> Unit = {
  let ts = datetime.from_parts(2024, 1, 15, 12, 30, 45)
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
  let ts = datetime.from_parts(2024, 1, 15, 12, 0, 0)
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
  let ts = datetime.from_parts(2024, 1, 15, 12, 0, 0)
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
  let ts = datetime.from_parts(2024, 1, 15, 12, 0, 0)
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
  let ts = datetime.from_parts(2024, 1, 15, 12, 0, 0)
  println(datetime.to_iso(datetime.add_seconds(ts, 90)))
}
```
```output
2024-01-15T12:01:30Z
```

### `datetime.diff_seconds(a: Int, b: Int) -> Int`

Compute the difference in seconds between two timestamps.

```almd run
fn main() -> Unit = {
  let earlier = datetime.from_parts(2024, 1, 15, 12, 0, 0)
  let later = datetime.add_hours(earlier, 1)
  println(int.to_string(datetime.diff_seconds(later, earlier)))
}
```
```output
3600
```

### `datetime.is_before(a: Int, b: Int) -> Bool`

Check if timestamp a is before timestamp b.

```almd run
fn main() -> Unit = {
  let earlier = datetime.from_parts(2024, 1, 15, 12, 0, 0)
  let later = datetime.add_hours(earlier, 1)
  println("${datetime.is_before(earlier, later)}")
}
```
```output
true
```

### `datetime.is_after(a: Int, b: Int) -> Bool`

Check if timestamp a is after timestamp b.

```almd run
fn main() -> Unit = {
  let earlier = datetime.from_parts(2024, 1, 15, 12, 0, 0)
  let later = datetime.add_hours(earlier, 1)
  println("${datetime.is_after(later, earlier)}")
}
```
```output
true
```

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (22 functions)

```
datetime.add_days(ts: Int, n: Int) -> Int
datetime.add_hours(ts: Int, n: Int) -> Int
datetime.add_minutes(ts: Int, n: Int) -> Int
datetime.add_seconds(ts: Int, n: Int) -> Int
datetime.day(ts: Int) -> Int
datetime.diff_seconds(a: Int, b: Int) -> Int
datetime.format(ts: Int, pattern: String) -> String
datetime.from_parts(y: Int, m: Int, d: Int, h: Int, min: Int, s: Int) -> Int
datetime.from_unix(seconds: Int) -> Int
datetime.hour(ts: Int) -> Int
datetime.is_after(a: Int, b: Int) -> Bool
datetime.is_before(a: Int, b: Int) -> Bool
datetime.minute(ts: Int) -> Int
datetime.month(ts: Int) -> Int
effect datetime.now() -> Int
effect datetime.monotonic_ns() -> Int
datetime.parse_iso(s: String) -> Result[Int, String]
datetime.second(ts: Int) -> Int
datetime.to_iso(ts: Int) -> String
datetime.to_unix(ts: Int) -> Int
datetime.weekday(ts: Int) -> String
datetime.year(ts: Int) -> Int
```

<!-- END GENERATED SIGNATURE INDEX -->
