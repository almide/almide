<a id="als-t--text-and-number-semantics-normative"></a>

# ALS §T — 文本与数值语义（规范）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](text-and-numbers.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](text-and-numbers.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](text-and-numbers.zh-CN.md)

> [固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/text-and-numbers.md)的非规范读者快照。下文的规范地位和要求属于原文，不属于本副本。不自动同步；本次翻译未独立复核历史实现、符合性及安全主张。代码与规则标识符保持不变。

> Last updated: 2026-10-26

> **状态**：规范。这些章节是独立于实现的**规范**，v0（native）和 v1（MIR/wasm）都必须符合。符合性证据来自 `spec/wasm_cross/` 对应夹具（三项观测：stdout、stderr、exit code）。**消除 oracle 循环**（flight-evidence-gaps F1）：制定本章之前，这些行为的“正确性”就是 v0 实现自身；此后 v0 也只是本章的一种实现。

<a id="als-t1-stringtrim"></a>

## ALS-T1 `string.trim`

`string.trim(s)` 从 s 的开头和末尾移除具有 **Unicode `White_Space` 属性的码点**的最长连续序列。规范依据 Unicode White_Space（PropList.txt）；截至 2026 年为以下 25 个码点：U+0009–U+000D、U+0020、U+0085、U+00A0、U+1680、U+2000–U+200A、U+2028、U+2029、U+202F、U+205F、U+3000。

**裁定**：仅检查 ASCII 的快速判定（U+0009–U+000D、U+0020）不符合规范。Unicode 更新改变 White_Space 集合时，本章跟进，实现遵循本章。
夹具：`spec/wasm_cross/string_whitespace.almd`。

<a id="als-t2-floatparse"></a>

## ALS-T2 `float.parse`

接受文法（**仅有限值**；无论大小写或符号都不接受 `inf` / `infinity` / `nan`，C-413）：

```ebnf
float   := ws* sign? number ws*
number  := digits ("." digits?)? exponent? | "." digits exponent?
exponent:= ("e"|"E") sign? digits
ws      := U+0009 | U+000A | U+000C | U+000D | U+0020
```

`ws` **仅含五个 ASCII 字符**（C-413），不是 ALS-T1 的 Unicode White_Space 集合，也不含 U+000B（VT）；其余空白均违反文法。

**值规范**：对于接受的十进制表示，返回 **IEEE-754 binary64 最近偶数舍入（round-half-to-even）下精确的最近值**。不受位数或指数大小影响，包括最小 denormal 4.9e-324、最大值 1.7976931348623157e308、超过 19 位的有效数。舍入结果为 ±inf 的表示（溢出）是错误，下溢为 ±0，保留符号（`-0.0` 返回负零）。结果始终有限。

**错误措辞（规范）**：空输入为 `cannot parse float from empty string`；文法错误（包括非有限值写法）为 `invalid float literal`；溢出为 `float literal out of range`。退出码遵循普通错误传播。
夹具：`spec/wasm_cross/float_parse.almd`、`float_parse_finite_only.almd`、`number_parse_ascii_trim.almd`。

<a id="als-t3-jsonparse"></a>

## ALS-T3 `json.parse`

以下是已采纳 C-391、C-400、C-401、C-413 的解释性摘要。接受文法遵循 RFC 8259 及这些契约记录的 I-JSON 限制。

- 数值按 ALS-T2 的值规范读取。浮点溢出为 `err("float literal out of range")`（C-413），不返回 ±inf。
- 合并正确的代理项对；拒绝未配对代理项、无效转义、原始控制字符、非字符、未闭合字符串（C-400）。
- 在 JSON 空白（space / tab / LF / CR）之间接受**恰好一个值**。拒绝尾随数据、重复键、无引号键、尾随逗号、缺少分隔符、非 JSON 空白、无效数值（C-401）。
- 带位置的错误按字符定位（码点 index）。容器嵌套上限见 C-391。

旧 0.59.1 的前缀 parse／删除未配对代理项说明由 C-400/C-401 取代；溢出返回 ±inf 的说明由 C-413 取代。
测试：`spec/wasm_cross/json_parse_strict_strings.almd`、`spec/wasm_cross/json_parse_strict_structure.almd`、`spec/wasm_cross/json_parse_lenient_edges.almd`、`json_string_span.almd`。
Contracts: C-391, C-400, C-401, C-413。

<a id="als-t4-listchunk--listwindow"></a>

## ALS-T4 `list.chunk` / `list.window`

**裁定**：非正大小参数按以下规则处理：

- `n <= 0`：**定义域错误**。负大小按 ALS-T16 负数量规则读为 0（almide/almide#4147），所以 `n < 0` 与 `n == 0` 一样，按 ALS-T6 终止约定：`chunk` 向 stderr 输出一行 `Error: chunk size must be positive`，`window` 输出 `Error: window size must be positive`，以 exit code 1 停止。原始 Rust panic（exit 101）、wasm trap（exit 134）、静默返回空／全部窗口均不符合规范。

> 注：旧 `n < 0` 裁定（`chunk` 将全部作为一个块，`window` 返回空）将 v0 Rust 实现细节（`chunks(n as usize)` 的 usize 重解释）提升为规范；现已撤回，以符合 ALS-T16 负数量读为 0 的规则（#4147）。`spec/wasm_cross/list_chunk_negative.almd` 固定负大小 abort。`n == 0` 即使在旧重解释下也未定义（Rust panic），在 v0.28.4 规范为 T6 形式 abort；此前 native 原始 panic，而 wasm `window(xs, 0)` **静默返回 len+1 个空窗口**（silent-wrong）。
> 夹具：`spec/wasm_cross/list_chunk_windows.almd`（值案例）、`list_chunk_zero.almd` / `list_windows_zero.almd`（abort 案例）。Contracts: C-129。

<a id="als-t5-stringto_upper--stringto_lower"></a>

## ALS-T5 `string.to_upper` / `string.to_lower`

**基础规范为 Unicode full case mapping**（UnicodeData.txt 的简单映射 + SpecialCasing.txt 的 1:N 映射，如 ß→SS）。`to_lower` 应用 **Final_Sigma 上下文规则**（Unicode 3.13：词末 Σ→ς）。**不应用**依赖 locale 的规则（土耳其语 İ/ı 等），这是 locale 无关裁定。

**禁止折叠为 ASCII**（C-412）：ASCII 标量按 ASCII 规则映射；非 ASCII 标量按上述映射，但若映射结果含任何 ASCII 标量，就**保留原标量不变**。Unicode 映射本身包含 U+0131→I、U+212A→k、U+017F→S，因此这是有意偏离标准。当前 Unicode 下保留的标量：`to_upper` 为 U+00DF、U+0131、U+0149、U+017F、U+01F0、U+1E96–U+1E9A、U+FB00–U+FB06；`to_lower` 为 U+0130、U+212A。Final_Sigma 上下文基于原字符串判断（保留的 U+212A 也算 cased）。`capitalize` 的首字符遵循同一规则。此映射与 locale 无关，不用于标识符身份比较。
实现必须跟随 Unicode 版本更新。当前生成表来自 `scripts/gen-case-tables.py`；无论由哪种实现生成，符合性都针对本章及 `spec/wasm_cross/string_case_unicode.almd`、`string_case_no_ascii_fold.almd` 判定。

<a id="als-t6-整数演算の終了規約termination-convention"></a>

## ALS-T6 整数运算终止约定

整数 `/`、`%` **全域定义**：零除数向 stderr 输出一行 `Error: division by zero`；有符号最小值 ÷ −1（各位宽真正的 MIN）输出 `Error: integer overflow`；均以 **exit code 1 停止**。硬件 trap（wasm unreachable、exit 134 等）或静默回绕不符合规范。同一约定用于 `int.pow` 的负指数（`Error: negative exponent`）、列表下标越界（`Error: index out of bounds`）、`int.clamp`/`float.clamp` 无效范围（lo > hi，float 还包括 NaN 边界：`Error: clamp requires min <= max`）。
夹具：`spec/wasm_cross/int_div_by_zero*.almd`、`int_mod_*`、`int8_div_overflow.almd`、`int_pow_negative_exponent.almd`、`index_bounds.almd`。

调用栈耗尽也采用相同终止形式：native 与嵌入式 wasm 宿主（`almide run --target wasm`）向 stderr 输出一行 `Error: stack overflow`，以 **exit code 1 停止**，不丢失此前写出的 stdout。Rust 的 `thread 'main' has overflowed its stack`（SIGABRT、exit 134）或 `Error: wasm trap: call stack exhausted` 不符合。耗尽深度不统一，而作为各目标结构性资源由 C-196 声明：native 使用耗尽线程自身的栈（main 平台默认 8 MiB，`fan` 分支和并行工作线程为 Rust 默认 2 MiB）；嵌入式宿主为 8 MiB wasm 栈。stock 运行时（由普通 `wasmtime run` 执行的 p1 模块／组件、浏览器）的耗尽是宿主自身 trap（wasmtime 为 exit 134），属于本约定声明的残余范围（C-196）。
夹具：`spec/wasm_fail/stack_overflow_abort.almd`（abort）、`spec/wasm_cross/recursion_depth_within_limits.almd`（两目标限制内的递归逐字节相同）。

<a id="als-t7-トップレベル-let-の評価時機"></a>

## ALS-T7 顶层 let 的求值时机

模块顶层 `let` 初始化器**按声明顺序在程序启动时、main 前求值**。可能 abort 的初始化器（如包含 ALS-T6 运算），即使绑定从未使用，也在启动时 abort。初始化器可引用之前的顶层绑定（声明顺序依赖）。
夹具：`spec/wasm_cross/top_let_div_eager.almd`、`top_let_div_used.almd`。

<a id="als-t8-整数パースの文法とエラー規範"></a>

## ALS-T8 整数解析文法与错误规范

`int.parse(s)` 先移除首尾 **ASCII 空白**（U+0009、U+000A、U+000C、U+000D、U+0020，即 ALS-T2 的 `ws`；不含 VT、非 ASCII 空白，C-413），所以 `"\u{00A0}99\u{3000}"` 得到 `invalid digit found in string`，然后将剩余内容读作 `sign? digit+`（十进制，无下划线）。错误消息与 Rust `ParseIntError` Display 逐字节一致：移除后为空为 `cannot parse integer from empty string`；无效字符为 `invalid digit found in string`；越界为 `number too large to fit in target type` / `number too small to fit in target type`。

```almide
test "int.parse trims ASCII whitespace only" {
  assert_eq(int.parse("\u{00A0}99\u{3000}") ?? -1, -1)
  assert_eq(int.parse("\t 99\r\n") ?? -1, 99)
}
```

夹具：`spec/wasm_cross/string_whitespace.almd`、`spec/wasm_cross/number_parse_ascii_trim.almd`。
Contracts: C-028, C-029。

<a id="als-t9-固定小数表示"></a>

## ALS-T9 定点小数显示

`float.to_fixed(x, n)` 对**精确二进制值执行 round-half-to-even**（银行家舍入）。经十进制字符串再次舍入或 half-up 不符合。n=0 不带小数点，负值、边界值（如恰好 0.5）也用相同规则。n 定义域为 **0..=4096**，域外按 T6 形式停止（`Error: to_fixed requires decimals in 0..=4096`，exit 1）。非有限值原样返回显示形式：`inf`、`-inf`、`NaN`。保留零量符号（`to_fixed(-0.4, 0)` 为 `-0`）。
夹具：`spec/wasm_cross/to_fixed_domain_abort.almd`、`spec/wasm_cross/to_fixed_wide_precision.almd`。
Contracts: C-025。

<a id="als-t10-数学関数の決定性"></a>

## ALS-T10 数学函数确定性

`math.sin/cos/tan/exp/log/pow` 等超越函数**在全部目标逐字节一致**（两目标共享 vendored libm，依赖宿主 libm 不符合）。距真值的距离（误差上限）由 ALS-T22 规定。
Contracts: C-026。

<a id="als-t11-バイナリテキスト符号化"></a>

## ALS-T11 二进制文本编码

`base64.encode/decode`（standard + URL-safe）和 `hex.encode/decode` 遵循 RFC 4648，decode 错误在两目标措辞相同（base64 无位置；带位置的是 hex 侧 `invalid hex char at 0`，C-030），包括大小写、填充规则、无效长度检测。
base64 encode 输出带 `=` 的规范形式；`base64.decode`（`+` `/`）和 `base64.decode_url`（`-` `_`）**只接受自身字母表的规范形式**。每个字节序列只有一种写法，不具有 RFC 4648 §12 的可塑性。按以下顺序检查，返回首个违反规则的 err：

| 规则 | err |
|------|-----|
| UTF-8 字节长度是 4 的倍数 | `invalid base64 length: <バイト長>`（字节长度） |
| 末尾 `=` 至多两个 | `invalid base64 padding` |
| 其余字节均属于该函数字母表（中途 `=`、另一字母表第 62/63 位均不可） | `invalid base64 character` |
| 被 `=` 丢弃的位为 0（一个 `=` 为最后字符低 2 位，两个为低 4 位） | `invalid base64 trailing bits` |

`QQ==` 为 `A`，`QR==` 为 `invalid base64 trailing bits`，`QQ` 为 `invalid base64 length: 2`，`QUJD====` 为 `invalid base64 padding`，`decode("-_8=")`、`decode_url("+/8=")` 为 `invalid base64 character`。
测试：`spec/wasm_cross/encoding_base64.almd`、`spec/wasm_cross/base64_decode_canonical.almd`、`spec/stdlib/encoding_test.almd`。
Contracts: C-027, C-030, C-411。

<a id="als-t12-非-abort-整数除算の一致"></a>

## ALS-T12 不 abort 的整数除法一致性

不导致 abort 的 `/`、`%` 与 Rust `i64` 截断除法／余数逐字节一致，包括负数舍入方向及余数符号：`-7 / 2 == -3`、`-7 % 2 == -1`。
Contracts: C-003。

<a id="als-t13-浮動小数の文字列化"></a>

## ALS-T13 浮点数字符串化

`float.to_string` 使用**最短往返十进制表示**（与 Dragon4/Ryū 等价）：满足 `parse(to_string(x)) == x` 且位数最少。整数值保留 `.0`，区别于 ALS-R2 的 Display。
Contracts: C-023。

<a id="als-t14-64-bit-レジスタと-wrap-のマスク飽和"></a>

## ALS-T14 64 bit 寄存器与 wrap 掩码饱和

`int.wrap_add` / `wrap_sub` / `wrap_mul` / `rotate_left` / `rotate_right` 不带位宽参数，以 64 bit `Int` 自身为寄存器回绕／旋转（R-int-12）。旋转次数按模 64 欧几里得缩减，负次数反向旋转。N bit 寄存器与 `int.wrap(x, bits)` 组合（`int.wrap(int.wrap_add(a, b), bits)`）。`int.wrap` 的 `bits ≥ 64` 使掩码**饱和**为 `u64::MAX`，原样返回 `x`，不是取模。
Contracts: C-048。

<a id="als-t15-符号と-minmax-の-nan-規則"></a>

## ALS-T15 符号与 min/max 的 NaN 规则

`float.sign` 为 `f64::signum`（NaN → NaN、±0 → ±1）。`float.min/max` **忽略 NaN**（一侧为 NaN 时返回另一侧，IEEE-754 minNum/maxNum 系）。`float.round` 保留零结果的符号（round(-0.0) = -0.0，half away from zero 不变）。±0 的 min/max 顺序见 ALS-T23（IEEE-754-2019 minimum/maximum）。
Contracts: C-049, C-140。

<a id="als-t16-個数添字の-i64-クランプ"></a>

## ALS-T16 数量和索引的 i64 钳制

接受 List / String **数量或索引**的 API，在**缩窄到内部位宽之前**，先在完整 i64 上钳制。因回绕或符号改变而错误访问不符合规范。钳制方向由 API 类型决定。**负数量读为 0**（almide/almide#4147；旧无符号重解释使 `take(xs, -1)` 返回全部，导致 `?limit=-1` 取消数量上限）。夹具 `spec/wasm_cross/negative_count_rule.almd`、`string_count_truncation.almd`、`list_count_index_truncation.almd` 固定以下规则：

| 类型 | 规则 | 负值／巨大值的结果 |
|------|------|--------------------|
| 数量（`list.take/drop/take_end/drop_end/split_at`、`string.take/drop/take_end/drop_end/split_at`、`bytes.take/drop/take_end/drop_end/split_at`、`pad_start/pad_end` 宽度）及范围端点（`list.slice`、`bytes.slice`、`matrix.slice_rows` 的 start/end） | **有符号** `max(n, 0).min(len)` | 负值为 0：`take(-1)` = 空，`drop(-1)` = 全部，`slice(xs, -2, 3)` = `slice(xs, 0, 3)`。`2^32` 及以上饱和到 `len`，不回绕为小值 |
| 大小（`list.chunk/window`、`string.chunk/window`、`bytes.chunk/window`） | 定义域从 1 开始（ALS-T4） | 0 及以下，包括负值，均 abort |
| 重复次数（`string.repeat` / `list.repeat` / `bytes.repeat`） | **有符号**，负为 0 | `repeat(s, -1)` = 空，两目标相同；panic / trap 不符合 |
| `string.slice` 的 start/end | **有符号** `max(0).min(len)` | 负 start 为 0 |
| 索引（`list.get/get_or/set/insert/remove_at/swap/update`） | **无符号** `i as usize` | 负索引或 `2^32` 及以上走越界 no-op / default / append 路径 |

`list.product` 与 `list.sum` 一样进行 i64 回绕（溢出模 2^64）。

```almide
test "a negative count is 0: take is empty, drop keeps everything, repeat is empty" {
  assert_eq(string.take("abcde", -1), "")
  assert_eq(string.drop("abcde", -1), "abcde")
  assert_eq(string.repeat("xy", -1), "")
  assert_eq(list.len(list.take([1, 2, 3], -1)), 0)
  assert_eq(list.slice([1, 2, 3], -2, 2), [1, 2])
}
```

夹具：`spec/wasm_cross/negative_count_rule.almd`、`spec/wasm_cross/string_count_truncation.almd`、`spec/wasm_cross/list_count_index_truncation.almd`。Contracts: C-054, C-056。

<a id="als-t17-datetimeformat-の指定子置換"></a>

## ALS-T17 datetime.format 的格式符替换

`datetime.format(ts, pattern)` 将 strftime 类格式符 `%Y %m %d %H %M %S` **逐次替换**为补零的日历字段（年 4 位，其余 2 位）。native / v0-wasm / 自托管三个后端运行同一逐次 `string.replace` 序列，输出逐字节一致。`%` 只在这些格式符前有特殊含义，不存在 `%%` 转义（未识别 `%X` 原样通过）。`%Y` 与 `to_iso` 一样为**可变宽度**：至少 4 位、补零、负年符号占一位（`-292277022657`、`0000`）。固定四位字段曾在年份溢出时只写低四位，负年份时写出非 ASCII 字节。Contracts: C-128、C-359。

<a id="als-t18-assert-の-abort-形非-test-位置"></a>

## ALS-T18 非 test 位置的 assert abort 形式

`test` 块之外的 `assert` 族失败，以 **stderr 结构化块 + exit code 1** 停止（T6 终止约定族）。原始 Rust panic（exit 101）、wasm trap（exit 134）、缺少值信息的输出不符合。块由每行一个 `  key: value` 构成；值采用 ALS-R2 Display，加上 C-153/C-418 控制字符及不可见字符转义，不直接嵌入原始换行：

- `assert_eq(l, r)` → `Error: assertion failed\n  at: line <N>\n  expected: <r>\n  found: <l>`
- `assert_ne(l, r)` → `Error: assertion failed\n  at: line <N>\n  expected: != <l>\n  found: <l>`
- `assert(c)` → `Error: assertion failed\n  at: line <N>`
- `assert(c, msg)` → `Error: assertion failed: <msg>\n  at: line <N>`

字段顺序固定，**没有终止符的 `found` 始终最后**。值内换行显示为 `\n` 等转义（C-153/C-418）。只有调用缺 span 时省略 `at:` 行；这是共享 frontend lowering 的性质，因此目标间始终一致。
操作数**仅求值一次**，失败消息再次引用已绑定临时值。`test` 块内遵循测试框架报告形式，不在本节范围。实现通过 frontend lowering 一次脱糖，native / v0-wasm / v1-wasm / interp 都继承同一 IR。
夹具：`spec/wasm_cross/assert_abort_eq.almd`、`assert_abort_ne.almd`、`assert_abort_msg.almd`、`assert_abort_multiline.almd`。Contracts: C-153。

`testing` 模块的 assert，在 `test` 块外直接调用时也以同一块（标题 `Error: assertion failed`，顺序 `at:` → `expected:` → `found:`）及 exit code 1 停止。`expected:` 写关系，`found:` 写操作数：

- `testing.assert_gt(a, b)` → `expected: > <b>`、`found: <a>`
- `testing.assert_lt(a, b)` → `expected: < <b>`、`found: <a>`
- `testing.assert_approx(a, b, t)` → `expected: within <t> of <b>`、`found: <a>`（仅 |a − b| < t 时通过，NaN 不通过）
- `testing.assert_contains(h, n)` → `expected: contains <n>`、`found: <h>`（空 `n` 通过）
- `testing.assert_some(o)` → `expected: some(_)`、`found: none`
- `testing.assert_none(o)` → `expected: none`、`found: <o>`
- `testing.assert_ok(r)` → `expected: ok(_)`、`found: <r>`
- `testing.assert_err(r)` → `expected: err(_)`、`found: <r>`

以函数值传递的调用不是直接调用，不在本节范围。
夹具：`spec/wasm_cross/testing_assert_abort_gt.almd`、`_lt`、`_approx`、`_contains`、`_some`、`_none`、`_ok`、`_err`。Contracts: C-381。

<a id="als-t19-数値決定性ファミリー"></a>

## ALS-T19 数值确定性族

Float 计算的可观测结果（stdout、stderr、exit code，以及 `float.to_bits` 值）**只由程序及输入决定**，不依赖目标、宿主 CPU、OS、构建标志、执行环境浮点模式。这一句是数值确定性规范。组成部分包括已有 T2（`float.parse` 精确舍入）、T9（`to_fixed`）、T10（超越函数单一实现）、T13（最短往返显示）、T15（符号及忽略 NaN）、ALS-C9（totalOrder）、ALS-E3（`-0.0` 显示）、ALS-C3（相等）、ALS-R4（非有限常量显示）、C-210（NaN 规范观测）；本族新设 T20（舍入及禁止缩约）、T21（保留次正规数）、T22（超越函数误差上限）、T23（有符号零）、T24（Float → Int 转换）。

**NaN 位观测**在 C-210 边界规范化。`float.to_bits` 返回 `0x7FF8000000000000`，bytes 的 f64 / f32 writer 分别写 `0x7FF8000000000000` / `0x7FC00000`。这不改变 NaN 相等规则（ALS-C3）。显示、编码遵循各节规则。

> 实现注记（非规范）：native `Set[Float]` 使用 `runtime/rs/src/set.rs` 的 PartialEq 比较。仅凭 C-210 位规范化不能推出键身份保证。相等性见 ALS-C3 / C-292。

```almide
test "a Float result is a function of the program alone" {
  let x = float.parse("0.1") ?? 0.0
  assert_eq(float.to_bits(x + 0.2), 4599075939470750516)
  assert_eq(float.to_string(x + 0.2), "0.30000000000000004")
}
```

本族夹具（`spec/wasm_cross/float_no_contraction.almd`、`float_subnormal_preserved.almd`、`math_transcendental_bits.almd`、`float_signed_zero_minmax.almd`、`float_to_int_edges.almd`）均同时注明 C-302。Contracts: C-302。

<a id="als-t20-丸めと縮約の禁止"></a>

## ALS-T20 舍入与禁止缩约

Float 每次运算（`+` `-` `*` `/`、`math.sqrt`、类型转换）都按 IEEE-754 binary64 **最近偶数舍入**，**逐次运算舍入**。其他舍入模式不可观测。**缩约不符合规范**：`a * b + c` 是乘法及加法两次舍入，不得替换为融合乘加 FMA 的一次舍入；wasm 无融合指令，native 后端也不得生成。语言不存在融合运算；如有需要，应另节将其规范为具名函数，而非隐式模式。
例：`a = 1 + 2^-52`、`b = 1 - 2^-53` 时，`a * b` 精确值为 `1 + 2^-53 - 2^-105`，binary64 舍入为 `1.0`，故 `a * b - 1.0 = 0.0`。若缩约，会留下 `2^-53 - 2^-105 ≈ 1.1e-16`，也破坏两目标一致性。

```almide
test "a*b+c rounds twice — no fused multiply-add" {
  let a = float.parse("1.0000000000000002") ?? 0.0
  let b = float.parse("0.9999999999999999") ?? 0.0
  assert_eq(a * b - 1.0, 0.0)
  assert_eq(a * b + (0.0 - 1.0), 0.0)
}
```

夹具：`spec/wasm_cross/float_no_contraction.almd`、`spec/stdlib/float_determinism_test.almd`。Contracts: C-303。

<a id="als-t21-非正規数の保存"></a>

## ALS-T21 保留次正规数

binary64 次正规数（`|x| < 2^-1022`，最小 `2^-1074 ≈ 4.9e-324`）在任意目标、宿主上都**作为值保留**。输入或输出侧的 flush-to-zero、denormals-are-zero 均不符合：`2.2250738585072014e-308 / 2.0` 是 `2^-1023` 而非 0，`float.to_bits` 返回 `2251799813685248`（= `2^51`）。parse（T2）、显示（T13）、算术、`to_bits` 边界都不丢弃次正规数。

```almide
test "subnormals survive arithmetic and observation" {
  let tiny = float.parse("2.2250738585072014e-308") ?? 0.0
  assert_eq(float.to_bits(tiny / 2.0), 2251799813685248)
  assert(tiny / 2.0 != 0.0)
  assert_eq(float.to_bits(float.parse("5e-324") ?? 0.0), 1)
}
```

夹具：`spec/wasm_cross/float_subnormal_preserved.almd`、`spec/stdlib/float_determinism_test.almd`。Contracts: C-304。

<a id="als-t22-超越関数の誤差上限"></a>

## ALS-T22 超越函数误差上限

T10 规定全部目标逐字节一致（单一 vendored 实现，不用宿主 libm）；本节声明**距真值的距离**，按“与精确舍入结果的 ulp 差”测量，同符号 binary64 为 `float.to_bits` 差的绝对值。

| 函数 | 上限 |
|------|------|
| `math.sqrt` | 精确舍入（0 ulp），IEEE-754 要求 |
| `math.exp` `math.log` `math.log2` `math.log10` | ≤ 1 ulp |
| `math.sin` `math.cos` `math.tan` | ≤ 1 ulp |
| `float.pow` / Float `**` | ≤ 1 ulp |
| `math.log_gamma` | 未声明；仅逐字节一致（T10）为规范，不主张精度 |

不主张表外函数精度。上限在样本点执行验证（下方 test 和 `spec/stdlib/math_accuracy_test.almd`；参考值按 70 位十进制计算，再精确舍入为 binary64）。

```almide
fn ulp_from(x: Float, reference_bits: Int) -> Int = int.abs(float.to_bits(x) - reference_bits)

test "sqrt is correctly rounded; exp/log/sin stay within 1 ulp at sample points" {
  assert_eq(ulp_from(math.sqrt(2.0), 4609047870845172685), 0)
  assert(ulp_from(math.exp(1.0), 4613303445314885481) <= 1)
  assert(ulp_from(math.log(2.0), 4604418534313441775) <= 1)
  assert(ulp_from(math.sin(1.0), 4605754516372524270) <= 1)
}
```

夹具：`spec/wasm_cross/math_transcendental_bits.almd`、`spec/stdlib/math_accuracy_test.almd`。Contracts: C-305。

<a id="als-t23-符号付きゼロ"></a>

## ALS-T23 有符号零

零的符号按 IEEE-754 传播：`-0.0 * 1.0 = -0.0`、`-0.0 + 0.0 = 0.0`（最近偶数舍入下异号零之和为 `+0`，即使 `+0.0` 为字面量也一样；`x + 0.0 → x` 对 `x = -0.0` 不成立，不得折叠；`x - 0.0 → x`、`x * 1.0 → x` 则成立）、`1.0 / -inf = -0.0`。相等忽略符号（`0.0 == -0.0` 为真，ALS-C3），显示保留符号（`-0.0` → `-0.0`，ALS-E3）。`float.min/max` 遵循 **IEEE-754-2019 `minimum`/`maximum` 的零顺序**：视为 `-0.0 < +0.0`，`min(-0.0, 0.0) = min(0.0, -0.0) = -0.0`、`max(-0.0, 0.0) = max(0.0, -0.0) = +0.0`。不依赖参数顺序（可交换），与 ALS-C9 totalOrder 一致。NaN 按 T15 忽略：一侧 NaN 返回另一侧，两侧 NaN 返回 NaN。

**裁定（2026-08-21，ADR-0016）**：旧“±0 相等时返回第一个参数”规则（C-049 旧文）撤回，由本节取代。

```almide
fn neg_zero() -> Float = (0.0 - float.abs(-1.0)) * 0.0

test "signed zero propagates, compares equal, and orders -0 < +0 in min/max" {
  let mz = neg_zero()
  assert_eq(float.to_string(mz), "-0.0")
  assert(mz == 0.0)
  assert_eq(float.to_string(mz + 0.0), "0.0")
  assert_eq(float.sign(float.min(mz, 0.0)), -1.0)
  assert_eq(float.sign(float.min(0.0, mz)), -1.0)
  assert_eq(float.sign(float.max(mz, 0.0)), 1.0)
  assert_eq(float.sign(float.max(0.0, mz)), 1.0)
  assert_eq(float.sign(float.min(0.0, mz)), -1.0)
  assert_eq(float.sign(float.max(mz, 0.0)), 1.0)
}
```

夹具：`spec/wasm_cross/float_signed_zero_minmax.almd`、`spec/wasm_cross/float_sign_minmax_ieee.almd`、`spec/stdlib/float_determinism_test.almd`（传播、相等）、`spec/stdlib/float_minmax_zero_order_test.almd`（±0 顺序；截至 0.59.1 仍为旧规则，pin 前进前该文件为红）、`spec/stdlib/float_signed_zero_sum_test.almd`（与字面量 `0.0` 相加；该文件失败发现 0.58.0 native 的 `x + 0.0 -> x` 折叠，0.59.1 已改为仅保留符号精确恒等式，当前为绿）。
Contracts: C-306。

<a id="als-t24-float--int-変換"></a>

## ALS-T24 Float → Int 转换

`int.from_saturating(x)` **向零截断**，越界时**饱和到 i64 两端**，NaN 为 `0`：`from_saturating(2.7) = 2`、`from_saturating(-2.7) = -2`、`from_saturating(+inf)` 为 `9223372036854775807`、`from_saturating(-inf)` 为 `-9223372036854775808`、`from_saturating(9.3e18)` 为 `9223372036854775807`（饱和）、`from_saturating(NaN) = 0`、`from_saturating(-0.0) = 0`。
checked 族（`int8.from_checked` … `uint64.from_checked`、`float32.from_checked`）**仅在值能由目标类型精确表示时返回 `some`**：有小数部分、越界、NaN、±inf 返回 `none`（`int64.from_checked(2.7) = none`、`int64.from_checked(-0.0) = some(0)`、`float32.from_checked(0.1) = none`）。`float.floor/ceil/round` 是 Float → Float，不在本节范围（round 的 ±0 见 T15）。

```almide
test "int.from_saturating truncates toward zero, saturates, and maps NaN to 0" {
  let inf = 1.0 / 0.0
  assert_eq(int.from_saturating(2.7), 2)
  assert_eq(int.from_saturating(-2.7), -2)
  assert_eq(int.from_saturating(inf), 9223372036854775807)
  assert_eq(int.from_saturating(0.0 - inf), -9223372036854775808)
  assert_eq(int.from_saturating(0.0 / 0.0), 0)
  assert_eq(int64.from_checked(2.7), none)
  assert_eq(int64.from_checked(-0.0), some(0))
}
```

夹具：`spec/wasm_cross/float_to_int_edges.almd`、`spec/stdlib/float_determinism_test.almd`。Contracts: C-307。

<a id="als-t25-matrix-の超越計算と積の順序"></a>

## ALS-T25 matrix 超越计算与乘积顺序

`matrix` 的逐元素超越计算（如 `matrix.map(m, (x) => float.pow(x, e))` 中到达 `float.pow` 等的计算）使用**随附 musl libm**（T22），而非宿主 libm。两目标输出相同字节，同一程序在任何平台构建都输出相同字节（C-223）。乘积（`a * b`）在所有路径都按 native 标量循环计算：从 0.0 累积，k 升序，先乘再加，不使用融合乘加。曾经过规范 fast-exp 的激活函数（`softmax_rows`、`gelu`、`swiglu_gate`）及 rope 逆频率表，在 dialect epoch 14 离开标准库，移至 almide-graphics/nn 包的 nn.kernels 模块（C-270）。
夹具：`spec/wasm_cross/matrix_pow_libm.almd`。Contracts: C-223。

<a id="als-t26-datetime-の暦フィールドと-parse_iso-の文法"></a>

## ALS-T26 datetime 日历字段与 parse_iso 文法

从 Unix 秒 `ts` 提取日历字段的 `datetime.year` / `month` / `day` / `weekday`，及经它们处理的 `to_iso` / `format`，以**向下整除**计算日编号；即使 `ts` 为负且不在日边界，也返回该秒所属的日：`to_iso(-1)` 为 `1969-12-31T23:59:59Z`，`weekday(-1)` 为 `Wednesday`。时间字段（`hour` / `minute` / `second`）仍用向下取余，因此两者指同一天。`from_parts` 在域内为逆函数（`from_parts(1969, 12, 31, 23, 59, 59)` 为 `ok(-1)`）。

`datetime.from_parts(y, mo, d, h, mi, s)` 返回 `Result[Int, String]`，用**与 `parse_iso` 相同的域和措辞**将越界字段变为 err，按年→月→日→时→分→秒顺序取首个越界字段（almide/almide#4149）。不进位：`from_parts(2026, 2, 30, 0, 0, 0)` 为 `err("day out of range: 30")`，不是 2026-03-02。两个构造器不会把同一输入解释为不同日期。
夹具：`spec/wasm_cross/datetime_from_parts_strict.almd`。

`datetime.parse_iso(s)` 去掉前后空白，读取以下文法：

```ebnf
datetime := date "T" time zone?
date     := digit+ "-" digit+ "-" digit+
time     := digit+ ":" digit+ ":" digit+
zone     := "Z" | ("+" | "-") digit+ ":" digit+
```

各字段只允许 ASCII 数字，宽度任意（接受 `2024-1-5T1:2:3`，不允许符号、空白、字母）。`Z` 与数值偏移互斥。偏移会**应用**，结果表示 UTC 瞬间：`2024-01-15T10:30:00+09:00` 为 1705282200。范围为年 0..=9999（RFC 3339 §5.6 的 `date-fullyear = 4DIGIT`；越界年份返回 err，不是回绕瞬间的 ok），月 1..=12，日 1..=当月天数（预推格里高利历闰年规则），时 0..=23，分／秒 0..=59（无闰秒），偏移时 0..=23、分 0..=59。

err 字符串如下，首个违规字段优先：

| 输入 | err |
|------|-----|
| 年／月／日／时／分／秒越界 | `<field> out of range: <value>`（`year` / `month` / `day` / `hour` / `minute` / `second`） |
| 偏移无效或越界 | `invalid offset: <符号以降の文字列>`（从符号起的字符串） |
| 其他无效写法 | `invalid datetime format` |
| 缺少 `T` | `expected YYYY-MM-DDTHH:MM:SSZ` |

测试：`spec/wasm_cross/datetime_pre_epoch.almd`、`spec/wasm_cross/datetime_parse_iso_strict.almd`、`spec/wasm_cross/datetime_parse_iso_year_range.almd`、`spec/stdlib/datetime_test.almd`。Contracts: C-359、C-360、C-410。

<a id="als-t27-urlparse-の-authority-規範"></a>

## ALS-T27 url.parse 的 authority 规范

`url.parse(s)` 读取 `scheme://host[:port][/path][?query][#fragment]`，检查的是 **host**，不是 authority。host 为空时返回 `url.parse: empty host`；`http://:8` 这样的仅 port 写法也属于空 host。port 仅允许 ASCII 数字，范围 0..=65535；非数字（包括 `+80`、`-0`、` 80`）返回 `url.parse: invalid port: <text>`，越界返回 `url.parse: port out of range: <text>`。两种已记录为范围外的形式，不作为 host 读取而点名拒绝：userinfo 返回 `url.parse: userinfo is not supported: <authority>`，IPv6 字面量返回 `url.parse: IPv6 hosts are not supported: <authority>`。host 包含 RFC 3986 reg-name 集合（unreserved / sub-delims / `%`）外的字符时，返回 `url.parse: invalid host: <host>`。接受的 URL 经 `to_string` 往返。
测试：`spec/wasm_cross/url_authority_edges.almd`、`spec/stdlib/url_test.almd`。Contracts: C-361。

<a id="als-t28-float32-演算の-binary32-丸め"></a>

## ALS-T28 Float32 运算的 binary32 舍入

`Float32` 各次运算（`+` `-` `*` `/` `%` `**`、单目 `-`）结果都按 IEEE-754 **binary32** 最近偶数规则，**每次运算后**舍入为 f32，与 native `f32` 运算同值。即使用更宽 f64 槽承载 `Float32`，也必须在每次运算后立即舍入为 binary32；否则经 `float64.from` 拓宽、比较、后续运算都会与 native 不同。转为 `Float32`（`float32.from(x)`，x 为 `Float`、`Int` 或定宽整数）舍入一次。比较（`==` `!=` `<` `<=` `>` `>=`）比较已舍入 binary32 值。`Float32` 上下文中的裸 float 字面量创建时就缩窄为 binary32（ALS-E3）。`math.*` 仅接受 `Float`，`Float32` 必须显式拓宽（`float.from(x)`）后传入。`Float32` 的 `a ** b` 将两侧拓宽后计算 `Float` 的 `**`（ALS-T10 的单一 pow 实现），再一次舍入到 binary32。
例：`1/3` 拓宽为 `0.3333333432674408`，`2^24 + 1` 舍入为 `2^24`，将 `0.1` 加十次得到 `1.0000001192092896`。
测试：`spec/wasm_cross/float32_arithmetic_rounds.almd`。Contracts: C-371。

<a id="als-t29-float32-の表示"></a>

## ALS-T29 Float32 显示

`Float32` 用**能往返为同一 binary32 的最短十进制表示**显示（Rust `f32` Display，位置记法，无指数）。`0.1` 就是 `0.1`，不是拓宽 f64 的 `0.10000000149011612`。若两个最短候选恰好等距，则向更大绝对值舍入（`2^-12` 为 `0.00024414063`）。字符串插值和容器内部（列表、记录、Option、元组、variant 载荷）去掉整数值 `.0`（ALS-R2 / C-011），`float32.to_string` 保留。保留 `-0` 的符号，非有限值为 `inf` / `-inf` / `NaN`。
测试：`spec/wasm_cross/float32_display_shortest.almd`。Contracts: C-372。

<a id="als-t30-testingassert_aborts--子で走る-abort-表明"></a>

## ALS-T30 testing.assert_aborts — 在子执行环境中断言 abort

`testing.assert_aborts(s, f)` 在 `test` 块内，仅将 **`f`** 放入子执行环境运行。native 将同一测试二进制作为新子进程启动，wasm 创建同一模块的新实例。子环境从程序初始状态开始：顶层 `let`、`var` 为初始值，看不到任何父状态；cwd、环境变量、argv 与测试相同。`f` 可以是 effect closure，其值被丢弃。

`f` 必须是现场写出的 lambda，不能捕获测试局部值（`let`、`var`、`where` 绑定、外层 lambda 参数）。捕获及非 lambda 字面量的函数值在检查时为 E098。只可引用顶层 `fn`、`let`、`var` 及 import 模块。在 `test` 块外（`fn` 或 `main`）调用、作为函数值使用、在另一个 `assert_aborts` closure 中调用，均为检查时 E099。

子环境中 `f` **按程序主体**进行降低处理。`assert` 族、`testing.assert_*` 按 ALS-T18 块形式停止，`panic` 以 C-219 的 `PANIC: <msg>` 停止。

只有子环境以**已定义 abort**结束，即 exit status 为 1 且 stderr 存在以 `Error: ` 或 `PANIC: ` 开头的行（abort 标题），才计作 abort。**abort 文本**从最后一行标题到 stderr 末尾。`assert_aborts` 仅在该文本包含 `s` 子串时通过，逐字节、区分大小写；此前 `eprintln` 不用于匹配。`panic`、下标越界、零除、栈溢出、失败 assert 均为 abort。

正常返回、其他 exit status（`process.exit(n)` 且 n ≠ 1、原始 panic、trap）、无标题的 exit 1 均不通过。失败按 ALS-T18 字段顺序报告：`Error: assertion failed`、`at: line <N>`、`expected: abort containing "<s>"`、`found: <理由>`。`<理由>`（原因）为 `returned normally`、`abort "<abort テキスト>"`（abort 文本）、`exit code <n>`、`exit code 101 (raw panic)`、`trap (exit 134)` 之一。若子环境有输出，其后附 `child stdout:`、`child stderr:` 章节。子输出仅出现在失败报告，通过时不转发。
每次调用后测试继续，同一测试中多个调用各有自己的子环境。
测试：`spec/stdlib/coverage_assert_aborts_test.almd`、`spec/stdlib/testing_polarity_test.almd`、`spec/lang/assert_aborts_continues_test.almd`。Contracts: C-384。

<a id="als-t31-hmac-と定数時間比較"></a>

## ALS-T31 HMAC 与恒定时间比较

`hash.hmac_sha256(key, b)`、`hash.hmac_sha512(key, b)` 是基于 SHA-256、SHA-512 的 HMAC（RFC 2104），**密钥是首个参数**。密钥为 `Secret[Bytes]`，结果 tag 也为 `Secret[Bytes]`。超过块长（64、128 字节）的密钥先哈希，较短密钥补零至块长。取出的 tag 为 32、64 字节，两种摘要都匹配 RFC 4231 测试 1–7。tag 显示为 `Secret(***)`，`==` 为恒定时间比较（ALS-C3、C-426）。没有 `_hex` 单元，tag 的十六进制写作 `hex.encode(secret.reveal(tag))`。`hash.is_valid_hmac_sha256(key, b, tag)` / `is_valid_hmac_sha512` 仅在 `Secret[Bytes]` 的 `tag` 取出值等于计算 MAC 时返回 true，通过 `bytes.is_equal_constant_time` 比较。长度不同为 false。收到十六进制 tag 时先 `hex.decode` 再传入，大小写十六进制均接受。`secret.new(hex.decode(sig) ?? bytes.from_list([]))` 将奇数长度、非十六进制字符、空串变为空 tag，验证不 abort 而返回 false。

`bytes.is_equal_constant_time(a, b)` / `string.is_equal_constant_time(a, b)` 仅在长度和字节序列都相等时返回 true，String 比较 UTF-8 字节。长度不同立即 false，长度是公开信息。等长时读取所有字节对，扫描完才将全部 `a[i] xor b[i]` 的 OR 与 0 比较一次，不存在依赖内容的分支、索引、退出。
测试：`spec/wasm_cross/hash_hmac_rfc4231.almd`、`spec/wasm_cross/bytes_is_equal_constant_time.almd`。Contracts: C-422, C-423。

> 原文注意点：T23 带日期的 fixture 红／绿状态和旧实现为历史描述，不是当前测试报告。T13 的一般往返公式按原文保留，但 T2 只接受有限输入，NaN 也不等于自身。T24 将整数 checked 转换与 float32.from_checked 并列，随后笼统称带小数部分的值返回 none；这里保留措辞，不自行整合为新转换规则。T22 声明上限及样本点验证，不表示这里独立完成了全域证明。T5 的禁止 ASCII 折叠例外仍是对基础 Unicode 映射的明确有意偏离。T31 恒定时间主张保留等长范围，不主张长度保密或整个程序的计时安全。
