<a id="reconciliation-follow-up--v0280-で見送った-develop-側の残件"></a>

# Reconciliation 后续——v0.28.0 暂缓的 develop 剩余事项

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](reconciliation-followup.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](reconciliation-followup.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](reconciliation-followup.zh-CN.md)



> 原文：[docs/roadmap/active/reconciliation-followup.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/reconciliation-followup.md)，固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`。这是历史路线图快照的非规范性读者译文，不自动同步。日期、状态、估算、测量和计划均属于原记录，不代表新增实现或独立验证。历史审批请求和建议命令不是现在的执行授权。以原文为准。

> 原文描述元数据：v0.28.0 reconciliation 后续：留待 0.28.1 的 develop 提交。

> 记录于 2026-07-06。v0.28.0 的 [reconciliation](../../../../../roadmap/active/v1-release-path.md) 在将 develop（0.27.13）的**全部 capability 重新应用到 v1 线**后发布。这里保留当时**有意暂缓的 develop 提交**（并非 capability）及在 0.28.1 及以后纳入的方针。
> **相关**：[v1-release-path](../../../../../roadmap/active/v1-release-path.md) / [v1-v0-parity](../../../../../roadmap/active/v1-v0-parity.md)。

## 前提

- v0.28.0 **已重新应用 develop 的全部 substantive fix**（mut-record writeback、wasm aliasing、E026、native cdylib、http、删除 npm、dep 0.252 等）。“v0 能做的事”已作为 capability 全部涵盖。
- 下列剩余事项**不是 capability**（性能优化 / docs / CI 基础设施 / 外观性重构）。它们保留在 develop 历史（`git log origin/main`）中，随时可 cherry-pick。
- reconciliation 使用 `git merge -s ours` 记录历史并保留 v1 tree，因此这些只是“不在 tree 中”，并非“丢失”。

<a id="0281-で取り込む候補cherry-pick-可"></a>

## 0.28.1 纳入候选（可 cherry-pick）

<a id="c2f87402--wasm-simd-perfq1_0transposefold"></a>

### c2f87402——WASM SIMD 性能（q1_0/transpose/fold）

- **性质**：性能优化。已证明 **correctness core（variant-eq tail-zero）在 v1 中冗余**（develop-v1 的 iter_scope free-list reset + iter-scope escape 已覆盖，leak-test 经 WASM 通过 + PCC ownership ACCEPT）。因此剩下的只有 **SIMD 矩阵加速**。
- **决策材料**：develop-v1 已有独立 SIMD 实现（matrix fast-exp、Q1_0 dequant）。**需调查与 c2f87402 的 q1_0/transpose/fold SIMD 重叠多少**。若重复则不需要；若有差异，则为 nn/qwen 工作负载的 v0-wasm 性能移植。
- **工作量／风险**：12 个文件，emit_wasm 的 SIMD emit。不影响 correctness 的性能工作，优先级为中。先测量 develop-v1 已有 SIMD 覆盖，再判断。

<a id="c5e13f61--certification-grade-roadmap-の-terminal-goal-記録"></a>

### c5e13f61——记录 certification-grade roadmap 的 terminal goal

- **性质**：仅 docs，向 `docs/roadmap/active/certification-grade.md` 追加。
- **工作量**：极小。cherry-pick 可能冲突，但可手动解决。

<a id="13691202--nightly-fuzz-harness-の子プロセス-group-reap--出力-bound"></a>

### 13691202——nightly fuzz harness 子进程组回收 + 输出上限

- **性质**：CI/test 基础设施加固（回收孤儿子进程、限制输出）。
- **工作量**：小，仅 fuzz harness 脚本。为提高 CI 稳定性，值得纳入。

<a id="永久スキップ"></a>

## 永久跳过

<a id="c3648caa--wasm-emitter-の-magic-number-撤廃named-constants"></a>

### c3648caa——消除 WASM emitter 的魔数（命名常量）

- **性质**：外观性重构（49 个文件），基于 develop 约 26 个文件的结构改为命名常量。
- **原因**：develop-v1 有**将 emit_wasm 拆分为 109 个文件的独立结构**。develop 的命名常量改造与 v1 结构不匹配，是 reconciliation 中 emit_wasm 冲突的主因。如果 v1 需要同等可读性改善，应适配 v1 结构**另行**进行（不移植 develop 的改动）。

<a id="受入方針"></a>

## 接受方针

- 即使纳入也要**维持 honest-wall + 全部门禁为绿**（parity 210 MISMATCH=0、PCC ACCEPT、mir/spec/cargo/drift/contracts/docs-gen）。性能变更必须通过 byte-identical 门禁。
- 版本升级时若不**更新 llms.txt 的 "Current stable" 叙述**，`docs_gen_check_passes_on_clean_checkout` 会失败（docs-gen 的版本描述检查）。
