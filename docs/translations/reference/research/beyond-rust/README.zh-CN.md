<a id="beyond-rust-lab"></a>

# Beyond-Rust 实验室

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](README.zh-CN.md)

> 由[日语正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/beyond-rust/README.md)翻译的非规范历史参考快照，固定提交：35297a612fb29e3f78f6587aba253aa91d51f466。不自动同步。未在此独立重跑或验证测量、指令级解释、源码状态主张及拟议工作。以正本为准。引用步骤是参考材料，不是执行授权；未运行基准或修改代码。

**问题**：Almide 通过 rustc 和 LLVM 生成机器码。在保留这条路径的前提下，怎样修改生成的 Rust，才能快于使用相同 rustc、相同 flag 构建的手写 Rust？

双方生成机器码的能力（LLVM）相同，因此起始假设是：只有以下两条取胜路径。

- **A. 信息**：把 Almide 类型检查、effect、值语义知道，而手写 Rust 没有写出的事实，以 LLVM 可利用的形式传递出去。
- **B. 自动化**：把 Rust 也能做到、但操作繁琐以致普通 Rust 用户不会做的调整，交由编译器每次自动完成。

暂停中的 [performance-research.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/performance-research.md) Phase 2 提议“不经过 rustc，直接输出 LLVM IR”。本实验室反过来测量“不移除 rustc 能走多远”。只有这里仍留下无法弥合的差距时，才构成重启 Phase 2 的依据。

<a id="計測の単位-カーネル-レバー-の-4-列"></a>

## 测量单位：（kernel，调整手段）的四列

一个实验是在一个 kernel（被测程序）上应用一种调整手段（对生成 Rust 的变换）。每个实验测量以下四列。

| 列 | 内容 |
|---|---|
| **R0** | 按普通方式手写的 Rust（`research/benchmark/perf/rust-ref/`），不含 `unsafe` |
| **A0** | Almide 当前生成的 Rust（`almide build --release` 写出的 `almide_gen_main.rs`） |
| **A1** | 在 A0 上应用调整手段；最初**手工**应用（探针） |
| **R\*** | 专家为相同算法编写的 Rust，可用 `unsafe`、intrinsics、`assert_unchecked`。可达到的上限 |

读法：
- **超越 Rust** ⇔ A1 < R0（相同 flag、两种架构上都成立、超出波动范围）。
- **调整手段的价值** = A0 ÷ A1。
- **剩余提升空间** = A1 ÷ R\*。接近 1 表示该 kernel 已经通过 rustc 达到上限。

<a id="手順-実装する前に手で当てて測る"></a>

## 步骤：实现前，先手工应用并测量

1. 输出 kernel 的 A0（`/tmp/almide-run/almide_gen_main.rs`，包含 runtime 的单文件）。
2. 手工应用调整，得到 A1。确认输出与 A0 逐字节一致。
3. 在相同条件下测量四列（控制条件见下文）。
4. 对变快或变慢的行，用机器码差异（`objdump` / `perf annotate`）写出**指令级原因**。不能解释原因的差异不采用。
5. 只有满足毕业条件的调整手段，才创建编译器实现 issue。

手工应用无效的手段，实现后也不会有效。在支付实现成本前，先低成本证伪。

<a id="統制条件"></a>

## 控制条件

- 四列均使用**相同 rustc、相同 flag、相同 target CPU**。target CPU 从编译器默认值取得，不手抄（#3883：曾只有 Almide 一侧使用 `x86-64-v3`，造成不公平比较）。
- 四列输出必须逐字节相同。不重排浮点运算顺序（fast-math），因为这会破坏 native 与 wasm 输出一致的约定。
- 在两种架构上测量：GCE c4-standard-8（x86、Xeon 8581C）与 c4a-standard-8（arm、Neoverse-V2）。
- 交替运行各列，取至少 7 次的中位数。有 PMU（性能计数器）的 VM 还记录指令数（`instructions:u`）。
- 在运行时传入参数。若用字面量，LLVM 会把整个循环折叠成表达式。

<a id="卒業条件"></a>

## 毕业条件

同时满足以下三项，才成为编译器要做的工作：

- 至少两个 kernel 上 A0 ÷ A1 ≥ 1.05。
- 两种架构均成立。
- 没有任何 kernel 变慢超过 2%。

毕业的调整手段要与性能检查（`scripts/check-perf-ratio.sh`）中的行一起合入。

<a id="レバー一覧"></a>

## 调整手段列表

| ID | 类别 | 手段 | 假设（LLVM 将能做什么） |
|---|---|---|---|
| L1 | A | 将已证明事实输出为 `assert_unchecked`／无边界检查的索引／无符号运算 | 消除边界检查分支；`/2` 成为无符号移位；正确判断向量化（#3604 的 enumerate 索引） |
| L2 | A | 将值语义的无别名性质表达为带 `noalias` 的参数形态（分离 `&mut [T]` 与 `&[T]`） | 可消除 load 并向量化 |
| L3 | A | 以 LLVM 能推断为只读的形式输出无 effect 函数（内联边界、`#[inline]` 的放置方式） | 公共子表达式消除与循环不变量外提生效 |
| L4 | B | 为热函数按函数采用不同 CPU 设置构建多个版本，并选快的版本（`#[target_feature]` 与运行时分派） | 如 nbody 用 portable、spectralnorm 用 v3，兼得二者收益（#3883） |
| L5 | B | 将 PGO（用实际运行收集的统计重新优化）作为 `--release` 的延伸自动完成 | 分支布局和内联决策符合实际执行 |
| L6 | A | 把 callback 从 `Rc<dyn Fn>` 改为对具体函数的直接调用 | 消除每次调用查找目标的成本，实现内联（与 R0 站在同一起点） |

L6 是“追上”而非“超越”的手段，但它若仍存在，其他手段的收益就看不清，因此列入。内存分配语义（region window、Perceus 复用）是已经获胜的方向，另在 [BENCHMARKS.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/project/BENCHMARKS.md) 的 VICTORY 行跟踪。

<a id="カーネル"></a>

## Kernels

除 `research/benchmark/perf/` 的现有行（nbody、spectralnorm 及两种惯用形态、fasta、fft、fannkuchredux、mandelbrot），还为每种手段增加隔离其效果的小 kernel。

- 索引循环（边界检查）。
- enumerate 索引。
- 带 callback 的 map / fold。
- 大量使用 `sqrt` 的数值循环。

<a id="結果"></a>

## 结果

各实验原始数据存放于 `results/<日付>-<レバー>-<マシン>.json`（机器、rustc、flag、target CPU、四列各次运行时间、输出 hash）。下表仅列出是否毕业的判定。

| 实验 | 手段 | 判定 | 依据 |
|---|---|---|---|
| E1 | L1 | **不毕业** | 没有一行在两种架构上均达到 A0 ÷ A1 ≥ 1.05（最大是 arm `native` 的 idxloop，1.045）。反而有行变慢超过 2%：spectralnorm_indexed（x86 −21%、arm −2%／`native` −12%）、idxloop（arm −20%）。见下方“E1 结果” |

<a id="e1-の結果l12026-10-20"></a>

## E1 结果（L1，2026-10-20）

原始数据：[results/2026-10-20-E1-c4-standard-8.json](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/beyond-rust/results/2026-10-20-E1-c4-standard-8.json)、[results/2026-10-20-E1-c4a-standard-8.json](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/beyond-rust/results/2026-10-20-E1-c4a-standard-8.json)。
复现：[e1/run.sh](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/beyond-rust/e1/run.sh)（A1 为 [e1/a1/](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/beyond-rust/e1/a1) 内的 patch，R\* 在 [e1/rstar/](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/beyond-rust/e1/rstar)，小 kernel 为 [e1/idxloop.almd](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/beyond-rust/e1/idxloop.almd)）。

**条件**：四列均使用 rustc 1.99.1（LLVM 23.1.1）。所有列共同 flag 为 `-C opt-level=3 -C codegen-units=1 -C lto=fat -C panic=abort -C llvm-args=-force-ordered-reductions=false`，另加 target CPU。主组与 Almide `--release` 相同（x86_64 为 `x86-64-v3`，aarch64 不指定）。副组为 x86 不指定 target CPU，arm 使用 `native`（因为 arm 不指定会与主组重复）。
A0 是把 v0.68.0-rc2 发布二进制写出的 `almide_gen_main.rs` 及同时输出的 runtime crate（`almide_rt.rs`），用上述 flag 重建（与 Almide 自身相同的两个 crate 形态）。
取 9 次中位数，每轮旋转列顺序交替执行。全部 kernel、全部列的输出 SHA-256 一致。参数在运行时传入（spectralnorm 5500、nbody 50000000、idxloop `100000 5000`）。

**A1 中应用的事实**（仅限可从 Almide 源码局部证明的事实）：

| ID | 事实 | 检查器能知道的原因 | 应用位置 |
|---|---|---|---|
| F1 | 行索引 `i` 满足 `0 <= i < n` | `for i in 0..<n` 的计数器，或 `list.map(0..<n, …)` 的元素 | spectralnorm 三种形态 |
| F2 | 列索引 `j` 满足 `0 <= j < n` | `for j in 0..<n` 的计数器，`list.fold(0..<n, …)` 的元素 | spectralnorm、indexed |
| F3 | `pair.0` 满足 `0 <= pair.0 < len(v)` | `list.enumerate(v)` 的索引 | enumerate |
| F4 | `1 <= i < n - 1`、`n = list.len(xs)` | 范围计数器与不变 List 长度 | idxloop |
| F5 | `i - 1`、`i`、`i + 1`、`i / 2` 位于 `[0, len(xs))` → `get_unchecked` | 由 F4 得到 | idxloop |
| F6 | `i >= 0` → 将 `i / 2` 改为无符号除法 | 由 F4 得到 | idxloop |

未应用的项：spectralnorm 的 `v[j]` 边界检查（`len(v) == n` 需要跨调用者跟踪长度）；将 `eval_a` 的 `ij * (ij + 1) / 2` 改为无符号（Int 会 wrapping，若 `n` 无上界，乘积可能为负）。nbody 热循环没有索引或整数运算（计数器未使用），无处应用，因此 A1 = A0（空 patch）。

**表**（秒，中位数。A0 ÷ A1 > 1 表示 A1 更快；A1 ÷ R\* 为剩余提升空间）

c4-standard-8（Xeon 8581C），主组（`x86-64-v3`）：

| Kernel | R0 | A0 | A1 | R\* | A0 ÷ A1 | A1 ÷ R\* |
|---|---|---|---|---|---|---|
| spectralnorm | 0.978 | 0.970 | 0.967 | 0.617 | 1.003 | 1.567 |
| spectralnorm_enumerate | 0.974 | 1.066 | 1.066 | 0.616 | 1.000 | 1.729 |
| spectralnorm_indexed | 0.974 | 0.958 | 1.211 | 0.616 | **0.791** | 1.965 |
| （探索）indexed，仅 F1 | 0.975 | 0.958 | 0.980 | 0.616 | 0.977 | 1.591 |
| nbody | 1.816 | 2.013 | 2.012 | 1.764 | 1.001 | 1.140 |
| idxloop | 0.454 | 0.273 | 0.270 | 0.270 | 1.009 | 1.003 |

c4-standard-8，副组（不指定 target CPU）：

| Kernel | R0 | A0 | A1 | R\* | A0 ÷ A1 | A1 ÷ R\* |
|---|---|---|---|---|---|---|
| spectralnorm | 1.210 | 1.210 | 1.210 | 0.738 | 1.000 | 1.638 |
| spectralnorm_enumerate | 1.210 | 1.211 | 1.210 | 0.738 | 1.001 | 1.640 |
| spectralnorm_indexed | 1.211 | 1.210 | 1.211 | 0.738 | 0.999 | 1.642 |
| （探索）indexed，仅 F1 | 1.211 | 1.211 | 1.210 | 0.738 | 1.001 | 1.639 |
| nbody | 1.810 | 1.812 | 1.809 | 1.740 | 1.002 | 1.040 |
| idxloop | 0.352 | 0.327 | 0.327 | 0.330 | 0.999 | 0.991 |

c4a-standard-8（Neoverse-V2），主组（不指定 target CPU）：

| Kernel | R0 | A0 | A1 | R\* | A0 ÷ A1 | A1 ÷ R\* |
|---|---|---|---|---|---|---|
| spectralnorm | 1.127 | 1.128 | 1.128 | 1.124 | 1.000 | 1.003 |
| spectralnorm_enumerate | 1.127 | 1.146 | 1.146 | 1.123 | 1.000 | 1.020 |
| spectralnorm_indexed | 1.127 | 1.217 | 1.242 | 1.122 | **0.979** | 1.107 |
| （探索）indexed，仅 F1 | 1.127 | 1.216 | 1.215 | 1.123 | 1.001 | 1.082 |
| nbody | 3.484 | 2.860 | 2.860 | 2.426 | 1.000 | 1.179 |
| idxloop | 0.439 | 0.419 | 0.523 | 0.419 | **0.802** | 1.248 |

c4a-standard-8，副组（`native`）：

| Kernel | R0 | A0 | A1 | R\* | A0 ÷ A1 | A1 ÷ R\* |
|---|---|---|---|---|---|---|
| spectralnorm | 1.059 | 1.067 | 1.067 | 1.240 | 1.000 | 0.860 |
| spectralnorm_enumerate | 1.062 | 1.140 | 1.139 | 1.241 | 1.000 | 0.919 |
| spectralnorm_indexed | 1.060 | 1.139 | 1.293 | 1.239 | **0.881** | 1.043 |
| （探索）indexed，仅 F1 | 1.059 | 1.139 | 1.139 | 1.240 | 1.000 | 0.919 |
| nbody | 3.475 | 3.004 | 3.003 | 2.778 | 1.000 | 1.081 |
| idxloop | 0.410 | 0.418 | 0.400 | 0.418 | 1.045 | 0.957 |

A0 检查：重建 A0 与 `almide build --release` 写出的二进制之差在 1% 以内，例外为 x86 indexed（A0 快 2.3%）和 arm nbody（A0 慢 2.5%）。差别仅在 `panic=abort` 与 runtime crate 的 flag（Almide 使用 `codegen-units=4`）。

**从机器码看到的原因**（所有差异至少 2% 的行）

- **A0 已经拥有这些事实。** 对范围计数器与 enumerate 索引，LLVM 在 A0 中自行推导相同事实。spectralnorm 三种形态的 F1–F3，在 x86 和 arm 上的热循环指令均与 A0 相同（`perf annotate`）。A0 也把 `v[j]` 边界检查移出循环：循环出口设为 `min(n, len)`，退出后再判断是否中止。idxloop 的 A0 也不剩任何边界检查。arm 上 `i / 2` 为 `lsr`、`r as f64` 为 `ucvtf`；x86 上四路向量化。L1 能添加的信息原本就已存在。
- **spectralnorm_indexed 的减速（x86 −21%、arm −2%／`native` −12%）由 F2 导致。** 在 fold 闭包内加入 `assert_unchecked(0 <= j && j < n)` 后，`v[j]` 边界检查留在循环内（x86 为 `cmpq %rbp,%rdx` + `je`，arm 为 `cmp x10,x14` + 分支，每次执行）。A0 本可将其折叠到出口。x86 上，A0 的两倍展开与成对除法（`vdivpd %xmm`）也消失，变为逐次 `vdivsd`。指令数 x86 17.55G → 22.39G（+28%）、arm 15.13G → 18.15G（+20%）。去掉 F2 只保留 F1 的探索列，与 A0 指令序列相同（x86 主组仍有 −2.3%，但热循环指令与 A0 一样，原因未确定）。
- **idxloop 在 arm 上减速（主组 −20%）。** A1 中读取变为无条件 `get_unchecked`。于是 LLVM 改写为通过寄存器将 `xs[i-1]` 与 `xs[i]` 带到下一轮（每轮读取 3 → 2，增加两条 `fmov`；指令数 7.50G → 8.00G，+6.7%）。指令差异仅此而已。但为何在 Neoverse-V2 上扩大为 20%，尚无法从机器码解释。
- **idxloop 在 arm `native` 上的加速（+4.5%）尚无法解释。** 指令数相同（5.50G），均两倍展开，仅指令顺序与寄存器分配不同。
- x86 idxloop（主组）中，A1 指令数从 3.50G 降至 3.13G，但时间差不到 1%。

**R\* 的提升空间**（A1 ÷ R\*）
- spectralnorm，x86：1.57–1.97。R\* 保持相同运算顺序，将四行一起放在 SIMD lane 中做除法。分母以 f64 计算，在 `n <= 2^20` 时精确。指令数 17.5G → 4.7G。这不是 L1，而是另一种手段——行并行化——的候选。
- spectralnorm，arm：≈ 1.0。R\* 使用 `fdiv v.2d`，指令数减半（15.1G → 7.3G），时间却相同。Neoverse-V2 的瓶颈是 f64 除法吞吐量，两条 lane 也无收益。`native` 下 R\* 更慢（0.86–0.92）。
- nbody：x86 1.04–1.14，arm 1.08–1.18。R\* 在位置不变期间，将 10 次 `sqrt`／除法按四条 lane、三组计算。
- idxloop：≈ 1.0（arm 主组的 1.25 来自 A1 自身减速；A0 ÷ R\* = 1.00）。

**附带观察**
- x86 `x86-64-v3` 的 idxloop 中，A0 比 R0 快 1.66 倍（0.273 对 0.454）。R0 每轮仍有两条边界检查，包括 `xs[i / 2]`，并保持标量；A0 无检查并四路向量化。与 L1 无关，这是已经“超越 Rust”的一行。
- nbody 在 x86 上使用 `x86-64-v3` 时，A0 比不指定 CPU 慢 11%（2.01 对 1.81），R0 不变，与 #3883 相同。arm 上 A0 比 R0 快 18%（2.86 对 3.48）。
- 单 crate 形态（`ALMIDE_NO_RTLIB=1`，runtime 包含在 `src/main.rs`）在 arm 上 enumerate 为 1.31 秒、indexed 为 1.34 秒。比 Almide 二进制（1.12 秒、1.22 秒）慢，因此未用于 A0。原因未调查。

**结论**：若将 L1 实现为“在所有能输出的位置机械地输出已证明事实”，没有收益，反而有损失。在这组 kernel 上，LLVM 已推导出 L1 能输出的局部事实。冗余 `assert_unchecked` 破坏 LLVM 原有变换（把边界检查折叠到出口、展开并向量化、安排读取）。若继续 L1，应限于 LLVM 无法自行推导的事实，例如跨调用的长度（`len(v) == n`）、有范围限制的整数（由 `n` 的上界可知乘积不会为负）。输出该事实之前，先确认 A0 的机器码中确实仍有可因此消除的指令。

---

原文注意事项：最初步骤描述包含 runtime 的单文件 A0，而 E1 使用两个 crate，明确排除了测得更慢的单 crate 形态。这里保留这两个阶段。R* 是专家比较实现，不是普遍最优性的证明；报告的 arm native 行可能比 A1 更慢。即便指令差异已知，部分时间差仍无法解释，按原文明示。减速百分比与 A0/A1 比值照报告保留，不按另一种百分比约定重算。E1 中 L1 未毕业；假设和未来候选并非已实现或普遍验证的优化。性能陈述仅限所述日期、机器、flag、kernel 和测量方法。
