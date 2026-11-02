说明：v1 登顶的 AI-DLC Bolt 待办表——将 camp/step 路线图表示为 intent 驱动、有时间限制的 Bolt（各含 intent / 完成定义 / gate / 依赖 / 状态）。构建防护规则为 goal-prompt 纪律；各 Bolt 的退出 gate 为独立评审（reviewer agent + Trust Spine CI + 无偏 dual-oracle corpus）；标明的分岔由人类（Mob）决定。跟踪每个顶峰“还剩多少 Bolt”。
<a id="v1-bolt-backlogai-dlc-管理"></a>

# v1 Bolt 待办表（AI-DLC 管理）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-bolt-backlog.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](v1-bolt-backlog.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](v1-bolt-backlog.zh-CN.md)

> 固定原文提交的非规范阅读快照；以原文为准，不自动同步。未独立执行、验证实现、检查证明或测量。历史命令与设计指令是资料，不是已执行的指令。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-bolt-backlog.md)

> 原文注记：这是 2026-09-27 的计划/测量快照，不是今日发布状态、市场排名、执行授权或资格化证据。保留 incumbent/structural 历史区别、部分 witness 覆盖及 byte 绑定缺口；此处 certificate 为有范围的证明检查产物，不是外部认证。wasm test 通过率不是另测的 native 字节一致率，假模型 A/B 接线结果不是效果证据，所列 Llama 比较不能确立所有模型优势或市场因果优势。原文总数不作整合：919 个 build 列出 881+24+5，462 个 test 列出 392+61，55 个下游文件列出 37+0+17+3；文中未说明缺失或重叠分类。近似比例及所有状态/数量照录。默认 leg 计划、kit 就绪、物理 target 校准、实际 Ferrocene 使用/资格化是不同义务。历史自主执行、dispatch、预算、push、CI 指令为不活性资料，不构成授权或执行。

> **这是什么**：将通往顶峰的 camp/step 路线图组织为 **AI-DLC Bolt**（intent 驱动、有时间限制、数小时规模的工作单元）。**1 Bolt ≒ 1 commit/brick**。以“还剩多少 Bolt”衡量进度，在自主循环中逐一完成。
> **相关**：[v1-kgi-kpi](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-kgi-kpi.md) / [flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) / [effectful-27-blueprint](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/effectful-27-blueprint.md) / [interp-is-desugar-to-tostring](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/interp-is-desugar-to-tostring.md) / [arena-breakthroughs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/arena-breakthroughs.md)。
>
> **最后更新：2026-09-27（v0.64.0 发布日，origin/develop `d027bbb77`）**。相对前次 2026-06-21 的变化汇总于下文“三个月中发生了什么变化”。状态均有 issue/PR/commit 支持。

<a id="3-か月で何が変わったか2026-06-21--09-27"></a>

## 三个月中发生了什么变化（2026-06-21 → 09-27）

- **名称变了**：6 月的“v1”现在是 **incumbent leg**（`crates/almide-mir`，持有证书的一侧）。默认发布的是从零新建的 **structural leg**（`crates/almide-wasm`）。Stage 2 在 #1599-#1601（2026-08-26）落地，v0.60.0 成为默认。来源：`docs/wasm/README.md`、`docs/ARCHITECTURE.md`。
- **oracle 变了**：不再是“与 v0 字节一致”，而是 native ⇄ wasm 字节一致（`spec/wasm_cross`、契约台账 C-NNN）+ interp 的 3-way。
- **①（运行率）越过底线**：无偏 corpus 从 6 月 0/15 提升到 666+15 / 684（约 99.6%，见下文 C1-B5）。
- **结构倒转**：**默认发布的 leg 没有证明**（是 trusted 而非 proven）。证书仅在 incumbent 侧。structural witness 在 wasm_cross 789 个中仅 **1 个**（`d9945cd57`）。→ Camp 4 主线从“增加证明”转为“**把证明迁移到默认 leg**”。
- **② 的工具已发布**：v0.64.0 发布 `almide survive` / `apply --if-survives`（#2147）和结构化 `repair` 诊断（#2149）。但 A/B 实际数字仍不存在。

<a id="ai-dlc-マッピング"></a>

## AI-DLC 映射

| AI-DLC | 本项目中的实体 |
|---|---|
| **Intent**（构建什么） | Camp（execution / MSR / WASI / proof / flight） |
| **Bolt**（限时工作单元） | 1 commit = 1 brick（含 intent + DoD + gate） |
| **Construction 纪律** | goal-prompt（筑墙而非留洞、push+CI、dual-oracle、false-green、root-fix-not-revert） |
| **Bolt 的 done-gate** | 独立 CI（Coq+公理+PCC）∧ native⇄wasm 字节一致 + 3-way oracle ∧ 契约台账 ∧ 纪律。**自行运行不等于完成** |
| **Mob checkpoint**（人类） | 号令 / 优先级 / 范围 / “这个目标正确吗” / MSR 结果→GTM 方向。GitHub 使用 `mob` 标签 |

<a id="bolt-テンプレ"></a>

## Bolt 模板

```
Bolt <ID>: <一行 intent>
  DoD  : 機械でチェックできる完了条件
  gate : 検証手段(CI / dual-oracle / unbiased corpus)
  deps : 依存 Bolt
  状態 : ⬜未 / 🔄進行 / ✅完了 / 🧱壁(部分) / 📐設計済 / 🪦前提が変わり置換
```

模板译文：Bolt ID 与一行 intent；DoD 为机器可检查的完成条件；gate 为验证手段（CI / dual-oracle / 无偏 corpus）；deps 为依赖 Bolt；状态为 ⬜ 未开始 / 🔄 进行中 / ✅ 完成 / 🧱 墙（部分）/ 📐 已设计 / 🪦 前提变化而被替换。

---

<a id="-camp-1--実行パリティほぼ完了残りは-incumbent-退役へ合流"></a>

## 🏕 Camp 1 — 执行对等【基本完成；剩余汇入 incumbent 退役】

- **C1-B1：执行 heap-result variant match** — ✅
  #1492 工作线（2026-08-17）**仅靠 lowering 即通过**。Coq/checker 未变，6 月“Camp 4 gated”是误诊。wall corpus 为 7/10，剩余 3 个为 Map/Set heap-key typed twin（→ N-B4）。来源：[camp4-general-heap-match](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/camp4-general-heap-match.md)。structural leg 可 lower 任意形状。
- **C1-B2：剩余 heap 值管线** — ✅（🪦 旧直方图以 incumbent 为前提，故替换）
  当前实测（#2739）：spec build lane 919 个 → structural 881 / 仅 incumbent 24 / 两 leg 均拒绝 5。test lane 462 个 → structural 392 / incumbent 61。
- **C1-B3：closures C2（first-class）** — ✅
  Closure Architecture v2 P0-P6 完成（[closure-architecture-v2](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/closure-architecture-v2.md)）。structural 的 env drop-glue（`3f60174d3`）、route handler closure（`cf90ab9d7`）。“闭包没有 ABI”系列：#2287 ✅ / #2288 ✅ / **#2289 🔄**（→ N-B3）。
- **C1-B4：stdlib 广度** — 🔄（基本完成）
  1,003 个公开 fn 中 structural 942、仅 incumbent 19、两 leg wall 42。函数级 reachability frontier 为 0（PR #1224）。剩余是 host capability。`proofs/wasm-reachability-baseline.txt`、`proofs/target-availability.toml`。
- **C1-B5：⭐ 用 *无偏* corpus 测量诚实距离** — ✅ **已重新测量（2026-09）**
  almide-dojo 的 LLM 解（另一进程生成）892 个 → structural 666 / incumbent 15 / 两 leg 拒绝 3 / 旧语法 208（排除）。**有效 684 个中约 99.6% 在 wasm 上通过**（6 月 0/15）。#2739、`dc832c1b9`。
  ⚠ 这是“在 wasm 上 test 通过”的比率，并非另行计算的 native 字节一致率。
- **C1-B6：跑通实际 repo（cross-repo conquest）** — 🔄
  csv ✅ / svg ✅（6 月）/ almide/dfa 在 wasm 上 50 个 test 字节一致。下游 4 个 repo（almai / comide / golemide / gramide-cli）55 个 test files → structural 37 / 依赖 incumbent 0 / 两 leg 拒绝 17 / E081（http/process）3。
- **Camp 1 Exit**：真实 agent 程序运行且字节一致 → **实质达成**。剩余为“两 leg 拒绝”的尾部和 incumbent 退役（N-B1）。

<a id="-camp-2--msr存亡の賭け道具は揃い実測待ち"></a>

## 🏕 Camp 2 — MSR（生存之赌）【工具齐备，等待实测】

MSR 工作移至 [almide/almide-dojo](https://github.com/almide/almide-dojo)（repo 边界见 CLAUDE.md）。方向从“二元生存率”转为“**保持契约的变更库 + survive 恢复的 A/B**”。

- **C2-B1：构建 MSR 对照实验** — ✅（🪦 6 月的 `msr-v2` workflow 已替换）
  Dojo 每日 lane（`src/main.almd`，harness 本身由 Almide 编写）与跨语言 lane（`src/msr/`，7 种语言 21 个任务）。每次运行标记 verdict `comparable` / `inconclusive-saturated`。为应对饱和，已投入保持契约的变更库（dojo#3、400 个任务、8 个 family、hidden oracle、wasm leg、resource oracle）。
- **C2-B2：运行并给出数字** — 🔄
  - 每日（2026-09-27、v0.63.1、38 个任务、comparable）：Llama 3.3 70B 28/38（73%）、Llama 3.1 8B 13/38（34%）。
  - 跨语言（2026-09-22、Llama 70B、21 个任务）：**Almide 80%** / Rust 95% / Go 100% / TS 100% / Zig 61% / MoonBit 47% / Gleam 14%。**Almide 输给 Rust/TS/Go**。
  - 没有 Sonnet 5 行（#1617 已关闭。“不把 API 密钥放进 CI”的裁定意味着需手动测量）。
  - 变更库 pilot（400×3×3）在 2026-09-25 运行 6h43m 后取消，尚未取得饱和 verdict。
  - #1963（P-critical）“thesis 没有当前测量”仍 OPEN。
- **C2-B3：⭐ 按重新定义的 MSR（失败明确性+可恢复性）重测** — 🔄（工具已发布，等待 A/B）
  v0.64.0 发布 `almide survive` / `apply --if-survives`（#2147、CLI/MCP/LSP）和结构化 `repair` 字段 + 双向 ratchet + `explain --list`（#2149 ✅）。dojo#58 已实现 survive A/B（control vs survive，相同预算 4 次，胜利条件 = 平均尝试次数 ≤ 0.8×control）。假模型的接线演示为 3.00→2.00，但这不是效果证据。
  剩余：dojo#59（pin 到 v0.64.0）→ 使用 Llama 70B dispatch（Workers AI 调用约 2,200-2,500 次）。#2147 在数字产生前保持 OPEN。6 月 inject-mistake catch 率已吸收到 survive A/B（推测）。
- **契约相对生存工作线 #1998**（2026-09-07 批准）：#1994 / #1995 / #1996 / #1991 / #1997 / #2009（L2/L3）✅，剩余 dojo#3（变更库）和 #1589（generic protocol）。edit-locality L1 预测循环（`proofs/l1-verdicts.toml` + dojo `src/l1_loop.almd`）正在运行（[edit-locality-theory](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/edit-locality-theory.md)）。
- **Camp 2 Exit（Mob 判断）**：决定 GTM 主轴。**跨语言 Almide 80% < Rust 95% 支持 6 月判断：“仅靠二元 MSR 无法取胜，② 验证+恢复更稳健。”** survive A/B 数字是最终判断材料。

<a id="-camp-3--検証済み-wasmwasi-component-producer大半は済検証部分が残る"></a>

## 🏕 Camp 3 — 已验证 WASM/WASI component producer【大半完成；“验证”部分尚存】

- **C3-B1：effectful WASI floor** — 🔄（大半完成）
  clock 经 host op 60 到达 embedded host / p1 shim / p2、p3 component 全部（`124d80cfb`）。fs 与 env.os/temp_dir/cwd 由 p1 shim 提供（`7782e66d0`）。p3 有 fs read / write（#1662 / #1666）。`declared ⊇ used` 已在 Coq 证明（`proofs/CapabilityBound.v` 为函数级，`CapabilityReach.v` 为传递闭包）。`--profile critical` 为 deny-all + `--allow`（#567 ✅）。
  剩余：env/process 的 p3 host surface（#1628 剩余项、#2589）。
  ⚠ capability 词汇是 nat 索引的通用 allowlist，未将 Fs/Net/Clock/Random/Env 分别做成 Coq 类型。是否视为满足 6 月 DoD，由 Mob 判断。
- **C3-B2：Component Model 边界验证/最小化** — 🔄（产出已完成，验证 ⬜）
  stage 0 adapter（#1631）→ stage 1 direct p2（#1632，大小为 adapter 形的 1/3.2）→ stage 2 direct p3（#1646）+ fan.any cancel（#1664）+ fs → stage 4 WASI pin ledger（`proofs/wasi-pin-policy.toml`）。canonical ABI（lift/lower）证明与最小化论证都**还没有**。跟踪 #1628 OPEN。
- **相关落地**：wasm VM #865 ✅（PR #2304，parity 681 一致 / 0 不一致）。http layer（v0.64.0：`http.serve` 在 embedded wasm lane 工作，C-367；call handle C-366）。后续 #1710（wasi:http@0.3 client）及 #2702（P3 handler export）。
- **Camp 3 Exit**：产出已验证 component → **产出达成，“已验证”等待 Camp 4 的迁移（N-B2）**。

<a id="-camp-4--証明の完全性incumbent-上は概ね達成--既定-leg-への載せ替えが本線"></a>

## 🏕 Camp 4 — 证明完整性【incumbent 上大致达成 → 迁至默认 leg 是主线】

- **C4-B1：leak-freedom / reuse / call-mode** — 🔄（**incumbent 上 ✅，structural 上 ⬜**）
  leak-freedom：`OwnershipChecker.check_sound` + OwnershipLoop。reuse：FreeList.v / FreeListRc.v（#909 ✅）/ CowSafety.v。call-mode：`CallModes.v`（`c485bef9a`）。runtime：StructuralRuntime/Alloc/Decode/Run.v（#576 ✅）。native ownership certifier #2231 ✅（台账 752→0，剩余 #2239 ✅）。
  ⚠ 默认 structural leg 中通过证书检验的 wasm_cross 仅 **789 个中的 1 个**（grow-only ratchet，`d9945cd57`）。剩余 #2755-#2759（branch / loop / closure / call-modes / names / caps）。
  ⚠ [v1-kgi-kpi](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-kgi-kpi.md) 的“4/8”仍是 07-30 时点，已过时。
- **C4-B2：byte 绑定（Gap 1，最难关）** — 🔄（部分）
  rc primitive（`$rc_inc` / `$rc_dec`）在 Coq 内已有 bytes→ISA 定理（WasmEncode/Exec/Isa/Decode.v、StructuralDecode.v）。任意程序 witness→bytes 未达成。近期一步是 **#2760**：从 structural build 再生成 receipt，pin 各 `.wasm` 的 SHA-256。
- **Camp 4 Exit**：安全性组合**在默认 leg 上** 8/8 → 信任主张走向飞行级。

<a id="-camp-5--飛行級cert-スパイン③④市場--kit-はほぼ揃った"></a>

## 🏕 Camp 5 — 飞行级（cert spine，③④市场）【📐 → 🔄，kit 基本齐备】

- **C5-B1：将 WCET / counted-loop 纳入 Coq** — 🔄
  #569 ✅（WCET 三层叙述 + gated PID kernel，PR #1642，`docs/project/WCET-STORY.md`）。循环所有权：`proofs/OwnershipLoop.v` / `CoownLoop.v`。G-F2（将循环提升至 Coq 并证明分配次数上界）⬜。structural witness 的 loop frame 为 #2757。target calibration 仍在外部。
- **C5-B2：生产 MIR→Rust + Ferrocene** — 🔄
  Ferrocene 迁移 lane 记录为 Rust 1.95.0 上 633 个全绿（#573 ✅），trace-map #572 ✅。但 [lane 定义](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/.github/workflows/ferrocene-lane.yml) 明确在引入 subscription 前使用 **stock rustc**。不可将该记录视为使用 Ferrocene 资格化二进制的运行证据，或 Almide→Rust lowering 的语义保持证明。**native Rust 存在两套 producer 的问题 #1480 等待 Mob**。
- **C5-B3：参考应用 + 资格化 kit** — 🔄（接近 ✅）
  参考 PID 应用为 C-230，G-F4 为 #776 ✅。kit 的 #566 / #567 / #568 / #574 / #575 / #576（Coq 45 定理）/ #571 dossier / #1534 签名全部 ✅，跟踪 #586 于 2026-09-21 关闭。剩余 ALS（CG-1）PARTIAL（als 侧参考求值器进行中）。
  ⚠ `proofs/TOOL-QUALIFICATION.md` 的 Gaps 表及 [flight-qualification](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-qualification.md) kit 表 10-13 已过时。
- **Camp 5 Exit**：飞行阶梯 G-F0..G-F6 → 安全关键/航空（合作）。**kit 侧齐备，将证明放到默认 leg 的 N-B2 是瓶颈**。

<a id="-新-bolt2026-09-に発生した本線"></a>

## 🆕 新 Bolt（2026-09 出现的主线）

- **N-B1：incumbent wasm emitter 退役**（#1696 step 4-5 → 跟踪 #2739）— 🔄
  DoD：两 leg 的 route census（91 行、shrink-only、#2741）降为 0 → 删除 WAT emitter 约 2.6 万行（#2761）。最大原因是 p1 的 `to_wasi` 缺 fs 操作（25 个）。各项为 #2742-#2753、#2703。deps：N-B2（若未先迁移证书至 structural，退役会使证明消失）。
- **N-B2：⭐ 将证书放到默认 leg（structural witness 阶梯）** — 🔄
  #2754 ✅ → #2755-#2759（branch / loop / closure / call-modes / names / caps）→ #2760（receipt 再生成 + SHA pin）。DoD：C-SAFE / C-PROVEN receipt 从 structural build 产生。使通过证书检验的 wasm_cross 从 1/789 增长。**Camp 3 / 4 / 5 的 Exit 全部经过这里**。
- **N-B3：把 fusion 做成 self-hosted HOF 本体的特化**（#2289）— 🔄
- **N-B4：Map/Set heap-key typed twin**（C1-B1 wall corpus 剩余 3 个，路线分割 `4d75df7ce`）— 🔄
- **N-B5：实测 survive A/B**（dojo#59 → dispatch → 判定 #2147 胜利条件）— ⬜ **等待 Mob 批准（Workers AI 预算）**
- **N-B6：重跑变更库 pilot 取得饱和 verdict**（dojo#3，9/25 取消）— ⬜
- **N-B7：将 make-verify 改为 survive / repair 版**（重启 S-B1）— ⬜
- **N-B8：Stage 4 durability — fuzz true-green streak**（`proofs/STAGE-STATUS.md` 中为 **0 天**）— 🔄
- **N-B9：#1628 剩余项**（host 边界 determinism 的 C 行、env/process p3）与 http 的 p3 化（#1710 / #2702）— 🔄

<a id="-頂上-bolt--gtm"></a>

## 🏔 顶峰 Bolt — GTM

- **S-B1：make-verify 杀手级演示** — 🔄（**停滞**）
  `demo/make-verify/` 自 2026-06-18（`cd3c4e248`）后未更新。使用 v0.64.0 的 `survive` / `repair`，已可实际演示“在应用前阻止错误修改并展示修复方法”（→ N-B7）。

---

<a id="mob-checkpoint人間あなたが決める分岐"></a>

## Mob checkpoint（由人类——你——决定的分岔）

**截至 2026-09-27 等待判断的事项**（GitHub `mob` 标签 8 项）：

- **survive A/B dispatch 批准**（N-B5）：Workers AI 预算。跨语言 lane 仅手动执行（dojo#55）。
- **frontier 模型（Sonnet 5 等）的行**：在 CI 不放密钥的方针下，由谁何时手动测量。
- **跨语言 80% < Rust/TS/Go 的处理**：是否用作 GTM 向 verification 方向靠拢的依据。
- **#1480 两套 native Rust producer 的统一**：C5-B2 瓶颈。
- **C3-B1 capability 词汇**：nat allowlist 算满足 DoD，还是分为 Coq 类型（soundness-critical）。
- **C4-B1 完成定义**：“哪个 leg 的证明？”本文定义为“在默认 leg 上”（N-B2）。

**持续存在的分岔**（6 月以来不变）：接下来攀登的 Camp/Bolt / 号令层级（开始、变更范围）/ proof、Coq 信任词汇扩展（必须由人判断）。

<a id="自走ループmob-以外"></a>

## 自主循环（Mob 以外）

```
Bolt を1つ取る(優先 = 既定 leg の証明被覆を最も上げる / release blocker が先)
  → goal-prompt 規律で construct
  → done-gate(独立 CI ∧ native⇄wasm byte一致 ∧ 契約台帳 ∧ false-green ∧ 壁)
  → 通れば次 Bolt / 落ちれば root-fix
  → self-run で完了にするな・危険1タスク延期≠セッション終了
```

循环译文：取一个 Bolt（优先最大幅度提高默认 leg 证明覆盖 / release blocker 优先）→ 按 goal-prompt 纪律构建 → done-gate（独立 CI ∧ native⇄wasm 字节一致 ∧ 契约台账 ∧ false-green ∧ 墙）→ 通过则下一 Bolt，失败则 root-fix → 不得以自行运行作为完成；延期一个危险任务 ≠ 结束会话。

<a id="進捗ビューあと何-bolt"></a>

## 进度视图（还剩多少 Bolt）

- **🚩 第一个商业顶峰（① agent 市场）** = Camp 1 **实质完成** + C3-B1（剩余 env/process）+ S-B1（N-B7）——**接近**。技术齐备，缺演示和恢复数字（N-B5）。
- **🏔 真正顶峰（所有市场、飞行）** = N-B2（默认 leg 证明）→ N-B1（退役）+ C4-B2（byte 绑定）+ Camp 5 的 G-F2 ——**瓶颈为 N-B2**。6 月是“Camp 4 escalation”限速，现在是“证明已有，但未附在发布物上”。

<a id="リリース--issue-スナップショット2026-09-27"></a>

## 发布 / issue 快照（2026-09-27）

- 最新：**v0.64.0**（2026-09-27，“应用编辑前检查、诊断中的修复、以及 http 层”）。同日 pre-release v0.64.1-rc1。
- open issue 88 项（enhancement 52 / A-wasm 29 / A-codegen 12 / A-stdlib 12 / proof 10 / mob 8 / bug 5 …）。
- **release blocker 3 项**（`scripts/count-release-blockers.sh`）：#2780（regression，自 0.64 起 `process.exit` 拒绝 126..255，wrapper 无法返回子进程 exit status）/ #2782（I-divergence，wasm `bytes.repeat` 绕过 C-197 上限检查而 trap）/ #2783（I-divergence，native `matrix.linear_f32_row_no_bias` 跳过 C-161 而 panic）。
- 按版本的计划见 [ROAD_TO_1_0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/ROAD_TO_1_0.md)（2026-09-27 重排 0.65 以后）。本文 N-B1 / N-B2 对应其 0.6x 剩余，C2 系列对应 0.8x。

---

<a id="履歴-mob-判断②-を勝ち筋として-active-昇格2026-06-18"></a>

## 历史：Mob 判断“将 ② 作为取胜路线提升为 active”（2026-06-18）

对照 north-star，判断 ②（验证+恢复）是取胜路线，①（运行率）是底线，并重排优先分岔。当时 active 为 S-B1 / C2-B3，Mob-gated 为 C4-B1 + C1-B1 cert-precision，淡出的是 C1-B2 / C1-B4。

**2026-09-27 再评价**：判断**维持并强化**。① 在无需 Camp 4 escalation 的情况下由 structural leg 越过底线（C1-B1 gated 是误诊）。② 已作为工具于 v0.64.0 发布，跨语言数字（Almide 80% < Rust 95%）也支持“仅靠二元 MSR 无法取胜”。剩下两个问题：用数字展示 ② 效果（N-B5），以及将验证附到发布物（N-B2）。
