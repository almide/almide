<a id="do-330-gap-analysis--the-consolidated-stage-5-document"></a>

# DO-330 差距分析——第 5 阶段的整合文档

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](DO330-GAP.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](DO330-GAP.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](DO330-GAP.zh-CN.md)

> [正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/DO330-GAP.md)（固定提交 35297a612fb29e3f78f6587aba253aa91d51f466）的非规范性历史阅读快照。以正本为准；此副本不自动同步，也不是生成器或证明输入。未独立执行命令、证明、测量或资格鉴定评估。来源引用、生成标记与带日期数字作为历史资料保留，并非认证、资格鉴定、主管机构接受或发布的证据。

这是分阶段计划要求的单页文档：将既有关卡映射到工具资格鉴定目标，列出三个形式化产物及剩余差距。它是基于验证手段的来源索引——每项主张都在别处有门禁；以下每个 `source:` 行必须可解析（`scripts/check-tor-refs.sh` 也扫描本文件）。全篇以 Ferrocene（Rust 的 ISO 26262/DO-178C 资格鉴定）为先例模型。

<a id="1-the-qualification-argument-mapping"></a>

## 1. 资格鉴定论证（映射）

DO-178C Table-A 目标映射和 DO-330 TQL 论证位于 G-F5 设计中。拟议论证通过独立输出验证降低不受信任编译器的资格鉴定负担，并聚焦检查器。这是待评估的资格鉴定论证，不是已分配的 TQL，也不是形式化方法信用的授予；适用目标及可接受证据仍由申请方与主管机构确定。
source: docs/roadmap/active/flight-qualification.md

主张强度概览（已测量，有偏差门禁）：
source: proofs/STAGE-STATUS.md

<a id="2-the-three-formalized-artifacts-goal-directed-trio--all-instrument-backed"></a>

## 2. 三个形式化产物（目标导向的三项——均有验证手段支撑）

| 产物 | 位置 | 强制措施 |
|---|---|---|
| 工具运行需求 | proofs/TOR.md（当前 TOR 行） | check-tor-refs.sh：每项需求保留其强制执行手段 |
| 验证工具的验证 | proofs/gate-verification.toml | check-gate-verification.sh：按可能如何失败分类每个门禁；UNVERIFIED 仅可减少 |
| 已知问题形式化 | proofs/releases/（逐版本 seal） | release-seal.sh：必须有 `known_problems`，派生字段持续对照标签重新测量 |

source: proofs/TOR.md
source: proofs/gate-verification.toml
source: scripts/release-seal.sh

<a id="3-remaining-gaps-enumerated-owned"></a>

## 3. 剩余差距（已列举、有负责人）

| 差距 | 状态 | 负责人／参考 |
|---|---|---|
| MC/DC 与分支覆盖 | 已有棘轮约束的行／条件覆盖和 safety-set 决策向量；领域级覆盖信用仍待评估 | flight-evidence-gaps F2 剩余项 |
| C-WCET（counted-loop 基石 あ） | 在 receipt 设计中为 PENDING | flight-reference-app Slice 2 |
| C-FAITHFUL + Ferrocene 路径（基石 い） | 部分 structural-runtime 精化；链中没有通过资格鉴定的 native 二进制工具链 | flight-reference-app Slice 3，需要 Ferrocene 访问权限 |
| wasmtime 资格鉴定背景 | 未鉴定的 COTS，程序性 TOR-9 | #865 |
| ALS 语法元素编写 | 只减不增棘轮；当前数量见记录表的 `unwritten_ceiling` 与 proofs/STAGE-STATUS.md | proofs/als-element-coverage.toml（冻结前提） |
| 持续性证据 | ledger-counts.yml 每晚重新测量连续天数；当前数量见 proofs/STAGE-STATUS.md | research/benchmark/fuzz-green/（90 天里程碑） |
| 流程独立性 | 已有 ratchet-separation 门禁；人员／组织层面的审核分离属于组织工作 | flight-evidence-gaps F5 关闭说明 |
| PSAC／认证规划 | 尚无计划文档 | flight-qualification §G-F6（dossier 模板） |
| 外部评估本身 | 属于市场／主管机构事实，而非记录表事实 | 按定义超出仓库范围 |

source: docs/roadmap/active/flight-evidence-gaps.md
source: proofs/als-element-coverage.toml

<a id="4-what-this-page-is-not"></a>

## 4. 本页不是什么

不声称获得资格鉴定、被采用或已完整——这是供评估者着手工作的诚实清单。可重新生成的数字以指针而非复制形式提供，以免静默偏离。
