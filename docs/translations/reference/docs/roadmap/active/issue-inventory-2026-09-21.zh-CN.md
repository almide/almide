<a id="open-issue-inventory-2026-09-21--78-issues-under-one-header"></a>

# 未关闭 issue 清单，2026-09-21 — 78 个 issue 采用统一 header

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](issue-inventory-2026-09-21.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](issue-inventory-2026-09-21.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](issue-inventory-2026-09-21.zh-CN.md)

> 非规范性固定历史阅读版；原本不变且为权威，不自动同步。未执行引用命令、CI修改、测量或当前issue复查。保留清单数量与日期状态，包括去重4项与script7项/#2400解释、等待决策与单PR修复分类的原文不一致；这不是当前任务清单。Web API和deployment adapter是提案，前提功能交付不证明框架已交付。比较及260KB是历史原文主张，未独立重测。保留代码及旧API写法，但不认证可运行。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/issue-inventory-2026-09-21.md)

这是 v0.63.0 final tag 前夜、全部 open issue 添加统一 header 后的快照：一个 `A-*` crate label、闭集合中的状态、一行摘要和证据。轴与 header 由 #2417 记录于 `docs/project/ISSUE-TAXONOMY.md`，七个 agent 使用的填写规范是 `issue-inventory-2026-09-21/issue-header-spec.md`。数字是 counter 对当时78个 open issue 的自检，不是手工统计：`issue-inventory-2026-09-21/render-issue-index.py` 生成 `issue-inventory-2026-09-21/issue-index-2026-09-21.txt`。数量变化时应重跑，而非编辑本页。

```
open issues: 78   with header: 78   without: 0
  no A-* label:                                0
  state outside the closed set:                0
  state=BLOCKER vs I-*/regression mismatch:    0
  two A-* labels: 7 (#1423 #1479 #1480 #2141 #2143 #2289 #2387 — each really spans two crates)
```

途中遇到一个计量工具 bug，记录于此以免后人引用其错误读数：状态名称 `計測器・ゲート` 含分隔符 `・`，按第一个 `・` 切分会截断名称，于是 counter 报告**“25 个不在闭集合内”**。问题在 parser，不在数据；现在按已知集合从最长名称开始匹配前缀。

<a id="release-blockers-4"></a>

## Release blocker（4个）

这是 `count-release-blockers.sh` 为 final tag 设置 gate 的集合。script 按 label 累加，而 #2400 有两个 blocker label，因此打印 **7**；去重后为4。

| issue | crate | 2026-09-21 状态 |
|---|---|---|
| #2395 | A-wasm | PR #2399 进行中 |
| #2397 | A-wasm | PR #2409，draft，等待 #2404（同一 ledger 行） |
| #2398 | A-wasm | PR #2404 进行中；#2399 落地后 rebase |
| #2419 | A-wasm | branch 已修复；judge 侧 almide/als#87 于2026-09-21合并，PR #2420 |

落地顺序：#2399 → #2404 → #2409；#2420 独立于这三项。

<a id="by-state"></a>

## 按状态

| 状态 | 数量 | 备注 |
|---|---|---|
| BLOCKER | 4 | 上述 |
| 实现 | 32 | 前提阻挡3（#1005→#1003、#1584→#1696、#2147→#2149）；等待决策3（#1107、#1696、#2010）；**可开始26** |
| 计量工具／gate | 25 | 等待决策3（#924、#1963、#2382）；前提阻挡1（#2009→#1997）；**可开始21** |
| 设计决定 | 7 | #1460 #1479 #1480 #1490 #1679 #2143 #2162 — 每项都需要○× |
| 跟踪 | 5 | #586 #1514 #1628 #1998 #2302 |
| 调查 | 5 | #2078 #2099 #2150 #2156 #2415 — 第一步是测量 |

<a id="by-crate"></a>

## 按 crate

| label | 数量 | issue |
|---|---|---|
| A-ci | 25 | #586 #924 #1423 #1469 #1508 #1514 #1617 #1998 #2009 #2141 #2143 #2146 #2152 #2163 #2380 #2381 #2382 #2387 #2390 #2391 #2400 #2402 #2403 #2405 #2406 |
| A-wasm | 22 | #1315 #1584 #1628 #1696 #1710 #1997 #2010 #2099 #2141 #2143 #2150 #2156 #2312 #2318 #2387 #2395 #2397 #2398 #2407 #2408 #2411 #2419 |
| A-codegen | 9 | #1331 #1480 #2044 #2045 #2162 #2289 #2293 #2393 #2410 |
| A-driver | 8 | #1003 #1490 #2092 #2147 #2148 #2151 #2153 #2372 |
| A-perf | 6 | #1963 #2142 #2157 #2302 #2331 #2415 |
| A-frontend | 5 | #1388 #1456 #1479 #1589 #2149 |
| A-stdlib | 5 | #1107 #1423 #1460 #2078 #2289 |
| A-ir | 3 | #1005 #1479 #1480 |
| A-runtime | 1 | #1679 |
| A-modules | 1 | #2332 |

A-ci 25 + A-perf 6 = 31个 issue 是计量工具与 gate；语言和编译器本体（wasm 22 + codegen 9 + frontend 5 + ir 3 + stdlib 5）为44。数量级相同，体现了所述优先级：质量 gate 高于语言功能，这在 backlog 形态中可见。

<a id="the-a-ci-25-sorted-by-what-closing-them-takes"></a>

## 按结案所需工作整理 A-ci 的25项

本节回答：CI 桶能否一次清空？14项可以，其余是持续计划或需要先做决定。

**一个 PR 即可完成、无未决设计问题的 gate 修复（14项）。** 每项都是可测量前后的 script 或 workflow 修改；改进 CI（#2386、#2416、#2418）进入 develop 后，可按单 issue PR 并行。

| issue | 修复内容 |
|---|---|
| #2400 | 计数去重 issue，而非逐 label 求和 |
| #2403 | `check-als-pin.sh` 同时比较 statement 和 id |
| #2406 | 对照 ledger 验证所有 `C-NNN` 引用，修复 stdlib 一个悬空引用 |
| #2405 | manifest generator 拒绝过时 tree，gate 读取当前跳过的 field |
| #2402 | domain-edge matrix 覆盖全部22个 json signature |
| #2382 | fuzz ladder 将双 leg 的 resource-limit abort 归为 resource，而非 semantic |
| #2390 | nightly fuzz report 把被抢占 shard 算作 unknown 而非0，取消正文20项上限 |
| #2391 | ledger-counts 修复以 PR 交付，nightly 指出修复 branch |
| #2387 | domain-edges i32_max 行依据 compiler 而非空闲内存判定（确定性上界或声明 skip） |
| #2143 | wasm runtime ratchet 在 `GITHUB_ACTIONS` 下判定 |
| #2141 | 在链接 stdlib 的 corpus 上执行 wasm size ratchet |
| #2163 | 将最后两个 almide-gates twin 与其 `.sh` 原本比较并提升 |
| #2380 | fold-seed-consumes-its-own-source 形态的 source-level gate |
| #2381 | critical path 55分钟 → #2386 落地后测量；剩余拆分作为附数字的后续 |

**2026-09-21 已决定、现在可开始的3项。** #924：连续夜数统计交付至少75%计划 fuzz-minutes 的夜晚（从 `shards=k/N` 读取），而非“full budget”，后者由 `if: always()` verdict 结构性满足。#2390 先落地，因为连续夜数所读的行计数偏低。#1998／#2009：两项义务命名为 **S2／S3**（contract-relative survival ladder）；`docs/specs/edit-locality.md:27,33` 的 L2／L3（cross-target agreement、diagnostic locality）不变。两项裁定都在 issue 评论中。

**持续计划而非一次清扫（8项）。** #586 和 #1514（跟踪 issue）、#1508（reference-suite mining）、#1469（doc generator，“没有 generator”的前提错误，45份 stdlib doc 中43份已有生成 index）、#1423（target-availability matrix）、#2152（portable trust／attestation）、#2146（公开 MSR harness，属于 Dojo）、#1617（用 Sonnet 5 重跑 Dojo，是运行而非改代码）。

<a id="premises-that-had-moved--recorded-in-the-headers-not-deleted"></a>

## 已变化的前提 — 记录于 header，不删除

超过20个 issue 的前提已不成立。每项发现来自一位 agent 的低成本 tree 检查（grep、`ls`、读一行），保留原正文，在 header 一行摘要及证据中写入。**这些都不关闭或重定义 issue 范围；仍由人决定。**

- #2044 — item 1 已消失（`pass_auto_parallel.rs` 退役）；`fannkuchredux` 已 VICTORY，仅剩 `mandelbrot` 行。
- #2045 — `|>` 已融合；剩下 `list.range` 构建 `Vec<i64>` head。
- #2318 — direction 1 已落地（`region.rs` 的 `forget_window_temps`）。
- #2150 — native `AlmideMap` 不是线性扫描（超过16 entry 有 index）；`research/benchmark/perf/README.md:95` 的“linear-scan”是陈旧描述，已排独立 PR。
- #1584 — structural 已为默认；fallback baseline 25 → 8；剩余等待 #1696 incumbent 退役。
- #1514 — 未勾选 child 除 #924 外均 CLOSED。
- #1469 — “没有 generator”前提错误（43/45 stdlib doc 有生成 index）。
- #2152 / #2151 / #2149 / #2142 / #2163 / #1423 / #1710 / #2312 / #2387 / #2391 — 部分落地，剩余已在各 header 明确。
- #1107 — `@deprecated(since=3)` 窗口在 dialect 4 到期，`unwrap_or(` 剩128处，不是47处，方向变差。
- #1388 — 迁移成本从“2 + 37”涨到71个 file。

一个真实设计隐患已解除：#1998 与 #2009 提议的“L2／L3”名称，在 `docs/specs/edit-locality.md` 已有其他含义；自2026-09-21起改为 S2／S3（见上）。

<a id="ci-what-the-first-improvement-measured"></a>

## CI：首次改进测得了什么

同夜落地三项 CI 更改：#2386（重测 shard 权重）、#2418（承担 clippy ratchet 与 run_parity 的 pre-push hook）、#2416（required 集合 green 时 enqueue；mutation gate 在落地后评判 develop）。第一项根据 API job 时长、`Test Rust (shard N/4)` 墙钟分钟数做前后测量，结果是否定的：

| run | head | shard 0 | 1 | 2 | 3 | max |
|---|---|---|---|---|---|---|
| push（旧权重） | e39133dc5 | 57.6 | 24.8 | 44.1 | 36.9 | 57.6 |
| push（旧） | 636a9f771 | 57.0 | 41.4 | 46.2 | 36.5 | 57.0 |
| push（旧） | e8c95e285 | 59.0 | 24.1 | 45.8 | 31.6 | 59.0 |
| push（旧） | 824ba091f | 58.3 | 42.4 | 33.0 | 36.6 | 58.3 |
| 携带 #2386 的 merge_group | 281f23135 | 60.0 | 42.0 | 57.4 | 41.1 | 60.0 |
| push，新权重 | 281f23135 | 60.0 | 25.6 | 41.1 | 27.9 | 60.0 |

shard 0 之前57–59分钟，之后两次60.0；权重效果落在运行间 noise 内。机制：shard 0 的一小时是单个 step（`Cargo tests`58.9分钟、download 0.6），权重只能把整个 test target 在 shard 间移动，任何分配都无法缩短单个59分钟 step。杠杆是拆 target 或减少最慢项的工作（#2381）；更改 target 列表或 shard 数必须在同 PR 再生成 `scripts/ci-test-weights.txt`。记录两个测量陷阱：docs-only push 让 shard job 在3–4秒内 no-op，故前后数据必须确认实际执行测试；一小时 job 的逐 job log API 在 setup step 截断，step 内逐 target 明细仍未测量。

首次经过 pre-push hook 的 push 所测成本：clippy 部分在非固定 toolchain 下0.09秒打印 `NO VERDICT`（该机器无 rustup，由 CI 判断 ratchet）；run_parity 每次 push 耗时103–159秒。相较一轮30–56分钟 CI 很划算，而且这个 gate 本可捕获当晚三轮浪费中的两轮；需防范迭代 push 中习惯性用 `LEFTHOOK=0` 跳过。

<a id="open-follow-ups-from-the-inventory-pass"></a>

## 清单整理留下的后续事项

- #2398 的年代：同夜在发布 asset 测量，报告 program 在 v0.62.0 不复现。对 v0.62.0..v0.63.0-rc1 执行 `git bisect run`（431 commit、10次 release build），指出首个 bad commit 为092c40d05（PR #2048，typed drop glue 的 Map/Set entry，2026-09-08），因此对报告 program 标注 `regression`。但缺陷更古老：另一个 literal-list fixture 在公开0.62.0 asset 打印已释放内存（同夜测量；关闭 guard 后，没有 `list.update` 的四条保留 heap 路径仍 clean，所以 map 是塑造 heap 布局，不是第二 carrier）。092c40d05 改变了哪些 program 暴露缺失 retain，而非缺陷何时出现。step script、reproducer、两份 run log 和 verdict table 位于 `issue-inventory-2026-09-21/bisect-2398/`；表由 run log 重建，不是 git 自身的 bisect log（后者在 `git bisect reset` 后不保留）。限定仍有效：trigger 依赖周围 allocation，主张是“此 program 在0.62.0不暴露它”，而非“0.62.0正确”。
- #2417（header 格式和 label 轴）及 #2386（shard 权重）在快照时 open。
- `research/benchmark/perf/README.md:95` 的 linear-scan 陈旧描述 — 独立 docs PR。
