> 说明（固定历史）：标准库重设计 L2/L3 — float：转换移至来源类型，to_bits/from_bits 成对，新增 IEEE 常量、trunc、is_finite、pow。
<a id="l2l3-float"></a>

# L2/L3: float

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-float.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-float.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-float.zh-CN.md)

> 非规范性历史重设计参照版。固定原本权威且不变。不自动同步、重取官方页面或独立进行数值/CPU/runtime测试。(F)与版本probe为原作者报告。保留来源侧转换提案与后来目标侧表、饱和版移除撤销、sqrt仅float与表中迁往math、声称新增11与后续行的差异。R-float-09 NaN彼此相等是排序约定，不是语言相等运算符的新保证。R-float-06仍待定，x86 native仍未实测。日文占位符指类型/来源。历史totalOrder/规范化及按宽度payload丢失约束不构成当前实现认证。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-float.md)

> 最后更新：2026-10-21
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)・[L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)
> 相关：[L2-sized-numbers](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md)（转换矩阵及 float32/float64）・[L2-math](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-math.md)・[L2-int](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-int.md)
> 来源：Rust https://doc.rust-lang.org/std/primitive.f64.html (F) ・ Go https://pkg.go.dev/math ・ Python https://docs.python.org/3/library/math.html ・ TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Number ・ Gleam https://gleam-stdlib.hexdocs.pm/gleam/float.html (F) ・ Elixir https://hexdocs.pm/elixir/Float.html ・ OCaml https://ocaml.org/manual/latest/api/Float.html (F) ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin/-double/ ・ Swift https://developer.apple.com/documentation/swift/double

(F)＝2026-10-21 获取官方页面核对拼写的单元格（Rust f64 常量、方法及 Gleam float、OCaml Float 函数清单）。

实测（`almide 0.68.0 dev 437b8f127`，native 与 wasm 相同）：`1.0 / 0.0` 为 `inf`，`float.min(0.0 / 0.0, 1.0)` 为 `1.0`（忽略 NaN），`float.sign(0.0)` 为 `1.0`，`float.to_int8(0.0 / 0.0)` 为 `0`。无穷与 NaN 无命名写法，只能写 `1.0 / 0.0`（NaN 为 `0.0 / 0.0`）。`float.parse` 仅返回有限值，`"inf"`、`"NaN"`、`"1e400"` 等溢出返回 err（#4121、C-413）。

<a id="l2-役割の比較"></a>

## L2：职责比较

| 职责 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 转为/来自字符串 | `to_string` `parse` | `to_string` `parse` | `FormatFloat` `ParseFloat` | `str` `float` | `toString` `parseFloat` | `to_string` `parse` (F) | `to_string` `parse` | `to_string` `of_string_opt` (F) | `toString` `toDoubleOrNull` | `String` `Double(s)` |
| 指定小数位转字符串 | `to_fixed` | `format!("{:.2}")` | `FormatFloat(f,'f',2,64)` | `f"{x:.2f}"` | `toFixed` | `to_precision`（Float）(F) | `Float.round` | `Printf` | `"%.2f".format` | `String(format:)` |
| 转 Int（截断） | `to_int` | `as i64` | `int64(f)` | `int(f)` | `Math.trunc` | `truncate` (F) | `trunc` | `to_int` (F) | `toLong` | `Int(f)` |
| 来自 Int | `from_int`（`int.to_float` 的重复项） | `as f64` | `float64(n)` | `float(n)` | — | `int.to_float` (F) | — | `of_int` (F) | `toDouble` | `Double(n)` |
| 位序列转换 | `to_bits` 与 `int.bits_to_float` | `to_bits` `from_bits` (F) | `Float64bits` `Float64frombits` | `struct` | `DataView` | — | — | `Int64.bits_of_float` | `toBits` `fromBits` | `bitPattern` `init(bitPattern:)` |
| 取整（向下、向上、四舍五入） | `floor` `ceil` `round` | `floor` `ceil` `round` (F) | `Floor` `Ceil` `Round` | `math.floor` 等 | `Math.*` | `floor` `ceiling` `round` (F) | `floor` `ceil` `round` | `floor` `ceil` `round` (F) | `floor` `ceil` `round` | `rounded(_:)` |
| 向零取整（保持 Float） | — | `trunc` (F) | `Trunc` | `math.trunc` | `Math.trunc` | `truncate`（返回 Int）(F) | — | `trunc` (F) | `truncate` | `rounded(.towardZero)` |
| 绝对值、最小、最大、范围 | `abs` `min` `max` `clamp`（`math.fmin`/`fmax` 为重复项） | `abs` `min` `max` `clamp` (F) | `Abs` `min` `max` | `abs` `min` `max` | `Math.*` | `absolute_value` `min` `max` `clamp` (F) | `abs` `min` `max` | `abs` `min` `max` (F) | `absoluteValue` `coerceIn` | `abs` `min` `max` |
| 平方根、幂 | `sqrt`（`math.sqrt` 为重复项）；幂为 `math.fpow` | `sqrt` `powf` (F) | `Sqrt` `Pow` | `math.sqrt` `math.pow` | `Math.sqrt` `Math.pow` | `square_root` `power` (F) | `:math.sqrt` `:math.pow` | `sqrt` `pow` (F) | `sqrt` `pow` | `squareRoot()` `pow` |
| 符号 | `sign` | `signum` (F) | — | `math.copysign` | `Math.sign` | — | — | `sign_bit` (F) | `sign` | `sign` |
| NaN、无穷判定 | `is_nan` `is_infinite` | `is_nan` `is_infinite` `is_finite` (F) | `IsNaN` `IsInf` | `isnan` `isinf` `isfinite` | `isNaN` `isFinite` | — | — | `is_nan` `is_infinite` `is_finite` (F) | `isNaN` `isInfinite` `isFinite` | `isNaN` `isInfinite` `isFinite` |
| 无穷、NaN、机器 epsilon、最大值 | — | `INFINITY` `NAN` `EPSILON` `MAX` `MIN` (F) | `Inf` `NaN` `MaxFloat64` | `math.inf` `math.nan` `sys.float_info` | `Infinity` `NaN` `EPSILON` `MAX_VALUE` | — | — | `infinity` `nan` `epsilon` `max_float` (F) | `POSITIVE_INFINITY` `NaN` `MAX_VALUE` | `.infinity` `.nan` `.ulpOfOne` `.greatestFiniteMagnitude` |
| 与固定宽度类型转换 | `to_<型>`・`_checked`・`_saturating` 与 `from_float32`/`from_float64` | `as` | `T(x)` | — | — | — | — | — | `toInt()` 等 | `init(exactly:)` 等 |

<a id="完全性の規則"></a>

### 完备性规则

1. **转换矩阵**遵循 [L2-sized-numbers](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md)。`float` 仅放“从 `Float` 出发”的转换，进入 `Float` 的转换放在来源类型的 `to_float`（R-L0-07）。`Float` 转整数有损，故提供 `to_T`、`to_T_checked`。`to_T` 本身与 Rust `as` 一样向零截断并饱和（NaN 为 0），因此不提供 `to_T_saturating`（R-L0-01：仅调用另一公开函数一次的是同义词，R-sized-04）。`to_int` 也遵循此规则。
2. **浮点载体**（`float`、`float32`、`float64`）都有相同七格：`to_bits`、`from_bits`、`infinity`、`nan`、`epsilon`、`max_value`、`min_value`。`min_value` 与 Rust `MIN` 相同，是“最小有限值（＝`-max_value()`）”。doc 首行注明与 `Kotlin`/`JS` 的 `MIN_VALUE`（最小正数）不同。
3. **不放类型重复项**：`sqrt`、`min`、`max`、`pow` 仅放 `float`（[R-math-01](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-math.md)）。
4. **有序类型提供 `compare`**（R-L0-16）：`float.compare(a, b) -> Int`（负、0、正）。顺序为 IEEE 754 totalOrder（同 `list.sort`，`-0.0 < 0.0`，NaN 大于正无穷），仅在 NaN、`-0.0` 的处理上与 `<` 运算符（涉及 NaN 的比较为 false）不同。新增属于 [R-sized-02](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md) 一格。
5. 常量采用无参数函数（`float.infinity()`），同现有 `int.min_value()`、`math.pi()`。

<a id="l2-の判定"></a>

### L2 判定

- 缺少职责：向零取整 `trunc`（8/9）、`is_finite`（7/9）、无穷/NaN/epsilon/最大值常量（7/9）→ R-float-03、R-float-04。幂从 `math.fpow` 移入 → R-math-01。
- 重复：`float.from_int` 与 `int.to_float`（R-L0-07 第 3 条），`float.from_float32`/`from_float64` 与待新增 `float32.to_float`/`float64.to_float` → R-float-01。`int.bits_to_float` 是 `float.to_bits` 的逆向却在 `int` → R-float-02。
- family 缺口：缺 `float.to_int_checked`（`to_int8` 等已有）→ [R-sized-01](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md)。缺 `compare` → [R-sized-02](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md)。
- 同义词：`float.to_int8_saturating` 等八函数仅调用 `float.to_int8` 等一次（`tests/stdlib_synonym_gate_test.rs:47-59` 允许列表放行）。原拟按 [R-sized-04](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md) 移除，但 R-L0-32 使无后缀版改为 abort，因此撤销。

<a id="案"></a>

## 提案

<a id="r-float-01-floatfrom_intfrom_float32from_float64-を撤去する---採用"></a>

### R-float-01 移除 `float.from_int`、`from_float32`、`from_float64` — ○ 采纳

- 内容：移除三个，替代为 `int.to_float`（已有）、`float32.to_float`、`float64.to_float`（R-sized-01 新增）。
- 依据的 L0 规则：R-L0-07 第 1、3 条（L0 已点名移除 `float.from_int`）、R-L0-01。
- (a) Gleam 只有 `int.to_float`，无 `float.from_int` (F)。OCaml `Float.of_int` 放在目标侧，但多数（5/9）放来源侧。
- (b) 相同 intrinsic，两 target 不变。
- (c) spec 同时出现 `int.to_float`、`float.from_int`，LLM 两种都会写；移除一方后可用提示修正。
- (d) 移除。`@deprecated(since=15, use="int.to_float")` 等，removal epoch 16。`almide fix`：`float.from_int(x)` → `int.to_float(x)`，`float.from_float32(x)` → `float32.to_float(x)`，`float.from_float64(x)` → `float64.to_float(x)`。使用数：`from_int` 24/11/11/0（＋26 research）、`from_float32` 4/2/6/29、`from_float64` 7/0/6/23（stdlib 数量为 float32/float64 模块主体）。重写 llms.txt:420。
- 门禁提案：R-sized-01 门禁（将浮点载体纳入矩阵），也检查 `float.from_<T>` 不存在。
- issue：#3935

<a id="r-float-02-ビット列の変換を-to_bits--from_bits-の対にそろえる---採用"></a>

### R-float-02 位序列转换对齐为 `to_bits` / `from_bits` 一对 — ○ 采纳

- 内容：新增 `float.from_bits(bits: Int) -> Float`，移除 `int.bits_to_float`。移除 `int.bits_to_f32`（低 32 bit 按 binary32 读取并扩为 Float），改用 `float32.from_bits(bits) |> float32.to_float`（`float32.from_bits` 属 R-sized-03）。
- 依据的 L0 规则：R-L0-07 第 2 条（从表示构造值放在目标模块 `from_<元>`）、R-L0-01。
- (a) Rust (F) `from_bits`、Kotlin `Double.fromBits`、Swift `init(bitPattern:)`、Go `Float64frombits` 均在 Float 侧。
- (b) 调用相同 intrinsic（`almide_rt_int_bits_to_float`），两 target 相同。
- (c) 看到 `float.to_bits` 的 LLM 会写 `float.from_bits`；目前它未定义，无法从名称找到 `int.bits_to_float`。
- (d) 新增 1、移除 2。`@deprecated(since=15, use="float.from_bits")`，removal epoch 16。`almide fix`：`int.bits_to_float(b)` → `float.from_bits(b)`，`int.bits_to_f32(b)` → `float32.to_float(float32.from_bits(b))`。使用数：`bits_to_float` 26/10/8/3（＋8 research）、`bits_to_f32` 5/3/5/1（llms.txt:388）、`to_bits` 30/4/20/8。
- 门禁提案：浮点载体七格（完备性规则 2）纳入 R-sized-01。在 NaN 以外边界值检查 `from_bits(to_bits(x)) == x` 的 spec。
- issue：#3910

<a id="r-float-03-ieee-の定数-infinitynanepsilonmax_valuemin_value-を足す---採用"></a>

### R-float-03 新增 IEEE 常量 `infinity`、`nan`、`epsilon`、`max_value`、`min_value` — ○ 采纳

- 内容：`float.infinity() -> Float`、`float.nan() -> Float`（规范 quiet NaN，通过 `to_bits` 得到与 `float.to_bits` 相同值）、`float.epsilon() -> Float`（2^-52）、`float.max_value() -> Float`、`float.min_value() -> Float`（＝`-max_value()`）。`float32`、`float64` 也提供相同格（R-sized-03）。
- 依据的 L0 规则：L1 新增标准（7/9 有命名形式）、R-L0-02（返回类型模块）、R-L0-08 矩阵思路（整数载体均有 `min_value`/`max_value`，浮点也对齐）。
- (a) Rust、Go、Python、JS、OCaml、Kotlin、Swift 七语言具备；Gleam、Elixir 不具备（Elixir 无法表示无穷）。`min_value` 含义各异（Rust `MIN` 为最小有限值，Kotlin/JS `MIN_VALUE` 为最小正数）。Almide 对齐整数 `min_value` 的“最小值”。
- (b) 纯常量。`float.to_bits` 已规范化 NaN 位，两 target 相同（ADR-0016）。
- (c) 目前只能写 `1.0 / 0.0`，LLM 会写未定义的 `float.infinity`、`math.inf`。
- (d) 新增，使用数 0。
- 门禁提案：R-sized-01 检查七格存在，以 `spec/stdlib/float_consts_test.almd` 固定值（`is_infinite(infinity())`、`is_nan(nan())`、`1.0 + epsilon() > 1.0`）。
- issue：#3910

<a id="r-float-04-trunc-と-is_finite-を足す---採用"></a>

### R-float-04 新增 `trunc`、`is_finite` — ○ 采纳

- 内容：`float.trunc(n: Float) -> Float`（向零取整，`-0.5` 为 `-0.0`）、`float.is_finite(n: Float) -> Bool`（NaN 与 ±inf 为 false）。
- 依据的 L0 规则：L1 新增标准（`trunc` 8/9、`is_finite` 7/9）、R-L0-09（返回 Bool，故 `is_`）。
- (a) 拼写：Rust、Go、Python、JS、OCaml 用 `trunc`；Rust、OCaml 用 `is_finite`，JS、Kotlin、Swift 用不分隔的 `isFinite`。
- (b) 纯计算，wasm 有 `f64.trunc` 指令。
- (c) 提供替代，避免用 `float.floor` 截负数导致差一错误。`is_finite` 免去每次写 `not is_nan(x) and not is_infinite(x)`。
- (d) 新增，使用数 0。
- 门禁提案：spec 放负数、`-0.0`、NaN、inf 行。
- issue：#3911

<a id="r-float-05-いまの-floatsign-を-floatsignum-に改名する---採用"></a>

### R-float-05 当前 `float.sign` 改名 `float.signum` — ○ 采纳

- 当前：`fn float.sign(n: Float) -> Float` 与 Rust `signum` 同行为，`sign(0.0) = 1.0`、`sign(-0.0) = -1.0`（C-049）。而 `int.sign(0)`、`bigint.sign` 返回 0。同名 `sign` 因类型不同对零处理不同。
- 提案：保持行为，改名 `float.signum`，移除 `float.sign`。行为不变，C-049 只修改函数名。
- 依据的 L0 规则：R-L0-00 第 1 条（同动词因模块而异）、R-L0-14.4（不在同名下改变含义，迁移新名）。
- (a) JS `Math.sign` (F)、Kotlin `sign` (F) 将零返回为零。只有 Rust `f64::signum` 返回 1.0，其拼写为 `signum`。
- (c) 可避免 `float.sign(dx) * speed`（dx 为 0 时本想停止）悄悄得到不同值。`signum` 是 Rust 拼写，名称体现行为差异。
- (d) 改名。`@deprecated(since=15, use="float.signum")`，epoch 16 移除，`almide fix` 一对一改写。先改 als C-049 文中的函数名。
- 完备性规则：所有数值类型名为 `sign` 的函数都“0 返回 0”。
- issue：#3972

<a id="r-float-06-floatminmax-の-nan-の扱いを-listminmax-とそろえる--保留"></a>

### R-float-06 将 `float.min`/`max` 的 NaN 处理对齐 `list.min`/`max` — 待定

- 当前：`float.max(NaN, 1.0) = 1.0`（忽略 NaN，C-049），`list.max([NaN, 1.0]) = NaN`（totalOrder）。
- 待定理由：传播 NaN（Go、JS、Kotlin、OCaml）与忽略 NaN（Rust、Swift）均未达五语言，且涉及改变 C-049、C-306 固定行为。R-L0-14.4 不允许同名只改变 NaN 输入结果，若改变须新名称。为判定，使用 almide-dojo（与 [almide-dojo#63](https://github.com/almide/almide-dojo/issues/63) 同框架）测量 LLM 多常踩到含 NaN 的 `fold(…, float.max)` 与 `list.max` 差异。
- issue：无（依据测量结果建 issue）

<a id="r-float-07-round-と-to_fixed-の同点の丸めの違いを文書とゲートで固定する---採用関数の面は変えない"></a>

### R-float-07 用文档、门禁固定 `round` 与 `to_fixed` 的舍入中点差异 — ○ 采纳（API 面不变）

- 当前：`float.round(2.5) = 3.0`（远离零），`float.to_fixed(2.5, 0) = "2"`（最接近二进制精确值，中点取偶）。JS `toFixed` 为 `"3"`。
- 提案：各函数 doc 首行写舍入规则，CHEATSHEET float 行补一行。`spec/stdlib` 用 `round`、`to_fixed` 固定 `0.5`、`1.5`、`2.5`、`-0.5`、`0.125`。
- (d) 仅文档、测试。
- issue：#3972

<a id="r-float-08-float-の床方向の剰余-floatmod-を足す---不採用"></a>

### R-float-08 新增 Float 向下取整余数 `float.mod` — × 不采纳

- 理由：提供此 Float 余数的仅 Python `%`、Kotlin `mod`、Gleam `float.modulo` 三种，未达五语言标准。Go、JS、OCaml、Swift、Elixir 使用向零余数。
- issue：无

<a id="r-float-09-nan-は-1-つの値にそろえ順序の約束を正確に書く---採用"></a>

### R-float-09 NaN 统一为一个值，精确描述顺序约定 — ○ 采纳

- 当前：共同约定原为“IEEE 754 totalOrder（NaN 大于正无穷）”。totalOrder 中符号位为 1 的 NaN 位于负无穷之前。实测（`almide 0.68.0 dev 437b8f127`，native 与 wasm 相同）：
  - 从字节读取的负 NaN（`bytes.read_f64_le` 读取 `00 00 00 00 00 00 f8 ff`）与 `0.0 / 0.0` 经 `list.sort`，一个在开头、一个在末尾。
  - `float.to_bits`、`bytes.append_f64_le` 把 NaN 统一为一个正值（`0x7ff8000000000000`）输出，因此负 NaN 位不往返。
  - `0.0 / 0.0` 符号位因 CPU 不同（x86 负、ARM 正）。这台 Mac（ARM）为正；尚未测 x86 native。
- 提案：NaN 统一为一个值（正 quiet NaN，`0x7ff8000000000000`）。运算结果、`bytes.read_f*`、`get_f*`、`next_f*`、`float.from_bits` 产生 NaN 时转为此值。
  - 顺序约定为“`-0.0 < 0.0`，NaN 大于正无穷，NaN 彼此相等”，因 NaN 只有一个而精确成立。
  - NaN 位不往返（读入任何 NaN，输出均为 `0x7ff8000000000000`），写入契约。
  - 与数学函数约定（“NaN 为一个确定值”，ADR-0016）同处理。
- 依据：R-L0-00（同值统一处理）、ADR-0016。
- (a) JavaScript NaN 有一个可观察值（部分引擎通过 `Float64Array` 读取不规范化）。Rust `total_cmp` 区分符号。Almide 约定两 target、所有 CPU 同结果，故选择统一。
- (b) 两 target、所有 CPU 顺序相同。
- (d) 语义变化（负 NaN 与 NaN 的排列改变）。标准 API 无观察 NaN 符号的代码。
- issue：#3990

<a id="r-float-10-nan-を型の幅ごとに-1-つにしビットの読み書きでも同じにすることを明記する---採用r-float-09-の補い"></a>

### R-float-10 明确每种宽度仅一个 NaN，位读写同样统一 — ○ 采纳（补充 R-float-09）

- 当前：R-float-09 规定统一为 `0x7ff8000000000000`，却只写 64 bit 形式，不适用于 `float32` 位、字节读写（`float32.to_bits`、`from_bits`、`bytes.append_f32_le`）。也未在约定写明经 `bytes` 读写与 `to_bits` 后 NaN payload 消失。
- 提案：
  - 每种类型宽度仅一个 NaN：`Float`、`Float64` 为 `0x7ff8000000000000`，`Float32` 为 `0x7fc00000`（均为正 quiet NaN）。
  - `to_bits`、`from_bits` 以及 `bytes` 的 `get_f32_le`、`get_f64_le`、`append_f32_le`、`append_f64_le` 等也使 NaN 成为同一个值，不保留读入 NaN 的 payload。
  - 若要原样往返含 NaN payload 的字节（wasm 字节、msgpack 等），不按浮点读取，用 `get_u32_le`、`get_u64_le` 按整数读写。
- 依据的 L0 规则：R-float-09。
- (a) Java `Float.floatToIntBits` 也统一为 `0x7fc00000`（`floatToRawIntBits` 不统一）。
- (b) 两 target 相同（R-float-09 实测）。
- (d) 文档定案，与 R-float-09 同在 epoch 15 引入。
- issue：#3995
<a id="l3-関数表"></a>

## L3：函数表

`float` 当前 46 函数与新增 11 函数（含 `compare`）。
| 函数 | 签名（当前，标明 mut） | 其他语言主要拼写 | 判定 | 提案 |
|---|---|---|---|---|
| `to_string` | `float.to_string(n: Float) -> String` | Rust `to_string` / Go `strconv.FormatInt` / Python `str` / JS `toString` / Gleam `to_string` (F) / Elixir `to_string` (F) / OCaml `to_string` (F) / Kotlin `toString` / Swift `String(x)` | 保留 | — |
| `to_int` | `float.to_int(n: Float) -> Int` | Rust `as`（饱和） / Kotlin `toInt()`（饱和）(F) / Swift `init(_:)`（trap） / Go `T(x)` / Gleam `truncate` (F) / OCaml `to_int` (F) | 移除（→ `int.from`） | R-L0-40 |
| `from_int` | `float.from_int(n: Int) -> Float` | OCaml `Float.of_int` (F) / Gleam 为 `int.to_float` (F) / Kotlin `toDouble` | 移除 | R-float-01（`int.to_float` 相同，R-L0-07 第 3 条） |
| `parse` | `float.parse(s: String) -> Result[Float, String]` | Rust `parse` / Go `ParseFloat` / Gleam `parse` (F) / OCaml `of_string_opt` (F) / Kotlin `toDoubleOrNull` / Swift `Double(s)` | 保留 | — |
| `to_fixed` | `float.to_fixed(n: Float, decimals: Int) -> String` | JS `toFixed` / Gleam `to_precision`（返回 Float）(F) / Kotlin `"%.2f".format` / Rust `format!("{:.2}")` | 保留 | — |
| `to_bits` | `float.to_bits(f: Float) -> Int` | Rust `to_bits` (F) / Go `math.Float64bits` / Kotlin `toBits` / Swift `bitPattern` | 保留 | R-float-02（`from_bits` 配对） |
| `sqrt` | `float.sqrt(n: Float) -> Float` | Rust `sqrt` (F) / Go `math.Sqrt` / Python `math.sqrt` / JS `Math.sqrt` / Gleam `square_root` (F) / OCaml `sqrt` (F) / Kotlin `sqrt` / Swift `squareRoot()` | 移除 → `math.sqrt` | R-math-03 |
| `abs` | `float.abs(n: Float) -> Float` | Rust `abs` (F) / Go `math.Abs` / Python `abs` / JS `Math.abs` / Gleam `absolute_value` (F) / OCaml `abs` (F) / Kotlin `absoluteValue` / Swift `abs` | 保留 | — |
| `floor` | `float.floor(n: Float) -> Float` | Rust `floor` (F) / Go `math.Floor` / JS `Math.floor` / Gleam `floor` (F) / OCaml `floor` (F) / Kotlin `floor` / Swift `rounded(.down)` | 保留 | — |
| `ceil` | `float.ceil(n: Float) -> Float` | Rust `ceil` (F) / Go `math.Ceil` / JS `Math.ceil` / Gleam `ceiling` (F) / OCaml `ceil` (F) / Kotlin `ceil` / Swift `rounded(.up)` | 保留 | — |
| `round` | `float.round(n: Float) -> Float` | Rust `round`（远离零）(F) / Go `math.Round` / JS `Math.round`（向 +∞） / Gleam `round` (F) / OCaml `round` (F) / Kotlin `round`（取偶） / Swift `rounded()` | 保留 | — |
| `min` | `float.min(a: Float, b: Float) -> Float` | Rust `min` (F) / Go `min` / JS `Math.min` / Gleam `min` (F) / OCaml `min` (F) / Kotlin `minOf` / Swift `min` | 保留 | R-math-01（`math.fmin` 的保留侧） |
| `max` | `float.max(a: Float, b: Float) -> Float` | Rust `max` (F) / Go `max` / JS `Math.max` / Gleam `max` (F) / OCaml `max` (F) / Kotlin `maxOf` / Swift `max` | 保留 | R-math-01（`math.fmax` 的保留侧） |
| `clamp` | `float.clamp(n: Float, lo: Float, hi: Float) -> Float` | Rust `clamp` (F) / Gleam `clamp` (F) / Kotlin `coerceIn` | 保留 | — |
| `sign` | `float.sign(n: Float) -> Float` | Rust `signum` (F) / JS `Math.sign` / Kotlin `sign` / OCaml `sign_bit` (F)（Bool） | 改名 → `signum`（行为不变） | R-float-05 |
| `is_nan` | `float.is_nan(n: Float) -> Bool` | Rust `is_nan` (F) / Go `math.IsNaN` / Python `math.isnan` / JS `Number.isNaN` / OCaml `is_nan` (F) / Kotlin `isNaN` / Swift `isNaN` | 保留 | — |
| `is_infinite` | `float.is_infinite(n: Float) -> Bool` | Rust `is_infinite` (F) / Go `math.IsInf` / Python `math.isinf` / OCaml `is_infinite` (F) / Kotlin `isInfinite` / Swift `isInfinite` | 保留 | — |
| `to_int8` | `float.to_int8(n: Float) -> Int8` | Rust `as`（饱和） / Kotlin `toInt()`（饱和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 移除（→ `int8.from`） | R-L0-40 |
| `to_int16` | `float.to_int16(n: Float) -> Int16` | Rust `as`（饱和） / Kotlin `toInt()`（饱和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 移除（→ `int16.from`） | R-L0-40 |
| `to_int32` | `float.to_int32(n: Float) -> Int32` | Rust `as`（饱和） / Kotlin `toInt()`（饱和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 移除（→ `int32.from`） | R-L0-40 |
| `to_uint8` | `float.to_uint8(n: Float) -> UInt8` | Rust `as`（饱和） / Kotlin `toInt()`（饱和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 移除（→ `uint8.from`） | R-L0-40 |
| `to_uint16` | `float.to_uint16(n: Float) -> UInt16` | Rust `as`（饱和） / Kotlin `toInt()`（饱和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 移除（→ `uint16.from`） | R-L0-40 |
| `to_uint32` | `float.to_uint32(n: Float) -> UInt32` | Rust `as`（饱和） / Kotlin `toInt()`（饱和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 移除（→ `uint32.from`） | R-L0-40 |
| `to_uint64` | `float.to_uint64(n: Float) -> UInt64` | Rust `as`（饱和） / Kotlin `toInt()`（饱和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 移除（→ `uint64.from`） | R-L0-40 |
| `to_float32` | `float.to_float32(n: Float) -> Float32` | Rust `as` / Kotlin `toFloat()` / Swift `init(_:)` | 移除（→ `float32.from`） | R-L0-40 |
| `to_int64` | `float.to_int64(n: Float) -> Int64` | Rust `as`（饱和） / Kotlin `toInt()`（饱和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 移除（→ `int64.from`） | R-L0-40 |
| `to_float64` | `float.to_float64(n: Float) -> Float64` | Rust `as` / Kotlin `toFloat()` / Swift `init(_:)` | 移除（→ `float64.from`） | R-L0-40 |
| `from_float32` | `float.from_float32(n: Float32) -> Float` | Rust `as` / Kotlin `toFloat()` / Swift `init(_:)` | 移除 | R-float-01（`float32.to_float`） |
| `from_float64` | `float.from_float64(n: Float64) -> Float` | Rust `as` / Kotlin `toFloat()` / Swift `init(_:)` | 移除 | R-float-01（`float64.to_float`） |
| `to_int8_checked` | `float.to_int8_checked(n: Float) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` 族 / 其余 6 语言 — | 移除（→ `int8.from_checked`） | R-L0-40 |
| `to_int16_checked` | `float.to_int16_checked(n: Float) -> Option[Int16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` 族 / 其余 6 语言 — | 移除（→ `int16.from_checked`） | R-L0-40 |
| `to_int32_checked` | `float.to_int32_checked(n: Float) -> Option[Int32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` 族 / 其余 6 语言 — | 移除（→ `int32.from_checked`） | R-L0-40 |
| `to_int64_checked` | `float.to_int64_checked(n: Float) -> Option[Int64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` 族 / 其余 6 语言 — | 移除（→ `int64.from_checked`） | R-L0-40 |
| `to_uint8_checked` | `float.to_uint8_checked(n: Float) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` 族 / 其余 6 语言 — | 移除（→ `uint8.from_checked`） | R-L0-40 |
| `to_uint16_checked` | `float.to_uint16_checked(n: Float) -> Option[UInt16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` 族 / 其余 6 语言 — | 移除（→ `uint16.from_checked`） | R-L0-40 |
| `to_uint32_checked` | `float.to_uint32_checked(n: Float) -> Option[UInt32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` 族 / 其余 6 语言 — | 移除（→ `uint32.from_checked`） | R-L0-40 |
| `to_uint64_checked` | `float.to_uint64_checked(n: Float) -> Option[UInt64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` 族 / 其余 6 语言 — | 移除（→ `uint64.from_checked`） | R-L0-40 |
| `to_float32_checked` | `float.to_float32_checked(n: Float) -> Option[Float32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` 族 / 其余 6 语言 — | 移除（→ `float32.from_checked`） | R-L0-40 |
| `to_int8_saturating` | `float.to_int8_saturating(n: Float) -> Int8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其余 7 语言 — | 移除（→ `int8.from_saturating`） | R-L0-40 |
| `to_int16_saturating` | `float.to_int16_saturating(n: Float) -> Int16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其余 7 语言 — | 移除（→ `int16.from_saturating`） | R-L0-40 |
| `to_int32_saturating` | `float.to_int32_saturating(n: Float) -> Int32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其余 7 语言 — | 移除（→ `int32.from_saturating`） | R-L0-40 |
| `to_int64_saturating` | `float.to_int64_saturating(n: Float) -> Int64` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其余 7 语言 — | 移除（→ `int64.from_saturating`） | R-L0-40 |
| `to_uint8_saturating` | `float.to_uint8_saturating(n: Float) -> UInt8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其余 7 语言 — | 移除（→ `uint8.from_saturating`） | R-L0-40 |
| `to_uint16_saturating` | `float.to_uint16_saturating(n: Float) -> UInt16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其余 7 语言 — | 移除（→ `uint16.from_saturating`） | R-L0-40 |
| `to_uint32_saturating` | `float.to_uint32_saturating(n: Float) -> UInt32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其余 7 语言 — | 移除（→ `uint32.from_saturating`） | R-L0-40 |
| `to_uint64_saturating` | `float.to_uint64_saturating(n: Float) -> UInt64` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其余 7 语言 — | 移除（→ `uint64.from_saturating`） | R-L0-40 |
| `from_bits` | （新增）`float.from_bits(bits: Int) -> Float` | Rust `f64::from_bits` (F) / Go `math.Float64frombits` / Kotlin `Double.fromBits` / Swift `Double(bitPattern:)` | 新增 | R-float-02 |
| `trunc` | （新增）`float.trunc(n: Float) -> Float` | Rust `trunc` (F) / Go `math.Trunc` / Python `math.trunc` / JS `Math.trunc` / OCaml `trunc` (F) / Kotlin `truncate` / Swift `rounded(.towardZero)` | 新增 | R-float-04 |
| `is_finite` | （新增）`float.is_finite(n: Float) -> Bool` | Rust `is_finite` (F) / Python `math.isfinite` / JS `Number.isFinite` / OCaml `is_finite` (F) / Kotlin `isFinite` / Swift `isFinite` | 新增 | R-float-04 |
| `pow` | （新增）`float.pow(base: Float, exp: Float) -> Float` | Rust `powf` (F) / Go `math.Pow` / Python `math.pow` / JS `Math.pow` / Gleam `power` (F) / OCaml `pow` (F) / Kotlin `pow` / Swift `pow` | 新增 | R-math-01 |
| `infinity` | （新增）`float.infinity() -> Float` | Rust `INFINITY` (F) / Go `math.Inf(1)` / Python `math.inf` / JS `Infinity` / OCaml `infinity` (F) / Kotlin `POSITIVE_INFINITY` / Swift `.infinity` | 新增 | R-float-03 |
| `nan` | （新增）`float.nan() -> Float` | Rust `NAN` (F) / Go `math.NaN()` / Python `math.nan` / JS `NaN` / OCaml `nan` (F) / Kotlin `NaN` / Swift `.nan` | 新增 | R-float-03 |
| `epsilon` | （新增）`float.epsilon() -> Float` | Rust `EPSILON` (F) / Python `sys.float_info.epsilon` / JS `Number.EPSILON` / OCaml `epsilon` (F) / Swift `.ulpOfOne` | 新增 | R-float-03 |
| `max_value` | （新增）`float.max_value() -> Float` | Rust `MAX` (F) / Go `math.MaxFloat64` / JS `Number.MAX_VALUE` / OCaml `max_float` (F) / Kotlin `MAX_VALUE` / Swift `.greatestFiniteMagnitude` | 新增 | R-float-03 |
| `min_value` | （新增）`float.min_value() -> Float` | Rust `MIN`（最小有限值）(F) / Swift `-.greatestFiniteMagnitude` / 注：Kotlin、JS 的 `MIN_VALUE` 为最小正数 | 新增 | R-float-03 |
| `compare` | （新增）`float.compare(a: Float, b: Float) -> Int`（IEEE totalOrder） | Rust `total_cmp` (F) / Go `cmp.Compare` / Gleam `compare` (F) / OCaml `compare` (F) / Kotlin `compareTo` / Swift `isTotallyOrdered` | 新增 | R-sized-02 |
| `from` | （新增）`float.from[S: Numeric](x: S) -> Float` | Rust `From`・`as` / Swift `init(_:)` / Python `int(x)` | 新增 | R-L0-40 |
| `from_checked` | （新增）`float.from_checked[S: Numeric](x: S) -> Float?` | Rust `TryFrom` / Swift `init(exactly:)` | 新增 | R-L0-40 |
