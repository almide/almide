# float64

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](float64.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](float64.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](float64.zh-CN.md)



> 原文：[float64.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/float64.md)。这是面向读者的非规范性固定快照。API 索引对应原文固定时点，本译文不参与自动生成或自动同步。签名、类型、`@since` 及代码示例（包括注释）保持原样，API 说明译文附在末尾。本次翻译未独立运行代码示例。

`Float64`（64位浮点数，IEEE-754 binary64）：边界、算术、位、文本以及到该类型的转换。自动导入，无需 `import`。

`Float64` 与规范的 `Float` 共享表示但属于不同类型，因此 `Float64` 与 `float32` 一样拥有独立模块。

<a id="conversions"></a>

## 转换

转换以其构造的类型命名（R-L0-40）：`float64.from(x)` 接受任意数值输入，包括 `Int`、有指定位宽的整数、`Float` 或有指定位宽的浮点数。

```almd
let x: Float64 = float64.from(1.5)
let r = float64.from(0.1)              // the same value, re-tagged
let exact = float64.from_checked(16777217) // some: every Int up to 2^53 is exact
let n = int.from_saturating(x)         // leaving Float64: the destination's family
let text = x.to_string()
```

| 函数 | 行为 |
|---|---|
| `float64.from(x)` | 最近的 Float64（中点到偶数）；NaN 保持 NaN |
| `float64.from_checked(x)` | Float64 能精确容纳 x 时为 `some`，否则（被舍入的值、NaN）为 `none` |

要从 `Float64` 转出，请调用目标类型的转换族：`float.from(x)` 精确拓宽；整数目标按名称选择——`int.from(x)` 遇到 NaN、无穷大或越界值会中止，`int.from_checked(x)` 对小数返回 `none`，`int.from_saturating(x)` 向零截断并限制范围（NaN 为0）。

确切的 API 列表见下方保留的原文机器管理签名索引（本译文是固定快照，不由机器维护）。

> 原文既说明 Float64 与 Float 共享表示，又将转换称为 “widens”。此处保留这一表述差异，不作统一。

<a id="signature-index-24-functions"></a>

## 原文固定快照 API 索引（24个函数）

```
// Plain decimal, never an exponent; 2.0 keeps .0.
// @since 0.37.0 or earlier
float64.to_string(x: Float64) -> String

// -1, 0 or 1 in IEEE 754 total order: -0.0 < 0.0, NaN above inf, NaN equal to NaN.
// @since unreleased
float64.compare(a: Float64, b: Float64) -> Int

// Lesser; a NaN operand is ignored; -0.0 < 0.0.
// @since unreleased
float64.min(a: Float64, b: Float64) -> Float64

// Greater; a NaN operand is ignored; 0.0 > -0.0.
// @since unreleased
float64.max(a: Float64, b: Float64) -> Float64

// x limited to lo..hi; lo > hi or a NaN bound aborts.
// @since unreleased
float64.clamp(x: Float64, lo: Float64, hi: Float64) -> Float64

// True only for NaN.
// @since unreleased
float64.is_nan(x: Float64) -> Bool

// True for inf and -inf; false for NaN.
// @since unreleased
float64.is_infinite(x: Float64) -> Bool

// True unless x is inf, -inf or NaN.
// @since unreleased
float64.is_finite(x: Float64) -> Bool

// Magnitude; -0.0 gives 0.0, NaN stays NaN.
// @since unreleased
float64.abs(x: Float64) -> Float64

// Round toward -inf; -0.5 gives -1.0.
// @since unreleased
float64.floor(x: Float64) -> Float64

// Round toward +inf; -0.5 gives -0.0.
// @since unreleased
float64.ceil(x: Float64) -> Float64

// Nearest whole value; halves round away from zero.
// @since unreleased
float64.round(x: Float64) -> Float64

// Round toward 0; -0.5 gives -0.0.
// @since unreleased
float64.trunc(x: Float64) -> Float64

// 1.0 or -1.0 by sign bit (0.0 gives 1.0, -0.0 gives -1.0); NaN for NaN.
// @since unreleased
float64.signum(x: Float64) -> Float64

// The 64 IEEE-754 binary64 bits as Int; any NaN gives the canonical NaN.
// @since unreleased
float64.to_bits(x: Float64) -> Int

// Float64 whose IEEE-754 binary64 bits are bits.
// @since unreleased
float64.from_bits(bits: Int) -> Float64

// Positive infinity.
// @since unreleased
float64.infinity() -> Float64

// The one quiet NaN (bits 0x7ff8000000000000).
// @since unreleased
float64.nan() -> Float64

// Gap between 1.0 and the next Float64: 2^-52.
// @since unreleased
float64.epsilon() -> Float64

// Largest finite Float64, about 1.7976931348623157e308.
// @since unreleased
float64.max_value() -> Float64

// Most negative finite Float64 (= -max_value()), not the smallest positive.
// @since unreleased
float64.min_value() -> Float64

// Float64 from ASCII-trimmed s, the grammar of float.parse; err on junk, inf, NaN.
// @since unreleased
float64.parse(s: String) -> Result[Float64, String]

// x as the nearest Float64 (ties to even); NaN stays NaN.
// @since unreleased
float64.from(x: S) -> Float64

// some(x as Float64) when Float64 holds x exactly, else none (a rounded value, NaN).
// @since unreleased
float64.from_checked(x: S) -> Option[Float64]
```

## API 说明译文（按上述签名顺序）

- `float64.to_string(x: Float64) -> String`：普通十进制，绝不使用指数；2.0保留 .0。
- `float64.compare(a: Float64, b: Float64) -> Int`：按 IEEE 754 全序返回 -1、0或1：-0.0 < 0.0，NaN 高于 inf，NaN 等于 NaN。
- `float64.min(a: Float64, b: Float64) -> Float64`：较小者；忽略 NaN 操作数；-0.0 < 0.0。
- `float64.max(a: Float64, b: Float64) -> Float64`：较大者；忽略 NaN 操作数；0.0 > -0.0。
- `float64.clamp(x: Float64, lo: Float64, hi: Float64) -> Float64`：将 x 限制在 lo..hi；lo > hi 或边界为 NaN 时中止。
- `float64.is_nan(x: Float64) -> Bool`：仅对 NaN 为 true。
- `float64.is_infinite(x: Float64) -> Bool`：对 inf 和 -inf 为 true，对 NaN 为 false。
- `float64.is_finite(x: Float64) -> Bool`：除非 x 为 inf、-inf 或 NaN，否则为 true。
- `float64.abs(x: Float64) -> Float64`：绝对值；-0.0得到0.0，NaN 保持 NaN。
- `float64.floor(x: Float64) -> Float64`：向 -inf 舍入；-0.5得到 -1.0。
- `float64.ceil(x: Float64) -> Float64`：向 +inf 舍入；-0.5得到 -0.0。
- `float64.round(x: Float64) -> Float64`：最近的整数值；中点向远离零的方向舍入。
- `float64.trunc(x: Float64) -> Float64`：向0舍入；-0.5得到 -0.0。
- `float64.signum(x: Float64) -> Float64`：按符号位返回1.0或-1.0（0.0得到1.0，-0.0得到-1.0）；NaN 得到 NaN。
- `float64.to_bits(x: Float64) -> Int`：将 IEEE-754 binary64 的64位作为 Int 返回；任何 NaN 都得到规范 NaN。
- `float64.from_bits(bits: Int) -> Float64`：IEEE-754 binary64 位为 bits 的 Float64。
- `float64.infinity() -> Float64`：正无穷。
- `float64.nan() -> Float64`：唯一的 quiet NaN（位为0x7ff8000000000000）。
- `float64.epsilon() -> Float64`：1.0与下一个 Float64 的间隔：2^-52。
- `float64.max_value() -> Float64`：最大的有限 Float64，约1.7976931348623157e308。
- `float64.min_value() -> Float64`：最负的有限 Float64（= -max_value()），不是最小正数。
- `float64.parse(s: String) -> Result[Float64, String]`：去除 s 两侧 ASCII 空白后解析为 Float64，语法同 float.parse；无效内容、inf、NaN 返回 err。
- `float64.from(x: S) -> Float64`：将 x 转为最近的 Float64（中点到偶数）；NaN 保持 NaN。
- `float64.from_checked(x: S) -> Option[Float64]`：若 Float64 能精确容纳 x，则 some(x as Float64)，否则（被舍入的值、NaN）为 none。
