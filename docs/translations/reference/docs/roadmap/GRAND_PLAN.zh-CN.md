<a id="almide-grand-plan"></a>

# Almide 总体计划

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](GRAND_PLAN.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](GRAND_PLAN.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](GRAND_PLAN.zh-CN.md)

> 非规范性的固定读者快照。以原文为准，不自动同步，未独立重新验证实现、测量或安全性。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/GRAND_PLAN.md)
> 已勾选与未勾选项是固定计划的历史记录，不是新实现审计。尤其旧的未勾选 build-cache 项不表示当前 CLI 没有缓存；1.0 清单明确不是当前就绪或安全判定。后续运行时／平台项目仍是计划。


> “在 1.0 作为语言赢得成功。此后，在 UI 之前积累也能成为 app runtime 的基础。”
>
> 按版本的执行计划：[ROAD_TO_1_0.md](../../../../roadmap/ROAD_TO_1_0.md)——向 0.65 → 0.99 每个 minor 分配重大功能的台账（2026-09-27 重新规划，0.41–0.64 为出货记录）
>
> 从评价轴看取胜路径：[ALL_AXES.md](../../../../roadmap/ALL_AXES.md)——速度／性能／正确性／LLM writability／DX／生态 6 个轴各自的当前值、胜利条件与放弃的轴

---

<a id="phase-1-original-10-readiness-assessment-historical"></a>

## Phase 1：原始 1.0 就绪评估（历史）

以下清单记录早期评估，不是当前发布就绪判定，也不是 API 冻结。数量与排除模块均为历史。当前语言／API 行为应参考 [SPEC](../../../../SPEC.md)、[stdlib 参考](../../../../stdlib/README.md) 及实际发布／版本元数据。本文不意味着作出了 1.0 发布决定。

作为语言赢得信任。不增加功能，固定完成定义。

- [x] 冻结语言规范（确定语法、运算符、关键字，添加 protocol）
- [x] 稳定编译器行为（ICE=0、warning=0、cross-target 106/106）
- [x] 确定 stdlib 边界（22 模块 390 函数，添加 set，排除 uuid/crypto/toml/compress/term）
- [x] 错误信息质量（E001-E010、--json、hint system）
- [x] 破坏性变更策略（edition、FROZEN_API、REJECTED_PATTERNS）
- [x] showcases（CLI/API/Data/DevTool/Script 共 5 个）
- [x] 2,500+ 测试（已达 2,681）
- [x] Protocol System（用户定义 convention、generic bounds）

1.0 后持续改进：
- 示例／实用配方集／迁移路径
- LLM 测量（MSR、首次正确率）——不阻塞

**最初评估目标：** 明示稳定性依据，使人能够判断是否采用。仅凭上述旧勾选结果，不保证当前所有用途的安全性、兼容性或 1.0 readiness。

<a id="phase-2-production-language"></a>

## Phase 2：生产语言

强化分发单元，建立模块生态经济（module economy）。

- [x] 包清单（almide.toml）
- [x] 锁文件（almide.lock）
- [ ] 明文规定 semver 约定
- [ ] 公开 API 稳定性规则
- [ ] 构建缓存（增量编译）
- [ ] 可复现构建
- [ ] monorepo / polyrepo 支持
- [ ] 目标兼容性契约（让 multi-target 成为“承诺”）

<a id="phase-3-runtime-foundation"></a>

## Phase 3：运行时基础

确立 Almide 独特性。**将 effect 发展为 capability。**

- [x] **[HKT Foundation Phase 1-4](../../../../roadmap/done/hkt-foundation-phase1.md)**——TypeConstructor/Kind/代数法则、Stream Fusion、Ty::Applied 统一（已完成记录的范围）
- [ ] **HKT 旧剩余计划 Phase 5-6**——Effect 类型集成、Trait 集成（Phase 4 的 Ty 统一已计入完成记录）
- [x] **[Effect System Phase 1-2](../../../../roadmap/done/effect-system-phase1-2.md)**——自动推断 capability（7 类、传递性）、almide check --effects、Security Layer 2
- [ ] **[Effect System Phase 3-4](../../../../specs/effect-system.md)**——依赖限制、类型级集成
- [ ] **类型化宿主绑定（Typed host bindings）**——IDL／模式优先、代码生成优先，无需手写 FFI
- [ ] **资源／任务模型（Resource/task model）**——任务／调度器、资源生命周期
- [ ] **持久化／同步原语（Persistence/sync primitives）**——本地优先、CRDT／合并、同步队列
- [ ] **可观测性（Observability）**——效果跟踪、IR 转储、pass 检查器、运行时事件时间线
- [ ] **[编译期契约（Compile-Time Contracts）](../../../../roadmap/on-hold/compile-time-contracts.md)**——通用函数前置条件、类型／值不变量仍为提案。与现有 generic bounds／test-context where 不同，旧 newtype/where 草图不作为当前语法

**指令：“让 effect 成为 运行时权限模型（runtime permission model）的核心。”**

<a id="phase-4-app-runtime-layer"></a>

## Phase 4：App Runtime 层

在 UI 前，构建让应用持续运行的执行模型。

- [ ] 宿主生命周期模型
- [ ] 存储／同步运行时
- [ ] 应用清单／权限清单
- [ ] 模块热升级（签名、ABI 检查、可回滚）
- [ ] 可选 UI 抽象（先做状态／效果／同步／宿主集成，再做按钮）

<a id="phase-5-platform-expansion"></a>

## Phase 5：平台扩展

- [ ] 移动端 runtime（iOS/Android）
- [ ] 桌面 runtime
- [ ] Web 嵌入
- [ ] 插件市场
- [ ] 托管分发／更新模型

---

<a id="やらないことリスト-今は"></a>

## 暂时不做的事

| 项目 | 原因 |
|---|---|
| 专用 UI 框架 | 运行时原语与宿主边界优先 |
| 自有渲染器 | 靠宿主 UI 或薄抽象即可 |
| 巨大 stdlib | 冻结成本高，要小而强 |
| “什么都能做”的定位 | 明确擅长什么、哪些暂不做 |

---

<a id="3つの最重要命令"></a>

## 3 条最重要指令

1. **发布 1.0**——谈未来前，先作为语言取得信任
2. **将 effect 发展为 capability**——Almide 的独特性在这里
3. **先做运行时原语，再做 UI**——成为 app runtime 的核心不是绘制，而是执行模型

---

<a id="一文で"></a>

## 一句话

> 完成通用语言的 1.0。
> 然后将效果系统转化为能力运行时的核心，
> 建立类型化宿主边界与运行时原语。
> UI 框架可以在此之后。
