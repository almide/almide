<a id="v1--develop-reflow-strategy"></a>

# v1 → develop 回流策略

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-develop-reflow.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](v1-develop-reflow.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](v1-develop-reflow.zh-CN.md)



> 原文：[docs/roadmap/active/v1-develop-reflow.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-develop-reflow.md)，固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`。这是历史路线图快照的非规范性读者译文，不自动同步。日期、状态、估算、测量和计划均属于原记录，不代表新增实现或独立验证。历史审批请求和建议命令不是现在的执行授权。以原文为准。

> 原文描述元数据：2026 年 7 月整合 develop-v1 的历史提案，不是尚待执行的请求。

<a id="historical-branch-integration-decision-record"></a>

## 历史分支整合决策记录

这是 2026 年 7 月 3 日的 `develop-v1` 整合提案。要求所有者作出 Go 决定及在分支内执行的内容是历史记录，不是针对当前检出的待审批请求。MIR、自托管源码及证明基础设施已经在本仓库中；当前交付的 Wasm 路径是 [structural](../../../../../../src/wasm_route.rs)。以下保留原有顺序及风险评估，并非重新发布指令。

> **定位**：关于何时、以什么单位将留在 develop-v1 中的 v1 成果（MIR trust spine、自托管源码、证据门禁）回流到 develop 的方针提案。**执行等待用户的 Go 决定**——本文整理决策材料。

<a id="還流資産の棚卸し2026-07-03-時点"></a>

## 回流资产盘点（截至 2026-07-03）

| 资产 | 对 develop 的价值 | 风险 | 提案 |
|---|---|---|---|
| **证据门禁**（工具链印记 / LC_ALL=C + solo-retry parity / ratchet 分离 lefthook / coverage.sh / 三点比较） | v0 验证也有相同缺口（区域设置排序和二进制漂移不依赖分支） | 近乎零——门禁不触及实现 | **第1波**：可原样 cherry-pick，对单独 v0 也立即有效 |
| **flight-evidence-gaps / TRUSTED_BASE 边界图 / roadmap 文档** | 审计台账是所有分支共用资产 | 零 | **第1波** |
| **同样适用于 v0 的编译器修复**（равi-recursive unify guard / structural-twin merge / cross-module @inline_rust——已在 v1 验证的 frontend/codegen 层） | 修复 v0 的真实缺陷 | 中——必须用 v0 全套测试重新验证 | **第2波**：1 修复 = 1 PR，进入 develop CI |
| **整个 MIR spine**（almide-mir crate + registry 自托管源码 + render_program） | v1 核心；引入 develop 后作为“第二个后端”并存 | 高——develop 的 CI 时间和维护面积翻倍 | **第3波**：与“v1 替换 v0 的父级”的决定同时；此前常驻原分支 |
| **stdlib 自托管（.almd）** | v0 路径不使用（registry 专用） | 低（不影响 v0），但单独无意义 | 与第3波同时 |

<a id="順序の原則"></a>

## 顺序原则

1. **证据优先**（第1波）：验证基础设施先于实现合流——提升 v0 绿灯的可信度，成为承接后续实现回流的网。
2. **1 修复 = 1 PR**（第2波）：将 v1 工作中发现的 v0 缺陷修复与 v1 分离，拆成可独立说明的粒度（原 governing directive 为“不需要 v0 回移”，因此这一方针变化需要明确批准）。
3. **替换当天才回流 spine**（第3波）：MIR spine 的回流与“退役 emit_wasm，将 render path 作为生产路径”的决定绑定。此前 develop-v1 是其归属分支。

<a id="未解決の判断ユーザー入力待ち"></a>

## 未决判断（等待用户输入）

- 第1波何时执行（随时可行——仅增加 CI）。
- 审核第2波目标清单（equi-recursive guard 会改变 v0 checker 行为，前提是用 spec 全量 + org 扫描重新验证 v0）。
- 第3波的触发条件（例如 parity baseline 超过可执行 spec 集合的 90%，org wall=0 达到 26/26，Matrix/closure brick 完成）。
