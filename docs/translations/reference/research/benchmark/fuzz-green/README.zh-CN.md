<a id="fuzz-true-green-streak-aviation-quality-stage-4"></a>

# Fuzz 真正正常运行的连续天数（aviation-quality Stage 4）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](README.zh-CN.md)

> 非规范、固定原文的读者快照。原路径仍为权威来源，不修改或替换机器输入。本副本不自动同步。历史测量及 aviation-quality 等名称不代表独立验证或认证；目标不等于已实现结果。
> [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/fuzz-green/README.md)


关键任务审计者查看的指标不是“修得多快”，而是“持续多久没有出错”。只有当天已完成的运行满足评分器的正常规则，日历日才算 CLEAN。分片运行使用 `Night verdict` 作业，要求完成至少 75% 的计划 fuzz 分钟且没有发现问题（包括回收的分片发现），不能只看 workflow 结论。失败或未完整运行的一天会中断正常连续记录；没有运行的日期既不增加也不重置天数。旧版及缺失 verdict 的处理见 `scripts/fuzz-green-streak.sh` 和共享评分器 `scripts/lib/fuzz-night-line.sh`。
首个里程碑：**连续 90 个正常日**。

计量器：`scripts/fuzz-green-streak.sh`。每晚的 ledger-counts 运行带 `--update` 调用它；每当连续天数与末行不同时便追加带日期的行，所发起的刷新会将该行与重新标记的计数一并提交。

| 测量日（UTC） | 连续天数 | 开始日期 | 最近运行日 |
|---|---|---|---|
| 2026-08-10 | 0 | - | 2026-08-10 |
