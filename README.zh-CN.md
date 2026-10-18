<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="./docs/assets/brand/almide-cover-dark.png">
    <img src="./docs/assets/brand/almide-cover.png" alt="Almide" width="720">
  </picture>
</p>

<p align="center"><strong>当模型出错时，编译器会点名该怎么改。</strong></p>

<p align="center">
  <a href="https://github.com/almide/almide/actions/workflows/ci.yml"><img src="https://github.com/almide/almide/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="./LICENSE"><img src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blue.svg" alt="License: MIT / Apache-2.0"></a>
  <a href="https://deepwiki.com/almide/almide"><img src="https://img.shields.io/badge/Ask-DeepWiki-blue.svg" alt="Ask DeepWiki"></a>
</p>

<p align="center">
  <a href="https://almide.github.io/playground/">Playground</a> ·
  <a href="./docs/CHEATSHEET.md">速查表</a> ·
  <a href="./docs/SPEC.md">规范</a> ·
  <a href="#为什么选-almide">为什么</a> ·
  <a href="#快速开始">快速开始</a> ·
  <a href="#测量了什么">证据</a> ·
  <a href="#工作原理">工作原理</a> ·
  <a href="#项目状态">状态</a>
</p>

<p align="center">
  <a href="README.md"><img alt="English" src="https://img.shields.io/badge/English-d0d7de?style=flat-square"></a>
  <a href="README.ja.md"><img alt="日本語" src="https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square"></a>
  <a href="README.zh-CN.md"><img alt="简体中文" src="https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square"></a>
</p>

---

## 活下来的一次修改

Almide 是一门面向「由 AI 编写、由 AI 修改的代码」的静态类型语言。它的主张不是「模型能写对」，而是：**当模型写错时，编译器在编写阶段就指出来，并点名该怎么改；当模型写对时，一个在 Rocq 中证明过的检查器会证明该程序内存安全。** `almide verify` 把程序的所有权、名称与能力见证交给独立的 `almide-verify`（针对所发布 wasm 字节本身的证书仍在推进中，#2755–#2760）。它同时编译为原生二进制（经由 Rust）和 WebAssembly，两者产生逐字节相同的输出。

一屏之内看懂前半句。模型给一个类型加了一个分支，并且像模型常做的那样，没有动别的地方：

```almd
type Shape =
  | Circle(Float)
  | Square(Float)
  | Triangle(Float, Float)   // 这次修改

fn area(s: Shape) -> Float =
  match s {
    Circle(r) => 3.14159 * r * r
    Square(w) => w * w
  }
```

```text
error[E010]: non-exhaustive match: missing Triangle(_, _)
  --> shape.almd:7:9
  in match
  here: match s {
  hint: add arms for Triangle(_, _):
  Triangle(arg1, arg2) => _
Or use `_ => todo()` to compile incrementally.
```

编译器在出问题的位置点名缺失的分支，写出该补的 arm，并给出一条在其余部分尚未写完时继续编译的路。模型的下一步就是 `Triangle(b, h) => 0.5 * b * h`；程序随后在原生和 wasm 上都能运行，并打印相同的字节。一次修改、一条本身即是修复的诊断、一次通过的构建 —— 下面每一个决定都在为这个循环服务。把同一个问题拿去问 Python，用八处注入的修改错误：[demo/make-verify](./demo/make-verify/)。

## 为什么选 Almide

- **可预测** —— 每个概念只有一种规范写法，减少 LLM 的 token 分支
- **局部** —— 读懂任何一段代码，只需要它附近的上下文
- **可修复** —— 诊断指向一个具体的修法，而不是若干种可能（如上例）
- **紧凑** —— 语义密度高，语法噪声低

完整理据见 [Design Philosophy](./docs/design/DESIGN.md)。冻结的表面与破坏性变更策略见 [STABILITY.md](./docs/STABILITY.md)（2026-08-20 宣布）—— 速查表和 `llms.txt` 里写着的东西，含义不会改变。

## 快速开始

**[在浏览器里试 →](https://almide.github.io/playground/)** —— 无需安装。

```bash
curl -fsSL https://raw.githubusercontent.com/almide/almide/main/tools/install.sh | sh   # macOS / Linux
irm https://raw.githubusercontent.com/almide/almide/main/tools/install.ps1 | iex        # Windows (PowerShell)
```

安装脚本在解包前会用该版本的 `almide-checksums.sha256` 校验压缩包。若要自己验证下载的产物 —— 包括校验和文件在内的每一个发布产物，都由发布工作流用 Sigstore 做了证明（见 [SECURITY.md](./SECURITY.md)）：

```bash
gh attestation verify almide-macos-aarch64.tar.gz -R almide/almide   # 来源: 由 almide/almide 的发布工作流构建
sha256sum -c --ignore-missing almide-checksums.sha256                # 摘要与已发布的校验和一致
```

每个压缩包还带有 `almide-verify`，一个独立版本化的证书检查器：`almide verify app.almd` 会输出该程序的所有权 / 名称 / 能力 / 调用模式见证，并交给它（它必须与 `almide` 相邻或位于 `PATH` 上 —— 没有内置回退）。

从源码构建需要 [Rust](https://rustup.rs/) 1.96+（即 `almide` 包的 `rust-version`，因为二进制内嵌 wasmtime 宿主；CI 用 1.96.0 构建）：`cargo build --release && cp target/release/almide target/release/almide-verify ~/.local/bin/`（或 `make install`）。

```almd
fn main() -> Unit = {
  println("Hello, world!")
}
```

```bash
almide run hello.almd                 # 原生
almide run hello.almd --target wasm   # 相同字节，跑在 wasmtime 上
```

## 特性

- **多目标** —— 同一份源码编译为原生二进制（经由 Rust）或 WebAssembly（直接产出，不经 LLVM）
- **泛型** —— 函数（`fn id[T](x: T) -> T`）、记录、variant 类型、自动 Box 包装的递归 variant
- **模式匹配** —— 带 variant 解构的穷尽 `match`
- **effect 函数** —— 用 `effect fn` 做显式错误传播：`expr!` 传播，裸的可失败调用是错误，绝不静默
- **双向类型推导** —— 注解流入表达式（`let xs: List[Int] = []`）
- **Codec** —— `Type.decode(value)` / `Type.encode(value)`，自动派生
- **Map 字面量** —— `["key": value]`、`m[key]`、`for (k, v) in m`
- **Fan** —— 结构化并发：`fan { a(), b() }` 在原生上让每个 arm 各占一个线程，在 wasm 上依次执行；`fan.map` / `fan.settle` 在两端都按列表顺序返回结果，`fan.map` 会运行每一个元素并给出下标最小的 `Err`（dialect epoch 7），原生上只有作用于标量的纯回调才会用线程（effect 回调一次只跑一个元素）。原生上每个 arm 和元素都写入自己的输出时间线，按列表顺序冲刷，所以并发 arm 的打印在两个目标上都按源码顺序出现，trap 会等待它之前的元素（[ADR-0024](./docs/adr/0024-fan-effect-callbacks-run-concurrently-as-if-sequential.md) 第 1、3 步，C-004、C-005、C-200）。`fan.min(compute.ms(n))` 选出耗费确定性计算量最少的 arm，因此两个目标上的胜者相同。按顺序取第一个 `Ok` 不是 fan 形式：写 `a() ?? b()`，对列表则用 `list.find_map`（dialect epoch 14 删除了 `fan.any`，并把 `fan.race` 改名为 `fan.min`）
- **任何文字都能作名字** —— 标识符遵循 Unicode UAX #31，以 NFC 保存（`let 合计 = 价格 * 数量`）；关键字仍是 ASCII
- **管道运算符** —— `data |> transform |> output`
- **模块系统** —— 包、子命名空间、可见性控制、菱形依赖解析
- **标准库** —— 自举的 `.almd` 模块：string、list、map、json、http、fs 等（[参考](./docs/stdlib/)；数量在[项目状态](#项目状态)中导出）。领域内核放在包里而不在语言里：LLM 提供方客户端在 [almide/llm](https://github.com/almide/llm)，神经网络矩阵内核在 [almide/nnlib](https://github.com/almide/nnlib)（dialect epoch 14）
- **内建测试** —— `test "name" { assert_eq(a, b) }`，用 `almide test` 运行；必须中止的调用用 `testing.assert_aborts(expected, () => …)`

## 测量了什么

本节的每一条主张，要么由脚本导出，要么带着测量日期。`scripts/check-readme-numbers.sh` 会在 CI 中拒绝裸的数字，也会拒绝超过 90 天、或者没有点名其来源 almide-dojo run 的 LLM 可写性记分卡。

### LLM 可写性

由 [almide-dojo](https://github.com/almide/almide-dojo) 于 2026-09-22 测得，覆盖其 38 个任务（basic / intermediate / advanced）的题库，固定编译器版本 `almide 0.62.0`，由该仓库的 CI 通道执行。两次 run 都被 harness 标记为 **`comparable`** —— 计划中的每个任务都送达了模型，所以每个比率是一个点而不是区间 —— 并且都在固定种子（`20260922`）和温度 0 下采样，按提供方实际发出的参数记录在 run 的 manifest 里。这些 run 已提交（[`almide-dojo@8af34bc`](https://github.com/almide/almide-dojo/commit/8af34bc3)），因此下表可以从它们的 `summary.md` 重新算出，而不必相信。后续 run 见[实时看板](https://almide.github.io/almide-dojo/)。出于决定，CI 中不放 Anthropic 或 OpenAI 的密钥，所以这里列出的是该通道无需密钥即可访问的模型：

| Model | Pass Rate | 1-Shot Rate |
|---|---|---|
| Llama 3.3 70B (fp8-fast) | 65% (25/38) | 39% (15/38) |
| Llama 3.1 8B | 44% (17/38) | 34% (13/38) |

最近一次同模型对比是 MiniGit 基准：2026-07-15，Sonnet 5 × 20 次试验，almide 0.29.0（之后的编译器上没有重跑），100% 通过，在 5 门语言中最简洁（233 行），agent 墙钟时间快于 Gleam 和 MoonBit，但慢于 Rust 和 TypeScript（573 秒对 297 秒）—— 这是在 6–9 倍自并行下测得的 **LLM 可写性**数字，**不是**生成代码的速度（[图](docs/figures/lang-bench-snapshot-2026-07.png) · [方法](research/benchmark/lang-bench/README.md) · [上游](https://github.com/mame/ai-coding-lang-bench)）。

### 跨目标逐字节相同

**每一个能为两个目标编译的程序，无论作为原生二进制还是 WebAssembly 运行，其可观测输出 —— stdout、stderr、退出码 —— 都逐字节相同。** 原生是基准；`native == wasm` 是硬不变量，而不是一处需要在文档里绕开的"目标差异"。

这个保证是**持续的，并且范围由台账明确管理**："逐字节相同"指的是执行输出，不是编译产物；本质不确定的来源改为证明确定性的*不变量*而非精确字节；尚未在 wasm 上实现的 API 是编译期或运行期的*拒绝* —— 绝不会是错误的字节；恰好两个 fn 因其职责就是报告宿主而被豁免 —— `env.os()` 和 `env.temp_dir()`，由 C-189 圈定，因为让它们跨目标一致才是缺陷而非保证。并发 `fan { }` arm 打印的输出也在保证之内：原生上每个 arm 写入自己的时间线，按源码顺序冲刷，因此读起来与 wasm 上一样（C-004，ADR-0024 第 1 步）。

这条主张不是散文。每一个可观测的承诺都是[行为契约台账](docs/contracts/)中一条具名契约，各自可追溯到可执行的证据，下面的数字由台账再生成（`scripts/gen-claims.sh`，CI 中由 `scripts/check-contracts.sh` 强制）：

<!-- claims:generated:start — derived from docs/contracts/contracts.toml by scripts/gen-claims.sh; DO NOT EDIT between the markers -->
> <!-- counts:generated:start (as of 2026-10-10) — stamped totals from proofs/ledger-counts.toml; refreshed only by scripts/gen-ledger-counts.sh, never by a fixture/contract PR; DO NOT EDIT between the markers -->
> **Ledger: 376 contracts — 376 active, 0 flagged-for-revision.**
> <!-- counts:generated:end -->
>
> **Divergences awaiting a fix: none.** Every contract in the ledger is
> `active`, carrying executable evidence of class >= `fixture`. The one
> by-design carve-out in the law — the platform-reporting fns `env.os`
> and `env.temp_dir` — is bounded by C-189.
<!-- claims:generated:end -->

范围、台账机制与证据栈（契约台账、跨目标 fixture 门、差分模糊测试、产出期 Σ 探针、Lean 带、组织级逐字节校验扫描）：**[docs/design/EQUIVALENCE.md](./docs/design/EQUIVALENCE.md)**。

### 内存安全 —— 证明之处以证明，信任之处以信任

你不写所有权注解、不写生命周期、不写 `free`：编译器里 [Perceus](https://www.microsoft.com/en-us/research/publication/perceus-garbage-free-reference-counting-with-reuse/) 式的所有权推导决定每个堆值在何处产生、复制与消费 —— 无垃圾回收，无停顿。这些决定的检查器是**内核级证明过的**（Rocq/Coq 主干，100 条经审计的定理与引理，公理干净，由 `coqchk` 独立复检；数量由 `proofs/check.sh` 断言），`almide verify --emit` 仍会产出它所检查的 MIR 所有权见证。每次构建的证书过去搭载在 incumbent wasm leg 上，而 #2761 删除了它；**structural wasm leg**（唯一的 wasm 渲染器）与**原生 leg** 是被信任的，证书仍在推进中（#2755–#2760）：它们的证据是差分的 —— 在契约语料上彼此之间、以及与解释器之间逐字节相同的输出，由只增不减的下界和语义变异网维持；structural 运行时的字节由 `proofs/check-structural-bytes.sh` 对照 Coq 解码器模型检查。逐阶段的边界见 **[proven-vs-trusted.md](docs/contracts/proven-vs-trusted.md)**；包含设计起点 Lean 4 Perceus 带在内的完整说明见 **[docs/design/MEMORY-SAFETY.md](./docs/design/MEMORY-SAFETY.md)**。

### 性能

没有 VM、没有 GC、没有解释器 —— 原生经 Rust 编译为机器码，WASM 作为自包含模块直接产出（程序用到的运行时支持会被链接进来，其余被裁掉）。

<!-- wasm-size:generated:start — rendered from docs/benchmarks/wasm-size.txt by scripts/gen-readme-stats.sh; DO NOT EDIT between the markers -->
| Program (`almide build --target wasm`, as shipped) | develop | released v0.66.0 |
|---|---:|---:|
| Hello, world | **325 B** | **953 B** |

The develop column is the develop build at 5f39c06ba, after the v0.66.0 release (`almide --version`: `almide 0.66.0 (dev, 5f39c06ba)`), measured 2026-10-03; CI rebuilds it on every push and fails if the bytes move without a restamp of `docs/benchmarks/wasm-size.txt`. The released column is the compiler from the v0.66.0 release asset (`almide 0.66.0 (release, 819bbc74f)`, `gh release download v0.66.0 -R almide/almide`), measured 2026-10-03. No post-hoc optimizer touches the shipped bytes (`--wasm-opt` is opt-in and its output is not the renderer's own module).
<!-- wasm-size:generated:end -->

同一 wasm 目标下的 Rust，即便把体积调到极致，Hello, world 也要 40 KB 以上：`rustc 1.96.1`、`wasm32-wasip1`、`opt-level="z"`、`lto`、`strip`、`panic="abort"`、`codegen-units=1` 时为 40,379 B（默认 release 配置为 64,844 B），2026-10-03 按 [WASM-OUTPUT.md](./docs/wasm/WASM-OUTPUT.md) 末尾的步骤测得。原生 CLI 形态的基准（`research/benchmark/perf/native/cli_app.almd`，由 `research/benchmark/perf/native/measure.sh` 构建）在 macOS arm64 上 strip 后为 464,488 B，没有 crate 依赖（只链接 `libSystem`），2026-10-03 用 develop 构建和 v0.66.0 发布版分别测得。逐字节的解剖（2026-07-23 测得，也涵盖已由 #2761 退役的 incumbent leg）：**[docs/wasm/WASM-OUTPUT.md](./docs/wasm/WASM-OUTPUT.md)**。

对比手写 Rust，算术内核持平：n-body 与 spectral-norm 在 Apple M4 Pro 上为 1.00×（2026-07-30），在 ubuntu-latest CI runner 上为 1.065× 与 1.032×（develop CI run [37092258733](https://github.com/almide/almide/actions/runs/37092258733)，2026-10-03）。当 Almide 掌握 Rust 所没有的信息时 —— 一棵树的整个生命周期就是一个 `check(make(depth))` 表达式，这一点由 effect system 证明 —— 它比同一程序的普通 Rust 更快：

<!-- native-victory:generated:start — rendered from docs/benchmarks/native-victory.txt by scripts/gen-readme-stats.sh; DO NOT EDIT between the markers -->
| Workload (`bench.py`, median of 9, interleaved) | optimization | Almide / ordinary Rust, M4 Pro | without it (`ALMIDE_REGION_OFF=1` / `ALMIDE_FAN_SEQUENTIAL=1`), M4 Pro | ubuntu-latest CI runner |
|---|---|---:|---:|---:|
| binarytrees | region window (#1991) | **0.35 (d17)** / **0.32 (d19)** | 1.25 | 0.581 (d17) |
| treealloc | region window (#1991) | **0.30 (d20)** / **0.30 (d21)** | 1.10 | 0.365 (d20) |
| fannkuchredux | parallel fan (#2044) | **0.21 (n10)** / **0.12 (n11)** | 1.06 | 0.315 (n11) |

Two ratios per row are the two input sizes (the win holds at both); the Rust side is the ordinary program a person writes for it — a `Box` per node, one thread, no arena, no `unsafe`, no SIMD — compiled with the same `rustc` flags, and the "without it" column is the same Almide source with the region window turned off, so the whole gap is that one optimization. The absolute ratio is allocator-dependent (the CI runner frees a `Box` cheaper), the direction is not: the `perf-ratchet` job fails if either row reaches 1.0 or the ablation stops paying. Declaration and methodology: [docs/project/BENCHMARKS.md](./docs/project/BENCHMARKS.md#faster-than-ordinary-rust-1330). Ledger: `docs/benchmarks/native-victory.txt`. The M4 Pro columns were measured on almide 0.62.0, 2026-09-08 and have not been re-measured since; the runner column is what the `perf-ratchet` job of develop CI run [37092258733](https://github.com/almide/almide/actions/runs/37092258733) printed at `466cac59b` (a develop build carrying version 0.66.0, not the 0.66.0 release), 2026-10-03 (the size is in each cell).
<!-- native-victory:generated:end -->

<!-- build-speed:generated:start — derived from docs/benchmarks/build-speed.txt by the almide-gates `bench` subcommand; DO NOT EDIT between the markers -->
Measured on almide 0.59.1, arm64 Darwin, `examples/lisp.almd` (268 lines), 2026-08-27. Every row is an N-run MEAN —
a single run of a 30ms process is scheduler noise. Cold clears BOTH `$TMPDIR/almide-run`
and the dependency cache before each repetition; clearing only the latter measures a warm
build. Regenerate with `almide run tools/almide-gates/src/main.almd -- bench`; the ratchet
(`-- bench --check`) fails CI at 1.5x.

| scenario | time | runs |
|---|---|---|
| `almide check` | **15.2 ms** | 20 |
| build, warm (content-cache hit) | **237.2 ms** | 5 |
| build, cold | **635.3 ms** | 3 |
| build, cold, `--target wasm` | **61.7 ms** | 3 |
<!-- build-speed:generated:end -->

`almide check` 接近线性扩展：在本仓库自身 stdlib 构成的 2k → 30k 行阶梯上，检查时间对项目行数的双对数斜率为 **1.13**（1.0 为线性，2.0 为二次），10k 行那一级是空项目下界的 **4.4 倍** —— 2026-08-13 测得，由 `scripts/check-edit-loop-scale.sh` 保持，表见 [BENCHMARKS.md](./docs/project/BENCHMARKS.md#edit-loop-scale-1334)。相对手写 Rust 的原生运行时间，在 Apple M4 Pro 上（2026-07-30，FFT 于 2026-08-13 重测）：n-body 与 spectral-norm 为 **1.00×**，fasta 为 1.16×，FFT 为 1.18×（[记分板](./docs/project/BENCHMARKS.md)）。listbuild 各行在那里是 1.47–1.69×，在 CI runner 上是 0.87–1.05×（2026-10-03）；它们在 M4 Pro 上的差距来自逐字节相同所需的确定性软件 `sin`/`cos`，而不是列表构建（2026-08-13 测得，`scripts/perf-ratio-baseline.txt`）。ubuntu-latest runner 在每次 develop CI run 上重测所有行，`perf-ratchet` 任务会在锚定行（n-body、spectral-norm、fasta、FFT、wordfreq）偏离已提交基线超过 40% 时失败（run [37092258733](https://github.com/almide/almide/actions/runs/37092258733)，2026-10-03：fasta 0.757×，FFT 1.002×）。wasm 运行时间亦已测量并设门（#1701）：

<!-- wasm-runtime:generated:start — rendered from docs/benchmarks/wasm-runtime.txt by scripts/gen-readme-stats.sh; DO NOT EDIT between the markers -->
| Benchmark (`almide bench`, verify-then-time, min of 2×5 interleaved) | wasm/native, `main` only | cold start (spawn vs compile + instantiate) |
|---|---:|---:|
| nbody | **1.10×** | 1.14× |
| spectralnorm | **1.21×** | 1.22× |
| binarytrees | **1.07×** | 1.06× |
| treealloc | **1.04×** | 1.04× |
| fasta | **1.52×** | 1.50× |
| fannkuchredux | **1.28×** | 1.28× |
| mandelbrot | **1.11×** | 1.10× |
| onebrc | **1.26×** | 1.33× |
| fft | **1.53×** | 1.53× |
| strchurn | **0.85×** | 0.85× |
| listbuild_append | **1.96×** | 1.95× |
| listbuild_combinator | **1.88×** | 1.86× |
| listbuild_prealloc | **1.68×** | 1.68× |
| mapbuild | **0.67×** | 0.72× |

Embedded wasm host (Perceus RC in linear memory) against the native binary, same machine, same run. The ratio times the program's own `main`, entry to return, on both legs (native in-process, wasm around the host call): process spawn and module compile/instantiate are outside it, and the cold-start column shows them (#2980). Small workloads run at a ledger-fixed size (`args=`) so `main` is long enough to time. Cross-engine ratios do NOT cancel hardware (a 2-core CI runner measures nbody ~10x worse), so the stamped ratio verdict runs on the stamping machine class; CI gates the STATUS taxonomy below and judges the wasm leg by a same-runner A/B against the latest release binary (interleaved, min-of-runs, `ab_band` in the ledger — #2143) (`scripts/check-wasm-runtime-ratio.sh`). Of these rows only fannkuchredux runs its `fan` in parallel: native on threads (#2044), the embedded wasm host on separate instances of the module (#3003, ADR-0011 §D2a; every other wasm host runs it sequentially, byte-identical); binarytrees' and mandelbrot's `fan.map` callbacks run sequentially on both legs (user time equals wall time on both, measured 2026-10-03). The unmeasured corpus cells stay honest instead of estimated: 0 wall on the wasm build path, 0 exhaust the embedded heap (#1729) — each re-measured every gate run, so a cell that starts benching fails the gate until its row is promoted. Ledger: `docs/benchmarks/wasm-runtime.txt` (a develop build carrying version 0.65.1, not the 0.65.1 release, 2026-09-29).
<!-- wasm-runtime:generated:end -->

## 工作原理

一个前端、一个 IR、每个目标一个渲染器：

```mermaid
flowchart LR
    SRC([".almd"]) --> FE["Lexer → Parser → Type Checker → Lowering"] --> IR(["IR"])
    IR --> NANO["Nanopass Pipeline<br/>semantic rewrites"] --> TMPL["Template Renderer<br/>TOML-driven"] --> RS([".rs → native binary"])
    IR --> STRUCT["structural leg<br/>commissioned engine, direct emit"] --> WASM([".wasm"])
```

**原生。** Nanopass 流水线施加目标相关的变换 —— `ResultPropagation`（Rust 的 `?`）、`CloneInsertion`（Rust 借用分析）、`LICM`（循环不变量外提）。Template Renderer 纯粹是语法性的：所有语义决定在 IR 阶段已经确定。

**WebAssembly。** **structural leg** —— 在 [#1599](https://github.com/almide/almide/pull/1599) 中 commissioning 的引擎，`almide::wasm_leg` 前端 + `crates/almide-wasm` 产出器，入口是 `src/cli/build.rs` 中的 `render_wasm_module_routed` —— 是唯一的 wasm 渲染器。它在 `wasm_cross` 语料上以 610/610 与原生逐字节相同被接受，其构建产物以 WASI 形式（[#1588](https://github.com/almide/almide/issues/1588)）发布，因此可在标准运行时上运行。它无法下降的程序是一个诚实的错误（`error[E082]`，点名墙与函数），绝不回退：曾经接收这些形态的 incumbent v1 leg 在 #2752 中失去了最后一条路径，并在 #2761 中被删除。`ALMIDE_VERIFIED_DEBUG=1` 会实况播报路径。

```bash
almide run app.almd                  # 编译 + 执行（原生）
almide build app.almd --target wasm  # 构建 WebAssembly（WASI）
almide test                          # 递归查找并运行所有 test 块
almide check app.almd                # 仅类型检查
almide check app.almd --target wasm  # + wasm 构建路径：检查期给出 E081/E082（#1922）
almide fmt app.almd                  # 格式化源码
```

完整命令列表见 `almide --help`（compile、add、deps、clean 等）。流水线与模块地图见 [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md)；wasm leg 的细节见 [docs/wasm/](./docs/wasm/README.md)。

### 接下来 —— v1，信任主干

上面的 Perceus 证明只证明了编译器的一趟，一次。v1 把这个原理推广到**整条流水线** —— 不去证明整个编译器，而是证明一个极小的*检查器*，并让编译器在每次构建时产出一份证书由检查器复验。若检查器接受，产物就具备该性质 —— 这是一条从不提及编译器内部的定理。它把可信基从整个编译器（`src/` 与 `crates/` 下约 30 万行 Rust，不含测试目录，2026-10-03 统计）收缩到被抽取出的检查器（约 1,400 行 OCaml，由证明机械导出），并提出一个比测试更难的问题：**不是"测试通过了吗"，而是"机器能证明输出正确吗"。** 架构、收据（C-SAFE / C-REPRO / C-FAITHFUL / C-PROVEN），以及构建为何被刻意放慢，见 **[docs/TRUST-SPINE.md](./docs/TRUST-SPINE.md)**。

## 项目状态

| 类别 | 状态 |
|----------|--------|
| 成熟度 | 1.0 之前，在 `develop` 上活跃开发；面向 LLM 的表面由 [STABILITY.md](docs/STABILITY.md) 冻结（2026-08-20 宣布） |
| 支持 | 仅最新发布线，1.0 之前 —— 策略与版本保证见 [SUPPORT.md](./SUPPORT.md) · 漏洞见 [SECURITY.md](./SECURITY.md) |
| 编译器 | 纯 Rust，单一二进制 |
| 目标 | Rust（原生）、WASM（直接产出 —— structural leg，见[工作原理](#工作原理)） |
| 已验证代码生成 | structural wasm leg：逐字节语料与变异门，被信任，每次构建的证书仍在推进中（#2755–#2760）；带 PCC 证书的 incumbent leg（自 0.29.0 起每次构建复验）已由 #2761 退役 |
| 代码生成 | Rust：Nanopass + TOML 模板；wasm：structural 引擎 → 直接产出（v0 产出器与 incumbent 的 MIR→WAT 渲染器已退役 —— 墙是错误，绝非回退） |
| 标准库检验 | 每个公开的 stdlib 函数都在 [`proofs/stdlib-inspection.toml`](./proofs/stdlib-inspection.toml) 中带有 `inspected` 标记：原生与 wasm 上字节相同、每次调用的开销不超过声明的阶、codopsy A、每次新生成的模糊组合、经由模块菱形的调用。截至 2026-10-19（almide 0.68.0）未检验数为 0；`tests/stdlib_inspection_test.rs` 会在签名或函数体改变时作废标记，未检验数只降不升 |
| 产物 | 经 `almide compile` 产生的 `.almdi` 模块接口文件 |
| Playground | [在线](https://almide.github.io/playground/) —— 编译器以 WASM 形式跑在浏览器里 |

<!-- stats:generated:start — derived from docs/stdlib/*.md, spec/, and docs/contracts/contracts.toml by scripts/gen-readme-stats.sh; DO NOT EDIT between the markers -->
<!-- counts:generated:start (as of 2026-10-10) — stamped totals from proofs/ledger-counts.toml; refreshed only by scripts/gen-ledger-counts.sh, never by a fixture/contract PR; DO NOT EDIT between the markers -->
| Derived count | Value |
|---|---|
| Stdlib | 1031 functions across 45 modules — self-hosted `.almd`, signature indexes regenerated from the compiler by `tools/gen-stdlib-doc-index.py` |
| Tests | 512 `.almd` test files under `spec/` (`almide test spec/`) + the 376-contract cross-target ledger |
<!-- counts:generated:end -->
<!-- stats:generated:end -->

<!-- mutation-score:generated:start (as of 2026-09-22) — stamped from proofs/mutation-score.toml by scripts/gen-mutation-score.sh; re-measured by every mutation-sweep run, refreshed only by `--from-run`; DO NOT EDIT between the markers -->
**Mutation score** — 41/41 mutants caught (100.0 %), 0 survived, 0 stale: the full release-shape net sweep of `ci/mutations/` ([`scripts/check-mutation-gate.sh`](./scripts/check-mutation-gate.sh)), stamped 2026-09-22 from mutation-sweep run [35650396971](https://github.com/almide/almide/actions/runs/35650396971) at `3b02f7dc4`.
<!-- mutation-score:generated:end -->

## 用 Almide 写成的

本仓库中最大的 Almide 程序是标准库。下面这些是在别处用 Almide 写的，也是那些表面主张与"并非为了让语言好看而写的代码"相遇的地方。

- **编码智能体** —— [comide](https://github.com/O6lvl4/comide)，一个目录里的对话 · [golemide](https://github.com/O6lvl4/golemide)，单一静态二进制，先观察后编辑，每次写入都过语法门 · [homullus](https://github.com/almide-ai/homullus)，模块化智能体运行时 · [manus](https://github.com/almide-ai/manus)，macOS 上的计算机操作
- **智能体基础设施** —— [porta](https://github.com/almide/porta)，WASM 隔离加 OS 沙箱，无需 Docker · [ctxgate](https://github.com/O6lvl4/ctxgate)，驻留在工具钩子里的上下文防火墙 · [hew](https://github.com/O6lvl4/hew)，面向智能体的结构感知 `cat`/`grep`/`sed` · [almai](https://github.com/almide-ai/almai)，覆盖所有提供方的统一 LLM 接口
- **语法分析** —— [gramide](https://github.com/O6lvl4/gramide)，给编码智能体用的语法树，附 [TypeScript](https://github.com/O6lvl4/gramide-typescript)、[Python](https://github.com/O6lvl4/gramide-python)、[Rust](https://github.com/O6lvl4/gramide-rust)、[Go](https://github.com/O6lvl4/gramide-go)、[JavaScript](https://github.com/O6lvl4/gramide-javascript)、[Almide](https://github.com/O6lvl4/gramide-almide) 的文法包，每个都与该语言自身的 parser 对照检查 · [parsegen](https://github.com/almide/parsegen)，无 C、无动态加载的 tree-sitter 兼容 parser 生成器
- **图形** —— [ceangal2](https://github.com/almide-graphics/ceangal2)，计算着色器优先的 2D/3D 渲染器 snaidhm 与其上的布局和控件 ceangal，一个 Almide 程序跑在 web、iOS、macOS、Android、Windows 与 Linux 上（可从 [ceangal-hello](https://github.com/almide-graphics/ceangal-hello) 开始） · [obsid](https://github.com/almide-graphics/obsid)，经 WASM 的 Canvas 2D / WebGL / 3D · [nn](https://github.com/almide-graphics/nn) 与 [slabhra](https://github.com/almide-graphics/slabhra)，神经网络原语与反向模式自动微分 · [cruth](https://github.com/almide-graphics/cruth)，VRM/glTF 角色 · [lumen](https://github.com/almide-graphics/lumen)，无需 `@extern` 的图形数学
- **桌面** —— [almide-shell](https://github.com/almide-graphics/almide-shell)，Hyprland 的 shell：状态栏、启动器、通知、屏幕显示 · [sprid](https://github.com/O6lvl4/sprid)，在严格内存预算下带回滚缓冲的终端模拟器
- **绑定与分发** —— [almide-bindgen](https://github.com/almide/almide-bindgen)，面向 20 个语言目标的 FFI · [almide-wasm-bindgen](https://github.com/almide/almide-wasm-bindgen) · [almide-lander](https://github.com/almide/almide-lander)，把 Almide 作为原生共享库 · [almide-web](https://github.com/almide/almide-web)，浏览器 API
- **库** —— [toml](https://github.com/almide/toml) · [yaml](https://github.com/almide/yaml) · [csv](https://github.com/almide/csv) · [svg](https://github.com/almide/svg) · [dfa](https://github.com/almide/dfa) · [bigint](https://github.com/almide/bigint) · [aes](https://github.com/almide/aes) · [rsa](https://github.com/almide/rsa) · [sha1](https://github.com/almide/sha1) · [base64](https://github.com/almide/base64) · [almide-sqlite](https://github.com/almide/almide-sqlite) · [llm](https://github.com/almide/llm)，LLM 提供方的流式客户端 · [nnlib](https://github.com/almide/nnlib)，神经网络内核（attention、RoPE、归一化、量化行）
- **一致性与测量** —— [als](https://github.com/almide/als)，带自有裁判的规范性规范，可对任意 `almide` 二进制运行 · [almide-dojo](https://github.com/almide/almide-dojo)，LLM 可写性的测量场，harness 本身用 Almide 写成 · [bonsai-almide](https://github.com/almide/bonsai-almide)，跑在浏览器里的 1-bit LLM

2026-09-30 测得：本仓库之外有 144 个公开仓库包含 Almide 源码。剔除生成产物后的手写行数：[nn](https://github.com/almide-graphics/nn) 21,815 行，[gramide](https://github.com/O6lvl4/gramide) 家族在 8 个仓库中共 19,780 行，[almide-shell](https://github.com/almide-graphics/almide-shell) 8,502 行，[snaidhm](https://github.com/almide-graphics/snaidhm) 6,800 行，[almide-bindgen](https://github.com/almide/almide-bindgen) 5,871 行，[ceangal](https://github.com/almide-graphics/ceangal) 5,099 行，[sprid](https://github.com/O6lvl4/sprid) 4,565 行，[porta](https://github.com/almide/porta) 4,419 行。snaidhm 与 ceangal 是在各自的仓库里统计的，这两个仓库此后已归档并迁入 ceangal2。

## 生态与文档

- [almide-grammar](https://github.com/almide/almide-grammar) —— 编辑器与 tree-sitter 工具据以生成的语法描述（关键字、运算符、优先级、TextMate scope），用 Almide 写成；它映照编译器的词法分析器而不是为其供给，每周的 `downstream-sync` 工作流（`scripts/check-downstream-grammar.sh`）会在两组 token 不一致时失败
- [vscode-almide](https://github.com/almide/vscode-almide) · [tree-sitter-almide](https://github.com/almide/tree-sitter-almide)（Neovim、Helix、Zed） · [playground](https://github.com/almide/playground)
- [docs/CHEATSHEET.md](./docs/CHEATSHEET.md) —— 面向 AI 代码生成的速查表 · [docs/SPEC.md](./docs/SPEC.md) —— 语言规范 · [docs/GRAMMAR.md](./docs/GRAMMAR.md) —— EBNF 文法 + stdlib 参考
- [docs/design/DESIGN.md](./docs/design/DESIGN.md) —— 设计哲学 · [docs/design/EQUIVALENCE.md](./docs/design/EQUIVALENCE.md) —— 逐字节相同的主张 · [docs/design/MEMORY-SAFETY.md](./docs/design/MEMORY-SAFETY.md) —— 证明与信任的划分 · [docs/TRUST-SPINE.md](./docs/TRUST-SPINE.md) —— v1
- [docs/contracts/](./docs/contracts/) —— 行为契约台账 · [docs/stdlib/](./docs/stdlib/) —— 按模块的标准库 · [docs/project/BENCHMARKS.md](./docs/project/BENCHMARKS.md) —— 体积、运行时间、编辑循环扩展性 · [docs/roadmap/](./docs/roadmap/README.md) —— 演进计划

## 参与贡献

欢迎在 [GitHub](https://github.com/almide/almide) 提 Issue 和 Pull Request。克隆后请安装 git 钩子（`brew install lefthook && lefthook install`）；提交信息必须为英文（由 commit-msg 钩子强制）。项目约定见 [CLAUDE.md](./CLAUDE.md)。

## 许可证

可任选 [MIT](./LICENSE-MIT) 或 [Apache 2.0](./LICENSE-APACHE)。
