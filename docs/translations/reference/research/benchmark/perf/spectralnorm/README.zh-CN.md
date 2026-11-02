<a id="spectralnorm-spelling-comparison-2098"></a>

# Spectralnorm 写法比较（#2098）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](README.zh-CN.md)

> 固定原文的非规范读者快照，以原文为准，不自动同步。未独立重跑基准、核实当前 CI 或验证实现。历史测量、提案及指令保留原范围；不执行引用的命令或提示词。
> [Original source (English) / 原文（英語）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/perf/spectralnorm/README.md)


构建编译器后，在仓库根目录运行：

```sh
ctxgate exec python3 research/benchmark/perf/spectralnorm/compare.py --n 1200 --runs 9
```

将现有命令式程序与索引 fold、enumerate-fold 变体比较。只有两个矩阵向量乘法函数不同。全部六个 native/wasm 制品的 stdout 与 stderr 必须一致。各制品先预热一次，再轮流采样；JSON 包含所有样本、中位数、相对命令式形式的比值及制品大小。测量包含进程启动，wasm 还包含运行时编译。小输入属于冒烟测试，不是稳态性能证据。

用 `--max-ratio R` 指定预算，任何比值超过它即失败。`scripts/check-spelling-ratio.sh` 正是这一机制的 CI 版（默认 2.0x，perf-ratchet 作业）。比值在一次轮流运行中比较同一路径的不同写法，因此共享 runner 较慢只会改变数字，不改变判定，与 native/Rust 比值门禁遵循相同纪律。native capture copy 改动落地后应降低预算，绝不能为了让失败构建通过而提高预算。本语料基准使用 `for` 和不断增长的输出列表，不是 issue 外部实验中预分配的 `while` 基准，因此不能把这些比值说成准确复现那些测量。各变体有意保持相同迭代顺序与浮点累加顺序。

同步标量 fold 会借用通过索引、显式借用或克隆被证明为只读的捕获项，且不存在逃逸的嵌套闭包。wasm 路径的标量 enumerate/fold 将源快照为标量列表，并在 callback 无法暴露 tuple 时复用一个私有 tuple 槽。native 路径上，SOURCE 位置的 `list.enumerate` 现在是链适配器（`.iter().cloned().enumerate()`），而非供下一阶段遍历并丢弃的 `Vec<(Int, T)>`；原先传给被消耗运行时调用的 clone 也随之消失。

`--release` 使用 opt-level 3 与 LTO；不带它时，`almide build` 使用项目默认的 opt-level 1（对正确性至关重要，见 `src/cli/cargo_build.rs`）。两个 profile 回答不同问题，因此下面都记录：默认 profile 是普通 `almide build` 的交付形态，release profile 则将 lowering 与 LLVM 本来就会清理的部分区分开。

2026-09-13，使用本地 v0.62.0 release build、Wasmtime 47.0.3，n=1200，预热后五个样本。六个制品的每次运行都输出 `1.274224150` 加换行，stderr 为空。

默认 profile（`almide build`，opt-level 1）：

| 目标 | 命令式 | 索引 fold | enumerate fold |
| --- | ---: | ---: | ---: |
| Native | 41.09 ms | 58.29 ms (1.42x) | 41.66 ms (1.01x) |
| Wasm | 93.20 ms | 118.51 ms (1.27x) | 96.12 ms (1.03x) |

Release profile（`almide build --release`，opt-level 3 + LTO）：

| 目标 | 命令式 | 索引 fold | enumerate fold |
| --- | ---: | ---: | ---: |
| Native | 47.95 ms | 48.23 ms (1.01x) | 48.14 ms (1.01x) |
| Wasm | — | 1.13x | 0.91x |

变化及其含义：

- **Native enumerate 从 1.68x 到 1.01x** 来自 source-adapter 融合。旧生成方式复制捕获的 1,200 元素向量，并为每行构建 `Vec<(i64, f64)>`，每次运行各 48,000 次；两者都消除了。
- **Native indexed 在默认 profile 为 1.42x，release 为 1.01x**；release 列说明原因：opt-level 3 下三种写法均收敛到约 48 ms。indexed 残余是 LLVM 在 opt-level 1 不对迭代器链做的优化，而不是 lowering 生成的负担。
- release 列不是推荐。同源轮流测量中，`--release` 对默认 profile 已处理良好的两种写法更慢（命令式 47.95 对 41.93 ms，enumerate 48.14 对 41.77 ms），仅对 indexed 更快（48.23 对 59.65 ms）。无论程序采用哪个 profile，各写法都应成本一致；这是语料门禁的性质。除 opt-level 1 的 indexed 外，两种 profile 现在都在测量噪声内满足它。
- Wasm 样本仍有较多噪声；这是本地比较测量，不是可移植的耗时保证。

2026-10-12 在 Linux 重新测量，develop `1df5391`，GCE c4-standard-8（x86_64）与 c4a-standard-8（aarch64、Neoverse-V2），n=1200，九个样本。表中为 native 路径各写法的 release 中位数 ÷ 默认中位数。两个 profile 通过 `compare.py --profile-pair` 轮流计时：

| 架构 | 命令式 | 索引 fold | enumerate fold |
| --- | ---: | ---: | ---: |
| x86_64 | 76.0 / 76.2 ms (1.00x) | 85.3 / 93.3 ms (0.91x) | 76.0 / 76.1 ms (1.00x) |
| aarch64 | 76.9 / 53.8 ms (**1.43x**) | 73.6 / 90.8 ms (0.81x) | 76.9 / 63.0 ms (**1.22x**) |

此次 aarch64 测量中，`--release` 对命令式和 enumerate 更慢，对索引 fold 更快（0.81x）。前两者的减速不是写法负担。按通用 aarch64 核心估算成本的 LLVM 循环向量化器，将 `eval_a` 的 i64 乘法分散到 NEON 通道。使用 `-C llvm-args=-vectorize-loops=false` 或 `-C target-cpu=native` 后减速消失（#3604）。写法之间的比值看不到所有写法共同承担的负担。门禁的 profile 路径将每种写法跨两个 profile 与自身比较；`perf-profile-arm` CI 作业在 `ubuntu-24.04-arm` 运行这一部分。

已于 2026-10-19 修复（#3604）。两个原因都具有一般性：

- **向量化器接纳了有序浮点归约。** Almide 的 `Float` `+` 是严格的，所以 `sum + eval_a(i, j) * v[j]` 为有序归约。x86 成本模型从不向量化它；AArch64 则启用，通用调优将 `eval_a` 的除法跨 NEON 通道配对，而 i64 乘法（NEON 没有 64 位向量乘法）和有序加法在周围逐通道执行（热循环中为 `fdiv v.2d`）。release build 现在对每个目标传入 `-C llvm-args=-force-ordered-reductions=false`（`src/cli/native_target.rs` 中的 `RELEASE_LLVM_ARGS`）：这些循环保持标量，整数归约和无归约循环照常向量化。对 x86，这是其成本模型原先就采取的做法。
- **fold 开头的范围字面量被实际构建了。** `list.fold(0..<n, …)` 每行实体化一个 `Vec<i64>`；从 `Vec` 读出的索引未向 LLVM 提供范围信息，所以 `v[j]` 的边界检查留在循环中。现在两条路径均像 `list.range(a, b)` 一样通过计数处理该字面量（#3810）。收集链（`list.map(0..<n, f)`）仍保留构建的头部；遇到任何机器都装不下的跨度时，仍以 C-197 的内存不足 abort 失败。

前 → 后，GCE，n=1200，21 个样本（hyperfine 中位数，ms），每个制品均输出 `1.274224150`：

| 架构 | Profile | 命令式 | 索引 fold | enumerate fold |
| --- | --- | ---: | ---: | ---: |
| aarch64 (Neoverse-V2) | release | 76.91 → 53.26 | 69.59 → 57.65 | 76.90 → 54.22 |
| aarch64 (Neoverse-V2) | default | 53.76 → 53.76 | 90.65 → 53.47 | 62.87 → 62.83 |
| x86_64 (Xeon 8581C) | release | 48.17 → 48.13 | 63.47 → 47.52 | 49.88 → 52.30 |
| x86_64 (Xeon 8581C) | default | 60.04 → 60.31 | 70.81 → 59.53 | 59.32 → 58.46 |

x86 enumerate release 前后机器码相同（该选项在那里不起作用，源码未变），两次读数体现此 VM 的运行间波动。aarch64 剩余问题是索引 fold 为命令式对应版的 1.08x：外层索引从已构建列表读取、内层通过计数时，LLVM 围绕已知范围的那个索引重新结合 `eval_a` 的 `ij * (ij + 1) / 2`（外层头部计数的 enumerate 中测得镜像的同一效应：另一构建只有一个 `madd` 的地方出现 `mul` + `sub`）。外层头部也计数时，indexed 测得 53.45 ms，enumerate 为 62.0 ms，而且会改变收集链的失败形式，因此不采纳。

对外层头部和 enumerate 索引计数，2026-10-20 测量后未采纳（#3604 方向 1）。测了两个候选 lowering：

- 将收集型范围字面量头部（`list.map(0..<n, f)`）通过计数填入一个按跨度预留的向量，并采用 C-197 abort。
- enumerate 适配器索引改为从 `0i64..i64::MAX` zip，而非转换 `.enumerate()` 的 `usize`。

Apple M4 Pro（aarch64-apple-darwin，宿主默认 `apple-m1` 调优），`--release`，n=5500，11 个轮流样本，中位数 ms，每个制品输出 `1.274224153`。“range”为 `list.map(list.range(0, n), (i) => list.fold(list.range(0, n), …))`，其头部原先就已计数：

| Profile | 命令式 | 索引 fold | enumerate fold | range fold |
| --- | ---: | ---: | ---: | ---: |
| release, develop | 727.8 | 694.1 (0.95x) | 710.6 (0.98x) | 712.0 (0.98x) |
| release, 两种 lowering | 730.7 | 709.6 (0.97x) | 707.7 (0.97x) | 714.9 (0.98x) |
| default, develop | 749.1 | 786.1 (1.05x) | 725.7 (0.97x) | 747.0 (1.00x) |
| default, 两种 lowering | 748.3 | 751.2 (1.00x) | 709.6 (0.95x) | 747.7 (1.00x) |

在 LLVM 的通用 aarch64 调优下，即 Linux aarch64 `--release` 构建采用的配置，外层计数有额外成本。相同输出 Rust 用 rustc `-C opt-level=3 -C codegen-units=1 -C panic=abort -C target-cpu=generic
-C llvm-args=-force-ordered-reductions=false` 构建，并在 M4 运行（n=3000，9 样本，ms）：

- 命令式：238.3。
- enumerate：develop 上 189.0，两种 lowering 一起为 234.5。
- indexed：develop 上 210.4，外层计数为 237.8。

逐项应用 enumerate 改动：仅 zip 索引为 188.9（中性），仅计数外层头部为 215.2。只测外层循环的 Rust 微基准中，通用调优下构建的 `Vec<i64>` 为 205 ms，而 `0..n` 上的 `extend`、`push`、`collect` 为 228 ms；`apple-m1` 下四者均为 207–212 ms。外层索引范围已知时，LLVM 对内层循环采用不同调度：使用 `i + j` 的 `madd`，而非强度削弱后的 `ij` 的 `mul`。

因此，在通用调优下，计数外层头部会使比命令式快 12–20% 的惯用写法降至与命令式相同速度，在宿主调优下则毫无收益。zip 索引在所有环境均中性。两者都不采纳：1.53x 本身已由 #3824 修复。实际落地的是原本已经计数的链的失败形式。`list.range(a, b) |> list.map(f)` 原先经 `.collect()` 收集，跨度超过任何机器容量时会以 `capacity overflow` panic；现在填充一个带 C-197 abort 的预留向量，与 `almide_rt_list_range` 和 wasm 路径一致（`tests/counted_index_chain_test.rs`）。Spectralnorm 不受影响。

为记录保留的早期读数：wasm 标量 enumerate/fold lowering 前，wasm enumerate 为 365.81 ms（3.21x），之后为 129.72 ms（1.17x）。native 标量 fold 捕获借用前，native indexed 为 97.39 ms（2.36x），之后为 58.28 ms（1.42x）。

> 原文注记：测量涉及不同日期、profile 与机器。早期默认 profile 表和附近轮流测量说明的读数不同（41.09/41.66/58.29 对 41.93/41.77/59.65 ms），均保留而不静默调和。后续更新及被否决方案不意味着所有早期观察至今仍成立。
