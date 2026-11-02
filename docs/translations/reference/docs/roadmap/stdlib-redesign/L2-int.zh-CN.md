<a id="l2l3-int"></a>

# L2/L3: int

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-int.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-int.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-int.zh-CN.md)

> 固定原文的非规范读者快照；原文仍为权威，不自动同步。未独立验证实现或执行引用指令。历史状态、计划及比较主张保留原范围。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-int.md)

> 原文历史注记：带日期的重设计记录，不是现行整数 API 或新测量。开头标2026-10-21，R-int-13另增2026-10-23观察。早期转换源侧放置与后续 R-L0-40 泛型目标侧转换、N 位参数与 R-int-12 类型宽度修订均保留。保留72+13计数和 to_bigint 提及，不虚构缺失表行，也不猜测协调后续新增。开头加法回绕观察与后文宽泛溢出安全措辞范围不同，不推出普遍溢出保证。表将 Rust Euclid 运算列入向下取整一栏，但未证明所有除数符号下等价。F 标记及语言比较仅为原文历史核查，未独立更新。采纳提案、弃用 epoch、fix 和门禁均非独立验证的实现或执行。


> 原文描述：标准库重设计 L2/L3 — int：移除 from_&lt;sized&gt; 列和独立的 u32/u8，补齐 wrap 族，增加向下取整除法和进制转换。
> 最后更新：2026-10-21
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)・[L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)
> 相关：[L2-sized-numbers](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md)（转换矩阵）・[L2-math](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-math.md)（类型的重复函数）・[L2-float](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-float.md)
> 来源： Rust https://doc.rust-lang.org/std/primitive.i64.html ・ Go https://pkg.go.dev/math/bits 与 https://pkg.go.dev/strconv ・ Python https://docs.python.org/3/library/stdtypes.html#numeric-types-int-float-complex ・ TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Number ・ Gleam https://gleam-stdlib.hexdocs.pm/gleam/int.html (F) ・ Elixir https://elixir.hexdocs.pm/Integer.html (F) 与 https://hexdocs.pm/elixir/Bitwise.html ・ OCaml https://ocaml.org/manual/latest/api/Int.html (F) ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin/-long/ (F) ・ Swift https://developer.apple.com/documentation/swift/int

(F) = 本次获取该官方页面并核实拼写的单元格。Gleam、Elixir、OCaml、Kotlin 的函数名列表获取于 2026-10-21。

实测（`almide 0.68.0 dev 437b8f127`，native 与 `--target wasm` 结果相同）：

| 表达式 | 结果 | 可得结论 |
|---|---|---|
| `int.wrap_add(1, 2, 0)` | `0` | `bits = 0` 静默返回 0 |
| `int.wrap_add(1, 2, -1)` | `3` | 负 `bits` 被视为“全部位”。`rotate_left` 在同样输入下 abort（C-047） |
| `int.to_hex(-1)` → `int.from_hex` | `err: number too large…` | `to_hex` 表示 64 位模式，`from_hex` 解析有符号数，无法往返 |
| `int.from_hex("-ff")` | `-255` | `from_hex` 接受符号 |
| `-7 / 2`、`-7 % 3` | `-3`、`-1` | 除法向 0 截断。没有向下取整余数 |
| `5 & 3` | 词法错误 | 没有位运算符，`int.band` 等是唯一写法 |
| `int.max_value() + 1` | `-9223372036854775808` | `Int` 运算回绕 |

<a id="l2-役割の比較"></a>

## L2：角色比较

| 角色 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 转为字符串 | `to_string` | `to_string` | `strconv.Itoa` | `str` | `toString` | `to_string` (F) | `to_string` (F) | `to_string` (F) | `toString` | `String(n)` |
| 从字符串解析 | `parse` | `parse` | `strconv.Atoi` | `int(s)` | `parseInt` | `parse` (F) | `parse` (F) | `of_string_opt` | `toIntOrNull` | `Int(s)` |
| 指定进制转为字符串 | `to_hex` 仅此 | `format!("{:b}")` | `FormatInt(n, b)` | `bin`/`oct`/`hex` | `toString(r)` | `to_base_string` (F) | `to_string(n, b)` (F) | — | `toString(r)` | `String(n, radix:)` |
| 按指定进制解析 | `from_hex` 仅此 | `from_str_radix` | `ParseInt(s, b, 64)` | `int(s, b)` | `parseInt(s, r)` | `base_parse` (F) | `parse(s, b)` (F) | — | `toIntOrNull(r)` | `Int(s, radix:)` |
| 转为 Float | `to_float` | `as f64` | `float64(n)` | `float(n)` | （同一类型） | `to_float` (F) | — | `to_float` (F) | `toDouble` | `Double(n)` |
| 绝对值／最小／最大／限制范围 | `abs` `min` `max` `clamp`（`math` 也有重复函数） | `abs` `min` `max` `clamp` | `min` `max` | `abs` `min` `max` | `Math.*` | `absolute_value` `min` `max` `clamp` (F) | `abs` | `abs` `min` `max` (F) | `absoluteValue` `coerceIn` (F) | `abs` `min` `max` |
| 符号 | `math.sign` | `signum` | — | — | `Math.sign` | — | — | — | `sign` (F) | `signum()` |
| 乘方 | `math.pow` | `pow` | — | `**` | `**` | `power`（Float）(F) | `pow` (F) | — | — | — |
| 奇偶 | `is_even` `is_odd` | — | — | — | — | `is_even` `is_odd` (F) | `is_even` `is_odd` (F) | — | — | `isMultiple(of:)` |
| 向下取整除法／余数 | — | `div_euclid` `rem_euclid` | — | `//` `%` | — | `floor_divide` `modulo` (F) | `floor_div` `mod` (F) | `fdiv` `erem` (F) | `floorDiv` `mod` (F) | — |
| 位 AND/OR/XOR/NOT | `band` `bor` `bxor` `bnot` | 运算符 | 运算符 | 运算符 | 运算符 | `bitwise_and` 等 (F) | `band` `bor` `bxor` `bnot` | `logand` 等 (F) | `and` `or` `xor` `inv` (F) | 运算符 |
| 算术移位 | `bshl` `bshr` | `<<` `>>` | `<<` `>>` | `<<` `>>` | `<<` `>>` | `bitwise_shift_*` (F) | `bsl` `bsr` | `shift_left` `shift_right` (F) | `shl` `shr` (F) | `<<` `>>` |
| 逻辑右移（补 0） | — | 无符号类型的 `>>` | 无符号类型的 `>>` | — | `>>>` | — | — | `shift_right_logical` (F) | `ushr` (F) | 无符号类型的 `>>` |
| 位计数／旋转／反转 | `pop_count` `count_*_zeros` `rotate_*` `bit_reverse` `byte_swap` `bit_width` | 提供 | `math/bits` | `bit_count` `bit_length` | `Math.clz32` | — | `popcount` (F) | `popcount` `leading_zeros` (F) | `countOneBits` `rotateLeft` (F) | 提供 |
| N 位回绕运算 | `wrap_add` `wrap_mul`（`wrap_sub` 缺失） | `wrapping_*` | 类型转换本身 | — | `BigInt.asUintN` | — | — | — | — | `&+` `&-` `&*` |
| 下限／上限 | `min_value` `max_value` | `MIN` `MAX` | `math.MinInt64` | — | `MIN_SAFE_INTEGER` | — | — | `min_int` `max_int` (F) | `MIN_VALUE` (F) | `.min` `.max` |
| 与定宽类型转换 | `to_<型>` 与 `from_<型>` 两者 | `as`・`from`・`try_from` | `T(x)` | — | — | — | — | `Int32.of_int` | `toByte()` 等 (F) | `init(truncatingIfNeeded:)` 等 |


<a id="完全性の規則"></a>

### 完整性规则

1. **转换矩阵**遵守 [L2-sized-numbers](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md) 的规则。`int` 是 12 个载体（carrier）之一；`int` 模块只放“从 `Int` 转出”的转换（R-L0-07）。转入 `Int` 的转换是源类型模块的 `to_int`。
2. **回绕运算族**（R-L0-08、#3854）：`wrap(n, bits)`、`wrap_add`、`wrap_sub`、`wrap_mul` 四格必须齐全。签名为 `(…, bits: Int) -> Int`，`bits` 规则与旋转（`rotate_*`）相同（`bits <= 0` 时 abort，`bits >= 64` 保持 64 位）。有意不设 `wrap_neg`（等价于 `int.wrap(0 - a, bits)`，`0 - min_value` 也通过回绕得到正确值）。
3. **不放类型的重复函数**（R-L0-01、R-L0-02）：以 `Int` 为主体的算术（`abs`、`min`、`max`、`sign`、`pow`）只放在 `int`，不留在 `math`（[L2-math](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-math.md) R-math-01）。
4. **进制转换成对提供**：转为字符串（`to_string_radix`）与从字符串解析（`parse_radix`）以相同进制和符号规则往返。
5. **有序类型具有 `compare`**（R-L0-16）：`int.compare(a, b) -> Int`（负、0、正）。作为 [R-sized-02](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md) 的一个单元格，同时加入所有数值载体。
6. 位运算保留 `b` 前缀族（`band` `bor` `bxor` `bnot` `bshl` `bshr`），新增单元格也采用该族形式。

<a id="l2-の判定"></a>

### L2 判定

- 缺失角色：`compare`（R-L0-16 必需）→ [R-sized-02](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md)。向下取整除法和余数（6/9）→ R-int-03。进制转换（8/9）→ R-int-04。逻辑右移（6/9）→ R-int-06。`sign` 和 `pow` 从 `math` 移入 → R-math-01。
- 重复：`int.to_u32` / `int.to_u8` 与 `int.to_uint32` / `int.to_uint8`（#3851 E）→ R-int-02。`int.from_hex` 与新 `int.parse_radix(s, 16)` → R-int-04。`int.from_<型>` 与 `<型>.to_int`（R-L0-07）→ R-int-01。代码中的“型”指类型。
- 族的缺口：缺少 `wrap_sub` 和 `wrap`（#3854）→ R-int-02。缺少 `int.to_float_checked` 和 `int.to_float64_checked`（`Int` 转 `Float` 超过 2^53 会舍入，属于有损转换）→ [R-sized-01](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md)。
- 拼写：`band`/`bor`/`bxor` 没有多数派拼写，保持不变 → R-int-05（×）。

<a id="案"></a>

## 提案

<a id="r-int-01-intfrom_型-の列を撤去し元の型の-to_int-に移す---採用"></a>

### R-int-01 移除 `int.from_<型>` 列，移至源类型的 `to_int` — ○ 采纳

- 内容：移除 `int.from_int8` `from_int16` `from_int32` `from_int64` `from_uint8` `from_uint16` `from_uint32` `from_uint64` 及 `int.from_uint64_checked` `int.from_uint64_saturating` 共 10 个函数。改为添加 `int8.to_int` … `uint64.to_int`、`uint64.to_int_checked`、`uint64.to_int_saturating`（新增主体见 [R-sized-01](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md)）。
- L0 依据：R-L0-07（值转换放在源类型的 `to_<型>`；两侧都有时仅保留 1 的一侧）、R-L0-01。代码中的“型”指类型。
- (a) 5/9 语言在源类型侧放 `to_X`（Rust、Gleam、Elixir、OCaml、Kotlin `toLong()`）。9 种语言均没有 `Int.from_int8` 形式。
- (b) 纯计算。当前 `int.from_<型>` 是 intrinsic，新 `to_int` 作为调用同一 intrinsic 的别名添加，并迁移 `self_host_registry.rs` 的行。两种 target 的值不变。
- (c) `x |> int8.to_int` 由管道头部类型决定模块。当前可写 `int8.to_int16` 却不能写 `int8.to_int`，LLM 根据规则推断必然出错。
- (d) 移除与新增配对。`@deprecated(since=15, use="int8.to_int")` 等，removal epoch 16。`almide fix` 改写：`int.from_<T>(x)` → `<T>.to_int(x)`（1 对 1）。使用数（spec/tests/docs/stdlib）：8 个函数合计 70/38/47/362（stdlib 的 362 是 sized 模块主体自行调用，迁移后变为 0），`from_uint64_checked` 9/2/7/1，`from_uint64_saturating` 4/2/4/0。同一变更改写 CHEATSHEET:1068 和 llms.txt:412。
- 门禁提案：将 `src/cli/docs_gen.rs` 的 `check_numeric_conversion_matrix` 中“Int 列为 `int.from_<s>`”改成“`<s>.to_int`”，并检查 `int.from_<s>` 不存在（与 R-sized-01 门禁同一变更）。
- issue：#3935

<a id="r-int-02-折り返しの族を-4-セルにそろえto_u32to_u8-を撤去する---採用"></a>

### R-int-02 补齐回绕族四格，移除 `to_u32`/`to_u8` — ○ 采纳

- 内容：
  - 新增：`int.wrap(n: Int, bits: Int) -> Int`（`n mod 2^bits`，`bits < 64` 时非负）、`int.wrap_sub(a: Int, b: Int, bits: Int) -> Int`。
  - 统一边界：`wrap`、`wrap_add`、`wrap_sub`、`wrap_mul` 的 `bits <= 0` 均 abort（措辞类型同 `rotate_*` 的 C-047）；`bits >= 64` 保持 64 位（同 C-048）。
  - 移除：`int.to_u32`（= `int.wrap(n, 32)`）、`int.to_u8`（= `int.wrap(n, 8)`）。
  - 不添加 `wrap_neg`（完整性规则 2）。
- L0 依据：R-L0-08（填补矩阵缺口）、R-L0-01（`to_u32` 与 `to_uint32` 值相同、类型不同，容易混淆）。
- (a) 所有提供回绕运算的语言都有回绕减法（Rust `wrapping_sub`、Swift `&-`）。没有仅有加法和乘法却无减法的语言。
- (b) 纯计算。可共用 `stdlib/int_wrap.almd` 的 `__wrap_mask`。当前 native 和 wasm 对 `bits <= 0` 都返回“0 或全部位”（实测见开头表），两者一同改为 abort。
- (c) LLM 看到 `wrap_add` 和 `wrap_mul` 会写 `int.wrap_sub(a, b, 32)`（#3854）。`to_u32` 与 `to_uint32` 只差一个字母，返回类型却分别是 `Int` 与 `UInt32`。
- (d) 新增 2、移除 2、一组边界语义变更。边界变更是将“曾返回无意义值的输入改为 abort”的定义域收窄，仍返回值的输入结果均不变（不属于 R-L0-14 第 4 项“同名异义”）。因为收窄定义域，与 R-L0-12 的位置写入一样不列入 epoch 的 `breaks`。移除采用 `@deprecated(since=15, use="int.wrap")`、removal epoch 16。`almide fix`：`int.to_u32(x)` → `int.wrap(x, 32)`、`int.to_u8(x)` → `int.wrap(x, 8)`。使用数：`to_u32` 6/2/10/0、`to_u8` 6/2/9/0、`wrap_add` 8/2/8/0、`wrap_mul` 7/2/6/0。
- 门禁提案：按 #3854，在 `stdlib_synonym_gate_test.rs` 旁增加测试，检查“wrap 族四格齐全、签名相同、在 `self_host_registry.rs` 有对应行”。用 `spec/wasm_cross` fixture 检查 `bits = 0`、`-1`、`64`、`65` 边界在两个 target 一致（先在 als 放新契约 C 编号）。
- issue：#3854（第 1 节）、#3851 E

<a id="r-int-03-床方向の割り算と剰余-intfloor_div--intmodulo-を足す---採用"></a>

### R-int-03 增加向下取整除法和余数 `int.floor_div` / `int.modulo` — ○ 采纳

- 内容：`int.floor_div(a: Int, b: Int) -> Int`（向 −∞ 舍入的商）、`int.modulo(a: Int, b: Int) -> Int`（结果符号与 `b` 相同；`a == floor_div(a, b) * b + modulo(a, b)`）。`b == 0` 与 `/` 一样 abort（违反契约，R-L0-06 第 3 项）。`floor_div(min_value, -1)` 也与 `/` 一样 abort（"integer overflow"）。
- L0 依据：R-L0-02（主体为 `Int`，故放在 `int`）、L1 新增标准（6/9 提供）。拼写取 R-L0-00 范围：余数用 `modulo`（Gleam；Elixir、Kotlin 的 `mod` 与 Almide 可见性关键字 `mod` 同名，不能作为函数名）；商用 `floor_div`（Elixir、OCaml `fdiv` 的含义、Kotlin `floorDiv`）。
- (a) 6/9 提供（Python、Gleam、Elixir、OCaml、Kotlin、Rust 的 euclid 版），仅 Go、JS、Swift 不提供。
- (b) 纯计算，可由 `/` 和 `%` 用 Almide 编写。
- (c) 当前 `%` 是向 0 截断的余数，用 `(i - 1) % n` 循环数组会得到负下标。按 Python 的向下取整 `%` 推断的 LLM 会静默出错。有明确命名的函数，就能在 CHEATSHEET 用一行写出“向下取整用 `int.modulo`”。
- (d) 新增，使用数 0。
- 门禁提案：`spec/stdlib/int_floor_div_test.almd` 固定四种符号组合与 `b = 0` 的 abort，再放一个 wasm_cross fixture。
- issue：#3918

<a id="r-int-04-基数の変換を-to_string_radix--parse_radix-の対にしfrom_hex-を撤去する---採用"></a>

### R-int-04 进制转换成对使用 `to_string_radix` / `parse_radix`，移除 `from_hex` — ○ 采纳

- 内容：
  - 新增：`int.to_string_radix(n: Int, radix: Int) -> String`（有符号，`-255` 在 radix 16 为 `"-ff"`，小写）、`int.parse_radix(s: String, radix: Int) -> Result[Int, String]`（去除首尾空白，接受 `+`/`-`，不区分大小写；仅 radix 16、8、2 分别接受可省略的 `0x`/`0o`/`0b` 前缀）。`radix` 超出 2–36 时 abort（违反契约）。
  - 移除：`int.from_hex`（= `int.parse_radix(s, 16)`；定义 `parse_radix` 使“0x 可省略、可带符号、超过 2^63−1 则 err”完全保持相同）。
  - 保留：`int.to_hex`，它负责“将 64 位模式写为十六进制”（`-1` → 16 个 `f`）。在 CHEATSHEET 加一行：有符号十六进制用 `to_string_radix(n, 16)`。
- L0 依据：R-L0-01（`from_hex` 与 `parse_radix(s, 16)` 同角色）、L1 新增标准（8/9 可指定进制）。拼写取 R-L0-00 范围（无多数派；Gleam `to_base_string`/`base_parse`，Kotlin `toString(radix)`/`toInt(radix)`，Swift `radix:`）。Almide 没有可选参数，因此将 `radix` 放入名字，以区别现有 `to_string`/`parse`。
- (a) 8/9 提供（仅 OCaml Stdlib 没有）。
- (b) 纯计算。
- (c) 二进制表示（`to_string_radix(n, 2)`）常用于算法题，当前只能手写。另设可往返的一对函数，可避开 `to_hex` 与 `from_hex` 无法往返的陷阱（实测见开头表）。
- (d) 新增 2、移除 1。`@deprecated(since=15, use="int.parse_radix")`，removal epoch 16。`almide fix`：`int.from_hex(s)` → `int.parse_radix(s, 16)`。使用数：`from_hex` 52/2/22/1、`to_hex` 23/2/10/2。
- 门禁提案：在 radix 2–36 及 `min_value`、`max_value`、`0`、`-1` 上检验 `to_string_radix` → `parse_radix` 往返的 spec；移除前窗口内，将现有 `from_hex` spec 通过 `parse_radix(s, 16)` 运行的差分测试。
- issue：#3920

<a id="r-int-05-bandborbxor-などを多数派の綴りに改名する---不採用改名しないr-int-10-で確かめた"></a>

### R-int-05 将 `band`/`bor`/`bxor` 等改为多数派拼写 — × 不采纳（不改名；R-int-10 已确认）

- 内容：不改名。
- L0 依据：R-L0-00。用函数名表示位运算的四种语言各不相同（Gleam `bitwise_and`、Elixir `band`、OCaml `logand`、Kotlin `and`，均为 (F)）；其他五种仅用运算符。没有多数派，保留当前拼写（与 Elixir 相同的 `b` 前缀）。
- (c) 写运算符 `&` 会产生词法错误，虽有提示却未点名 `int.band`。应通过诊断帮助而不是改名：另建小 issue，让 E0xx（unknown-char）提示“位运算用 `int.band` / `int.bor` / `int.bxor` / `int.bshl` / `int.bshr`”。
- issue：#3905（仅诊断提示）

<a id="r-int-06-論理右シフト-intbshr_logical-を足す---採用"></a>

### R-int-06 增加逻辑右移 `int.bshr_logical` — ○ 采纳

- 内容：`int.bshr_logical(a: Int, n: Int) -> Int`。高位补 0 的右移。`n` 与 `bshr` 一样取除以 64 的余数。
- L0 依据：L1 新增标准（6/9）、R-L0-00（将 OCaml `shift_right_logical` 的词形改为 Almide 的 `b` 族形式；Kotlin `ushr` 和 JS `>>>` 不符合族形式）。
- (a) 6/9（Rust、Go、Swift 无符号类型的 `>>`，JS `>>>`，OCaml，Kotlin）。
- (b) 纯计算。runtime 已有 `prim.bshr_u`，只需公开名称。
- (c) 编写哈希函数或随机数（R-random-01 的 SplitMix64 也使用它）时，算术移位会混入符号，静默产生不同值。当前只能手写 `int.band(int.bshr(x, k), mask)`。
- (d) 新增，使用数 0。
- 门禁提案：在 `spec/stdlib/int_bits_test.almd` 增加负数逻辑移位，并用 wasm_cross fixture 保证两个 target 一致。
- issue：#3919

<a id="r-int-07-pop_count-を-count_ones-に改名する---採用"></a>

### R-int-07 将 `pop_count` 改名为 `count_ones` — ○ 采纳

- 当前：位计数族为 `count_leading_zeros`、`count_trailing_zeros`，只有统计 1 位的函数叫 `pop_count`。
- 提案：改为 `int.count_ones`，统一 `count_` 前缀。
- L0 依据：R-L0-00（族内拼写不一致）。
- (a) Rust `count_ones`、Kotlin `countOneBits`、Python `bit_count`、Go `OnesCount64`。`pop_count` 仅接近 OCaml、Elixir 的 `popcount`。
- (d) 改名。`@deprecated(since=15, use="int.count_ones")`、`removal_epoch = 16`，由 `almide fix` 改写。
- issue：#3985

<a id="r-int-08-intto_hex-を-to_string_radixx-16-にまとめる---不採用"></a>

### R-int-08 将 `int.to_hex` 合并为 `to_string_radix(x, 16)` — × 不采纳

- 原因：负数结果不同，不是同义词。`int.to_hex(-1)` 是 64 位模式的十六进制（`"ffffffffffffffff"`），`int.to_string_radix(-1, 16)` 则带符号（`"-1"`）。显示哈希值或位模式，与用其他进制表示数值是不同用途。
- issue：无

<a id="r-int-09-intsign-と-floatsignum-の綴りを-1-つにする---不採用"></a>

### R-int-09 统一 `int.sign` 与 `float.signum` 的拼写 — × 不采纳

- 原因：对 0 的处理不同，同名会重现 R-float-05 避免的错误。`int.sign(0)`、`bigint.sign(0)` 返回 0，`float.signum(0.0)` 返回 1.0（同 Rust `signum`）。`sign` 只用于“0 返回 0”的函数（R-float-05 完整性规则）。
- issue：无

<a id="r-int-10-ビット演算の綴りを計測で決める---採用関数のまま十二巡目に演算子の案を取り消し"></a>

### R-int-10 用测量决定位运算拼写 — ○ 采纳（仍用函数；第十二轮撤回运算符提案）

- 当前：`band`、`bor`、`bxor`、`bnot`、`bshl`、`bshr`、`bshr_logical`（与 Elixir 相同的 `b` 前缀）。R-sized-10、R-bigint-07 将其设为 `Integer` 要求，扩展到 10 种整数类型的 61 个函数。
- R-int-05（×）的理由仍成立：四种用函数名的语言拼写分裂，没有多数派。也不只根据类型错误决定改名（R-L0-36）。
- 测量候选：
  1. 当前 `int.band(a, b)`（Elixir）。
  2. 单词形式 `int.bit_and(a, b)`、`bit_or`、`bit_xor`、`bit_not`、`shift_left`、`shift_right`（接近 Gleam `bitwise_and`、Roc `bitwiseAnd`）。
  3. 运算符 `a & b`、`a | b`、`a ^ b`、`~a`（R-L0-39 腾出 `^` 后；与 Rust、Go、Python、JS、Swift 五种语言相同）。由于 `>>` 表示函数组合，移位仍用函数（`shift_left`、`shift_right`）。
- 决定（第十二轮）：保留方案 1 的当前函数，并作为 `Integer` 要求。第九轮曾选运算符（3），但 `^` 曾表示乘方、`>>` 表示函数组合，改作其他含义会违反 R-L0-43，因此撤回。若仅把 `&`、`|` 设为运算符，XOR 和移位仍为函数，形式不统一。四种函数拼写没有多数派（R-int-05），保留当前拼写。
- issue：almide-dojo#65

<a id="r-int-11-bit_width-を-bit_length-に改名する---採用"></a>

### R-int-11 将 `bit_width` 改名为 `bit_length` — ○ 采纳

- 当前：`int.bit_width(n)` 返回表示值所需位数（Go `bits.Len64`、Python `bit_length`）。Swift `bitWidth` 为类型宽度（`Int` 始终为 64），同名不同值。按 Swift 推断，将 `int.bit_width(n)` 当作类型宽度会静默得到错误值。
- 提案：改名为 `bit_length` 并作为 `Integer` 要求（也加入八种定宽类型及 `bigint`；`bigint` 对应 Python `int.bit_length`、Go `big.Int.BitLen`）。
- (d) 改名与新增。epoch 15 标记 `@deprecated`，epoch 16 移除。
- issue：#4004

<a id="r-int-12-n-bit-の演算を型の幅の演算に置き換える---採用"></a>

### R-int-12 将 N 位运算替换为类型宽度运算 — ○ 采纳

- 当前：`int.wrap_add(a, b, bits)`、`wrap_sub`、`wrap_mul`、`rotate_left(a, n, bits)`、`rotate_right` 将 `Int` 当作 N 位数。定宽类型由类型决定宽度，将其作为 `FixedWidthInteger` 要求时形式发生冲突。
- 提案：五个函数都作为 `FixedWidthInteger` 要求，去掉 `bits`，按类型宽度运算（`int` 为 64 位，`int32.wrap_add(a, b)` 为 32 位；同 Swift `&+`、`&-`、`&*` 及 Rust `wrapping_add`、`rotate_left`）。要截断为 N 位时，与 `int.wrap(n, bits)` 组合（`int.wrap(int.wrap_add(a, b), 12)`）。`int.wrap` 仍为 `int` 独有函数。
- (d) 语义变更。`almide fix` 将 `int.wrap_add(a, b, bits)` 改成 `int.wrap(int.wrap_add(a, b), bits)`。宽度不是 8、16、32、64 的 `rotate_*` 无法改写，因此给出提示。
- issue：#4004

<a id="r-int-13-床方向の剰余の名前は-modulofloor_divmin_value--1-は-abort---採用r-int-03-を直す"></a>

### R-int-13 向下取整余数命名为 `modulo`，`floor_div(min_value, -1)` abort — ○ 采纳（修正 R-int-03）

- 发现（wave 1B、实现 #4045–#4048 时）：
  - `mod` 是可见性关键字（`mod fn f() = …`、`TokenType::Mod`）。`fn mod(` 无法解析，`int.mod` 与 `int8.mod` 都不能写。
  - R-int-03 曾写“`floor_div(min_value, -1)` 与 `Int` 运算一样回绕”，但 `Int` 的 `/` 在 native 和 wasm 都会在这里以 "integer overflow" abort（2026-10-23 实测）。
- 提案：
  - 向下取整余数使用 `modulo`（`int`、八种整数类型、`bigint`）。不采纳将 `mod` 移出关键字供函数名使用的提案，因为这需按上下文区分声明开头 `mod fn` 与表达式 `x.mod(…)`，属于语法变更。
  - `floor_div(min_value, -1)` 与 `/` 一样 abort（全部九种整数类型）。整数溢出不静默回绕是[安全表](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/security-by-construction.md)“整数溢出”一行的前提。需要回绕时用 `wrap_*`。
- L0 依据：R-L0-00（拼写来自比较语言范围）、R-L0-14（同名不异义：统一 `/` 与 `floor_div` 的边界）。
- (a) 余数函数名：Gleam `int.modulo`、Elixir `Integer.mod`、Kotlin `mod`、Rust `rem_euclid`。`modulo` 与 Gleam 同名。`MIN / -1`：Rust `div_euclid` 在 debug 下 panic，Python 多精度不溢出，Kotlin `floorDiv` 回绕。
- (c) LLM 常写 `int.mod`。调用 `int.mod` 时，给出“请用 `int.modulo`”的诊断（E002 hint）。
- (d) 名称变更（函数尚未发布，不影响用户代码），并重新决定一项边界语义。
- issue：#4045

<a id="l3-関数表"></a>

## L3：函数表

`int` 当前 72 个函数和新增 13 个函数（包括 `compare` 及 [L2-bigint](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-bigint.md) 的 `to_bigint`）。
| 函数 | 签名（当前，标明 mut） | 其他语言主要拼写 | 判定 | 提案 |
|---|---|---|---|---|
| `to_string` | `int.to_string(n: Int) -> String` | Rust `to_string` / Go `strconv.FormatInt` / Python `str` / JS `toString` / Gleam `to_string` (F) / Elixir `to_string` (F) / OCaml `to_string` (F) / Kotlin `toString` / Swift `String(x)` | 保留 | — |
| `to_hex` | `int.to_hex(n: Int) -> String` | Rust `format!("{:x}")` / Go `strconv.FormatInt(n,16)` / Python `hex` / JS `toString(16)` / Kotlin `toHexString` (F) / Gleam `to_base16` (F) | 保留 | R-int-04（64 位模式的十六进制；有符号进制转换用 `to_string_radix`） |
| `parse` | `int.parse(s: String) -> Result[Int, String]` | Rust `parse` / Go `strconv.Atoi` / Gleam `parse` (F) / Elixir `parse` (F) / OCaml `of_string_opt` / Kotlin `toIntOrNull` / Swift `Int(s)` | 保留 | — |
| `from_hex` | `int.from_hex(s: String) -> Result[Int, String]` | Rust `from_str_radix(s,16)` / Go `ParseInt(s,16,64)` / Python `int(s,16)` / Gleam `base_parse(s,16)` (F) / Elixir `parse(s,16)` (F) | 移除 | R-int-04（`int.parse_radix(s, 16)` 统一替代） |
| `to_float` | `int.to_float(n: Int) -> Float` | Rust `as f64` / Gleam `to_float` (F) / OCaml `to_float` (F) / Kotlin `toDouble` (F) | 移除（→ `float.from`） | R-L0-40 |
| `bits_to_float` | `int.bits_to_float(bits: Int) -> Float` | Rust `f64::from_bits` (F) / Go `math.Float64frombits` / Kotlin `Double.fromBits` / Swift `Double(bitPattern:)` | 移除 | R-float-02（`float.from_bits`） |
| `bits_to_f32` | `int.bits_to_f32(bits: Int) -> Float` | Rust `f32::from_bits` / Go `math.Float32frombits` / Kotlin `Float.fromBits` | 移除 | R-float-02（`float32.from_bits` → `float32.to_float`） |
| `abs` | `int.abs(n: Int) -> Int` | Rust `abs` / Go — / Python `abs` / JS `Math.abs` / Gleam `absolute_value` (F) / Elixir `abs` / OCaml `abs` (F) / Kotlin `absoluteValue` (F) / Swift `abs` | 保留 | R-math-01（`math.abs` 是被移除的对应函数） |
| `min` | `int.min(a: Int, b: Int) -> Int` | Rust `min` / Go `min` / Python `min` / JS `Math.min` / Gleam `min` (F) / OCaml `min` (F) / Kotlin `minOf` / Swift `min` | 保留 | R-math-01（`math.min` 是被移除的对应函数） |
| `max` | `int.max(a: Int, b: Int) -> Int` | Rust `max` / Go `max` / Python `max` / JS `Math.max` / Gleam `max` (F) / OCaml `max` (F) / Kotlin `maxOf` / Swift `max` | 保留 | R-math-01（`math.max` 是被移除的对应函数） |
| `clamp` | `int.clamp(n: Int, lo: Int, hi: Int) -> Int` | Rust `clamp` / Gleam `clamp` (F) / Kotlin `coerceIn` (F) / Swift — | 保留 | — |
| `is_even` | `int.is_even(n: Int) -> Bool` | Gleam `is_even` (F) / Elixir `is_even` (F) / Swift `isMultiple(of: 2)` / 其他 — | 保留 | — |
| `is_odd` | `int.is_odd(n: Int) -> Bool` | Gleam `is_odd` (F) / Elixir `is_odd` (F) / 其他 — | 保留 | — |
| `band` | `int.band(a: Int, b: Int) -> Int` | Gleam `bitwise_and` (F) / Elixir `Bitwise.band` / OCaml `logand` (F) / Kotlin `and` (F) / Rust・Go・Python・JS・Swift 运算符 `&` | 保留 | R-int-05 ×（不改名） |
| `bor` | `int.bor(a: Int, b: Int) -> Int` | Gleam `bitwise_or` (F) / Elixir `Bitwise.bor` / OCaml `logor` (F) / Kotlin `or` (F) / 其他 5 种语言 运算符 `\|` | 保留 | R-int-05 × |
| `bxor` | `int.bxor(a: Int, b: Int) -> Int` | Gleam `bitwise_exclusive_or` (F) / Elixir `Bitwise.bxor` / OCaml `logxor` (F) / Kotlin `xor` (F) / 其他 运算符 `^` | 保留 | R-int-05 × |
| `bnot` | `int.bnot(a: Int) -> Int` | Gleam `bitwise_not` (F) / Elixir `Bitwise.bnot` / OCaml `lognot` (F) / Kotlin `inv` (F) / 其他 运算符 `!`/`~` | 保留 | R-int-05 × |
| `bshl` | `int.bshl(a: Int, n: Int) -> Int` | Gleam `bitwise_shift_left` (F) / Elixir `Bitwise.bsl` / OCaml `shift_left` (F) / Kotlin `shl` (F) / 其他 运算符 `<<` | 保留 | R-int-05 × |
| `bshr` | `int.bshr(a: Int, n: Int) -> Int` | Gleam `bitwise_shift_right` (F) / Elixir `Bitwise.bsr` / OCaml `shift_right` (F) / Kotlin `shr` (F) / 其他 运算符 `>>` | 保留 | R-int-05 × |
| `count_leading_zeros` | `int.count_leading_zeros(n: Int) -> Int` | Rust `leading_zeros` / Go `bits.LeadingZeros64` / OCaml `leading_zeros` (F) / Kotlin `countLeadingZeroBits` (F) / Swift `leadingZeroBitCount` | 保留 | — |
| `count_trailing_zeros` | `int.count_trailing_zeros(n: Int) -> Int` | Rust `trailing_zeros` / Go `bits.TrailingZeros64` / OCaml `trailing_zeros` (F) / Kotlin `countTrailingZeroBits` (F) / Swift `trailingZeroBitCount` | 保留 | — |
| `pop_count` | `int.pop_count(n: Int) -> Int` | Rust `count_ones` / Go `bits.OnesCount64` / Python `bit_count` / OCaml `popcount` (F) / Elixir `popcount` (F) / Kotlin `countOneBits` (F) / Swift `nonzeroBitCount` | 改名 → `count_ones` | R-int-07 |
| `bit_reverse` | `int.bit_reverse(n: Int) -> Int` | Rust `reverse_bits` / Go `bits.Reverse64` / 其他 — | 保留 | — |
| `byte_swap` | `int.byte_swap(n: Int) -> Int` | Rust `swap_bytes` / Go `bits.ReverseBytes64` / Swift `byteSwapped` | 保留 | — |
| `bit_width` | `int.bit_width(n: Int) -> Int` | Go `bits.Len64` / Python `bit_length` / OCaml `unsigned_bitsize` (F) / Swift `bitWidth`（含义不同） | 改名 → `bit_length` | R-int-11 |
| `log2_floor` | `int.log2_floor(n: Int) -> Int` | Rust `ilog2` / 其他 — | 保留 | — |
| `log2_ceil` | `int.log2_ceil(n: Int) -> Int` | 其他 —（Rust 来自 `next_power_of_two`） | 保留 | — |
| `next_power_of_two` | `int.next_power_of_two(n: Int) -> Int` | Rust `next_power_of_two` / 其他 — | 保留 | — |
| `prev_power_of_two` | `int.prev_power_of_two(n: Int) -> Int` | Kotlin `takeHighestOneBit` (F) / 其他 — | 保留 | — |
| `wrap_add` | `int.wrap_add(a: Int, b: Int) -> Int` | Rust `wrapping_add` / Swift `&+` / JS `BigInt.asUintN` / 其他 — | 语义变更（`bits` 移除后按 64 位回绕；N 位与 `int.wrap` 组合） | R-int-12 |
| `wrap_mul` | `int.wrap_mul(a: Int, b: Int) -> Int` | Rust `wrapping_mul` / Swift `&*` / 其他 — | 语义变更（`bits` 移除后按 64 位回绕；N 位与 `int.wrap` 组合） | R-int-12 |
| `rotate_right` | `int.rotate_right(a: Int, n: Int) -> Int` | Rust `rotate_right` / Go `bits.RotateLeft64(x,-k)` / Kotlin `rotateRight` (F) | 语义变更（`bits` 移除后按 64 位旋转） | R-int-12 |
| `rotate_left` | `int.rotate_left(a: Int, n: Int) -> Int` | Rust `rotate_left` / Go `bits.RotateLeft64` / Kotlin `rotateLeft` (F) | 语义变更（`bits` 移除后按 64 位旋转） | R-int-12 |
| `to_u32` | `int.to_u32(a: Int) -> Int` | JS `x >>> 0` / Rust `as u32 as i64` / 其他 — | 移除 | R-int-02（`int.wrap(x, 32)`。#3851 E） |
| `to_u8` | `int.to_u8(a: Int) -> Int` | Rust `as u8` / 其他 — | 移除 | R-int-02（`int.wrap(x, 8)`。#3851 E） |
| `to_int8` | `int.to_int8(n: Int) -> Int8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 移除（→ `int8.from`） | R-L0-40 |
| `to_int16` | `int.to_int16(n: Int) -> Int16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 移除（→ `int16.from`） | R-L0-40 |
| `to_int32` | `int.to_int32(n: Int) -> Int32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 移除（→ `int32.from`） | R-L0-40 |
| `to_uint8` | `int.to_uint8(n: Int) -> UInt8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 移除（→ `uint8.from`） | R-L0-40 |
| `to_uint16` | `int.to_uint16(n: Int) -> UInt16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 移除（→ `uint16.from`） | R-L0-40 |
| `to_uint32` | `int.to_uint32(n: Int) -> UInt32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 移除（→ `uint32.from`） | R-L0-40 |
| `to_uint64` | `int.to_uint64(n: Int) -> UInt64` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 移除（→ `uint64.from`） | R-L0-40 |
| `to_float32` | `int.to_float32(n: Int) -> Float32` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 移除（→ `float32.from`） | R-L0-40 |
| `to_float64` | `int.to_float64(n: Int) -> Float64` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 移除（→ `float64.from`） | R-L0-40 |
| `to_int64` | `int.to_int64(n: Int) -> Int64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 移除（→ `int64.from`） | R-L0-40 |
| `from_int8` | `int.from_int8(n: Int8) -> Int` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 移除 | R-sized-01（`int8.to_int` 为新位置。R-L0-07） |
| `from_int16` | `int.from_int16(n: Int16) -> Int` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 移除 | R-sized-01（`int16.to_int` 为新位置。R-L0-07） |
| `from_int32` | `int.from_int32(n: Int32) -> Int` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 移除 | R-sized-01（`int32.to_int` 为新位置。R-L0-07） |
| `from_int64` | `int.from_int64(n: Int64) -> Int` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 移除 | R-sized-01（`int64.to_int` 为新位置。R-L0-07） |
| `from_uint8` | `int.from_uint8(n: UInt8) -> Int` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 移除 | R-sized-01（`uint8.to_int` 为新位置。R-L0-07） |
| `from_uint16` | `int.from_uint16(n: UInt16) -> Int` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 移除 | R-sized-01（`uint16.to_int` 为新位置。R-L0-07） |
| `from_uint32` | `int.from_uint32(n: UInt32) -> Int` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 移除 | R-sized-01（`uint32.to_int` 为新位置。R-L0-07） |
| `from_uint64` | `int.from_uint64(n: UInt64) -> Int` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 移除 | R-sized-01（`uint64.to_int` 为新位置。R-L0-07） |
| `to_int8_checked` | `int.to_int8_checked(n: Int) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` 系列 / 其他 6 种语言 — | 移除（→ `int8.from_checked`） | R-L0-40 |
| `to_int16_checked` | `int.to_int16_checked(n: Int) -> Option[Int16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` 系列 / 其他 6 种语言 — | 移除（→ `int16.from_checked`） | R-L0-40 |
| `to_int32_checked` | `int.to_int32_checked(n: Int) -> Option[Int32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` 系列 / 其他 6 种语言 — | 移除（→ `int32.from_checked`） | R-L0-40 |
| `to_uint8_checked` | `int.to_uint8_checked(n: Int) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` 系列 / 其他 6 种语言 — | 移除（→ `uint8.from_checked`） | R-L0-40 |
| `to_uint16_checked` | `int.to_uint16_checked(n: Int) -> Option[UInt16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` 系列 / 其他 6 种语言 — | 移除（→ `uint16.from_checked`） | R-L0-40 |
| `to_uint32_checked` | `int.to_uint32_checked(n: Int) -> Option[UInt32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` 系列 / 其他 6 种语言 — | 移除（→ `uint32.from_checked`） | R-L0-40 |
| `to_uint64_checked` | `int.to_uint64_checked(n: Int) -> Option[UInt64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` 系列 / 其他 6 种语言 — | 移除（→ `uint64.from_checked`） | R-L0-40 |
| `to_float32_checked` | `int.to_float32_checked(n: Int) -> Option[Float32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` 系列 / 其他 6 种语言 — | 移除（→ `float32.from_checked`） | R-L0-40 |
| `to_int8_saturating` | `int.to_int8_saturating(n: Int) -> Int8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 移除（→ `int8.from_saturating`） | R-L0-40 |
| `to_int16_saturating` | `int.to_int16_saturating(n: Int) -> Int16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 移除（→ `int16.from_saturating`） | R-L0-40 |
| `to_int32_saturating` | `int.to_int32_saturating(n: Int) -> Int32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 移除（→ `int32.from_saturating`） | R-L0-40 |
| `to_uint8_saturating` | `int.to_uint8_saturating(n: Int) -> UInt8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 移除（→ `uint8.from_saturating`） | R-L0-40 |
| `to_uint16_saturating` | `int.to_uint16_saturating(n: Int) -> UInt16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 移除（→ `uint16.from_saturating`） | R-L0-40 |
| `to_uint32_saturating` | `int.to_uint32_saturating(n: Int) -> UInt32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 移除（→ `uint32.from_saturating`） | R-L0-40 |
| `to_uint64_saturating` | `int.to_uint64_saturating(n: Int) -> UInt64` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 移除（→ `uint64.from_saturating`） | R-L0-40 |
| `from_uint64_checked` | `int.from_uint64_checked(n: UInt64) -> Option[Int]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn` 系列 / 其他 6 种语言 — | 移除 | R-sized-01（`uint64.to_int_checked`） |
| `from_uint64_saturating` | `int.from_uint64_saturating(n: UInt64) -> Int` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 其他 7 种语言 — | 移除 | R-sized-01（`uint64.to_int_saturating`） |
| `min_value` | `int.min_value() -> Int` | Rust `i64::MIN` / Go `math.MinInt64` / OCaml `min_int` (F) / Kotlin `MIN_VALUE` (F) / Swift `.min` | 保留 | — |
| `max_value` | `int.max_value() -> Int` | Rust `i64::MAX` / Go `math.MaxInt64` / OCaml `max_int` (F) / Kotlin `MAX_VALUE` (F) / Swift `.max` | 保留 | — |
| `wrap` | （新增）`int.wrap(n: Int, bits: Int) -> Int` | Rust `as uN` 系列 / JS `BigInt.asUintN(bits, n)` / 其他 — | 新增 | R-int-02 |
| `wrap_sub` | （新增）`int.wrap_sub(a: Int, b: Int) -> Int` | Rust `wrapping_sub` / Swift `&-` / 其他 — | 新增 | R-int-02、R-int-12 |
| `pow` | （新增）`int.pow(base: Int, exp: Int) -> Int` | Rust `pow` / Gleam `power`（Float）(F) / Elixir `Integer.pow` (F) / Python `**` | 新增 | R-math-01 |
| `sign` | （新增）`int.sign(n: Int) -> Int` | Rust `signum` / Kotlin `sign` (F) / JS `Math.sign` / Swift `signum()` | 新增 | R-math-01 |
| `modulo` | （新增）`int.modulo(a: Int, b: Int) -> Int` | Python `%` / Gleam `modulo` (F) / Elixir `Integer.mod` (F) / Kotlin `mod` (F) / Rust `rem_euclid` | 新增 | R-int-03 |
| `floor_div` | （新增）`int.floor_div(a: Int, b: Int) -> Int` | Python `//` / Gleam `floor_divide` (F) / Elixir `Integer.floor_div` (F) / Kotlin `floorDiv` (F) / OCaml `fdiv` (F) | 新增 | R-int-03 |
| `to_string_radix` | （新增）`int.to_string_radix(n: Int, radix: Int) -> String` | Go `FormatInt(n, base)` / JS `toString(radix)` / Gleam `to_base_string` (F) / Elixir `to_string(n, base)` (F) / Kotlin `toString(radix)` / Swift `String(n, radix:)` | 新增 | R-int-04 |
| `parse_radix` | （新增）`int.parse_radix(s: String, radix: Int) -> Result[Int, String]` | Rust `from_str_radix` / Go `ParseInt(s, base, 64)` / Python `int(s, base)` / Gleam `base_parse` (F) / Elixir `parse(s, base)` (F) / Kotlin `toIntOrNull(radix)` / Swift `Int(s, radix:)` | 新增 | R-int-04 |
| `compare` | （新增）`int.compare(a: Int, b: Int) -> Int` | Rust `cmp` / Go `cmp.Compare` / Gleam `compare`（Order）(F) / OCaml `compare` (F) / Kotlin `compareTo` (F) / Swift `<` | 新增 | R-sized-02 |
| `from` | （新增）`int.from[S: Numeric](x: S) -> Int` | Rust `From`・`as` / Swift `init(_:)` / Python `int(x)` | 新增 | R-L0-40 |
| `from_checked` | （新增）`int.from_checked[S: Numeric](x: S) -> Int?` | Rust `TryFrom` / Swift `init(exactly:)` | 新增 | R-L0-40 |
| `from_saturating` | （新增）`int.from_saturating[S: Numeric](x: S) -> Int` | Swift `init(clamping:)` / Rust `as`（从浮点数转换） | 新增 | R-L0-40 |
| `from_wrapping` | （新增）`int.from_wrapping[S: Integer](x: S) -> Int` | Swift `init(truncatingIfNeeded:)` / Rust `as` | 新增 | R-L0-40 |
| `bshr_logical` | （新增）`int.bshr_logical(a: Int, n: Int) -> Int` | OCaml `shift_right_logical` (F) / Kotlin `ushr` (F) / JS `>>>` | 新增 | R-int-06 |

