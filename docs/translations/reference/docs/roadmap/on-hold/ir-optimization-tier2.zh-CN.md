<a id="ir-optimization-tier-2"></a>

# IR 优化 Tier 2

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](ir-optimization-tier2.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](ir-optimization-tier2.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](ir-optimization-tier2.zh-CN.md)

> 非规范性的固定读者快照。以原文为准，不自动同步，未独立重新验证实现、测量或安全性。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/ir-optimization-tier2.md)
> 本搁置路线图区分已有保守 scalar/list-sink 内联与拟议扩展。图是旧布局提案而非完整当前流水线；基准改善是成功条件，不是实测结果。


> 原文描述：用于跨目标 IR 优化的 CSE 与内联 pass

**优先级：** 中——自动应用于所有目标的优化
**前提：** Tier 1（常量折叠、DCE）已完成，nanopass 流水线已建立
**目标：** 扩展 IR 级优化 pass，改善生成代码质量，而非仅依赖下游 native 或 Wasm 引擎优化

> “IR 聪明，所有目标都会聪明。”

---

<a id="why"></a>

## 原因

Tier 1（常量折叠、DCE）已完成，但仍有需要在 IR 级解决的优化：

- 相同子表达式的重复计算
- 超出当前允许的 scalar／list-sink 子集的内联
- 不逃逸对象的堆分配
- 可用低成本替代的高成本操作

共享 IR 优化可惠及 native 与 Wasm 目标。TypeScript 不是当前编译器目标。Wasm 不经过 rustc；下游 Wasm 引擎优化与这些编译器 pass 不同。

---

<a id="passes"></a>

## Pass

<a id="-completed"></a>

### ✅ 已完成

| Pass | 文件 | 概要 |
|------|------|---------|
| LICM | `pass_licm.rs` | 循环不变代码移动。排除 effect fn 调用 |
| StreamFusion | `pass_stream_fusion.rs` | 融合 map/filter/fold 链，消除中间列表 |
| 标量内联 | `crates/almide-wasm/src/inline_calls.rs`, `crates/almide-mir/src/lower/inline_scalar_fns.rs` | 保守的纯标量子集，不是任意函数 |
| List-sink 内联 | `crates/almide-optimize/src/optimize/sink_inline.rs` | 将符合条件的根 list sink 暴露给目标特定融合 |

原来的隐式 `AutoParallelPass` 已退役；显式 `fan` 的降低处理在 `pass_rust_lowering_fan.rs`，不是通用自动并行化 pass。

<a id="remaining-proposals"></a>

### 剩余提案

<a id="pass-1-common-subexpression-elimination-cse"></a>

### Pass 1：公共子表达式消除（CSE）

将相同表达式的重复计算合并到 let 绑定。

```almd
// Before
let a = list.len() * 2
let b = list.len() * 3

// After (IR level)
let __cse_0 = list.len()
let a = __cse_0 * 2
let b = __cse_0 * 3
```

**条件：** 表达式纯（无效果），且结构相同。

<a id="pass-2-simple-inlining"></a>

### Pass 2：简单内联

仅在已有 scalar／list-sink pass 不适用之处扩展内联。下述原始使用次数／单表达式策略是拟议扩展，并非 Almide 当前完全不内联的证据。

```almd
fn double(x: Int) -> Int { x * 2 }
let y = double(5)
// → let y = 5 * 2 → 10 (chains with constant folding)
```

**约束：**
- 本体仅为一个表达式
- 非递归
- 使用次数 ≤ 阈值（初始值：1）

<a id="pass-3-strength-reduction-future"></a>

### Pass 3：强度削减（未来）

用低成本替代高成本操作。

```
x * 2      → x << 1      (integer)
x / 4      → x >> 2      (positive integer)
x % 2 == 0 → x & 1 == 0  (integer)
```

---

<a id="architecture"></a>

## 架构

```
Lower → IR
         │
         ▼
    ┌─────────────────────────┐
    │ Tier 1 (done)             │
    │  ├── ConstFoldPass       │
    │  └── DCEPass             │
    ├─────────────────────────┤
    │ Tier 1.5 (done)          │
    │  ├── LICMPass            │
    │  ├── StreamFusionPass    │
    │  └── (explicit fan separately) │
    ├─────────────────────────┤
    │ Tier 2 (this roadmap)    │
    │  ├── CSEPass             │
    │  └── InlinePass          │
    ├─────────────────────────┤
    │ Tier 3 (future)          │
    │  └── StrengthReduction   │
    └─────────────────────────┘
         │
         ▼
    mono() → nanopass → codegen
```

这是原先提出的 pass 排列，不是完整的当前流水线图。现有内联器运行于不同 IR／降低处理位置；共享扩展须在实现前明确放置位置及准入规则。

---

<a id="phases"></a>

## 阶段

<a id="phase-1-cse"></a>

### Phase 1：CSE

- [ ] 实现 `CSEPass`（结构相等检查＋插入绑定）
- [ ] 纯度检查（排除含 effect fn 调用的表达式）
- [ ] 测试：stdlib 调用去重

<a id="phase-2-inlining"></a>

### Phase 2：内联

- [ ] 规定并实现超出现有子集的内联，避免重复 pass（使用次数＋本体大小策略待评估）
- [ ] 与常量折叠连用的测试
- [ ] 监测代码体积膨胀（WASM 二进制大小回归测试）

---

<a id="success-criteria"></a>

## 成功条件

- CSE 与 Inline pass 集成到 nanopass 流水线
- WASM 二进制大小不回退（与 DCE 平衡）
- 全部现有测试继续通过
- 基准（Mandelbrot 等）取得可测量的改善
