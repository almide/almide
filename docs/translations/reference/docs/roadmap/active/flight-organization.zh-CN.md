> 原文描述：Flight-grade 的 B 和 C 节：法人、对外接洽、资金、客户
<a id="flight-grade-the-organization-and-track-record-rungs"></a>

# Flight-grade：组织与业绩的阶梯

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](flight-organization.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](flight-organization.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](flight-organization.zh-CN.md)

> 非规范性的固定读者快照。以原文为准，不自动同步，未独立重新验证实现、测量或安全性。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-organization.md)
> 组织、认证、市场及监管机构陈述属于固定路线图的假设与拟议退出标准，不是当前法律／监管建议，也非已取得认证、已配备独立验证人员、资金或客户证据。状态标记描述 2026-08-13。保留 Critical profile 将来可用的条件，以及 agent 生成证据与人类／法人问责之间的明确区别。


[flight-grade 计划](https://github.com/almide/almide/issues/586)分为三节。A 节 — **工程** — 保留在 issue tracker，因为每项都是可由人落地的代码、证明或门禁。B 和 C 则不是：它们是法人、接洽计划、资金顺序、首位客户。原先以 #577–#585 登记，每个正文都说明了这一点（*“不是工程项；为保持计划完整性而在此跟踪”*）。

九个这样的 issue 留在工程队列中，会让队列误报可执行工作的数量。因此改放在这里。**总括 issue #586 保留检查清单** — 本文在实质内容上原样保留各 issue 的细节，因此关闭它们不会损失信息。

状态标记沿用合并时（2026-08-13）的 issue：❌ 未开始，◑ 部分处理。

---

<a id="b--organization"></a>

## B — 组织

<a id="b1-a-legal-entity-able-to-sign-dossiers-and-carry-liability--"></a>

### B1. 能签署 dossier 并承担责任的法人 ❌
*（原 #577）*

认证机构与主承包商和法人签约，而不是与仓库签约：审计答复、责任及 dossier 签名都需要法人。agent 辅助流程可以生成证据；只有法人能签署。Ferrocene 与 AdaCore 以公司形式存在，正是出于这个原因。

**内容**：决定公司形式与司法辖区；定义法人保证的内容（按 dossier 说明的工具链行为）以及保持开源的内容。

**退出标准**：存在能签署发布 dossier 并订立支持合同的法人。

<a id="b2-a-multi-decade-lts-policy-and-versioning-commitment--"></a>

### B2. 跨数十年的 LTS 政策与版本承诺 ❌
*（原 #578）*

飞机项目的寿命长于工具链：采用者需要书面承诺，说明固定工具链版本能获得多久的修复、如何通知 Known Problems，以及如何对升级进行资格鉴定。

**内容**：LTS 政策文件 — 各已鉴定发布版本的支持期限、回移规则、与稳定性契约一致的弃用规则、Known Problems 通知渠道。

**退出标准**：公开 LTS 政策，并在 dossier 中引用。

<a id="b3-certification-authority-relationships-der--notified-body-engagement--"></a>

### B3. 与认证机构的关系（DER／公告机构接洽）❌
*（原 #579）*

资格鉴定需要协商，而不是单纯提交：在大规模生成证据*之前*，尽早与指定工程代表／公告机构接洽，确定哪些证据被认可。

**内容**：确定首个目标制度（很可能是首位客户适用的制度：先考虑航天机构软件标准或 UAS 当局，而非民用航空电子设备），聘请有经验的顾问／DER，针对 dossier 做差距评估。

**退出标准**：由曾让工具通过资格鉴定的人出具书面差距评估。

<a id="b4-qualification-engineering-beyond-the-solo-plus-agents-model--"></a>

### B4. 超越单人加 agent 模式的资格鉴定工程 ❌
*（原 #580）*

资格鉴定工作有无法去除的人类部分：审计访谈、签名评审、承担责任的判断。当前模式（一个人加 agent 群）以异常快的速度生成证据，但单凭这一模式无法满足验证独立性的期待。

**内容**：定义最小团队 — 至少一位具有工具资格鉴定经验的工程师，以及独立验证角色 — 并记录独立性说明：谁验证什么，agent 的输出算作什么。

**退出标准**：为首次资格鉴定尝试建立并记录验证独立性结构，配齐人员。

<a id="b5-funding-the-climb-from-trust-market-revenue--"></a>

### B5. 用信任市场收入为攀登提供资金 ◑
*（原 #581 — trust-layer 路线图已有策略）*

资格鉴定是跨多年的成本中心；agent／wasm 沙箱信任市场是为其买单的收入与证据引擎。使用同一套证据机制，面对更早期的买家 — 不是绕开监管目标，而是通往它的资金路径。

**内容**：安排 trust-layer 变现顺序（receipts、capability containment、以 MCP 为切入口），让 dossier 级证据作为产品副产物积累；明确划出资格鉴定预算。

**退出标准**：有资金支持的计划，使资格鉴定里程碑（B1–B4 和 #574 数据包）具有收入来源。

**参考**：[trust-layer.md](../../../../../roadmap/active/trust-layer.md)

---

<a id="c--track-record"></a>

## C — 业绩记录

<a id="c1-the-first-regulated-adjacent-deployment-to-start-the-service-history-clock--"></a>

### C1. 首个邻近监管领域的部署，启动服役历史时钟 ❌
*（原 #582）*

产品服役历史本身就是认证中的筹码，且只能在真实部署中积累。小型卫星飞行软件和 UAS（特定类别）的进入门槛显著低于民用航空电子设备，并能产生恰当类型的运行记录。

**内容**：找到并赢得一位设计合作伙伴，在航天或 UAS 场景中飞行运行 Almide 生成的代码（Critical profile 可用后采用），从第一天起部署问题报告机制。

**退出标准**：Almide 代码在已部署的航天／UAS 系统中运行，并记录服役历史。

<a id="c2-the-deployment-ladder--"></a>

### C2. 部署阶梯 ❌
*（原 #583）*

没人从飞行控制开始。阶梯是：地面／验证工具（该市场也受监管，但更近）→ 航天／UAS 项目 → 低保证等级的机载项目（DAL-D/C 类）→ 更高等级。每一级都为下一级产生客户、证据与历史。

**内容**：将阶梯写成持续更新的文档 — 每级目标制度、所需及产出的证据，以及级间晋升标准。

**退出标准**：在 `docs/roadmap/` 中有阶梯文档，明确当前所在级别。

<a id="c3-service-history-and-problem-reporting-record-keeping--"></a>

### C3. 服役历史与问题报告的记录管理 ❌
*（原 #584 — 依赖 C1）*

服役历史只有以可供审计的方式记录才有价值：服役版本、暴露时间、发现的问题、处置，以及反馈至 Known Problems 的闭环。

**内容**：与发布 dossier 及 flagged-contracts 台账关联的轻量运行记录格式；从 C1 开始的每次部署都通过它报告。

**退出标准**：定义记录管理格式，并用于首次部署。

<a id="c4-a-published-position-on-certifying-machine-written-code--"></a>

### C4. 公开关于认证机器编写代码的立场 ❌
*（原 #585）*

保证框架假定作者是人类；监管机构（EASA AI 路线图、FAA 对应机构）正在积极研究 AI 辅助开发的含义。原生以机器作者为先、具有机械证据的工具链，是对方尚未拥有的答案 — 尽早参与讨论，才能让新来者被选择，而非仅被容忍。

**内容**：立场文件 — 当作者为模型时，按规范键关联的契约、逐构建证书及证据阶梯如何满足开发保证期待 — 并与相关工作组接洽。

**退出标准**：发布文件；至少记录一次与监管机构／行业工作组的互动。

---

<a id="critical-path-unchanged"></a>

## 关键路径（不变）

#563（提前期最长）→ #572 + #573（技术押注：经资格鉴定的生成器模型）→ **C1**（历史时钟）→ #574 + **B1**（可签署的数据包）。其余全部支持这些步骤。

**参考**：[certification-grade.md](../../../../../roadmap/active/certification-grade.md)、[flight-qualification.md](../../../../../roadmap/active/flight-qualification.md)、[trust-layer.md](../../../../../roadmap/active/trust-layer.md)
