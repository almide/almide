# almide-kernel

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](README.zh-CN.md)

> 本文是[固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-kernel/README.md)的非规范读者译文；原文仍是权威版本，本译文不会自动同步。




> 本译文是固定源版本的非规范性快照（修订：35297a612fb29e3f78f6587aba253aa91d51f466），不会自动同步。测量结果与主张未经独立重新验证；以英文原文为准。
>
> 原文注意：开头说明已迁移或移除的内核，其测量、证明和实现描述仍作为历史记录保留于正文；它们不是当前实现清单或重新验证的性能保证。

面向 Almide 的各目标 SIMD 数值内核 — **既快，又经过证明**。这是经 Wyve 证明的内核的 Rust 移植：在自动向量化器受到结构性阻碍的地方使用显式 SIMD，在结构允许的地方进行静态验证。

> **Dialect epoch 14 (2026-10-17)：** Q1_0、GELU、SiLU 和 softmax 内核
>（`q1_0`, `q1_0_packed`, `gelu`, `silu`, `softmax`）与它们服务的 `matrix` 函数一起离开了此 crate，移至
> [almide-graphics/nn](https://github.com/almide-graphics/nn) 包（模块 `nn.kernels`）。此 crate 现包含 `matmul`、
> `scale`、`transpose`、`transpose_f64` 和 `bridge`。下面的 q1_0 测量与证明是那项工作的记录；代码在 git 历史中。

## 概览

```
Fast (4-quadrant — beats Rust AND Almide, on native AND wasm):
  transpose 8x8      native 5.18× / wasm 3.66×   (vs Rust)
  q1_0 block dot     native 3.73× / wasm 2.52×
  linear_q1_0 matmul native 6.74× / wasm 3.03×   (real inference hot path)
  scale              ties autovec → ships naive (no ceremony)

Proven (nothing left to "promised"):
  permutation (transpose) → static, all inputs   (index run = total proof)
  selection (q1_0 signs)  → static, exhaustive   (256 bytes = all bit patterns)
  reduction (q1_0 sum)    → bitwise to a named order + bounded error (n·u·Σ|x|)

Per-target: x86 AVX/AVX2 · wasm simd128 · naive fallback (one source, cargo --target)
Standalone: zero deps, 16 tests green, builds native + wasm
```

这是用 Rust 实现、原生面向 Almide 的 Exo 路线（算法/调度分离 + 构造即正确）；对于置换/选择内核，它*比 Exo 更轻量*（无需 SMT 求解器；该结构可判定）。

## 组织形式

```
Wyve (Racket/Lean)      = research lab : design a kernel, prove it bitwise-exact
                                          and in-range (@bounds), in Lean
   ↓ port the proven kernel
almide-kernel (Rust)    = factory      : production SIMD (core::arch),
                                          Almide-native, zero dependencies
   ↑ pinned by
differential test       = bridge       : SIMD == the same naive reference Wyve
                                          proves against → the proof carries over
```

Wyve 不参与链接（没有 Racket/wyvec/LLVM-clang 构建依赖）。它是*证明*内核的地方，而此 crate 是*交付*内核的地方。新内核的流程：在 Wyve 中设计并证明 → 在此移植为 Rust → 用差分测试验证逐位精确一致。

## 内核

| 内核 | 实现 | 正确性 | 速度 |
|---|---|---|---|
| `transpose_8x8` | wasm simd128 shuffle；其他目标用普通分块（x86 AVX 的 3 遍网络没有服务任何程序，已随其 `unsafe` 一起移除，#4342） | 逐位精确一致，100 种模式 | AVX 测得 **默认 1.57× / native 4.23×**；普通分块比它慢 1.28×（GCE Intel，2026-10-30） |
| `scale` | 朴素实现（自动向量化）；仅用于测量的 `scale_avx` 已移除（#4342） | 逐位精确一致（avx == naive） | **native 0.99×** → 保留朴素实现 |
| `q1_0_dot` | AVX2 位解包 + 带符号求和 + 归约 | 容差范围内（重结合归约） | **3.5×（默认和 native 均如此）** |

### 在哪里超越 Rust，以及正确性应采用什么标准

```
transpose (shuffle)   : 4.23× native, bitwise-exact   — autovec can't shuffle
q1_0 (bit-unpack)     : 3.5× any target, reassoc       — autovec can't bit-address
scale (elementwise)   : 1.0× native, bitwise-exact     — autovec already wins → naive
```

优势恰好来自 rustc 自动向量化器受到*结构性*阻碍的地方：shuffle 网络和打包位寻址。无论 target-cpu 如何，q1_0 都以 3.5× 超越 Rust；自动向量化无从下手，因此基线始终无法赶上。

正确性标准取决于操作：**数据移动（transpose）→ 逐位精确一致**；**归约（q1_0）→ 容差范围内**，因为 SIMD 调度重新结合了浮点求和（误差界是*各项绝对值之和*，不是 |result|；结果可能相消到接近零）。为每个操作选择合适的标准才是应遵守的原则。

### 静态等价 — Exo 的思想，置换时更轻量

Exo *静态地*（不执行）证明调度与其算法等价：计算每条语句的读写**效应**，并使用 **SMT 求解器**证明变换后的程序与原程序可交换。almide-kernel 借鉴这一点，对**置换内核**（转置、shuffle）则*无需求解器*：

```
A permutation moves positions, never values. So running it on the index array
input[k]=k extracts the ENTIRE permutation in one shot. If that equals the
transpose spec, the SIMD schedule is correct for EVERY input — a total proof,
not 100 samples. (f32/f64 hold 0..N exactly, so the source index is recovered
losslessly.)
```

`schedule_is_the_transpose_permutation_for_all_inputs`（f32 8x8、f64 8x8，以及包括不整齐边缘的任意尺寸）用对所有输入的静态保证取代抽样差分测试。在 Exo 需要 SMT 求解器的地方，置换一次运行即可判定。

**归约分为两层。** q1_0 不是置换，但其符号的*放置*是，所以我们也证明这一部分。位解包（`apply_sign`）是与值无关的符号位 XOR，一个字节覆盖 8 个通道，因此 **256 个字节值穷尽所有位模式**：`avx2_bit_unpack_total_proof` 证明对*每个*字节和每个输入，符号都会进入正确通道。这是有限、完整的证明，无需求解器。而浮点*求和*也不止停留在“容差范围内”，而是分两层证明：

```
layer 1 (order is the spec) : the SIMD reduction is BITWISE-EXACT to a specified
                              tree-order reduction (8 lanes → lo+hi → hadd → hadd).
                              avx2_is_bitwise_exact_to_tree_order, 500 seeds, no tolerance.
layer 2 (the error is bounded): tree-order vs the idealized exact sum is within
                              n·u·Σ|xₖ| (n=128, u=2⁻²⁴) — a reassociation-error
                              theorem (Lean-provable; the test witnesses it holds).
```

浮点数依赖顺序，所以“和”从来就不是唯一的；一旦将顺序明确为规范，内核便*精确*实现它（第 1 层），与理想值的距离则*有界，而非寄希望于足够小*（第 2 层）。对于浮点归约，这是证明，而不是承诺。

```
q1_0 correctness:  sign placement → PROVEN (256 exhaustive)
                   reduction      → PROVEN (bitwise to tree-order + bounded error)
```

因此，almide-kernel 从头到尾的正确性图谱是：
- 置换（transpose）→ 静态、所有输入（一次索引运行）
- 选择/位解包（q1_0 符号）→ 静态、穷尽（256 字节）
- 浮点归约（q1_0 求和）→ 与指定顺序逐位一致 + 有界误差
没有任何部分仍停留在“承诺”。

### 4 象限：在 native 和 wasm 上同时超越 Rust 和 Almide（q1_0）

|               | native (AVX2) | wasm (simd128) |
|---------------|---------------|----------------|
| 相比 Rust 朴素实现 | **3.73×**     | **2.52×**      |
| 相比 Almide     | **3.62×**     | **2.43×**      |

**transpose**（8x8，两个目标均逐位精确一致）：

|               | native (AVX) | wasm (simd128) |
|---------------|--------------|----------------|
| 相比 Rust 朴素实现 | **5.18×**    | **3.66×**      |
| 相比 Almide     | **2.70×**    | **3.50×**      |

almide-kernel 提供**各目标 SIMD**（x86 AVX/AVX2、wasm simd128）及朴素回退实现。对于 q1_0，Rust 的自动向量化和 Almide 自身的点积都使用标量（Almide 的 SIMD 仅支持 NEON；打包位寻址阻碍自动向量化器）；对于转置，自动向量化器根本无法构建 shuffle 网络。因此 almide-kernel 在两个内核的全部四个单元格中都获胜：在 native *和* wasm 上都比 Rust *和* Almide 快。（Almide 使用 f64，almide-kernel 使用 f32；为任务选择合适位宽也是优势的一部分。）

**完整量化线性运算** `linear_q1_0` — 推理热路径，即 W 为 Q1_0 时的 `x @ Wᵀ`，在 1×2048×2048 下测量（一个 token 经过 2048→2048 层）：

|               | native (AVX2) | wasm (simd128) |
|---------------|---------------|----------------|
| 相比 Rust 朴素实现 | **6.74×**     | **3.03×**      |

（最大相对误差 6.7e-8 / 8.9e-8，在 f32 重结合容差范围内。）矩阵乘法比单块点积的差距*更大*（3.5×→6.74×）：每块 SIMD 调度的成本在整个矩阵上摊销。这是真实工作负载，不是微基准。

**任意尺寸转置** `transpose_matrix` — 这是 `almide_rt` 实际需要的（Almide 的矩阵并不都是 8x8）。完整的 8x8 分块经过 SIMD 内核，不整齐的边缘用标量；任意尺寸均逐位精确一致。在 512×512 下测量：

|               | native (AVX) | wasm (simd128) |
|---------------|--------------|----------------|
| 相比 Rust 朴素实现 | **3.50×**    | **3.15×**      |
| 相比 Almide f64 | **3.13×**    | **3.63×**      |

这就是 `almide_rt` 接入的核心：将任意矩阵分块映射到固定尺寸的 SIMD 内核。

### ABI 形态决定 SIMD 的收益（已测量）

`bridge.rs` 是完整接入（`Vec<Vec<f64>>` → 内核 → `Vec<Vec<f64>>`），逐位精确一致。但在 512×512 下，**同一内核**测得：

|                          | 相比 Almide 朴素实现的加速比 |
|--------------------------|-------------------------|
| 嵌套 `Vec<Vec<f64>>`   | **0.44×**（更慢！）     |
| 扁平 `Vec<f64>`          | **3.07×**               |

仅 ABI 就造成了 7× 的波动。嵌套→扁平 / 扁平→嵌套转换（两次完整复制 + 每行 Vec 分配）的成本超过 SIMD 转置节省的时间。所以生产实践的教训不是内核，而是 Almide 的**类型**：给内核**扁平**缓冲区（SmallF32 / 扁平 f64），就能获得 3×；给它 `Vec<Vec<f64>>`，转换便会吞噬收益。当且仅当矩阵为扁平形式时，接入才有回报。

完整生产接入尚需：Almide 在热路径上采用扁平矩阵 ABI，以及处理 Almide 的构建流程（`almide_rt` 需要注入 prelude；只能在 `almide run`/`build` 内编译）。

AVX 转置复现了 Wyve 已验证的 3 遍 shuffle 网络（解包相邻行 → shuffle 64-bit 组 → 置换 128-bit 通道）。

### 经测量确认的设计原则

```
data-movement (transpose) : autovec can't build a shuffle network → explicit
                            SIMD wins regardless of target-cpu (4.23× native)
elementwise (scale)       : autovec ties explicit AVX at target-cpu=native
                            (0.99×) → ship naive, no ceremony
```

`scale` 在*默认*构建下的 1.23× 是假象：自动向量化被限制在 SSE2 基线。到了 `target-cpu=native` 就是平局。所以 almide-kernel 只在自动向量化器*结构上*无法跟进的地方编写显式 SIMD，即数据移动。其余保持朴素实现。（元素级速度的修复在于 Almide 构建标志，而非此 crate 中的额外形式。）

## Exo 风格：算法 / 调度分离

转置内核像 Exo 一样将**算法**（做什么）与**调度**（怎么做）分离，但就在此 crate 内，用普通 Rust 实现：

```rust
// algorithm: the spec
fn transpose_8x8_naive(..)  { out[j][i] = in[i][j] }

// schedule: named passes, composed — THIS line is the schedule
store(pass_permute(pass_shuffle(pass_unpack(load(input)))))
//    └─ recompose the passes to change how it runs;
//       the algorithm above is untouched.
```

| Exo | almide-kernel |
|---|---|
| 算法与调度分离 | 相同 — 朴素规范 + 组合的 pass |
| 通过效应分析实现程序等价（静态） | 逐位精确一致的差分测试（动态、日常使用）+ Lean（可选的更强保障） |
| 自有后端 | Rust = LLVM，Almide 原生 |
| 人编写调度 | 现在人编写 → **以后由 Almide 推导**（其他人尚未占据的位置） |

测量结果：拆成具名 pass 后仍逐位精确一致，性能为 4.19x（单一整体时为 4.23x）；`#[inline(always)]` 消除了组合开销。调度显式、可重新组合，运行时成本为**零**。

## 为什么单独建 crate

- **没有依赖，无需注入 prelude** — 可独立构建和测试。
  （`almide_rt` 不行：其 src 假定 Almide 的 prelude 会注入 `use` 项，因此只能在完整流水线内编译。此 crate 是普通 Rust。）
- `almide_rt` 为**数据移动操作**（转置、shuffle）调用它，这些操作即使在 native 上，显式 SIMD 也能超越自动向量化器；这正是实测确认的 Wyve 优势所在。

## 基准测试的诚实性

最初两次基准尝试是陷阱，值得记录：
1. 只使用 `out[1]` 会让 rustc 的 DCE 把朴素转置消成一次复制（朴素实现看起来*快* 7×，其实它没在转置）。
2. 把全部 64 个输出求和会让 `sum` 占据主导，掩盖转置差异（1.03×）。

`black_box(&o)` 配合每次迭代变化的输入，强制计算并存储完整转置，仅测量 shuffle 网络：**1.57×**。务必检查微基准没有在测 DCE 或错误的热循环。

## 运行

```
cargo test                              # naive-is-transpose + SIMD==naive bitwise
cargo run --release --example bench     # AVX vs naive
```

> 关于证据类别：原文使用“proven”，正文同时包含500 seeds的测试记录和“Lean-provable（可用Lean证明）”的表述。本译文不将这些内容视为同一种证明类别，也未独立确认Lean证明已完成或Rust移植已对所有输入完成形式化验证。
