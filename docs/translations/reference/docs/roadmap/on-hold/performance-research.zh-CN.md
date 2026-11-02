原文说明元数据：通过语义感知优化超越手写 Rust 的研究计划
<a id="performance-research-path-to-world-1"></a>

# 性能研究：通往世界 #1 的路径

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](performance-research.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](performance-research.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](performance-research.zh-CN.md)

> 固定原文提交的非规范阅读快照；以原文为准，不自动同步。未独立执行、验证实现、检查证明或测量。历史命令与设计指令是资料，不是已执行的指令。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/performance-research.md)

> 原文注记：这是含有测量设置未记录的历史观察、拟议目标和发表假设的研究计划，不是实测世界排名或当前速度保证。早期语义注解表及纯函数注释省略了义务：仅不可变不能确立无别名，仅使用一次不能确立不捕获，仅纯粹不能确立终止、不 unwind 或所有内存属性。后文别名/准入条件限制与早期草图一并保留。编译器架构、当前/完成标签、LLVM 时间、定律基础设施主张和竞品数字仍是历史原文主张；未独立验证优化、基准或外部文档。

**研究论点**：在整个编译流水线中保留的高层语义信息，能够实现传统低层编译器无法完成的优化。为 LLM 准确性设计的语言，也同时针对编译器分析进行优化 — 两者都受益于显式语义、清晰数据流和尽量少的别名。

**最初观察（此处未记录 toolchain / host / run）**：n-body 为1.74s（Almide）对1.69s（手写 Rust），约2.9% 开销。这不是当前基准结果；新的比较需要匹配工作负载和构建设置。

**目标**：在标准基准中持续超越手写 Rust。

---

<a id="phase-0-close-the-29-gap-engineering"></a>

## Phase 0：弥合2.9% 差距（工程）

**目标**：在 n-body 和额外3+个基准上达到手写 Rust 的1.00x。

**对该观察的最初根因假设**：Copy 类型上冗余的 `.clone()`、不必要的 `as f64` 转换、非最优的变量绑定模式。

| 任务 | 效果 | 难度 |
|------|--------|-----------|
| 消除 Copy 类型上的 clone | i64/f64/bool 上的 `.clone()` → 删除 | 低 |
| 消除冗余转换 | x 已是 f64 时的 `x as f64` → 删除 | 低 |
| 减少 let 绑定 | tmp 仅使用一次的 `let tmp = expr; use(tmp)` → 内联 | 中 |
| Stream fusion Phase 2 | 检测已完成 → 实现实际 IR 重写 | 中 |
| Clone reduction Phase 4 | for-in、成员访问、match 对象、record spread | 中 |

**指标**：n-body ≤ 1.69s，spectral-norm ≤ Rust 参考实现，binary-trees ≤ Rust 参考实现。

**基准实现现已存在**于 `research/benchmark/perf/`（`nbody`、`spectralnorm`、`binarytrees`、`fannkuchredux`、`mandelbrot`）。存在不等于达到性能目标；报告比率前须重新运行：
- [x] n-body（引力模拟）
- [x] spectral-norm（特征值）
- [x] binary-trees（GC 压力 / 分配）
- [x] fannkuch-redux（组合计算）
- [x] mandelbrot（浮点并行）

---

<a id="phase-1-surpass-human-written-rust-applied-research"></a>

## Phase 1：超越人类编写的 Rust（应用研究）

**目标**：在 ≥3 个基准上比手写 Rust 快 ≥5%。

**研究问题**：全程序语义分析能否生成人类不会写出的代码？

<a id="1a-whole-program-inlining"></a>

### 1a. 全程序内联

Almide 能看到整个程序。rustc 的内联受 crate 边界和启发式规则限制。

```
Almide approach:
  1. Build call graph with use-counts (already have this)
  2. Inline functions with use_count == 1 unconditionally
  3. Inline small functions (body ≤ 3 IR nodes) unconditionally
  4. For hot paths: inline up to depth 3
  5. Emit single monolithic function for hot loops
```

代码外翻译：Almide 方法：1. 构建带使用次数的调用图（已有）。2. 无条件内联 use_count == 1 的函数。3. 无条件内联小函数（函数体 ≤ 3 个 IR 节点）。4. 对热点路径内联到深度3。5. 为热点循环发出单个一体化函数。

rustc + LTO 能做其中一些，但 Almide 拥有更好的信息：它知道哪些函数纯粹（effect system）、哪些变量逃逸（借用分析），以及精确使用次数。

<a id="1b-allocation-elimination"></a>

### 1b. 消除分配

Almide 类型系统保证默认不可变。这意味着：

```
// Almide knows this list is never modified after creation
let xs = [1, 2, 3, 4, 5]
let sum = xs.fold(0, |acc, x| acc + x)

// Instead of Vec::new() + push + push + ..., emit:
// static array on stack + direct fold (no heap allocation)
```

代码外注释翻译：Almide 知道此列表在创建后绝不修改。代替 Vec::new() + push + push + …，发出栈上静态数组 + 直接 fold（无堆分配）。

| 模式 | 最初基线草图 | 期望的优化 codegen |
|---------|----------------|-------------------|
| 小列表字面量 | `vec![1, 2, 3]`（堆） | `[1, 2, 3]`（栈数组） |
| 仅用于 fold 的列表 | `Vec<T>` + iterator | 直接累加（无列表） |
| 字符串拼接链 | 多个 `String::new()` + `push_str` | 预设容量的 `String::with_capacity` |
| 创建后立即解构的 record | struct 分配 + 字段访问 | 直接传递值 |

<a id="1c-semantic-aware-dead-code-elimination"></a>

### 1c. 语义感知的死代码消除

effect system 支持积极的 DCE：

```
// Almide knows: pure fn never has side effects
fn square(x: Int) -> Int = x * x

let a = square(10)  // pure, use_count == 0 → eliminate entire call
```

代码外注释翻译：Almide 知道纯函数绝无副作用。纯粹、use_count == 0 → 消除整个调用。

当前 DCE 保守（保留全部函数调用）。有了 effect 注解，就能消除结果未使用的纯函数调用。

<a id="1d-type-specialized-container-operations"></a>

### 1d. 类型专用容器操作

Almide 知道每个调用点的具体元素类型：

```
// list.sort_by knows T = Int at this call site
users.sort_by(|u| u.age)

// Instead of generic comparison, emit specialized:
// .sort_by(|a, b| a.age.cmp(&b.age))  ← already specific
// But also: for Int lists, use counting sort / radix sort when len > threshold
```

代码外注释翻译：list.sort_by 知道此调用点 T = Int。不发出通用比较，而发出专用比较（所示 .sort_by 已是具体比较）。另外，对 Int 列表在 len > threshold 时使用计数排序 / 基数排序。

**指标**：在 ≥3 个 Benchmarks Game 程序上，Almide 比 Rust 参考实现快 ≥5%。

---

<a id="phase-2-llvm-ir-direct-emission-engineering--research"></a>

## Phase 2：直接发出 LLVM IR（工程 + 研究）

**目标**：完全跳过 rustc，从 Almide IR 发出 LLVM IR。

**研究问题**：移除 Rust 抽象层是否能带来新的优化机会？

<a id="architecture"></a>

### 架构

```
Current:   Almide IR → Rust source → rustc → LLVM IR → LLVM → binary
Proposed:  Almide IR → LLVM IR → (custom passes) → LLVM → binary
```

图外翻译：当前为 Almide IR → Rust source → rustc → LLVM IR → LLVM → binary；提案为 Almide IR → LLVM IR →（自定义 pass）→ LLVM → binary。

<a id="why-this-matters"></a>

### 为何重要

1. **编译时间**：rustc frontend 占编译时间60-70%。直接 LLVM IR 将 `almide run` 从约2s 缩短到约0.6s。
2. **自定义 LLVM pass**：在 Almide lowering 和 LLVM 优化之间插入利用 Almide 语义信息的优化 pass。
3. **细粒度控制**：可根据 Almide effect / purity 分析精确发出 LLVM IR 注解（`noalias`、`readonly`、`nounwind`、`willreturn`）。

<a id="semantic-annotations-almide-can-emit"></a>

### Almide 可发出的语义注解

| Almide 知识 | LLVM 注解 | 效果 |
|-----------------|-----------------|--------|
| 纯函数（无 effect） | `readonly`, `willreturn`, `nounwind` | 支持积极的 CSE、LICM |
| 不可变变量 | 指针参数上的 `noalias` | 支持消除 load |
| 使用次数 == 1 | `nocapture` | 支持提升到栈 |
| 无别名（ownership） | `noalias`, `dereferenceable` | 支持向量化 |
| 已知循环边界（Range） | `!llvm.loop` 元数据 | 支持循环展开 |

rustc 会添加其中一部分，但 Almide 拥有更高层的语义信息，因此可以**更精确**。

<a id="implementation-path"></a>

### 实现路径

| 步骤 | 工作 | LOC 估计 |
|------|------|-------------|
| LLVM IR 文本发射器 | nanopass 流水线中的新 codegen target | ~2,000 |
| LLVM IR 运行时库 | 将 core_runtime.txt 移植为 LLVM bitcode | ~1,000 |
| 语义注解 pass | 从 effect 分析发出 noalias/readonly/nounwind | ~500 |
| 自定义 LLVM pass：deforestation | 消除中间数据结构 | ~1,500（C++） |
| 自定义 LLVM pass：arena coalescing | 合并生命周期已知的分配 | ~1,000（C++） |

**依赖**：`inkwell` crate（Rust LLVM bindings）或原始 LLVM-C API。

---

<a id="phase-3-semantic-aware-optimization-pure-research"></a>

## Phase 3：语义感知优化（纯研究）

**目标**：利用 Almide 丰富语义的新型优化 pass。

**研究问题**：当编译器知道纯粹性、effect、ownership 和代数定律时，哪些优化会成为可能？

<a id="3a-deforestation-intermediate-structure-elimination"></a>

### 3a. Deforestation（消除中间结构）

Stream fusion（Phase 0）把 `map.filter.fold` 融合为单个循环。Deforestation 将其推广到**所有中间数据结构**：

```almide
fn process(users: List[User]) -> List[String] =
  users
    |> filter(|u| u.age > 18)
    |> map(|u| u.name)
    |> sort_by(|n| n)
    |> take(10)

// Current: filter → new Vec → map → new Vec → sort → new Vec → take → new Vec
// Deforested: single pass with in-place sort on pre-filtered view
```

代码外注释翻译：当前为 filter → 新 Vec → map → 新 Vec → sort → 新 Vec → take → 新 Vec；消除中间结构后，在预过滤 view 上原地排序的单遍处理。

**研究贡献**：由 TypeConstructorRegistry 代数定律指导的 deforestation。Almide 已有定律基础设施（HKT foundation Phase 1-4）。

<a id="3b-effect-guided-parallelization"></a>

### 3b. Effect 引导的并行化

effect 注解支持自动并行化：

```almide
// Almide knows: square is pure, map preserves order
let results = large_list.map(|x| square(x))

// Safe to parallelize because:
// 1. square has no effects
// 2. map is a Functor operation (preserves structure)
// 3. No mutable state captured
```

代码外注释翻译：Almide 知道 square 纯粹、map 保持顺序。可安全并行化，因为：1. square 无 effect。2. map 是 Functor 操作（保持结构）。3. 未捕获可变状态。

在以下条件下发出 `rayon::par_iter()` 或 SIMD intrinsics：
- 函数纯粹（effect 分析）
- 操作易于并行（代数定律：map 满足分配律）
- 集合大小超过阈值

<a id="3c-memoization-of-pure-functions"></a>

### 3c. 纯函数的记忆化

```almide
fn fib(n: Int) -> Int =
  if n <= 1 { n }
  else { fib(n - 1) + fib(n - 2) }

// Almide knows: fib is pure, recursive, Int → Int
// Auto-memoize: HashMap<i64, i64> cache
```

代码外注释翻译：Almide 知道 fib 纯粹、递归、Int → Int。自动记忆化：HashMap&lt;i64, i64&gt; 缓存。

条件：纯函数 + 递归 + 小输入域。

<a id="3d-data-layout-optimization"></a>

### 3d. 数据布局优化

Almide 通过使用次数分析知道字段访问模式：

```almide
type User { name: String, age: Int, email: String, score: Float }

// If hot loop only accesses .age and .score:
for u in users { process(u.age, u.score) }

// Emit SoA (Struct of Arrays) for hot fields:
// ages: Vec<i64>, scores: Vec<f64> — cache-friendly
```

代码外注释翻译：若热点循环仅访问 .age 和 .score，则为热点字段发出 SoA（数组的结构体）：ages: Vec&lt;i64&gt;, scores: Vec&lt;f64&gt; — 对缓存友好。

**研究贡献**：由字段使用次数与循环分析指导的自动 AoS → SoA 转换。

---

<a id="phase-4-validation--publication"></a>

## Phase 4：验证与发表

<a id="benchmark-targets"></a>

### 基准目标

| 基准 | 最初观察 | Phase 0 目标 | Phase 1 目标 | Phase 2-3 目标 |
|-----------|---------|---------|---------|-----------|
| n-body | 1.03x | 1.00x | 0.97x | 0.90x |
| spectral-norm | — | 1.00x | 0.95x | 0.90x |
| binary-trees | — | 1.00x | 0.90x | 0.85x |
| fannkuch-redux | — | 1.00x | 0.98x | 0.92x |
| mandelbrot | — | 1.00x | 0.95x | 0.88x |

（x = 相对于最佳 Rust 实现的比率。< 1.0 = Almide 更快。）

<a id="publication-targets"></a>

### 发表目标

| Phase | 标题 | 场所 |
|-------|-------|-------|
| 0-1 | “Almide：通过语义感知 Rust 生成实现近零开销高层编译” | arXiv 预印本 |
| 2 | “LLVM 的语义注解：利用高层语言属性进行低层优化” | CGO / CC |
| 3 | “Effect 引导的优化：纯粹性如何支持自动并行化和 deforestation” | PLDI / OOPSLA |
| 全部 | “修改存活率 × 性能：为 LLM 与硬件共同设计的语言” | POPL |

<a id="original-competitive-hypotheses-not-measured-rankings"></a>

### 最初竞争假设（不是实测排名）

下列未注明版本的比较只作为研究动机保留。它们不确立当前相对性能、MSR 或加速倍数。

| 语言 | 性能 | LLM 准确性 | Almide 优势 |
|----------|------------|--------------|-----------------|
| C | 基线 | 低（UB、手动内存） | 通过语义优化更安全、更快 |
| C++ | ~C | 低（复杂性） | 更简单且相当 |
| Rust | ~C | 中（borrow checker 错误） | 同一 backend + 语义优化 |
| Zig | ~C | 中 | 语义丰富性 |
| Mojo | 宣称 ~C | 未知 | 已证实的 MSR 指标 |
| Go | ~2x C | 高 | 快10-50x |
| TypeScript | ~20x C | 高 | 快100x+ |

---

<a id="key-insight-llm-friendliness--optimization-friendliness"></a>

## 关键洞察：LLM 友好 = 优化友好

这是待评估的研究假设，不是已经确立的独特性主张。让 LLM 易于准确编写语言的同一组属性，也为优化提供丰富信息：

| LLM 友好属性 | 优化收益 |
|-----------------------|---------------------|
| 默认不可变 | 可能简化推理；不可变值仍可存在别名，所以优化需要显式 ownership / purity 条件 |
| 显式 effect（`effect fn`） | 纯粹性分析 → DCE、并行化、记忆化 |
| 无隐藏控制流（纯函数中无异常） | 可预测的优化 |
| IR 中按类型分派的运算符 | 无运行时类型检查 |
| UFCS | 解析合格调用；函数值及间接分派仍需调用图分析 |
| 无继承 | 不消除函数值 / closure 分派；内联仍取决于准入条件 |

**这种相关性并非巧合，而是结构性的。** 为 LLM 减少歧义的语言也为优化器减少歧义。若经基准验证，这一论点就是可发表的成果。

---

<a id="priority"></a>

## 优先级

Phase 0（弥合差距）→ Phase 1b-1c（消除分配 + 语义 DCE）→ Phase 1a（内联）→ Phase 2（直接 LLVM）→ Phase 3a（deforestation）→ Phase 3b（并行化）。

Phase 0 是工程。Phase 1 是应用研究。Phase 2-3 是新研究。Phase 4 是验证。
