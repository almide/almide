<a id="c-033--copy-on-write-for-aliased-mutables"></a>

# C-033 — 带别名可变值的写时复制

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](C-033-cow-truth-table.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](C-033-cow-truth-table.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](C-033-cow-truth-table.zh-CN.md)

> 非规范性、固定原文的读者快照；未修改原本仍为权威来源。不自动同步，未独立执行、重新测量或新验证契约保证。当前／历史描述均属于固定原文时点。C-006 区分旧 timeout 形式删除与后来协作式块形式。C-033 保留修复前诊断及关闭指示作为历史，不执行旧删除／门禁修改指示。repr helper／export 细节描述历史 emitter，不是当前公开 ABI。代码注释保持原样。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/C-033-cow-truth-table.md)

> [contracts.toml](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/contracts.toml) 中契约 C-033 的文字说明。
> 状态：**active**——已由 AliasCowPass + native pass_clone 修复（PR #394，2026-06-06）：下表现在在两个目标上逐字节相同，由 `spec/wasm_cross/alias_cow.almd` 锁定。以下历史移自 `crates/almide-codegen/rt-oracle-registry.toml` 中此前作为历史豁免保留的 `cow_check` 注记（随 v0 emitter 在 #782 退役；见 git 历史；现行门禁为 spec/wasm_cross + interp oracle）。
> 修复之前，这只是记录差异的文档。以下保留修复前诊断及关闭条件，不是当前实现状态。

<a id="the-promise"></a>

## 承诺

```almide
var a = [1, 2, 3, 4, 5]
var b = a        // b is a value copy, not an alias
a[0] = 999       // mutate a in place
// a == [999, 2, 3, 4, 5], b == [1, 2, 3, 4, 5]   (b UNCHANGED)
```

值语义：`var b = a` 进行复制，所以之后对 `a` 的原地修改不得通过 `b` 被观测到。机制是写时复制——首次原地修改共享值时，复制底层缓冲区。

<a id="historical-divergence-before-the-fix"></a>

## 历史差异（修复前）

| 步骤 | native（Rust） | wasm |
|------|---------------|------|
| `var a = [1,2,3,4,5]` | `a` -> 缓冲区 A | `a` -> 堆缓冲区 A |
| `var b = a` | 应为 `Rc::make_mut` / clone-on-write | 将 `a` 和 `b` 合并为一个 local；对别名做复制传播 |
| `a[0] = 999` | 分类缺口：最小复现触发 **E0382** move-after-use | **原地**修改缓冲区 A（无 COW） |
| 读取 `a`、`b` | （最小复现无法编译） | `a = 999,2,3,4,5` **且** `b = 999,2,3,4,5` |

观测探针：native `999,2,3,4,5 | 1,2,3,4,5`，而 wasm 为 `999,2,3,4,5 | 999,2,3,4,5`——wasm 中 `b` 被破坏。

<a id="why-the-historical-runtime-only-fix-was-insufficient"></a>

## 为何当时仅修复运行时还不够

本用于防止此问题的 wasm 例程 `__cow_check` 在 wasm emitter 中**没有调用方**——`IndexAssign` / `FieldAssign` / `ListSwap` / `ListReverse` 都原地修改。在 `IndexAssign` 接入 `__cow_check` **必要但不充分**：wasm 流水线还会**把带别名的可变 `var` 合并到同一个 local**并对别名复制传播，使堆 COW 无从分离。native 自身也存在相应的 VarStorage 分类缺口——同一最小复现在 Rust 目标触发 E0382 move-after-use。

正确修复横跨**两个目标的 VarStorage／别名合并模型**：每个原地修改点都要以 COW 写回不同 local，**同时**禁止可变变量别名的复制传播。这超出 runtime-routine drain 的范围，因此**如实保留历史豁免**，而不是强行作出虚假的“verified”切换。

<a id="historical-closure-criteria-fulfilled-by-the-fix-above"></a>

## 历史关闭条件（已由上文修复满足）

当 VarStorage／别名工作落地时：向 `spec/wasm_cross/` 添加 `var b = a; a[i] = v; read both` fixture，以它作为 `class = "fixture"` 证据，把 `C-033` 改为 `status = "active"`，删除本文，并降低 `scripts/check-rt-oracle-registry.sh` 中的 `MAX_GRANDFATHERED` 和 `scripts/check-contracts.sh` 中的 `MAX_FLAGGED`——**全部在同一 PR 完成**。flagged 数量是单向约束，只能下降。
