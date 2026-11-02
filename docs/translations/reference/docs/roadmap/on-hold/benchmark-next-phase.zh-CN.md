<a id="llm-benchmark-next-phase"></a>

# LLM 基准：下一阶段

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](benchmark-next-phase.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](benchmark-next-phase.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](benchmark-next-phase.zh-CN.md)

> 非规范性的固定读者快照。以原文为准，不自动同步，未独立重新验证实现、测量或安全性。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/benchmark-next-phase.md)
> 历史观察在此缺少运行／模型版本。旧 n≥10 目标、拟议检验、工时估计及预期优势不等于统计充分性、独立性、当前承诺或已证实优越性。恢复实验由 Dojo 管理，此处不恢复旧本地 harness。


> 原文描述：LLM 基准 Phase 2-3：跨语言比较、更难的问题、发布

此文记录早期基准／发布计划及其报告的 Phase 1 观察，不是当前可运行的基准指南。依 [CLAUDE.md](../../../../../../CLAUDE.md)，MSR 收集由 [Almide Dojo](https://github.com/almide/almide-dojo) 负责；本地 harness 已于 2026-08-08 删除。复用该研究计划需要 Dojo 管理的运行及溯源，而非重建相互竞争的本地 MSR 数值。

---

<a id="historical-reported-data-runmodel-versions-not-recorded-here"></a>

## 历史报告数据（此处未记录运行／模型版本）

| 语言 | 模型 | n | 平均 MSR | 范围 |
|----------|-------|---|----------|-------|
| Almide | Haiku | 14 | 92.1% | 80–100% |
| Almide | Sonnet | 9 | 92.2% | 70–100% |
| Python | Haiku | 3 | 88.9% | 77–100% |

**缺口：** Python 数据不足以统计比较（n=3），需要 n≥10。

---

<a id="phase-2a-python-n10-cross-language-comparison"></a>

## Phase 2A：Python n=10（跨语言比较）

**目标：** 判断 Almide 相对 Python 的 MSR 优势是否具有统计显著性。

**任务：**
1. 使用 Haiku 运行 Python MSR 基准 n=10
   - 原脚本位置：`research/benchmark/msr/python/modifications/run.sh`（已删除，不是当前命令）。任何新运行都在 Dojo 定义。
   - 约 30 分钟（10 次 × 9 个任务 × 每任务约 20 秒）
2. 使用 Sonnet 运行 Python MSR 基准 n=10
3. 逐任务及汇总应用 Fisher 精确检验
4. 报告：置信区间、p 值、效应量

**原始假设：** 约 8–12% 差距。这不是实测结果，也不保证检验功效。收集前应选择抽样单位和分析；重复任务／运行并不自动成为独立观察。

**风险：** 若 Python 也达 90%+，此难度下差距可能不显著。这也是有效发现，意味着需要更难的问题来区分。

---

<a id="phase-2b-harder-problems"></a>

## Phase 2B：更难的问题

**目标：** 创建修改任务，使 Almide 类型系统产生更大、可测量的优势。

**考验类型系统的问题类别：**

| 类别 | 预期 Almide 胜出的理由 | 示例 |
|----------|----------------------|---------|
| **多模块重构** | 编译器发现所有调用点；Python 依赖 grep | 重命名在 3 个模块使用的类型 |
| **错误传播链** | 显式 `Result` 处理／`!` 传播与手动 try/except 比较 | 添加经 4 个函数传播的新错误 case |
| **泛型类型更改** | `List[A]` → `Map[String, A]`；所有使用方必须更新 | 把函数返回类型从 List 改为 Map |
| **强制穷尽性** | 添加 variant 暴露受影响的不穷尽 match（通配符 match 可能仍然穷尽） | 给 4 个函数使用的 5-case enum 添加 3 个 variant |
| **添加必需记录字段** | 类型检查器发现缺失字段 | 给 10 处使用的记录添加必需字段 |
| **跨 effect 边界** | pure fn 意外调用 effect fn = 编译错误 | 重构，把 IO 移入专用 effect fn |

**难度层级：**
- Tier 1（原基线）：单文件、50-80 行、1-2 个类型变化 — MSR 约 92%
- Tier 2（目标）：单文件、100-200 行、3-5 个连锁变化
- Tier 3（挑战）：多文件、200-500 行、架构修改

**实施：**
1. 编写 10 个 Tier 2 任务（v1 解答＋修改指令＋v2 测试）
2. 加入 Dojo 管理的任务库；`research/benchmark/msr/modifications/prompts/` 是已删除的本地位置
3. Almide 与 Python 均运行 n=10
4. 比较各层级 MSR

---

<a id="phase-3-publication"></a>

## Phase 3：发布

**目标：** 发布可复现且限定于实测任务、模型和语言版本的比较，包含负面或不确定结果。

**交付物：**
1. **README 小节**——包含关键数字的 3 句概要
2. **博客／技术报告**——方法、原始数据、统计分析
3. **复现包**——随 Dojo 管理的实验发布脚本＋提示词＋原始输出

**统计方法：**
- 逐任务：Fisher 精确检验（2×2 表：通过／失败 × 语言）
- 汇总：配对比例检验（McNemar 或分层 Fisher）
- 报告：p 值、差异的 95% CI、效应量（Cohen's h）
- 透明性：提交全部原始 LLM 输出，完整可复现

**可信度清单：**
- [ ] 所有条件 n ≥ 10
- [ ] 多个模型（至少 Haiku＋Sonnet）
- [ ] 多个难度层级
- [ ] 与 Python 比较时任务结构相同
- [ ] 原始数据公开
- [ ] 报告 p 值的统计检验

---

<a id="execution-order"></a>

## 执行顺序

1. 先 **Phase 2A**——在 Dojo 重建实验，原本地基础设施已删除
2. **Phase 2B**——需要问题设计（1-2 天）
3. **Phase 3**——数据完整后

**原工时估计：** 3–5 天，不是当前承诺。
**恢复前：** 商定 Dojo 负责方，找到历史原始数据，定义新实验。此处不宣称当前外部运行状态。
