<a id="gpu-compute--matrix-type-and-compiler-driven-gpu-execution"></a>

# GPU 计算——Matrix 类型与编译器驱动的 GPU 执行

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](gpu-compute.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](gpu-compute.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](gpu-compute.zh-CN.md)

> 非规范固定读者快照。未改变的原文仍为权威来源，不替换机器输入。不自动同步，也未独立执行。区分保留历史测量、提案及原文时点的实现更新，不构成当前正确性或性能的独立证明。代码与注释保持原样。 10月31日状态将已交付 Matrix/CPU 基础、现有 GPU 路径与尚未实现的 CUDA/grad/训练提案区分。保留历史清单未勾选状态。消除 bug 和后端支持的说法是设计目标，不是已验证的普遍保证。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/gpu-compute.md)

> 原 metadata 非活动摘要：具有编译器驱动 CPU/GPU 执行的 Matrix 基元类型。

**优先级：** Phase 3（运行时基础）
**前提：** Bytes 类型完成、WASM export 完成、nanopass 流水线完成
**原则：** 用户编写普通 Almide 代码，不考虑 GPU。`--target cuda` 在 GPU 上运行，否则在 CPU 上运行。
**语法约束：** 不新增关键字。只添加 `Matrix` 基元类型和 `grad` 内置函数。

> “你写 Almide；在 CPU 还是 GPU 上运行，由编译器决定。”

---

<a id="current-foundation-and-remaining-proposal-2026-10-31"></a>

## 当前基础与剩余提案（2026-10-31）

`Matrix` 和 CPU 矩阵运算已经存在：见 [`stdlib/matrix.almd`](../../../../../../stdlib/matrix.almd)，包括构造器、shape/transpose 和运算符 lowering helper。运行时位于 [`runtime/rs/src/matrix.rs`](../../../../../../runtime/rs/src/matrix.rs)。仓库也有独立 GPU 执行路径 [`almide-gpu-run`](../../../../../../crates/almide-gpu-run)，但其存在不代表下方提案中的 `--target cuda`、`grad` 或自动训练循环已实现。

下方清单是**原始分阶段设计**，不是当前 Matrix 缺失功能列表。尤其 Step 1 的类型／CPU／运算符基础已通过 `.almd` stdlib 交付（旧 `stdlib/defs/*.toml` 源格式已退役）。CUDA/autograd 集成和拟议的类型级 shape 设计仍是独立决策，不可推断为已完成。

<a id="design-philosophy"></a>

## 设计理念

<a id="why"></a>

### 为什么

对 Almide“LLM 能最准确编写的语言”这一使命而言，最大的未开发领域是 ML。LLM 编写 ML 代码时最常见的 bug：
1. 张量维度不匹配
2. 设备放置错误（CPU/GPU）
3. 梯度流断开

Almide 类型系统加编译器优化可以消除这些问题。

<a id="design-principles"></a>

### 设计原则

1. **不新增概念**——`Matrix` 像 `Bytes` 一样是基元，`grad` 像 `println` 一样是内置函数。
2. **用运算符写**——`x * w` 是矩阵乘法，而不是 `tensor.matmul(x, w)`。
3. **隐藏 GPU**——后端随 target 标志改变，代码不变。
4. **利用现有 Rust 生态**——burn/candle 已提供 autograd、GPU 执行和内存管理，不重复造轮子。
5. **通过 nanopass 融合**——与 `map |> map` 融合相同的机制，将逐元素运算变成融合 kernel。

---

<a id="phases"></a>

## 阶段

<a id="step-1-matrix-primitive-type-historical-foundation"></a>

### Step 1：Matrix 基元类型（历史基础）

与 Bytes 类型同样的模式，最小化编译器更改。

**语言侧：**
```
let w = matrix.zeros(512, 1536)
let x = matrix.randn(32, 512)
let y = x * w + bias
let shape = matrix.shape(y)  // → (32, 1536)
```

**编译器实现：**
- [ ] 向 `Ty` enum 添加 `Ty::Matrix`
- [ ] 注册 `TypeConstructorId::Matrix`
- [ ] 向 parser/checker 类型解析添加 `"Matrix" => Ty::Matrix`
- [ ] Rust codegen：`ndarray::Array2<f64>`（CPU）
- [ ] `stdlib/defs/matrix.toml`——zeros、randn、shape、transpose、from_lists、to_lists
- [ ] `runtime/rs/src/matrix.rs`——ndarray wrapper
- [ ] 测试

**运算符重载：**
- [ ] `(Matrix, Matrix)` 的 `*` → 矩阵乘法
- [ ] `(Matrix, Matrix)` 的 `+` `-` → 逐元素运算
- [ ] `(Matrix, Float)` 的 `*` `/` → scalar 运算
- [ ] 向 codegen Binary op dispatch 添加 Matrix 模式

**当前基础：** Matrix 运算通过 `almide run` 在 CPU 上执行；上方旧实现清单记录初始计划。

<a id="step-2-gpu-backend"></a>

### Step 2：GPU 后端

通过 `--target cuda` 切换至 burn/candle，无需修改用户代码。

**codegen dispatch：**
```
Ty::Matrix codegen →
  target == Rust:  ndarray::Array2<f64>      (CPU)
  target == Cuda:  burn::Tensor<CudaBackend, 2>  (GPU)
```

**实现：**
- [ ] 添加 `codegen::pass::Target::Cuda`
- [ ] Rust codegen 按 target 分支渲染 Matrix 类型
- [ ] `runtime/rs/src/matrix_gpu.rs`——burn wrapper（matmul、add、transpose 等）
- [ ] 向 `Cargo.toml` 模板添加 burn 依赖
- [ ] `almide build model.almd --target cuda` 生成 GPU 二进制
- [ ] 在 CUDA 环境测试

**验证：** 同一 .almd 在 CPU 和 GPU 上返回相同结果

<a id="step-3-grad-and-training-loops"></a>

### Step 3：grad 与训练循环

`grad(loss, w)` 展开为 burn 的 autodiff。

**语言侧：**
```
fn train_step(x: Matrix, w: Matrix, target: Matrix, lr: Float) -> Matrix =
  let pred = x * w |> leaky_relu(0.5) |> square
  let loss = cross_entropy(pred, target)
  let dw = grad(loss, w)
  w - lr * dw
```

**编译器实现：**
- [ ] 将 `grad(loss, param)` 识别为内置函数
- [ ] codegen：展开为 `loss.backward()` + `grads.get(&param)`（burn autodiff）
- [ ] 标记需跟踪梯度的变量（burn 的 `require_grad`）
- [ ] 向 stdlib 添加激活函数：leaky_relu、relu、softmax、sigmoid、tanh
- [ ] 向 stdlib 添加损失函数：cross_entropy、mse
- [ ] 测试：小型 MLP 训练收敛

**验证：** XOR 问题训练有效

<a id="step-4-nanopass-fusion"></a>

### Step 4：Nanopass 融合

将逐元素运算链融合成一个 kernel，直接影响 Parameter Golf。

**示例：**
```
x * w |> leaky_relu(0.5) |> square
// ↓ nanopass rewrite
fused_elementwise(x * w, [leaky_relu(0.5), square])
// ↓ codegen
// 1 fused CUDA kernel (3 kernel launches → 1)
```

**实现：**
- [ ] IR 模式：`map(map(x, f), g)` → `map(x, f >> g)`（现有）
- [ ] 逐元素数值模式检测 pass
- [ ] 融合 kernel codegen（Triton 或 CUDA C）
- [ ] 基准：融合与未融合速度比较

**验证：** Parameter Golf 基线 forward pass 更快

<a id="step-5-future-type-level-dimension-checking"></a>

### Step 5（未来）：类型级维度检查

```
fn linear(x: Matrix[B, 512], w: Matrix[512, 1536]) -> Matrix[B, 1536] =
  x * w
```

编译时检测维度不匹配，是依赖类型的子集。Phase 3 及以后。

---

<a id="target-mapping"></a>

## 目标映射

```
Almide type    Rust (CPU)              CUDA (GPU)              WASM
──────────────────────────────────────────────────────────────────────
Int            i64                     i64                     i64
Float          f64                     f64                     f64
String         String                  String                  i32 ptr
Bytes          Vec<u8>                 Vec<u8>                 i32 ptr
Matrix         ndarray::Array2<f64>    burn::Tensor<Cuda, 2>   loops (small scale)
```

<a id="dependency-crates"></a>

## 依赖 crate

| Step | CPU | GPU |
|------|-----|-----|
| 1 | ndarray | — |
| 2 | — | burn (burn-cuda or burn-candle) |
| 3 | — | burn-autodiff |
| 4 | — | burn-fusion or Triton |

<a id="non-goals-for-now"></a>

## 当前非目标

| 项目 | 原因 |
|------|--------|
| N 维张量 | Matrix（2D）足够，3D+ 留待未来 |
| 自定义 CUDA kernel | burn/candle kernel 足够 |
| 分布式训练（multi-GPU） | Parameter Golf 使用 8xH100，但先从 1 GPU 开始 |
| 自定义 autograd | 交给 burn；编译器 AD 留待未来考虑 |
| TPU/ROCm 支持 | burn 支持后即可自动使用 |

---

<a id="in-one-sentence"></a>

## 一句话概括

> 添加像 Bytes 一样的 Matrix 基元类型，用运算符编写矩阵运算，用 grad 求梯度，由编译器按 target 发射 CPU/GPU 代码。用户不必知道 GPU 的存在。

代码中的英文注记译文：nanopass rewrite 表示 nanopass 改写，codegen 表示代码生成。该提案例用 1 个融合 CUDA kernel，将 kernel 启动次数从 3 减至 1。目标表中的 loops (small scale) 指小规模循环。
