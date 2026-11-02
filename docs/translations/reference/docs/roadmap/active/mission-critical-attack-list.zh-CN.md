原文 description 元数据：分层清单，让 Almide 能有据可依地用于关键任务领域
<a id="mission-critical-attack-list"></a>

# 关键任务能力攻坚清单

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](mission-critical-attack-list.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](mission-critical-attack-list.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](mission-critical-attack-list.zh-CN.md)

> 固定原文的非规范读者快照；原文仍为权威，不自动同步。未独立验证实现或执行引用指令。历史状态、计划及比较主张保留原范围。
> [Original source (English) / 原文（英語）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/mission-critical-attack-list.md)

> 原文注记：mission-critical 可用性是目标，不是适格性认定或安全认证。勾选项报告原文当时的结果，未经独立重新验证。末表仍是 2026-08-18 基线，不因后续关闭更新而重算。A1-1 的 27 行与剩余 23 次弃权等不同计数，以及关于两项行为改变的不完整短句，均保留而不臆测统一。连续记录、qualification 与生产阶梯的未勾选条件仍未勾选。


写于 2026-08-18，deep-wash campaign 结束时（发现并修复 7 类 compiler bug，501 fixture corpus，wall 固定为 16，`8be514bb4` 的 CI green）。目标状态：**Almide 能有充分依据地用于关键任务领域。** 每项都有可度量退出条件，没有条件的项目不列入此表。

规模：S（≤1 天）/ M（≤1 周）/ L（≤1 月）/ XL（一个季度以上）。

<a id="tier-0--semantics-must-stop-moving-everything-else-builds-on-this"></a>

## Tier 0 — 语义必须停止变动（其他一切的基础）

- [x] **A0-1 (#530 + #1485 + #1483) (L) 规范冻结 + 兼容性契约。**
  DECLARED 2026-08-20（4997c7535，在 owner 指定顺序的 ADR-0012 D2/D3 环节之后）：docs/STABILITY.md — 冻结 surface（面向 LLM = stable）、ALS 一致性条款（“实现有错”）、破坏性变更政策（major + @dialect 升级 + migration note）；proofs/stability-closure.toml — 在观察数字前固定 N 的六项稳定性标准（#1485 CLOSED），每次 push 都报告。剩余消减工作仍归各自事项：#530（文字矛盾归零）、#1483（双向 surface 全面扫描 + experimental marker）。0.57.2 本身载有决定：DECIDED 2026-08-20（owner）：冻结 GO，但安排在 ADR-0012 error surface 最终状态落地之后——先 #1193（D2：`-> T!E` 在 fallibility marker 中携带 typed error），再 #1194（D3：fmt 将返回位置的 Result 正规化为 `T!E`/`T!`），因为 fmt 规范形式会改变要冻结的 surface。顺序：#1193 -> #1194 -> ALS 冻结声明 + 破坏性变更政策（变更 = 显式 major）+ #530/#1485/#1483 收尾草案。
  两项行为改变；patch 层级的语义变动会使该语言失去进入这个市场的资格。退出条件：ALS 冻结文档、破坏性变更政策（弃用周期，绝不默改），以及由 release gate 强制的 1.0 标准清单。

<a id="tier-1--close-the-verification-holes-compiler-correctness"></a>

## Tier 1 — 填补验证缺口（compiler 正确性）

- [x] **A1-1 (#1226) (L) Interp 的带 tag heap 范围（#1226 slice 2）。** HEAP_BOUNDARY 弃权 = 115/501 fixture；每次弃权都可能让共享 frontend bug 以 2-0 票通过（E0004 系列正是这种形态）。退出条件：弃权率 < 5%（投票 ≥ 476/501）。
  DONE 2026-08-19（#1226 closed）：弃权 135→27 行 = corpus 的 4.6%，实际三方投票 566/589（96.1%），每次增量都保持零异议；剩余 23 次弃权分类于 interp-abstain-classes.toml。
- [x] **A1-2 (XL, #1527) 按 fuzz 频率消减 wall。** subset wall 影响日常 code。按观测频率消减（每 30 分钟 fuzz run）：List 参数物化（约98×）、含调用分支的无法解析 `if`（约59×）、registry 缺口 `result.flatten/to_list/filter`、`list.zip_with`（约78×）、scalar-binding subset（约25×），再到 16 个固定 specimen（3 层嵌套、Map 值 map、tuple-list equality、heap-acc fold）。退出条件：每 30 分钟 fuzz run 的 wall < 100，且每次解除限制，都在同一 PR 将 specimen 加入契约下的投票 fixture（仅缩小规则已存在）。
  CLOSED 2026-09-06（#1527）：2026-09-06 nightly 为**每 30 分钟 64 wall**（8 × 295 s 内 11,129 program、84 wall），finding 0；walled-real ratchet 永久保持 0，所有解除限制均纳入契约。
- [x] **A1-3 (L, #1528) 负向测试 10×（test-surface-25x tier 1）。** 99 diagnostic pair 对 Rust 约15k，是最大的成熟度差距。退出条件：每个 E-code 都有 fixture 系列覆盖各 hint variant 与 fix-it verdict；≥1,000 pair；coverage gate 列出 fixture < 3 的 E-code。
  PROGRESS 2026-08-18：**749 pair**（wave-2 的 730 + #1518/#1521/#1486/#1515 系列）；三个新 E-code（E058/E059/E060）连同 fixture 系列落地。
  CLOSED 2026-09-02（#1784）：**恰好 1,000 pair**；多文件 fixture（`almide.toml` + `src/*.almd` 兄弟文件）取消 E033/E420 豁免，因此每个 code 都受 coverage gate 强制的 ≥3 系列标准约束。
- [x] **A1-4 (M, #1529) RC 放置 snapshot（koka parc 模式）。** 对 RC 关键 corpus（以 koka_parc* 系列为种子），把 RC 插入后的 drop/dup 放置作为预期输出，与 runtime 结果一起 commit，使今天无害的放置移动也能显著暴露。退出条件：CI snapshot gate 覆盖 ≥ 20 种形状。
  DONE 2026-08-19（#1529 closed）：21 形状 snapshot gate（tests/rc_placement_snapshot_test.rs），drop 顺序 mutant 的检出证据。
- [x] **A1-5 (M, #1530) heap 上限 leak harness（grain makeGcProgram 模式）。** 两个 target 都设硬性 heap 预算开关；每个 RC fixture 分别用 N 单位与 N-1 运行，静默 leak 变成确定性 OOM。退出条件：CI 中 churn corpus 在上限下运行，一个故意 leak 的 control 证明 harness 有效。
  DONE 2026-08-20（#1530）：两个 target 的 `almide build --heap-cap`（wasm 在 $alloc 中设 frontier ceiling，native 用 live-bytes global allocator，均采用定义的 C-197 abort 形态）；harness tests/heap_cap_test.rs — churn corpus 在上限下不受扰动，移除 rc_dec 的 mutant 中 4/9 出现确定性 OOM，native cap=1 强制生效。
- [x] **A1-6 (M, #1531) allocation 次数 assertion（roc alloc-count 模式）。** allocation counter 接口 + 精确的 loop body 内 allocation 次数，每个 assertion 都有 control program。退出条件：gate 覆盖 ≥ 10 种 loop 形状，assert 每次迭代 allocation 为零。
  DONE 2026-08-19（#1531 closed）：针对交付 WAT 的 static-zero gate（tests/alloc_count_gate_test.rs），10 种形状 + 2 个会 allocation 的 control。
- [x] **A1-7 (S) 已登记的三个 diagnostic。** #1509（`not (expr)` guard parse）、#1510（`t.0.1` float lexing hint）、#1511（fmt Option 正规化 E054——fmt 必须对所有合法 program 全域定义）。退出条件：三项全部带 fixture 关闭。DONE 2026-08-18，随 v0.57.2 发布。

<a id="tier-2--prove-the-findings-have-dried-up-time-is-the-ingredient"></a>

## Tier 2 — 证明发现已趋于枯竭（时间是必要因素）

- [ ] **A2-1 (M, #1532 + #924) fuzz nightly 加固 + 90 天连续 green。** 先修 drain 卡死（卡住的 native cargo build 存活超过逐 case timeout；runner 必须在 BUILD 阶段 kill；本周已卡死两个 campaign）。再做轮换 seed 的 nightly run、finding 自动登记与连续记录 meter。退出条件：连续 90 夜 green，零新增 correctness finding。
  PROGRESS：卡死问题于 2026-08-18 关闭（#1532）。meter 是 #924，其计数条件于 2026-09-21 批准：完成计划 fuzz 分钟数的 ≥ 75% 才算一夜（`shards=k/N`）；连续记录值可信之前须先落地 #2390。finding 不再登记到 #924（#2379 将其路由到 `fuzz-findings` label）。
- [x] **A2-2 (M, #1533) 实际 code 验收层。** 将真实下游 project（dfa、parsegen 等 consumer）在 CI 中编译并测试，形成 acceptance ring；E0004 与 #1501 都是实际 code 而非生成 code 发现的。退出条件：每次 develop push 上 ≥ 3 个真实 project 的 CI green。
  DONE 2026-08-20（#1533 closed）：Acceptance Ring workflow——每次 push/PR 用 develop 构建的 compiler 测试固定 ref 的 dfa/parsegen/svg，设 test-file 下限与 native-fallback 上限；两者都有 kill 证据；首轮起即 green。teastia（private）在有 read token 时加入。
- [x] **A2-3 (S) 新检测角度的节奏。** 每季度增加一个新角度（本轮增加 reference-suite 移植、panic-wash、nested-type matrix）。feature 落地时使用 #1508 backlog（or-pattern、string pattern、NaN-bits）。退出条件：每季度一条常设 roadmap；采用新角度且零 finding 的季度，就是这个市场要求的趋于枯竭证据。2026-Q3 超额完成（2026-08-18）：pass 隔离 semantic-preservation gate（#1487，已验证 mutant 检出）、perf ablation 部分（#1466）、cap-effect consistency gate（#1515）、带 seed 的 regex 差分（当天确认首次 CI kill）。2026-Q4 再次开展。

<a id="tier-3--outside-the-compiler-no-issue-fixing-moves-these"></a>

## Tier 3 — compiler 之外（修 issue 无法推进这些）

- [x] **A3-1 (#1534) (S) security 态势底线。** SECURITY.md 提供披露渠道、CI 中 dependency lock audit（以 MVS+lock 工作为种子），以及 release signing 方案。退出条件：文档化并从 README 链接。
  CLOSED 2026-08-31（#1534）：`SECURITY.md` 已 merge（#1636），演练中 dependency audit 对已知脆弱 pin（RUSTSEC-2020-0071）报 red，所有 release asset 都有 Sigstore attestation（`release.yml`）。
- [x] **A3-2 (#1535) (M) support 契约。** LTS 政策（哪些 version 获修复、持续多久）、versioning 保证，以及如实描述维护者范围的 bus-factor 声明。退出条件：SUPPORT.md 获批准。
  CLOSED 2026-08-27（#1535）：`SUPPORT.md`——每项保证都指出强制它的机制。
- [ ] **A3-3 (XL) qualification 资料包（仅在面向需认证行业时）。** 将现有证据——三方差分 oracle、295 contract 台账、gate 验证清单、wall 的诚实处理——映射到 tool-confidence 论证（IEC 61508 / ISO 26262 TCL 术语）。基础设施是非常强的原材料；资料包需要撰写和审计，而非新 code。退出条件：经 review 的 qualification dossier 框架。
- [ ] **A3-4 (XL) 生产部署阶梯。** 非关键内部 tool → 受监督 pilot → mission-critical，各阶有 incident SLO。退出条件：首次外部生产部署经历一个无 post-mortem 的季度。

<a id="sequencing"></a>

## 顺序

A0-1、A1-1、A1-2 是关键路径，它们决定“verified”究竟意味着什么。A1-3..7 与 A2-1..2 可自由并行。Tier 3 现在开始（A3-1 一天），与其他工作并行。诚实的时间表：Tier 0–2 需要集中工作一到两个季度；A3-4 的阶梯由市场驱动；A3-3 仅在目标行业要求认证时进行。

<a id="standing-metrics-check-each-quarter"></a>

## 常设指标（每季度检查）

| 指标 | 当前（2026-08-18）| mission-critical 标准 |
|---|---|---|
| 三方投票率 | 76.6% | ≥ 95% |
| 每 30 分钟 fuzz run 的 wall | ~500 | < 100 |
| diagnostic fixture pair | 99 | ≥ 1,000 |
| fuzz 连续 green 记录 | 2 rounds | 90 nights |
| CI 中真实 project | 0 | ≥ 3 |
| 未关闭 miscompile 类 bug | 0 | 0（保持）|
