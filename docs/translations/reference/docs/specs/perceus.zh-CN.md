<a id="almide-perceus-type-guided-automatic-memory-management"></a>

# Almide Perceus：类型指导的自动内存管理

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](perceus.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](perceus.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](perceus.zh-CN.md)



> 权威原文：[docs/specs/perceus.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/perceus.md)，固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`。这是非规范性的读者快照译文，不自动同步。原规格及其声明的规范地位保持不变。模型证明、实现主张、历史状态和证据边界均保留原文范围；本译文不提供独立执行、测量、符合性或安全性证明。代码块内代码及注释逐字保留。

> 最后更新：2026-10-31
>
> 用户代码无需所有权注解或显式 `free`。所有权推断和引用计数机制是实现机制，不是所有被接受的程序或生成产物都内存安全的证明。

<a id="overview"></a>

## 概述

Almide 使用 Perceus 风格的所有权分析，决定堆值在哪里引入、共享和消耗。执行因目标而异：原生输出使用 Rust 所有权／运行时操作，结构化 Wasm 发射器管理自己的堆和引用计数。关于已证明性质、受信任组件及未解决缺口的当前说明见[内存安全](../../../../design/MEMORY-SAFETY.md)。

<a id="reference-counting-model"></a>

## 引用计数模型

这些是概念规则，不是为每个源码表达式承诺的逐条指令序列。降层转换、借用、静态值和优化都会影响所需操作。

1. **共享**：必须计入新的拥有型引用；不能直接转移所有权或借用时，通常通过增加计数实现。
2. **释放**：必须计入对拥有型引用的消耗或丢弃。计数降到零时，减计数操作可释放分配。
3. **复合 drop**：释放拥有型容器时，也必须计入它拥有的堆值。布局和表示决定如何访问子值，闭包环境也属于这项义务。

静态字符串、拥有的堆缓冲区和借用引用不是可互换的情况。一个值只使用一次，不意味着可以省略其最终释放。

<a id="optimizations-and-limits"></a>

## 优化与限制

所有权和唯一性信息允许受支持操作复用内存或原地更新。融合可避免中间容器。这些是有条件的优化，不保证任意 `map` / `filter` / `fold` 流水线都不分配内存。回调行为、表示、别名及选择的降层转换都重要。

单次使用的值及拥有的临时值可避免冗余共享操作，但仍需要正确消耗或清理。单次使用或类型已解析本身，都不能证明所有 RC 操作可删除。

<a id="verification-boundary"></a>

## 验证边界

实现具有多种证据，其范围各不相同：

- [`perceus_verified.rs`](../../../../../crates/almide-codegen/src/perceus_verified.rs) 对应 Lean 定义，并指明模型内部的插入／计数性质。这些性质不能证明完整生成程序中每项降层转换和运行时操作正确。
- Rocq/Coq 证书和内存安全模型见[内存安全](../../../../design/MEMORY-SAFETY.md)及 [`proofs/Certified.v`](../../../../../proofs/Certified.v)。其中声明的假设和所表示的轨迹是主张的一部分，不是可以省略的细节。
- [`almide verify`](../../../../../src/cli/verify.rs) 生成结构化 Wasm 见证，并调用独立的 `almide-verify` 二进制。产物字节绑定将一个包与特定文件关联；它不会把当前检查变为不受限制的整个产物内存安全定理。

尤其是，**目前类型检查器接受，并不蕴含每个生成产物都有已证明的无泄漏／无释放后使用／无重复释放定理**。尚未履行的降层转换、轨迹和原生运行时义务仍列在链接的内存安全缺口表中。不要从类型指导 RC 模型推断它们已成立。

<a id="references"></a>

## 参考文献

- Reinking et al., "Perceus: Garbage Free Reference Counting with Reuse" (ICFP 2021)
- Ullrich & de Moura, "Counting Immutable Beans" (IFL 2019)
- Tofte & Talpin, "Region-Based Memory Management" (POPL 1997)
