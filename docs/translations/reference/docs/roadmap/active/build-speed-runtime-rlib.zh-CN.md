<a id="build-speed-runtime-rlib--hot-fn-inlining"></a>

# 构建速度：运行时 rlib 与热点函数内联

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](build-speed-runtime-rlib.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](build-speed-runtime-rlib.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](build-speed-runtime-rlib.zh-CN.md)

> 固定原文的非规范读者快照，不自动同步。带日期的当前实现章节优先于下方较早的 no-LTO／内联计划。本次翻译未独立测量历史耗时和性能主张。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/build-speed-runtime-rlib.md)

<a id="current-implementation-2026-10-31"></a>

## 当前实现（2026-10-31）

发布版快速路径现在以 **fat LTO 和一个代码生成单元**链接；运行时 rlib 本身则关闭 LTO，使用四个代码生成单元构建。`src/cli/cargo_build.rs` 记录了 10 月 9 日的实验：仅给包装函数添加 `#[inline]` 无法弥补观测到的非 LTO 性能差距。缓存键包括源码、工具链身份、优化级别、CPU 和代码生成标志，位于 `build_dir::cache_base()` 下，而非固定的公共临时路径。

交叉构建、matrix 使用、runtime-crate 依赖、原生依赖和 source root 都会绕过 run/build 快速路径。实际判定条件见 [cargo_build.rs](../../../../../../src/cli/cargo_build.rs)。下文的耗时与 no-LTO/inline 计划描述较早的实现，仅保留为测量和设计历史，不是当前建议。rlib 快速路径的原生依赖支持与预构建分发仍是独立提案。

<a id="earlier-implementation-and-inlining-proposal"></a>

## 早期实现与内联提案

> 目标：类似 Go 的构建速度。开发／测试／运行循环和交付构建都不应每次付出重新编译运行时的成本。

<a id="status"></a>

## 状态

**已交付（分支 `build-speed-parallel-rustc`）**：`almide_rt` 运行时仅编译一次为 `.rlib`（`codegen::emit_runtime_crate()`），按 runtime-source + rustc-version + opt-level 为键缓存在 `$TMPDIR/almide-rtlib-<hash>`。每文件／每构建的 rustc 生成精简 main（`#[macro_use] extern crate almide_rt;
use almide_rt::*;` 加用户代码，以 `RT_BOUNDARY_MARKER` 分割），并用 `--extern almide_rt=<rlib>` 链接 rlib。任何失败都退回 inline/cargo 路径；`ALMIDE_NO_RTLIB=1` 将其禁用。

已接入：
- **测试路径**（`cargo_build_test_with_native`）：rlib 为 opt1，精简 main 为 **opt0**（测试不需要优化用户代码）→ **每文件2.45倍**，spec/lang 116 个文件为**墙钟2.26倍／CPU 3.1倍**。
- **运行／构建路径**（`cargo_build_generated_with_native`）：rlib 和精简 main 使用二进制的优化级别（debug 1、release 3）。交付用 `--release` 构建**约20倍**（27～33秒→1～2秒），已验证输出相同。

排除：`http`/`zlib`/`sse`（非 std 依赖）仍走 cargo 路径。

<a id="the-remaining-gap-this-roadmap-item-shipping-build-inlining"></a>

## 剩余差距（本路线图事项）：交付构建的内联

由于运行时是**不使用 LTO**编译的独立 crate，非泛型且没有 `#[inline]` 的运行时函数**不会跨 crate 边界内联**。在大量使用字符串／列表的热点循环中（实测：`csv-to-json` 处理20万行 CSV），交付二进制比整体式构建**慢约10%**（典型程序为2～5%）。泛型运行时函数（多数 `list`/`map`/`option`/`result` 操作）和已有的 `#[inline(always)]` 辅助函数不受影响——它们已经跨 crate 内联。

拒绝的替代方案：**ThinLTO**。它能恢复内联，但在链接时重新优化运行时位码，抵消构建速度收益（约等于整体式构建耗时）。实测没有编译收益，因此不值得采用。

<a id="plan-inline-the-hot-non-generic-runtime-fns"></a>

### 计划：给热点非泛型运行时函数加 `#[inline]`

在 `runtime/rs/src/*.rs` 中，为少量热点非泛型运行时函数——主要是 `string.*`（`trim`、`split`、`lines`、`concat`、`len`、切片）及频繁调用的标量辅助函数——标记 `#[inline]`。rlib 暴露其 MIR 后，rustc 即使没有 LTO 也能跨 crate 内联，在维持约20倍构建收益及“独立运行时 crate、无 LTO”模型的同时恢复损失的性能。这类似 Go 的“小函数自动内联”立场。

步骤：
1. 对运行时密集负载进行性能分析（如20万行 CSV 的 `csv-to-json` 基准），找出主导 rlib 与整体式构建差距的运行时函数。
2. 为它们添加 `#[inline]`（优先定向使用 `#[inline]`，而非一律 `#[inline(always)]`，以避免调用方编译时间膨胀）。
3. 重新测量：确认交付时的运行性能回退降至约0%，而构建仍保持约20倍。
4. 防止过度内联：观察精简 main 的编译时间是否重新增长（候选越多，最终 rustc 的工作越多）。

<a id="open-follow-ups"></a>

### 尚待处理的后续事项
- 为带有**原生依赖**的项目（当前只能走 cargo）将 rlib 接入 `almide build`；需要通过路径依赖或 `RUSTFLAGS --extern` 将 rlib 传给 cargo。
- 考虑随编译器发布预构建 rlib，使新环境中的首次构建跳过一次性的 rlib 编译。
