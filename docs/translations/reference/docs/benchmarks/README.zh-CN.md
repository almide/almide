<a id="what-is-measured"></a>

# 测量了什么

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](README.zh-CN.md)

> 固定原文的非规范读者快照。以原文为准，不自动同步或生成，不是机器消费者输入。未独立重跑 benchmark、proof check 或 CI。历史生成 marker 仅作为可见 metadata 展示，不执行其命令。
> [Original source (English) / 原文（英語）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/benchmarks/README.md)

> 原文注记：日期、数量、机器/模型条件及实现或 CI 状态均为历史原文主张，不是当前保证。原文本身记录了尚未解决的 contract-ledger 分歧。core 制品的证书检查不等于完整内存安全或 native 机器码证明。自包含 Wasm 仍需兼容 import 的 host；无 GC 不代表没有引用计数。旧有999函数/44module总数与9月38任务成绩卡均予保留，不与其他页面或后续 run 调和。即使本摘录未列出 STATUS 分类，仍保留原文对下方分类的提及。


本页每项主张要么由 script 导出，要么附带测量日期；`scripts/check-readme-numbers.sh` 在 CI 中拒绝无依据的裸数字，也拒绝超过 90 天或未注明来源 almide-dojo run 的 LLM-writability 成绩卡。`generated` marker 之间的表来自此目录已提交的 ledger（`*.txt`）及 `proofs/`；渲染 block 与 ledger 不一致时 CI 失败。不要手工编辑它们。marker 外 prose 的变更只运行 CI 文档检查；内部或 marker 本身的变更运行 full lane。方法与逐 benchmark 成绩表见 [docs/project/BENCHMARKS.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/project/BENCHMARKS.md)。

<a id="llm-writability"></a>

## LLM 编写能力

由 [almide-dojo](https://github.com/almide/almide-dojo) 于 2026-09-22 在其 38 任务库（basic / intermediate / advanced）上，用固定编译器 `almide 0.62.0`、通过该 repo 的 CI lane 测量。两次 run 都被 harness 标为 **`comparable`**：每个计划任务均到达模型，因此每个比率是点而非区间。二者均采用固定 seed（`20260922`）和 temperature 0 采样，run manifest 记录了 provider 实际发送的值。run 已提交——[`almide-dojo@8af34bc`](https://github.com/almide/almide-dojo/commit/8af34bc3)——因此下表可从其 `summary.md` 重算，而非只能相信；后续 run 见[实时 dashboard](https://almide.github.io/almide-dojo/)。按决策，CI 不放 Anthropic 或 OpenAI key，所以这里的模型是 lane 不用这些 key 就能访问的模型：

| 模型 | 通过率 | 1-shot（一次）通过率 |
|---|---|---|
| Llama 3.3 70B (fp8-fast) | 65% (25/38) | 39% (15/38) |
| Llama 3.1 8B | 44% (17/38) | 34% (13/38) |

最近的同模型比较是 MiniGit bench：2026-07-15 使用 almide 0.29.0、Sonnet 5 × 20 次试验（尚未在后续编译器上重跑），100% 通过、5 种语言中最精简（233 LOC）；agent 实际耗时快于 Gleam 和 MoonBit，却慢于 Rust 和 TypeScript（573 s 对 297 s）。这是在 6–9× 自我并行下测得的 LLM-writability 数值，**不是**生成 code 的速度（[图表](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/figures/lang-bench-snapshot-2026-07.png) · [方法](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/lang-bench/README.md) · [upstream](https://github.com/mame/ai-coding-lang-bench)）。

<a id="byte-identical-across-targets"></a>

## 跨 target 逐字节一致

**Cross-target 观察在各契约范围内比较。** 字节比较针对匹配输入和 host 假设下的执行输出，而非制品字节。随机性、platform 报告、压缩输出及相对 oracle 的 deadline 有已声明的不变量或比较条件，而非无限制相等。不支持操作的拒绝不算成功的 parity run。见[范围](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/EQUIVALENCE.md)。

这项主张不只是 prose。每个可观察承诺都对应[行为契约 ledger](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts)中一个命名契约，且可追溯到可执行证据；下方数字由 ledger 重新生成（`scripts/gen-claims.sh`，在 CI 中由 `scripts/check-contracts.sh` 强制）：

原文 claims 生成 block 开始：由 scripts/gen-claims.sh 从 docs/contracts/contracts.toml 导出。原文指令：不要编辑 marker 之间内容。

> 原文 counts 生成 block 开始（截至 2026-10-21）：来自 proofs/ledger-counts.toml 的带 stamp 总数；只由 scripts/gen-ledger-counts.sh 更新，绝不由 fixture/contract PR 更新。原文指令：不要编辑 marker 之间内容。

> **Ledger：388 个契约——388 个 active，0 个标记待修订。**

> 原文 counts 生成 block 结束。

>
> **标记待修订的契约：无。** 每个契约都标为 `active`；ledger gate 要求 class >= `fixture` 的可执行证据。
> 此 status 不会取消有范围限制的比较条件，也不能证明所有程序等价。
> 见 [C-189](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/contracts.toml)（platform 报告）、[C-208](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/contracts.toml)（相对 oracle 的 deadline）及 [C-331](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/contracts.toml)（依赖执行侧的压缩字节）。

原文 claims 生成 block 结束。


**Ledger 一致性注记：** `active` status 并不证明所有契约陈述一致。C-189 仍只描述两种 platform 相关例外和 `env.cwd` 的 Wasm wall，而 C-227 把 `env.cwd` 纳入已提供的 family，C-331 则声明依赖执行侧的压缩字节。这些陈述仍在[固定 contract ledger](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/contracts.toml)中。本页不解决该规范分歧：协调必须遵循 [judge-first 流程](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/scripts/check-als-pin.sh)，而非悄悄重写契约、pin 或生成 count。

范围、ledger 机制及证据栈（contract ledger、cross-target fixture gate、differential fuzz、emit-time Σ-probe、Lean belt、组织级 byte-verify sweep）：**[docs/design/EQUIVALENCE.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/EQUIVALENCE.md)**。

<a id="memory-safety--proven-where-it-is-proven-trusted-where-it-is-trusted"></a>

## 内存安全——已证明的归证明，受信任的归信任

无需编写 ownership 注解、lifetime 或 `free`：编译器放置 Perceus 风格的 ownership 操作。Rocq/Coq checker 证明的是已建模 witness 的性质，不是完整制品的内存安全或语义正确性。proof spine 有 137 个经审计定理；`proofs/check.sh` 将此数量与其 `Print Assumptions` audit 对照，并独立重检 spine。`almide verify <file.almd> --emit <bundle>` 现在记录结构化 Wasm bundle，由独立的 `almide-verify` 检查。受支持的 **core 制品**若被提供或在默认路径找到，可通过 SHA-256 绑定；此证书路径不支持 component 制品。native MIR witness 仍是单独的 proof/gate 输入，不是所有 native 机器码均已验证的证据。

记录侧的信任、未建模 access 及 coverage 限制仍存在。differential corpus 与 semantic-mutation gate 提供互补行为证据；`proofs/check-structural-bytes.sh` 对照 Coq decoder model 检查结构化 runtime。见[已证明与受信任部分](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/proven-vs-trusted.md)及[内存安全 gap 表](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/MEMORY-SAFETY.md)。

<a id="performance"></a>

## 性能

无 VM、无 GC、无 interpreter——native 经 Rust 编译为机器码，WASM 直接输出为自包含 module（链接程序可达的 runtime 支持，其余被裁剪）。

原文 wasm-size 生成 block 开始：由 scripts/gen-readme-stats.sh 从 docs/benchmarks/wasm-size.txt 渲染。原文指令：不要编辑 marker 之间内容。

| 程序 (`almide build --target wasm`, 交付状态) | develop | 已发布 v0.68.0-rc2 |
|---|---:|---:|
| Hello, world | **325 B** | **325 B** |

develop 列是 v0.68.0 打 tag 前、437b8f127 处的 develop build（`almide --version`：`almide 0.68.0 (dev, 437b8f127)`），测于 2026-10-20；CI 每次 push 都重建，若字节变化却未重打 `docs/benchmarks/wasm-size.txt` 的 stamp 就失败。released 列的编译器来自 v0.68.0-rc2 release asset（`almide 0.68.0 (release, fcba2bcd5)`，`gh release download v0.68.0-rc2 -R almide/almide`），测于 2026-10-20。交付字节未被事后 optimizer 处理（`--wasm-opt` 需主动启用，其输出不是 renderer 自身的 module）。

原文 wasm-size 生成 block 结束。


Rust 在同一 wasm target 上，即便充分针对大小优化，Hello, world 仍为 37 KB 以上：使用 `rustc 1.99.1`、`wasm32-wasip1`、`opt-level="z"`、`lto`、`strip`、`panic="abort"`、`codegen-units=1` 时为 37,541 B（默认 release profile 为 62,405 B），于 2026-10-20 在 GCE c4-standard-8 上按 [WASM-OUTPUT.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/wasm/WASM-OUTPUT.md) 底部配方测量。native CLI 形状 benchmark（`research/benchmark/perf/native/cli_app.almd`，由 `research/benchmark/perf/native/measure.sh` 执行的构建）在 x86_64 Linux（GCE c4-standard-8）上 strip 后为 507,960 B，在 aarch64 Linux（GCE c4a-standard-8）上为 463,720 B。2026-10-20 用 v0.68.0-rc2 release binary 构建，无 crate 依赖（只链接 C library——`libc`、`libgcc_s`）。逐字节分析：**[docs/wasm/WASM-OUTPUT.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/wasm/WASM-OUTPUT.md)**。

对照使用与 Almide 一侧相同 flag 编译的手写 Rust——release profile 加 native `--release` build 的 target CPU（x86_64 为 `x86-64-v3`，aarch64 为 generic CPU；`almide build --print-release-rustflags` 会打印）——使用 `bench.py`、full workload、5 次交错运行中位数。2026-10-20 用 v0.68.0-rc2 release binary（`almide 0.68.0 (release, fcba2bcd5)`）在 GCE c4-standard-8（x86）/ c4a-standard-8（arm）测得：n-body **0.94×** / **1.02×**、spectral-norm **1.00×** / **0.99×**、fasta **1.16×** / **1.10×**、FFT **0.88×** / **0.94×**、wordfreq **0.73×** / **0.59×**（8M 次抽取，即 ratchet 规模；2M 低于 0.08 s 启动噪声下限）、listbuild **0.87–0.89×** / **0.93–0.95×**（[成绩表](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/project/BENCHMARKS.md#runtime-scoreboard)）。当 Almide 拥有 Rust 没有的信息——例如 effect system 证明整个生命周期局限于一个 `check(make(depth))` 表达式的树——它比同一程序的普通 Rust 更快：

原文 native-victory 生成 block 开始：由 scripts/gen-readme-stats.sh 从 docs/benchmarks/native-victory.txt 渲染。原文指令：不要编辑 marker 之间内容。

| Workload（工作负载） (`bench.py` 方法，9次交错运行中位数) | 优化 | Almide / 普通 Rust, x86 | Almide / 普通 Rust, arm | 关闭后 (`ALMIDE_REGION_OFF=1` / `ALMIDE_FAN_SEQUENTIAL=1`), x86 / arm |
|---|---|---:|---:|---:|
| binarytrees | region window（区域处理窗口） (#1991) | **0.28 (d17)** / **0.27 (d19)** | **0.33 (d17)** / **0.32 (d19)** | 0.89 / 1.04 |
| treealloc | region window（区域处理窗口） (#1991) | **0.22 (d20)** / **0.24 (d21)** | **0.27 (d20)** / **0.28 (d21)** | 0.91 / 1.03 |
| fannkuchredux | 并行 fan (#2044) | **0.25 (n10)** / **0.23 (n11)** | **0.21 (n10)** / **0.18 (n11)** | 0.97 / 1.02 |

于 2026-10-20 使用从 main fcba2bcd5 构建的 v0.68.0-rc2 release asset（almide-linux-x86_64 / almide-linux-aarch64，`almide 0.68.0 (release, fcba2bcd5)`）测量。x86 = GCE c4-standard-8（Intel Xeon Platinum 8581C、8 vCPU），arm = GCE c4a-standard-8（Neoverse-V2、8 vCPU）。参考实现为 rustc 1.99.1，release flag 如 `almide build --print-release-rustflags` 所报告（x86 为 -C target-cpu=x86-64-v3；两侧均有 -C llvm-args=-force-ordered-reductions=false）。每格两个比率对应两种输入规模（规模写在格内）；Rust 一侧是人通常会写的程序——每节点一个 `Box`、1 个 thread、无 arena、无 `unsafe`、无 SIMD——用与 Almide 一侧相同的 `rustc` flag 编译。“关闭后”列为相同 Almide source 仅关闭该项优化，在较大规模下测得。`perf-ratchet` job 在行达到 1.0 或消融比较不再显示优化收益时失败。声明与方法：[docs/project/BENCHMARKS.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/project/BENCHMARKS.md#faster-than-ordinary-rust-1330)。Ledger：`docs/benchmarks/native-victory.txt`。

原文 native-victory 生成 block 结束。


原文 build-speed 生成 block 开始：由 almide-gates 的 `bench` subcommand 从 docs/benchmarks/build-speed.txt 导出。原文指令：不要编辑 marker 之间内容。

测于 almide 0.68.0 (dev)、x86_64 Linux、INTEL(R) XEON(R) PLATINUM 8581C CPU @ 2.30GHz [GCE c4-standard-8]，`examples/lisp.almd`（297 行），2026-10-19。每行均为 N 次运行的**均值**；30ms process 的单次运行只是 scheduler noise。cold 每轮前清除 `$TMPDIR/almide-run` 与 dependency cache **两者**，只清后者测到的是 warm build。以 `almide run tools/almide-gates/src/main.almd -- bench` 重新生成。ratchet（`-- bench docs/benchmarks --check`）在上述机器 class 达到 1.5x 时失败，其他机器则明确说明并跳过。

| 场景 | 时间 | 次数 |
|---|---|---|
| `almide check` | **25.2 ms** | 20 |
| build，warm（content-cache 命中） | **92.9 ms** | 5 |
| build，cold | **4846.1 ms** | 3 |
| build，cold, `--target wasm` | **148.7 ms** | 3 |

原文 build-speed 生成 block 结束。


`almide check` 接近线性扩展：在本 repo 自身 stdlib 的 2k → 33k 行阶梯上，check 时间相对 project 行数的 log-log 斜率两台机器均为 **1.10**（1.0 为线性、2.0 为二次），10.7k 行级别耗时为空 project 下限的 **4.2×**（x86）/ **4.3×**（arm）。2026-10-20 用 v0.68.0-rc2 release binary 在 GCE c4-standard-8 / c4a-standard-8 上测得（`research/benchmark/editloop/scale.py --runs 40`），由 `scripts/check-edit-loop-scale.sh` 维持，表见 [BENCHMARKS.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/project/BENCHMARKS.md#edit-loop-scale-1334)。`perf-ratchet` job（`scripts/check-perf-ratio.sh`）每次 develop CI 都重测所有 native 行；anchored 行（n-body、spectral-norm、fasta、FFT、wordfreq）超出已提交 baseline 40% 以上则失败。已测量并门控的 Wasm runtime（#1701）：

原文 wasm-runtime 生成 block 开始：由 scripts/gen-readme-stats.sh 从 docs/benchmarks/wasm-runtime.txt 渲染。原文指令：不要编辑 marker 之间内容。

| Benchmark (`almide bench`, 先验证再计时，2×5次交错运行的最小值) | wasm/native，仅 `main` | cold start（启动 对 compile + instantiate） |
|---|---:|---:|
| nbody | **1.21×** | 1.41× |
| spectralnorm | **1.18×** | 1.27× |
| binarytrees | **1.20×** | 1.34× |
| treealloc | **1.08×** | 1.13× |
| fasta | **0.90×** | 1.00× |
| fannkuchredux | **1.37×** | 1.48× |
| mandelbrot | **1.21×** | 1.30× |
| onebrc | **1.90×** | 2.56× |
| fft | **1.42×** | 1.52× |
| strchurn | **0.92×** | 0.95× |
| listbuild_append | **1.77×** | 1.98× |
| listbuild_combinator | **1.55×** | 1.74× |
| listbuild_prealloc | **1.36×** | 1.53× |
| mapbuild | **0.92×** | 1.08× |

embedded wasm host（linear memory 中的 Perceus RC）与 native binary 在同一机器、同次运行中比较。比率测量两侧程序自身 `main` 从进入到返回（native 在 process 内，wasm 在 host call 前后）；process 启动、module compile/instantiate 不在其中，cold-start 列显示它们（#2980）。小 workload 按 ledger 固定规模（`args=`）运行，使 `main` 足够长可计时。cross-engine 比率**不会**消除硬件因素（2-core CI runner 测得的 nbody 约差 10x），因此带 stamp 的比率判定在 stamp 所属机器 class 上运行；CI 门控下述 STATUS 分类，并通过相同 runner 与最新 release binary 的 A/B 比较判断 wasm 侧（交错执行、run 最小值、ledger 中 `ab_band`——#2143）（`scripts/check-wasm-runtime-ratio.sh`）。这些行只有 fannkuchredux 的 `fan` 并行：native 使用 thread（#2044），embedded wasm host 使用 module 的不同 instance（#3003、ADR-0011 §D2a；其他所有 wasm host 都顺序执行，字节相同）；binarytrees 和 mandelbrot 的 `fan.map` callback 在两侧都顺序运行（两侧 user time 均等于 wall time，测于 2026-10-03）。未测 corpus cell 保持诚实而非估算：wasm build path 上 0 个 wall、0 个耗尽 embedded heap（#1729），每次 gate 都重测，因此开始能够计时的 cell 会使 gate 失败，直到该行被提升。Ledger：`docs/benchmarks/wasm-runtime.txt`（带 version 0.68.0 的 develop build，不是 0.68.0 release；2026-10-19，在 x86_64 Linux、INTEL(R) XEON(R) PLATINUM 8581C CPU @ 2.30GHz [GCE c4-standard-8] 上测量）。

原文 wasm-runtime 生成 block 结束。


<a id="derived-counts"></a>

## 导出计数

[Project status](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/status.md) 背后的带 stamp 总数，以及 gate net 的 mutation score。

原文 stats 生成 block 开始：由 scripts/gen-readme-stats.sh 从 docs/stdlib/*.md、spec/ 和 docs/contracts/contracts.toml 导出。原文指令：不要编辑 marker 之间内容。

原文 counts 生成 block 开始（截至 2026-10-21）：来自 proofs/ledger-counts.toml 的带 stamp 总数；只由 scripts/gen-ledger-counts.sh 更新，绝不由 fixture/contract PR 更新。原文指令：不要编辑 marker 之间内容。

| 导出计数 | 值 |
|---|---|
| Stdlib | 999 个函数，44个module——自举 `.almd`, signature index 由以下工具从 compiler 重新生成： `tools/gen-stdlib-doc-index.py` |
| 测试 | 514 个 `.almd` 测试文件，位于 `spec/` (`almide test spec/`) + 388契约的 cross-target ledger |

原文 counts 生成 block 结束。

原文 stats 生成 block 结束。


原文 mutation-score 生成 block 开始（截至 2026-09-22）：由 scripts/gen-mutation-score.sh 从 proofs/mutation-score.toml 打 stamp；每次 mutation-sweep run 重测，仅由 `--from-run` 更新。原文指令：不要编辑 marker 之间内容。

**Mutation score**——捕获 41/41 个 mutant（100.0 %），0 个存活、0 个 stale：`ci/mutations/` 的完整 release-shape net sweep（[`scripts/check-mutation-gate.sh`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/scripts/check-mutation-gate.sh)），于 2026-09-22 从 `3b02f7dc4` 的 mutation-sweep run [35650396971](https://github.com/almide/almide/actions/runs/35650396971) 打 stamp。

原文 mutation-score 生成 block 结束。

