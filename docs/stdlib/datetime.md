# datetime

Date and time. import datetime, effect.

### `datetime.now() -> Int`

Get the current time as a Unix timestamp (seconds, UTC).

```almd
let ts = datetime.now()
```

### `datetime.from_parts(y: Int, m: Int, d: Int, h: Int, min: Int, s: Int) -> Int`

Create a timestamp from year, month, day, hour, minute, second (UTC).

```almd
datetime.from_parts(2024, 1, 15, 12, 0, 0)
```

### `datetime.parse_iso(s: String) -> Result[Int, String]`

Parse an ISO 8601 date string into a timestamp.

```almd
datetime.parse_iso("2024-01-15T12:00:00Z") // => ok(1705320000)
```

### `datetime.from_unix(seconds: Int) -> Int`

Convert a Unix timestamp (identity function for documentation clarity).

```almd
datetime.from_unix(1705320000)
```

### `datetime.format(ts: Int, pattern: String) -> String`

Format a timestamp using a pattern string.

```almd
datetime.format(ts, "%Y-%m-%d") // => "2024-01-15"
```

### `datetime.to_iso(ts: Int) -> String`

Format a timestamp as ISO 8601 string.

```almd
datetime.to_iso(1705320000) // => "2024-01-15T12:00:00Z"
```

### `datetime.to_unix(ts: Int) -> Int`

Get the Unix timestamp value (identity function).

```almd
datetime.to_unix(ts) // => 1705320000
```

### `datetime.year(ts: Int) -> Int`

Extract the year from a timestamp.

```almd
datetime.year(ts) // => 2024
```

### `datetime.month(ts: Int) -> Int`

Extract the month (1-12) from a timestamp.

```almd
datetime.month(ts) // => 1
```

### `datetime.day(ts: Int) -> Int`

Extract the day of month (1-31) from a timestamp.

```almd
datetime.day(ts) // => 15
```

### `datetime.hour(ts: Int) -> Int`

Extract the hour (0-23) from a timestamp.

```almd
datetime.hour(ts) // => 12
```

### `datetime.minute(ts: Int) -> Int`

Extract the minute (0-59) from a timestamp.

```almd
datetime.minute(ts) // => 30
```

### `datetime.second(ts: Int) -> Int`

Extract the second (0-59) from a timestamp.

```almd
datetime.second(ts) // => 45
```

### `datetime.weekday(ts: Int) -> String`

Get the day of week as a string (Monday-Sunday).

```almd
datetime.weekday(ts) // => "Monday"
```

### `datetime.add_days(ts: Int, n: Int) -> Int`

Add n days to a timestamp.

```almd
datetime.add_days(ts, 7) // one week later
```

### `datetime.add_hours(ts: Int, n: Int) -> Int`

Add n hours to a timestamp.

```almd
datetime.add_hours(ts, 3)
```

### `datetime.add_minutes(ts: Int, n: Int) -> Int`

Add n minutes to a timestamp.

```almd
datetime.add_minutes(ts, 30)
```

### `datetime.add_seconds(ts: Int, n: Int) -> Int`

Add n seconds to a timestamp.

```almd
datetime.add_seconds(ts, 90)
```

### `datetime.diff_seconds(a: Int, b: Int) -> Int`

Compute the difference in seconds between two timestamps.

```almd
datetime.diff_seconds(later, earlier) // => 3600
```

### `datetime.is_before(a: Int, b: Int) -> Bool`

Check if timestamp a is before timestamp b.

```almd
datetime.is_before(earlier, later) // => true
```

### `datetime.is_after(a: Int, b: Int) -> Bool`

Check if timestamp a is after timestamp b.

```almd
datetime.is_after(later, earlier) // => true
```
