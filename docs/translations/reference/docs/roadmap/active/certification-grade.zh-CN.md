原文说明元数据：认证级强化 — 为机器编写软件的信任层采纳 DO-178C / ISO 26262 / IEC 61508 的机制（规范、可追溯性、覆盖、工具资格、材料包）
<a id="certification-grade-hardening--認証級への硬化"></a>

# 认证级强化

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](certification-grade.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](certification-grade.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](certification-grade.zh-CN.md)

> 固定原文提交的非规范阅读快照；以原文为准，不自动同步。未独立执行、验证实现、检查证明或测量。历史命令与设计指令是资料，不是已执行的指令。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certification-grade.md)

> 原文注记：“认证级”是工程目标，不表示已获证书、工具资格、监管批准或安全保证。标准映射、竞品资格及成本收益是原文主张，不是独立审核结论。早期 native oracle/规范缺口及起草计划与后来的 ALS/spec-keying 落地记录并存，初次121-fixture 数字属于历史。N-version 多数表决及提升 interp 本身不证明独立性或全部语义。Critical 适用于所述入口程序范围，stdlib 受信任；fan admissibility 和 RC 级联上限仍未完成。translation validation 须满足制品对应及 checker 目标性质义务，不自动替代 DO-330 流程要求。

> **目标**：吸收构成航天、航空、汽车标准（DO-178C / ISO 26262 / ECSS / IEC 61508）的5种机制中所有可通过工程获得的部分，将 [trust-layer](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/trust-layer.md) 的 L2/L3 提升到“认证级”的定义。证书本身（TÜV 审核 / DO-330 TQL）单列为商业选项 — 此处构建的是其前提证据体系。
> **实地审核发现**：[flight-evidence-gaps](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-evidence-gaps.md) — 2026-07-03 动手改修时观察到的7处证据体系缺口（F1 oracle 循环是 CG-1 的实测支持，F2 覆盖幻觉是 evidence ladder 的实测支持）。
> **活动范围：CG-1** — 将 ALS（Almide Language Specification）+ reference interpreter 规范化，并为契约关联 spec 键。
> **退出条件（CG-1）**：全部 active 契约通过 `spec` 字段引用 ALS 章节，`check-contracts.sh` 强制 spec ↔ contract ↔ fixture 三层可追溯性。`almide-interp` 从“3rd judge”提升为规范语义，native / wasm 成为“必须与规范一致的实现”。

<a id="規格の分解--認証基準は-5-つのメカニズムでできている"></a>

## 分解标准 — 认证标准由5种机制组成

DO-178C（航空，Level A-E）、ISO 26262（汽车，ASIL A-D）、ECSS-E-ST-40C / NPR 7150.2（航天）、IEC 61508（母标准，SIL 1-4）剥离外层后可分解为同样的部件：

| # | 机制 | 标准中的名称 | Almide 现状 |
|---|---|---|---|
| ① | 独立于实现的规范性规格 | requirements / language specification | ❌ native = oracle 的循环（最深缺口） |
| ② | 双向可追溯性 | bidirectional traceability | ✅ C-NNN ↔ `@contract:` 双向门禁 |
| ③ | 验证分级及下限 | MC/DC coverage, DO-333 formal credit | ◑ evidence ladder rank 0-5 + floor rule。缺失：编译器自身覆盖 |
| ④ | 工具资格 | DO-330 TQL-1, ISO 26262 TCL3 | ◑ `Verified<T>` belt + byte-identity = 逐构建自验证。缺失：资格材料包 |
| ⑤ | 流程与责任 | audits, LTS, Known Problems, liability | ◑ flagged ratchet 相当于 Known Problems ledger。缺失：组织侧（范围外） |

<a id="現有資産の規格対応表"></a>

## 现有资产的标准对应表

用标准词汇重述已有内容。CG 各 Phase 填补此表的“缺失”。

| Almide 资产 | 标准中的对应物 |
|---|---|
| 契约台账双向门禁（[contracts.toml](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/contracts.toml) ↔ `spec/wasm_cross/*.almd`） | DO-178C bidirectional traceability（reqs ↔ tests） |
| evidence ladder doc-only→lean（[contract-classes.txt](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/scripts/lib/contract-classes.txt) rank 0-5） | SPARK assurance ladder（Stone→Platinum）/ DO-333 formal credit |
| `flagged-for-revision` 棘轮（只能减少） | 取得资格的编译器所必需的 Known Problems ledger |
| [Perceus-belt](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/almide-perceus-belt.md) `Verified<T>` 类型状态 | tool self-verification（未经验证的 IR 无法 emit） |
| [capability system](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/effect-system-capability.md) | ISO 26262 freedom-from-interference / ARINC 653 partitioning |
| byte-identity gate + host-arch deterministic codegen | configuration management（超过标准要求） |
| nightly fuzz（在 evidence 记录 `n =`） | 量化验证证据 |
| reference interpreter（`crates/almide-interp`，3rd judge） | **规范语义候选**（CG-1 中提升） |

<a id="最深のギャップ-実装が仕様は認証では通らない"></a>

## 最深缺口：“实现就是规范”无法通过认证

当前台账基于 native = oracle 成立。从工程上看，这是正确的自举，但在认证领域构成循环 — “需求独立于代码存在，代码针对需求验证”是绝对条件。

先行者已走过此路：**Ferrocene 为将 Rust 按 ISO 26262 ASIL D / IEC 61508 进行资格评定，先编写了 FLS（Ferrocene Language Specification）**（因为 Rust 没有规范性规格；后来被采纳为 Rust Project 官方规范的基础）。

Almide 的起点比 Ferrocene 更好：已有可执行的 reference interpreter。CG-1 中反转 oracle 后，**spec（interp）/ native / wasm 三方 = 真正的 N-version 验证**（2-of-3 多数表决），不仅消除循环，还增强验证能力。

<a id="phases"></a>

## 阶段

<a id="cg-1-als--規範意味論--当初の計画と着地記録"></a>

### CG-1：ALS + 规范语义 — 最初计划及落地记录

ALS 现在位于 [docs/specs/als/](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/README.md)，契约 spec-keying 也已纳入当前台账/门禁。下述初次121-fixture 审核及起草步骤是历史计划、测量，并不表示 ALS 至今尚未起草。

1. **将 `almide-interp` 提升为规范语义** — 从“3rd judge”到“THE spec”。首先进行缺口审核：盘点 interp 无法求值的语言特性 / stdlib 函数（无法求值区域作为规范缺口入账，通过棘轮缩小）。
   - ✅ **台账 + 双向门禁已落地**（issue #564）：`crates/almide-interp/interp-abstain-ledger.txt` + `wasm_runtime_interp_ledger.rs::interp_abstain_ledger`（无需后端 = 在 CI 不会自行跳过）。新增 abstain 强制更新台账；已解决条目滞留也 fail — 台账只可缩小。首次审核：**121 fixtures 中可求值49（40%）/ 不可求值72**。构成：设计排除约18（transcendental 6 / in-place 5 / fan 非确定性7）+ **未实现 glue 约54（map.new, set.from_list, json.parse, string.take/slice, list.get_or, bytes.* …）** — 后者是可从“skip”转成“第三票”的提升空间。
2. **起草 ALS** — 以 interp 为 executable semantics 参照的散文规范。结构：语法 / 静态语义（类型、capability）/ 动态语义（interp 参照）/ 可观察等价定义（stdout, stderr, exit code）。**53个契约 statement 是章节种子** — 契约已有规范性文风。
3. **契约 spec-keying** — 给 `[[contract]]` 添加 `spec = "ALS §x.y"` 字段（active 契约 REQUIRED），将 `check-contracts.sh` 扩展为三层对称门禁。

<a id="cg-2-コンパイラ自体の-structural-coverage-m"></a>

### CG-2：编译器自身 structural coverage（M）

- 在 spec / fixture 执行下测量编译器 branch coverage（cargo llvm-cov），纳入 CI 棘轮（不可下降）。
- 列举当前安全检查，确定覆盖范围。原 `PerceusVerifyPass` / `StackBalancePass` 是旧 emitter 名称，因此从 `proofs/receipt.sh` / codegen pass 构建当前清单。
- 标准对应：DO-178C Level A structural coverage。发现的不可达代码要么是死分支（删除），要么缺少 fixture（补充）。

<a id="cg-3-almidecritical-profile-m"></a>

### CG-3：Almide/Critical profile（M）

类似 MISRA C / Ada Ravenscar / SPARK subset。`almide check --profile critical`：

- **必须全域** — 将 C-001/C-002/C-047（div/mod/pow 不 trap 而 abort）的主题推广到整个语言。
- 有界递归、分配上限（可静态检查的形式）、RC drop 级联长度上限 → WCET 分析可行性。
- capability 从 deny-all 出发（`Rand` / `Time` 也须显式 grant）。
- admissibility 遵循当前 ALS §B 和 critical-profile 检查。不要把0.29.0 移除 `fan.timeout` 作为当前禁止理由（后来以 oracle body 形式重新引入）。
- **Critical 是 subset，不是方言** — 全部 Critical 代码在普通模式下仍原样有效（与 SPARK ⊂ Ada 相同）。

**状态（#567 落地）**：`almide check --profile critical` 已运行 — 对全部函数应用 bounded profile（ALS §B，E070–E078）；capability 从 deny-all 出发，由 `--allow IO|Net|Env|Time|Rand|Process` 授权。范围是入口程序（stdlib 是 profile 下的 trusted runtime）。`tests/critical_profile_test.rs` 对所有负例交叉检查普通模式 PASS，在 CI 强制 subset 性质。静态内存纪律（#568）也已落地：参考应用 critical-clean + kernel emit 无分配 + 在 `--heap-cap` 固定预算内完成（超预算确定性 OOM abort）；分区 host 要求见 docs/project/PARTITIONED-RUNTIME.md。剩余：`fan.*` admissibility（等待 #1628 stage 2 成本边界）、RC drop 级联上限。

<a id="cg-4-translation-validation-l"></a>

### CG-4：Translation validation（L）

将 belt 思想扩展到逐构建证明：对每个制品附上“此 .wasm 是此已验证 IR 的正确翻译”的证据。

- 考虑将逐制品验证作为资格评估证据。不声称它是 DO-330 的超集，也不声称自动替代流程要求。
- 原 `Verified<T>` 属于旧 emitter 设计。列举当前对象 / trusted seam，从 emit invariant 机器检查推进到必要语义保持义务 →（远期）在 Lean 中证明 emit 规则。
- CompCert 级完整证明见 [completeness roadmap](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/correctness-guarantee-gaps.md) 的历史讨论。若要 translation validation 提供等同保证，须另行确立 checker 目标性质与实际制品之间的对应。

<a id="cg-5-qualification-bundle-s-m"></a>

### CG-5：Qualification bundle（S-M）

将 `make verify`（[trust-layer](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/trust-layer.md) receipts harness）输出变成带签名、版本的 dossier：

- 契约台账 + spec 可追溯性 + 覆盖 + Lean 0-sorry + fuzz `n =` + byte-identity + Known Problems（flagged 清单）。
- 每个 release 自动生成并附上。它是**未来商业资格评定（Ferrocene playbook）的输入**；先具备 dossier 会大幅降低资格评定成本。

<a id="何をしないか"></a>

## 不做什么

- **取得证书本身**（TÜV / DO-330 审核）— 审核应对、LTS 保证、赔偿责任是组织问题，工程买不到这些。Ferrocene、AdaCore 正是销售这些服务的公司。确立类别后单列为商业选项。
- **进入飞行软件市场** — 买方是 agent 平台。面向受监管行业是在 dossier 齐备后的选项。
- **CompCert 级完整编译器证明** — 已依据数据否决，以 CG-4 替代。

<a id="先行者の地図--何を借りるか"></a>

## 先行者地图 — 借鉴什么

| 先行者 | 成果 | 借鉴内容 |
|---|---|---|
| SCADE KCG | DO-178C Level A 已取得资格的代码生成器 | 参考资格材料结构。不依据本 repo 断定无需审查输出 |
| Ferrocene | 为 rustc 取得 ISO 26262 ASIL D / IEC 61508 资格 | **先写 spec** 的 playbook（FLS）；资格材料的公开风格 |
| SPARK | 在 DO-333 下让证明获认可为测试替代 | assurance ladder 设计（evidence ladder 已同构） |
| CompCert / seL4 | 已验证编译器 / 直到 binary 的证明 | translation validation 思路（CG-4） |
| MISRA / Ravenscar | 可认证子集 | profile 是 subset 而非方言的原则（CG-3） |

<a id="リスク"></a>

## 风险

| 风险 | 应对 |
|---|---|
| ALS 变成对实现的反向抄写，循环换个形式留下 | **先**提升 interp，再针对 interp 编写 ALS。与 native 不一致视为实现侧 bug，而非 ALS 侧 bug |
| 覆盖工作量泥潭 | 从仅安全 pass 的 MC/DC 开始，整体仅用 branch ratchet |
| Critical profile 将语言一分为二 | 用门禁强制 subset 原则（CG-3）— Critical 中 valid 则普通模式也 valid |
| 认证扮演（没人用的 dossier） | CG-5 是 trust-layer `make verify` 同一内容的格式化，不另建独立制品 |

<a id="組織実績のはしごbc-セクション"></a>

## 组织与实绩阶梯（B/C 节）

flight-grade 计划的**非工程**部分 — 能签字的法人、与 DER 协商、LTS 政策、首次生产部署、服务历史记录、关于机器编写代码认证的立场声明 — 汇总在 [flight-organization.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-organization.md)。之前曾以 issue #577–#585 排队，但它们各自在正文称自己“不是工程项”；混入可着手工作队列会让队列失真。umbrella issue #586 的 checklist 继续记录进度。
