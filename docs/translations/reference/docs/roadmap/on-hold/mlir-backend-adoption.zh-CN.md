<!-- description: Stage 2渐进降低：dialect walker通过全部227个spec测试 -->
<a id="mlir-backend--egg-rewrite-engine"></a>

# MLIR后端与Egg重写引擎

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](mlir-backend-adoption.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](mlir-backend-adoption.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](mlir-backend-adoption.zh-CN.md)

> 非规范性历史参照版。固定原文为权威且不变，不自动同步，未新核实实现、性能、新颖性或外部项目。开头搁置状态优先于旧计划；Stage 1里程碑及PoC不等于walker全迁移、GPU支持或PGO完成。旧版本日期、测试数和pass撤去记录为历史，不是当前期限或完整pass清单。最佳/首个/最优愿景及先例仅属原文主张，不是新证实优势。图代码保持原样，译文置于fence外。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/mlir-backend-adoption.md)

> **状态：搁置** — egg rewriter已交付（Stage 1完成），dialect后端延期。
> Stage 1（egg等式饱和）：✅ 完成，0.16.0历史里程碑。
> 当前native流水线包含EggSaturationPass及target特定StreamFusion；旧替换/行数报告不是完整的当前pass清单。
> Stage 2（dialect）：PoC完成，SSA lowering、Rust/LLVM emit、7个equiv测试。完整walker迁移延期。
> Stage 3–4（GPU/PGO）：搁置。

<a id="decision"></a>

## 决策

Almide计划通过两大支柱构建“世界最好的dispatch基础设施”：

1. 在Almide dialect层应用**基于egg（e-graph）的等式饱和重写器**的前端优化器。
2. 后端采用**MLIR渐进lowering**，形式化处理Rust/WASM/native/GPU多target。

先完成Stdlib Declarative Unification arc（[stdlib-declarative-unification.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/stdlib-declarative-unification.md)）。声明规则及typed intrinsic可直接转为e-graph rewrite规则 / MLIR FunctionImport，保留已有投入价值。

<a id="全体アーキテクチャ"></a>

## 整体架构

```
Source (.almd)
     ↓
AST + Type Check (既存)
     ↓
Almide IR (既存、強化)
     ↓
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  Front-end optimizer: egg e-graph equality saturation
  - 宣言的 rewrite rule (@rewrite attribute から自動変換)
  - algebraic identities, stream fusion, monad laws
  - cost function (target-aware: Rust/WASM/GPU で重みが違う)
  - 等価クラス全探索 → 最適形を extract
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
     ↓
Almide dialect (MLIR)
     ↓
  ┌────────── Rust emit (direct, LLVM 経由しない) ──→ Rust source
  │
  └─ Progressive lowering (MLIR pass manager)
       ↓ affine / arith / memref / scf dialects
       ↓ LLVM dialect  ──→ native binary
       ↓ wasm dialect  ──→ WebAssembly binary
       ↓ SPIR-V / NVVM ──→ GPU kernel
```

图中文字译文：AST及类型检查已存在；Almide IR已存在并增强。声明式rewrite规则从rewrite属性自动转换。target-aware cost对Rust/WASM/GPU使用不同权重，探索等价类后提取最优形态。直接Rust emit不经过LLVM。

**重要边界**：原文将egg置于Almide dialect以下的高层；MLIR pass负责从Almide dialect lowering。两者职责分离。

<a id="なぜ-egg--mlir-の組合せか"></a>

## 为什么结合egg与MLIR？

<a id="egg-単体ではダメな理由"></a>

### 仅egg不足的原因

- egg只是重写引擎，target特定后端优化（vectorize、inline、register alloc）交给LLVM。
- 用egg编写GPU codegen不现实；SPIR-V生成利用既有工具链。

<a id="mlir-単体ではダメな理由"></a>

### 仅MLIR不足的原因

- MLIR PDL（Pattern Description Language）是按优先级重写，不提供等式饱和：**同时保留所有等价形态，最后选择最优者**。
- 仍有合流/phase ordering问题，例如先fusion还是先inline；egg从结构上处理这些问题。

<a id="両者を組み合わせる理由"></a>

### 结合两者的原因

- egg寻找最优**形态**，即代数搜索。
- MLIR生成最优**代码**，即target lowering与LLVM结合。
- 此分工旨在使纯Almide代码在所有target降低到最优形态。

<a id="先行事例"></a>

### 先例

- **Cranelift ISLE**：原文描述为在指令选择中使用egg式e-graph，Rust native。
- **Herbie**：采用egg搜索数值表达式等价性。
- **egglog / eggcc**：通过e-graph与datalog研究编译器优化。
- **torch-mlir**：通过MLIR对PyTorch进行多target lowering。

Almide目标定位是“**首个认真将egg + MLIR集成实现为语言编译器**”，处于研究与实用交界。

<a id="rust-source-output-option-b-採択"></a>

## Rust源码输出：采纳Option B

<a id="選択肢の比較"></a>

### 选项比较

**Option A**：放弃Rust源码输出，统一为MLIR → LLVM → native/WASM。
- 损失：Almide输出可读Rust所带来的信任、Cargo生态源码级集成。
- 收益：充分利用MLIR能力，降低复杂度。

**Option B（采纳）**：从Almide dialect直接emit Rust，不经过LLVM。
- 保留Almide dialect → Rust source walker作为Rust target路径。
- WASM/native/GPU采用MLIR渐进lowering。
- 复杂度：两条emit路径。
- 优势：保留既有信任与可调试性。

Rust源码输出是Almide“让LLM最准确编写的语言”使命的一部分。LLM/用户能读emit结果的信任、Rust crate源码级互操作、Cargo共享基础设施，其价值高于egg + MLIR统一形式的美感。

<a id="サブステージ-stage-1-4"></a>

## 子阶段：Stage 1–4

<a id="stage-1--egg-rewriter--almide-dialect-3-ヶ月--0160"></a>

### Stage 1 — egg重写器与Almide dialect（3个月 / `0.16.0`）

<a id="stage-1a--egg-rewriter-単独駆動--完了-2026-04-19-pr-217-merged-on-develop-636b8540"></a>

#### Stage 1A — 单独启用egg重写器 ✅ **完成**（2026-04-19，PR #217合入develop `636b8540`）

Stage 1A以纯Rust、无需FFI完成。该历史记录撤去两层命令式fusion pass，使egg成为唯一fusion driver。

- [x] 为`almide-egg-lab`、`almide-codegen`添加`egg`依赖。
- [x] 通过egg `Language` trait暴露Almide IR（`AlmideExpr`：list + matrix = 13 + 6 variants）。
- [x] 将stdlib `@rewrite`声明编译为egg规则，由`crates/almide-egg-lab/buildscript/egg_rules.rs`自动生成，stdlib为单一source。
- [x] cost function v1：atomic 200 / fused 110 / compose 1；extractor总选fused形态。
- [x] 在Nanopass起点插入等式饱和。
- [x] matrix bridge（IR ↔ RecExpr）：13 variants、slot opaque leaves与let-split inline实现round-trip。
- [x] list bridge：map/filter/fold/flat_map/filter_map及四种compose marker（`compose-fold` / `compose-flatmap` / `compose-map-filter` / `compose-fm-fold`）的beta-reduction。
- [x] 撤去MatrixFusionPass（`pass_matrix_fusion.rs`、`pass_matrix_fusion_rules.rs`及generated文件）。
- [x] 撤去StreamFusionPass，删除`pass_stream_fusion/`五个文件。
- [x] 撤去`--opt egg`、`ALMIDE_OPT_EGG`、`CodegenOptions.opt_egg`，egg默认持续启用。
- [x] spec/ 218/218通过，净减1183行，PR #217。

**Stage 1A剩余，Stage 4 PGO时重新引入**：
- [ ] 通过egg重新引入fma/fma3优化：需扩展schema，从`@rewrite`自动生成可计算系数的Applier。记录称Transformer/Llama 1-block bench不使用相关chain，因此不受影响。

<a id="stage-1b--mlir-dialect--完了-2026-05-11-option-b-採択"></a>

#### Stage 1B — MLIR dialect ✅ **完成**（2026-05-11，采纳Option B）

在`almide-dialect` crate实现纯Rust dialect schema，无MLIR FFI。

- [x] 创建workspace member `almide-dialect`。
- [x] `types.rs`：`DialectType` enum及`Ty→DialectType` converter，覆盖所有Almide类型。
- [x] `ops.rs`：`OpKind`含30+操作（const/binop/call/match/lambda/fan/for/while/record/list/map/result/option/computed_call）。
- [x] `lower.rs`：`IrProgram → Module` SSA lowering（ValueId/BlockId/Terminator）。
- [x] `verify.rs`：良构检查，检测未定义value与Unknown type。
- [x] `dump.rs`：MLIR式文本pretty-printer，`almide emit --emit-dialect`。
- [x] `emit_rust.rs`：dialect → Rust源码，带NameMap、use-count clone、borrow insertion、variant qualification。
- [x] `emit_llvm.rs`：经Inkwell emit LLVM IR，feature gate为`llvm = ["inkwell"]`，要求LLVM 22。
- [x] CLI：`--emit-dialect`、`--target dialect`（带runtime的Rust）、`--target llvm`（LLVM IR）。
- [x] 七个等价测试通过：variants、list ops、Result match、closures/HOF、records、arithmetic、maps。
- [x] CI通过，LLVM feature可选，默认build不依赖LLVM。

已知缺口：
- [ ] stdlib borrow insertion按module/function名称启发式判断，非类型驱动。
- [ ] dialect emitter未处理`test "name" { }`测试块。
- [ ] LLVM后端仅支持i64/f64/bool，尚无String/List/Map/records。
- [ ] dialect emit跳过Type.method惯例方法。

<a id="stage-2--progressive-lowering--rust-emit-再配線-partially-done"></a>

### Stage 2 — 渐进lowering与Rust emit重接线，部分完成

- [x] dialect → Rust源码并嵌入runtime：`codegen_dialect()` + `embed_rust_runtime()`。
- [x] 简单程序经dialect流水线正确编译运行。
- [ ] Almide dialect → arith/scf/memref lowering，延至采用melior。
- [ ] 现有walker全面迁移至dialect，逐步进行，以全测试套件通过为条件。
- [ ] 在dialect上重做nanopass流水线，包括StdlibLowering、BorrowInsertion等。

<a id="stage-3--wasm--native--gpu-3-ヶ月--0170"></a>

### Stage 3 — WASM / native / GPU（3个月 / `0.17.0`）

- LLVM后端 → native binary（Linux/macOS AOT）：已实现Inkwell `emit_object()`，stdlib runtime集成未完成。
- LLVM后端 → WASM：可通过Inkwell `target-webassembly` feature实验。
  - 与现有direct WASM emit做性能比较。
  - direct emit runtime较轻，约10KB，保留是否混合使用的决策。
- **首个GPU PoC**：通过SPIR-V或NVVM dialect offload `matrix.multiply`。
  - schedule DSL作为`@schedule(device=gpu)`运作。
  - 以experimental flag开放`almide build --target cuda matrix_kernel.almd`。

<a id="stage-4--安定化と最適化-2-ヶ月--017x"></a>

### Stage 4 — 稳定与优化（2个月 / `0.17.x`）

- 将egg cost function扩展为profile-guided，PGO。
- 默认流水线纳入LLVM标准优化，inline、LICM、vectorize。
- 全部迁移至egg rule与MLIR pattern后撤去既有命令式Nanopass。
- 用dojo MSR测量arc前后差异，确认零回归。
- 旧案将`--opt egg`默认启用，已在Stage 1记录完成；与剩余PGO/MLIR研究区分。

最初估计合计**10个月**，属历史估计，并非当前期限：0.15.0完成Stdlib Unification，0.16.0完成egg + MLIR基础，0.17.0实现GPU PoC。

<a id="stdlib-unification-arc-との関係"></a>

## 与Stdlib Unification arc的关系

本arc也是[stdlib-declarative-unification.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/stdlib-declarative-unification.md)的**投入回收阶段**：

| Stdlib Unification产物 | 本arc用途 |
|---|---|
| `stdlib/<m>.almd`中的纯Almide body | Almide dialect输入 |
| typed intrinsic（`@intrinsic(rust=..., wasm=...)`） | 表示为MLIR FunctionImport |
| `@rewrite`声明规则 | **自动编译**为egg rewrite规则 |
| `@schedule` block | 转换为affine dialect schedule attribute |

Stdlib Unification整理本arc的**输入格式**。可以跳过，但原文估计stdlib若保留三层定义，MLIR移植工作量翻倍。顺序不变。

<a id="非目標"></a>

## 非目标

- **不在MLIR重现Rust borrow checker**，Rust emit路径借用检查交给rustc。
- **不引入整个MLIR生态**，TensorFlow dialect、Torch-MLIR、CIRCT等近期不在范围。
- **不将LLVM IR作为Almide用户表面**，LLVM dialect仅内部使用。
- **不在frontend compile暴露egg**，用户只用`@rewrite`写规则，egg为内部引擎。

<a id="未決事項"></a>

## 未决事项

1. **egg Language trait设计**：如何嵌入Almide IR；使用单一`Expr`还是按node kind拆分？
2. **cost function v1**：先用node数＋target系数，profile-guided延至Stage 4；中间是否允许用户定义cost？
3. **TableGen或builder API**：Almide dialect定义初期用纯Rust builder API开发更快，稳定后重评TableGen迁移。
4. **WASM direct emit未来**：Stage 3比较LLVM-WASM性能后决定保留混合还是完全替换。
5. **首个GPU runtime target**：CUDA、ROCm、Vulkan、WebGPU。WebGPU自然延伸WASM，但原文认为生态成熟度低。
6. **等式饱和终止**：egg依iteration limit；需实测观察Almide可容忍多少timeout。
7. **增量编译集成**：设计salsa-style与MLIR pass manager边界。

<a id="成功判定"></a>

## 成功标准

- `0.17.0`能运行`almide build --target cuda matrix_multiply.almd`。
- Rust/WASM target零回归，spec/全部通过，dojo MSR无变化。
- stdlib三层定义为零，与Stdlib Unification完成配合。
- `cargo build --release`编译速度在基线±20%内，经egg + MLIR也不退化。
- benchmark证明egg等式饱和发现严格多于现有StreamFusion的最优形态。

<a id="当初のバージョンスケジュール履歴現在のリリース計画ではない"></a>

## 原始版本时间表：历史记录，并非当前发布计划

```
0.14.x         — 前段完了 (v0.14.7 Dispatch Ideal Form)、Stdlib Unification 準備
0.15.0         — Stdlib Unification 完了
0.16.0 ✅      — Stage 1A (egg rewriter) + Stage 1B (dialect schema + LLVM PoC)
                 + bit introspection + Hashable protocol
0.16.x (当初の次段)  — Stage 2 continuation: dialect walker を既存テストスイートで検証、
                 borrow insertion の型駆動化、nanopass の dialect 上再実装
0.17.0         — Stage 3 (LLVM native binary / GPU PoC)
0.17.x         — Stage 4 の当初案 (PGO、imperative pass 全廃。egg default 化は Stage 1 で完了)
1.0 前提       — 本 arc 完走
```

时间表译文：0.14.x完成前置v0.14.7 Dispatch Ideal Form并准备Stdlib Unification；0.15.0完成该统一；0.16.0完成Stage 1A/1B及bit introspection、Hashable。原先下一阶段0.16.x用既有测试验证dialect walker，使borrow insertion类型驱动，在dialect上重做nanopass。0.17.0对应LLVM native/GPU PoC，0.17.x原计划为PGO与命令式pass全撤，egg默认启用已在Stage 1完成。此历史时间表把arc全完成作为1.0前提。

目标终点是“**纯Almide经egg整理为最优形态，在Rust/WASM/native/GPU任一target落为最优代码**”的编译器，即“世界最佳理想形态”在dispatch与优化侧的愿景。
