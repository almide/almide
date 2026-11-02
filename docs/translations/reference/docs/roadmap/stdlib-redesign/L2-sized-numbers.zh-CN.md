> 描述（固定历史）：标准库重设计 L2/L3 — int8…uint64、float32、float64：十二载体的单一转换矩阵、来源侧转换、各载体具备 parse/compare/to_string、各浮点载体具备 IEEE 格。
<a id="l2l3-固定幅の数int8-int16-int32-int64-uint8-uint16-uint32-uint64-float32-float64"></a>

# L2/L3：定宽数字（int8 int16 int32 int64 uint8 uint16 uint32 uint64 float32 float64）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-sized-numbers.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-sized-numbers.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-sized-numbers.zh-CN.md)

> 非规范性历史重设计参照版。固定原本权威且不变。不自动同步、重新获取官方来源或独立执行。(F)、实测、数量、其他语言实现的陈述是原作者当时报告。保留早期源类型to_方向与后来R-L0-40目标类型from表、无后缀回绕/饱和与后来abort规则、撤除饱和格的取消、算术/判定的否决与后来协议要求。早期函数数量未必等于后来追加行数。原文关于整数is_nan以及提升精度再转回不改变舍入的概括原样保留，未经独立验证，不是普遍数值保证。日文代码占位符指类型、目标、源和整数。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md)

> 最后更新：2026-10-21
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md) / [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)（R-L1-20：不合并模块）。
> 相关：[L2-int](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-int.md) / [L2-float](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-float.md)
> 来源：Rust https://doc.rust-lang.org/std/primitive.i8.html 与 https://doc.rust-lang.org/std/convert/trait.TryFrom.html / Go https://go.dev/ref/spec#Conversions 与 https://pkg.go.dev/strconv / Python https://docs.python.org/3/library/struct.html / TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/BigInt/asIntN / Gleam https://gleam-stdlib.hexdocs.pm/gleam/int.html (F) / Elixir https://elixir.hexdocs.pm/Integer.html (F) / OCaml https://ocaml.org/manual/latest/api/Int32.html / Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin/-long/ (F) / Swift https://developer.apple.com/documentation/swift/binaryinteger/init(truncatingifneeded:)

(F) = 2026-10-21 获取官方页面确认的拼写。Swift `init(truncatingIfNeeded:)` / `init(clamping:)` / `init(exactly:)` 与 Kotlin `Double.toInt()` 饱和已在 [facts/naming-conventions.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/facts/naming-conventions.md) 第 11 节 (F) 确认。

<a id="l2-役割の比較"></a>

## L2：角色比较

| 角色 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 定宽类型 | 八模块 + `float32` `float64` | `i8`〜`u64` `f32` `f64` | `int8`〜`uint64` `float32` | —（`struct` 仅类型码） | 类型化数组/`BigInt.asIntN` | — | — | `Int32` `Int64` | `Byte`〜`ULong` `Float` | `Int8`〜`UInt64` `Float` |
| 有损转换（无后缀；Almide 按 R-L0-32 abort，回绕为 R-L0-34 的 `_wrapping`） | `to_<型>` | `as` | `T(x)` | — | `asIntN` | — | — | `of_int` | `toByte()` 等 | `init(truncatingIfNeeded:)` (F) |
| 越界 none | `to_<型>_checked` | `try_from` | — | — | — | — | — | Base `to_int_exn` 族 | — | `init(exactly:)` (F) |
| 越界饱和至端点 | `to_<型>_saturating` | 整数 —（只有运算 `saturating_*`），float→int 的 `as` | — | — | — | — | — | — | `coerceIn` | `init(clamping:)` (F) |
| 默认 `Int` / `Float` 目标 | `int.from_<型>`、`float.from_float32/64`（目标侧） | `i64::from` / `as` | `int64(x)` | — | — | — | — | `Int32.to_int` | `toLong()` | `Int(x)` |
| 转字符串 | `to_string` | `to_string` | `FormatInt` | — | — | — | — | `Int32.to_string` | `toString` | `String(x)` |
| 由字符串 | — | `parse::<i8>` | `ParseInt(s, 10, 8)` | — | — | — | — | `Int32.of_string_opt` | `toByteOrNull` | `Int8(s)` |
| 比较 | 仅运算符 | `cmp` | `cmp.Compare` | — | — | — | — | `Int32.compare` | `compareTo` | `<` |
| 下限/上限 | 整数 `min_value` `max_value`，浮点 — | `MIN` `MAX` | `math.MaxInt8` 等 | — | — | — | — | `Int32.min_int` | `MIN_VALUE` | `.min` `.max` |
| 比特（浮点） | — | `f32::to_bits` `from_bits` | `Float32bits` | `struct` | `DataView` | — | — | `Int32.bits_of_float` | `toBits` | `bitPattern` |

<a id="完全性の規則変換の行列"></a>

### 完备性规则（转换矩阵）

十二载体：`Int`、`Int8`、`Int16`、`Int32`、`Int64`、`UInt8`、`UInt16`、`UInt32`、`UInt64`、`Float`、`Float32`、`Float64`；对应模块 `int`、`int8` … `uint64`、`float`、`float32`、`float64`。

1. **方向**（R-L0-07）：转换放在来源模块 `to_<先>`，不放目标侧 `from_<元>`（撤除 `int.from_int8`、`float.from_int`、`float.from_float32`、`float.from_float64`）。
2. **plain**：所有不同载体的有序对（12 × 11 = 132）均有 `to_<先>`。
3. **`_checked`**：仅可能无法精确表示的有损对。整数→整数为范围不能容纳；整数→浮点为有效位超过尾数（Float32 24、Float64/Float 53）；浮点→整数总是；以及 Float64/Float→Float32。无损对不设（只会返回 some 的函数是噪声）。
4. **`_saturating`**：仅整数→整数范围不容纳的对。浮点→整数 `to_<先>` 已饱和（同 Rust `as`、Kotlin `toInt()`，NaN 为零），不另设 `_saturating`（R-sized-04）。整数→浮点/浮点互转溢出有 `inf`，不设。
5. **全部载体**：`to_string`、`parse`（`Result[T, String]`，越界 err）、`compare`（`-> Int`，浮点 IEEE totalOrder）。
6. **整数载体**：`min_value`、`max_value`。
7. **浮点载体**（`float`、`float32`、`float64`）：`to_bits`、`from_bits`、`infinity`、`nan`、`epsilon`、`max_value`、`min_value`。
8. **有意省略**：定宽算术函数（`abs`、`min`、`max`、`pow`、位运算等）。定宽类型用于存储/边界，可用运算符（`+` `-` `*` `/` `%` `<`）；其他计算通过 `to_int` 转为 `Int` 后用 `int` 函数。

按此规则，矩阵缺 52 格、多 8 格（下表）。

| 模块 | 缺格（R-sized-01） | 多余格（R-sized-04） |
|---|---|---|
| `int` | `to_float_checked` `to_float64_checked` | — |
| `int8` `int16` `uint8` `uint16` | `to_int` `to_float` | — |
| `int32` `uint32` | `to_int` `to_float` `to_float32_checked` | — |
| `int64` | `to_int` `to_float` `to_float_checked` `to_float32_checked` `to_float64_checked` | — |
| `uint64` | `to_int` `to_int_checked` `to_int_saturating` `to_float` `to_float_checked` `to_float32_checked` `to_float64_checked` | — |
| `float` | `to_int_checked` | `to_int8_saturating` … `to_uint64_saturating`（8） |
| `float32` | `to_int` `to_float` 及面向九整数类型的 `_checked`（`to_int_checked` `to_int8_checked` … `to_uint64_checked`） | — |
| `float64` | `to_int` `to_float` `to_float32_checked` 及面向九整数类型的 `_checked` | — |

当前 gate（`src/cli/docs_gen.rs` 的 `check_numeric_conversion_matrix`，#956）仅看九个整数载体。三个浮点不在矩阵内，正是 `float32`/`float64` 完全无 `_checked` 的原因。

<a id="l2-の判定"></a>

### L2 判定

- 方向重复：十个 `int.from_<型>`、三个 `float.from_*` → R-sized-01（与 R-int-01/R-float-01 同 issue）。
- 族缺口：上表 52 格 → R-sized-01；`parse`（十模块）、`compare`（十二模块）→ R-sized-02；浮点七格（float32/float64 共 14）→ R-sized-03。
- 同义词：八个 `float.to_<整数>_saturating` → R-sized-04。
- 不合并：按 R-L1-20（R-sized-05 否决）。

<a id="案"></a>

## 提案

<a id="r-sized-01-変換の行列を-12-個の運び手に広げ向きを元の型の側にそろえる---採用"></a>

### R-sized-01 转换矩阵扩展至十二载体，统一放在来源侧 — ○ 采纳

- 内容：增加上述 52 格，撤除 `int.from_int8` … `int.from_uint64`、`int.from_uint64_checked`、`int.from_uint64_saturating`、`float.from_int`、`float.from_float32`、`float.from_float64`。含义同现有格：plain 整数回绕，浮点→整数向零截断并饱和、NaN 为零；`_checked` 无法精确表示则 none（整数→浮点发生舍入则 none，同当前 `int.to_float32_checked`）。
- 第六轮注：上述 plain 整数回绕是采纳时规则，无后缀已由 R-L0-32 改为越界 abort，回绕移往 R-L0-34 `_wrapping`。
- L0 依据：R-L0-07、R-L0-08（补矩阵）、R-L0-01。
- (a) Swift 三初始化器 `truncatingIfNeeded:`/`exactly:`/`clamping:` 最接近 (F)。Rust 三族 `as`/`TryFrom`/`From`，其他七语言只有一两种，无多数（按 L0 R-L0-08 保留后缀矩阵）。
- (b) 纯计算。新格可由旧格组合，在 `stdlib/<型>_convert.almd` 用 Almide 编写，例如 `float32.to_int16_checked(x) = float.to_int16_checked(float32.to_float(x))`，无需新增 intrinsic。使用既有 `spec/stdlib/sized_conversion_matrix_test.almd` 同边界行确认两目标同值。
- (c) 当前可写 `int8.to_int16` 却无 `int8.to_int`，有 `float32.to_int8` 却无 `float32.to_int8_checked`，矩阵每个缺格都会使 LLM 按规则推测的拼写失败。
- (d) 新增 52、撤除 13。撤除标记 `@deprecated(since=15, use="int8.to_int")` 等，epoch 16 移除。`almide fix` 一对一：`int.from_<T>(x)` → `<T>.to_int(x)`、`int.from_uint64_checked(x)` → `uint64.to_int_checked(x)`、`float.from_int(x)` → `int.to_float(x)`、`float.from_float32(x)` → `float32.to_float(x)`。使用数（spec/tests/docs/stdlib）：八个 `int.from_<型>` 合计 70/38/47/362、`from_uint64_checked` 9/2/7/1、`from_uint64_saturating` 4/2/4/0、`float.from_int` 24/11/11/0、`from_float32` 4/2/6/29、`from_float64` 7/0/6/23。stdlib 是 sized 模块本体自身调用，迁移后归零。改写 CHEATSHEET:1068 的“Widening is `int.from_<src>(x)`”。
- gate：`check_numeric_conversion_matrix` 增加 `float`/`float32`/`float64`，损失判断从范围扩至范围或尾数位数。`Int` 列的 `from_<s>` 期待替换为 `<s>.to_int`，检查 `int.from_*`/`float.from_*` 进入禁止存在项。semantics 的 `sized_conversion_matrix_test.almd` 增加浮点行。
- Issue：#3935。

<a id="r-sized-02-すべての運び手に-parse-と-compare-を置く---採用"></a>

### R-sized-02 所有载体具备 `parse`/`compare` — ○ 采纳

- 内容：`int8.parse(s: String) -> Result[Int8, String]` … `float64.parse`（十函数，整数同 `int.parse` 语法且越界 err，浮点同 `float.parse`）。十二载体全加 `<mod>.compare(a: T, b: T) -> Int`（负/零/正；浮点 IEEE 754 totalOrder，与 `list.sort` 同序）。
- L0 依据：R-L0-16（有序类型有 `compare`）、R-L0-06 第 4 项（可失败构造返回 Option/Result）、R-L0-02（放返回类型模块）、R-L0-06（`parse` 失败有理由，故 Result）。
- (a) 五种有定宽类型的语言 Rust/Go/OCaml/Kotlin/Swift 都可按类型解析字符串。
- (b) 纯计算，可组合 `int.parse` 与 `_checked`。
- (c) 两步 `int.parse(s)! |> int.to_int8_checked` 可由一函数表达，两步会丢失越界理由。
- (d) 新增 22，使用数零。
- gate：矩阵要求每载体具备 `to_string`/`parse`/`compare`。
- Issue：#3902。

<a id="r-sized-03-float32float64-に浮動小数点の-7-セルを置く---採用"></a>

### R-sized-03 `float32`/`float64` 增加浮点七格 — ○ 采纳

- 内容：`float32.to_bits(x: Float32) -> Int`（低 32 位）、`float32.from_bits(bits: Int) -> Float32`（读低 32 位）、`float32.infinity()`、`nan()`、`epsilon()`（2^-23）、`max_value()`、`min_value()`。`float64` 同七格（`to_bits` 64 位）。`float` 见 [R-float-02](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-float.md)/[R-float-03](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-float.md)。
- L0 依据：完备性规则 7（R-L0-08 矩阵）、R-L0-07 第 2 项（从位构造属于目标侧 `from_bits`）。
- (a) Rust/Go/Kotlin/Swift 按类型提供 32 位浮点比特/常数。
- (b) 纯计算。`float32.from_bits` 接替撤除的 `int.bits_to_f32`，用同一 intrinsic（低 32 位读为 binary32）。
- (c) 当前陷阱是读取 GGUF 等 f32 权重代码无法按名找到 `int.bits_to_f32`。
- (d) 新增 14，使用数零（`int.bits_to_f32` 的 5/3/5/1 迁入）。
- gate：R-sized-01 同 gate 检查七格。
- Issue：#3910。

<a id="r-sized-04-floatto_整数_saturating-8-関数を撤去する---取り消しr-l0-32"></a>

### R-sized-04 撤除八个 `float.to_<整数>_saturating` — × 取消（R-L0-32）

- 取消（第五轮）：R-L0-32 令无后缀 `float.to_int8` 等越界 abort，`_saturating` 不再同义，而是当前饱和行为的唯一格。保留八个，再加 `float.to_int_saturating` 成九个。
- 内容：撤除 `float.to_int8_saturating` … `float.to_uint64_saturating`，因 `float.to_int8` 等已有同饱和。
- L0 依据：R-L0-01（仅调用其他公开函数一次也属同义词）。当前由 `tests/stdlib_synonym_gate_test.rs:47-59` 允许列表放行，撤除使列表减少八行。
- (a) Rust `as`、Kotlin `toInt()` 只有一种浮点→整数饱和拼写。
- (c) 两个同结果拼写会让人误读 `to_int8` 不饱和而回绕。规则 4 写 plain 浮点→整数饱和更准确。
- (d) 撤除 8。`@deprecated(since=15, use="float.to_int8")` 等，epoch 16 撤除。`almide fix` 仅去掉 `_saturating`。使用数：`to_int32_saturating` 5/0/3/0、`to_uint16_saturating` 4/0/3/0，另六个 0/0/3/0。
- gate：浮点→整数 `_saturating` 列为禁止存在，同义词允许列表删八行。
- Issue：#3935。

<a id="r-sized-05-固定幅のモジュールをまとめる算術関数を足す---不採用算術の部分は--r-sized-10-で見直し"></a>

### R-sized-05 合并定宽模块/新增算术函数 — × 否决（算术部分由 R-sized-10 重审）

- 内容：不合并模块（R-L1-20），不新增 `int8.abs`、`uint32.max` 等算术函数（完备性规则 8）。
- L0 依据：R-L0-02（主语类型模块；Almide 无方法，故每类型一个模块）、R-L0-01（不将相同 `Int` 计算复制十倍）。
- (c) 定宽算术只有一种写法：`to_int` 后用 `int.*`。矩阵补齐 `to_int`（R-sized-01），每类型同拼写。
- Issue：无。

<a id="r-sized-06-整数の運び手すべてに-parse_radixto_string_radix-を置く---採用"></a>

### R-sized-06 所有整数载体具备 `parse_radix`/`to_string_radix` — ○ 采纳

- 现状：仅 `int`、`bigint` 有 `parse_radix(s, radix)`、`to_string_radix(n, radix)`。
- 提案：八个 `int8`–`uint64` 新增 `fn <T>.parse_radix(s: String, radix: Int) -> Result[<T>, String]`、`fn <T>.to_string_radix(x: <T>, radix: Int) -> String`（16 函数），语法/基数范围同 `int.parse_radix`。
- L0 依据：同 R-sized-02（两步会丢失越界理由）。
- 对否决 R-sized-05 的新证据：规则 8 的 `to_int` 后 `int.*` 对 `uint64` ≥2^63 不成立。实测 `int.from_hex("ffffffffffffffff")` 为 err，`int.max(int.from_uint64(uint64.max_value()), 1)` 为 `1`。
- (a) 五语言可按基数读写无符号 64 位：Rust `u64::from_str_radix` (F)、Go `ParseUint`/`FormatUint`、Kotlin `toULong(radix)`、Swift `UInt64(s, radix:)`、Python `int(s, 16)`。
- (c) 读取十六进制 64 位 hash 很常见，LLM 会写 `uint64.parse_radix(s, 16)`，当前其他路径静默丢值。
- (d) 新增 16。
- 完备性：整数载体（`int`/`bigint`/八定宽）全有 `parse`/`to_string`/`parse_radix`/`to_string_radix`。
- gate：数值转换矩阵增加两列，对 `uint64` 的 `max_value` 以基数 2/16/36 检查往返。
- Issue：#3972。

<a id="r-sized-07-float32float64-に-is_nan-などの判定を足す---不採用-r-sized-08-で見直し"></a>

### R-sized-07 为 `float32`/`float64` 新增 `is_nan` 等判定 — × 否决（→ R-sized-08 重审）

- 理由：`float32.to_float` 无损，规则 8 的 `to_float` 后 `float.*` 正确。扩展到判定后又要反复决定是否加 `floor`/`sqrt`。改为 E002 指引（R-L0-19）增加 `float32.to_float(x) |> float.is_nan`。
- Issue：无。

<a id="r-sized-08-integerfloatingpoint-を宣言し選択と分類の関数を全型にそろえる---採用r-sized-07-を見直し"></a>

### R-sized-08 声明 `Integer`/`FloatingPoint`，所有类型补齐选择/分类 — ○ 采纳（重审 R-sized-07）

- 现状：比较/选择/分类因类型缺失。有 `int.min`/`max`/`clamp` 无 `uint64.min`（`UInt64` 上半区不能移入 `Int`，故不能 `to_int` 后 `int.min` 代替）；有 `float32.nan`/`infinity` 无 `float32.is_nan`，两个外部评审连续指出。
- 提案：声明 R-L0-31 两 protocol，补格。
  - `Integer`（`int` 加八类型）：`compare`、`min`、`max`、`clamp`、`parse`、`parse_radix`、`to_string`、`to_string_radix`、`min_value`、`max_value`。新增八类型 `min`/`max`/`clamp`（24 函数）。
  - `FloatingPoint`（`float`/`float32`/`float64`）：`compare`、`min`、`max`、`clamp`、`parse`、`to_string`、`is_nan`、`is_infinite`、`is_finite`、`nan`、`infinity`、`epsilon`、`min_value`、`max_value`。新增两类型 `min`/`max`/`clamp`/`is_nan`/`is_infinite`/`is_finite`（12 函数）。
  - 边界：选择/比较/分类结果在类型内闭合，按类型具备。计算函数（`abs`/`floor`/`round`/`sqrt` 等）仍 `to_float` 后 `float.*`（R-sized-07 剩余理由；提高精度计算再转回不会改变舍入结果）。
- L0 依据：R-L0-31、R-L0-12。
- (a) Swift `BinaryInteger`（`min`/`max` 为泛型函数），Rust 整数类型及 `f32` 均有 `min`/`max`/`clamp`/`is_nan`。
- (d) 新增 36。
- Issue：#3989。

<a id="r-sized-09-float32float64-に-to_int_saturating-を足す---採用"></a>

### R-sized-09 `float32`/`float64` 新增 `to_int_saturating` — ○ 采纳

- 现状：R-L0-32 新增 `float.to_int_saturating`，但 `float32`、`float64` 仅 `Int` 目标缺失（有 `to_int`/`to_int_checked`/`to_int64_saturating`）。
- 提案：新增 `float32.to_int_saturating(x: Float32) -> Int`、`float64.to_int_saturating(x: Float64) -> Int`。
- L0 依据：R-L0-32 完备性（浮点来源有损转换三种）。
- (d) 新增两函数。
- 完备性：矩阵 gate 确认浮点三类型×整数九类型的 27 格都有无后缀/`_checked`/`_saturating`。
- Issue：#3995。

<a id="r-sized-10-整数-8-型に-integerfixedwidthinteger-の要件をそろえる---採用r-sized-05-を見直し"></a>

### R-sized-10 八整数类型补齐 `Integer`/`FixedWidthInteger` 要求 — ○ 采纳（重审 R-sized-05）

- 现状：`int` 有 `floor_div`/`modulo`/`pow`/`is_even`/`is_odd`、位运算 `band`/`bor`/`bxor`/`bnot`/`bshl`/`bshr`、`count_leading_zeros`/`count_trailing_zeros`/`count_ones`/`byte_swap`/`abs`/`sign`，`int8`–`uint64` 没有。
- 提案：作为 [protocol 层级](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/protocol-view.md) 要求放到八类型。
  - `Integer`：`floor_div(a, b)`/`modulo(a, b)`（`floor_div(min_value, -1)` 同 `/` abort，R-int-13）、`pow(base, exp: Int)`、`is_even`、`is_odd`、`band`、`bor`、`bxor`、`bnot`、`bshl(a, n: Int)`、`bshr(a, n: Int)`。`bshr` 对有符号保留符号、无符号填零（同 Swift `>>`）。
  - `FixedWidthInteger`：`count_leading_zeros`、`count_trailing_zeros`、`count_ones`、`byte_swap`（按类型宽度计数；`int8.count_leading_zeros(1)` 为 7）。
  - `SignedInteger`（`int8`–`int64`）：`abs`、`sign`。
  - 结果越出类型范围时与该类型 `+`/`*` 同处理。
- 重审 R-sized-05：原理由“不将 `Int` 计算复制十倍”，但 R-L0-37 是单一 protocol 默认实现，不按类型编写。另一个理由 `to_int` 后 `int.*` 对 `uint64` ≥2^63 不成立（R-sized-06 实测）。
- 有意省略：不放 `int` 的 N bit 运算 `wrap_add(a, b, bits)`/`rotate_left(a, n, bits)`/`bit_width`/`bshr_logical`。定宽由类型决定宽度，同名会异形（protocol-view.md“此层级决定的事”）。
- (a) Swift `BinaryInteger`/`FixedWidthInteger` 给所有整数类型这些函数；Rust 同样（`div_euclid`/`rem_euclid`/`pow`/`count_ones`/`leading_zeros`/`swap_bytes`）。
- (d) 新增 128（八类型×15，有符号四类型×2）。
- Issue：#4000。

<a id="r-sized-11-float32float64-に-floatingpoint-の要件をそろえる---採用r-sized-08-の線引きを見直し"></a>

### R-sized-11 `float32`/`float64` 补齐 `FloatingPoint` 要求 — ○ 采纳（重审 R-sized-08 边界）

- 现状：`float` 有 `abs`/`floor`/`ceil`/`round`/`trunc`/`signum`，`float32`/`float64` 无。R-sized-08 曾要求计算函数先 `to_float` 再 `float.*`。
- 提案：作为 protocol `Signed`（`abs`）、`FloatingPoint`（`floor`/`ceil`/`round`/`trunc`/`signum`）要求放入两类型，同 `float` 同名语义（`round` 向远离零一侧舍入）。
- 重审理由：担忧是反复决定增加到哪里；现在 protocol 要求即边界，不再反复。`sqrt` 等数学函数留在 `math`，不属 `FloatingPoint` 要求。
- (a) Swift `FloatingPoint` 有 `rounded`/`magnitude`，Rust `f32` 有 `abs`/`floor`/`ceil`/`round`/`trunc`/`signum`。
- (d) 新增 12。
- Issue：#4000。

<a id="l3-関数表"></a>

## L3：函数表

<a id="l3-int8"></a>

### L3: `int8`

当前 20 函数，新增四函数。

| 函数 | 签名（当前，显示 mut） | 其他语言主要拼写 | 判定 | 提案 |
|---|---|---|---|---|
| `to_int16` | `int8.to_int16(x: Int8) -> Int16` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤除（→ `int16.from`） | R-L0-40 |
| `to_int32` | `int8.to_int32(x: Int8) -> Int32` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤除（→ `int32.from`） | R-L0-40 |
| `to_int64` | `int8.to_int64(x: Int8) -> Int64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤除（→ `int64.from`） | R-L0-40 |
| `to_uint8` | `int8.to_uint8(x: Int8) -> UInt8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `uint8.from`） | R-L0-40 |
| `to_uint16` | `int8.to_uint16(x: Int8) -> UInt16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `uint16.from`） | R-L0-40 |
| `to_uint32` | `int8.to_uint32(x: Int8) -> UInt32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `uint32.from`） | R-L0-40 |
| `to_uint64` | `int8.to_uint64(x: Int8) -> UInt64` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `uint64.from`） | R-L0-40 |
| `to_float32` | `int8.to_float32(x: Int8) -> Float32` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 撤除（→ `float32.from`） | R-L0-40 |
| `to_float64` | `int8.to_float64(x: Int8) -> Float64` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 撤除（→ `float64.from`） | R-L0-40 |
| `to_string` | `int8.to_string(x: Int8) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | 保留 | — |
| `to_uint8_checked` | `int8.to_uint8_checked(x: Int8) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `uint8.from_checked`） | R-L0-40 |
| `to_uint8_saturating` | `int8.to_uint8_saturating(x: Int8) -> UInt8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `uint8.from_saturating`） | R-L0-40 |
| `to_uint16_checked` | `int8.to_uint16_checked(x: Int8) -> Option[UInt16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `uint16.from_checked`） | R-L0-40 |
| `to_uint16_saturating` | `int8.to_uint16_saturating(x: Int8) -> UInt16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `uint16.from_saturating`） | R-L0-40 |
| `to_uint32_checked` | `int8.to_uint32_checked(x: Int8) -> Option[UInt32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `uint32.from_checked`） | R-L0-40 |
| `to_uint32_saturating` | `int8.to_uint32_saturating(x: Int8) -> UInt32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `uint32.from_saturating`） | R-L0-40 |
| `to_uint64_checked` | `int8.to_uint64_checked(x: Int8) -> Option[UInt64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `uint64.from_checked`） | R-L0-40 |
| `to_uint64_saturating` | `int8.to_uint64_saturating(x: Int8) -> UInt64` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `uint64.from_saturating`） | R-L0-40 |
| `min_value` | `int8.min_value() -> Int8` | Rust `MIN` / Go `math.Min…` / Kotlin `MIN_VALUE` / Swift `.min` | 保留 | — |
| `max_value` | `int8.max_value() -> Int8` | Rust `MAX` / Go `math.Max…` / Kotlin `MAX_VALUE` / Swift `.max` | 保留 | — |
| `parse` | （新增）`int8.parse(s: String) -> Result[Int8, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` 等 / Swift `T(s)` | 新增 | R-sized-02 |
| `compare` | （新增）`int8.compare(a: Int8, b: Int8) -> Int` | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | 新增 | R-sized-02 |
| `floor_div` | （新增）`int8.floor_div(a: Int8, b: Int8) -> Int8` | Rust `div_euclid` / Swift — / Python `//` | 新增 | R-sized-10 |
| `modulo` | （新增）`int8.modulo(a: Int8, b: Int8) -> Int8` | Rust `rem_euclid` / Python `%` | 新增 | R-sized-10 |
| `pow` | （新增）`int8.pow(base: Int8, exp: Int) -> Int8` | Rust `pow` / Swift — | 新增 | R-sized-10 |
| `is_even` | （新增）`int8.is_even(n: Int8) -> Bool` | Swift `isMultiple(of: 2)` / Kotlin — | 新增 | R-sized-10 |
| `is_odd` | （新增）`int8.is_odd(n: Int8) -> Bool` | Swift `!isMultiple(of: 2)` | 新增 | R-sized-10 |
| `count_leading_zeros` | （新增）`int8.count_leading_zeros(n: Int8) -> Int` | Rust `leading_zeros` / Swift `leadingZeroBitCount` | 新增 | R-sized-10 |
| `count_trailing_zeros` | （新增）`int8.count_trailing_zeros(n: Int8) -> Int` | Rust `trailing_zeros` / Swift `trailingZeroBitCount` | 新增 | R-sized-10 |
| `count_ones` | （新增）`int8.count_ones(n: Int8) -> Int` | Rust `count_ones` / Swift `nonzeroBitCount` | 新增 | R-sized-10 |
| `byte_swap` | （新增）`int8.byte_swap(n: Int8) -> Int8` | Rust `swap_bytes` / Swift `byteSwapped` | 新增 | R-sized-10 |
| `abs` | （新增）`int8.abs(n: Int8) -> Int8` | Rust `abs` / Swift `abs` | 新增 | R-sized-10 |
| `sign` | （新增）`int8.sign(n: Int8) -> Int` | Rust `signum` / Swift `signum()` | 新增 | R-sized-10 |
| `from` | （新增）`int8.from[S: Numeric](x: S) -> Int8` | Rust `From`・`as` / Swift `init(_:)` / Python `int(x)` | 新增 | R-L0-40 |
| `from_checked` | （新增）`int8.from_checked[S: Numeric](x: S) -> Int8?` | Rust `TryFrom` / Swift `init(exactly:)` | 新增 | R-L0-40 |
| `from_saturating` | （新增）`int8.from_saturating[S: Numeric](x: S) -> Int8` | Swift `init(clamping:)` / Rust `as`（来自浮点数） | 新增 | R-L0-40 |
| `from_wrapping` | （新增）`int8.from_wrapping[S: Integer](x: S) -> Int8` | Swift `init(truncatingIfNeeded:)` / Rust `as` | 新增 | R-L0-40 |
| `wrap_add` | （新增）`int8.wrap_add(a: Int8, b: Int8) -> Int8` | Swift `&+` / Rust `wrapping_add` | 新增 | R-int-12 |
| `wrap_sub` | （新增）`int8.wrap_sub(a: Int8, b: Int8) -> Int8` | Swift `&-` / Rust `wrapping_sub` | 新增 | R-int-12 |
| `wrap_mul` | （新增）`int8.wrap_mul(a: Int8, b: Int8) -> Int8` | Swift `&*` / Rust `wrapping_mul` | 新增 | R-int-12 |
| `rotate_left` | （新增）`int8.rotate_left(a: Int8, n: Int) -> Int8` | Rust `rotate_left` / Go `bits.RotateLeft` | 新增 | R-int-12 |
| `rotate_right` | （新增）`int8.rotate_right(a: Int8, n: Int) -> Int8` | Rust `rotate_right` | 新增 | R-int-12 |
| `bit_length` | （新增）`int8.bit_length(n: Int8) -> Int` | Python `bit_length` / Go `bits.Len` | 新增 | R-int-11 |
| `band` | （新增）`int8.band(a: Int8, b: Int8) -> Int8` | Elixir `band` / Rust・Go・Swift 运算符 `&` | 新增 | R-sized-10 |
| `bor` | （新增）`int8.bor(a: Int8, b: Int8) -> Int8` | Elixir `bor` / Rust・Go・Swift 运算符 `\|` | 新增 | R-sized-10 |
| `bxor` | （新增）`int8.bxor(a: Int8, b: Int8) -> Int8` | Elixir `bxor` / Rust・Go・Swift 运算符 `^` | 新增 | R-sized-10 |
| `bnot` | （新增）`int8.bnot(a: Int8) -> Int8` | Elixir `bnot` / Swift `~` | 新增 | R-sized-10 |
| `bshl` | （新增）`int8.bshl(a: Int8, n: Int) -> Int8` | Elixir `bsl` / 运算符 `<<` | 新增 | R-sized-10 |
| `bshr` | （新增）`int8.bshr(a: Int8, n: Int) -> Int8` | Elixir `bsr` / 运算符 `>>` | 新增 | R-sized-10 |

<a id="l3-int16"></a>

### L3: `int16`

现有 22 个函数，另新增 4 个函数。

| 函数 | 签名（当前，显示 mut） | 其他语言主要拼写 | 判定 | 提案 |
|---|---|---|---|---|
| `to_int8` | `int16.to_int8(x: Int16) -> Int8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `int8.from`） | R-L0-40 |
| `to_int32` | `int16.to_int32(x: Int16) -> Int32` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤除（→ `int32.from`） | R-L0-40 |
| `to_int64` | `int16.to_int64(x: Int16) -> Int64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤除（→ `int64.from`） | R-L0-40 |
| `to_uint8` | `int16.to_uint8(x: Int16) -> UInt8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `uint8.from`） | R-L0-40 |
| `to_uint16` | `int16.to_uint16(x: Int16) -> UInt16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `uint16.from`） | R-L0-40 |
| `to_uint32` | `int16.to_uint32(x: Int16) -> UInt32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `uint32.from`） | R-L0-40 |
| `to_uint64` | `int16.to_uint64(x: Int16) -> UInt64` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `uint64.from`） | R-L0-40 |
| `to_float32` | `int16.to_float32(x: Int16) -> Float32` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 撤除（→ `float32.from`） | R-L0-40 |
| `to_float64` | `int16.to_float64(x: Int16) -> Float64` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 撤除（→ `float64.from`） | R-L0-40 |
| `to_string` | `int16.to_string(x: Int16) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | 保留 | — |
| `to_int8_checked` | `int16.to_int8_checked(x: Int16) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `int8.from_checked`） | R-L0-40 |
| `to_int8_saturating` | `int16.to_int8_saturating(x: Int16) -> Int8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `int8.from_saturating`） | R-L0-40 |
| `to_uint8_checked` | `int16.to_uint8_checked(x: Int16) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `uint8.from_checked`） | R-L0-40 |
| `to_uint8_saturating` | `int16.to_uint8_saturating(x: Int16) -> UInt8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `uint8.from_saturating`） | R-L0-40 |
| `to_uint16_checked` | `int16.to_uint16_checked(x: Int16) -> Option[UInt16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `uint16.from_checked`） | R-L0-40 |
| `to_uint16_saturating` | `int16.to_uint16_saturating(x: Int16) -> UInt16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `uint16.from_saturating`） | R-L0-40 |
| `to_uint32_checked` | `int16.to_uint32_checked(x: Int16) -> Option[UInt32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `uint32.from_checked`） | R-L0-40 |
| `to_uint32_saturating` | `int16.to_uint32_saturating(x: Int16) -> UInt32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `uint32.from_saturating`） | R-L0-40 |
| `to_uint64_checked` | `int16.to_uint64_checked(x: Int16) -> Option[UInt64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `uint64.from_checked`） | R-L0-40 |
| `to_uint64_saturating` | `int16.to_uint64_saturating(x: Int16) -> UInt64` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `uint64.from_saturating`） | R-L0-40 |
| `min_value` | `int16.min_value() -> Int16` | Rust `MIN` / Go `math.Min…` / Kotlin `MIN_VALUE` / Swift `.min` | 保留 | — |
| `max_value` | `int16.max_value() -> Int16` | Rust `MAX` / Go `math.Max…` / Kotlin `MAX_VALUE` / Swift `.max` | 保留 | — |
| `parse` | （新增）`int16.parse(s: String) -> Result[Int16, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` 等 / Swift `T(s)` | 新增 | R-sized-02 |
| `compare` | （新增）`int16.compare(a: Int16, b: Int16) -> Int` | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | 新增 | R-sized-02 |
| `floor_div` | （新增）`int16.floor_div(a: Int16, b: Int16) -> Int16` | Rust `div_euclid` / Swift — / Python `//` | 新增 | R-sized-10 |
| `modulo` | （新增）`int16.modulo(a: Int16, b: Int16) -> Int16` | Rust `rem_euclid` / Python `%` | 新增 | R-sized-10 |
| `pow` | （新增）`int16.pow(base: Int16, exp: Int) -> Int16` | Rust `pow` / Swift — | 新增 | R-sized-10 |
| `is_even` | （新增）`int16.is_even(n: Int16) -> Bool` | Swift `isMultiple(of: 2)` / Kotlin — | 新增 | R-sized-10 |
| `is_odd` | （新增）`int16.is_odd(n: Int16) -> Bool` | Swift `!isMultiple(of: 2)` | 新增 | R-sized-10 |
| `count_leading_zeros` | （新增）`int16.count_leading_zeros(n: Int16) -> Int` | Rust `leading_zeros` / Swift `leadingZeroBitCount` | 新增 | R-sized-10 |
| `count_trailing_zeros` | （新增）`int16.count_trailing_zeros(n: Int16) -> Int` | Rust `trailing_zeros` / Swift `trailingZeroBitCount` | 新增 | R-sized-10 |
| `count_ones` | （新增）`int16.count_ones(n: Int16) -> Int` | Rust `count_ones` / Swift `nonzeroBitCount` | 新增 | R-sized-10 |
| `byte_swap` | （新增）`int16.byte_swap(n: Int16) -> Int16` | Rust `swap_bytes` / Swift `byteSwapped` | 新增 | R-sized-10 |
| `abs` | （新增）`int16.abs(n: Int16) -> Int16` | Rust `abs` / Swift `abs` | 新增 | R-sized-10 |
| `sign` | （新增）`int16.sign(n: Int16) -> Int` | Rust `signum` / Swift `signum()` | 新增 | R-sized-10 |
| `from` | （新增）`int16.from[S: Numeric](x: S) -> Int16` | Rust `From`・`as` / Swift `init(_:)` / Python `int(x)` | 新增 | R-L0-40 |
| `from_checked` | （新增）`int16.from_checked[S: Numeric](x: S) -> Int16?` | Rust `TryFrom` / Swift `init(exactly:)` | 新增 | R-L0-40 |
| `from_saturating` | （新增）`int16.from_saturating[S: Numeric](x: S) -> Int16` | Swift `init(clamping:)` / Rust `as`（来自浮点数） | 新增 | R-L0-40 |
| `from_wrapping` | （新增）`int16.from_wrapping[S: Integer](x: S) -> Int16` | Swift `init(truncatingIfNeeded:)` / Rust `as` | 新增 | R-L0-40 |
| `wrap_add` | （新增）`int16.wrap_add(a: Int16, b: Int16) -> Int16` | Swift `&+` / Rust `wrapping_add` | 新增 | R-int-12 |
| `wrap_sub` | （新增）`int16.wrap_sub(a: Int16, b: Int16) -> Int16` | Swift `&-` / Rust `wrapping_sub` | 新增 | R-int-12 |
| `wrap_mul` | （新增）`int16.wrap_mul(a: Int16, b: Int16) -> Int16` | Swift `&*` / Rust `wrapping_mul` | 新增 | R-int-12 |
| `rotate_left` | （新增）`int16.rotate_left(a: Int16, n: Int) -> Int16` | Rust `rotate_left` / Go `bits.RotateLeft` | 新增 | R-int-12 |
| `rotate_right` | （新增）`int16.rotate_right(a: Int16, n: Int) -> Int16` | Rust `rotate_right` | 新增 | R-int-12 |
| `bit_length` | （新增）`int16.bit_length(n: Int16) -> Int` | Python `bit_length` / Go `bits.Len` | 新增 | R-int-11 |
| `band` | （新增）`int16.band(a: Int16, b: Int16) -> Int16` | Elixir `band` / Rust・Go・Swift 运算符 `&` | 新增 | R-sized-10 |
| `bor` | （新增）`int16.bor(a: Int16, b: Int16) -> Int16` | Elixir `bor` / Rust・Go・Swift 运算符 `\|` | 新增 | R-sized-10 |
| `bxor` | （新增）`int16.bxor(a: Int16, b: Int16) -> Int16` | Elixir `bxor` / Rust・Go・Swift 运算符 `^` | 新增 | R-sized-10 |
| `bnot` | （新增）`int16.bnot(a: Int16) -> Int16` | Elixir `bnot` / Swift `~` | 新增 | R-sized-10 |
| `bshl` | （新增）`int16.bshl(a: Int16, n: Int) -> Int16` | Elixir `bsl` / 运算符 `<<` | 新增 | R-sized-10 |
| `bshr` | （新增）`int16.bshr(a: Int16, n: Int) -> Int16` | Elixir `bsr` / 运算符 `>>` | 新增 | R-sized-10 |

<a id="l3-int32"></a>

### L3: `int32`

现有 24 个函数，另新增 5 个函数。

| 函数 | 签名（当前，显示 mut） | 其他语言主要拼写 | 判定 | 提案 |
|---|---|---|---|---|
| `to_int8` | `int32.to_int8(x: Int32) -> Int8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `int8.from`） | R-L0-40 |
| `to_int16` | `int32.to_int16(x: Int32) -> Int16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `int16.from`） | R-L0-40 |
| `to_int64` | `int32.to_int64(x: Int32) -> Int64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤除（→ `int64.from`） | R-L0-40 |
| `to_uint8` | `int32.to_uint8(x: Int32) -> UInt8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `uint8.from`） | R-L0-40 |
| `to_uint16` | `int32.to_uint16(x: Int32) -> UInt16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `uint16.from`） | R-L0-40 |
| `to_uint32` | `int32.to_uint32(x: Int32) -> UInt32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `uint32.from`） | R-L0-40 |
| `to_uint64` | `int32.to_uint64(x: Int32) -> UInt64` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `uint64.from`） | R-L0-40 |
| `to_float32` | `int32.to_float32(x: Int32) -> Float32` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 撤除（→ `float32.from`） | R-L0-40 |
| `to_float64` | `int32.to_float64(x: Int32) -> Float64` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 撤除（→ `float64.from`） | R-L0-40 |
| `to_string` | `int32.to_string(x: Int32) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | 保留 | — |
| `to_int8_checked` | `int32.to_int8_checked(x: Int32) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `int8.from_checked`） | R-L0-40 |
| `to_int8_saturating` | `int32.to_int8_saturating(x: Int32) -> Int8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `int8.from_saturating`） | R-L0-40 |
| `to_int16_checked` | `int32.to_int16_checked(x: Int32) -> Option[Int16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `int16.from_checked`） | R-L0-40 |
| `to_int16_saturating` | `int32.to_int16_saturating(x: Int32) -> Int16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `int16.from_saturating`） | R-L0-40 |
| `to_uint8_checked` | `int32.to_uint8_checked(x: Int32) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `uint8.from_checked`） | R-L0-40 |
| `to_uint8_saturating` | `int32.to_uint8_saturating(x: Int32) -> UInt8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `uint8.from_saturating`） | R-L0-40 |
| `to_uint16_checked` | `int32.to_uint16_checked(x: Int32) -> Option[UInt16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `uint16.from_checked`） | R-L0-40 |
| `to_uint16_saturating` | `int32.to_uint16_saturating(x: Int32) -> UInt16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `uint16.from_saturating`） | R-L0-40 |
| `to_uint32_checked` | `int32.to_uint32_checked(x: Int32) -> Option[UInt32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `uint32.from_checked`） | R-L0-40 |
| `to_uint32_saturating` | `int32.to_uint32_saturating(x: Int32) -> UInt32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `uint32.from_saturating`） | R-L0-40 |
| `to_uint64_checked` | `int32.to_uint64_checked(x: Int32) -> Option[UInt64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `uint64.from_checked`） | R-L0-40 |
| `to_uint64_saturating` | `int32.to_uint64_saturating(x: Int32) -> UInt64` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `uint64.from_saturating`） | R-L0-40 |
| `min_value` | `int32.min_value() -> Int32` | Rust `MIN` / Go `math.Min…` / Kotlin `MIN_VALUE` / Swift `.min` | 保留 | — |
| `max_value` | `int32.max_value() -> Int32` | Rust `MAX` / Go `math.Max…` / Kotlin `MAX_VALUE` / Swift `.max` | 保留 | — |
| `parse` | （新增）`int32.parse(s: String) -> Result[Int32, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` 等 / Swift `T(s)` | 新增 | R-sized-02 |
| `compare` | （新增）`int32.compare(a: Int32, b: Int32) -> Int` | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | 新增 | R-sized-02 |
| `floor_div` | （新增）`int32.floor_div(a: Int32, b: Int32) -> Int32` | Rust `div_euclid` / Swift — / Python `//` | 新增 | R-sized-10 |
| `modulo` | （新增）`int32.modulo(a: Int32, b: Int32) -> Int32` | Rust `rem_euclid` / Python `%` | 新增 | R-sized-10 |
| `pow` | （新增）`int32.pow(base: Int32, exp: Int) -> Int32` | Rust `pow` / Swift — | 新增 | R-sized-10 |
| `is_even` | （新增）`int32.is_even(n: Int32) -> Bool` | Swift `isMultiple(of: 2)` / Kotlin — | 新增 | R-sized-10 |
| `is_odd` | （新增）`int32.is_odd(n: Int32) -> Bool` | Swift `!isMultiple(of: 2)` | 新增 | R-sized-10 |
| `count_leading_zeros` | （新增）`int32.count_leading_zeros(n: Int32) -> Int` | Rust `leading_zeros` / Swift `leadingZeroBitCount` | 新增 | R-sized-10 |
| `count_trailing_zeros` | （新增）`int32.count_trailing_zeros(n: Int32) -> Int` | Rust `trailing_zeros` / Swift `trailingZeroBitCount` | 新增 | R-sized-10 |
| `count_ones` | （新增）`int32.count_ones(n: Int32) -> Int` | Rust `count_ones` / Swift `nonzeroBitCount` | 新增 | R-sized-10 |
| `byte_swap` | （新增）`int32.byte_swap(n: Int32) -> Int32` | Rust `swap_bytes` / Swift `byteSwapped` | 新增 | R-sized-10 |
| `abs` | （新增）`int32.abs(n: Int32) -> Int32` | Rust `abs` / Swift `abs` | 新增 | R-sized-10 |
| `sign` | （新增）`int32.sign(n: Int32) -> Int` | Rust `signum` / Swift `signum()` | 新增 | R-sized-10 |
| `from` | （新增）`int32.from[S: Numeric](x: S) -> Int32` | Rust `From`・`as` / Swift `init(_:)` / Python `int(x)` | 新增 | R-L0-40 |
| `from_checked` | （新增）`int32.from_checked[S: Numeric](x: S) -> Int32?` | Rust `TryFrom` / Swift `init(exactly:)` | 新增 | R-L0-40 |
| `from_saturating` | （新增）`int32.from_saturating[S: Numeric](x: S) -> Int32` | Swift `init(clamping:)` / Rust `as`（来自浮点数） | 新增 | R-L0-40 |
| `from_wrapping` | （新增）`int32.from_wrapping[S: Integer](x: S) -> Int32` | Swift `init(truncatingIfNeeded:)` / Rust `as` | 新增 | R-L0-40 |
| `wrap_add` | （新增）`int32.wrap_add(a: Int32, b: Int32) -> Int32` | Swift `&+` / Rust `wrapping_add` | 新增 | R-int-12 |
| `wrap_sub` | （新增）`int32.wrap_sub(a: Int32, b: Int32) -> Int32` | Swift `&-` / Rust `wrapping_sub` | 新增 | R-int-12 |
| `wrap_mul` | （新增）`int32.wrap_mul(a: Int32, b: Int32) -> Int32` | Swift `&*` / Rust `wrapping_mul` | 新增 | R-int-12 |
| `rotate_left` | （新增）`int32.rotate_left(a: Int32, n: Int) -> Int32` | Rust `rotate_left` / Go `bits.RotateLeft` | 新增 | R-int-12 |
| `rotate_right` | （新增）`int32.rotate_right(a: Int32, n: Int) -> Int32` | Rust `rotate_right` | 新增 | R-int-12 |
| `bit_length` | （新增）`int32.bit_length(n: Int32) -> Int` | Python `bit_length` / Go `bits.Len` | 新增 | R-int-11 |
| `band` | （新增）`int32.band(a: Int32, b: Int32) -> Int32` | Elixir `band` / Rust・Go・Swift 运算符 `&` | 新增 | R-sized-10 |
| `bor` | （新增）`int32.bor(a: Int32, b: Int32) -> Int32` | Elixir `bor` / Rust・Go・Swift 运算符 `\|` | 新增 | R-sized-10 |
| `bxor` | （新增）`int32.bxor(a: Int32, b: Int32) -> Int32` | Elixir `bxor` / Rust・Go・Swift 运算符 `^` | 新增 | R-sized-10 |
| `bnot` | （新增）`int32.bnot(a: Int32) -> Int32` | Elixir `bnot` / Swift `~` | 新增 | R-sized-10 |
| `bshl` | （新增）`int32.bshl(a: Int32, n: Int) -> Int32` | Elixir `bsl` / 运算符 `<<` | 新增 | R-sized-10 |
| `bshr` | （新增）`int32.bshr(a: Int32, n: Int) -> Int32` | Elixir `bsr` / 运算符 `>>` | 新增 | R-sized-10 |

<a id="l3-int64"></a>

### L3: `int64`

现有 26 个函数，另新增 7 个函数。

| 函数 | 签名（当前，显示 mut） | 其他语言主要拼写 | 判定 | 提案 |
|---|---|---|---|---|
| `to_int8` | `int64.to_int8(x: Int64) -> Int8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `int8.from`） | R-L0-40 |
| `to_int16` | `int64.to_int16(x: Int64) -> Int16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `int16.from`） | R-L0-40 |
| `to_int32` | `int64.to_int32(x: Int64) -> Int32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `int32.from`） | R-L0-40 |
| `to_uint8` | `int64.to_uint8(x: Int64) -> UInt8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `uint8.from`） | R-L0-40 |
| `to_uint16` | `int64.to_uint16(x: Int64) -> UInt16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `uint16.from`） | R-L0-40 |
| `to_uint32` | `int64.to_uint32(x: Int64) -> UInt32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `uint32.from`） | R-L0-40 |
| `to_uint64` | `int64.to_uint64(x: Int64) -> UInt64` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `uint64.from`） | R-L0-40 |
| `to_float32` | `int64.to_float32(x: Int64) -> Float32` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 撤除（→ `float32.from`） | R-L0-40 |
| `to_float64` | `int64.to_float64(x: Int64) -> Float64` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 撤除（→ `float64.from`） | R-L0-40 |
| `to_string` | `int64.to_string(x: Int64) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | 保留 | — |
| `to_int8_checked` | `int64.to_int8_checked(x: Int64) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `int8.from_checked`） | R-L0-40 |
| `to_int8_saturating` | `int64.to_int8_saturating(x: Int64) -> Int8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `int8.from_saturating`） | R-L0-40 |
| `to_int16_checked` | `int64.to_int16_checked(x: Int64) -> Option[Int16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `int16.from_checked`） | R-L0-40 |
| `to_int16_saturating` | `int64.to_int16_saturating(x: Int64) -> Int16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `int16.from_saturating`） | R-L0-40 |
| `to_int32_checked` | `int64.to_int32_checked(x: Int64) -> Option[Int32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `int32.from_checked`） | R-L0-40 |
| `to_int32_saturating` | `int64.to_int32_saturating(x: Int64) -> Int32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `int32.from_saturating`） | R-L0-40 |
| `to_uint8_checked` | `int64.to_uint8_checked(x: Int64) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `uint8.from_checked`） | R-L0-40 |
| `to_uint8_saturating` | `int64.to_uint8_saturating(x: Int64) -> UInt8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `uint8.from_saturating`） | R-L0-40 |
| `to_uint16_checked` | `int64.to_uint16_checked(x: Int64) -> Option[UInt16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `uint16.from_checked`） | R-L0-40 |
| `to_uint16_saturating` | `int64.to_uint16_saturating(x: Int64) -> UInt16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `uint16.from_saturating`） | R-L0-40 |
| `to_uint32_checked` | `int64.to_uint32_checked(x: Int64) -> Option[UInt32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `uint32.from_checked`） | R-L0-40 |
| `to_uint32_saturating` | `int64.to_uint32_saturating(x: Int64) -> UInt32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `uint32.from_saturating`） | R-L0-40 |
| `to_uint64_checked` | `int64.to_uint64_checked(x: Int64) -> Option[UInt64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `uint64.from_checked`） | R-L0-40 |
| `to_uint64_saturating` | `int64.to_uint64_saturating(x: Int64) -> UInt64` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `uint64.from_saturating`） | R-L0-40 |
| `min_value` | `int64.min_value() -> Int64` | Rust `MIN` / Go `math.Min…` / Kotlin `MIN_VALUE` / Swift `.min` | 保留 | — |
| `max_value` | `int64.max_value() -> Int64` | Rust `MAX` / Go `math.Max…` / Kotlin `MAX_VALUE` / Swift `.max` | 保留 | — |
| `parse` | （新增）`int64.parse(s: String) -> Result[Int64, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` 等 / Swift `T(s)` | 新增 | R-sized-02 |
| `compare` | （新增）`int64.compare(a: Int64, b: Int64) -> Int` | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | 新增 | R-sized-02 |
| `floor_div` | （新增）`int64.floor_div(a: Int64, b: Int64) -> Int64` | Rust `div_euclid` / Swift — / Python `//` | 新增 | R-sized-10 |
| `modulo` | （新增）`int64.modulo(a: Int64, b: Int64) -> Int64` | Rust `rem_euclid` / Python `%` | 新增 | R-sized-10 |
| `pow` | （新增）`int64.pow(base: Int64, exp: Int) -> Int64` | Rust `pow` / Swift — | 新增 | R-sized-10 |
| `is_even` | （新增）`int64.is_even(n: Int64) -> Bool` | Swift `isMultiple(of: 2)` / Kotlin — | 新增 | R-sized-10 |
| `is_odd` | （新增）`int64.is_odd(n: Int64) -> Bool` | Swift `!isMultiple(of: 2)` | 新增 | R-sized-10 |
| `count_leading_zeros` | （新增）`int64.count_leading_zeros(n: Int64) -> Int` | Rust `leading_zeros` / Swift `leadingZeroBitCount` | 新增 | R-sized-10 |
| `count_trailing_zeros` | （新增）`int64.count_trailing_zeros(n: Int64) -> Int` | Rust `trailing_zeros` / Swift `trailingZeroBitCount` | 新增 | R-sized-10 |
| `count_ones` | （新增）`int64.count_ones(n: Int64) -> Int` | Rust `count_ones` / Swift `nonzeroBitCount` | 新增 | R-sized-10 |
| `byte_swap` | （新增）`int64.byte_swap(n: Int64) -> Int64` | Rust `swap_bytes` / Swift `byteSwapped` | 新增 | R-sized-10 |
| `abs` | （新增）`int64.abs(n: Int64) -> Int64` | Rust `abs` / Swift `abs` | 新增 | R-sized-10 |
| `sign` | （新增）`int64.sign(n: Int64) -> Int` | Rust `signum` / Swift `signum()` | 新增 | R-sized-10 |
| `from` | （新增）`int64.from[S: Numeric](x: S) -> Int64` | Rust `From`・`as` / Swift `init(_:)` / Python `int(x)` | 新增 | R-L0-40 |
| `from_checked` | （新增）`int64.from_checked[S: Numeric](x: S) -> Int64?` | Rust `TryFrom` / Swift `init(exactly:)` | 新增 | R-L0-40 |
| `from_saturating` | （新增）`int64.from_saturating[S: Numeric](x: S) -> Int64` | Swift `init(clamping:)` / Rust `as`（来自浮点数） | 新增 | R-L0-40 |
| `from_wrapping` | （新增）`int64.from_wrapping[S: Integer](x: S) -> Int64` | Swift `init(truncatingIfNeeded:)` / Rust `as` | 新增 | R-L0-40 |
| `wrap_add` | （新增）`int64.wrap_add(a: Int64, b: Int64) -> Int64` | Swift `&+` / Rust `wrapping_add` | 新增 | R-int-12 |
| `wrap_sub` | （新增）`int64.wrap_sub(a: Int64, b: Int64) -> Int64` | Swift `&-` / Rust `wrapping_sub` | 新增 | R-int-12 |
| `wrap_mul` | （新增）`int64.wrap_mul(a: Int64, b: Int64) -> Int64` | Swift `&*` / Rust `wrapping_mul` | 新增 | R-int-12 |
| `rotate_left` | （新增）`int64.rotate_left(a: Int64, n: Int) -> Int64` | Rust `rotate_left` / Go `bits.RotateLeft` | 新增 | R-int-12 |
| `rotate_right` | （新增）`int64.rotate_right(a: Int64, n: Int) -> Int64` | Rust `rotate_right` | 新增 | R-int-12 |
| `bit_length` | （新增）`int64.bit_length(n: Int64) -> Int` | Python `bit_length` / Go `bits.Len` | 新增 | R-int-11 |
| `band` | （新增）`int64.band(a: Int64, b: Int64) -> Int64` | Elixir `band` / Rust・Go・Swift 运算符 `&` | 新增 | R-sized-10 |
| `bor` | （新增）`int64.bor(a: Int64, b: Int64) -> Int64` | Elixir `bor` / Rust・Go・Swift 运算符 `\|` | 新增 | R-sized-10 |
| `bxor` | （新增）`int64.bxor(a: Int64, b: Int64) -> Int64` | Elixir `bxor` / Rust・Go・Swift 运算符 `^` | 新增 | R-sized-10 |
| `bnot` | （新增）`int64.bnot(a: Int64) -> Int64` | Elixir `bnot` / Swift `~` | 新增 | R-sized-10 |
| `bshl` | （新增）`int64.bshl(a: Int64, n: Int) -> Int64` | Elixir `bsl` / 运算符 `<<` | 新增 | R-sized-10 |
| `bshr` | （新增）`int64.bshr(a: Int64, n: Int) -> Int64` | Elixir `bsr` / 运算符 `>>` | 新增 | R-sized-10 |

<a id="l3-uint8"></a>

### L3: `uint8`

现有 14 个函数，另新增 4 个函数。

| 函数 | 签名（当前，显示 mut） | 其他语言主要拼写 | 判定 | 提案 |
|---|---|---|---|---|
| `to_int8` | `uint8.to_int8(x: UInt8) -> Int8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `int8.from`） | R-L0-40 |
| `to_int16` | `uint8.to_int16(x: UInt8) -> Int16` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤除（→ `int16.from`） | R-L0-40 |
| `to_int32` | `uint8.to_int32(x: UInt8) -> Int32` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤除（→ `int32.from`） | R-L0-40 |
| `to_int64` | `uint8.to_int64(x: UInt8) -> Int64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤除（→ `int64.from`） | R-L0-40 |
| `to_uint16` | `uint8.to_uint16(x: UInt8) -> UInt16` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤除（→ `uint16.from`） | R-L0-40 |
| `to_uint32` | `uint8.to_uint32(x: UInt8) -> UInt32` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤除（→ `uint32.from`） | R-L0-40 |
| `to_uint64` | `uint8.to_uint64(x: UInt8) -> UInt64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤除（→ `uint64.from`） | R-L0-40 |
| `to_float32` | `uint8.to_float32(x: UInt8) -> Float32` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 撤除（→ `float32.from`） | R-L0-40 |
| `to_float64` | `uint8.to_float64(x: UInt8) -> Float64` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 撤除（→ `float64.from`） | R-L0-40 |
| `to_string` | `uint8.to_string(x: UInt8) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | 保留 | — |
| `to_int8_checked` | `uint8.to_int8_checked(x: UInt8) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `int8.from_checked`） | R-L0-40 |
| `to_int8_saturating` | `uint8.to_int8_saturating(x: UInt8) -> Int8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `int8.from_saturating`） | R-L0-40 |
| `min_value` | `uint8.min_value() -> UInt8` | Rust `MIN` / Go `math.Min…` / Kotlin `MIN_VALUE` / Swift `.min` | 保留 | — |
| `max_value` | `uint8.max_value() -> UInt8` | Rust `MAX` / Go `math.Max…` / Kotlin `MAX_VALUE` / Swift `.max` | 保留 | — |
| `parse` | （新增）`uint8.parse(s: String) -> Result[UInt8, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` 等 / Swift `T(s)` | 新增 | R-sized-02 |
| `compare` | （新增）`uint8.compare(a: UInt8, b: UInt8) -> Int` | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | 新增 | R-sized-02 |
| `floor_div` | （新增）`uint8.floor_div(a: UInt8, b: UInt8) -> UInt8` | Rust `div_euclid` / Swift — / Python `//` | 新增 | R-sized-10 |
| `modulo` | （新增）`uint8.modulo(a: UInt8, b: UInt8) -> UInt8` | Rust `rem_euclid` / Python `%` | 新增 | R-sized-10 |
| `pow` | （新增）`uint8.pow(base: UInt8, exp: Int) -> UInt8` | Rust `pow` / Swift — | 新增 | R-sized-10 |
| `is_even` | （新增）`uint8.is_even(n: UInt8) -> Bool` | Swift `isMultiple(of: 2)` / Kotlin — | 新增 | R-sized-10 |
| `is_odd` | （新增）`uint8.is_odd(n: UInt8) -> Bool` | Swift `!isMultiple(of: 2)` | 新增 | R-sized-10 |
| `count_leading_zeros` | （新增）`uint8.count_leading_zeros(n: UInt8) -> Int` | Rust `leading_zeros` / Swift `leadingZeroBitCount` | 新增 | R-sized-10 |
| `count_trailing_zeros` | （新增）`uint8.count_trailing_zeros(n: UInt8) -> Int` | Rust `trailing_zeros` / Swift `trailingZeroBitCount` | 新增 | R-sized-10 |
| `count_ones` | （新增）`uint8.count_ones(n: UInt8) -> Int` | Rust `count_ones` / Swift `nonzeroBitCount` | 新增 | R-sized-10 |
| `byte_swap` | （新增）`uint8.byte_swap(n: UInt8) -> UInt8` | Rust `swap_bytes` / Swift `byteSwapped` | 新增 | R-sized-10 |
| `from` | （新增）`uint8.from[S: Numeric](x: S) -> UInt8` | Rust `From`・`as` / Swift `init(_:)` / Python `int(x)` | 新增 | R-L0-40 |
| `from_checked` | （新增）`uint8.from_checked[S: Numeric](x: S) -> UInt8?` | Rust `TryFrom` / Swift `init(exactly:)` | 新增 | R-L0-40 |
| `from_saturating` | （新增）`uint8.from_saturating[S: Numeric](x: S) -> UInt8` | Swift `init(clamping:)` / Rust `as`（来自浮点数） | 新增 | R-L0-40 |
| `from_wrapping` | （新增）`uint8.from_wrapping[S: Integer](x: S) -> UInt8` | Swift `init(truncatingIfNeeded:)` / Rust `as` | 新增 | R-L0-40 |
| `wrap_add` | （新增）`uint8.wrap_add(a: UInt8, b: UInt8) -> UInt8` | Swift `&+` / Rust `wrapping_add` | 新增 | R-int-12 |
| `wrap_sub` | （新增）`uint8.wrap_sub(a: UInt8, b: UInt8) -> UInt8` | Swift `&-` / Rust `wrapping_sub` | 新增 | R-int-12 |
| `wrap_mul` | （新增）`uint8.wrap_mul(a: UInt8, b: UInt8) -> UInt8` | Swift `&*` / Rust `wrapping_mul` | 新增 | R-int-12 |
| `rotate_left` | （新增）`uint8.rotate_left(a: UInt8, n: Int) -> UInt8` | Rust `rotate_left` / Go `bits.RotateLeft` | 新增 | R-int-12 |
| `rotate_right` | （新增）`uint8.rotate_right(a: UInt8, n: Int) -> UInt8` | Rust `rotate_right` | 新增 | R-int-12 |
| `bit_length` | （新增）`uint8.bit_length(n: UInt8) -> Int` | Python `bit_length` / Go `bits.Len` | 新增 | R-int-11 |
| `band` | （新增）`uint8.band(a: UInt8, b: UInt8) -> UInt8` | Elixir `band` / Rust・Go・Swift 运算符 `&` | 新增 | R-sized-10 |
| `bor` | （新增）`uint8.bor(a: UInt8, b: UInt8) -> UInt8` | Elixir `bor` / Rust・Go・Swift 运算符 `\|` | 新增 | R-sized-10 |
| `bxor` | （新增）`uint8.bxor(a: UInt8, b: UInt8) -> UInt8` | Elixir `bxor` / Rust・Go・Swift 运算符 `^` | 新增 | R-sized-10 |
| `bnot` | （新增）`uint8.bnot(a: UInt8) -> UInt8` | Elixir `bnot` / Swift `~` | 新增 | R-sized-10 |
| `bshl` | （新增）`uint8.bshl(a: UInt8, n: Int) -> UInt8` | Elixir `bsl` / 运算符 `<<` | 新增 | R-sized-10 |
| `bshr` | （新增）`uint8.bshr(a: UInt8, n: Int) -> UInt8` | Elixir `bsr` / 运算符 `>>` | 新增 | R-sized-10 |

<a id="l3-uint16"></a>

### L3: `uint16`

现有 18 个函数，另新增 4 个函数。

| 函数 | 签名（当前，显示 mut） | 其他语言主要拼写 | 判定 | 提案 |
|---|---|---|---|---|
| `to_int8` | `uint16.to_int8(x: UInt16) -> Int8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `int8.from`） | R-L0-40 |
| `to_int16` | `uint16.to_int16(x: UInt16) -> Int16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `int16.from`） | R-L0-40 |
| `to_int32` | `uint16.to_int32(x: UInt16) -> Int32` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤除（→ `int32.from`） | R-L0-40 |
| `to_int64` | `uint16.to_int64(x: UInt16) -> Int64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤除（→ `int64.from`） | R-L0-40 |
| `to_uint8` | `uint16.to_uint8(x: UInt16) -> UInt8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `uint8.from`） | R-L0-40 |
| `to_uint32` | `uint16.to_uint32(x: UInt16) -> UInt32` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤除（→ `uint32.from`） | R-L0-40 |
| `to_uint64` | `uint16.to_uint64(x: UInt16) -> UInt64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤除（→ `uint64.from`） | R-L0-40 |
| `to_float32` | `uint16.to_float32(x: UInt16) -> Float32` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 撤除（→ `float32.from`） | R-L0-40 |
| `to_float64` | `uint16.to_float64(x: UInt16) -> Float64` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 撤除（→ `float64.from`） | R-L0-40 |
| `to_string` | `uint16.to_string(x: UInt16) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | 保留 | — |
| `to_int8_checked` | `uint16.to_int8_checked(x: UInt16) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `int8.from_checked`） | R-L0-40 |
| `to_int8_saturating` | `uint16.to_int8_saturating(x: UInt16) -> Int8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `int8.from_saturating`） | R-L0-40 |
| `to_int16_checked` | `uint16.to_int16_checked(x: UInt16) -> Option[Int16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `int16.from_checked`） | R-L0-40 |
| `to_int16_saturating` | `uint16.to_int16_saturating(x: UInt16) -> Int16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `int16.from_saturating`） | R-L0-40 |
| `to_uint8_checked` | `uint16.to_uint8_checked(x: UInt16) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `uint8.from_checked`） | R-L0-40 |
| `to_uint8_saturating` | `uint16.to_uint8_saturating(x: UInt16) -> UInt8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `uint8.from_saturating`） | R-L0-40 |
| `min_value` | `uint16.min_value() -> UInt16` | Rust `MIN` / Go `math.Min…` / Kotlin `MIN_VALUE` / Swift `.min` | 保留 | — |
| `max_value` | `uint16.max_value() -> UInt16` | Rust `MAX` / Go `math.Max…` / Kotlin `MAX_VALUE` / Swift `.max` | 保留 | — |
| `parse` | （新增）`uint16.parse(s: String) -> Result[UInt16, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` 等 / Swift `T(s)` | 新增 | R-sized-02 |
| `compare` | （新增）`uint16.compare(a: UInt16, b: UInt16) -> Int` | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | 新增 | R-sized-02 |
| `floor_div` | （新增）`uint16.floor_div(a: UInt16, b: UInt16) -> UInt16` | Rust `div_euclid` / Swift — / Python `//` | 新增 | R-sized-10 |
| `modulo` | （新增）`uint16.modulo(a: UInt16, b: UInt16) -> UInt16` | Rust `rem_euclid` / Python `%` | 新增 | R-sized-10 |
| `pow` | （新增）`uint16.pow(base: UInt16, exp: Int) -> UInt16` | Rust `pow` / Swift — | 新增 | R-sized-10 |
| `is_even` | （新增）`uint16.is_even(n: UInt16) -> Bool` | Swift `isMultiple(of: 2)` / Kotlin — | 新增 | R-sized-10 |
| `is_odd` | （新增）`uint16.is_odd(n: UInt16) -> Bool` | Swift `!isMultiple(of: 2)` | 新增 | R-sized-10 |
| `count_leading_zeros` | （新增）`uint16.count_leading_zeros(n: UInt16) -> Int` | Rust `leading_zeros` / Swift `leadingZeroBitCount` | 新增 | R-sized-10 |
| `count_trailing_zeros` | （新增）`uint16.count_trailing_zeros(n: UInt16) -> Int` | Rust `trailing_zeros` / Swift `trailingZeroBitCount` | 新增 | R-sized-10 |
| `count_ones` | （新增）`uint16.count_ones(n: UInt16) -> Int` | Rust `count_ones` / Swift `nonzeroBitCount` | 新增 | R-sized-10 |
| `byte_swap` | （新增）`uint16.byte_swap(n: UInt16) -> UInt16` | Rust `swap_bytes` / Swift `byteSwapped` | 新增 | R-sized-10 |
| `from` | （新增）`uint16.from[S: Numeric](x: S) -> UInt16` | Rust `From`・`as` / Swift `init(_:)` / Python `int(x)` | 新增 | R-L0-40 |
| `from_checked` | （新增）`uint16.from_checked[S: Numeric](x: S) -> UInt16?` | Rust `TryFrom` / Swift `init(exactly:)` | 新增 | R-L0-40 |
| `from_saturating` | （新增）`uint16.from_saturating[S: Numeric](x: S) -> UInt16` | Swift `init(clamping:)` / Rust `as`（来自浮点数） | 新增 | R-L0-40 |
| `from_wrapping` | （新增）`uint16.from_wrapping[S: Integer](x: S) -> UInt16` | Swift `init(truncatingIfNeeded:)` / Rust `as` | 新增 | R-L0-40 |
| `wrap_add` | （新增）`uint16.wrap_add(a: UInt16, b: UInt16) -> UInt16` | Swift `&+` / Rust `wrapping_add` | 新增 | R-int-12 |
| `wrap_sub` | （新增）`uint16.wrap_sub(a: UInt16, b: UInt16) -> UInt16` | Swift `&-` / Rust `wrapping_sub` | 新增 | R-int-12 |
| `wrap_mul` | （新增）`uint16.wrap_mul(a: UInt16, b: UInt16) -> UInt16` | Swift `&*` / Rust `wrapping_mul` | 新增 | R-int-12 |
| `rotate_left` | （新增）`uint16.rotate_left(a: UInt16, n: Int) -> UInt16` | Rust `rotate_left` / Go `bits.RotateLeft` | 新增 | R-int-12 |
| `rotate_right` | （新增）`uint16.rotate_right(a: UInt16, n: Int) -> UInt16` | Rust `rotate_right` | 新增 | R-int-12 |
| `bit_length` | （新增）`uint16.bit_length(n: UInt16) -> Int` | Python `bit_length` / Go `bits.Len` | 新增 | R-int-11 |
| `band` | （新增）`uint16.band(a: UInt16, b: UInt16) -> UInt16` | Elixir `band` / Rust・Go・Swift 运算符 `&` | 新增 | R-sized-10 |
| `bor` | （新增）`uint16.bor(a: UInt16, b: UInt16) -> UInt16` | Elixir `bor` / Rust・Go・Swift 运算符 `\|` | 新增 | R-sized-10 |
| `bxor` | （新增）`uint16.bxor(a: UInt16, b: UInt16) -> UInt16` | Elixir `bxor` / Rust・Go・Swift 运算符 `^` | 新增 | R-sized-10 |
| `bnot` | （新增）`uint16.bnot(a: UInt16) -> UInt16` | Elixir `bnot` / Swift `~` | 新增 | R-sized-10 |
| `bshl` | （新增）`uint16.bshl(a: UInt16, n: Int) -> UInt16` | Elixir `bsl` / 运算符 `<<` | 新增 | R-sized-10 |
| `bshr` | （新增）`uint16.bshr(a: UInt16, n: Int) -> UInt16` | Elixir `bsr` / 运算符 `>>` | 新增 | R-sized-10 |

<a id="l3-uint32"></a>

### L3: `uint32`

现有 22 个函数，另新增 5 个函数。

| 函数 | 签名（当前，显示 mut） | 其他语言主要拼写 | 判定 | 提案 |
|---|---|---|---|---|
| `to_int8` | `uint32.to_int8(x: UInt32) -> Int8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `int8.from`） | R-L0-40 |
| `to_int16` | `uint32.to_int16(x: UInt32) -> Int16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `int16.from`） | R-L0-40 |
| `to_int32` | `uint32.to_int32(x: UInt32) -> Int32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `int32.from`） | R-L0-40 |
| `to_int64` | `uint32.to_int64(x: UInt32) -> Int64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤除（→ `int64.from`） | R-L0-40 |
| `to_uint8` | `uint32.to_uint8(x: UInt32) -> UInt8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `uint8.from`） | R-L0-40 |
| `to_uint16` | `uint32.to_uint16(x: UInt32) -> UInt16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `uint16.from`） | R-L0-40 |
| `to_uint64` | `uint32.to_uint64(x: UInt32) -> UInt64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤除（→ `uint64.from`） | R-L0-40 |
| `to_float32` | `uint32.to_float32(x: UInt32) -> Float32` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 撤除（→ `float32.from`） | R-L0-40 |
| `to_float64` | `uint32.to_float64(x: UInt32) -> Float64` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 撤除（→ `float64.from`） | R-L0-40 |
| `to_string` | `uint32.to_string(x: UInt32) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | 保留 | — |
| `to_int8_checked` | `uint32.to_int8_checked(x: UInt32) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `int8.from_checked`） | R-L0-40 |
| `to_int8_saturating` | `uint32.to_int8_saturating(x: UInt32) -> Int8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `int8.from_saturating`） | R-L0-40 |
| `to_int16_checked` | `uint32.to_int16_checked(x: UInt32) -> Option[Int16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `int16.from_checked`） | R-L0-40 |
| `to_int16_saturating` | `uint32.to_int16_saturating(x: UInt32) -> Int16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `int16.from_saturating`） | R-L0-40 |
| `to_int32_checked` | `uint32.to_int32_checked(x: UInt32) -> Option[Int32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `int32.from_checked`） | R-L0-40 |
| `to_int32_saturating` | `uint32.to_int32_saturating(x: UInt32) -> Int32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `int32.from_saturating`） | R-L0-40 |
| `to_uint8_checked` | `uint32.to_uint8_checked(x: UInt32) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `uint8.from_checked`） | R-L0-40 |
| `to_uint8_saturating` | `uint32.to_uint8_saturating(x: UInt32) -> UInt8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `uint8.from_saturating`） | R-L0-40 |
| `to_uint16_checked` | `uint32.to_uint16_checked(x: UInt32) -> Option[UInt16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `uint16.from_checked`） | R-L0-40 |
| `to_uint16_saturating` | `uint32.to_uint16_saturating(x: UInt32) -> UInt16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `uint16.from_saturating`） | R-L0-40 |
| `min_value` | `uint32.min_value() -> UInt32` | Rust `MIN` / Go `math.Min…` / Kotlin `MIN_VALUE` / Swift `.min` | 保留 | — |
| `max_value` | `uint32.max_value() -> UInt32` | Rust `MAX` / Go `math.Max…` / Kotlin `MAX_VALUE` / Swift `.max` | 保留 | — |
| `parse` | （新增）`uint32.parse(s: String) -> Result[UInt32, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` 等 / Swift `T(s)` | 新增 | R-sized-02 |
| `compare` | （新增）`uint32.compare(a: UInt32, b: UInt32) -> Int` | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | 新增 | R-sized-02 |
| `floor_div` | （新增）`uint32.floor_div(a: UInt32, b: UInt32) -> UInt32` | Rust `div_euclid` / Swift — / Python `//` | 新增 | R-sized-10 |
| `modulo` | （新增）`uint32.modulo(a: UInt32, b: UInt32) -> UInt32` | Rust `rem_euclid` / Python `%` | 新增 | R-sized-10 |
| `pow` | （新增）`uint32.pow(base: UInt32, exp: Int) -> UInt32` | Rust `pow` / Swift — | 新增 | R-sized-10 |
| `is_even` | （新增）`uint32.is_even(n: UInt32) -> Bool` | Swift `isMultiple(of: 2)` / Kotlin — | 新增 | R-sized-10 |
| `is_odd` | （新增）`uint32.is_odd(n: UInt32) -> Bool` | Swift `!isMultiple(of: 2)` | 新增 | R-sized-10 |
| `count_leading_zeros` | （新增）`uint32.count_leading_zeros(n: UInt32) -> Int` | Rust `leading_zeros` / Swift `leadingZeroBitCount` | 新增 | R-sized-10 |
| `count_trailing_zeros` | （新增）`uint32.count_trailing_zeros(n: UInt32) -> Int` | Rust `trailing_zeros` / Swift `trailingZeroBitCount` | 新增 | R-sized-10 |
| `count_ones` | （新增）`uint32.count_ones(n: UInt32) -> Int` | Rust `count_ones` / Swift `nonzeroBitCount` | 新增 | R-sized-10 |
| `byte_swap` | （新增）`uint32.byte_swap(n: UInt32) -> UInt32` | Rust `swap_bytes` / Swift `byteSwapped` | 新增 | R-sized-10 |
| `from` | （新增）`uint32.from[S: Numeric](x: S) -> UInt32` | Rust `From`・`as` / Swift `init(_:)` / Python `int(x)` | 新增 | R-L0-40 |
| `from_checked` | （新增）`uint32.from_checked[S: Numeric](x: S) -> UInt32?` | Rust `TryFrom` / Swift `init(exactly:)` | 新增 | R-L0-40 |
| `from_saturating` | （新增）`uint32.from_saturating[S: Numeric](x: S) -> UInt32` | Swift `init(clamping:)` / Rust `as`（来自浮点数） | 新增 | R-L0-40 |
| `from_wrapping` | （新增）`uint32.from_wrapping[S: Integer](x: S) -> UInt32` | Swift `init(truncatingIfNeeded:)` / Rust `as` | 新增 | R-L0-40 |
| `wrap_add` | （新增）`uint32.wrap_add(a: UInt32, b: UInt32) -> UInt32` | Swift `&+` / Rust `wrapping_add` | 新增 | R-int-12 |
| `wrap_sub` | （新增）`uint32.wrap_sub(a: UInt32, b: UInt32) -> UInt32` | Swift `&-` / Rust `wrapping_sub` | 新增 | R-int-12 |
| `wrap_mul` | （新增）`uint32.wrap_mul(a: UInt32, b: UInt32) -> UInt32` | Swift `&*` / Rust `wrapping_mul` | 新增 | R-int-12 |
| `rotate_left` | （新增）`uint32.rotate_left(a: UInt32, n: Int) -> UInt32` | Rust `rotate_left` / Go `bits.RotateLeft` | 新增 | R-int-12 |
| `rotate_right` | （新增）`uint32.rotate_right(a: UInt32, n: Int) -> UInt32` | Rust `rotate_right` | 新增 | R-int-12 |
| `bit_length` | （新增）`uint32.bit_length(n: UInt32) -> Int` | Python `bit_length` / Go `bits.Len` | 新增 | R-int-11 |
| `band` | （新增）`uint32.band(a: UInt32, b: UInt32) -> UInt32` | Elixir `band` / Rust・Go・Swift 运算符 `&` | 新增 | R-sized-10 |
| `bor` | （新增）`uint32.bor(a: UInt32, b: UInt32) -> UInt32` | Elixir `bor` / Rust・Go・Swift 运算符 `\|` | 新增 | R-sized-10 |
| `bxor` | （新增）`uint32.bxor(a: UInt32, b: UInt32) -> UInt32` | Elixir `bxor` / Rust・Go・Swift 运算符 `^` | 新增 | R-sized-10 |
| `bnot` | （新增）`uint32.bnot(a: UInt32) -> UInt32` | Elixir `bnot` / Swift `~` | 新增 | R-sized-10 |
| `bshl` | （新增）`uint32.bshl(a: UInt32, n: Int) -> UInt32` | Elixir `bsl` / 运算符 `<<` | 新增 | R-sized-10 |
| `bshr` | （新增）`uint32.bshr(a: UInt32, n: Int) -> UInt32` | Elixir `bsr` / 运算符 `>>` | 新增 | R-sized-10 |

<a id="l3-uint64"></a>

### L3: `uint64`

现有 26 个函数，另新增 9 个函数。

| 函数 | 签名（当前，显示 mut） | 其他语言主要拼写 | 判定 | 提案 |
|---|---|---|---|---|
| `to_int8` | `uint64.to_int8(x: UInt64) -> Int8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `int8.from`） | R-L0-40 |
| `to_int16` | `uint64.to_int16(x: UInt64) -> Int16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `int16.from`） | R-L0-40 |
| `to_int32` | `uint64.to_int32(x: UInt64) -> Int32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `int32.from`） | R-L0-40 |
| `to_int64` | `uint64.to_int64(x: UInt64) -> Int64` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `int64.from`） | R-L0-40 |
| `to_uint8` | `uint64.to_uint8(x: UInt64) -> UInt8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `uint8.from`） | R-L0-40 |
| `to_uint16` | `uint64.to_uint16(x: UInt64) -> UInt16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `uint16.from`） | R-L0-40 |
| `to_uint32` | `uint64.to_uint32(x: UInt64) -> UInt32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤除（→ `uint32.from`） | R-L0-40 |
| `to_float32` | `uint64.to_float32(x: UInt64) -> Float32` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 撤除（→ `float32.from`） | R-L0-40 |
| `to_float64` | `uint64.to_float64(x: UInt64) -> Float64` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 撤除（→ `float64.from`） | R-L0-40 |
| `to_string` | `uint64.to_string(x: UInt64) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | 保留 | — |
| `to_int8_checked` | `uint64.to_int8_checked(x: UInt64) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `int8.from_checked`） | R-L0-40 |
| `to_int8_saturating` | `uint64.to_int8_saturating(x: UInt64) -> Int8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `int8.from_saturating`） | R-L0-40 |
| `to_int16_checked` | `uint64.to_int16_checked(x: UInt64) -> Option[Int16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `int16.from_checked`） | R-L0-40 |
| `to_int16_saturating` | `uint64.to_int16_saturating(x: UInt64) -> Int16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `int16.from_saturating`） | R-L0-40 |
| `to_int32_checked` | `uint64.to_int32_checked(x: UInt64) -> Option[Int32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `int32.from_checked`） | R-L0-40 |
| `to_int32_saturating` | `uint64.to_int32_saturating(x: UInt64) -> Int32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `int32.from_saturating`） | R-L0-40 |
| `to_int64_checked` | `uint64.to_int64_checked(x: UInt64) -> Option[Int64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `int64.from_checked`） | R-L0-40 |
| `to_int64_saturating` | `uint64.to_int64_saturating(x: UInt64) -> Int64` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `int64.from_saturating`） | R-L0-40 |
| `to_uint8_checked` | `uint64.to_uint8_checked(x: UInt64) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `uint8.from_checked`） | R-L0-40 |
| `to_uint8_saturating` | `uint64.to_uint8_saturating(x: UInt64) -> UInt8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `uint8.from_saturating`） | R-L0-40 |
| `to_uint16_checked` | `uint64.to_uint16_checked(x: UInt64) -> Option[UInt16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `uint16.from_checked`） | R-L0-40 |
| `to_uint16_saturating` | `uint64.to_uint16_saturating(x: UInt64) -> UInt16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `uint16.from_saturating`） | R-L0-40 |
| `to_uint32_checked` | `uint64.to_uint32_checked(x: UInt64) -> Option[UInt32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`族 / 其他 6 种语言 — | 撤除（→ `uint32.from_checked`） | R-L0-40 |
| `to_uint32_saturating` | `uint64.to_uint32_saturating(x: UInt64) -> UInt32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 撤除（→ `uint32.from_saturating`） | R-L0-40 |
| `min_value` | `uint64.min_value() -> UInt64` | Rust `MIN` / Go `math.Min…` / Kotlin `MIN_VALUE` / Swift `.min` | 保留 | — |
| `max_value` | `uint64.max_value() -> UInt64` | Rust `MAX` / Go `math.Max…` / Kotlin `MAX_VALUE` / Swift `.max` | 保留 | — |
| `parse` | （新增）`uint64.parse(s: String) -> Result[UInt64, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` 等 / Swift `T(s)` | 新增 | R-sized-02 |
| `compare` | （新增）`uint64.compare(a: UInt64, b: UInt64) -> Int` | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | 新增 | R-sized-02 |
| `floor_div` | （新增）`uint64.floor_div(a: UInt64, b: UInt64) -> UInt64` | Rust `div_euclid` / Swift — / Python `//` | 新增 | R-sized-10 |
| `modulo` | （新增）`uint64.modulo(a: UInt64, b: UInt64) -> UInt64` | Rust `rem_euclid` / Python `%` | 新增 | R-sized-10 |
| `pow` | （新增）`uint64.pow(base: UInt64, exp: Int) -> UInt64` | Rust `pow` / Swift — | 新增 | R-sized-10 |
| `is_even` | （新增）`uint64.is_even(n: UInt64) -> Bool` | Swift `isMultiple(of: 2)` / Kotlin — | 新增 | R-sized-10 |
| `is_odd` | （新增）`uint64.is_odd(n: UInt64) -> Bool` | Swift `!isMultiple(of: 2)` | 新增 | R-sized-10 |
| `count_leading_zeros` | （新增）`uint64.count_leading_zeros(n: UInt64) -> Int` | Rust `leading_zeros` / Swift `leadingZeroBitCount` | 新增 | R-sized-10 |
| `count_trailing_zeros` | （新增）`uint64.count_trailing_zeros(n: UInt64) -> Int` | Rust `trailing_zeros` / Swift `trailingZeroBitCount` | 新增 | R-sized-10 |
| `count_ones` | （新增）`uint64.count_ones(n: UInt64) -> Int` | Rust `count_ones` / Swift `nonzeroBitCount` | 新增 | R-sized-10 |
| `byte_swap` | （新增）`uint64.byte_swap(n: UInt64) -> UInt64` | Rust `swap_bytes` / Swift `byteSwapped` | 新增 | R-sized-10 |
| `from` | （新增）`uint64.from[S: Numeric](x: S) -> UInt64` | Rust `From`・`as` / Swift `init(_:)` / Python `int(x)` | 新增 | R-L0-40 |
| `from_checked` | （新增）`uint64.from_checked[S: Numeric](x: S) -> UInt64?` | Rust `TryFrom` / Swift `init(exactly:)` | 新增 | R-L0-40 |
| `from_saturating` | （新增）`uint64.from_saturating[S: Numeric](x: S) -> UInt64` | Swift `init(clamping:)` / Rust `as`（来自浮点数） | 新增 | R-L0-40 |
| `from_wrapping` | （新增）`uint64.from_wrapping[S: Integer](x: S) -> UInt64` | Swift `init(truncatingIfNeeded:)` / Rust `as` | 新增 | R-L0-40 |
| `wrap_add` | （新增）`uint64.wrap_add(a: UInt64, b: UInt64) -> UInt64` | Swift `&+` / Rust `wrapping_add` | 新增 | R-int-12 |
| `wrap_sub` | （新增）`uint64.wrap_sub(a: UInt64, b: UInt64) -> UInt64` | Swift `&-` / Rust `wrapping_sub` | 新增 | R-int-12 |
| `wrap_mul` | （新增）`uint64.wrap_mul(a: UInt64, b: UInt64) -> UInt64` | Swift `&*` / Rust `wrapping_mul` | 新增 | R-int-12 |
| `rotate_left` | （新增）`uint64.rotate_left(a: UInt64, n: Int) -> UInt64` | Rust `rotate_left` / Go `bits.RotateLeft` | 新增 | R-int-12 |
| `rotate_right` | （新增）`uint64.rotate_right(a: UInt64, n: Int) -> UInt64` | Rust `rotate_right` | 新增 | R-int-12 |
| `bit_length` | （新增）`uint64.bit_length(n: UInt64) -> Int` | Python `bit_length` / Go `bits.Len` | 新增 | R-int-11 |
| `band` | （新增）`uint64.band(a: UInt64, b: UInt64) -> UInt64` | Elixir `band` / Rust・Go・Swift 运算符 `&` | 新增 | R-sized-10 |
| `bor` | （新增）`uint64.bor(a: UInt64, b: UInt64) -> UInt64` | Elixir `bor` / Rust・Go・Swift 运算符 `\|` | 新增 | R-sized-10 |
| `bxor` | （新增）`uint64.bxor(a: UInt64, b: UInt64) -> UInt64` | Elixir `bxor` / Rust・Go・Swift 运算符 `^` | 新增 | R-sized-10 |
| `bnot` | （新增）`uint64.bnot(a: UInt64) -> UInt64` | Elixir `bnot` / Swift `~` | 新增 | R-sized-10 |
| `bshl` | （新增）`uint64.bshl(a: UInt64, n: Int) -> UInt64` | Elixir `bsl` / 运算符 `<<` | 新增 | R-sized-10 |
| `bshr` | （新增）`uint64.bshr(a: UInt64, n: Int) -> UInt64` | Elixir `bsr` / 运算符 `>>` | 新增 | R-sized-10 |

<a id="l3-float32"></a>

### L3: `float32`

现有 10 个函数，另新增 20 个函数。

| 函数 | 签名（当前，显示 mut） | 其他语言主要拼写 | 判定 | 提案 |
|---|---|---|---|---|
| `to_int8` | `float32.to_int8(x: Float32) -> Int8` | Rust `as`（饱和） / Kotlin `toInt()`（饱和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 撤除（→ `int8.from`） | R-L0-40 |
| `to_int16` | `float32.to_int16(x: Float32) -> Int16` | Rust `as`（饱和） / Kotlin `toInt()`（饱和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 撤除（→ `int16.from`） | R-L0-40 |
| `to_int32` | `float32.to_int32(x: Float32) -> Int32` | Rust `as`（饱和） / Kotlin `toInt()`（饱和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 撤除（→ `int32.from`） | R-L0-40 |
| `to_int64` | `float32.to_int64(x: Float32) -> Int64` | Rust `as`（饱和） / Kotlin `toInt()`（饱和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 撤除（→ `int64.from`） | R-L0-40 |
| `to_uint8` | `float32.to_uint8(x: Float32) -> UInt8` | Rust `as`（饱和） / Kotlin `toInt()`（饱和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 撤除（→ `uint8.from`） | R-L0-40 |
| `to_uint16` | `float32.to_uint16(x: Float32) -> UInt16` | Rust `as`（饱和） / Kotlin `toInt()`（饱和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 撤除（→ `uint16.from`） | R-L0-40 |
| `to_uint32` | `float32.to_uint32(x: Float32) -> UInt32` | Rust `as`（饱和） / Kotlin `toInt()`（饱和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 撤除（→ `uint32.from`） | R-L0-40 |
| `to_uint64` | `float32.to_uint64(x: Float32) -> UInt64` | Rust `as`（饱和） / Kotlin `toInt()`（饱和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 撤除（→ `uint64.from`） | R-L0-40 |
| `to_float64` | `float32.to_float64(x: Float32) -> Float64` | Rust `as` / Kotlin `toFloat()` / Swift `init(_:)` | 撤除（→ `float64.from`） | R-L0-40 |
| `to_string` | `float32.to_string(x: Float32) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | 保留 | — |
| `parse` | （新增）`float32.parse(s: String) -> Result[Float32, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` 等 / Swift `T(s)` | 新增 | R-sized-02 |
| `compare` | （新增）`float32.compare(a: Float32, b: Float32) -> Int`（IEEE totalOrder） | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | 新增 | R-sized-02 |
| `to_bits` | （新增）`float32.to_bits(x: Float32) -> Int` | Rust `to_bits` (F) / Go `math.Float32bits` / Kotlin `toBits` | 新增 | R-sized-03 |
| `from_bits` | （新增）`float32.from_bits(bits: Int) -> Float32` | Rust `from_bits` (F) / Go `math.Float32frombits` / Kotlin `fromBits` | 新增 | R-sized-03 |
| `infinity` | （新增）`float32.infinity() -> Float32` | Rust `INFINITY` / Swift `.infinity` | 新增 | R-sized-03 |
| `nan` | （新增）`float32.nan() -> Float32` | Rust `NAN` / Swift `.nan` | 新增 | R-sized-03 |
| `epsilon` | （新增）`float32.epsilon() -> Float32` | Rust `EPSILON` / Swift `.ulpOfOne` | 新增 | R-sized-03 |
| `max_value` | （新增）`float32.max_value() -> Float32` | Rust `MAX` / Go `math.MaxFloat32` / Kotlin `MAX_VALUE` | 新增 | R-sized-03 |
| `min_value` | （新增）`float32.min_value() -> Float32` | Rust `MIN` / Swift `-.greatestFiniteMagnitude` | 新增 | R-sized-03 |
| `abs` | （新增）`float32.abs(x: Float32) -> Float32` | Rust `abs` / Swift `magnitude` | 新增 | R-sized-11 |
| `floor` | （新增）`float32.floor(x: Float32) -> Float32` | Rust `floor` / Swift `rounded(.down)` | 新增 | R-sized-11 |
| `ceil` | （新增）`float32.ceil(x: Float32) -> Float32` | Rust `ceil` / Swift `rounded(.up)` | 新增 | R-sized-11 |
| `round` | （新增）`float32.round(x: Float32) -> Float32` | Rust `round` / Swift `rounded()` | 新增 | R-sized-11 |
| `trunc` | （新增）`float32.trunc(x: Float32) -> Float32` | Rust `trunc` / Swift `rounded(.towardZero)` | 新增 | R-sized-11 |
| `signum` | （新增）`float32.signum(x: Float32) -> Float32` | Rust `signum` | 新增 | R-sized-11 |
| `from` | （新增）`float32.from[S: Numeric](x: S) -> Float32` | Rust `From`・`as` / Swift `init(_:)` / Python `int(x)` | 新增 | R-L0-40 |
| `from_checked` | （新增）`float32.from_checked[S: Numeric](x: S) -> Float32?` | Rust `TryFrom` / Swift `init(exactly:)` | 新增 | R-L0-40 |

<a id="l3-float64"></a>

### L3: `float64`

现有 10 个函数，另新增 21 个函数。

| 函数 | 签名（当前，显示 mut） | 其他语言主要拼写 | 判定 | 提案 |
|---|---|---|---|---|
| `to_int8` | `float64.to_int8(x: Float64) -> Int8` | Rust `as`（饱和） / Kotlin `toInt()`（饱和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 撤除（→ `int8.from`） | R-L0-40 |
| `to_int16` | `float64.to_int16(x: Float64) -> Int16` | Rust `as`（饱和） / Kotlin `toInt()`（饱和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 撤除（→ `int16.from`） | R-L0-40 |
| `to_int32` | `float64.to_int32(x: Float64) -> Int32` | Rust `as`（饱和） / Kotlin `toInt()`（饱和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 撤除（→ `int32.from`） | R-L0-40 |
| `to_int64` | `float64.to_int64(x: Float64) -> Int64` | Rust `as`（饱和） / Kotlin `toInt()`（饱和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 撤除（→ `int64.from`） | R-L0-40 |
| `to_uint8` | `float64.to_uint8(x: Float64) -> UInt8` | Rust `as`（饱和） / Kotlin `toInt()`（饱和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 撤除（→ `uint8.from`） | R-L0-40 |
| `to_uint16` | `float64.to_uint16(x: Float64) -> UInt16` | Rust `as`（饱和） / Kotlin `toInt()`（饱和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 撤除（→ `uint16.from`） | R-L0-40 |
| `to_uint32` | `float64.to_uint32(x: Float64) -> UInt32` | Rust `as`（饱和） / Kotlin `toInt()`（饱和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 撤除（→ `uint32.from`） | R-L0-40 |
| `to_uint64` | `float64.to_uint64(x: Float64) -> UInt64` | Rust `as`（饱和） / Kotlin `toInt()`（饱和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 撤除（→ `uint64.from`） | R-L0-40 |
| `to_float32` | `float64.to_float32(x: Float64) -> Float32` | Rust `as` / Kotlin `toFloat()` / Swift `init(_:)` | 撤除（→ `float32.from`） | R-L0-40 |
| `to_string` | `float64.to_string(x: Float64) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | 保留 | — |
| `parse` | （新增）`float64.parse(s: String) -> Result[Float64, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` 等 / Swift `T(s)` | 新增 | R-sized-02 |
| `compare` | （新增）`float64.compare(a: Float64, b: Float64) -> Int`（IEEE totalOrder） | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | 新增 | R-sized-02 |
| `to_bits` | （新增）`float64.to_bits(x: Float64) -> Int` | Rust `to_bits` (F) / Go `math.Float64bits` / Kotlin `toBits` | 新增 | R-sized-03 |
| `from_bits` | （新增）`float64.from_bits(bits: Int) -> Float64` | Rust `from_bits` (F) / Go `math.Float64frombits` / Kotlin `fromBits` | 新增 | R-sized-03 |
| `infinity` | （新增）`float64.infinity() -> Float64` | Rust `INFINITY` / Swift `.infinity` | 新增 | R-sized-03 |
| `nan` | （新增）`float64.nan() -> Float64` | Rust `NAN` / Swift `.nan` | 新增 | R-sized-03 |
| `epsilon` | （新增）`float64.epsilon() -> Float64` | Rust `EPSILON` / Swift `.ulpOfOne` | 新增 | R-sized-03 |
| `max_value` | （新增）`float64.max_value() -> Float64` | Rust `MAX` / Go `math.MaxFloat64` / Kotlin `MAX_VALUE` | 新增 | R-sized-03 |
| `min_value` | （新增）`float64.min_value() -> Float64` | Rust `MIN` / Swift `-.greatestFiniteMagnitude` | 新增 | R-sized-03 |
| `parse_radix` | `int8.parse_radix(s: String, radix: Int) -> Result[Int8, String]`（提案） | Rust `from_str_radix` / Go `ParseInt`・`ParseUint` | 新增 | R-sized-06 |
| `to_string_radix` | `int8.to_string_radix(x: Int8, radix: Int) -> String`（提案） | Rust `format!("{:x}")` / Go `FormatInt`・`FormatUint` | 新增 | R-sized-06 |
| `parse_radix` | `int16.parse_radix(s: String, radix: Int) -> Result[Int16, String]`（提案） | Rust `from_str_radix` / Go `ParseInt`・`ParseUint` | 新增 | R-sized-06 |
| `to_string_radix` | `int16.to_string_radix(x: Int16, radix: Int) -> String`（提案） | Rust `format!("{:x}")` / Go `FormatInt`・`FormatUint` | 新增 | R-sized-06 |
| `parse_radix` | `int32.parse_radix(s: String, radix: Int) -> Result[Int32, String]`（提案） | Rust `from_str_radix` / Go `ParseInt`・`ParseUint` | 新增 | R-sized-06 |
| `to_string_radix` | `int32.to_string_radix(x: Int32, radix: Int) -> String`（提案） | Rust `format!("{:x}")` / Go `FormatInt`・`FormatUint` | 新增 | R-sized-06 |
| `parse_radix` | `int64.parse_radix(s: String, radix: Int) -> Result[Int64, String]`（提案） | Rust `from_str_radix` / Go `ParseInt`・`ParseUint` | 新增 | R-sized-06 |
| `to_string_radix` | `int64.to_string_radix(x: Int64, radix: Int) -> String`（提案） | Rust `format!("{:x}")` / Go `FormatInt`・`FormatUint` | 新增 | R-sized-06 |
| `parse_radix` | `uint8.parse_radix(s: String, radix: Int) -> Result[UInt8, String]`（提案） | Rust `from_str_radix` / Go `ParseInt`・`ParseUint` | 新增 | R-sized-06 |
| `to_string_radix` | `uint8.to_string_radix(x: UInt8, radix: Int) -> String`（提案） | Rust `format!("{:x}")` / Go `FormatInt`・`FormatUint` | 新增 | R-sized-06 |
| `parse_radix` | `uint16.parse_radix(s: String, radix: Int) -> Result[UInt16, String]`（提案） | Rust `from_str_radix` / Go `ParseInt`・`ParseUint` | 新增 | R-sized-06 |
| `to_string_radix` | `uint16.to_string_radix(x: UInt16, radix: Int) -> String`（提案） | Rust `format!("{:x}")` / Go `FormatInt`・`FormatUint` | 新增 | R-sized-06 |
| `parse_radix` | `uint32.parse_radix(s: String, radix: Int) -> Result[UInt32, String]`（提案） | Rust `from_str_radix` / Go `ParseInt`・`ParseUint` | 新增 | R-sized-06 |
| `to_string_radix` | `uint32.to_string_radix(x: UInt32, radix: Int) -> String`（提案） | Rust `format!("{:x}")` / Go `FormatInt`・`FormatUint` | 新增 | R-sized-06 |
| `parse_radix` | `uint64.parse_radix(s: String, radix: Int) -> Result[UInt64, String]`（提案） | Rust `from_str_radix` / Go `ParseInt`・`ParseUint` | 新增 | R-sized-06 |
| `to_string_radix` | `uint64.to_string_radix(x: UInt64, radix: Int) -> String`（提案） | Rust `format!("{:x}")` / Go `FormatInt`・`FormatUint` | 新增 | R-sized-06 |
| `min` | （新增）`int8.min(a: Int8, b: Int8) -> Int8` | Rust `min` / Swift `min` | 新增 | R-sized-08 |
| `max` | （新增）`int8.max(a: Int8, b: Int8) -> Int8` | Rust `max` / Swift `max` | 新增 | R-sized-08 |
| `clamp` | （新增）`int8.clamp(x: Int8, lo: Int8, hi: Int8) -> Int8` | Rust `clamp` | 新增 | R-sized-08 |
| `min` | （新增）`int16.min(a: Int16, b: Int16) -> Int16` | Rust `min` / Swift `min` | 新增 | R-sized-08 |
| `max` | （新增）`int16.max(a: Int16, b: Int16) -> Int16` | Rust `max` / Swift `max` | 新增 | R-sized-08 |
| `clamp` | （新增）`int16.clamp(x: Int16, lo: Int16, hi: Int16) -> Int16` | Rust `clamp` | 新增 | R-sized-08 |
| `min` | （新增）`int32.min(a: Int32, b: Int32) -> Int32` | Rust `min` / Swift `min` | 新增 | R-sized-08 |
| `max` | （新增）`int32.max(a: Int32, b: Int32) -> Int32` | Rust `max` / Swift `max` | 新增 | R-sized-08 |
| `clamp` | （新增）`int32.clamp(x: Int32, lo: Int32, hi: Int32) -> Int32` | Rust `clamp` | 新增 | R-sized-08 |
| `min` | （新增）`int64.min(a: Int64, b: Int64) -> Int64` | Rust `min` / Swift `min` | 新增 | R-sized-08 |
| `max` | （新增）`int64.max(a: Int64, b: Int64) -> Int64` | Rust `max` / Swift `max` | 新增 | R-sized-08 |
| `clamp` | （新增）`int64.clamp(x: Int64, lo: Int64, hi: Int64) -> Int64` | Rust `clamp` | 新增 | R-sized-08 |
| `min` | （新增）`uint8.min(a: UInt8, b: UInt8) -> UInt8` | Rust `min` / Swift `min` | 新增 | R-sized-08 |
| `max` | （新增）`uint8.max(a: UInt8, b: UInt8) -> UInt8` | Rust `max` / Swift `max` | 新增 | R-sized-08 |
| `clamp` | （新增）`uint8.clamp(x: UInt8, lo: UInt8, hi: UInt8) -> UInt8` | Rust `clamp` | 新增 | R-sized-08 |
| `min` | （新增）`uint16.min(a: UInt16, b: UInt16) -> UInt16` | Rust `min` / Swift `min` | 新增 | R-sized-08 |
| `max` | （新增）`uint16.max(a: UInt16, b: UInt16) -> UInt16` | Rust `max` / Swift `max` | 新增 | R-sized-08 |
| `clamp` | （新增）`uint16.clamp(x: UInt16, lo: UInt16, hi: UInt16) -> UInt16` | Rust `clamp` | 新增 | R-sized-08 |
| `min` | （新增）`uint32.min(a: UInt32, b: UInt32) -> UInt32` | Rust `min` / Swift `min` | 新增 | R-sized-08 |
| `max` | （新增）`uint32.max(a: UInt32, b: UInt32) -> UInt32` | Rust `max` / Swift `max` | 新增 | R-sized-08 |
| `clamp` | （新增）`uint32.clamp(x: UInt32, lo: UInt32, hi: UInt32) -> UInt32` | Rust `clamp` | 新增 | R-sized-08 |
| `min` | （新增）`uint64.min(a: UInt64, b: UInt64) -> UInt64` | Rust `min` / Swift `min` | 新增 | R-sized-08 |
| `max` | （新增）`uint64.max(a: UInt64, b: UInt64) -> UInt64` | Rust `max` / Swift `max` | 新增 | R-sized-08 |
| `clamp` | （新增）`uint64.clamp(x: UInt64, lo: UInt64, hi: UInt64) -> UInt64` | Rust `clamp` | 新增 | R-sized-08 |
| `min` | （新增）`float32.min(a: Float32, b: Float32) -> Float32` | Rust `min` / Swift `min` | 新增 | R-sized-08 |
| `max` | （新增）`float32.max(a: Float32, b: Float32) -> Float32` | Rust `max` / Swift `max` | 新增 | R-sized-08 |
| `clamp` | （新增）`float32.clamp(x: Float32, lo: Float32, hi: Float32) -> Float32` | Rust `clamp` | 新增 | R-sized-08 |
| `is_nan` | （新增）`float32.is_nan(x: Float32) -> Bool` | Rust `is_nan` / Swift `.isNaN` | 新增 | R-sized-08 |
| `is_infinite` | （新增）`float32.is_infinite(x: Float32) -> Bool` | Rust `is_infinite` / Swift `.isInFinite` | 新增 | R-sized-08 |
| `is_finite` | （新增）`float32.is_finite(x: Float32) -> Bool` | Rust `is_finite` / Swift `.isFinite` | 新增 | R-sized-08 |
| `min` | （新增）`float64.min(a: Float64, b: Float64) -> Float64` | Rust `min` / Swift `min` | 新增 | R-sized-08 |
| `max` | （新增）`float64.max(a: Float64, b: Float64) -> Float64` | Rust `max` / Swift `max` | 新增 | R-sized-08 |
| `clamp` | （新增）`float64.clamp(x: Float64, lo: Float64, hi: Float64) -> Float64` | Rust `clamp` | 新增 | R-sized-08 |
| `is_nan` | （新增）`float64.is_nan(x: Float64) -> Bool` | Rust `is_nan` / Swift `.isNaN` | 新增 | R-sized-08 |
| `is_infinite` | （新增）`float64.is_infinite(x: Float64) -> Bool` | Rust `is_infinite` / Swift `.isInFinite` | 新增 | R-sized-08 |
| `is_finite` | （新增）`float64.is_finite(x: Float64) -> Bool` | Rust `is_finite` / Swift `.isFinite` | 新增 | R-sized-08 |
| `abs` | （新增）`float64.abs(x: Float64) -> Float64` | Rust `abs` / Swift `magnitude` | 新增 | R-sized-11 |
| `floor` | （新增）`float64.floor(x: Float64) -> Float64` | Rust `floor` / Swift `rounded(.down)` | 新增 | R-sized-11 |
| `ceil` | （新增）`float64.ceil(x: Float64) -> Float64` | Rust `ceil` / Swift `rounded(.up)` | 新增 | R-sized-11 |
| `round` | （新增）`float64.round(x: Float64) -> Float64` | Rust `round` / Swift `rounded()` | 新增 | R-sized-11 |
| `trunc` | （新增）`float64.trunc(x: Float64) -> Float64` | Rust `trunc` / Swift `rounded(.towardZero)` | 新增 | R-sized-11 |
| `signum` | （新增）`float64.signum(x: Float64) -> Float64` | Rust `signum` | 新增 | R-sized-11 |
| `from` | （新增）`float64.from[S: Numeric](x: S) -> Float64` | Rust `From`・`as` / Swift `init(_:)` / Python `int(x)` | 新增 | R-L0-40 |
| `from_checked` | （新增）`float64.from_checked[S: Numeric](x: S) -> Float64?` | Rust `TryFrom` / Swift `init(exactly:)` | 新增 | R-L0-40 |
