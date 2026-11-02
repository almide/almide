> 说明（原本索引元数据的非活动副本）：标准库重设计 L2/L3 — math：仅保留无法归入类型模块的内容；把 abs/min/max/sign/pow 移至 int，把 sqrt/fmin/fmax/fpow 移至 float；补充缺失的反三角、双曲、hypot 和 cbrt
<a id="l2l3-math"></a>

# L2/L3: math

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-math.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-math.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-math.zh-CN.md)

> 非规范性历史重设计参照版。固定原本权威且不变，不自动同步、未独立重跑或核实现行实现。保留初始将sqrt移至float的方案及后来逆转、反双曲函数初始排除及后来新增、不同使用数快照。原文移除9/现行23加7的计数与改订决定及扩展表并存，7/9主张与列举实例也不擅自统一。数值检查及target一致性是原提案/主张，不是新核实结果。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-math.md)

> 最后更新：2026-10-21
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)・[L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)
> 相关：[L2-int](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-int.md)・[L2-float](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-float.md)
> 来源：Rust https://doc.rust-lang.org/std/primitive.f64.html (F) ・Go https://pkg.go.dev/math ・Python https://docs.python.org/3/library/math.html ・TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Math ・Gleam https://gleam-stdlib.hexdocs.pm/gleam/float.html (F) ・Elixir https://www.erlang.org/doc/apps/stdlib/math.html ・OCaml https://ocaml.org/manual/latest/api/Float.html (F) ・Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.math/ ・Swift https://developer.apple.com/documentation/foundation/1779833-pow

(F) = 于 2026-10-21 获取官方页面函数列表并核对的拼写。Gleam 标准库没有三角函数（float 仅有 `power`・`square_root`・`exponential`・`logarithm` (F)）。

<a id="l2-役割の比較"></a>

## L2：角色比较

| 角色 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 常数 π・e | `pi()` `e()` | `consts::PI` | `math.Pi` | `math.pi` | `Math.PI` | — | `:math.pi` | `Float.pi` (F) | `PI` `E` | `.pi` |
| 三角函数 | `sin` `cos` `tan` `atan` | `sin` 等 (F) | `Sin` 等 | `sin` 等 | `Math.sin` 等 | — | `:math.sin` 等 | `sin` 等 (F) | `sin` 等 | `sin` 等 |
| 反三角函数・`atan2` | `atan` 仅此 | `asin` `acos` `atan2` (F) | `Asin` `Acos` `Atan2` | `asin` `acos` `atan2` | `asin` `acos` `atan2` | — | `asin` `acos` `atan2` | `asin` `acos` `atan2` (F) | `asin` `acos` `atan2` | `asin` `acos` `atan2` |
| 双曲函数 | `tanh` 仅此 | `sinh` `cosh` `tanh` (F) | `Sinh` `Cosh` `Tanh` | 有 | 有 | — | 有 | 有 (F) | 有 | 有 |
| 指数／对数 | `exp` `log` `log10` `log2` | `exp` `ln` `log10` `log2` (F) | `Exp` `Log` 等 | `exp` `log` 等 | `Math.exp` `log` 等 | `exponential` `logarithm` (F) | `exp` `log` 等 | `exp` `log` 等 (F) | `exp` `ln` 等 | `exp` `log` 等 |
| 平方根／立方根／斜边 | `sqrt`（`float.sqrt` 的重复项） | `sqrt` `cbrt` `hypot` (F) | `Sqrt` `Cbrt` `Hypot` | `sqrt` `cbrt` `hypot` | `sqrt` `cbrt` `hypot` | `square_root` (F) | `sqrt` | `sqrt` `cbrt` `hypot` (F) | `sqrt` `cbrt` `hypot` | `sqrt` `cbrt` `hypot` |
| 幂 | `pow`（Int）`fpow`（Float） | `i64::pow` `f64::powf` (F) | `Pow` | `pow` `**` | `**` | `int.power` `float.power` (F) | `Integer.pow` (F) | `Float.pow` (F) | `pow` | `pow` |
| 绝对值／最小／最大／符号（Int） | `abs` `min` `max` `sign`（`int.*` 的重复项） | Int 方法 | `min` `max` | 内置 | `Math.*` | `int.*` (F) | `abs` | `Int.abs` (F) | `abs` `sign` | `abs` |
| 最小／最大（Float） | `fmin` `fmax`（`float.min/max` 的重复项） | Float 方法 (F) | `min` `max` | 内置 | `Math.min` | `float.min` (F) | `min` | `Float.min` (F) | `minOf` | `min` |
| 伽马函数的对数 | `log_gamma` | `ln_gamma`（nightly）(F) | `Lgamma` | `lgamma` | — | — | — | — | — | `lgamma` |
| 阶乘／二项式系数 | `factorial` `choose` | — | — | `factorial` `comb` | — | — | — | — | — | — |

<a id="完全性の規則"></a>

### 完整性规则

1. **归属位置**（R-L0-02）：`math` 只容纳无法归入类型模块的函数，即超越函数（三角、反三角、双曲、指数、对数、伽马）、常数以及整数的组合数学（`factorial`、`choose`）。主体类型确定的算术（`abs`、`min`、`max`、`sign`、`pow`、`sqrt`）放入 `int` 或 `float`。
2. **超越函数族**：三角函数（`sin` `cos` `tan`）、反三角函数（`asin` `acos` `atan` `atan2`）、双曲函数（`sinh` `cosh` `tanh`）均应完整提供。有意不提供反双曲函数（如 `asinh`）（→ R-math-05 重新审议后决定添加）（Rust・Python・JS・OCaml・Swift 等标准库提供，但使用场景少，可通过 `log` 和 `sqrt` 用 1 行写出）。
3. `math` 函数全部接收 `Float`（仅 `factorial` 和 `choose` 的 `Int` 例外）。不采用用 `f` 前缀区分类型的拼写（`fmin` `fmax` `fpow`）。

<a id="l2-の判定"></a>

### L2 判定

- 重复（#3851 B）：`math.abs` `min` `max` ≡ `int.*`、`math.sqrt` ≡ `float.sqrt`、`math.fmin` `fmax` ≡ `float.min` `max` → R-math-01。
- 归属：`math.sign`（Int）、`math.pow`（Int）、`math.fpow`（Float）的主体类型确定 → 在 R-math-01 中移至 `int.sign`、`int.pow`、`float.pow`。#3851 曾以“没有 `int.pow`，因此没有可合并的对象”为由保留 `math.pow`・`fpow`・`sign`，但后来确定的 L0 R-L0-02 规定“`math` 仅容纳无法归入类型模块的内容”，因此改判为移动。
- 缺失角色：反三角 `asin` `acos` `atan2`（8/9）、双曲 `sinh` `cosh`（8/9）、`hypot` `cbrt`（7/9）→ R-math-02。
- 保持：`factorial` 超过 20 时回绕，由 R-L1-03 的 `bigint` 承接大数需求（[L2-bigint](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-bigint.md)）。

<a id="案"></a>

## 提案

<a id="r-math-01-型の決まる算術を-intfloat-に移す---採用"></a>

### R-math-01 将类型确定的算术移至 `int`・`float` — ○ 采纳

- 内容：
  - 移除（保留的一侧已存在）：`math.abs` → `int.abs`、`math.min` → `int.min`、`math.max` → `int.max`、（`math.sqrt` → `float.sqrt` 在 R-math-03 中取消）、`math.fmin` → `float.min`、`math.fmax` → `float.max`。
  - 移动（新增与移除配对）：`math.sign` → `int.sign(n: Int) -> Int`、`math.pow` → `int.pow(base: Int, exp: Int) -> Int`、`math.fpow` → `float.pow(base: Float, exp: Float) -> Float`。语义不变（`int.pow` 回绕，负的 `exp` 会 abort；`int.sign(0)` 为 0）。
- 依据的 L0 规则：R-L0-01、R-L0-02。
- (a) 7/9 的语言将其放在主体类型的模块或方法中（Rust・Kotlin・Swift 为方法，Gleam・Elixir・OCaml 为类型模块）。Gleam 按类型提供 `int.power` 和 `float.power` (F)。
- (b) 函数体相同（`runtime/rs/src/math.rs` 与 `int.rs`・`float.rs` 中的相同计算，#3851 B）。`int.pow` 和 `float.pow` 原样接用 `math.pow`・`math.fpow` 的 intrinsic 与 self-host 函数体。两个 target 均不变。
- (c) 像 `int.abs` 和 `math.abs` 那样有 2 个归属位置，LLM 就猜不准应在哪一处。`fpow` 的 `f` 前缀在 9 种语言中均无这种拼写；`float.pow` 更能从“Float 的函数在 `float.`”的规则推中。
- (d) 移除 9 个，新增 3 个（`int.sign`・`int.pow`・`float.pow`，表位于 L2-int・L2-float）。采用 `@deprecated(since=15, use="int.abs")` 等，removal epoch 16。`almide fix` 全部 1 对 1（如 `math.fpow(a, b)` → `float.pow(a, b)`）。使用数（spec/tests/docs/stdlib）：`abs` 13/4/9/0、`min` 7/1/10/0、`max` 7/0/10/0、`sqrt` 22/0/11/2（+20 research）、`fmin` 12/4/10/2、`fmax` 11/0/8/0、`sign` 6/0/6/1、`pow` 20/1/19/3（+8 research）、`fpow` 35/3/12/6。改写 llms.txt:413・419・426 的示例。
- 拟议门禁：用 #3851 F 的同义词门禁规则 5（跨模块相同函数体）防止 `math`↔`int`/`float` 再次出现重复。设立 R-L0-G 的归属检查（`math` 公开函数的第 1 个参数为 `Float`，仅 `factorial`・`choose` 例外），允许列表为 0。
- issue: #3851 B（已有；在此 issue 追加 `sign`・`pow`・`fpow` 的判定变更）

<a id="r-math-02-逆三角関数双曲線関数hypotcbrt-を足す---採用"></a>

### R-math-02 添加反三角、双曲函数以及 `hypot`・`cbrt` — ○ 采纳

- 内容：`math.asin(x: Float) -> Float`、`math.acos(x: Float) -> Float`、`math.atan2(y: Float, x: Float) -> Float`、`math.sinh(x: Float) -> Float`、`math.cosh(x: Float) -> Float`、`math.hypot(x: Float, y: Float) -> Float`、`math.cbrt(x: Float) -> Float`。定义域之外为 NaN（`asin(2.0)`），与现有 `math.*` 相同。
- 依据的 L0 规则：L1 新增标准（7～8/9）、完整性规则 2。`atan2` 参数顺序（y, x）在全部 9 种语言中相同。
- (a) Rust (F)・Go・Python・JS・Elixir（`:math`）・OCaml (F)・Kotlin・Swift。仅 Gleam 没有。
- (b) 纯计算。wasm 没有三角函数指令，因此像现有 `math_trig.almd`・`math_atan.almd` 一样用 Almide 移植 musl，纳入 ADR-0016（数值确定性）的检查对象。native 不用 libm，而走相同 Almide 函数体，或通过 fixture 确认 byte 一致。
- (c) 计算角度时 LLM 首先写的是 `atan2`。目前写 `atan(y / x)` 会弄错象限。
- (d) 新增 7 个，使用数 0。
- 拟议门禁：在现有数值检查（semantics-manifest 行与 inspection ledger）中添加 7 行，用 wasm_cross fixture 固定两个 target 的 byte 一致。
- issue: #3926

<a id="r-math-03-実数の数学関数は-math-に集めるsqrt-を-math-に戻しfloatsqrt-を撤去---採用r-math-01-の-sqrt-の部分を見直す"></a>

### R-math-03 将实数数学函数集中到 `math`（把 `sqrt` 放回 `math`，移除 `float.sqrt`）— ○ 采纳（重新审议 R-math-01 中的 `sqrt` 部分）

- 现状：R-math-01 移除 `math.sqrt` 并统一到 `float.sqrt`，R-math-02 又将 `cbrt`・`hypot` 加入 `math`。于是只有 `sqrt` 在 `float`，而 `cbrt`・`hypot` 及三角、指数在 `math`。
- 提案：保留 `math.sqrt(x: Float) -> Float`，移除 `float.sqrt`。用以下 1 句话确定归属边界：“`Int` 也有相同角色的内容（`abs`・`min`・`max`・`clamp`・`sign`・`pow`）属于类型模块；仅实数的数学函数（`sqrt`・`cbrt`・`hypot`、三角・指数・对数・伽马）属于 `math`；舍入・判定・常数・位模式・转换作为表示层函数属于 `float`。”
- 依据的 L0 规则：R-L0-02（由一条规则确定归属；将 L0 表述改为这 1 句话）、R-L0-00 的 2、R-L0-01。
- (a) 9 种语言中 8 种把 `sqrt` 与 `sin` 放在同一位置，其中 6 种（Go・Python・JS・Elixir・Kotlin・Swift）放在 math 一侧。没有语言将 `sqrt` 与 `cbrt`・`hypot` 分置不同位置。
- (c) 当前提案下，`math.cbrt(x)` 可以通过，而 `math.sqrt(x)` 得到 E002。`math.sqrt` 是 LLM 最容易写出的拼写。
- (d) 移除 `float.sqrt`（`@deprecated(since=15, use="math.sqrt")`，epoch 16，`almide fix` 1 对 1）。取消移除 `math.sqrt`。使用数为 `math.sqrt` 17/0/5/0/18、`float.sqrt` 14/2/4/1/0（spec/tests/docs/stdlib/research），此方向所需改写更少。
- 完整性规则：`math` 公开函数的参数全部为 `Float`（`factorial`・`choose` 例外）。`float` 与 `math` 不放置同名函数。
- 拟议门禁：在 R-L0-G 中加入 `math`/`float` 名称分配表，与两个模块公开名称对照。
- issue: #3972

<a id="r-math-04-expm1log1p-を足す---採用"></a>

### R-math-04 添加 `expm1`・`log1p` — ○ 采纳

- 现状：已有 `math.exp`・`math.log`，但没有针对接近 0 的 `x` 精确求取 `exp(x) - 1`・`log(1 + x)` 的函数。组合计算会丢失有效数字，因此并非便利函数，而是其他函数无法表达的角色。
- 提案：`fn math.expm1(x: Float) -> Float`、`fn math.log1p(x: Float) -> Float`。
- 依据：L1 新增标准（5 种语言以上）。
- (a) Rust `exp_m1`/`ln_1p`、Go `Expm1`/`Log1p`、Python `expm1`/`log1p`、JS `Math.expm1`/`log1p`、Kotlin `expm1`/`ln1p`、OCaml `expm1`/`log1p`。
- (b) 纯计算。与 R-math-02 一样，用 Almide 移植 musl，纳入 ADR-0016（数值确定性）检查。
- (d) 新增 2 个。
- issue: #3980

<a id="r-math-05-逆双曲線関数-asinhacoshatanh-を足す完全性の規則-2-の見直し---採用"></a>

### R-math-05 添加反双曲函数 `asinh`・`acosh`・`atanh`（重新审议完整性规则 2）— ○ 采纳

- 现状：完整性规则 2 曾规定“反双曲函数使用场景少，因此不提供”。但 Rust・Go・Python・JS・Kotlin・OCaml・Swift 标准库均提供，满足 L1 新增标准（5 种语言以上）。没有理由对其他函数应用标准，却唯独排除这一处。
- 提案：`fn math.asinh(x: Float) -> Float`、`fn math.acosh(x: Float) -> Float`、`fn math.atanh(x: Float) -> Float`。定义域之外（`acosh` 的 `x < 1`、`atanh` 的 `|x| > 1`）与 `math.asin` 一样返回 NaN。
- (b) 同 R-math-04。
- (d) 新增 3 个。三角与双曲函数族的正函数、反函数均完整。
- 完整性规则（替换 2）：三角函数与双曲函数各具备 3 个正函数及 3 个反函数。
- issue: #3980

<a id="l3-関数表"></a>

## L3：函数表

`math` 当前的 23 个函数及新增的 7 个函数。

| 函数 | 签名（现行，mut 标注） | 其他语言的主要拼写 | 判定 | 提案 |
|---|---|---|---|---|
| `abs` | `math.abs(n: Int) -> Int` | Rust `abs` / Gleam `int.absolute_value` (F) / OCaml `Int.abs` (F) / Kotlin `absoluteValue` (F) | 移除 | R-math-01（`int.abs`） |
| `atan` | `math.atan(x: Float) -> Float` | Rust `atan` (F) / Go `Atan` / Python・JS `atan` / OCaml `atan` (F) / Kotlin `atan` | 保持 | — |
| `choose` | `math.choose(n: Int, k: Int) -> Int` | Python `math.comb` / 其他 — | 保持 | — |
| `cos` | `math.cos(x: Float) -> Float` | Rust `cos` (F) / Go `Cos` / Python・JS `cos` / OCaml `cos` (F) | 保持 | — |
| `e` | `math.e() -> Float` | Rust `consts::E` / Go `math.E` / Python `math.e` / JS `Math.E` / Kotlin `E` | 保持 | — |
| `exp` | `math.exp(x: Float) -> Float` | Rust `exp` (F) / Go `Exp` / Gleam `exponential` (F) / OCaml `exp` (F) | 保持 | — |
| `factorial` | `math.factorial(n: Int) -> Int` | Python `math.factorial` / 其他 — | 保持 | —（大数见 [L2-bigint](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-bigint.md)） |
| `fmax` | `math.fmax(a: Float, b: Float) -> Float` | Rust `f64::max` (F) / Go `max` / Gleam `float.max` (F) / OCaml `Float.max` (F) | 移除 | R-math-01（`float.max`） |
| `fmin` | `math.fmin(a: Float, b: Float) -> Float` | Rust `f64::min` (F) / Go `min` / Gleam `float.min` (F) / OCaml `Float.min` (F) | 移除 | R-math-01（`float.min`） |
| `fpow` | `math.fpow(base: Float, exp: Float) -> Float` | Rust `powf` (F) / Go `Pow` / Gleam `float.power` (F) / OCaml `Float.pow` (F) / Kotlin `pow` | 移除 | R-math-01（`float.pow`） |
| `log` | `math.log(x: Float) -> Float` | Rust `ln` (F) / Go `Log` / Python・JS `log` / OCaml `log` (F) / Kotlin `ln` | 保持 | — |
| `log10` | `math.log10(x: Float) -> Float` | Rust `log10` (F) / Go `Log10` / Python・JS `log10` / OCaml `log10` (F) | 保持 | — |
| `log2` | `math.log2(x: Float) -> Float` | Rust `log2` (F) / Go `Log2` / Python・JS `log2` / OCaml `log2` (F) | 保持 | — |
| `log_gamma` | `math.log_gamma(x: Float) -> Float` | Rust `ln_gamma`（nightly）(F) / Go `Lgamma` / Python `lgamma` / Swift `lgamma` | 保持 | — |
| `max` | `math.max(a: Int, b: Int) -> Int` | Rust `max` / Gleam `int.max` (F) / OCaml `Int.max` (F) | 移除 | R-math-01（`int.max`） |
| `min` | `math.min(a: Int, b: Int) -> Int` | Rust `min` / Gleam `int.min` (F) / OCaml `Int.min` (F) | 移除 | R-math-01（`int.min`） |
| `pi` | `math.pi() -> Float` | Rust `consts::PI` / Go `math.Pi` / Python `math.pi` / JS `Math.PI` / OCaml `Float.pi` (F) | 保持 | — |
| `pow` | `math.pow(base: Int, exp: Int) -> Int` | Rust `i64::pow` / Gleam `int.power`（Float）(F) / Elixir `Integer.pow` (F) | 移除 | R-math-01（`int.pow`） |
| `sign` | `math.sign(n: Int) -> Int` | Rust `signum` / Kotlin `sign` (F) / JS `Math.sign` | 移除 | R-math-01（`int.sign`） |
| `sin` | `math.sin(x: Float) -> Float` | Rust `sin` (F) / Go `Sin` / Python・JS `sin` / OCaml `sin` (F) | 保持 | — |
| `sqrt` | `math.sqrt(x: Float) -> Float` | Rust `sqrt` (F) / Go `Sqrt` / Gleam `float.square_root` (F) / OCaml `Float.sqrt` (F) | 保持 | R-math-03（R-math-01 的重新审议） |
| `tan` | `math.tan(x: Float) -> Float` | Rust `tan` (F) / Go `Tan` / Python・JS `tan` / OCaml `tan` (F) | 保持 | — |
| `tanh` | `math.tanh(x: Float) -> Float` | Rust `tanh` (F) / Go `Tanh` / Python・JS `tanh` / OCaml `tanh` (F) | 保持 | — |
| `asin` | （新增）`math.asin(x: Float) -> Float` | Rust `asin` (F) / Go `Asin` / Python・JS `asin` / OCaml `asin` (F) / Kotlin `asin` / Swift `asin` | 新增 | R-math-02 |
| `acos` | （新增）`math.acos(x: Float) -> Float` | Rust `acos` (F) / Go `Acos` / Python・JS `acos` / OCaml `acos` (F) / Kotlin `acos` / Swift `acos` | 新增 | R-math-02 |
| `atan2` | （新增）`math.atan2(y: Float, x: Float) -> Float` | Rust `y.atan2(x)` (F) / Go `Atan2` / Python・JS `atan2` / OCaml `atan2` (F) / Kotlin `atan2` / Swift `atan2` | 新增 | R-math-02 |
| `sinh` | （新增）`math.sinh(x: Float) -> Float` | Rust `sinh` (F) / Go `Sinh` / Python・JS `sinh` / OCaml `sinh` (F) | 新增 | R-math-02 |
| `cosh` | （新增）`math.cosh(x: Float) -> Float` | Rust `cosh` (F) / Go `Cosh` / Python・JS `cosh` / OCaml `cosh` (F) | 新增 | R-math-02 |
| `hypot` | （新增）`math.hypot(x: Float, y: Float) -> Float` | Rust `hypot` (F) / Go `Hypot` / Python・JS `hypot` / OCaml `hypot` (F) / Kotlin `hypot` | 新增 | R-math-02 |
| `cbrt` | （新增）`math.cbrt(x: Float) -> Float` | Rust `cbrt` (F) / Go `Cbrt` / Python・JS `cbrt` / OCaml `cbrt` (F) / Kotlin `cbrt` | 新增 | R-math-02 |
| `expm1` | （新增）`math.expm1(x: Float) -> Float` | Rust `exp_m1` / Go `Expm1` / Python・JS `expm1` | 新增 | R-math-04 |
| `log1p` | （新增）`math.log1p(x: Float) -> Float` | Rust `ln_1p` / Go `Log1p` / Python・JS `log1p` | 新增 | R-math-04 |
| `asinh` | （新增）`math.asinh(x: Float) -> Float` | Rust `asinh` / Go `Asinh` / Python・JS `asinh` | 新增 | R-math-05 |
| `acosh` | （新增）`math.acosh(x: Float) -> Float` | Rust `acosh` / Go `Acosh` / Python・JS `acosh` | 新增 | R-math-05 |
| `atanh` | （新增）`math.atanh(x: Float) -> Float` | Rust `atanh` / Go `Atanh` / Python・JS `atanh` | 新增 | R-math-05 |
