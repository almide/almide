<!-- description: Stdlib redesign L2/L3 — duration: the six wall-clock constructors stay as ADR-0001 fixed them; add compare as the function form of its Ord, keep readers and Float constructors omitted -->
<a id="l2l3-duration"></a>

# L2/L3：duration

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-duration.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-duration.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-duration.zh-CN.md)

> 非规范性历史阅读快照。以固定原文为准，不自动同步。未独立重跑测量、证明 gate 或当前实现检查。未执行引用命令。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-duration.md)
> 这是带日期的重设计决定与gate提案记录，不是新实现核实或使用量普查。R-duration-01/03采纳及R-duration-03现状表述与L3将compare列为新增并存，不静默调和阶段差异。保留读取/Float/日单位省略及重启条件；TimeAmount遵循不合并时钟、不允许转换。Compute计数冻结机器charge，不是实测CPU时间。

> 最后更新：2026-10-21
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)、[L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)（R-L1-21：不与 `compute` 合并）、[ADR-0001](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0001-deterministic-time-units.md)
> 相关：[L2-compute](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-compute.md)、[L2-datetime](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-datetime.md)
> 来源：Rust https://doc.rust-lang.org/std/time/struct.Duration.html ；Go https://pkg.go.dev/time#Duration ；Python https://docs.python.org/3/library/datetime.html#timedelta-objects ；TS/JS https://tc39.es/proposal-temporal/docs/duration.html ；Gleam https://hexdocs.pm/gleam_time/gleam/time/duration.html（官方 package）；Elixir https://hexdocs.pm/elixir/Duration.html ；OCaml https://ocaml.org/manual/latest/api/Unix.html（标准库无 Duration 类型）；Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.time/-duration/ ；Swift https://developer.apple.com/documentation/swift/duration

`duration` 是按 ADR-0001 规范封闭的表面：六个构造器 `ns` `us` `ms` `s` `min` `h`，负值确定性 abort，构造溢出饱和；运算只包括 `+`、`-`（在0饱和）、`* Int` 及同类型比较（S3）。数值读取函数（如 `in_ms`）、`Float` 构造器及日以上单位，是 S7 带重新开启条件的有意省略。2026-10-16 裁定已撤回添加读取函数的方案，展示使用 `Repr`。

<a id="l2-役割の比較"></a>

## L2：角色比较

| 角色 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 按单位构造 | `ns` `us` `ms` `s` `min` `h` | `from_nanos` `from_millis` `from_secs` 等 | `n * time.Millisecond` | `timedelta(milliseconds=n)` | `Temporal.Duration.from` | pkg `duration.milliseconds` | `Duration.new!` | — | `n.milliseconds` | `.milliseconds(n)` |
| 日以上单位 | —（S7） | — | — | `days=` | `days` | — | `day:` | — | `n.days` | — |
| 从小数构造 | —（S7） | `from_secs_f64` | — | `seconds=1.5` | — | — | — | — | `1.5.seconds` | `.seconds(1.5)` |
| 加减及整数倍 | `+` `-` `* Int` | `+` `-` `*` | `+` `-` `*` | `+` `-` `*` | `add` | pkg `add` | `Duration.add` | — | `+` `-` `*` | `+` `-` `*` |
| 比较 | `<` `==` 等（`Ord`） | `<` `cmp` | `<` | `<` | `Duration.compare` | pkg `compare` | `Duration` 无比较函数 | — | `<` `compareTo` | `<` |
| 按数值读取 | —（S7，展示为 `Repr`） | `as_millis` | `Milliseconds()` | `total_seconds()` | `total` | pkg `to_seconds` | `to_timeout` | — | `inWholeMilliseconds` | `components` |
| 区分不同时钟的量 | `Compute` 混用则类型错误 | — | — | — | — | — | — | — | — | `Clock` 各自的 `Instant` |

<a id="完全性の規則"></a>

### 完整性规则

1. ADR-0001 S2：两个时钟 × 六单位的12格完整存在，分别为 `duration` 六个与 [compute](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-compute.md) 六个；单位是封闭集合。
2. ADR-0001 S3：用运算符表达运算，`Duration` 与 `Compute` 不可混用，没有转换函数。
3. R-L0-16（有序类型具有 `compare`）：`Duration` 推导 `Ord`，因此提供 `duration.compare(a, b) -> Int`，与 `compute.compare` 同形。它是 `<` 的函数形式，不是将量读为数值的函数，与 S7 省略兼容。
4. 有意省略（S7 台账）：读取函数、`Float` 构造器、日以上单位、`T / Int`、`min` 和 `max`。

<a id="l2-の判定"></a>

### L2 判定

- 缺失角色：读取（7/9）、小数构造器（4/9）、日单位（5/9）不添加，因为 ADR-0001 S7 重新开启条件未满足（R-duration-02 ×）。
- 缺失格：`compare`（R-L0-16）→ R-duration-01。
- 重复：无。`duration.ms` 和 `compute.ms` 是不同类型的不同值，如 #3851 的“Dropped”所述。

<a id="案"></a>

## 方案

<a id="r-duration-01-durationcompare-を足す---採用"></a>

### R-duration-01 添加 `duration.compare` — ○ 采纳

- 内容：`duration.compare(a: Duration, b: Duration) -> Int`，`a < b` 时负，相等为0，`a > b` 时正。
- L0 依据：R-L0-16（有序类型有 `<mod>.compare(a, b) -> Int`）、R-L0-02。
- (a) Rust `Ord::cmp`、Kotlin `compareTo`、Temporal `Duration.compare`。
- (b) 纯计算；比较两个各为单个纳秒 `Int` 的值，双 target 相同。
- (c) 可直接传给接收比较函数的函数，如 `list.sort_with(ds, duration.compare)`。与 `Int` 的 `int.compare` 同写法，可从规则推得。
- (d) 新增，使用数0。ADR-0001 S2 的12格 gate（S6-1）只数构造器，不变。
- Gate 方案：将 `Duration` 和 `Compute` 加入 R-L0-16 gate，检查所有推导 `Ord` 的标准类型均有 `compare`。
- issue：#3902。

<a id="r-duration-02-読み出し関数小数の構築子日の単位を足す---不採用"></a>

### R-duration-02 添加读取函数、小数构造器及日单位 — × 不采纳

- 内容：不添加，遵循 ADR-0001 S7 台账，也符合2026-10-16撤回读取函数方案的裁定。
- 依据：ADR-0001 S7 重新开启条件——预算用于计算的实例、Dojo 观察到整数换算写错、实际需求记录——仍未满足。L1 新增标准要求“既有裁定无法覆盖”。
- issue：无。

<a id="r-duration-03-duration-と-compute-を-1-つのプロトコル-timeamount-に従わせる---採用"></a>

### R-duration-03 让 `Duration` 和 `Compute` 遵循一个 `TimeAmount` 协议 — ○ 采纳

- 现状：`Duration`（墙钟时间）和 `Compute`（确定性计算时间）拥有相同六单位构造器、`compare` 及运算符。但“表面相同”仅由 ADR-0001 S1 文字和逐类型复制的矩阵 gate 守护，并未成为语言声明。
- 方案：标准库放一个协议，两类型遵循它。
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
  - 一处声明即可读出两类型只区别于“哪个时钟的量”，用法相同。
  - 类型仍不同（R-L1-21、ADR-0001 S3：混用类型错误，无转换函数）。协议只表明形状相同，不允许将一方当另一方传递。
  - 构造器和 `compare` 直接以当前 module 函数（如 `duration.ms`、`compute.compare`）成为协议 method，不另加转发同一计算的函数（R-L0-01）。
  - 运算符（`+`、`-`、`* Int`、`<`）仍按 ADR-0001 S3 矩阵放在两类型上。协议只能包含 method，因此不纳入这里。
  - 用户写一个不选择时钟的函数即可用于两者：`fn half_or_less[T: TimeAmount](a: T, limit: T) -> T`。
- L0 依据：R-L0-01（同一角色不要手写两处），CLAUDE.md 的“性质从一处声明导出”。
- 术语注意：`Compute` 不是 OS 测量的 CPU 时间，而是冻结抽象机的 charge 数（函数入口和循环头计数），在所有 host 上相同的确定性量（ADR-0001）。它是与墙钟相对的另一个时钟。
- (a) Swift `Clock` 协议可为每个时钟确定遵循 `DurationProtocol` 的量类型（[DurationProtocol](https://developer.apple.com/documentation/swift/durationprotocol)；标准 `ContinuousClock` 与 `SuspendingClock` 都用同一 `Duration` 类型）。Java `TemporalAmount`（由 `Duration` 和 `Period` 遵循）也以一个接口归纳量类型。其他语言未按类型区分时钟差异。
- (b) 仅 checker 更改；值表示仍为单个纳秒 Int。
- (c) 学会 `duration.ms(5)`，也可按同形写 `compute.ms(5)`，这一对应成为类型声明，不只是文档承诺。协议检查也防止只向一方添加函数的偏差。
- (d) 新增一个协议及两个类型的声明，既有代码不变。选择 `TimeAmount` 因其不与其他语言写法冲突，且能读为“时间的量”。
- 完整性规则：遵循 `TimeAmount` 的类型拥有全部六单位构造器及 `compare`，由协议检查强制。ADR-0001 S2 的12格矩阵 gate 被此协议检查取代。
- Gate 方案：用 `spec/stdlib/` 中的泛型函数测试确认两类型遵循 `TimeAmount`，对 `Duration` 和 `Compute` 调用同一泛型函数。
- issue：#3964。

<a id="r-duration-04-min分を-minutes-などの完全な綴りにする---不採用"></a>

### R-duration-04 将 `min`（分钟）改为 `minutes` 等完整写法 — × 不采纳

- 理由：六单位拼写由 ADR-0001 S2 决定。若误以最小值之意写 `duration.min(a, b)`，参数数量不符会报 E004，不会静默产生另一值。
- issue：无。

<a id="l3-関数表"></a>

## L3：函数表

`duration` 当前六个函数及新增一个函数。

| 函数 | 签名（当前，显示 mut） | 其他语言主要写法 | 判定 | 方案 |
|---|---|---|---|---|
| `ns` | `duration.ns(n: Int) -> Duration` | Rust `from_nanos` / Go `time.Nanosecond` / Kotlin `nanoseconds` / Swift `.nanoseconds` | 维持 | — |
| `us` | `duration.us(n: Int) -> Duration` | Rust `from_micros` / Go `time.Microsecond` / Python `microseconds=` / Kotlin `microseconds` | 维持 | — |
| `ms` | `duration.ms(n: Int) -> Duration` | Rust `from_millis` / Go `time.Millisecond` / Python `milliseconds=` / Kotlin `milliseconds` / Swift `.milliseconds` | 维持 | — |
| `s` | `duration.s(n: Int) -> Duration` | Rust `from_secs` / Go `time.Second` / Python `seconds=` / Kotlin `seconds` / Swift `.seconds` | 维持 | — |
| `min` | `duration.min(n: Int) -> Duration` | Rust `from_mins` / Go `time.Minute` / Python `minutes=` / Kotlin `minutes` | 维持 | — |
| `h` | `duration.h(n: Int) -> Duration` | Rust `from_hours` / Go `time.Hour` / Python `hours=` / Kotlin `hours` | 维持 | — |
| `compare` | （新增）`duration.compare(a: Duration, b: Duration) -> Int` | Rust `cmp` / Kotlin `compareTo` / Temporal `Duration.compare` | 新增 | R-duration-01 |
