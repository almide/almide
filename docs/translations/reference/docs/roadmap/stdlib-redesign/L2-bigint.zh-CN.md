> 说明（原本索引元数据的非活动副本）：标准库重设计 L2/L3 — bigint（新模块，R-L1-03）：与 int 对齐的任意精度整数拟议接口，运算符脱糖到该接口，两种目标产生逐字节相同的结果
<a id="l2l3-bigint新しいモジュール提案する面だけ"></a>

# L2/L3: bigint（新模块；仅列拟议接口）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-bigint.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-bigint.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-bigint.zh-CN.md)

> 非规范性历史参照版，固定原本仍为权威且不变。不自动同步，未独立重跑或核实现行实现。保留初始26函数提案与后段24公开函数的说法、初始排除与后来采纳的位运算/直接转换、内部命名变化。不擅自统一早期mod与后段modulo拼写。数值确定性及带日期测量为原文主张，并非新验证。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-bigint.md)

> 最后更新：2026-10-21
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)・[L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)（R-L1-03：添加 `bigint`）
> 相关：[L2-int](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-int.md)（使用与 `int` 相同的拼写）・[L2-sized-numbers](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md)（转换矩阵规则）
> 来源：Rust —（标准库中没有；https://docs.rs/num-bigint/ 是事实标准）・Go https://pkg.go.dev/math/big#Int ・Python https://docs.python.org/3/library/stdtypes.html#numeric-types-int-float-complex ・TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/BigInt ・Gleam https://gleam-stdlib.hexdocs.pm/gleam/int.html (F)（Erlang 目标上的 Int 是任意精度）・Elixir https://elixir.hexdocs.pm/Integer.html (F)（Integer 是任意精度）・OCaml —（Zarith 是外部库 https://github.com/ocaml/Zarith ）・Kotlin https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/math/BigInteger.html（仅 JVM）・Swift —

当前 Almide 的 `Int` 是 64 bit，溢出时回绕（实测：`int.max_value() + 1` 为 `-9223372036854775808`；`math.factorial(21)` 也会回绕）。阶乘、较大的斐波那契数、组合数会悄无声息地产生错误答案。

<a id="l2-役割の比較"></a>

## L2：角色比较

| 角色 | 提案（Almide） | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin（JVM） | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 类型 | `BigInt` | — | `big.Int` | `int` | `BigInt` | ◐ `Int`（Erlang） | `Integer` | — | `BigInteger` | — |
| 从 Int | `int.to_bigint` | — | `NewInt` | （同一类型） | `BigInt(n)` | — | — | — | `valueOf` | — |
| 转为 Int | `to_int`（`_checked`・`_saturating`） | — | `Int64` `IsInt64` | （同一类型） | `Number(b)` `asIntN` | — | — | — | `toLong` `longValueExact` | — |
| 从字符串／转为字符串 | `parse` `to_string`（也有 `_radix` 版本） | — | `SetString` `String` `Text(base)` | `int(s)` `str` | `BigInt(s)` `toString(r)` | — | `parse` `to_string` (F) | — | `BigInteger(s)` `toString(r)` | — |
| 四则运算与余数 | `add` `sub` `mul` `div` `rem`（运算符） | — | `Add` `Sub` `Mul` `Quo` `Rem` | `+ - * // %` | `+ - * / %` | — | `+ - * div rem` | — | `add` `subtract` `multiply` `divide` `remainder` | — |
| 向下取整的除法／模 | `floor_div` `modulo` | — | `Div` `Mod`（Euclid） | `//` `%` | — | — | `floor_div` `mod` (F) | — | `mod`（非负） | — |
| 取负／绝对值／幂 | `neg` `abs` `sign` `pow` | — | `Neg` `Abs` `Sign` `Exp` | `-` `abs` `**` | `-` `**` | — | `abs` `pow` (F) | — | `negate` `abs` `signum` `pow` | — |
| 比较／最小／最大 | `compare` `min` `max`（运算符 `<` 也支持） | — | `Cmp` | `<` | `<` | — | `<` | — | `compareTo` `min` `max` | — |
| 转为 Float | `to_float`（`_checked`） | — | `Float64` | `float` | `Number` | — | — | — | `toDouble` | — |

<a id="完全性の規則"></a>

### 完整性规则

1. **与 `int` 拼写一致**：若 `int` 中存在相同角色的函数，`BigInt` 函数采用相同名称及参数顺序（`abs` `min` `max` `sign` `pow` `compare` `parse` `to_string` `to_string_radix` `parse_radix` `floor_div` `mod`）。与 `int` L2 中新增的函数（R-int-03・R-int-04）对齐。
2. **转换方向**（R-L0-07）：`Int` → `BigInt` 使用源类型一侧的 `int.to_bigint`，`BigInt` → `Int` 使用 `bigint.to_int`。`BigInt` → `Int` 有损，故提供三件套（`to_int`・`to_int_checked`・`to_int_saturating`）；`BigInt` → `Float` 提供 `to_float` 和 `to_float_checked`（同 [L2-sized-numbers](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md) 规则 3・4）。不提供与定宽类型的直接转换（经由 `to_int`）。
3. **运算符**：`+` `-` `*` `/` `%` `==` `<` 等脱糖到 `bigint.add` …（L1 R-L1-03）。`/` 和 `%` 与 `Int` 一样向 0 截断，除以 0 则 abort。
4. **有意不提供的内容**：位运算（如 `band`）、`gcd`（标准库提供者只有 Go・Python・Elixir・JVM 这 4 个，不到多数）、平方根、与 `Int` 的隐式转换、`123n` 这样的字面量（R-bigint-02）。

<a id="l2-の判定"></a>

### L2 判定

- 新模块。没有重复，也不与现有拼写冲突。
- 承接的需求：保留 `math.factorial` 和 `math.choose` 的回绕（[L2-math](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-math.md)），在文档中说明需要大数时用 `bigint` 编写。

<a id="案"></a>

## 提案

<a id="r-bigint-01-任意精度整数のモジュール-bigint-を足す---採用"></a>

### R-bigint-01 添加任意精度整数模块 `bigint` — ○ 采纳

- 内容：类型 `BigInt` 与下方 L3 表的 26 个函数（含 `int.to_bigint`）。
- 依据的 L0 规则：L1 R-L1-03、R-L0-02（返回类型／主体类型的模块）、R-L0-07（转换方向）、R-L0-06（`parse` 返回 `Result`，`to_int_checked` 返回 `Option`）、R-L0-16（有序类型具有 `compare`）。
- (a) Go・Python・JS・Elixir・Kotlin（JVM）以及 Erlang 目标的 Gleam，共 6 种语言提供。
- (b) 用 Almide 写成纯计算（把数位序列放在 `List[Int]` 或 `Bytes` 中）。不需要 host 功能，将 native 与 wasm 结果连 1 bit 都不相差纳入 ADR-0016（数值确定性）的承诺。不为速度而仅在 native 使用另一套实现（ADR-0027 D8 的思路：不把单侧路径作为终点）。
- (c) 对“30 的阶乘”“第 100 个斐波那契数”等问题，LLM 会按 Python `int` 的习惯编写，因 `Int` 回绕而静默出错。有了 `bigint`，CHEATSHEET 就能用 1 行写“超过 20 的阶乘用 `bigint`”。
- (d) 新增（新模块；是否不放入 `AUTO_IMPORT_BUNDLED` 而要求 `import bigint`，遵循 L1 import 规则）。使用数 0。
- 拟议门禁：对于能放入 `Int` 的输入，差分对照 `int` 和 `bigint` 的同名函数（如 `bigint.to_int(bigint.add(a, b)) == a + b`，限于不回绕的范围）。用 wasm_cross fixture 固定两个 target 的一致性，先在 als 中加入新的契约 C 编号。从签名检查完整性规则 1 的“与 `int` 拼写一致”。
- issue: #3931

<a id="r-bigint-02-123n-のような-bigint-のリテラルを足す---不採用"></a>

### R-bigint-02 添加 `123n` 这样的 BigInt 字面量 — × 不采纳

- 内容：不添加。用 `bigint.parse("123…")!` 和 `int.to_bigint(123)` 编写。
- 依据：修改词法规则就是修改语言，ADR-0001 D3 因同样理由搁置了时间单位字面量（`100ms`）。仅 JS 有这种拼写，也不属于多数。
- issue：无

<a id="r-bigint-03-int-と-bigint-を混ぜた演算を自動で広げる---不採用"></a>

### R-bigint-03 自动扩宽混合 `Int` 与 `BigInt` 的运算 — × 不采纳

- 内容：不做。`a + b` 中一方是 `Int`、另一方是 `BigInt` 时产生类型错误，并在诊断提示中给出 `int.to_bigint(a)`。
- 依据：R-L0-07（用具名函数写转换）。隐式转换使读者看不到实际运行了哪一种类型的运算（同 ADR-0001 以类型区分时钟的理由）。
- issue：无

<a id="r-bigint-04-bigintaddsubmuldivremneg-を公開しない演算子だけにする---採用"></a>

### R-bigint-04 不公开 `bigint.add`/`sub`/`mul`/`div`/`rem`/`neg`（仅用运算符）— ○ 采纳

- 现状：提案公开 `bigint.add(a, b)` 等 6 个函数，将运算符 `+ - * / %` 和一元 `-` 脱糖到它们。
- 提案：把 6 个函数改为内部名称（如 `__add`），公开写法只保留运算符。`floor_div`・`modulo`・`abs`・`pow` 在 `int` 中也存在，因此仍公开。
- 依据的 L0 规则：R-L0-01（一个角色一种拼写）、bigint 完整性规则 1（与 `int` 拼写相同；不存在 `int.add`・`int.rem`）、R-L0-26（不公开运算符的脱糖目标）。先例是 R-matrix-02 的 `__` 名称。
- (a) 能用运算符书写的语言（Python・JS・Elixir）不出现函数拼写。采用函数写法的 Go `Add`・JVM `add` 属于没有运算符重载的语言。
- (c) 同时存在 `a + b` 和 `bigint.add(a, b)` 两种写法，会使生成代码摇摆。有 `bigint.rem` 却没有 `int.rem` 的不对称，会让 LLM 猜出 `int.rem` 而出错。
- (d) 新模块，没有兼容性影响。对源自 Java 的 `bigint.add`，提示“写成 `a + b`”（R-L0-19）。
- issue: #3973

<a id="r-bigint-05-bigintclamp-を足す---採用"></a>

### R-bigint-05 添加 `bigint.clamp` — ○ 采纳

- 提案：`fn bigint.clamp(n: BigInt, lo: BigInt, hi: BigInt) -> BigInt`。`lo > hi` 时与 `int.clamp` 一样 abort。
- 依据的 L0 规则：bigint 完整性规则 1（若 `int` 存在相同角色，便采用相同名称）。已有 `min`・`max`，唯独遗漏了 `clamp`。
- (a) Rust `Ord::clamp`、Kotlin `coerceIn`、Gleam `int.clamp`。
- (d) 新增 1 个。新模块。
- 拟议门禁：bigint 门禁也检查“`int` 中存在而不在排除表中的内容，在 bigint 中也存在”这一方向。
- issue: #3972

<a id="r-bigint-06-uint64-と-bigint-を値を失わずに行き来させる---採用"></a>

### R-bigint-06 让 `uint64` 与 `BigInt` 无损互转 — ○ 采纳

- 现状：bigint 规则 2 写着“不提供与定宽类型的直接转换（经由 `to_int`）”，但 `uint64.to_int` 会将 2^63 及以上回绕为负数（实测：`int.from_uint64(uint64.max_value())` 是 `-1`）。
- 提案：`fn uint64.to_bigint(x: UInt64) -> BigInt`（无损）和 `fn bigint.to_uint64(b: BigInt) -> UInt64` 的三件套（`_checked`・`_saturating`）。
- 依据的 L0 规则：R-L0-07（转换采用源类型的 `to_<先>`（保留的日文占位符表示目标类型））、R-L0-08（有损方向提供三件套）。
- (a) Go `SetUint64`/`Uint64`/`IsUint64`、JS `BigInt.asUintN(64, b)`。
- (d) 新增 4 个。
- 完整性规则：经由 `to_int` 会损失值的载体（目前仅 `uint64`）直接与 `BigInt` 相连。
- issue: #3972

<a id="r-bigint-07-bigint-に-integer-のビット演算を置く---採用"></a>

### R-bigint-07 为 `bigint` 提供 `Integer` 的位运算 — ○ 采纳

- 现状：`bigint` 具有 `Integer` 的算术运算（`floor_div`・`modulo`・`pow`・`is_even`・`is_odd`），但没有位运算。
- 提案：提供 `band`・`bor`・`bxor`・`bnot`・`bshl`・`bshr`（与 `int` 相同的函数，R-int-10）。负数按高位无限延伸 1 的二进制补码处理（同 Python `int`、Swift `BinaryInteger`）。`bnot(x)` 等于 `-x - 1`。
- 有意遗漏：不提供 `count_leading_zeros`・`count_trailing_zeros`・`count_ones`・`byte_swap`（它们是 `FixedWidthInteger` 的要求，对没有位宽的类型无固定答案）。
- (a) Swift 的 `BinaryInteger` 要求位运算，任意精度整数也应具备。Python 的 `int`、Go 的 `big.Int`（`And`・`Or`・`Xor`・`Not`・`Lsh`・`Rsh`）、Java/Kotlin 的 `BigInteger`（`and`・`or`・`xor`・`not`・`shiftLeft`・`shiftRight`）也提供。
- (d) 新增 6 个。
- issue: #4000

<a id="実装の状況wave-22026-10-24"></a>

## 实现状态（wave 2，2026-10-24）

- 已实现：L3 表中的 24 个公开函数（包括 `from`・`from_checked`）。在 `stdlib/bigint.almd` 中用纯 Almide 编写，通过 `import bigint` 使用。表示为符号以及按低位在前排列的 30 bit limb 的 `List[Int]`；0 不为负且没有高位为 0 的 limb，故 `==` 按值比较。
- 运算符（R-L1-03、R-bigint-04）也已实现（2026-10-24）。`BigInt` 声明为 `type BigInt: Numeric, Integer`；`+ - * / %` 和一元 `-` 脱糖到模块中的 `local fn`：`add`・`sub`・`mul`・`div`・`rem`・`neg`；`< <= > >=` 脱糖到 `compare(a, b)` 与 0 的比较（运算符与要求的对应关系仅集中在 `OPERATOR_REQUIREMENTS` 一处）。`/` 向 0 取整，`%` 采用被除数符号，与 `Int` 相同。不公开脱糖目标函数。`list.sum` 也能对 `BigInt` 求和。`spec/stdlib/bigint_operators_test.almd` 在两个 target 上检查。
- `bigint.from` 对整数保持原值，对浮点数给出向 0 截断后的精确值（连 `1e300` 都不损失数位）。`from_checked` 在存在小数部分、NaN、无穷大时返回 none。

<a id="l3-関数表提案"></a>

## L3：函数表（提案）

| 函数 | 签名（提案） | 其他语言的主要拼写 | 判定 | 提案 |
|---|---|---|---|---|
| `parse` | `bigint.parse(s: String) -> Result[BigInt, String]` | Go `SetString` / Python `int(s)` / JS `BigInt(s)` / Elixir `Integer.parse` (F) | 新增 | R-bigint-01 |
| `parse_radix` | `bigint.parse_radix(s: String, radix: Int) -> Result[BigInt, String]` | Go `SetString(s, base)` / Python `int(s, base)` / Kotlin `BigInteger(s, radix)` | 新增 | R-bigint-01 |
| `to_string` | `bigint.to_string(b: BigInt) -> String` | Go `String` / Python `str` / JS `toString` / Elixir `to_string` (F) | 新增 | R-bigint-01 |
| `to_string_radix` | `bigint.to_string_radix(b: BigInt, radix: Int) -> String` | Go `Text(base)` / JS `toString(radix)` / Kotlin `toString(radix)` | 新增 | R-bigint-01 |
| `add` | `bigint.add(a: BigInt, b: BigInt) -> BigInt`（`+`） | Go `Add` / Kotlin `add` / 其他为 `+` | 不添加（作为运算符内部脱糖目标） | R-bigint-04 |
| `sub` | `bigint.sub(a: BigInt, b: BigInt) -> BigInt`（`-`） | Go `Sub` / Kotlin `subtract` | 不添加（作为运算符内部脱糖目标） | R-bigint-04 |
| `mul` | `bigint.mul(a: BigInt, b: BigInt) -> BigInt`（`*`） | Go `Mul` / Kotlin `multiply` | 不添加（作为运算符内部脱糖目标） | R-bigint-04 |
| `div` | `bigint.div(a: BigInt, b: BigInt) -> BigInt`（`/`，向 0；除以 0 则 abort） | Go `Quo` / Kotlin `divide` / JS `/` | 不添加（作为运算符内部脱糖目标） | R-bigint-04 |
| `rem` | `bigint.rem(a: BigInt, b: BigInt) -> BigInt`（`%`） | Go `Rem` / Kotlin `remainder` / JS `%` | 不添加（作为运算符内部脱糖目标） | R-bigint-04 |
| `floor_div` | `bigint.floor_div(a: BigInt, b: BigInt) -> BigInt` | Python `//` / Elixir `Integer.floor_div` (F) | 新增 | R-bigint-01（与 R-int-03 拼写相同） |
| `modulo` | `bigint.modulo(a: BigInt, b: BigInt) -> BigInt` | Python `%` / Elixir `Integer.mod` (F) / Kotlin `mod` | 新增 | R-bigint-01（与 R-int-03 拼写相同） |
| `neg` | `bigint.neg(a: BigInt) -> BigInt`（一元 `-`） | Go `Neg` / Kotlin `negate` | 不添加（作为运算符内部脱糖目标） | R-bigint-04 |
| `abs` | `bigint.abs(a: BigInt) -> BigInt` | Go `Abs` / Python `abs` / Kotlin `abs` | 新增 | R-bigint-01 |
| `sign` | `bigint.sign(a: BigInt) -> Int` | Go `Sign` / Kotlin `signum` | 新增 | R-bigint-01 |
| `pow` | `bigint.pow(base: BigInt, exp: Int) -> BigInt`（负的 `exp` 会 abort） | Go `Exp` / Python `**` / Elixir `Integer.pow` (F) / Kotlin `pow` | 新增 | R-bigint-01 |
| `compare` | `bigint.compare(a: BigInt, b: BigInt) -> Int` | Go `Cmp` / Kotlin `compareTo` | 新增 | R-bigint-01 |
| `min` | `bigint.min(a: BigInt, b: BigInt) -> BigInt` | Kotlin `min` / Python `min` | 新增 | R-bigint-01 |
| `max` | `bigint.max(a: BigInt, b: BigInt) -> BigInt` | Kotlin `max` / Python `max` | 新增 | R-bigint-01 |
| `is_even` | `bigint.is_even(a: BigInt) -> Bool` | Elixir `Integer.is_even` (F) / Kotlin `!testBit(0)` | 新增 | R-bigint-01（与 `int.is_even` 拼写相同） |
| `is_odd` | `bigint.is_odd(a: BigInt) -> Bool` | Elixir `Integer.is_odd` (F) | 新增 | R-bigint-01 |
| `clamp` | `bigint.clamp(n: BigInt, lo: BigInt, hi: BigInt) -> BigInt` | Rust `clamp` / Kotlin `coerceIn` | 新增 | R-bigint-05 |
| `from` | （新增）`bigint.from[S: Numeric](x: S) -> BigInt` | Rust `From`・`as` / Swift `init(_:)` / Python `int(x)` | 新增 | R-L0-40 |
| `from_checked` | （新增）`bigint.from_checked[S: Numeric](x: S) -> BigInt?` | Rust `TryFrom` / Swift `init(exactly:)` | 新增 | R-L0-40 |
| `bit_length` | （新增）`bigint.bit_length(n: BigInt) -> Int` | Python `bit_length` / Go `BitLen` | 新增 | R-int-11 |
| `band` | `bigint.band(a: BigInt, b: BigInt) -> BigInt` | Elixir `band` / Rust・Go・Swift 运算符 `&` | 新增 | R-bigint-07 |
| `bor` | `bigint.bor(a: BigInt, b: BigInt) -> BigInt` | Elixir `bor` / Rust・Go・Swift 运算符 `\|` | 新增 | R-bigint-07 |
| `bxor` | `bigint.bxor(a: BigInt, b: BigInt) -> BigInt` | Elixir `bxor` / Rust・Go・Swift 运算符 `^` | 新增 | R-bigint-07 |
| `bnot` | `bigint.bnot(a: BigInt) -> BigInt` | Elixir `bnot` / Swift `~` | 新增 | R-bigint-07 |
| `bshl` | `bigint.bshl(a: BigInt, n: Int) -> BigInt` | Elixir `bsl` / 运算符 `<<` | 新增 | R-bigint-07 |
| `bshr` | `bigint.bshr(a: BigInt, n: Int) -> BigInt` | Elixir `bsr` / 运算符 `>>` | 新增 | R-bigint-07 |
