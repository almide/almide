<a id="the-float-printer-measured-then-chosen-1985"></a>

# 浮点打印器：先测量，再选择 (#1985)

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](float-printer.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](float-printer.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](float-printer.zh-CN.md)

> 非规范、固定原文提交的历史阅读快照；原文具有权威性。不自动同步。测量、实现状态、精确性证据及主张未在此独立重跑或验证。命令作为历史资料保留，并非已执行的指令。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/benchmarks/float-printer.md)

> 原文注记：最初选择无表方案以及“最小的精确打印器”标题描述第一轮测量，并非永久的全局最优。两篇带日期补记随后替换了保留的 Dragon4 定点路径及运行时推导表；各阶段及不同基线均予保留。早期与后期大小、分配台账、计时样本、临时空间成本及被否决的紧凑表编码不可混为一谈。样本相等不是对所有 float 的穷尽证明：原文本身记录了 float 扫描漏检、专门表测试发现的表损坏。对其他打印器的广泛比较及输出不可能改变的说法仍是原文在其所述论据与条件下的主张，不添加当前普遍保证。原文开头称定点记法，后文示例却使用指数记法；保留这一措辞，不予重新解释。不主张已删除原型或历史命令现在仍可运行。

`println("${1.5}")` 占用 7,783 bytes 的 wasm，而 Hello, world 为 1,337 bytes——仅 `float.to_string` 就要 6.4 KB，是任何 Almide 程序可能承担的最大固定开销。打印器是 Dragon4 的移植：精确大整数运算、五个各含 128 limb 的大整数，以及逐位执行大整数乘法/比较/减法的循环。其输出——Rust 的 `format!("{}", x)`（最短往返、定点记法）加上整数后缀 `.0`——是一项跨目标契约 (C-023)，不能改变。Issue #1985 要的是测量，而非文献答案：用 Almide 构建替代方案，在结构化路径上测量输出字节数，然后交付更小的精确打印器，或将 Dragon4 记录为下限。

这就是该测量。日期：2026-09-08，develop 位于 `8517914d8`（almide 0.62.0），结构化路径，`almide build --target wasm`（交付的 WASI 形式，无 wasm-opt）。

<a id="result"></a>

## 结果

| 方案 | 精确？（不匹配 / 测试数） | wasm bytes，`println("${1.5}")` | 相对 Dragon4 的 Δ | native ns/转换 | wasm ns/转换 | 注记 |
|---|---|---:|---:|---:|---:|---|
| Dragon4（0.62.0，原有方案） | 0 / 20M（自身验证） | **7,783** | — | 295 (`format!`) | 13,715 | 精确大整数，无表；47 个函数 |
| **Schubfach，运行时推导 g(k)**（已交付） | 相对 native `format!` **0 / 1,032,489** | **5,508** | **−2,275 (−29 %)** | 295 (`format!`) | **1,044** | 完全没有表：g(k) 来自各一个小整数乘法与小整数除法的大整数循环 |
| Schubfach，紧凑表（78 × 189-bit，每隔 8 个 k，重新缩放） | 0 / 52,489 | 8,543 ¹ | +760 | — | 448 | 对全部 617 个 k 精确（离线检查）；234 个 `Int` 字面量 |
| Schubfach，完整表（617 × 126-bit） | 0 / 37,489 | 24,372 ¹ | +16,589 | — | 583 | 1,234 个 `Int` 字面量，各约 17 B；每次调用都重建表 |
| Grisu3 + 精确回退 | 未构建 | ≥ 回退方案单独的大小 | > 0 | — | — | 任何精确回退都是完整打印器；Grisu3 只能在其上增加字节（见下文） |

¹ 测量形式为 `println(sf(1.5))`，打印器内联进探测程序（已交付行是实际的 `${1.5}`，打印器在 stdlib 中；已交付方案的内联形式测得 4,921，因此内联表方案各行约乐观了 600 B）。

精确性列指打印字符串的逐字节相等。“相对 native `format!`”是可用的最强判定基准：同一带种子的值流分别由 wasm 路径的原型和 native 路径的 `float.to_string`（它*就是* Rust 的 `format!`）打印，两个 1,032,489 行输出经 `cmp` 比较完全相同。该流包含覆盖整个 64-bit 空间的 1,000,000 个带种子的 xorshift64 位模式（全部指数，包括特殊值及两种符号），加上一个 32,489 值的边界集合：每个偏置指数配以有效数 {0, 1, 2, 3, mid, max−1, max}、通过反复 ×10 / ÷10 可达的每个 10^k 及其两侧位模式邻值、0.1+0.2、1/3、2^53+1——每项都包含符号位清零和置位的情况。已交付方案还在 wasm 路径上与 Dragon4 移植对同一流进行了比较：0 个不匹配。

<a id="targeted-sets-beyond-uniform-bit-patterns"></a>

### 定向集合（均匀位模式之外）

在 wasm 路径上接入的同一 `float.to_string` 与 native `format!` 比较，每个集合用一个程序在两条路径运行并以 `cmp` 比较（`research/spike/float-printer/gen_evidence.py`；涉及解析器时，行中还带有解析后的位，因此能区分解析器差异与打印器差异——均未发生）。0.62.0 release binary（Dragon4 打印器）也运行了同样的程序，作为第二个参考。

| 集合 | 测试数 | 相对 native `format!` 的不匹配 | 相对 0.62.0 Dragon4 |
|---|---:|---:|---|
| 1. 指数字段 1000..1060（约 1e-7..1e10），随机符号及有效数 | 1,000,000 | **0** | 在内存耗尽前打印的 519,151 个值上相同（每次调用 3,744 B 临时空间） |
| 2. 十进制往返：1..17 位有效数字，形式为 `d.dddd`、`dd.dd`、`-d.ddd`、`d.ddde±N`、`ddde±N`（N 为 −330..309），用 `float.parse` 解析后打印 | 500,000 | **0** | 相同 |
| 3. 每个整数 0..100000；k 在 −320..308 的 10^k；k 在 −1074..1023 的 2^k；1.0、1e15、1e16、1e17、1e22、1e23 周围的 201 个双精度数；这些锚点处双精度数中间的十进制数；最小次正规数、最大有限值；0.1/0.2/0.3/0.7/1.1/2.675/5e-324/9007199254740993 | 103,969 | **0** | 相同 |
| 4. `float.to_fixed`（保留的 Dragon4 定点路径），随机位模式 × 精度 0..10 | 100,000 | **0** | 逐字节相同 |

集合 2 直接在 `wasmtime` 下运行：wasm 路径的 `float.parse` 每个字符串约耗时 0.3 ms，而该 release 的 `almide run` 宿主会在 30 s 时中断（现在不再如此：#2615）。这些集合没有引出规则变化；下文两项 Java 与 Rust 的差异都由边界集合发现。集合 1 的 Dragon4 参考运行内存耗尽，是因为 0.62.0 release 的旧打印器每次调用需 3,744 B 临时块；已交付打印器在同一宿主上完成了 10,000,000 次调用的探测（每次调用 208 B 临时空间）。

计时使用 `almide bench`（预热后 3 次的中位数），转换带种子流中的 1,000,000 个值——均匀随机位模式，因此大多是极大或极小的绝对值，对任何打印器都是高开销一端。Native 没有 Almide 打印器：其 `float.to_string` 是 `format!`，因此 native 列每行都是同一个 295 ns。1,044 ns 来自内联进基准程序的打印器；接入 stdlib 后（wasm 路径上的 `float.to_string`），同一个 1M 次转换基准测得 1,126 ns。

Hello, world 为 1,337 B；已交付打印器在其上增加 4,171 B，其中约 330 B 是 Dragon4 也携带的字符串渲染器（`__dg_render`），约 1,000 B 为运行时 g(k) 推导，其余是 Schubfach 核心（mulhi、rop、候选选择）以及打印器引入的运行时辅助函数（list/string 分配、`int.to_string` 连接代码）。

<a id="why-the-table-free-schubfach-is-the-smallest-exact-printer"></a>

## 为什么无表 Schubfach 是最小的精确打印器

每一种最短往返算法都是代码与十次幂表之间的权衡：

- **Dragon4**：无表，代码大（大整数逐位循环占主体）。
- **Ryū / Schubfach**：代码小，617 项 × 128-bit 表（原始大小 9.9 KB）。在该路径上，一个字面量 `List[Int]` 每元素约占 17 B，字符串字面量每字节占 1 B，因此完整表无论如何编码都为 12–21 KB——实测总计 24,372 B。Issue 的预测（“10 KB 的表比 6.4 KB 的代码更差”）成立。
- **紧凑表**：每隔 8 项保留一项并携带足够的保护位（189），使通过 10^0..10^7 重新缩放能重现全部 617 个精确项（已离线验证），将原始数据减至 1.9 KB——作为字面量为 4 KB——但仍比 Dragon4 *多* 760 B。
- **运行时推导表**：g(k) = ⌊10^k · 2^-r⌋ + 1 本身就是小型精确计算。k ≥ 0 时，在 u32 limb 上分块 ×10^9 得到 P = 10^k，g − 1 是 P 的最高 126 bits；k < 0 时，P = 2^N 经分块 ÷10^9 除以 10^-k——整数上 `⌊⌊x/a⌋/b⌋ = ⌊x/(ab)⌋`，所以分块除法是精确的——恰好留下 g − 1 的 126 bits。这是一个乘小整数循环、一个除小整数循环、一次设置二次幂以及位提取器：以约 1 KB 代码代替 10 KB 数据，并因去掉逐位循环而比 Dragon4 快 13×（每个值三个 64×64 乘积和几次比较）。
- **Grisu3** 对无法确认正确性的约 0.5 % 输入需要回退。回退必须是完整的精确打印器——Dragon4 或本 Schubfach——因此 Grisu3 严格增加字节，只能用于追求速度。未构建它：它能回答的大小问题已由上一行解决，而 wasm 上 1 µs/转换时，速度问题也不再悬而未决。

为与 `format!` 逐字节相同，需要对 Giulietti 的 Java 做两项偏离，均由边界集合而非阅读发现：

1. **中点按绝对值向上舍入，而非取偶。** 两个最近候选等距时（1669760939663944.25——...4.2 与 ...4.3 都能往返），Rust 的 Dragon4（`2R >= S`）选择 ...4.3；Java 选择偶数 ...4.2。
2. **从 s ≥ 10，而非 s ≥ 100 开始尝试更短的 10 的倍数候选。** Java 至少打印两位，因此从不需要一位结果。`format!` 对 20 · 2^-1074 打印 `1e-322`（s = 98，100 位于舍入区间）。同样，Java 的 `C_TINY` 分支（将 MIN_VALUE 打印为 `4.9E-324`）不存在：`format!` 打印 `5e-324`。

<a id="what-shipped"></a>

## 交付内容

- `stdlib/float_to_string.almd`：`float.to_string` 是采用运行时 g(k) 的 Schubfach；`float.to_fixed` 保留 Dragon4 定点模式逐位循环（不同算法——恰好 N 位小数，中点取偶），两者均不可达对方代码，因此程序只为调用的打印器付出开销。已删除无用的 Dragon4 最短模式函数。
- `spec/stdlib/float_to_string_shortest_test.almd`：上述边界情况，由位模式固定。
- `crates/almide-wasm/tests/golden/size-baseline*.txt`：重新批准——语料中 113 个 fixtures 发生变化，111 个缩小，总量 1,823,014 → 1,569,846 B（−253,168 B，−13.9 %；交付形式台账为 −252,348 B）。增长的两个（`edge_float_formatting` +470 B、`float_concrete` +478 B）同时调用 `to_string` 与 `to_fixed`，所以旧版两者共享一个大整数核心的位置，现在携带两个打印器。`alloc-baseline.txt` 总计 −281,632 B（3,744-byte Dragon4 临时块变成 208 bytes）；因较小块改变分配器峰值位置，38 行增加了 64–80 B。
- `research/spike/float-printer/`：差分测试框架（`gen_harness.py` + `harness_common.almd`：带种子流、边界集合、wasm/native dump/diff 以及 `bench` 程序）、精确性集合及大小探测。测量比较的原型（`schubfach.almd`，将已交付核心作为独立程序；`schubfach_rt.almd` / `schubfach_rt2.almd`，移位减法版本；两个表方案及推导它们的 `gen_table_variants.py`）基于 `prim.*` 编写，当 `prim` 不再是用户表面（#3025）时被删除——可用 `git log --diff-filter=D -- research/spike/float-printer/` 恢复；现在它们只能作为 stdlib 源码编译。要针对已交付打印器重跑 1M 检查：执行 `python3 gen_harness.py /dev/null float.to_string 12345 1000000 out`，然后 `almide run out/dump_proto.almd --target wasm` 与 `almide run out/dump_native.almd`，再对两个输出运行 `cmp`。

该变更运行的关卡：`almide test spec/stdlib/`（161 files）与 `spec/lang/`（211），已接入 `float.to_string` 的 1M 跨目标差分，在两条路径运行每个涉及 float 的 `spec/wasm_cross` fixture（105 fixtures，stdout 相同），大小棘轮及分配台账的重新批准。

<a id="recommendation"></a>

## 建议

交付无表 Schubfach（本 PR 已完成），并将 5.5 KB 视为打印一个 float 的程序的新下限，其中约 4.2 KB 是打印器。剩下的手段不是算法，而是可达性。float 上的 `${x}` 引入整个最短往返路径；只打印整数值 float 或固定精度的程序，可能只需可达一个小得多的格式化器——将 #1712/#1962 按需拆分的原则应用于 stdlib 的一族功能，这是 issue #1985 的选项 3，仍待解决。

<a id="addendum-2099-2026-09-24-to_fixed-on-exact-scaling-one-printer-for-both-forms"></a>

## 补记 (#2099, 2026-09-24)：基于精确缩放的 to_fixed，两种形式共用一个打印器

在 develop `c653598ee` 上重新测量（#2368 移除重复字符串常量之后）：`println("${1.25}")` 为 5,405 B，而 Int 对照为 1,970 B，因此最短打印器占 3,435 B。`println(float.to_fixed(1.25, 2))` 为 6,650 B：保留的 Dragon4 定点路径占 4,680 B，比最短打印器还多，而且它正是 `spectralnorm`（大小阶梯的 T2 阶）实际调用的路径。作出如下变更，输出不变：

- `__sf_scale(x, q, k)` 在大整数上精确计算 floor(x · 2^q · 10^-k)——将 x 移位或乘以 10^-k，然后以 10^9 分块除以 10^k，或右移——并从余数读取 sticky bit。约 250 B。
- `float.to_fixed(x, nd)` 调用它一次来求 M = floor(2 · v · 10^nd)：N = M >> 1，舍弃的位代表一半，若 sticky bit 或 N 的末位被置位则向上舍入（中点取偶），然后反复除以 10^9 得到 N 的各位。Dragon4 定点模式逐位循环——每位一次大整数比较减法、五个大整数、`math.log10` 估算及其修正——已移除。
- `float.to_string_compound`（`${x}` 形式）是带 `dot0` 标志的同一打印器，而不是将 `to_string` 的结果重新渲染以去掉 `.0` 的独立模块；零通过同一 writer 渲染，而非四个字符串字面量，因此打印器除 `NaN` / `inf` / `-inf` 外不携带任何字面量（字面量会移动静态池，进而移动每个打印 float 的程序的每一行分配台账）。
- `__sf_wd` 在一次逆向遍历中将数字、零和小数点直接写入结果字符串——无数字缓冲、无复制、无三路渲染器。数字源耗尽后，无需除法即可写出零（极大或极小的值主要是零）。
- `float.to_string` 保留上述采用运行时 g(k) 的 Schubfach 核心。也尝试过用 `__sf_scale` 计算它的三个数（各自一条精确大整数流水线）：还能节省约 400 B，但会令 wasm 上的 `float.to_string` 慢 3×（每值 0.95 → 2.9 µs），而没有主流打印器为此接受最短打印慢 3×。最短打印器的大小交给紧凑表后续工作处理。

测量（`almide build --target wasm`，develop `547b0fdaf` → 本变更）：

| 程序 | 之前 | 之后 | Δ |
|---|---:|---:|---:|
| `println(float.to_string(1.25))` | 4,886 | 4,864 | −22 |
| `println("${1.25}")` | 5,405 | 5,068 | −337 (−6.2 %) |
| `println(float.to_fixed(1.25, 2))` | 6,650 | 4,529 | −2,121 (−31.9 %) |
| `spectralnorm`（大小阶梯 T2） | 12,121 | 10,000 | −2,121 |
| `spec/wasm_cross/edge_float_formatting.almd`（两种打印器） | 16,700 | 13,915 | −2,785 |
| `spec/wasm_cross/float_to_fixed.almd` | 7,200 | 5,079 | −2,121 |
| `spec/wasm_cross/float_shortest_roundtrip.almd` | 5,096 | 5,074 | −22 |

`size-baseline.txt`：146 行变化，全部下降，总计 −31,748 B（交付形式台账 −31,916 B）；大小阶梯中打印 float 的各阶合计 −12,904 B。

wasm 路径运行时间：`almide bench --target wasm`，对带种子的 xorshift64 流进行 200,000 次转换（主要是极大和极小绝对值，开销高的一端），在有负载的机器上与 develop 交错运行 9 次取中位数：`float.to_string` 在 develop 上为 163–165 ms，这里为 159–162 ms（在噪声范围内未变）；`float.to_fixed(x, 3)` 为 5,859 ms → 113 ms（29 → 0.56 µs：大整数逐位循环已移除）。没有哪行 perf 会打印超过少数几个 float。

本变更的精确性证据：`tests/float_to_string_cross_target_test.rs` 在 wasm 路径上打印每个偏置指数 × 7 个有效数 × 两种符号（28,672 个值）的全部三种形式、历史中的十进制特殊案例、1074 / 1100 / 1200 / 4096 位的 `to_fixed`，以及 100,000 个 xorshift64 位模式（本地 1,000,000），并与 Rust `format!` 逐字节比较；在故意破坏的守卫（`s >= 100`，Java 规则：首次不匹配位于 100,521 行）下失败，在此打印器下通过。发现精确缩放重写中两个错误的是指数扫描，而非随机流：一个整数值的尾随零落到了左边；定点值的 9 位块以错误顺序写入。

<a id="addendum-2099-2026-09-24-floatto_string-reads-gj-from-a-compact-table"></a>

## 补记 (#2099, 2026-09-24)：`float.to_string` 从紧凑表读取 g(j)

运行时 g(j) 推导（每个值都进行一次大整数十次幂计算）占用了 `float.to_string` 的大部分时间。现在从紧凑表读取。其形式是每个面向 wasm 的正确打印器都采用的：Swift 的 `FloatingPointToString.swift`、Wado 的 `fpfmt.wado`，以及 Ryu / Zig 的小表。

- **表。** 每隔 27 个幂存储一个 127-bit C(b) = floor(5^b · 2^-s)（b = −297 … 324，24 项）。g(j) = floor(C(b) · 5^(j−b) / 2^u) + d(j) + 1：
  - 用现有 64×64 高位乘积辅助函数求一个 127×63-bit 乘积；
  - 最多 26 次乘法计算精确的 5^f；
  - 每个 j 存储一个修正位 d(j) ∈ {0, 1}。它补偿 C 的截断；一位足够，因为 C 比保留的 126 位多携带一位。
- **为什么输出不可能改变。** 对打印器可能请求的全部 617 个 j，结果都是论文定义的精确 g(j)，所以候选选择看到的值逐位相同。
- **表放在哪里。** 表的每个字节携带 7 bits，因此 552-byte 表是普通字符串字面量：位于数据段，而非代码中的 i64 常量。本文第一轮尝试的紧凑表是 234 个 `Int` 字面量，增加了 +760 B。
- **移除了什么。** `to_string` 不再分配临时块。仅通过 `to_string` / `${x}` 打印 float 的程序不再链接大整数辅助函数。

相对 develop `547b0fdaf` 测量（`almide build --target wasm`）：

| 程序 | develop | 本变更 | Δ |
|---|---:|---:|---:|
| `println(float.to_string(1.25))` | 4,886 | 4,596 | −290 |
| `println("${1.25}")` | 5,405 | 4,800 | −605 (−11.2 %) |
| `println(float.to_fixed(1.25, 2))` | 6,650 | 4,529 | −2,121 |
| `spec/wasm_cross/edge_float_formatting.almd`（两种打印器） | 16,700 | 14,448 | −2,252 |
| `spec/wasm_cross/float_shortest_roundtrip.almd` | 5,096 | 4,806 | −290 |

同时调用两种打印器的程序携带表以及 `to_fixed` 需要的大整数辅助函数。与紧邻之前的步骤相比，这些程序增加约 540 B（2 个 fixtures）；所有其他打印 float 的 fixture 都缩小。

分配水位台账计算静态池，因此每个打印 float 的程序，其行会增加表的大小，约 0.5 KB。分配本身减少：次数台账显示所有变化的行都下降，每次 `to_string` 调用少一次 `alloc_list`。

Wasm 运行时间，`almide bench --target wasm`，对带种子 xorshift64 流进行 200,000 次转换，在有负载机器（load ≈ 18）上与 develop build 交错运行：

| build | 中位数 | 最小值 |
|---|---:|---:|
| develop | 246–349 ms | 218–220 ms |
| 本变更 | 72–102 ms | 66–83 ms |

在同一机器、同一时刻约为 3×。安静机器上的 develop 数值为 145 ms（每值 0.73 µs），因此本变更约为每值 0.25 µs。剩余时间大部分用于写出那些极大与极小值所打印的约 300 字符字符串。每次 store 写入八个零实测快约 10–15 %，但每个打印 float 的程序都要多 88 B，因此未采用。

精确性证据：

- `tests/float_gtable_exact_test.rs` 从 stdlib 源码解码表，并对每个 j 重放打印器自身的 64-bit 字运算。它将各 g(j) 与大整数定义比较。
  - 改变一个表字节（第 12 项，第 3 字节）会使它在 j = 27 … 53 时失败。
  - float 扫描不会捕获该变更：损坏的位约在 g 最高位之下 2^-100，任何采样的舍入决策都没有触及它们。这就是表拥有专门测试的原因。
- float 扫描（所有偏置指数、特殊案例、100,000 个模式；本地 1,000,000）通过。去掉修正位后，在 100,479 行失败。
