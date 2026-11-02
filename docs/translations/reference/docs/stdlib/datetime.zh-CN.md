# datetime

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](datetime.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](datetime.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](datetime.zh-CN.md)



> 原文：[datetime.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/datetime.md)。这是面向读者的非规范性固定快照，不参与自动生成或自动同步。签名、类型、`@since` 及代码（包括注释）保持原样，API 说明译文附在末尾。本次翻译未独立运行或验证示例、性能、安全性或跨目标一致性。

> 输入年份范围与输出宽度、任意位数输入与 RFC 3339 四位年份的表述，以及 parse_iso 简短索引与正文偏移支持的差异均按原文保留。这不是对整套标准的合规认证，也不表示支持时区或夏令时。

日期与时间。自动导入；读取时钟带有 effect，转换是纯操作。

### `datetime.now() -> Int` (effect)

获取当前时间，表示为 Unix 时间戳（秒，UTC）。这是 **effect fn**（#1515）：墙上时钟的读取具有非确定性，因此纯 `fn` 不能调用它（E006），调用者必须是 `effect fn`。与其他 effect 调用一样，可以写作 `datetime.now()!`；对于这个不会出错的内建操作，该标记不做任何事，不带标记的调用也返回 `Int`。`monotonic_ns` 遵循同一规则。

```almd check
effect fn stamp() -> Int = datetime.now()

effect fn main() -> Unit = {
  let ts = stamp()!
  println(datetime.to_iso(ts))
}
```

### `datetime.from_parts(y: Int, m: Int, d: Int, h: Int, min: Int, s: Int) -> Result[Int, String]`

根据年、月、日、时、分、秒（UTC）创建时间戳。字段越界返回点名该字段的 `err`，规则与文本同 `parse_iso`：年0..=9999，月1..=12，日1..=该月天数，时0..=23，分和秒0..=59。不发生进位：2月30日得到 `err("day out of range: 30")`，而不是3月2日。

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

将 ISO 8601 日期时间解析为时间戳：`YYYY-MM-DDTHH:MM:SS` 后接 `Z`、`+HH:MM`、`-HH:MM` 或不接内容（UTC）。会应用偏移量，结果为 UTC 时刻，因此 `2024-01-15T10:30:00+09:00` 是 `2024-01-15T01:30:00Z`。每个字段为任意位数的 ASCII 数字，日期和时间各恰有三个字段；年 `0..=9999`（RFC 3339 的四位年份；更大的年得到 `"year out of range: <y>"`，绝不回绕成其他时刻），月 `1..=12`，日 `1..=` 该月天数（向前延伸的格里高利历闰年规则），时 `0..=23`，分和秒 `0..=59`，偏移量在 `23:59` 以内。拒绝时指出字段和读到的值（`"month out of range: 13"`、`"invalid offset: +24:00"`）；格式错误是 `"invalid datetime format"`，缺少 `T` 则为 `"expected YYYY-MM-DDTHH:MM:SSZ"`。

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

用 strftime 风格的模式格式化时间戳。以下指定符会替换为补零的日历字段；其他字符（包括后面没有紧接已识别指定符的 `%`）全部原样复制。**没有** `%%` 转义。

| 指定符 | 字段 | 宽度 |
| --------- | --------------- | ----- |
| `%Y` | 年 | 4 |
| `%m` | 月 (01–12) | 2 |
| `%d` | 日 (01–31) | 2 |
| `%H` | 时 (00–23) | 2 |
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

原生与 wasm 目标的输出逐字节一致（契约 C-128）。`%Y` 至少占四个字符位置，补零，符号占其中一位；对任何年份都与 `to_iso` 规则相同。

### `datetime.to_iso(ts: Int) -> String`

将时间戳格式化为 ISO 8601 字符串。

```almd run
fn main() -> Unit = {
  println(datetime.to_iso(1705320000))
}
```
```output
2024-01-15T12:00:00Z
```

### `datetime.year(ts: Int) -> Int`

从时间戳提取年。

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

从时间戳提取月（1-12）。

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

从时间戳提取月内的日（1-31）。

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

从时间戳提取时（0-23）。

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

从时间戳提取分（0-59）。

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

从时间戳提取秒（0-59）。

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

获取星期名称字符串（Monday-Sunday）。

每个日历字段提取操作（`year` / `month` / `day` / `weekday`，以及基于它们的 `to_iso` / `format`）都会将负时间戳向下取整到其所属的日期：`-1` 是星期三的 `1969-12-31T23:59:59Z`。

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

给时间戳加 n 天。

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

给时间戳加 n 小时。

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

给时间戳加 n 分钟。

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

给时间戳加 n 秒。

```almd run
fn main() -> Unit = {
  let ts = datetime.from_parts(2024, 1, 15, 12, 0, 0) ?? 0
  println(datetime.to_iso(datetime.add_seconds(ts, 90)))
}
```
```output
2024-01-15T12:01:30Z
```


## 原文固定快照 API 索引（18个函数）

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

## API 说明译文（按上述签名顺序）

- `datetime.add_days(ts: Int, n: Int) -> Int`：ts + n * 86400；没有夏令时或日历逻辑。
- `datetime.add_hours(ts: Int, n: Int) -> Int`：ts + n * 3600；n 可以为负。
- `datetime.add_minutes(ts: Int, n: Int) -> Int`：ts + n * 60；n 可以为负。
- `datetime.add_seconds(ts: Int, n: Int) -> Int`：ts + n；n 可以为负。
- `datetime.day(ts: Int) -> Int`：日历日1..31（UTC）。
- `datetime.format(ts: Int, pattern: String) -> String`：替换 %Y %m %d %H %M %S（UTC）；没有 %% 转义。
- `datetime.from_parts(y: Int, m: Int, d: Int, h: Int, min: Int, s: Int) -> Result[Int, String]`：UTC 纪元秒；与 parse_iso 一样，字段越界为 err。
- `datetime.hour(ts: Int) -> Int`：UTC 小时0..23。
- `datetime.minute(ts: Int) -> Int`：UTC 分钟0..59。
- `datetime.month(ts: Int) -> Int`：UTC 月份1..12。
- `effect datetime.now() -> Int`：墙上时钟的 Unix 秒（非单调）。
- `effect datetime.monotonic_ns() -> Int`：自首次调用起的单调纳秒；比较差值。
- `effect datetime.now_ms() -> Int`：墙上时钟自 Unix 纪元起的毫秒（非单调）。
- `datetime.parse_iso(s: String) -> Result[Int, String]`：YYYY-MM-DDTHH:MM:SSZ 的纪元秒，年0..=9999；小数部分为 err。
- `datetime.second(ts: Int) -> Int`：UTC 秒0..59。
- `datetime.to_iso(ts: Int) -> String`：UTC 的 YYYY-MM-DDTHH:MM:SSZ。
- `datetime.weekday(ts: Int) -> String`：UTC 的英语星期名；0为 Thursday。
- `datetime.year(ts: Int) -> Int`：UTC 日历年。
