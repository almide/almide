# duration

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](duration.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](duration.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](duration.zh-CN.md)



> 原文：[duration.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/duration.md)。这是供读者参考的非规范性固定快照。API 索引取自固定版本原文，本译文不参与自动生成或自动同步。签名、类型、`@since` 及代码示例（包括注释）保持原文，API 说明译文列于文末。本次翻译未独立执行代码示例。

墙钟时间构造器。自动可用（检查器提供的接口，无需 import，不是标准库模块）。规范规则：[ADR-0001](../../../../adr/0001-deterministic-time-units.md)。

`Duration` 值是**墙钟时间**的量，即秒表在程序实际运行的机器上测到的时间。这是 oracle 层的时钟：`fan.timeout(duration.ms(n)) { body }` 在资源用量计量点进行协作式检查（效应接口的截止时间也预留同一类型，属于未来功能）；确定性层读取另一种时钟 [compute](../../../../stdlib/compute.md)。两个类型从不混用，也不能相互转换：正是这道隔离，使两者可以安全共享熟悉的 `ms` 词汇。

单位集合是封闭的：`ns / us / ms / s / min / h`。没有时间字面量（`5s` 无法解析），普通 `Int` 从来不是时间。

### `duration.ns(n: Int) -> Duration` … `duration.h(n: Int) -> Duration`

```almd
duration.ms(5000)  // the conventional timeout spelling
duration.s(30)
duration.min(10)
```

负参数会确定性地触发运行时中止；构造溢出时饱和至最大可表示时间。

<a id="algebra-adr-0001-s3"></a>

### 代数运算（ADR-0001 S3）

在同一种时钟内，接口与 `Compute` 相同：

```almd
duration.s(1) + duration.ms(500)   // Duration — saturating add
duration.s(2) - duration.s(3)      // Duration — saturates at 0
duration.ms(100) * 3               // Duration — scale by Int
duration.ms(1) < duration.s(1)     // Bool
```

任何运算符中的 `Duration ⊕ Compute` 都是类型错误——墙钟截止时间不能混入确定性预算（`fan.bounded(duration.ms(5))` 被拒绝，并指向 `compute.ms(...)` / `fan.timeout`）。



<a id="signature-index-7-functions"></a>

## 固定快照 API 索引（7个函数）

```
// n nanoseconds of wall-clock time; negative n aborts.
// @since 0.51.0 or earlier
duration.ns(n: Int) -> Duration

// n microseconds of wall-clock time; negative n aborts.
// @since 0.51.0 or earlier
duration.us(n: Int) -> Duration

// n milliseconds of wall-clock time; negative n aborts.
// @since 0.51.0 or earlier
duration.ms(n: Int) -> Duration

// n seconds of wall-clock time; negative n aborts.
// @since 0.51.0 or earlier
duration.s(n: Int) -> Duration

// n minutes of wall-clock time; negative n aborts.
// @since 0.51.0 or earlier
duration.min(n: Int) -> Duration

// n hours of wall-clock time; negative n aborts.
// @since 0.51.0 or earlier
duration.h(n: Int) -> Duration

// Negative if a < b, 0 if equal, positive if a > b.
// @since unreleased
duration.compare(a: Duration, b: Duration) -> Int
```




## API 说明译文（按上述签名顺序）

- `duration.ns(n: Int) -> Duration` — n 纳秒墙钟时间；n 为负则中止。
- `duration.us(n: Int) -> Duration` — n 微秒墙钟时间；n 为负则中止。
- `duration.ms(n: Int) -> Duration` — n 毫秒墙钟时间；n 为负则中止。
- `duration.s(n: Int) -> Duration` — n 秒墙钟时间；n 为负则中止。
- `duration.min(n: Int) -> Duration` — n 分钟墙钟时间；n 为负则中止。
- `duration.h(n: Int) -> Duration` — n 小时墙钟时间；n 为负则中止。
- `duration.compare(a: Duration, b: Duration) -> Int` — a < b 时为负，相等时为0，a > b 时为正。
