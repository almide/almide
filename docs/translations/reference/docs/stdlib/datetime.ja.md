# datetime

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](datetime.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](datetime.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](datetime.zh-CN.md)



> 原文：[datetime.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/datetime.md)。読者向けの非規範的な固定スナップショットです。自動生成・自動同期の対象ではありません。署名・型・`@since` とコード（コメントを含む）は原文のまま保持し、API 説明の訳を末尾に併記しています。コード例・性能・安全性・ターゲット間の一致は、この翻訳作業では独立実行・検証していません。

> 年の入力範囲と出力の桁幅、任意桁の入力と RFC 3339 の4桁年への言及、parse_iso の短い索引説明と本文のオフセット対応は原文のまま保持しています。規格全体への適合認証やタイムゾーン・夏時間対応を示すものではありません。

日付と時刻。自動インポートされます。時計の読み取りは effect を持ち、変換は純粋です。

### `datetime.now() -> Int` (effect)

現在時刻を Unix タイムスタンプ（秒、UTC）として取得します。**effect fn** です（#1515）。実時間時計の読み取りは非決定的なので、純粋な `fn` からは呼べず（E006）、呼び出し元は `effect fn` でなければなりません。他の effect 呼び出しと同様に `datetime.now()!` と書けます。このエラーにならない組み込み関数ではマーカーは何もせず、マーカーなしの呼び出しも `Int` を返します。`monotonic_ns` も同じ規則に従います。

```almd check
effect fn stamp() -> Int = datetime.now()

effect fn main() -> Unit = {
  let ts = stamp()!
  println(datetime.to_iso(ts))
}
```

### `datetime.from_parts(y: Int, m: Int, d: Int, h: Int, min: Int, s: Int) -> Result[Int, String]`

年、月、日、時、分、秒（UTC）からタイムスタンプを作ります。範囲外のフィールドはその名前を示す `err` で、規則と文面は `parse_iso` と同じです。年0..=9999、月1..=12、日1..=その月の日数、時0..=23、分と秒0..=59。繰り上がりはありません。2月30日は3月2日ではなく `err("day out of range: 30")` です。

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

ISO 8601 の日時をタイムスタンプに解析します。`YYYY-MM-DDTHH:MM:SS` の後ろに `Z`、`+HH:MM`、`-HH:MM`、または何も付けない形式（UTC）です。オフセットを適用した結果は UTC の時点なので、`2024-01-15T10:30:00+09:00` は `2024-01-15T01:30:00Z` になります。各フィールドは任意の桁数の ASCII 数字で、日付3フィールドと時刻3フィールドが必要です。年は `0..=9999`（RFC 3339 の4桁年。より大きい年は `"year out of range: <y>"` となり、時点が折り返すことはありません）、月は `1..=12`、日は `1..=` その月の日数（先発グレゴリオ暦のうるう年規則）、時は `0..=23`、分と秒は `0..=59`、オフセットは `23:59` 以内です。拒否時はフィールド名と見つかった値を示します（`"month out of range: 13"`、`"invalid offset: +24:00"`）。形式不正は `"invalid datetime format"`、`T` がなければ `"expected YYYY-MM-DDTHH:MM:SSZ"` です。

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

strftime 形式のパターンでタイムスタンプを整形します。下記の指定子をゼロ埋めした暦上の値に置き換え、それ以外の文字（認識される指定子が直後にない `%` も含む）はそのままコピーします。`%%` エスケープは**ありません**。

| 指定子 | フィールド | 幅 |
| --------- | --------------- | ----- |
| `%Y` | 年 | 4 |
| `%m` | 月 (01–12) | 2 |
| `%d` | 日 (01–31) | 2 |
| `%H` | 時 (00–23) | 2 |
| `%M` | 分 (00–59) | 2 |
| `%S` | 秒 (00–59) | 2 |

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

出力はネイティブと wasm ターゲットでバイト単位に一致します（契約 C-128）。`%Y` は最低4桁幅でゼロ埋めし、符号もその1桁を占めます。どの年でも `to_iso` と同じ規則です。

### `datetime.to_iso(ts: Int) -> String`

タイムスタンプを ISO 8601 文字列に整形します。

```almd run
fn main() -> Unit = {
  println(datetime.to_iso(1705320000))
}
```
```output
2024-01-15T12:00:00Z
```

### `datetime.year(ts: Int) -> Int`

タイムスタンプから年を取り出します。

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

タイムスタンプから月（1-12）を取り出します。

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

タイムスタンプから月内の日（1-31）を取り出します。

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

タイムスタンプから時（0-23）を取り出します。

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

タイムスタンプから分（0-59）を取り出します。

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

タイムスタンプから秒（0-59）を取り出します。

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

曜日を文字列（Monday-Sunday）で取得します。

暦上の値を取り出す各操作（`year` / `month` / `day` / `weekday`、およびそれらを使う `to_iso` / `format`）は、負のタイムスタンプをその時点が属する日へ床方向に丸めます。`-1` は水曜日の `1969-12-31T23:59:59Z` です。

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

タイムスタンプに n 日を加えます。

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

タイムスタンプに n 時間を加えます。

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

タイムスタンプに n 分を加えます。

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

タイムスタンプに n 秒を加えます。

```almd run
fn main() -> Unit = {
  let ts = datetime.from_parts(2024, 1, 15, 12, 0, 0) ?? 0
  println(datetime.to_iso(datetime.add_seconds(ts, 90)))
}
```
```output
2024-01-15T12:01:30Z
```


<a id="signature-index-18-functions"></a>

## 原文固定スナップショットの API 索引（18関数）

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

## API 説明の訳（上記署名順）

- `datetime.add_days(ts: Int, n: Int) -> Int`：ts + n * 86400。夏時間や暦のロジックはありません。
- `datetime.add_hours(ts: Int, n: Int) -> Int`：ts + n * 3600。n は負でも構いません。
- `datetime.add_minutes(ts: Int, n: Int) -> Int`：ts + n * 60。n は負でも構いません。
- `datetime.add_seconds(ts: Int, n: Int) -> Int`：ts + n。n は負でも構いません。
- `datetime.day(ts: Int) -> Int`：暦上の日1..31（UTC）。
- `datetime.format(ts: Int, pattern: String) -> String`：%Y %m %d %H %M %S を置換します（UTC）。%% エスケープはありません。
- `datetime.from_parts(y: Int, m: Int, d: Int, h: Int, min: Int, s: Int) -> Result[Int, String]`：UTC のエポック秒。parse_iso と同様、範囲外のフィールドは err。
- `datetime.hour(ts: Int) -> Int`：UTC の時0..23。
- `datetime.minute(ts: Int) -> Int`：UTC の分0..59。
- `datetime.month(ts: Int) -> Int`：UTC の月1..12。
- `effect datetime.now() -> Int`：実時間時計の Unix 秒（単調ではありません）。
- `effect datetime.monotonic_ns() -> Int`：初回呼び出しからの単調な ns。差分を比較してください。
- `effect datetime.now_ms() -> Int`：実時間時計の Unix エポックからのミリ秒（単調ではありません）。
- `datetime.parse_iso(s: String) -> Result[Int, String]`：YYYY-MM-DDTHH:MM:SSZ のエポック秒。年0..=9999、小数部分は err。
- `datetime.second(ts: Int) -> Int`：UTC の秒0..59。
- `datetime.to_iso(ts: Int) -> String`：UTC の YYYY-MM-DDTHH:MM:SSZ。
- `datetime.weekday(ts: Int) -> String`：UTC の英語の曜日名。0 は Thursday。
- `datetime.year(ts: Int) -> Int`：UTC の暦年。
