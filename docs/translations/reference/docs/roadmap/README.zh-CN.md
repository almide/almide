<a id="almide-roadmap"></a>

# Almide 路线图

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](README.zh-CN.md)

> 固定的已生成索引之手工非规范阅读快照，不是自动生成或同步索引。原文仍为正本；未运行生成命令，也未独立重验状态、证明、性能或认证。99/32/15 项及日期反映原文快照。active/on-hold/done 路由及简短主张不证明当前完成、普遍正确、世界领先或外部资格化。范围请参阅各原文和 reader 注记；链接保留固定原文。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/README.md)

> 原文生成历史：原索引由目录结构自动生成，原更新命令为 `bash docs/roadmap/generate-readme.sh > docs/roadmap/README.md`。本 reader 是手工翻译快照，不是该生成器输出。
>
> [GRAND_PLAN.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/GRAND_PLAN.md) — 五阶段战略
> [ROAD_TO_1_0.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/ROAD_TO_1_0.md) — 0.41 → 0.99 版本阶梯

<a id="active"></a>

## 进行中

99 项

| 项目 | 说明 |
|------|-------------|
| [ADR-0013：native String 作为共享buffer — 实现](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/adr-0013-string-sharing.md) |  |
| [AlmidePerceusBelt](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/almide-perceus-belt.md) | 历史 Perceus belt 设计、已退役的生成阶段类型状态，以及有范围限制的证明证据 |
| [全轴制胜的突破点——来自竞技场与邻近比较的设计指引](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/arena-breakthroughs.md) | 全轴制胜突破点台账：基于邻近竞技场实测及 18 个邻近编译器的机制阅读，明确落后轴、成为世界第一所需突破、获胜条件、门禁和顺序 |
| [异步起点——以逻辑时间为语言的时间基准](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/async-inception.md) | 异步起点：论点、文法、语义、证明、主张及计划统一于一份宪章 |
| [2026 年 Async 主张 — 审计与拉开差距的计划](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/async-world-claim.md) | 对“2026 年最佳 async”主张的审计：方向、对手与拉开差距的五招。 |
| [Behavioral Contract——区分修改保持与批准性质的设计提案](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/behavioral-contract.md) | 行为证据提案：将前版保持与人类批准性质，和限定范围的编译器/运行时主张区分开。 |
| [构建速度：运行时 rlib 与热点函数内联](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/build-speed-runtime-rlib.md) | Runtime rlib 构建路径：当前 fat-LTO 发布链接及历史内联实验 |
| [Camp-4 的一般化：使 heap-result `match` 从识别形状改为全域](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/camp4-general-heap-match.md) | 仅在 lowering 中使 heap-result match 从识别形状改为全域 |
| [证书格式 v1 — 设计](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certificate-format-v1.md) | 逐函数所有权证书格式 — i/a/d/m 字母表、调用/分支/闭包扩展，以及已交付的构件。 |
| [认证级强化](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certification-grade.md) | 认证级强化 — 为机器编写软件的信任层采纳 DO-178C / ISO 26262 / IEC 61508 的机制（规范、可追溯性、覆盖、工具资格、材料包） |
| [CI 关键路径：实测基线、同类调查及排序计划（2026-09-21）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/ci-critical-path.md) |  |
| [CI 警告清理](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/ci-warnings-cleanup.md) | CI 容忍的编译器警告常设台账，以及逐类清零计划 |
| [Closure 跨 target 完整性](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/closure-cross-target-completeness.md) | 历史 closure 扫描、后续架构背景及证据限制 |
| [Codegen 遍历完整性](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/codegen-traversal-totality.md) | 带有已实现 fold/lint 基础及其有限覆盖范围的 traversal-totality 设计。 |
| [通过构造实现完备性](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/completeness-by-construction.md) | 设计笔记，将 Perceus / borrow checker 风格的机械完备性保证借用于 Almide 自身的pass。 |
| [并发立场：确定性数据并行](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/concurrency-stance.md) | 并发模型决策及其确定的 fan 家族修复 |
| [持续改进台账](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/continuous-improvement.md) |  |
| [正确性保证的缺口](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/correctness-guarantee-gaps.md) | 从类型正确源码到正确二进制的链中，仍缺少机械或数学证明的层 |
| [跨目标完整性（封盖）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/cross-target-completeness.md) | 历史跨目标清理策略、带日期目录及差分证据限制 |
| [确定性／纯度 Belt](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/determinism-belt.md) | 历史 determinism belt 设计、退役 type-state 边界及当前证据限制 |
| [确定性边界——在 byte-identity 下使计算上限合法](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/deterministic-bounds.md) | 在 byte-identity 下使 fan.timeout 合法——fuel、确定性配额、胜者顺序、effect 隔离 |
| [edit-locality 理论——Almide 为之存在、要证明的定理](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/edit-locality-theory.md) | 把有界编辑影响范围从设计直觉变为已证明定理 |
| [effect 检验框架](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/effect-inspection-harness.md) |  |
| [基于 capability 的 effect system](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/effect-system-capability.md) | 用于 sandbox AI agent container 的 capability-based effect system |
| [蓝图 — 使约27个 effectful / raw-pointer stdlib 函数在 v1 中达到 FUNCTIONAL](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/effectful-27-blueprint.md) | effect/raw-pointer 蓝图；字节测试的 tier 不是普遍语义证明。 |
| [Execution Inception — 将 as-if 规则完善至并发与 target 之外](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/execution-inception.md) | 8 月执行基体调查及原计划，附当前 ADR 与 component host 指引。 |
| [Fan v2——参考示例](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/fan-v2-examples.md) | fan v2 表面的参考示例：每个 head、边界语义、诊断 |
| [Fan v2 — 执行策略设计与迁移记录](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/fan-v2.md) | Fan v2 设计和迁移历史；当前选择表面遵循 ADR-0028 与 SPEC |
| [飞行证据缺口——实地审计发现台账](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-evidence-gaps.md) | 实地内部审计发现（2026-07-03）：将 DAL-A 理念与 DAL-A 证据的实测距离，列为附纠正工作项及验收条件的 7 项发现 |
| [Flight-grade：组织与业绩的阶梯](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-organization.md) | Flight-grade 的 B 和 C 节：法人、对外接洽、资金、客户 |
| [飞行配置 — 航空品质的接地分析与两个关键支点](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) | DO-178C 飞行级差距分析：6 个支柱的状态 + 2 个工程关键支点。 |
| [飞行资格化 — DO-178C/DO-330/DO-333 映射 + 资格化套件（G-F5 + G-F6）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-qualification.md) | 拟议的资格化证据映射和套件设计；本文不确立已获受理的监管 credit 或已资格化工具 |
| [飞行参考应用 — PID 控制律 kernel + make verify + receipt（G-F4）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-reference-app.md) | 飞行 gate G-F4 — 端到端通过 `make verify` 的参考应用（在 counted sim 循环上运行的定点 Q16.16 PID 控制律 kernel）、7阶段验证管线（已存在 vs 依赖基石）、产出 receipt（C-SAFE/C-PROVEN 绿，C-WCET/C-FAITHFUL 待做），以及降低风险顺序（Slice 0 scalar-no-print 当前绿 → Slice 1 print=G-F0 frontier → Slice 2 基石あ解锁 C-WCET → Slice 3 基石い解锁 Ferrocene）。 |
| [飞行基石（い）— v1 MIR → Rust → Ferrocene（忠实性绑定）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-rust-ferrocene.md) | native renderer 与拟议 Ferrocene 资格化路径；抽象 RC 引理不证明 Almide-to-Rust 翻译 |
| [Almide 飞行配置 — 原始子集提案](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-subset-spec.md) | 原始飞行子集提案；规范性有界配置的权威是 ALS §B，而非历史上的 @flight 草案。 |
| [飞行基石（あ）— 拟议 counted loop 与分配上限](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-wcet-loops.md) | 拟议 counted-loop witness 与分配次数上限；target 特定的物理 WCET 仍是独立义务 |
| [Fuzz 发现分诊：让夜间差分门禁重新变绿](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/fuzz-findings-triage.md) | 历史夜间差分 fuzz 分诊，保留发现、修复及撤回假设 |
| [整数字面量的域 — 跨语言调查与 Almide 的目标](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/integer-literal-domain.md) | 其他语言如何检查整数字面量范围，以及 Almide 应达到的目标 |
| [StringInterp 不是特殊语法 — 将其脱糖为 `concat + to_string(part)`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/interp-is-desugar-to-tostring.md) | 历史插值脱糖决策及其限定范围的正确性依据 |
| [未关闭 issue 清单，2026-09-21 — 78 个 issue 采用统一 header](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/issue-inventory-2026-09-21.md) |  |
| [Issue 台账燃尽——完整性行动的剩余工作量表](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/issue-ledger-burndown.md) | issue 台账燃尽指标：让未关闭 issue 数诚实反映剩余工作 |
| [#1437：提升后 carrier 的 ABI——声明驱动、单一可读布局](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/lifted-carrier-abi.md) | #1437：用一个声明驱动的 carrier 布局取代生成方漂移 |
| [LLM-first 语言](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/llm-first-language.md) | 通过 dojo MSR 衡量，打造 LLM 最能准确编写的语言的计划 |
| [逻辑时间 Async — async 文法设计](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/logical-time-async.md) | async 文法，以 fuel 为逻辑时钟、确定性 race、oracle 层。 |
| [逻辑时间 Async — 实现蓝图](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/logical-time-implementation.md) | 实现蓝图：Op::Charge、fuel ABI、计量克隆、race lowering、门禁。 |
| [逻辑时间异步 — 证明台账](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/logical-time-proofs.md) | 逻辑时间异步语义的证明台账：定理、Lean 核心、模型门禁 |
| [Map / Set 数据结构路线图](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/map-data-structure-roadmap.md) | Map / Set 数据结构路线图，含被否决的 seq-in-entry 设计及其错误原因 |
| [恢复 monkey 回归测试集：`spec/` 究竟是什么](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/mir-caps-call-count-breach.md) | 2026 年 8 月 monkey-suite caps 记账诊断、恢复的公开来源及待做的当前复现 |
| [关键任务能力攻坚清单](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/mission-critical-attack-list.md) | 分层清单，让 Almide 能有据可依地用于关键任务领域 |
| [native：Box 包装的（递归 variant）字段中的嵌套 ctor／字面量模式](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/native-boxed-pattern-lowering.md) | 历史嵌套装箱模式诊断，已实现 native lowering 及回归引用 |
| [Native Trust Spine — Perceus 作为单一内存模型（#764）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/native-trust-spine.md) | 让 almide build --target rust 经与 wasm 相同的 v1 Perceus MIR，使单一内存模型服务两条路径 |
| [embedded wasm 路径上的 net Unix 套接字与 shm（临时 ops 104..=116）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/net-unix-shm-embedded.md) | net 的 Unix 域套接字和共享内存文件使用的临时 embedded host ops 104..=116，以及在 ADR-0023 step 7 的退役。 |
| [外部视角审计 2026-07 — 消减证据层技术债](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/outside-review-audit-2026-07.md) | 2026-07-27 五视角外部评审审计：证据层落后于 v0→v1 迁移，诚实梯度反转（内部文档诚实、对外主张错误）；分层清理至“零错误主张，每个门禁真实”，附 issue #913–#932 |
| [Playground UI 更新](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/playground-revamp.md) | Playground UI 更新：文件标签、分享URL、TS风格示例库 |
| [0.29 之后的改进盘点](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/post-0.29-improvement-sweep.md) | 0.29.0 后改进扫描：从表面修复到 v0 wasm 退役，将全部开放缺口建 issue 并排序 |
| [Protocol：声明式符合 + 主动选择的 `any P`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/protocol-any-existentials.md) | 声明式符合 + 主动选择的 `any P` 存在类型——采用 Go 的 interface 值易用性，而不采用其隐式满足与 nil-interface 陷阱；唯一值得借鉴的 Swift 想法，其余均不采用。 |
| [Receipt Logic——回执逻辑](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/receipt-logic.md) | 信任层的形式基础——回执逻辑：主张类型、威胁模型、信任基底、反证程序及相对于用途的完整性。 |
| [Reconciliation 后续——v0.28.0 暂缓的 develop 剩余事项](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/reconciliation-followup.md) | v0.28.0 协调后续：为 0.28.1 推迟的 develop 提交 |
| [result-family-from-type — layout 是类型的函数，名称只选择代码](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/result-family-from-type.md) | 从类型而非模块/函数名称表导出调用结果布局 |
| [根除 Return-op 分化：`!` 一次脱糖，删除位置群](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/return-op-eradication.md) | 为 ! 运算符只做一次脱糖，删除按位置分化的 return-op 群 |
| [rot-eradication — 腐化分布与有参考依据的治理措施](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/rot-eradication.md) | 编译器腐化的分布与有参考依据的治理措施 |
| [通过构造实现安全](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/security-by-construction.md) | 安全类别台账及已记录控制，明确剩余缺口和主张范围 |
| [Self-host linking v2 — 链接 mono instance，退役 twin matrix](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/selfhost-link-v2.md) | Self-host linking v2 — 链接 renderer 已生成的单态化 generic 函数体，退役 shim registry 的逐类型 twin |
| [Shell 脚本立场：结构化流水线，不做交互 shell](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/shell-scripting-surface.md) | 借鉴 Nushell 的脚本数据模型，不构建交互 shell |
| [标准库重新设计（三层）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/stdlib-redesign.md) | 对照九种语言的标准库三层重设计：规则、分类表、逐模块职责表、每个公开函数的判定，以及采纳提案对应的 issue |
| [测试覆盖面 25×——迈向参考编译器规模](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/test-surface-25x.md) | 将已提交测试表面扩大 25 倍，迈向参考编译器规模 |
| [Ticks 接口审计——所需的 API 形态是否不同？](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/ticks-interface-audit.md) | 对照 timeout 形态 API 审计 ticks：无法写出的数字问题、三项修正。 |
| [Trust Layer——机器编写软件的信任层](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/trust-layer.md) | 品类战略：赢得“机器编写软件的信任层”，包含 MWS Trust Levels、回执、关键路径 |
| [类型 Where 约束](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/type-where-constraints.md) | 在既有 inline generic bound 之外，提案 where 子句语法及类型定义约束 |
| [v0 wasm codegen：Try/Unwrap/Fan 提前 return 的 heap leak](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v0-unwrap-early-return-leak.md) | 退役 v0 wasm emitter 的 Try/Unwrap/Fan 提前返回堆泄漏，保留为该类历史记录 |
| [目标PROMPT — 完成ADT brick 5（heap-field bind + recursive drop），打通首要突破点](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-adt-brick5-goal.md) | 历史ADT brick-5目标prompt、已完成recursive drop及保留的被否决尝试 |
| [v1 ① — 将自定义 ADT（variant）作为一等值](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-adt-value-model.md) | 将自定义 ADT（variant）作为 v1 值模型中的一等值 |
| [V1 待办——trust spine 剩余事项（含优先级）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-backlog.md) | v1 trust-spine 积压：健全性、drop 完整性、自托管表面、wall |
| [v1 Bolt 待办表（AI-DLC 管理）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-bolt-backlog.md) | v1 攀登的 AI-DLC Bolt 积压：将 camp/step 路线写为意图驱动、限时 Bolt（意图/完成定义/gate/依赖/状态）。构造护栏为 goal-prompt 纪律，各 Bolt 出口门禁是独立评审（reviewer agent + Trust Spine CI + 无偏双 oracle corpus），人类 Mob 决定标示的分叉，追踪距各顶点还剩多少 Bolt |
| [v1 → develop 回流策略](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-develop-reflow.md) | 2026 年 7 月整合 develop-v1 的历史提案，不是待执行请求 |
| [v1 堆结果 `if`/`match` 执行 — 设计（已降低风险：无需修改 Coq）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-heap-result-control-flow.md) | 历史首个 heap-result 控制流切片及后续 lowering 背景 |
| [v1 join 完备性与线性 checker](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-join-completeness.md) | 通过 bind 位置 join 消除 continuation 复制，并使已证明 checker 对 witness 大小呈线性。 |
| [v1 KGI / KPI 记分板](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-kgi-kpi.md) | 2026 年 6 月历史 KGI/KPI 记分板；区分目标与已测量、已证明结果。 |
| [v1 循环携带堆所有权——方案 C（cert-spine 扩展），完整性修复](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-loop-ownership-cert.md) | 以证书 spine 扩展处理循环携带的堆所有权，是完整性修复而非特例 |
| [Almide v1：以 MIR 为唯一真相的单一语义架构](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-mir-architecture.md) | 单一语义架构：所有权和布局在 MIR 中只决定一次，renderer 仅重放决定。 |
| [组织字节验证 — 在两种目标上运行各仓库自身的向量](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-org-byte-verification.md) | 组织范围 v0==wasm 字节验证扫描及其暴露的 wasm 缺陷类别 |
| [v1——parser-TCO 杠杆（真正跨仓库的 heap-result-expr 杠杆）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-parser-tco-lever.md) | parser-TCO 杠杆：解锁跨仓库 heap-result 表达式 |
| [Almide v1 Phase 1：MIR 核心 + 双渲染器——实现设计](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-phase1-mir-core.md) | 历史 MIR Phase 1 实现设计，以及 IR 与 MIR oracle 的区别。 |
| [v1 证明架构 — 构想（最终形态）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-proof-architecture.md) | 提案的最终态证明架构，当前实现及可信基仍另行限定范围 |
| [v1 Proof Spine — #31 进展（全V / leak-freedom / 传递 caps / 抽取缺口）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-proof-spine-progress.md) | v1 proof-spine 进展 — task #31（全V / leak-freedom / 传递 caps / 抽取缺口）哪些已证明、哪些仍待完成。记录 CapabilityReach.v 的传递 caps 定理（2026-06-21）。 |
| [v1 — records 功能：svg 全面攻克（目标提示词）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-records-svg.md) | records/svg 历史攻关、最终实测结果及原生回退例外 |
| [v1 发布路线 — 分阶段发布计划](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-release-path.md) | 2026 年 7 月历史发布路线决策，纠正零风险及资格化夸大主张 |
| [v1 stdlib self-host — 机制阶段（Option / List构造 / closures）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-selfhost-machinery.md) | 早期self-host机制活动历史；后续closure/drop工作及剩余linking提案 |
| [v1 自托管 print 底座 — ③ 可观测性基石](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-selfhost-print-floor.md) | 自托管 print 底座，支撑其余自托管工作的可观测性基石 |
| [v1 trust-spine正确性漏洞（对抗扫描2026-06-27）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-spine-correctness-holes.md) | 对v1 trust spine的对抗扫描（2026-06-27）及暴露的正确性漏洞。 |
| [v1 系统图 — 全貌（Mermaid）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-system-map.md) | 预期 v1 保证架构及历史成熟度图，不是已完成信任链证书 |
| [v1 TCO — 自递归尾调用 → 标量状态循环（yaml parser 的关键支柱）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-tco-self-recursion.md) | 将自递归尾调用变成标量状态循环 — yaml parser 的关键支柱 |
| [V1 → V0 对等——完成计划](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-v0-parity.md) | 历史 incumbent-v0 对等计划，明确已测试和见证保证的限制 |
| [v1 动态 Value 模型——yaml 基石（路径 A：自托管 + 一个受信任递归 drop 例程）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-value-model.md) | 动态 Value 模型：自托管加一个受信任递归 drop 例程 |
| [支柱 C 扩展：将 Value RC 纳入证书覆盖区域](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/value-rc-cert.md) | 将 Value 引用计数纳入证书覆盖区域 |
| [WASM 引用计数释放：所有权纪律的问题清理](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/wasm-frees-ownership-discipline.md) | 通过机械化所有权纪律，清理引用计数释放的积压问题 |
| [WASM 所有权 emit 层的机械化（从结构上遏制 Perceus 漂移）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/wasm-ownership-emit-mechanization.md) | 所有权 emit 事故历史与已实现的测量基础设施 |
| [WASM 平台前沿 — 核心 Wasm 3.0 之外](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/wasm-platform-frontier.md) | 历史平台调查，附当前 core/component/embedded 路线引用 |
| [门禁语言边界](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/zero-committed-shell.md) | 哪些门禁属于 Almide、哪些保留 bash，以及如何守住界线 |

<a id="on-hold"></a>

## 搁置

32 项

| 项目 | 说明 |
|------|-------------|
| [通过 almide-lander 实现 Almide 间 FFI](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/almide-to-almide-ffi.md) | 使用 almide-lander，通过共享库 FFI 从 Almide 调用已编译的 Almide 库 |
| [Almide UI — 作为 Almide 库的响应式 Web 框架](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/almide-ui.md) | 以纯 Almide 库构建类似 SolidJS 的响应式 UI 框架 |
| [API 差异与自动版本管理](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/api-diff-auto-versioning.md) | 在已交付 stdlib interface gate 之外，面向 package 的 API 差异与版本建议 |
| [LLM 基准：下一阶段](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/benchmark-next-phase.md) | LLM 基准 Phase 2–3：跨语言比较、更难问题、发表 |
| [编译期契约](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/compile-time-contracts.md) | 通过 where 子句定义编译期前置条件和类型不变量 |
| [将 `try:` 片段从 Rust 字面量中外部化](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/diagnostic-snippet-externalization.md) | 将诊断修复片段从 Rust 字面量迁至经检查源码模板的剩余工作 |
| [Flow[T] — 惰性流式序列](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/flow-design.md) | Flow[T] 惰性流式序列，flow.* 命名空间的动词与 list.* 对齐。 |
| [Flow[T] — 用户规范（草案）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/flow-spec-draft.md) | Flow[T] 面向用户的规范草案——在阶段 1 后提升到 docs/specs/flow.md |
| [GPU 计算——Matrix 类型与编译器驱动的 GPU 执行](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/gpu-compute.md) | 具有编译器驱动 CPU/GPU 执行的 Matrix 原始类型 |
| [IR 优化 Tier 2](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/ir-optimization-tier2.md) | 用于跨目标 IR 优化的 CSE 与内联 pass |
| [law 块实验 — 测量 C-ASSERTED 的记录（2026-09-27）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/law-blocks-experiment.md) | C-ASSERTED 的首次实地测量。让 LLM 阅读 law 块（对所有输入成立的性质）的 MSR 实验、同样的 law 能有多少仅靠 Lean 自动 tactic 关闭的实验，以及逐函数写 law 能否无需数学技法就组装证明的实验。记录数字、发现、距离 Lean 级还缺什么，以及下一步。 |
| [law 证明层 — 设计（草案，2026-09-28）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/law-proofs-design.md) | 以 Lean 为 kernel 证明逐函数 law 的层设计。纯片段自动翻译、计算 proved/tested/refuted、Int 回绕与余数语义、终止性、翻译信任链、Lean 许可和商标、分阶段计划。 |
| [LLM 集成](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/llm-integration.md) | 用于库生成、自动修复及代码解释的内置 LLM 命令 |
| [LSP 代码操作 — 剩余重构](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/lsp-code-actions.md) | 已交付诊断 quickfix 之外，生成重构和 import 管理的剩余覆盖 |
| [小型 ML 推理运行时](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/ml-inference.md) | 使用编译时模型特化的小型 ML 推理运行时。 |
| [MLIR后端与Egg重写引擎](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/mlir-backend-adoption.md) | Stage 2 渐进 lowering：dialect walker 通过全部 227 个 spec 测试 |
| [中央包注册表](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/package-registry.md) | 既有 git package 系统之外的集中包发布、所有权及来源 |
| [依赖版本约束](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/package-version-resolution.md) | 已交付 MVS resolver 之外，剩余依赖范围语义及可用版本选择 |
| [性能研究：通往世界 #1 的路径](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/performance-research.md) | 通过语义感知优化超越手写 Rust 的研究计划 |
| [Porta Embedded——WASI host 上小于 10KB 的 Almide IoT agent](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/porta-embedded.md) | 嵌入式 WASI agent runtime 提案；大小及跨 host 目标需要 hardware 证据 |
| [进程组合：管线是值，运行是独立步骤](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/process-composition.md) | 将命令管线构建为值，运行它是一个显式步骤；搁置至出现真实需求 |
| [Rainbow Bridge — 将外部代码包装为 Almide package](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/rainbow-bridge.md) | 通过 @extern 将外部 Rust/TS/Python 代码包装为 native Almide package |
| [基于区域的内存管理](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/region-inference.md) | 历史 v0.23.4 region 工作及未实现的完整 region 推断提案 |
| [研究：修改存活率论文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/research-modification-survival-rate-paper.md) | 测量跨语言 LLM 代码修改存活率的学术论文 |
| [The Rumbling — Almide OSS 重写活动](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/rumbling.md) | 用 Almide 重写 OSS 工具，以证明 WASM 大小及 LLM 准确度的活动 |
| [通过设计保障安全](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/secure-by-design.md) | 分层安全模型提案、已实现基础及剩余信任边界 |
| [Shell 补全](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/shell-completions.md) | 用于 bash/zsh/fish 自动补全的 almide completions 子命令 |
| [基于文件的快照测试 — 搁置](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/snapshot-testing.md) | 已交付源码字面量快照断言之外，可选的文件型快照 |
| [监督与 Actor](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/supervision-and-actors.md) | 将 Erlang 风格 actor、supervisor、类型化 channel 作为标准库模块 |
| [WASM 异常处理](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/wasm-exception-handling.md) | 提案的 Wasm 异常 lowering；上游默认值依赖版本及 engine |
| [WASM HTTP 客户端](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/wasm-http-client.md) | 已交付 wasm HTTP client 路线与剩余默认 component 迁移 |
| [Web 框架](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/web-framework.md) | stdlib http 上的 Hono 风格外部框架，集成 template 和 Codec |

<a id="done"></a>

## 完成

15 项

<details>
<summary>显示全部 15 个归档项目</summary>

| 完成 | 项目 | 说明 |
|------|------|-------------|
| 2026-07-23 | [代码健康：Codopsy 驱动的文件拆分与函数分解](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/code-health-codopsy.md) | Codopsy 驱动代码健康改进：拆分 1000 行以上文件，分解 cog>100 函数 |
| 2026-07-15 | [主张措辞：Perceus 表述与字节一致性保证的范围](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/claim-wording-perceus-byte-identity.md) | 两项主张措辞修正，使公开介绍 100% 由测量支持 |
| 2026-06-02 | [闭包 codegen 的跨目标差异](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/closure-codegen-cross-target-gaps.md) | 对抗差分扫描发现的 native/wasm 闭包代码生成差异，全部 8 项已修复 |
| 2026-06-02 | [闭包架构 v2](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/closure-architecture-v2.md) | 闭包架构 v2——一个身份、一个捕获集，提升即lowering；将闭包表示与内联优化分离 |
| 2026-04-20 | [Variant 穷尽性细化](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/variant-exhaustiveness-refinement.md) | 不穷尽 match 提示缺少分支代码，不可达分支改为硬错误 |
| 2026-04-20 | [重实现 lint：通过签名匹配检测 stdlib 重实现](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/reimpl-lint.md) | 检测签名匹配标准库函数的用户函数，建议委托 |
| 2026-04-19 | [Stdlib 声明式统一——走向单一事实来源](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/stdlib-declarative-unification.md) | 推动 stdlib 采用单一事实来源：`.almd` + 多 target ABI attribute |
| 2026-04-19 | [Codegen 理想形态](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/codegen-ideal-form.md) | 面向声明式 dispatch 与显式 symbol 解析的 WASM codegen 重设计。 |
| 2026-04-17 | [内置 Almide Stdlib — 理想形式](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/bundled-almide-ideal-form.md) | 捆绑 Almide 标准库理想形：一个 dispatch 路径，无补丁层特例 |
| 2026-04-16 | [标准库 module 的捆绑 Almide 调度](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/bundled-almide-dispatch.md) | 让 stdlib/&lt;module&gt;.almd 扩展 TOML module（修复 codegen dispatch） |
| 2026-03-19 | [HKT 基础 — 阶段 1–4 + 流融合（全部 6 条定律）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/hkt-foundation-phase1.md) | 包含类型构造器与代数定律的 HKT 基础 Phase 1–4 |
| 2026-03-19 | [效应系统 — 阶段1–2](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/effect-system-phase1-2.md) | 含 7 类的效应推断引擎及 checker 集成 |
| 2026-03-11 | [Playground 修复轮次](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/playground-repair.md) | Playground AI 错误修复及类型 checker 集成 |
| 未记录 | [WASM优化路线图](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/wasm-optimization-roadmap.md) |  |
| 未记录 | [Almide WASM Engine——全面重设计](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/wasm-engine-redesign.md) | 围绕 LayoutRegistry、WasmIR、WasmBuilder 全面重设计 WASM engine |

</details>

