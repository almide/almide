# 研究：修改存活率论文

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](research-modification-survival-rate-paper.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](research-modification-survival-rate-paper.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](research-modification-survival-rate-paper.zh-CN.md)



> 这是[固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/research-modification-survival-rate-paper.md)的非规范读者译文，是不自动同步的历史固定快照。状态、计划和数量均为原文时点的记录，不证明当前实现或完成情况。本次翻译未独立执行或验证这些主张。

> 原文说明：衡量不同语言中 LLM 代码修改存活率的学术论文。

目标：arXiv 预印本 — *《面向 LLM 代码修改的编程语言设计：衡量迭代编辑中的存活率》*（原题：*"Designing Programming Languages for LLM Code Modification: Measuring Survival Rate Across Iterative Edits"*）

### 历史数据收集计划

这是早期论文大纲，不是在仓库内运行的数据收集任务。按照仓库的[归属边界](../../../../../../CLAUDE.md)记录，所有 MSR 工作现在都属于 [Almide Dojo](https://github.com/almide/almide-dojo)。本地的 `research/benchmark/msr/` 和 `research/benchmark/framework/` 测试框架已于2026-08-08移除。以下观察仅作为历史主张保留；新论文需要 Dojo 管理的数据、确切的运行／模型版本及来源记录。本文不证明外部收集或发表已经发生。
- [x] 原始基准设施（benchmark.rb、SPEC-v1/v2、测试脚本、多语言；历史内容，不是当前本地运行器）
- [x] 基线数据：16种语言 × minigit 任务
- [ ] UFCS 引入后的 Almide 数据（v0.3.0及以后，10次以上试验）
- [ ] 前后比较：相同任务、相同 LLM，UFCS 引入前与引入后
- [ ] 每种语言10次以上试验，以获取统计显著性（理想为30次）

### 消融研究
- [ ] UFCS 开启与关闭
- [ ] LLM 错误消息开启与关闭
- [ ] 提供与不提供 CLAUDE.md 模板
- [ ] 测量哪个设计因素对存活率贡献最大

### 论文结构
- [ ] “修改存活率”的形式化定义（任务规格、修改步骤、通过／失败标准）
- [ ] 多语言比较（Almide、Python、Go、TS、Rust，相同 LLM、相同试验）
- [ ] 分析：为何语言设计影响修改成功率？
- [ ] 可复现性：所有基准、规格和测试脚本均开源
