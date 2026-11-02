原文 description 元数据：其他语言如何检查整数字面量范围，以及 Almide 应达到的目标
<a id="integer-literal-domain--cross-language-survey-and-almides-target"></a>

# 整数字面量的域 — 跨语言调查与 Almide 的目标

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](integer-literal-domain.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](integer-literal-domain.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](integer-literal-domain.zh-CN.md)

> 固定原文的非规范读者快照；原文仍为权威，不自动同步。未独立验证实现或执行引用指令。历史状态、计划及比较主张保留原范围。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/integer-literal-domain.md)

> 原文注记：测量、版本及实现状态是原作者 2026-07-26 的记录，不是新运行或当前语言调查。独创性、先例与诊断优越性的说法限定为历史作者主张；此处未测量 MSR 改善。临时接受的 UInt64 范围不是其声明的数学范围。未独立核查待办 carrier 扩宽及旧 radix 限制的当前状态。OCaml native int 位宽观测依赖平台，不是普遍位宽保证。保留诊断块中的日文注释在块外提供译文。


2026-07-26 引入 C-173（整数字面量的类型域）时发现：在 `UInt64` 声明域的上半区，**两个 target 会以相同方式出错**（#872）。本文记录了在实机上确认“其他语言如何处理”的结果，以及由此导出的 Almide 目标。

调查不依赖记忆，**所有项目均在本地执行**。版本为 Rust 1.94.1 / Go 1.26.1 / Swift 6.1.2 / Zig 0.15.2 / .NET 10.0.105 / Apple clang 17.0.0 / javac 26 / Python 3.11.9 / OCaml 5.4.1。

<a id="実測マトリクス"></a>

## 实测矩阵

| 语言 | `u64 = u64::MAX` | `u64 = -5` | `u64 = -0` | `u64 = u64::MAX+1` |
|---|---|---|---|---|
| Rust | ✅ 正确 | ❌ E0600 | ❌ E0600 | ❌ out of range |
| Go | ✅ 正确 | ❌ overflows | ✅ `0` | ❌ overflows |
| Swift | ✅ 正确 | ❌ overflows into unsigned | ✅ `0` | ❌ |
| Zig | ✅ 正确 | ❌ cannot represent | ❌ **ambiguous** | ❌ |
| C# | ✅ 正确 | ❌ CS0031 | ✅ `0` | ❌ |
| C | ✅ 正确 | ⚠️ **通过**（回绕为 `…611`） | ✅ `0` | — |
| Java | 无此类型 | 无此类型 | 无此类型 | ❌ 整数过大 |
| **Almide（修复前）** | ⚠️ 通过后为 **`0`**，`/` 后为 **`-1`** | ⚠️ **通过**后 rustc 报 E0600 | ✅ `0` | ⚠️ 通过后为 `0` |
| **Almide（C-173 后）** | ❌ E024 carrier | ❌ E024 sign | ✅ `0` | ❌ E024 magnitude |

<a id="軸1--符号なし型に負リテラル"></a>

## 轴 1 — 无符号类型中的负字面量

**除 C 外所有语言均拒绝。** 只有 C 以隐式 mod 2⁶⁴ 回绕接受；若不加 `-Wsign-conversion`，甚至不会警告（加上会警告，实测 2 条）。这是著名的易错点，后来的语言一致封堵了该漏洞。

修复前 Almide 是“C 的行为 + 后续 rustc 失败”，**甚至不是 C**。

解释拒绝理由有 **2 种方式**。

- **从运算符解释**（Rust）— `cannot apply unary operator '-' to type 'u64'`。结构上准确：`-5` 是 `Neg::neg(5)`，而 `u64` 没有 `Neg` 实现。但**完全不告诉你如何修复**。
- **从值域解释**（Go / Swift / Zig / C#）— 表示类型无法表示该值。Zig 的 `type 'u64' cannot represent integer value '-5'` 最简洁。

Almide 采用**值域方式**，并进一步**点名相同位宽的有符号类型**：

```
error[E024]: integer literal '-5' is out of range for UInt64
  hint: UInt64 is unsigned — it has no negative values at all; drop the '-',
        or use the signed Int64 if the value can go below zero
```

在所调查范围内，**其他语言都不点名替代类型**。按 mission（LLM 能最准确编写的语言）来看，明确修复候选有助于 modification survival rate，因此这里有意比其他语言再多走一步。

<a id="軸2--キャリア問題本丸"></a>

## 轴 2 — carrier 问题（核心）

**Almide 的情况没有先例。** 原因很简单：

> 设计合理的语言**不会让字面量内部表示比目标类型更窄**。

| 策略 | 语言 | 实测依据 |
|---|---|---|
| 任意精度 | Go, Zig, Python | `(1 << 100) >> 100` 返回 `1`（保留超出机器类型的中间值）。Go 规范要求实现**至少用 256bit 表示整数常量** |
| 固定，但不窄于最宽类型 | Rust (`u128`), C (≥ `uintmax_t`), C#, Swift | `u64::MAX` 正确 |
| **只有有符号 carrier + 无无符号类型** | **Java, OCaml** | 通过**操作而非类型**提供无符号解释（见下） |

Java 和 OCaml 很重要。**carrier 有符号本身并非缺陷。** 两者与 carrier 一致，均**以函数**提供无符号运算（实测）：

```
Java    Long.toUnsignedString(-1L)          → 18446744073709551615   ✅
OCaml   Int64.unsigned_div (-1L) 2L         → 9223372036854775807    ✅
OCaml   Printf.printf "%Lu" (-1L)           → 18446744073709551615   ✅
```

OCaml 更彻底：**native `int` 为 63bit**（因为带标签；`Sys.int_size = 63`、`max_int = 4611686018427387903`，实测）。它不自称 64bit，需要时让用户显式使用 `Int64` 模块。**不声明无法承载的位宽**，正是 Almide 缺少的态度。

也就是说，Almide 的缺陷不是“i64 carrier”，而是**声明类型与 carrier 不匹配**。自称 `UInt64` 类型，却无法承载其域，这才是问题。而 `uint64.to_string` 经由 `int.from_uint64` 按有符号显示，甚至没有达到 Java/OCaml 正确实现的“通过操作提供无符号”的水平。

<a id="帰結-着地点は-2-つある"></a>

### 推论：有 2 个目标方向

**(1) 扩宽 carrier（Rust 模型）** — 将 `IrExprKind::LitInt { value: i64 }` 扩宽至相当于 `i128`，在 MIR 中加入 `NTy::U64` 和 `div_u`/`rem_u`/`lt_u`，去除 `uint64.to_string` 的 i64 中转。`UInt64` 就符合声明。

**(2) 放弃无符号 64bit“类型”（Java / OCaml 模型）** — 与 i64 carrier 保持一致，通过 `uint64.*` 函数提供无符号解释。成本低，但剥夺现有用户的类型。**当前甚至不是 (2)** — `uint64.to_string` 按有符号显示，尚未达到 `Long.toUnsignedString` / `Int64.unsigned_div` 的水平。

**推荐 (1)。** 拥有 `UInt64` 类型的语言无一例外采用 (1)，而采用 (2) 的 Java / OCaml **本来就没有这种类型**。既然声明类型，就有责任承载其域。若选择 (2)，必须连同 `UInt64` 类型一起撤除，否则同样的虚假承诺仍然存在。

<a id="872-への訂正"></a>

### 对 #872 的纠正

issue #872 写了“向 MIR 添加 `NTy::U64`”，但仅如此**不能修复**。字面量到达 MIR 前就在 i64 carrier 中损坏，所以顺序是：

1. **扩宽字面量表示**（这是根本，与 Rust 的 `LitKind::Int` = `u128` 相同）
2. MIR 无符号运算（`div_u` / `rem_u` / `lt_u`，按操作数类型选择）
3. 从 `uint64.to_string` / `to_float*` 去除 i64 中转
4. 撤除 E024 的 carrier 上限，恢复 E024.md 和 llms.txt 对 `UInt64` 的推荐

没有 1，先单独做 2 没有意义。

临时上限是“其他语言没有先例的异常状态”，因此也是**不应长期保留它的理由**。

<a id="軸3---0-は本当に三分する"></a>

## 轴 3 — `-0` 确实分成三派

只有这里业界存在分歧，Almide 的选择需要理由。

| 行为 | 语言 |
|---|---|
| **接受**为 `0` | Go, Swift, C#, C |
| **拒绝**（作为运算符） | Rust |
| **拒绝**（有歧义，即使有符号类型也如此） | Zig |

Zig 很独特，即使对 `i64` 也说 `integer literal '-0' is ambiguous`（实测）。它的理由不同：无法区分整数 `0` 和浮点数 `-0.0`。

Rust 拒绝是**轴 1 采用运算符方式的自然结果**，并非特别处理 `-0`。

**Almide 继续接受。** 不是因为多数语言如此，而是**轴 1 选择了值域方式**：

> E024 的规则是“该类型能否表示此值”。`UInt64` 能表示 `0`，因此拒绝 `-0` 就与规则本身矛盾。

要拒绝，就需改用运算符方式，而轴 1 已放弃了它（因为不说明修复方法）。**方式的一致性优先于追随 Rust。**

一度也考虑过“无例外规则更容易让 LLM 记住”= 拒绝，但这误解了规则的制定方式。Almide 的规则不是“无符号不能用 `-`（例外：0）”，而是“不允许类型不能表示的值”，所以**没有例外**。

<a id="軸4--診断の質-値域を書く"></a>

## 轴 4 — 诊断质量：写明值域

Rust **在注记中明确写出范围**：

```
error: literal out of range for `u64`
  = note: the literal `18446744073709551616` does not fit into the type `u64`
          whose range is `0..=18446744073709551615`
```

Almide 的 magnitude 提示只说 “use a literal within the type's range”（使用类型范围内的字面量），**却没说范围是什么**。能背出 `UInt32` 上限的读者（人或 LLM）不多。写出范围成本低，直接服务于 mission。

→ 已实施。E024 的 magnitude 提示会给出具体范围。

<a id="almide-がこの-survey-から採った--採らなかったもの"></a>

## Almide 从调查中采用 / 未采用的做法

| 项目 | 取舍 | 来源 |
|---|---|---|
| 拒绝无符号类型的负字面量 | 采用 | Rust/Go/Swift/Zig/C# 全部 |
| 用值域方式解释 | 采用 | Go/Swift/Zig/C#（不采用 Rust 运算符方式） |
| 点名替代有符号类型 | **独创** | 其他语言都不这样做 |
| 提示中写明具体值域 | 采用 | Rust |
| 接受 `-0` | 采用 | Go/Swift/C#（不采用 Rust/Zig，理由见轴 3） |
| 将 carrier 扩宽至至少最宽类型 | **未完成 → #872** | Rust/Go/Zig/C/C#/Swift 全部 |
| 不设无符号类型，改用函数提供 | 不采用 | Java（既然声明类型就须承载其域） |

<a id="付録--各言語の実測メッセージ全文"></a>

## 附录 — 各语言实测消息全文

设计诊断时的一手资料，均使用上述版本在本地执行。

<a id="符号なし型に負リテラルu64---5"></a>

### 无符号类型中的负字面量（`u64 = -5`）

```
Rust   error[E0600]: cannot apply unary operator `-` to type `u64`
Go     cannot use -5 (untyped int constant) as uint64 value in variable declaration (overflows)
Swift  error: negative integer '-5' overflows when stored into unsigned type 'UInt64'
Zig    error: type 'u64' cannot represent integer value '-5'
C#     error CS0031: Constant value '-5' cannot be converted to a 'ulong'
C      （通る。-Wsign-conversion で警告 2 件。値は 18446744073709551611）
Almide error[E024]: integer literal '-5' is out of range for UInt64
       hint: UInt64 is unsigned — it has no negative values at all; drop the '-',
             or use the signed Int64 if the value can go below zero
```

保留块中 C 行日文译文：通过；使用 -Wsign-conversion 时有 2 条警告，值为 18446744073709551611。

Zig 最简洁，Swift 解释最充分。**只有 Almide 提供修复候选类型。**

<a id="型の域を超える大きさu64--u64max--1"></a>

### 超出类型域的大小（`u64 = u64::MAX + 1`）

```
Rust   error: literal out of range for `u64`
       = note: the literal `18446744073709551616` does not fit into the type `u64`
               whose range is `0..=18446744073709551615`
       = note: `#[deny(overflowing_literals)]` on by default
Go     cannot use 18446744073709551616 (untyped int constant) as uint64 value in variable declaration (overflows)
Swift  error: integer literal '18446744073709551616' overflows when stored into 'UInt64'
Zig    error: type 'u64' cannot represent integer value '18446744073709551616'
C#     error CS1021: Integral constant is too large
Java   エラー: 整数が大きすぎます。      ← long のみ。符号なし型が無い
Almide error[E024]: integer literal '18446744073709551616' is out of range for UInt64
       hint: UInt64 would silently fold to 0 here; its range is 0..=9223372036854775807,
             so use a literal within it, or model larger magnitudes as Float (lossy)
             or a parsed string
```

Java 行日文译文：错误：整数过大。只有 long，没有无符号类型。

**只有 Rust 和 Almide 明确写出范围。** 其他都止于“太大”。Almide 给出的 `UInt64` 范围不是声明域，而是**实际能通过的范围**（#872 上限之后）。在诊断中写出编译器不遵守的范围，比不写范围更糟。

<a id="-0"></a>

### `-0`

```
Rust   error[E0600]: cannot apply unary operator `-` to type `u64`
Zig    error: integer literal '-0' is ambiguous        ← i64 に対しても同じ
Go     0
Swift  0
C#     0
Almide 0
```

Zig 行日文注释译文：对 i64 也相同。

<a id="関連"></a>

## 相关

- 契约 C-173 — `docs/contracts/contracts.toml`；规范依据为 ALS-M14
- #872 — `UInt64` 的上半区（本文轴 2 决定修复顺序）
- #873 — lexer 基数字面量（单独 `0x` 默默成为 0，`0b`/`0o` 尚不支持）
- `docs/diagnostics/E024.md` — 3 种偏离与 3 个提示
- `tests/int_literal_domain_test.rs` — 固定分类
