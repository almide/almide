<a id="mission-critical-stage-status--measured-regenerated"></a>

# 任务关键阶段状态——测量与重新生成

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](STAGE-STATUS.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](STAGE-STATUS.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](STAGE-STATUS.zh-CN.md)

> [正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/STAGE-STATUS.md)（固定提交 35297a612fb29e3f78f6587aba253aa91d51f466）的非规范性历史阅读快照。以正本为准；此副本不自动同步，也不是生成器或证明输入。未独立执行命令、证明、测量或资格鉴定评估。来源引用、生成标记与带日期数字作为历史资料保留，并非认证、资格鉴定、主管机构接受或发布的证据。

五阶段计划（Stage 1 消灭 accept-and-wrong → Stage 2 全面 translation validation → Stage 3 冻结语义 → Stage 4 持续性测量 → Stage 5 DO-330 差距分析）是持续采用路线图。本页是其唯一可检查的状态产物：标记之间每个数字都从已提交记录表测量到 `proofs/ledger-counts.toml`，由 `scripts/gen-ledger-counts.sh` 标记日期（作为发布步骤，或回应每晚 `scripts/check-ledger-counts.sh` 报告的偏差），再由 `scripts/gen-claims.sh` 渲染。手动编辑会使 CI 失败（`check-contracts.sh` 执行 `--check`）。审计者应阅读本页，而非散落在各会话中的文字主张。阶段语义与证据入口：`docs/roadmap/active/flight-evidence-gaps.md`（审计发现记录表，2026-08-12 重新测量）及 `proofs/TOR.md`（运行契约）。

<!-- stages:generated:start — derived from the proofs/ ledgers by scripts/gen-claims.sh; DO NOT EDIT between the markers -->
<!-- counts:generated:start (as of 2026-10-21) — stamped totals from proofs/ledger-counts.toml; refreshed only by scripts/gen-ledger-counts.sh, never by a fixture/contract PR; DO NOT EDIT between the markers -->
> **Stage 1（消灭 accept-and-wrong）：审计已完成，并有门禁** —
> scalar-read 64 个分支／0 UNGUARDED；
> platform-libm 4 处已分类。未分类的新条目不能合入。
>
> **Stage 2（translation validation）：940/957 个 fixture 进行真正的三方投票（98%）** —
> 剩余弃权项已分类，且只能减少（interp-heap 工作线，#1226）。
>
> **Stage 3（语义冻结）：388/388 项契约具有 spec 键；语法元素覆盖
> 73/73 已关联章节（0 UNWRITTEN，只减不增——冻结前提为 0）。**
>
> **Stage 4（持续性）：fuzz true-green 连续记录 = 0 天**（带日期的计量；
> 仅基于正确性的每晚判定于 2026-08-12 引入——里程碑为 90 天）。
>
> **Stage 5（可审计性）：19 个 release seal；126 个验证门禁已分类
> （只减不增上限下 0 UNVERIFIED）；TOR 有 10 行具备强制措施；
> 差距分析整合于 proofs/DO330-GAP.md（受引用门禁检查）。**
<!-- counts:generated:end -->
<!-- stages:generated:end -->

这些数字不声称：外部采用（市场事实，而非记录表事实）、领域级 MC/DC 资格鉴定信用，或完整产物精化。行／条件覆盖和 safety-set 决策向量是独立证据手段。C-WCET / C-FAITHFUL 的范围及剩余假设应查阅 receipt。上述带日期语法元素映射满足的是该映射前提，而非语义完整性，也不表示主管机构接受已冻结的规范。
