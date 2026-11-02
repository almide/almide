# float32

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](float32.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](float32.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](float32.zh-CN.md)



> 原文：[float32.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/float32.md)。这是面向读者的非规范性固定快照。API 索引对应原文固定时点，本译文不参与自动生成或自动同步。签名、类型、`@since` 及代码示例（包括注释）保持原样，API 说明译文附在末尾。本次翻译未独立运行代码示例。

`Float32`（32位浮点数，IEEE-754 binary32）：边界、算术、位、文本以及到该类型的转换。自动导入，无需 `import`。

`Float32` 具有 binary32 精度：转换和算术结果均舍入到该精度。超出 binary32 有限范围的转换会变为无穷大，而不是错误。

<a id="conversions"></a>

## 转换

转换以其构造的类型命名（R-L0-40）：`float32.from(x)` 接受任意数值输入，包括 `Int`、有指定位宽的整数、`Float` 或有指定位宽的浮点数。

```almd
let x: Float32 = float32.from(1.5)
let r = float32.from(0.1)              // the nearest Float32, rounded once
let exact = float32.from_checked(0.1)  // none: 0.1 is not a binary32 value
let n = int.from_saturating(x)         // leaving Float32: the destination's family
let text = x.to_string()
```

| 函数 | 行为 |
|---|---|
| `float32.from(x)` | 最近的 Float32（中点到偶数）；NaN 保持 NaN |
| `float32.from_checked(x)` | Float32 能精确容纳 x 时为 `some`，否则（被舍入的值、NaN）为 `none` |

要从 `Float32` 转出，请调用目标类型的转换族：`float.from(x)` 精确拓宽；整数目标按名称选择——`int.from(x)` 遇到 NaN、无穷大或越界值会中止，`int.from_checked(x)` 对小数返回 `none`，`int.from_saturating(x)` 向零截断并限制范围（NaN 为0）。

确切的 API 列表见下方保留的原文机器管理签名索引（本译文是固定快照，不由机器维护）。

> 溢出得到 inf 是本 Float32 原文的说明，不将其推广为所有 Float 的 parse 行为；签名索引对 float.parse 语法的引用也保持原文含义。

<a id="signature-index-24-functions"></a>

## 原文固定快照 API 索引（24个函数）

```
// Shortest f32 round-trip digits, no exponent; 0.1 stays 0.1, 2.0 keeps .0.
// @since 0.15.0 or earlier
float32.to_string(x: Float32) -> String

// -1, 0 or 1 in IEEE 754 total order: -0.0 < 0.0, NaN above inf, NaN equal to NaN.
// @since unreleased
float32.compare(a: Float32, b: Float32) -> Int

// Lesser; a NaN operand is ignored; -0.0 < 0.0.
// @since unreleased
float32.min(a: Float32, b: Float32) -> Float32

// Greater; a NaN operand is ignored; 0.0 > -0.0.
// @since unreleased
float32.max(a: Float32, b: Float32) -> Float32

// x limited to lo..hi; lo > hi or a NaN bound aborts.
// @since unreleased
float32.clamp(x: Float32, lo: Float32, hi: Float32) -> Float32

// True only for NaN.
// @since unreleased
float32.is_nan(x: Float32) -> Bool

// True for inf and -inf; false for NaN.
// @since unreleased
float32.is_infinite(x: Float32) -> Bool

// True unless x is inf, -inf or NaN.
// @since unreleased
float32.is_finite(x: Float32) -> Bool

// Magnitude; -0.0 gives 0.0, NaN stays NaN.
// @since unreleased
float32.abs(x: Float32) -> Float32

// Round toward -inf; -0.5 gives -1.0.
// @since unreleased
float32.floor(x: Float32) -> Float32

// Round toward +inf; -0.5 gives -0.0.
// @since unreleased
float32.ceil(x: Float32) -> Float32

// Nearest whole value; halves round away from zero.
// @since unreleased
float32.round(x: Float32) -> Float32

// Round toward 0; -0.5 gives -0.0.
// @since unreleased
float32.trunc(x: Float32) -> Float32

// 1.0 or -1.0 by sign bit (0.0 gives 1.0, -0.0 gives -1.0); NaN for NaN.
// @since unreleased
float32.signum(x: Float32) -> Float32

// The 32 IEEE-754 binary32 bits as a non-negative Int; any NaN gives 0x7fc00000.
// @since unreleased
float32.to_bits(x: Float32) -> Int

// Float32 whose binary32 bits are the low 32 bits of bits.
// @since unreleased
float32.from_bits(bits: Int) -> Float32

// Positive infinity.
// @since unreleased
float32.infinity() -> Float32

// The one quiet NaN (bits 0x7fc00000).
// @since unreleased
float32.nan() -> Float32

// Gap between 1.0 and the next Float32: 2^-23.
// @since unreleased
float32.epsilon() -> Float32

// Largest finite Float32, about 3.4028235e38.
// @since unreleased
float32.max_value() -> Float32

// Most negative finite Float32 (= -max_value()), not the smallest positive.
// @since unreleased
float32.min_value() -> Float32

// Float32 from trimmed s, the grammar of float.parse; err on junk; overflow gives inf.
// @since unreleased
float32.parse(s: String) -> Result[Float32, String]

// x as the nearest Float32 (ties to even), rounded once; NaN stays NaN.
// @since unreleased
float32.from(x: S) -> Float32

// some(x as Float32) when Float32 holds x exactly, else none (a rounded value, NaN).
// @since unreleased
float32.from_checked(x: S) -> Option[Float32]
```

## API 说明译文（按上述签名顺序）

- `float32.to_string(x: Float32) -> String`：f32 可往返转换的最短数字串，无指数；0.1保持0.1，2.0保留 .0。
- `float32.compare(a: Float32, b: Float32) -> Int`：按 IEEE 754 全序返回 -1、0或1：-0.0 < 0.0，NaN 高于 inf，NaN 等于 NaN。
- `float32.min(a: Float32, b: Float32) -> Float32`：较小者；忽略 NaN 操作数；-0.0 < 0.0。
- `float32.max(a: Float32, b: Float32) -> Float32`：较大者；忽略 NaN 操作数；0.0 > -0.0。
- `float32.clamp(x: Float32, lo: Float32, hi: Float32) -> Float32`：将 x 限制在 lo..hi；lo > hi 或边界为 NaN 时中止。
- `float32.is_nan(x: Float32) -> Bool`：仅对 NaN 为 true。
- `float32.is_infinite(x: Float32) -> Bool`：对 inf 和 -inf 为 true，对 NaN 为 false。
- `float32.is_finite(x: Float32) -> Bool`：除非 x 为 inf、-inf 或 NaN，否则为 true。
- `float32.abs(x: Float32) -> Float32`：绝对值；-0.0得到0.0，NaN 保持 NaN。
- `float32.floor(x: Float32) -> Float32`：向 -inf 舍入；-0.5得到 -1.0。
- `float32.ceil(x: Float32) -> Float32`：向 +inf 舍入；-0.5得到 -0.0。
- `float32.round(x: Float32) -> Float32`：最近的整数值；中点向远离零的方向舍入。
- `float32.trunc(x: Float32) -> Float32`：向0舍入；-0.5得到 -0.0。
- `float32.signum(x: Float32) -> Float32`：按符号位返回1.0或-1.0（0.0得到1.0，-0.0得到-1.0）；NaN 得到 NaN。
- `float32.to_bits(x: Float32) -> Int`：将 IEEE-754 binary32 的32位作为非负 Int 返回；任何 NaN 都得到0x7fc00000。
- `float32.from_bits(bits: Int) -> Float32`：binary32 位为 bits 的低32位的 Float32。
- `float32.infinity() -> Float32`：正无穷。
- `float32.nan() -> Float32`：唯一的 quiet NaN（位为0x7fc00000）。
- `float32.epsilon() -> Float32`：1.0与下一个 Float32 的间隔：2^-23。
- `float32.max_value() -> Float32`：最大的有限 Float32，约3.4028235e38。
- `float32.min_value() -> Float32`：最负的有限 Float32（= -max_value()），不是最小正数。
- `float32.parse(s: String) -> Result[Float32, String]`：将去除两侧空白的 s 解析为 Float32，语法同 float.parse；无效内容返回 err，溢出得到 inf。
- `float32.from(x: S) -> Float32`：将 x 舍入一次至最近的 Float32（中点到偶数）；NaN 保持 NaN。
- `float32.from_checked(x: S) -> Option[Float32]`：若 Float32 能精确容纳 x，则 some(x as Float32)，否则（被舍入的值、NaN）为 none。
