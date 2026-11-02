<a id="xtarget-fuzz--almide-generative-fuzzer"></a>

# xtarget-fuzz — Almide 生成式 fuzzer

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](README.zh-CN.md)



> 原文：[tools/xtarget-fuzz/README.md @ 35297a](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/tools/xtarget-fuzz/README.md)。这是供读者参考的非规范性快照译文，不会自动同步。未对测量值及主张进行独立复核，以原文为准。

completeness roadmap 的 Stage 3：持续搜寻组合空间中**可观察分歧**与编译器故障的机器。

它通过 synthesis、corpus mutation、identity、composition 及 multi-module shape 系列生成 Almide 程序，在两个目标上编译执行的 oracle ladder 中运行并判定结果。每个程序都可从 `(seed, index, family)` 三元组重现。发现写入 `findings/`。最小化因系列而异；shape 发现保留整个项目，不声称是最小复现。

<a id="which-oracle-judges-what"></a>

## 哪种 oracle 判断什么

*差分* fuzzer 没有用于判断两条路径共同 bug 的 oracle。native 和结构化 Wasm 共享 frontend 与 linked-IR 机制（不是共享 MIR/WAT renderer），因此其中的误编译会让两条路径同样出错，投票却全票一致。#1322 正是如此：scalar let-alias 误编译从 v0.57.0 存在至今，而 `spec/wasm_cross` 全部 394 个一致性样例一直为 green。

| oracle | 能定罪的情况 | 覆盖范围 |
|---|---|---|
| native ⇄ wasm 差分 | 两个 backend 之间的分歧 | 每个程序 |
| `almide-interp`（第三裁判，#516） | interpreter 能运行程序时，某个 backend | 可自由弃权——只是一票，不是判决 |
| **按构造确定** | **任一路径，或两条同时** | identity scalar/control flow、composition operator/payload 单元，以及带期望输出的 shape project |

第三行是前两者无法提供的：identity 系列程序的期望 stdout 是其自身源码中的字面量，因此**单独**判断每条路径，一致并不证明什么。

这是独立 crate（像 `tools/wasmgen-harness` 一样有自己的 `[workspace]`）。它通过 path 依赖父 `almide` crate 获取 AST / parser / formatter，并**调用外部进程**运行新构建的 `almide` 二进制来执行 oracle ladder。因此 compiler ICE 崩溃的是可观察的子进程，而不是 fuzzer。

<a id="build--run"></a>

## 构建与运行

```bash
# 1. Build the compiler (the fuzzer drives this binary).
cargo build --release --bin almide          # from the repo root

# 2. Build the fuzzer.
cd tools/xtarget-fuzz && cargo build --release

# 3. Run a campaign (time-budgeted or fixed count).
./target/release/xtarget-fuzz run --minutes 60          # 60-minute hunt
./target/release/xtarget-fuzz run --count 200 --jobs 8  # 200 programs, 8 workers

# A campaign run ENTIRELY under the by-construction oracle (#1332).
./target/release/xtarget-fuzz run --minutes 30 --family identity

# Inspect / reproduce one program deterministically.
./target/release/xtarget-fuzz gen    --seed 100 --index 42   # print source
./target/release/xtarget-fuzz replay --seed 100 --index 42   # re-run the ladder

# Re-judge a saved repro. An identity repro carries its own oracle in
# `// @expect` header lines, so this needs no seed and no family.
./target/release/xtarget-fuzz ladder findings/<dir>/repro.almd

# Catalogue / corpus sizes.
./target/release/xtarget-fuzz stats
```

`--family all|identity|synthesis|composition|shape`（默认 `all`）选择 campaign 采样的系列。**`(seed, index)` 只有在相同 `--family` 下才能重现程序**：`all` 在生成前用一次随机抽样选择系列，`identity` 不用，因此同一 index 从第一个字节就不同。每个发现的 `meta.txt` 都记录 family，`reproduce` 行也明确写出。

WASM 执行阶段要求 `PATH` 中存在 `wasmtime`。repo root 与 `almide` 二进制会自动检测，可用 `--repo` / `--almide` 覆盖。

<a id="the-generator"></a>

## generator

五个系列分阶段选择（`src/generator/mod.rs` 的 `Engine::generate`）。Identity 占 `all` 的 3/13；composition 占剩余抽样的 3/13；shape 占两者留下部分的 1/6。最后的余量按 7:3 分为 synthesis 和 mutation。旧的 54% synthesis / 23% mutation 数字早于 composition 和 shape 阶段。

identity 决策取自**独立的** RNG 子流（`IDENTITY_STREAM_SALT`），而非程序主随机流。在主流添加一次抽样就会改变对应关系，使归档发现的 `(seed, index)` 无法重新生成其最小化前的程序。拆出此决策可让 #1332 前的 synthesis 和 mutation 随机流保持逐字节相同。

<a id="the-identity-family-23--the-by-construction-oracle-1332"></a>

### identity 系列（约 23%）——按构造确定的 oracle（#1332）

`src/generator/identity.rs`。采用 Rustlantis 的做法，从**答案倒推**构建每个程序。每个 accumulator 始于字面量 `K`，初始化与 `println` 之间的每组语句都是**恒等变换**。因此程序必须打印自身源码可见的字面量 `K`，无需第二次执行即可判断。

```almide
// @expect a0=-3728
fn main() -> Unit = {
  var a0 = -3728
  let snap2 = a0          // ← #1322's shape: sound only if this COPIES
  a0 = a0 + 1796
  var it8 = 0
  while it8 < 3 {         // ← loop-carried scalar state
    let cur9 = a0
    a0 = cur9 + 896
    it8 = it8 + 1
  }
  a0 = a0 - 2688
  a0 = snap2
  println("a0=${a0}")
}
```

健全性来自结构，而非测量：

1. **每个块在代数上都是恒等变换。** 它携带自身逆操作（`+n`/`-n`、`*m`/`/m`、`xor n`/`xor n`、swap/swap、snapshot/restore），或由 generator 输出时计算的常量补偿（`while` 次数 × step、`0..<t` 的三角数、元素和为零的列表），或在分支的**两个** arm 上保持平衡，使实际选中的 arm 无关紧要。块体本身也是恒等块列表，所以嵌套可组合。
2. **任何算术都不会溢出或截断。** generator 维护 `|acc|` 的保守上界，拒绝会使其超出 `2^40` 的块。整数除法只出现在 `*m` / `/m` 对的收尾部分，被除数恰为初始值的 `m ×`。（Almide 的 `+`/`-` lower 为 `wrapping_*`，所以即使没有此界，加法逆操作仍成立；该界使 `/` 和补偿循环精确。）

两个性质都不依赖编译器正确，这正是要点。块权重偏向 #1332 指出的三种形态：跨循环 scalar 状态、let-of-var aliasing、branch-arm assignment。

**有意不覆盖**：strings、floats、Unicode、作为数据的 collections、effects、generics。它们在此 identity 系列中都没有廉价的按构造成立的逆操作。其他系列覆盖额外形态：composition 和 shape 有自己的按构造期望输出；synthesis 和 mutation 主要使用差分证据。

**最小化是结构性的**（`minimize::minimize_plan`）：identity 发现通过 *plan* 缩减，而非文本。几乎每行都是逆操作对的一半，删除一行就改变程序应打印的值。文本层缩减会因与 bug 无关的原因“仍能复现”，悄悄把误编译变成 generator 造成的假象。

<a id="the-composition-family--operatorpayload-interactions"></a>

### composition 系列——operator/payload 相互作用

`--family composition` 对期望输出按构造已知的 operator × payload × position 单元采样（`src/generator/composition.rs`）：在 tail、let 和 argument 位置，对 Int/String/tuple/record payload 使用 `??`、`!`、`?`、pipe、match 和 tuple extraction。它不靠 backend 一致来决定期望答案。

<a id="the-shape-family--multi-module-projects-3309"></a>

### shape 系列——多模块项目（#3309）

`--family shape`（以及 `all` 中 identity 与 composition 留下部分的六分之一抽样）。两天内用户提交了十一份已被接受、却有一条路径无法构建的程序（#3283–#3307）；没有单文件程序，也没有使用固定 pool 中的名字。此系列生成**项目**：一至四个声明模块，每个是 sibling module 或 path-dependency package（最多一部分放在入口自身），含所有 value kind 的 top-level let（Rc-backed `Bytes`/`Matrix`、fn 值、closure record），被 closure、nested closure、HOF callback 和其他模块修改的 module `var`s，并从对抗性 pool 选名：仅大小写不同的双胞胎、native 路径的生成 binder（`c`、`_fn_arg0`、`__cap_*`、`__almide_*`），以及其 mangle static 的拼写（`almide_rt_<module>_<name>`）。

词汇来自原样包含的 `tests/shape_matrix/gen.rs`（`#[path]`）：确定性的 shape-matrix gate 在每个 PR 上枚举其有界笛卡尔积，而本系列越界采样（每模块多个 let、同时多个模块、closure 嵌套深至四层）。每个项目按构造知道自己的 stdout。两项规则不同于其他系列：**wasm wall 是发现**（check 已接受项目，因此 wall 属于 #3286 / #3296 类，而非覆盖债务）；发现要完整记录——`repro.almd` 是以 `// ==== <path> ====` 为头的全部文件集合，不做最小化（`replay --family shape` 在 scratch dir 重写项目）。

<a id="the-other-two-families"></a>

### 另两个系列

- **类型导向 synthesis**——`src/generator/{term,program}.rs`。从有类型项文法构建*按构造*良类型的程序：选择目标类型，再从字面量、作用域内变量、返回类型可与目标统一的 stdlib call、HOF 实参的 inline lambda、`if` arm 生成该类型表达式。以 fuel 限制保证终止。歧义字面量（`[]`、`none`、`ok`/`err`）提升到带注解的 `let` 绑定，以通过类型检查。
- **Stdlib catalogue**——`src/generator/catalogue.rs`。签名从**机器来源**提取：由真实 Almide parser 解析同捆的 `stdlib/*.almd` 声明文件。人工整理的*权重表*覆盖在解析表层上，将选择偏向历史分歧集中点（string/Unicode、float formatting、closures/HOFs）。
- **Value pool**——`src/generator/pools.rs`。具名、有注释的易分歧字面量表：多字节字符串（`日本語`、emoji、`é`、组合标记、`ß`），float 边界（`-0.0`、`5e-324`、`1e300`、`0.1+0.2` 形态），int 极值（`i64::MIN/MAX`、宽度边界）。
- **Mutation**——`src/generator/mutate.rs`。解析带 `main` 的语料（`spec/wasm_cross`、`examples` 等），移除 `test` 块，施加保持类型的 AST mutation（pool 中的字面量扰动、同类子表达式交换、语句复制）。排除 `// wasm:skip` 文件（已知分歧）。

确定性：程序自身使用以 `for_program(seed, index)` 为种子的 `SplitMix64`（`src/rng.rs`），系列抽样则使用上述加盐子流。Effect 不限于打印：精选 catalogue 允许来自 fs/env/io/args/process/net/http 的可重现 sandbox 操作，使用有种子的文件、固定输入和 loopback fixture。由 host 决定的结果，在已声明处使用 property verdict。边界见 `SANDBOXED_EFFECT_MODULES`、`generator/verdicts.rs`、`oracle/sandbox.rs`、`oracle/fixture.rs`；这不意味着允许任意未沙箱化 effect。

<a id="the-oracle-ladder"></a>

## oracle ladder

`src/oracle/`——由便宜到昂贵，第一个失败决定程序分类：

| 阶段 | 检查 | 失败意味着 |
|------|-------|---------------|
| a | `almide check` 接受 | **generator bug**（承诺良类型）——计数，不算发现 |
| b | `parse∘fmt` 幂等 | formatter 不稳定 |
| c | native build + run（无 ICE） | native codegen 故障 |
| d | wasm build + validate | wasm codegen 故障 |
| e | **self-check**——各路径对其 `@expect` 输出 | **误编译**，归因于 `native` / `wasm` / **`both legs`** |
| f | 运行两者，逐字节比较 stdout/exit | **分歧**（或 hang） |
| g | 两者对比参考 interpreter | interpreter 投票时，某个 backend 错误 |

(e) 只对携带按构造 oracle 的程序（identity、composition、shape 系列）触发。它在差分比较**之前**运行，因为只有它能同时给两条路径定罪；但在 resource-limit skip（C-196 stack、C-197 wasm32 memory）**之后**，所以 wasm OOM 仍是 skip，而非虚假的误编译。

resource-limit skip（`ladder.rs` 中的 `resource_class`）读取各路径自己的 abort signature：wasm 路径为 `call stack exhausted`（原版 wasmtime trap）/ `Error: out of memory`，native 为 `stack overflow`（C-196 的 `Error:
stack overflow`，或未注册 thread 上 Rust 自己的信息）/ `memory allocation of … bytes failed`。只有受限路径死亡前打印的全部内容是另一条路径 stdout 的前缀，才丢弃该次运行。另一条路径的结局不属此问题（#2382）：“wasm 首次分配 OOM，而 native 继续到有意 panic”是 resource skip；“wasm 打印了 native 没有的行然后 OOM”是被限制截断的发现；没有 resource signature 的 abort 始终是语义 verdict（`RunFailureDivergence` 或 `OutputDivergence`）。

## Minimizer

`src/minimize.rs`——delta-debugging：先删除语句再简化表达式，重新运行 ladder，只有同种发现再次出现才保留缩减。identity 发现改走结构路径（`minimize_plan`，见上文）。输出写到 `findings/<kind>/`：`repro.almd`、`original.almd`、`meta.txt`（seed/index/family/replay command）、`native.out`、`wasm.out`。以 `(kind, summary)` 去重。

<a id="throughput"></a>

## 吞吐量

瓶颈是 WASM build 与 native cargo build。每个 worker 拥有**隔离的** build scratch dir（`ALMIDE_RUN_PROJECT_DIR`），共享 `/tmp` 的 build flock 因而不会将它们串行化，吞吐量随核心数扩展。本地测量（Apple M-series，见 campaign summary 行）：视 `--jobs` 和 native-build cache 热度，约 **40–120 programs/min**（每程序成本主要是 native `cargo` 重建，热 cache 约 0.25 s/program）。

## Nightly CI

`.github/workflows/fuzz-nightly.yml` 于 03:00 UTC（以及 manual dispatch）运行有时间预算的 campaign，将 `findings/` 上传为 artifact，且发现 > 0 时新建/更新带 `fuzz` 标签的 tracking issue。PR CI 有意不作改变；fuzzer 只在 nightly 运行。

nightly 无需修改便能采用 identity 系列：它使用默认 `--family all`，其中包括 identity、composition 和 shape 的按构造 oracle。实际交付占比取决于分阶段选择和完成程序，而非旧的 identity-only 百分比。campaign summary 以 `self-checked = N (P%)` 报告准确份额。`SelfCheckFailure` 是正确性类别，因此夜间发现它时会像任何其他误编译一样变红并创建 tracking issue。

未来改进（尚未接入）：为一个 matrix shard 指定 `--family identity`，不论权重如何重调，都使夜间覆盖的固定比例由 oracle 支持。
