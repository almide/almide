> 说明（原本索引元数据的非活动副本）：标准库重设计 L2/L3 — datetime：保留 Int Unix 秒表示；新增 now_ms 作为唯一毫秒时钟；移除恒等函数和运算符包装
<a id="l2l3-datetime"></a>

# L2/L3: datetime

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-datetime.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-datetime.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-datetime.zh-CN.md)

> 非规范性历史重设计参照版，固定原本权威且不变。不自动同步、未重新获取来源、独立probe或核实现行实现。(F)和实测指原作者当时核查。保留旧from_parts进位行为与后段Result提案以及表中旧签名，不静默改成现行API。原文宽泛的秒输入/仅now_ms表述与monotonic_ns并存，不推广为单位保证。Duration提取和类型决定属于该时期记录，不擅自与后续ADR统一。保留伪代码中的単位意为单位。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-datetime.md)

> 最后更新：2026-10-21
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)・[L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)（R-L1-25：时钟归 `datetime`；时区不纳入标准）
> 相关：[L2-duration](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-duration.md)・[L2-compute](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-compute.md)・L2-env
> 来源：Rust https://doc.rust-lang.org/std/time/struct.SystemTime.html ・Go https://pkg.go.dev/time ・Python https://docs.python.org/3/library/datetime.html 和 https://docs.python.org/3/library/time.html ・TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Date ・Gleam https://hexdocs.pm/gleam_time/（官方 package，非标准库）・Elixir https://hexdocs.pm/elixir/DateTime.html ・OCaml https://ocaml.org/manual/latest/api/Unix.html ・Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.time/-instant/ ・Swift https://developer.apple.com/documentation/foundation/date

各语言时间功能归属见 [facts/categories.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/facts/categories.md) 1.G・3.3 节（Kotlin `kotlin.time.Instant` 与 WASI clocks 已核实 (F)）。

实测（`almide 0.68.0 dev 437b8f127`，native 与 wasm 相同）：`datetime.weekday(0)` 为 `"Thursday"`。`datetime.from_parts(2024, 13, 1, 0, 0, 0)` 为 `2025-01-01T00:00:00Z`（超出月份进位）。`datetime.format(0, "%Y %%")` 为 `"1970 %%"`（不将 `%%` 变成 `%`）。`parse_iso("2024-01-01T09:00:00+09:00")` 为 `1704067200`（正确减去偏移），小数秒为 err。`datetime.from_unix(5)` 为 `5`（不做任何事的函数）。

<a id="l2-役割の比較"></a>

## L2：角色比较

| 角色 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 时刻表示 | `Int`（Unix 秒） | `SystemTime` | `time.Time` | `datetime` / `float` 秒 | `Date`（ms） | pkg `Timestamp` | `DateTime` | `float`（`Unix.time`） | `Instant` | `Date` |
| 当前时刻（秒） | `now()` 与 `env.unix_timestamp()` | `SystemTime::now()` | `time.Now().Unix()` | `time.time()` | — | pkg | `System.os_time(:second)` | `Unix.time` | `Clock.System.now()` | `Date()` |
| 当前时刻（ms） | `env.millis()` | `duration_since(UNIX_EPOCH)` | `UnixMilli()` | `time.time_ns()` | `Date.now()` | pkg | `System.os_time(:millisecond)` | — | `toEpochMilliseconds()` | `timeIntervalSince1970` |
| 单调时钟 | `monotonic_ns()` | `Instant::now()` | `time.Now()`（带单调部分） | `time.monotonic_ns()` | `performance.now()` | pkg | `System.monotonic_time` | — | `TimeSource.Monotonic` | `ContinuousClock` |
| 从组件构造 | `from_parts` | — | `time.Date` | `datetime(...)` | `Date.UTC` | pkg | `DateTime.new` | `Unix.mktime` | `LocalDateTime(...)` | `DateComponents` |
| 提取组件 | `year` `month` `day` `hour` `minute` `second` `weekday` | — | `Year()` 等 | `.year` 等 | `getUTCFullYear()` 等 | pkg | `.year` 等 | `Unix.gmtime` | `.year` 等 | `Calendar.component` |
| 星期 | `weekday`（英文名称 String） | — | `Weekday()`（数字，`String()` 取名称） | `weekday()`（0 = 周一） | `getUTCDay()`（0 = 周日） | pkg | `Date.day_of_week`（1 = 周一） | `tm_wday`（0 = 周日） | `dayOfWeek`（枚举） | 数字（1 = 周日） |
| 读写 ISO 8601 | `parse_iso` `to_iso` | — | `Parse(RFC3339)` `Format` | `fromisoformat` `isoformat` | `toISOString` `Date.parse` | pkg | `from_iso8601` `to_iso8601` | — | `Instant.parse` `toString` | `ISO8601DateFormatter` |
| 格式 | `format(ts, "%Y-%m-%d")` | — | `Format("2006-01-02")` | `strftime` | `Intl` | pkg | `Calendar.strftime` | — | `format` | `DateFormatter` |
| 时间加减 | `add_days` `add_hours` `add_minutes` `add_seconds`、`diff_seconds` | `+ Duration` | `Add` `Sub` | `+ timedelta` | 算术 | pkg | `DateTime.add` `diff` | 算术 | `+ Duration` | `addingTimeInterval` |
| 前后比较 | `is_after` `is_before` | `<` | `After` `Before` | `<` | `<` | pkg | `DateTime.compare` | `<` | `<` | `<` |
| 与 Unix 秒互转 | `from_unix` `to_unix`（无操作） | `UNIX_EPOCH +` | `time.Unix` `.Unix()` | `fromtimestamp` `timestamp` | `new Date(ms)` `getTime` | pkg | `from_unix` `to_unix` | — | `fromEpochSeconds` `epochSeconds` | `Date(timeIntervalSince1970:)` |
| 增加月份（日历计算） | — | — | `AddDate` | — | `setUTCMonth` | pkg | `Date.shift` | — | `plus(DateTimePeriod)` | `Calendar.date(byAdding:)` |
| 时区 | —（仅 UTC 与固定偏移） | — | `LoadLocation` | `zoneinfo` | `Intl` | — | 外部 db | — | pkg | `TimeZone` |

<a id="完全性の規則"></a>

### 完整性规则

1. **表示**：时刻采用 `Int` Unix 秒（UTC）。毫秒时刻仅由 `now_ms()` 返回，`datetime` 的其他函数全部接受秒。
2. **时钟归属**（R-L1-25）：真实时间（秒・毫秒）和单调时钟只归 `datetime`。共 3 个：`now`（秒）、`now_ms`（毫秒）、`monotonic_ns`（纳秒，仅差值有意义），名称后缀表示单位。
3. **组件配对**：可通过 `from_parts` 构造的 6 个组件（年・月・日・时・分・秒）各有提取函数（`year` … `second`）。
4. **加法族**：`add_<単位>` 限定为 `days`・`hours`・`minutes`・`seconds` 共 4 个。不提供周・月・年（月和年需要日历规则，属于 L1 已决定“日历及 tz 放入 package”的范围）。
5. 不提供与运算符同计算的函数或恒等函数（R-L0-01；仅调用另一个公开函数 1 次也计为同义词）。
6. **比较**：时刻是 `Int`，因此不提供 `datetime.compare`。直接传入三向比较函数 `int.compare`（R-L0-16）（`list.sort_with(ts, int.compare)`）。若添加，会转发到 `int.compare`，违反 R-L0-01。

<a id="l2-の判定"></a>

### L2 判定

- 重复：`env.unix_timestamp` ≡ `datetime.now`（#3851 B），`env.millis` 移至 `datetime.now_ms`（R-L1-25）→ R-datetime-01（`env` 侧移除见 L2-env）。`from_unix` `to_unix`（恒等）、`is_after` `is_before` `diff_seconds`（与 `>` `<` `-` 相同）→ R-datetime-03。
- 表示：是否创建时刻专用类型 → R-datetime-02（×）。
- 缺失角色：月份加法（5/9，但需日历规则）、时区（L1 中 ×）→ 不添加。
- 星期返回类型 → R-datetime-04（×）。

<a id="案"></a>

## 提案

<a id="r-datetime-01-ミリ秒の現在時刻-datetimenow_ms-を足す---採用"></a>

### R-datetime-01 添加毫秒当前时刻 `datetime.now_ms` — ○ 采纳

- 内容：`effect fn datetime.now_ms() -> Int`（自 Unix epoch 起的毫秒，真实时间）。与 `env.millis` 同一函数体。`env.millis` 与 `env.unix_timestamp` 的移除在 L2-env 提案中进行，提示分别明确指向 `datetime.now_ms()`、`datetime.now()`。
- 依据的 L0 规则：R-L0-01、R-L0-02（时钟归 `datetime`）、L1 R-L1-25。
- (a) 多数语言将时钟集中一处（Rust `std::time`、Go `time`、Kotlin `kotlin.time`）。
- (b) wasm 用 `wasi:clocks` 的 `wall-clock`，native 用 OS 时钟，保持当前 `env.millis` 路径。
- (c) 测量经过时间时，LLM 会错误地取 `datetime.now` 的秒差而得到 0。同模块提供毫秒时钟即可按名称查找（文档会说明此场景应使用 `monotonic_ns`）。
- (d) 新增，使用数 0（`env.millis` 11/7/21/3、llms.txt:388 将移入）。
- 拟议门禁：为 ADR-0027 effect 检查添加 verdict 行。#3851 F 同义词门禁规则 5 在移除前将与 `env.millis` 的重复计入允许列表。
- issue: #3851 B（已有）

<a id="r-datetime-02-時刻専用の型instant-などを作る---不採用"></a>

### R-datetime-02 创建时刻专用类型（如 `Instant`）— × 不采纳

- 内容：不创建。时刻保持 `Int` Unix 秒。
- 依据的 L0 规则：R-L0-00。根据 R-L0-14 的 4，若旧写法变为类型错误，现在可在同名下改变类型。即便如此，创建类型会改变全部 22 个函数的参数及返回类型，所有以 `Int` 处理时刻的用户代码都需改写。不值得付出此成本并失去下方 (b)(c) 的优点。
- (a) 多数语言采用时刻专用类型（Rust、Go、Python、JS、Elixir、Kotlin、Swift）。Almide 仍以命名与检查弥补差异：单位由函数名后缀（`now`・`now_ms`・`monotonic_ns`）表示；ADR-0001 用类型分开的是时间“量”（`Compute`・`Duration`），而非时刻（某个瞬间）。
- (b) `Int` 秒直接符合 ADR-0016 确定性承诺，也可直接输出 JSON。
- (c) 若创建类型，由于没有从 `Duration` 提取数值的函数（ADR-0001 S7），就没有将 `diff` 结果按秒显示的途径。当前 `Int` 可用一种写法表达。
- 重启条件：若 Dojo 测量观察到混淆毫秒和秒（`datetime.year(datetime.now_ms())`），将创建类型方案以新名称族重新提出。
- issue：无

<a id="r-datetime-03-何もしない関数と演算子の双子を撤去する---採用"></a>

### R-datetime-03 移除恒等函数与运算符重复函数 — ○ 采纳

- 内容：移除 `from_unix`（原样返回参数）、`to_unix`（同样）、`is_after(a, b)`（= `a > b`）、`is_before(a, b)`（= `a < b`）、`diff_seconds(a, b)`（= `a - b`）。
- 依据的 L0 规则：R-L0-01（一个角色一种拼写；仅调用其他公开函数 1 次也算同义词。运算符按 ADR-0005 脱糖为标准函数，因此仅含 1 个运算符的函数同样处理）。
- (a) 在时刻是专用类型的语言中，Elixir `from_unix`、Kotlin `isAfter` 等有意义。但时刻是 `Int` 的 Almide 没有做任何转换。
- (c) 写 `datetime.from_unix(ms)` 时，LLM 会理解为“转换单位”，从而隐藏将毫秒原样当秒的错误。没有函数时，写法本身会显露其为 `Int`。
- (d) 移除 5 个。`@deprecated(since=15, use="x")`（`from_unix`・`to_unix`，提示“直接使用参数”）、`use="a > b"`（`is_after`）、`use="a < b"`（`is_before`）、`use="a - b"`（`diff_seconds`），全部 `removal_epoch = 16`。`almide fix`：`datetime.from_unix(x)` → `x`、`datetime.to_unix(x)` → `x`、`datetime.is_after(a, b)` → `a > b`、`datetime.is_before(a, b)` → `a < b`、`datetime.diff_seconds(a, b)` → `a - b`。使用数（spec/tests/docs/stdlib）：`from_unix` 12/0/6/0、`to_unix` 7/0/6/0（+1 research）、`is_after` 3/0/6/0、`is_before` 5/0/6/0、`diff_seconds` 4/0/6/0。CHEATSHEET・llms.txt 中没有。
- 拟议门禁：在同义词门禁中添加检测“函数体原样返回参数的公开函数”和“函数体仅有 1 个运算符的公开函数”的规则（允许列表只能缩减）。
- issue: #3903

<a id="r-datetime-04-weekday-を数を返す関数にする---不採用"></a>

### R-datetime-04 让 `weekday` 返回数字 — × 不采纳

- 内容：不改变。`weekday` 继续返回英文星期名（`"Monday"` …）的 `String`。
- 依据的 L0 规则：R-L0-00。返回类型改成 `Int` 会让旧写法（`== "Monday"`）产生类型错误，故 R-L0-14 的 4 允许。然而如 (a) 所述，改为数字无益，因此不变。
- (a) 多数语言返回数字，但各语言从 0 还是 1 开始、从星期日还是星期一开始各异（见上表），反而更难猜对。
- (c) 字符串和数字混用会被类型检查阻止，不会静默出错。
- issue：无

<a id="r-datetime-05-format-の--を--にする---不採用"></a>

### R-datetime-05 让 `format` 将 `%%` 变为 `%` — × 不采纳

- 内容：不改变。保留文档中的“不支持 `%%` escape”，并写入 CHEATSHEET 的 `format` 行。
- 依据的 L0 规则：R-L0-14 的 4（不能在同名下令旧写法通过类型检查却产生不同结果，改变 `%%` 输出正属于此）。spec 中含 `%%` 的格式为 0 个；相比改变写法的收益，与训练数据旧语义冲突的损失更大。
- issue：无

<a id="r-datetime-06-add_単位-の族を行列として宣言し演算子の双子の検出から外す---採用記録とゲートの修正"></a>

### R-datetime-06 将 `add_<単位>` 函数族声明为矩阵，排除出“运算符重复”检测 — ○ 采纳（记录及门禁修改）

- 现状：`add_seconds(ts, n)` 函数体是 `ts + n`。R-datetime-03 拟议门禁（检测函数体仅有 1 个运算符的公开函数）也会抓到 `add_seconds`。另一方面，完整性规则将 `add_<単位>` 限定为 4 个。
- 提案：将 `add_{seconds, minutes, hours, days}` 声明为单位轴矩阵，按 R-L0-01“类型矩阵单元不计为同义词”处理。例外不是允许列表，而是从函数族声明（1 处）推导。
- (d) 函数不变。
- issue: #3974

<a id="r-datetime-07-from_parts-は範囲外の部品を-err-にする---採用"></a>

### R-datetime-07 `from_parts` 对越界组件返回 err — ○ 采纳

- 现状：`datetime.from_parts(...) -> Int` 会对越界组件进位（2 月 30 日 25 时 61 分成为 3 月 2 日 02:01）。`parse_iso` 对相同输入返回 err。生成同一值的 2 个函数对无效日期处理相反。
- 提案：改为 `fn datetime.from_parts(y, m, d, h, min, s) -> Result[Int, String]`，以与 `parse_iso` 相同措辞（如“day out of range: 30”）返回 err。
- 依据的 L0 规则：R-L0-06 的 4（可能失败的构造返回附理由的 Result，不静默变成其他值）。
- (a) Python `datetime()`、Elixir `DateTime.new`、Kotlin `LocalDateTime` 拒绝；Go `time.Date`、JS `Date.UTC`、OCaml `Unix.mktime`、Swift `Calendar` 进位，无多数。优先与 Almide 内部 `parse_iso` 一致。
- (c) 在“检查日期是否合法”任务中，LLM 用 `from_parts` 会放过无效日期。
- (d) 语义变更。返回类型改变，旧代码因类型错误停止，满足 R-L0-14.4。`almide fix` 在调用上添加 `!`。用 `from_parts(y, m + 1, …)` 绕过月份加法（已决定不提供）的代码，越界会 err，因此提示手写月份进位的形式。
- 拟议门禁：在 spec 固定边界输入（2 月 29・30 日、13 月、24 时）的结果。
- issue: #3974

<a id="l3-関数表"></a>

## L3：函数表

`datetime` 当前 22 个函数与新增 1 个函数。

| 函数 | 签名（现行，mut 标注） | 其他语言的主要拼写 | 判定 | 提案 |
|---|---|---|---|---|
| `add_days` | `datetime.add_days(ts: Int, n: Int) -> Int` | Go `AddDate(0,0,n)` / Python `+ timedelta(days=n)` / Elixir `DateTime.add(dt, n, :day)` | 保持 | — |
| `add_hours` | `datetime.add_hours(ts: Int, n: Int) -> Int` | Go `Add(n*time.Hour)` / Python `timedelta(hours=n)` / Kotlin `+ n.hours` | 保持 | — |
| `add_minutes` | `datetime.add_minutes(ts: Int, n: Int) -> Int` | Go `Add(n*time.Minute)` / Python `timedelta(minutes=n)` | 保持 | — |
| `add_seconds` | `datetime.add_seconds(ts: Int, n: Int) -> Int` | Go `Add(n*time.Second)` / Elixir `DateTime.add(dt, n)` / Swift `addingTimeInterval` | 保持 | — |
| `day` | `datetime.day(ts: Int) -> Int` | Go `Day()` / Python `.day` / JS `getUTCDate()` / Elixir `.day` | 保持 | — |
| `diff_seconds` | `datetime.diff_seconds(a: Int, b: Int) -> Int` | Go `Sub` / Elixir `DateTime.diff` / 其他为 `-` | 移除 | R-datetime-03（`a - b`） |
| `format` | `datetime.format(ts: Int, pattern: String) -> String` | Python `strftime` / Elixir `Calendar.strftime` / Go `Format`（示例日期） | 保持 | R-datetime-05 |
| `from_parts` | `datetime.from_parts(y: Int, m: Int, d: Int, h: Int, min: Int, s: Int) -> Int` | Go `time.Date` / Python `datetime(...)` / JS `Date.UTC` / Elixir `DateTime.new` | 语义变更（越界 err；`-> Result[Int, String]`） | R-datetime-07 |
| `from_unix` | `datetime.from_unix(seconds: Int) -> Int` | Elixir `DateTime.from_unix` / Go `time.Unix` / Kotlin `fromEpochSeconds` | 移除 | R-datetime-03（参数本身） |
| `hour` | `datetime.hour(ts: Int) -> Int` | Go `Hour()` / Python `.hour` / JS `getUTCHours()` | 保持 | — |
| `is_after` | `datetime.is_after(a: Int, b: Int) -> Bool` | Go `After` / Kotlin `>`（Java `isAfter`） / 其他为 `>` | 移除 | R-datetime-03（`a > b`） |
| `is_before` | `datetime.is_before(a: Int, b: Int) -> Bool` | Go `Before` / 其他为 `<` | 移除 | R-datetime-03（`a < b`） |
| `minute` | `datetime.minute(ts: Int) -> Int` | Go `Minute()` / Python `.minute` / JS `getUTCMinutes()` | 保持 | — |
| `month` | `datetime.month(ts: Int) -> Int` | Go `Month()` / Python `.month` / JS `getUTCMonth()`（从 0 开始） | 保持 | — |
| `now` | `effect datetime.now() -> Int` | Python `time.time()` / Go `time.Now().Unix()` / Elixir `System.os_time(:second)` | 保持 | R-datetime-01（`env.unix_timestamp` 的保留一侧） |
| `monotonic_ns` | `effect datetime.monotonic_ns() -> Int` | Python `time.monotonic_ns()` / Rust `Instant::now()` / Elixir `System.monotonic_time` | 保持 | — |
| `parse_iso` | `datetime.parse_iso(s: String) -> Result[Int, String]` | Python `fromisoformat` / Elixir `DateTime.from_iso8601` / Go `Parse(time.RFC3339, s)` / Kotlin `Instant.parse` | 保持 | — |
| `second` | `datetime.second(ts: Int) -> Int` | Go `Second()` / Python `.second` / JS `getUTCSeconds()` | 保持 | — |
| `to_iso` | `datetime.to_iso(ts: Int) -> String` | Python `isoformat` / JS `toISOString` / Elixir `to_iso8601` | 保持 | — |
| `to_unix` | `datetime.to_unix(ts: Int) -> Int` | Elixir `DateTime.to_unix` / Go `.Unix()` / Python `timestamp()` | 移除 | R-datetime-03（参数本身） |
| `weekday` | `datetime.weekday(ts: Int) -> String` | Go `Weekday()` / Python `weekday()`（数字） / Elixir `Date.day_of_week`（数字） | 保持 | R-datetime-04 |
| `year` | `datetime.year(ts: Int) -> Int` | Go `Year()` / Python `.year` / JS `getUTCFullYear()` | 保持 | — |
| `now_ms` | （新增）`effect datetime.now_ms() -> Int` | JS `Date.now()` / Go `time.Now().UnixMilli()` / Elixir `System.os_time(:millisecond)` / Kotlin `toEpochMilliseconds()` | 新增 | R-datetime-01 |

<a id="l0-への差し戻し処理済み"></a>

## 退回 L0 的事项（已处理）

- 接收时间量的函数类型：保持 `Int`（× 不采纳）。既已决定时刻以 Int Unix 秒表示（R-datetime-02），时间量也统一为名称包含单位的 `Int`（`add_seconds`、`sleep_ms`）。`Duration` 按 ADR-0001 限于 `fan` 预算与期限。`Duration` 没有读取函数（S7），混入时刻计算就需增加读取途径或按类型提供 2 套函数，两者都违反 R-L0-01。
