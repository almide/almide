原文 description 元数据：围绕 LayoutRegistry、WasmIR、WasmBuilder 全面重设计 WASM engine
<a id="almide-wasm-engine--complete-redesign"></a>

# Almide WASM Engine——全面重设计

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](wasm-engine-redesign.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](wasm-engine-redesign.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](wasm-engine-redesign.zh-CN.md)

> 固定原文的非规范读者快照；原文仍为权威，不自动同步。未独立验证实现或执行引用指令。历史状态、计划及比较主张保留原范围。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/wasm-engine-redesign.md)

> 原文注记：位于 done 不代表所有 phase 已交付；开头明确指出这是无完成日期的历史 emitter 提案。正文广泛 stack/layout correctness 和有效字节主张是设计提议，不是内存安全、全部语义或当前 component 支持的独立证明。原样保留简单 stack depth 草图，不视作完整类型/control-flow 验证。数字指标和 Wasm/WASI version 标签均属历史，未重测或更新。


> **状态**：历史设计提案。下述基础/剩余工作描述旧 emitter，而非当前 compiler。文件作为归档设计记录保留于 `done/`；未记录完成日期，该位置不表示每个提议 phase 都已交付。当前 structural emitter 路径及限制见 [WASM-OUTPUT](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/wasm/WASM-OUTPUT.md) 和 [wasm_route.rs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/src/wasm_route.rs)。
> **动机**：手写 WASM 输出是 correctness 最大缺口。此外，WASM 3.0、WASI Preview 2、Component Model 都需要正规编译 pipeline，而非原始指令组装。
> **目标**：将 40 文件手写 assembler 换成识别类型、layout-safe、stack-verified 的 WASM compiler，输出 core module 或 Component Model component。

<a id="problem-statement"></a>

## 问题陈述

当前 `emit_wasm/` 是 assembler：AlmideIR → 原始 `wasm-encoder` 指令。每次 memory access 都手算 offset，stack balance 由人验证。

**后果**：
- layout 变动 → grep 40 文件 → 漏掉位置 → 静默 memory corruption
- 无静态 stack-effect 验证 → runtime stack bug
- Swiss Table map 迭代每个 method 需 30 多行 boilerplate
- WASI Preview 1 原始 import 无法与 Component Model ecosystem 互操作
- 无 WIT 生成 → 无法参与 WASM component ecosystem

<a id="architecture"></a>

## 架构

```
AlmideIR (after nanopass pipeline)
  ↓
┌──────────────────────────────────────────────────┐
│  Almide WASM Engine v2                           │
│                                                  │
│  LayoutRegistry          ← memory layout SoT     │
│  Lowering (AlmideIR → WasmIR)                    │
│  Stack-Effect Verifier   ← static balance proof   │
│  Optimize (RC fusion, local coloring, peephole)   │
│  Core Emit (WasmIR → wasm-encoder)               │
│  Component Wrap (core → component + WIT)          │
│  WASI Adapter (canonical ABI for system calls)    │
│                                                  │
└──────────────────────────────────────────────────┘
  ↓                        ↓
Core WASM module     Component (.wasm + WIT)
```

图示翻译：nanopass pipeline 后的 AlmideIR → Almide WASM Engine v2。内含 memory layout 事实来源 LayoutRegistry、AlmideIR → WasmIR lowering、静态 balance 证明的 Stack-Effect Verifier、优化（RC fusion、local coloring、peephole）、WasmIR → wasm-encoder 的 Core Emit、core → component + WIT 的 Component Wrap、system call canonical ABI 的 WASI Adapter。输出 Core WASM module 或 Component（.wasm + WIT）。

<a id="key-design-decisions"></a>

### 关键设计决定

1. **内部 layout 保持 Almide 原生**（LayoutRegistry）。内部数据无 canonical ABI overhead；canonical ABI 只用于 component 边界（import/export）。
2. **stack effect 是结构性的，不是注解性的**。每个 WasmIR Op 声明 `(pops: u8, pushes: u8)`；verifier 计算每个 block/function 的净 stack effect，并在输出前拒绝不匹配。
3. **Component Model 是 wrapper，不是重写**。core module 正常编译，独立 Component Wrap phase 生成以下内容，将它提升为 component：
   - 由 Almide `pub fn` / `pub type` export 生成 WIT 声明
   - canonical ABI adapter 函数（Almide layout ↔ canonical layout）
   - 经 `wit-component` 的 WASI Preview 2 import
4. **WASI Preview 2 替换 Preview 1**。当前原始 import（`fd_write`、`clock_time_get` 等）替换成 Component Model import（`wasi:filesystem/types`、`wasi:http/outgoing-handler` 等）。

<a id="core-components"></a>

## 核心 component

<a id="1-layoutregistry-exists"></a>

### 1. LayoutRegistry（已存在）

声明式定义全部 memory layout，其他地方不设 hardcoded offset。

内置 layout：String、List、SwissMap、Set、AllocHeader、ClosurePair、Variant、Option、Result。

<a id="2-wasmir-with-stack-effects"></a>

### 2. 带 stack effect 的 WasmIR

```rust
pub struct WasmOp {
    pub kind: WasmOpKind,
    pub pops: u8,    // values consumed from stack
    pub pushes: u8,  // values produced onto stack
}

pub enum WasmOpKind {
    // ── Typed memory access (layout-safe) ──
    FieldLoad { layout: LayoutId, field: FieldId },   // pops: 1 (base), pushes: 1
    FieldStore { layout: LayoutId, field: FieldId },   // pops: 2 (base, val), pushes: 0
    FieldAddr { layout: LayoutId, field: FieldId },    // pops: 1 (base), pushes: 1 (addr)

    // ── Collection iteration (single IR node) ──
    ListForEach { body: Vec<WasmOp> },     // pops: 1 (list), pushes: 0
    MapForEach { body: Vec<WasmOp> },      // pops: 1 (map), pushes: 0

    // ── Allocation ──
    Alloc { layout: LayoutId, size: SizeExpr },
    AllocCollection { layout: LayoutId, len_local: Local, stride: u32 },

    // ── Reference counting ──
    RcInc,                                  // pops: 1 (ptr), pushes: 0
    RcDec { layout: LayoutId },            // pops: 1 (ptr), pushes: 0
    CowCheck { clone_body: Vec<WasmOp> },  // pops: 1 (ptr), pushes: 1 (ptr or clone)

    // ── Structured control flow ──
    Block { body: Vec<WasmOp>, result_ty: Option<ValType> },
    Loop { body: Vec<WasmOp> },
    If { then: Vec<WasmOp>, else_: Vec<WasmOp>, result_ty: Option<ValType> },
    Br(u32),
    BrIf(u32),
    Return,
    ReturnCall(FuncId),          // WASM 3.0 tail call

    // ── Stack primitives ──
    LocalGet(Local),
    LocalSet(Local),
    LocalTee(Local),
    Const(WasmConst),
    Drop,
    BinOp(WasmBinOp),
    Call(FuncId),
    CallIndirect { sig: SigId },

    // ── High-level ops (lowered to primitives in emit) ──
    StringConcat,                           // pops: 2, pushes: 1
    StringInterp { part_count: u32 },       // pops: N, pushes: 1
    DeepEq { ty: AlmideTy },               // pops: 2, pushes: 1 (i32 bool)

    // ── Component Model canonical ABI ──
    CanonLift { wit_func: WitFuncId },      // core → component boundary
    CanonLower { wit_func: WitFuncId },     // component → core boundary
    CanonStringToMemory,                    // canonical string → Almide string
    CanonStringFromMemory,                  // Almide string → canonical string
    CanonListToMemory { elem_ty: ValType }, // canonical list → Almide list
    CanonListFromMemory { elem_ty: ValType },

    // ── Memory ──
    MemoryCopy { dst_mem: u32, src_mem: u32 },
    MemoryGrow(u32),
    MemorySize(u32),

    Seq(Vec<WasmOp>),
    Unreachable,
}
```

代码注释翻译：pops 是从 stack 消耗的值数，pushes 是生成的值数。类型化 memory access 为 layout-safe：FieldLoad 从 1 个 base 生成 1 个值，FieldStore 消耗 base 和 val 共 2 个值而产生 0 个，FieldAddr 从 1 个 base 产生 1 个 address。collection iteration 是单一 IR node，消耗 1 个 list/map、产生 0 个。其后为 allocation、reference counting（RcInc/RcDec 消耗 1 个 pointer、产生 0 个；CowCheck 从 1 个 pointer 产生 1 个原 pointer 或 clone）、结构化 control flow（ReturnCall 是 WASM 3.0 tail call）、stack primitive。高层 operation 在 emit 中 lowering 为 primitive：StringConcat 是 2→1，StringInterp 是 N→1，DeepEq 是 2→1 个 i32 bool。Component Model canonical ABI 包含 core/component 边界 lift/lower、canonical string/list 与 Almide string/list 相互转换，末尾为 memory operation。

<a id="3-stack-effect-verifier"></a>

### 3. Stack-Effect Verifier

```rust
fn verify_stack_balance(ops: &[WasmOp], expected_result: Option<ValType>) -> Result<(), StackError> {
    let mut depth: i32 = 0;
    for op in ops {
        depth -= op.pops as i32;
        if depth < 0 { return Err(StackError::Underflow { op, depth }); }
        depth += op.pushes as i32;
    }
    let expected = if expected_result.is_some() { 1 } else { 0 };
    if depth != expected {
        return Err(StackError::Imbalance { expected, actual: depth });
    }
    Ok(())
}
```

递归运行于每个 Block/If/Loop body，嵌套 scope 独立验证。

<a id="4-lowering-almideir--wasmir"></a>

### 4. Lowering（AlmideIR → WasmIR）

语义翻译，每个 AlmideIR node 映射为 WasmIR operation。

<a id="5-core-emit-wasmir--wasm-encoder"></a>

### 5. Core Emit（WasmIR → wasm-encoder）

机械翻译，是**唯一**接触原始 WASM 指令的位置。输入验证正确的 WasmIR，输出有效 WASM byte。

<a id="6-component-wrap"></a>

### 6. Component Wrap

从 core module 生成 Component Model component：

```
Core WASM module
  + WIT declarations (from pub fn/type exports)
  + Canonical ABI adapter functions
  + WASI Preview 2 import declarations
  = Component (.wasm)
```

图示翻译：Core WASM module + 来自 pub fn/type export 的 WIT 声明 + canonical ABI adapter 函数 + WASI Preview 2 import 声明 = Component（.wasm）。

用 `wit-component` crate 承担繁重工作，Almide 生成：
- **从类型生成 WIT**：`pub type User = { name: String, age: Int }` → `record user { name: string, age: s64 }`
- **从函数生成 WIT**：`pub fn greet(name: String) -> String` → `greet: func(name: string) -> string`
- **canonical ABI adapter**：在 export 边界转换 Almide 内部 layout 与 canonical ABI

<a id="7-wasi-preview-2-adapter"></a>

### 7. WASI Preview 2 Adapter

将原始 WASI Preview 1 import 替换为 Component Model import：

| 当前（Preview 1）| 新方案（经 CM 的 Preview 2）|
|---------------------|----------------------|
| `fd_write(fd, iovs, ...)` | `wasi:cli/stdout.get-stdout` + `output-stream.write` |
| `fd_read(fd, iovs, ...)` | `wasi:cli/stdin.get-stdin` + `input-stream.read` |
| `path_open(...)` | `wasi:filesystem/types.open-at` |
| `clock_time_get(...)` | `wasi:clocks/monotonic-clock.now` |
| （无）| `wasi:http/outgoing-handler.handle` |

<a id="implementation-phases"></a>

## 实现阶段

<a id="phase-0-stack-effect-verifier"></a>

### Phase 0：Stack-Effect Verifier

向既有 WasmIR Op enum 添加 `(pops, pushes)`，实现 verifier，在 WasmBuilder 输出上运行。原构想认为仅此即可关闭 engine 层 correctness 缺口。

- [ ] 向 WasmIR Op 添加 stack-effect field
- [ ] 实现 `verify_stack_balance()` 递归 checker
- [ ] 接入 WasmBuilder——构建每个函数后验证
- [ ] gate：`almide test spec/ --target wasm` 的 stack-effect 违规为零

<a id="phase-1-new-lowering"></a>

### Phase 1：新 lowering

用 AlmideIR → WasmIR lowering 替代手写输出。

- [ ] 全部 IrExprKind variant 的 `lower_expr()`
- [ ] 全部 IrStmtKind variant 的 `lower_stmt()`
- [ ] 带 local allocation 和结构化 control flow 的 `lower_function()`
- [ ] Core emit：WasmIR → wasm-encoder（机械）
- [ ] gate：全部 `spec/lang/` + `spec/stdlib/` 通过新 pipeline（240/240）

<a id="phase-2-component-model"></a>

### Phase 2：Component Model

- [ ] 从 Almide pub export 生成 WIT（`almide build --target wasm-component`）
- [ ] canonical ABI adapter 生成（string、list、record、variant、option、result）
- [ ] 集成 `wit-component` 做 component wrapping
- [ ] gate：编译后的 component 通过 `wasm-tools validate --features component-model`

<a id="phase-3-wasi-preview-2"></a>

### Phase 3：WASI Preview 2

- [ ] 用 `wasi:cli/std{in,out,err}` 替换 fd_write/fd_read
- [ ] 用 `wasi:filesystem/types` 替换 path_open/fd_seek 等
- [ ] 用 `wasi:clocks/monotonic-clock` 替换 clock_time_get
- [ ] 为 HTTP client 添加 `wasi:http/outgoing-handler`
- [ ] gate：`almide run app.almd --target wasm` 在带 WASI Preview 2 的 wasmtime 运行

<a id="phase-4-exception-handling-wasm-30-v2"></a>

### Phase 4：异常处理（WASM 3.0 v2）

等待 wasmtime 默认启用。

- [ ] effect fn 的 `?` 传播采用 `try_table`/`throw`
- [ ] zero-cost error path（无 Result heap allocation）
- [ ] gate：effect fn benchmark 显示可测改善

<a id="phase-5-component-model-async-wasm-30-v3"></a>

### Phase 5：Component Model Async（WASM 3.0 v3）

- [ ] `fan` → `future<T>` + `waitable-set` 多路复用
- [ ] streaming data 用 `stream<T>`
- [ ] gate：fan benchmark 在带 WASI 0.3 的 wasmtime 运行

<a id="what-dies"></a>

## 被淘汰的部分

当前全部 `emit_wasm/`（40 文件，约15K LOC）：

| 当前 | 替代 |
|---------|------------|
| `expressions.rs`、`statements.rs`、`control.rs` | `lower_expr()`、`lower_stmt()` |
| `calls_*.rs`（12 文件）| stdlib lowering rule |
| `rt_*.rs`（6 文件）| 以 WasmIR sequence 表示 runtime |
| `runtime.rs`、`runtime_eq.rs` | emit phase primitive |
| `closures.rs`、`calls_lambda.rs` | closure lowering |
| `equality.rs`、`collections.rs` | `DeepEq` operation、collection lowering |
| `scratch.rs` | emit phase 中的 local allocator |
| `values.rs` | LayoutRegistry |
| `dce.rs` | WasmIR 层 DCE pass |

保留 `engine/`（LayoutRegistry、WasmIR、WasmBuilder、emit）——这就是新 engine。

<a id="existing-foundation"></a>

## 既有基础

已在 `emit_wasm/engine/` 实现：
- **LayoutRegistry**：全部 layout 已定义（String、List、SwissMap、Set、AllocHeader、ClosurePair、Variant、Option、Result）
- **WasmIR**：40+ operation variant，带类型化 memory access
- **WasmBuilder**：chainable API，支持 layout-safe field access、collection iteration、RC operation
- **Emit**：WasmIR → wasm-encoder 翻译

已有 23 文件迁移至 LayoutRegistry。WasmBuilder 用于 list_layout.rs 和 Perceus core。

<a id="metrics"></a>

## 指标

- **correctness**：`almide test spec/ --target wasm`——240/240
- **stack safety**：零 stack-effect 违规（结构性强制）
- **layout safety**：零 hardcoded offset（由 LayoutRegistry 强制）
- **binary size**：≤ 当前
- **performance**：≥ 当前
- **code 量**：emit_wasm/ LOC 减少 ≥50%
- **Component Model**：通过 `wasm-tools validate --features component-model`
- **WASI Preview 2**：无需 Preview 1 adapter 在 wasmtime 运行
