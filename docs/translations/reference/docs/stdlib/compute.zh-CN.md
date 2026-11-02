# compute

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](compute.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](compute.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](compute.zh-CN.md)



> 原文：[compute.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/compute.md)。这是供读者参考的非规范性固定快照，不参与自动生成或自动同步。签名、代码示例和 API 索引注释保持原文，说明译文附于文末。本次翻译未独立复核测量、行为或示例。
>
> 原文注意：确定性计算时间并非实际经过时间或截止时间保证。规范定义在所链接 ADR 原文中，此固定译文不构成新规范。

确定性计算时间构造器。自动可用（checker 表层功能，无需 import，并非 stdlib module）。规范规则见 [ADR-0001](../../../../adr/0001-deterministic-time-units.md)。

`Compute` 值是**确定性计算时间**的量：它在冻结的抽象机器上度量程序自身工作（在函数入口和循环头计费单位），因此预算判定仅由程序及其输入决定，在每个目标、每个宿主上相同。这是 `fan.bounded` 读取的时钟，也是 `fan.min(compute…)` 选择依据的时钟（`fan.min` 的时钟参数必需，其类型说明按什么度量“最小”）。实际经过时间是另一种时钟，见 [duration](../../../../stdlib/duration.md)。

单位集合封闭：`ns / us / ms / s / min / h`。没有时间字面量（`100ms` 无法解析），裸 `Int` 从不代表时间。

### `compute.ns(n: Int) -> Compute` … `compute.h(n: Int) -> Compute`

```almd run
fn spin(n: Int) -> Int = {
  var i = 0
  while i < n {
    i = i + 1
  }
  42
}

effect fn main() -> Unit = {
  let a = compute.ns(1500)   // nanoseconds
  let b = compute.us(50)     // microseconds
  let c = compute.ms(100)    // milliseconds
  let d = compute.s(2)       // seconds
  let e = compute.min(5)     // minutes
  let f = compute.h(1)       // hours
  // every unit is the same clock: the sum is a budget fan.bounded reads
  println(int.to_string(fan.bounded(a + b + c + d + e + f) { spin(1000) } ?? -1))
}
```
```output
42
```

负参数会确定性地在运行时 abort（`Error: negative time: compute.us(-5)`，exit 1，两个目标相同）；构造溢出时饱和为可表示的最大时间。

<a id="algebra-adr-0001-s3"></a>

### 代数（ADR-0001 S3）

```almd run
fn spin(n: Int) -> Int = {
  var i = 0
  while i < n {
    i = i + 1
  }
  42
}

effect fn main() -> Unit = {
  let sum = compute.ms(2) + compute.ms(3)      // Compute — saturating add
  let diff = compute.ms(5) - compute.ms(2)     // Compute — saturates at 0 (never negative)
  let scaled = compute.ms(2) * 3               // Compute — scale by Int (either order);
                                               // a negative factor aborts
  let less = compute.ms(2) < compute.ms(3)     // Bool — same-clock comparison
  let floor = compute.ms(1) - compute.ms(2)    // saturated to 0: admits no loop
  println(int.to_string(fan.bounded(sum) { spin(1000) } ?? -1))
  println(int.to_string(fan.bounded(diff) { spin(1000) } ?? -1))
  println(int.to_string(fan.bounded(scaled) { spin(1000) } ?? -1))
  println(int.to_string(fan.bounded(3 * compute.ms(2)) { spin(1000) } ?? -1))
  println(int.to_string(fan.bounded(floor) { spin(1000) } ?? -1))
  println(if less then "less" else "not less")
}
```
```output
42
42
42
42
-1
less
```

`T * T`（时间 × 时间）、混合 `Compute` 与 `Duration` 或裸 `Int` 的任意操作，以及 `/` 都是类型错误；每个单元为何封闭请见 ADR。

<a id="consumers"></a>

### 使用者

```almd run
fn work(n: Int) -> Int = n * 2

fn fast() -> Int = 1

fn slow() -> Int = {
  var i = 0
  while i < 100000 {
    i = i + 1
  }
  2
}

effect fn main() -> Unit = {
  let input = 21
  let fallback = -1
  let r = fan.bounded(compute.ms(100)) { work(input) } ?? fallback
  let w = fan.min(compute.us(50)) { fast(), slow() } ?? fallback
  println(int.to_string(r))
  println(int.to_string(w))
}
```
```output
42
1
```

`almide run --time-report` 将程序的确定性时间打印在测得的实际经过时间旁（`time: 0.15ms deterministic (≈38.7ms wall here)`）；两者是不同的时钟，从不声称是同一个量。



<a id="signature-index-7-functions"></a>

## 固定快照 API 索引 (7 个函数)

```
// n nanoseconds of deterministic compute time; negative n aborts.
// @since 0.51.0 or earlier
compute.ns(n: Int) -> Compute

// n microseconds of deterministic compute time; negative n aborts.
// @since 0.51.0 or earlier
compute.us(n: Int) -> Compute

// n milliseconds of deterministic compute time; negative n aborts.
// @since 0.51.0 or earlier
compute.ms(n: Int) -> Compute

// n seconds of deterministic compute time; negative n aborts.
// @since 0.51.0 or earlier
compute.s(n: Int) -> Compute

// n minutes of deterministic compute time; negative n aborts.
// @since 0.51.0 or earlier
compute.min(n: Int) -> Compute

// n hours of deterministic compute time; negative n aborts.
// @since 0.51.0 or earlier
compute.h(n: Int) -> Compute

// Negative if a < b, 0 if equal, positive if a > b.
// @since unreleased
compute.compare(a: Compute, b: Compute) -> Int
```



## API 说明译文（按上述签名顺序）

- `compute.ns(n: Int) -> Compute` — n 纳秒确定性计算时间；负 n 则 abort。
- `compute.us(n: Int) -> Compute` — n 微秒确定性计算时间；负 n 则 abort。
- `compute.ms(n: Int) -> Compute` — n 毫秒确定性计算时间；负 n 则 abort。
- `compute.s(n: Int) -> Compute` — n 秒确定性计算时间；负 n 则 abort。
- `compute.min(n: Int) -> Compute` — n 分钟确定性计算时间；负 n 则 abort。
- `compute.h(n: Int) -> Compute` — n 小时确定性计算时间；负 n 则 abort。
- `compute.compare(a: Compute, b: Compute) -> Int` — a < b 为负，相等为 0，a > b 为正。
