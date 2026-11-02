<a id="outside-review-audit-2026-07--証拠層の負債バーンダウン"></a>

# 外部视角审计 2026-07 — 消减证据层技术债

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](outside-review-audit-2026-07.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](outside-review-audit-2026-07.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](outside-review-audit-2026-07.zh-CN.md)

> [日英混合正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/outside-review-audit-2026-07.md)的非规范性读者快照，固定提交为 `35297a612fb29e3f78f6587aba253aa91d51f466`。原文仍是正本。本快照不自动同步，也不是生成器／检查器输入。下文日期、计划、完成状态、测量和比较主张均保留为历史原文记述，不是独立复现或对当前版本的验证。代码块及注释保持原样。历史收集元数据以不生效的读者注记显示。

> 历史描述：2026-07-27 五视角外部评审审计发现证据层落后于 v0→v1，诚实梯度倒转（内部文档诚实，对外主张失实）；结合 issue #913–#932 分层消减，目标为“零虚假主张，门禁全部真实”。

> 原文注意事项：这是注明日期的七月审计记录，不是新审计，也不证明全部发现如今仍未关闭。强烈比较评价、缺陷数量和拟定时长是原文陈述，不是独立验证的背书或排期承诺。修复方案与 issue 操作是历史引用，本翻译不执行。保留失败证据并不赋予一般编译器或认证保证。

> 于 2026-07-27 执行。针对“作为 2026 年顶尖编译器接受外部行家（Rust/MLIR/Koka/CompCert 圈的评审者）审阅时会被指出什么”，并行从五个视角 — 架构／IR、性能、DX、证明／认证、生态／外观 — 扫描并汇总。全部发现已带 file:line 证据和结构性关闭条件，提交为 issue #913–#932 + bonsai-almide#1。本文是其地图。

<a id="核心の構図"></a>

## 核心格局

**v0→v1 迁移推进了编译器本体，但周围的证据层 — 主张、基准、门禁、文档 — 没有跟上迁移，诚实程度的梯度倒转。**

- 内部文档（TRUSTED_BASE.md / proven-vs-trusted.md / flight-*.md）诚实得让评审者惊讶 — 评价为“比学术 verified-compiler 论文更诚实”。
- 对外表面（README / TRUST-SPINE.md / BENCHMARKS.md / docs-site）集中着事实错误的主张。
- 对标榜 evidence-first 的项目，这是最深的弱点：**外部评审者发现一项虚假主张后，连真正的优势（契约台账、coqchk、skip 台账）也会打折看待。**

五份报告全部出现同型模式：性能榜是已删除 v0 emitter 的测量；roadmap/active 文档描述消失的目录；部分 CI 门禁徒有形式（fmt --check、axiom-clean、LEDGERED examples）；fuzz 在 20 晚中仅成功 1 晚。根因是**编译器变好了，证据却仍陈旧**。

<a id="第1層--外向き主張の虚偽-数時間1日級最優先"></a>

## 第一层 — 对外主张失实（数小时至一天级，最高优先级）

| issue | 内容 |
|---|---|
| #913 | README 的 Lean 主张三重失实：`allHeapFreed` = “Dec ≥1”（不是排除泄漏）；`perceusTransform` 是 12 行玩具；“Formally Verified”覆盖零证明的 native 执行端 |
| #914 | TRUST-SPINE.md 四项失实：V = 子串搜索；ALS 规范语义不存在（80 行／2 个定理）；TCB 含未使用的 CompCert/CertiCoq；checker 称“数百行”，实际 1,144 行 |
| #915 | fan 并发：`fan.map` 顺序执行，`fan.race` 只运行首个 thunk。docs-site 写有不存在的 tokio/TS backend 和已删除的 fan.timeout |
| #916 | 性能主张：README“faster than Gleam/MoonBit”指的是 LLM 工作时间；BENCHMARKS“100 ops 1.1s”无基准；vs-Rust 表是 v0 遗迹；wasm-opt 标志 docs≠code |
| #918 | 清理陈旧文档：PRODUCTION_READY 冻结于 v0.8.0；wasm-engine-redesign.md 描述已消失的 emit_wasm/；847 vs 834 |
| bonsai#1 | PERF_ROADMAP“724 tok/s”实测 0.725 — 1000 倍误记（朝显得获胜的方向） |

<a id="第2層--動かないゲート-アイデンティティの傷"></a>

## 第二层 — 不工作的门禁（伤及身份定位）

| issue | 内容 |
|---|---|
| #919 | `almide fmt --check` 始终 exit 0 — 使用它的全部 CI 门禁都是 no-op |
| #920 | axiom-clean“门禁”是 printf — 添加 Axiom/Admitted 仍为 green。修复是 3 行 grep + tamper drill |
| #921 | trust-spine.yml 不在发布路径（向 main 的 PR）上运行；paths filter 排除 frontend；缺少 wasmtime 时 check-wasm-exec 静默 exit 0 |
| #922 | LEDGERED examples 从未编译 — 22 例中 6 例损坏了 4 个月 |
| #923 | installed binary 中 `almide explain` 的 32 个代码有 21 个失败（docs 未嵌入） |
| #924 | fuzz-nightly 在 20 晚中仅成功 1 晚 — silent-wrong 类别（258 个已关闭案例中 49 个，占主导类别）的测量器停摆。阻塞 #796 |

<a id="第3層--構造債-arc-級依存関係あり"></a>

## 第三层 — 结构性债务（系列工作级，有依赖）

| issue | 内容 | 依赖 |
|---|---|---|
| #925 | frontend 每次构建运行 2 次，丢弃第一次 IR；6 套手工同步 driver → 统一 driver | #928 的前提 |
| #926 | `is_heap_type` 在 3 个 crate 的 6 处定义分歧（pass_anf 缺 Ty::Named，已伴随真实泄漏症状）→ 单一来源 | — |
| #927 | LSP 正确性：把 UTF-16 位置当字节（非 ASCII 时 panic）；rename 是字符串替换；didChange 执行 git fetch + lock 写入 | — |
| #928 | incremental/query 基础为零 — LSP 每次按键都运行批处理编译器 | #925 |
| #929 | wasm 执行端缺 nanopass optimizer（almide-mir 不依赖 almide-codegen）且 SIMD 为零 — 优化质量从结构上分叉 | #917 |
| #930 | 死代码清理：不可达的 26-pass wasm 管线（含第二套 Perceus）、两个废弃 Rust emitter、未使用的 wasm-encoder | — |
| #931 | 屏障诊断：`LowerError::Unsupported(String)` 无 span、`{:?}` 嵌套 dump 加“请提 issue” → Diagnostic 化及逐形状 rewrite hint | — |
| #917 | v1 执行端对 Rust 重新跑基准 + CI perf ratchet（目前基准哪里都没跑） | — |
| #932 | 机械化 renderer→证明字节链接（把 check-wasm-bytes.sh 的手写 heredoc 比较改为 renderer 实际输出） | — |

<a id="第4層--証明のコアギャップ-内部では自認済み対外整合が先"></a>

## 第四层 — 证明核心缺口（内部已承认，先统一对外表述）

不属于新起票对象（归已有系列工作／TRUSTED_BASE 剩余台账）。只要修正对外文档（#913/#914），以下即可成立为“诚实披露的研究课题”：

- 证明对象是由不受信任的编译器自己写的 certificate 字符串 — 生成者 = 主张者，相关 bug 不可见（已自行记录此类别出货 5 次）。
- 不存在 Almide 形式语义，`⟦s⟧ ≈ ⟦compile(s)⟧` 形式的定理为零 — 字节一致来自差分测试（→ #530 ALS 升格才是真正解法）。
- native 执行端证明覆盖为零（→ #764 native trust spine）。
- Perceus reuse `r` 已在 Coq 证明健全，但不存在输出 `Op::Reuse` 的路径 — 已证明、未实现。

<a id="守るべき強み-割引かれる前に"></a>

## 应守住的优势（被打折之前）

187 项契约的 CI 强制台账（带 README 漂移检测）、coqchk 独立复验、Lean 0 sorry、wasm skip 台账（30→11，必须有理由）、org-trust-status.md（公开自身失败）、770B verified Hello World、wasm-opt differential parity gate、rustc 级诊断数据模型（`try_replace_span`）、playground。
**验证文化本身超过获得资金支持的项目** — 五份报告的一致评价。第一至二层是阻止这些资产受损的工作，而非新增投资。

<a id="着手順"></a>

## 着手顺序

1. **Sprint 1 = 第一层 + 第二层**（全部为数小时至一天级）。完成状态：**“虚假主张为零，门禁全部真实有效”。** 将数值（定理数／checker 行数／stdlib fn 数）纳入 gen-claims 机制，防止再次漂移。
2. **#924 恢复 fuzz** — 测量器恢复前，“open silent bug 为 0”没有意义。
3. **第三层纳入已有系列工作，按常规推进** — 只固定 #925→#928 顺序。#929 根据 #917 测量结果定范围（先测再做）。

<a id="閉鎖条件"></a>

## 关闭条件

第一层 + 第二层全部 issue 关闭后，再跑一次相同的五视角审计，要求**对外文档 fatal 发现为零**。第三层各 issue 有自己的关闭条件（本文不等待它们）。达成时把本文移到 done/，复审发现成为新台账。

<a id="未起票の-serious-次の-dx-arc-でまとめる"></a>

## 尚未起票的 serious 项（下个 DX 系列工作集中处理）

REPL 每行运行 cargo（未用 almide-interp）；test 失败按文件报告（assertion 作为生成 Rust 的 panic 出现）；parse error 位于诊断格式之外的二等路径（span 经字符串往返还原）；无 error dedup（缺一个 import 出现 8 个相同错误）；formatter 输出偏离仓库自身风格，并改写 import。
