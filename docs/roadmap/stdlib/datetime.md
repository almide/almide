# stdlib: datetime [Tier 1]

日時操作。これがないと実用的なアプリケーションが書けない。現在 `time` モジュール（bundled .almd）に基本的な Duration/Timestamp があるが、日時パース・フォーマット・タイムゾーン・比較がない。

## 他言語比較

### 生成

| 操作 | Go (`time`) | Python (`datetime`) | Rust (`chrono`) | Deno (`Temporal`) |
|------|------------|--------------------|--------------------|-------------------|
| 現在時刻 | `time.Now()` | `datetime.now()` | `Utc::now()` / `Local::now()` | `Temporal.Now.instant()` |
| 部品から | `time.Date(y,m,d,h,m,s,ns,loc)` | `datetime(y,m,d,h,m,s)` | `NaiveDate::from_ymd(y,m,d)` | `Temporal.PlainDateTime.from({})` |
| パース | `time.Parse(layout, s)` | `datetime.strptime(s, fmt)` | `NaiveDateTime::parse_from_str(s, fmt)` | `Temporal.Instant.from(s)` |
| Unix timestamp | `time.Unix(sec, nsec)` | `datetime.fromtimestamp(ts)` | `DateTime::from_timestamp(sec, nsec)` | `Temporal.Instant.fromEpochSeconds(s)` |

### フォーマット

| 操作 | Go | Python | Rust | Deno |
|------|-----|--------|------|------|
| 文字列化 | `t.Format("2006-01-02")` | `dt.strftime("%Y-%m-%d")` | `dt.format("%Y-%m-%d")` | `dt.toString()` |
| ISO 8601 | `t.Format(time.RFC3339)` | `dt.isoformat()` | `dt.to_rfc3339()` | `instant.toString()` |

### 部品アクセス

| 要素 | Go | Python | Rust | Deno |
|------|-----|--------|------|------|
| year | `t.Year()` | `dt.year` | `dt.year()` | `dt.year` |
| month | `t.Month()` | `dt.month` | `dt.month()` | `dt.month` |
| day | `t.Day()` | `dt.day` | `dt.day()` | `dt.day` |
| hour | `t.Hour()` | `dt.hour` | `dt.hour()` | `dt.hour` |
| minute | `t.Minute()` | `dt.minute` | `dt.minute()` | `dt.minute` |
| second | `t.Second()` | `dt.second` | `dt.second()` | `dt.second` |
| weekday | `t.Weekday()` | `dt.weekday()` | `dt.weekday()` | `dt.dayOfWeek` |

### 算術

| 操作 | Go | Python | Rust | Deno |
|------|-----|--------|------|------|
| 加算 | `t.Add(duration)` | `dt + timedelta(days=1)` | `dt + Duration::days(1)` | `dt.add({days: 1})` |
| 差分 | `t.Sub(other)` | `dt - other` → `timedelta` | `dt - other` → `Duration` | `dt.until(other)` |
| 比較 | `t.Before(other)` | `dt < other` | `dt < other` | `Temporal.Instant.compare(a, b)` |

### タイムゾーン

| 操作 | Go | Python | Rust | Deno |
|------|-----|--------|------|------|
| UTC | `time.UTC` | `timezone.utc` | `Utc` | `"UTC"` |
| ローカル | `time.Local` | `None` (naive) | `Local` | `Temporal.Now.timeZoneId()` |
| 変換 | `t.In(loc)` | `dt.astimezone(tz)` | `dt.with_timezone(&tz)` | `instant.toZonedDateTimeISO(tz)` |
| TZ 指定 | `time.LoadLocation("Asia/Tokyo")` | `zoneinfo.ZoneInfo("Asia/Tokyo")` | `chrono_tz::Asia::Tokyo` | `"Asia/Tokyo"` |

### Duration

| 操作 | Go | Python | Rust | Deno |
|------|-----|--------|------|------|
| 生成 | `time.Duration(n)` | `timedelta(days=1)` | `Duration::days(1)` | `Temporal.Duration.from({days: 1})` |
| 秒 | `d.Seconds()` | `td.total_seconds()` | `d.num_seconds()` | `d.total({unit: "second"})` |
| ミリ秒 | `d.Milliseconds()` | `td.total_seconds()*1000` | `d.num_milliseconds()` | `d.total({unit: "millisecond"})` |

## 追加候補 (~25 関数)

### P0 (生成・パース)
- `datetime.now() -> DateTime` — 現在時刻 (UTC)
- `datetime.now_local() -> DateTime` — 現在時刻 (ローカル)
- `datetime.from_parts(y, m, d, h, min, s) -> DateTime`
- `datetime.parse(s, format) -> Result[DateTime, String]` — フォーマット指定パース
- `datetime.parse_iso(s) -> Result[DateTime, String]` — ISO 8601 パース
- `datetime.from_unix(seconds) -> DateTime` — Unix timestamp から

### P0 (フォーマット)
- `datetime.format(dt, pattern) -> String` — フォーマット
- `datetime.to_iso(dt) -> String` — ISO 8601
- `datetime.to_unix(dt) -> Int` — Unix timestamp

### P0 (部品)
- `datetime.year(dt) -> Int`
- `datetime.month(dt) -> Int`
- `datetime.day(dt) -> Int`
- `datetime.hour(dt) -> Int`
- `datetime.minute(dt) -> Int`
- `datetime.second(dt) -> Int`
- `datetime.weekday(dt) -> String` — "Monday" 等

### P1 (算術)
- `datetime.add_days(dt, n) -> DateTime`
- `datetime.add_hours(dt, n) -> DateTime`
- `datetime.add_minutes(dt, n) -> DateTime`
- `datetime.add_seconds(dt, n) -> DateTime`
- `datetime.diff_seconds(a, b) -> Int` — 差分（秒）
- `datetime.is_before?(a, b) -> Bool`
- `datetime.is_after?(a, b) -> Bool`

### P2 (タイムゾーン)
- `datetime.in_timezone(dt, tz) -> DateTime`
- `datetime.timezone(dt) -> String`

## 実装戦略

TOML + runtime。Rust: `chrono` crate。TS: `Temporal` API (or `Date` + `Intl`)。
DateTime は内部的には Unix timestamp (i64 ミリ秒) として表現し、部品アクセスは計算で導出。
タイムゾーンは Phase 2 以降（tz database が必要）。
