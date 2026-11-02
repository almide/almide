说明：2026 年 6 月历史 KGI/KPI 记分板；区分目标与已测量、已证明结果。
<a id="v1-kgi--kpi-スコアボード"></a>

# v1 KGI / KPI 记分板

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-kgi-kpi.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](v1-kgi-kpi.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](v1-kgi-kpi.zh-CN.md)

> 固定原文提交的非规范阅读快照；以原文为准，不自动同步。未独立执行、验证实现、检查证明或测量。历史命令与设计指令是资料，不是已执行的指令。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-kgi-kpi.md)

> 原文注记：2026年6月填写记录及后续订正框架是历史资料。旧表中当前/安全/完美/100%健全表述保留原修辞，不是新增普遍保证。全生产安全、数百行资格化 checker、外部资格化、最准确机器编辑均为目标，不是已证实当前成就。4/8标签与安全性组合列举不整合计数而保留；无泄漏loop测试不增加Perceus定理，pattern检查不等于执行，无print的exit/输出一致未观察返回值。保留同日未开始条目与后续切片、变化的corpus数字、未完成hole-hunt/主张漂移检查，以及泄漏commit在发布前修复的原记录。原STATUS 6链接及wiki式引用为未解析源记法，不虚构目的地。每周模板仅作为不活性资料显示。未执行shell指令、self-host构建、证明检查或测量。

<a id="historical-scoreboard-targets-are-not-achieved-guarantees"></a>

## 历史记分板；目标不是已达成的保证

下方最新已填写周记录为 2026 年 6 月 21 日。无日期的“当前”栏是早期记分板，不是今日编译器、TS 支持、证明覆盖或 MSR 的测量。KGI-T 完整安全/资格化组合是目标，不是 corpus gate 绿灯就已确立的主张。

限定范围的证据见[当前证明边界](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/proven-vs-trusted.md)和 [receipt generator](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/receipt.sh)。依[仓库边界](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/CLAUDE.md)，MSR 工作属于 Dojo；此更新不虚构外部结果，不以估计填旧计数。下方保留每周观察及原指标。

> **这是什么**：每周填写 v1 最终指标（KGI）及防守/进攻指标（KPI）的记分板。本项目结构特殊——**KPI 分“进攻类（增长）”和“防守类（绝不能退化的不变量）”；一旦破坏防守，进攻成果即无效**。所以先看防守。
> **相关**：[v1-system-map](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-system-map.md) / [receipt-logic](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/receipt-logic.md) / [trust-layer](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/trust-layer.md)（L0-L4）/ [v1-proof-architecture](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-proof-architecture.md)。

<a id="守るものの本質一行"></a>

## 防守的本质（一行）

> **让必须信任的对象保持小而纯。** checker 保持数百行，信任基底保持声明的底层。若这崩溃，无论增加多少性质也无法达到 flight-grade。

---

<a id="kgi終局指標--これが真なら勝った"></a>

## KGI（最终指标——为真即“获胜”）

胜利是**合取**。仅一半是已知失败模式（只有信任 = CompCert 小众 / 只有可写性 = 普通 AI 语言）。

| KGI | 达成状态（应为真） | 当前 |
|---|---|---|
| **KGI-T（信任）** | 第三方**仅信任 irreducible floor（Coq kernel / wasm 语义 / wasmtime 忠实性 / HW / ALS 有效性）**，用**数百行资格化 checker**，对**真实生产使用的全部程序**，在**每次 build 重新推导**[安全性组合]。外部资格化（相邻市场 → 航空） | ⬜ 仅切片 |
| **KGI-W（可写性）** | 同条件、有对照组，机器能**修改 Almide 并比其他语言更准确地编写**（赢得受控 MSR） | ⬜ 未测量 |
| **合取（真正胜利）** | 上述 2 项**同时**为真——机器编写最准确，且输出在 flight-grade 上可信 | ⬜ |

**[安全性组合]** = 内存安全 / 无泄漏 / capability 有界 / 类型确定 / call 规约符合 / 字节 refine 源语义（C-FAITHFUL）/ 字节可复现（C-REPRO）。

---

<a id="守る系-kpi不変条件--1つでも破れたら攻める成果は無効"></a>

## 防守类 KPI（不变量——破一项则进攻成果无效）

**目标是“始终 = 此值 / ≤ 此上限”。不是增长，而是不退化。**

| 防守 KPI | 应守值 | 当前 |
|---|---|---|
| **checker 规模** | ≤ 数百行，**不依赖程序 / 语言 / 编译器复杂度**（规模 ∝ #事件字符 + #subset 性质 + #op→pattern 表） | ✅ |
| **信任基底（TB）** | = 声明 floor。**creep 为零**（import 增加也由台账界定） | ✅ 维护 TRUSTED_BASE.md |
| **公理纯度** | 全定理 `Print Assumptions` ⊆ 标准，信任扩展（native_decide 等）为零 | ✅ “在全局上下文下闭合” |
| **独立检查证明** | kernel-checked + coqchk + 跨版本（0 sorry / 0 Axiom） | ✅ 9.1.1 + 9.2 |
| **主张漂移** | = 0（公开主张 ⊆ 机器核对） | ⚠️ 尚未机器核对（手动发现并修正 1 项） |
| **静默通过** | = 0（受理 ⟹ 安全或 `[COMPILER BUG]` 停止；shortcut 全有机器 gate） | 🔄 hole-hunt 尚未干涸 |

**此表含义**：以“破坏防守 KPI”换进攻成果，不是前进而是**净损失**（checker 成数千行，即使覆盖全语言也丢掉资格化对象）。这是“整理结构之战”的 KPI 表达——**破坏不变量的进步，不如没有**。

---

<a id="攻める系-kpi進捗--kgi-へ向けて伸ばす"></a>

## 进攻类 KPI（进度——向 KGI 增长）

| 进攻 KPI | 0 ──────────→ 完成 | 当前 | gap / C-* |
|---|---|---|---|
| **性质覆盖** | 安全性组合中的 多少 / 8 | **4 / 8**（mem✅ name✅ cap✅ type✅ / leak、reuse、call-mode、byte 剩余） | C-SAFE / C-PROVEN |
| **证书格式表达力** | eager → perceus → full | **eager**（i/a/d/m 合法，r/b 拒绝） | 横跨（格式 v1） |
| **真实程序覆盖**（③ EXECUTE 且 v0 字节一致） | 端到端执行且与 v0 一致的真实 repo/函数 | **真实 repo 2 个 full-conquest**（`csv` 4/4 + `svg` records render）+ 多个 fixture——均字节一致 + 无泄漏（10⁴ loop）+ corpus-wall ACCEPT | Gap 3 |
| **语言面覆盖**（③ 执行 path） | subset → call → 控制流 → closure → nested heap → 全语言 | **call / 控制流 / heap-result if/match / closure（defunc inline）/ nested-heap list / records（构造、field、spread、递归 drop、List[Record] literal+concat）/ `Map[String,String]` entries**——真实 repo EXECUTE。剩余：Set / guards / 一般 Map（非 String 值）/ TS-target | Gap 3 |
| **字节绑定** | §3 契约（信任）→ 已证明 | **仍为信任** | **Gap 1（核心）** |
| **frees / leak-freedom** | eager（已知泄漏负例）→ frees（无泄漏已证明） | **eager** | Gap 2 |
| **受控 MSR** | 未测量 → 与对照组之差 | **未测量** | 支柱 A |
| **外部** | 本地 → CI 强制 → 多样性 2 实现 → CertiCoq 机器码 → 相邻资格化 → 航空 | **CI 强制 ✅** | KGI-T |

<a id="gap--攻める-kpi-対応"></a>

### gap ↔ 进攻 KPI 对应

- **Gap 1**（最重要、最深）：字节绑定——witness ⟹ wasm 字节从 §3 契约变为已证明。WasmCert-Coq import + runtime heap refinement 证明 + per-build V'。
- **Gap 2**：frees / leak-freedom / reuse——将精确所有权模型（Perceus per-edge）纳入 MIR。证书格式 a/m/r 字符是将其落实为 ground-fact 的入口。
- **Gap 3**（= ③ 执行对等，下个主目标）——**2026-06-14 实测修正范围**：
  验证链（frontend → MIR → proof checker）**已通过 call 和控制流**（实测 ACCEPT：`helper()` 调用 `im` / `if-then-else` `im` / `match Option` `ad|im` / `for` 累积 `ad`；gate.sh 已有 two_functions/transitive_caps）。`render_wasm`/`render_rust` 也处理 MIR op（含 CallFn），V（translation_validation #570）以 **pattern 检查**确认 op→wasm 指令存在。**过时“subset、无 call”不适用于验证侧**——验证侧已能验证 call/控制流程序的**所有权**（call 在 cert 上仍 deferred/elided）。
  **所以 ③ 真正 frontier 是“v1 忠实 EXECUTE 真实程序，且与 v0 字节一致”**：当前 V 不执行，只 pattern 核对；执行为 v0（双 oracle reference，receipt C-REPRO“until v1 parity”）。lower 为 cert defer call，所以**执行需 un-defer call（忠实调用+真实结果）的 EXECUTION path**。③ = **建立 v1 执行 path、达到 v0 parity**（faithful call → 控制流执行 → closure → nested heap），每段以 v0 字节一致覆盖测试。真实程序覆盖（端到端=执行且 v0 一致）是进度计，**GTM 解锁条件**。验证侧先行，因此这是“建立执行”的新轴。

  **第一切片实测（2026-06-14）：建立 EXECUTION 入口工具并跑一例**：
  新增 `crates/almide-mir/examples/render_program.rs`（.almd → 全函数 lower → MirProgram → `render_wasm_program` → 完整 wat module，EXECUTION 侧 emit_cert_from_source 对应物；production/验证代码未改=安全）。实测：render 输出 op 结构（用户函数 call `$double`、算术、控制流），但 RUNTIME 是临时脚手架（preamble 手写 WAT：仅 `$print_list`/`$print_int`/list_copy/itoa，无 `$print_str`/`$int.to_string` 等）。

  **第二切片实测（2026-06-14）：HEAP 值真实程序通过 v1 执行 path，与 v0 字节一致（无 print、遵守纪律）**。3 例——`fn greeting()->String="hi"; main{let _g=
  greeting()}` / List 返回 call / String chained call——在 **v1（lower→MIR→render_wasm→wasm→wasmtime）exit 0、输出一致**。2 修复（均在 render，不增加手写 WAT）：
  ① **`Init::Str` un-defer**（lib.rs 加 `Init::Str(String)`，alloc_init 为 LitStr 装真实字节。ownership cert 不变，Alloc 无关内容仍一个 `i`）。
  ② **call-result-repr 修复**（`value_reprs_wasm` 读取 CallFn 的 `result` Repr。String/List 返回 call 为 Ptr=i32 handle，scalar i64 默认与 `$alloc` i32 类型不一致）。
  新增回归 `heap_returning_call_types_result_as_i32_handle`（end-to-end build_and_run）。corpus-wall（in-profile 4083、3 性质 ACCEPT）、gate、cargo test 93/0 全绿。

  **print = self-host 的实证确认（纪律之墙）**：唯一真实 print 路径 `println→PrintStr`（`lower/calls.rs`）要求 `$print_str`。手动在 preamble 加 `$print_str` 后 `println("hello")` 可运行并与 v0 字节一致，**但** discipline test（`handwritten_wasm_runtime_does_not_grow`、baseline 11）立刻阻止——**正是 v0 陷阱**，测试信息也说“用 Almide self-host 新 routine，再经 CallFn 调用”。因此 revert `$print_str` routine，保留 `PrintStr→(call $print_str)` arm（为未来 self-hosted print 正确接 CallFn）。`$print_int`/`$print_list` 在真实源码无 producer（仅测试 fixture），所以临时 print 对真实程序是死的。**print 的前进 = Phase 3 self-host**（低层 Almide subset：内存/host-call primitive + Almide 写 print_str）。
  **③ 下个切片候选（顺序）：(a) scalar 调用执行 → 第三切片 DONE（下文）。(a') scalar 值计算（算术/字面量）——`Op::Const` 搬运值 + `IntBinOp`（IntOp 仅 Add/Sub/Mul，Div/Mod/比较需扩展）。当前无 print，端到端不可观察 ⇒ 只能 PrintInt unit test 验证；Op enum 12 处变更，侵入性大。(b) print = self-host（大型，Phase 3）。**

  **第三切片实测（2026-06-14）：SCALAR call 通过 v1 执行 path，与 v0 一致 + 验证覆盖提高**。
  新增 `try_lower_scalar_call`（lower/calls.rs），将 scalar-result Named / pure-Module call lower 为真实 `CallFn{dst:Some, args:lower_call_args, result:Some(scalar)}`（镜像 heap call binds.rs:114，去掉 heap-push）；bind/tail 失败时**rollback→defer**（Const + elided marker）保持全域性（新 wall 零，in-profile 4083 不变）。4 例 battery（`add(2,3)` literal 参数 / 未用 scalar 结果 / 嵌套 `g(f())` / scalar+heap 混合 `mix(5,"hi")`）在 **v1 valid wat → wasmtime → 与 v0 exit/输出一致**（无 print ⇒ 值未观察，但结构运行）。**副作用：验证覆盖 UP**——deferred call 实体化使 caps 3528→**3582**（+54 函数 TAINTED→VERIFIED），ownership 13007→**13153**（+146，heap 参数 Alloc+Drop 平衡）。
  **3 agent 对抗验证 SOUND**（brick #56 系健全 caps 回收——不健全 flip 零；2 函数 de-taint 后暴露真实 Stdout 可达性，正确保持未验证；name 1:1 保持、caps 集不变、double-count gate 0、proven-checker 兜底）。回归 `scalar_user_call_lowers_to_executable_callfn`（from-source lowering，cargo test 94/0）。
  **剩余：module scalar call 因 `$string.len` 未定义，执行悬空（= heap module call 同样 self-host gap；验证改进）。corpus-wall.sh caps step 存在旧 harness flake（3582 连续 checker spawn 的 OS-OOM/shared-scratch，确定性重查 0-reject——将来 batch/in-process）。**

  **⚠ 设计核心——v0-reuse 是 v0 陷阱（见 v1-mir-architecture.md §4 与 ⚠ 注）。正确路线为 SELF-HOST RUNTIME**：**用 Almide 写 runtime**，经同一 Core→MIR→target，由 v1 自行编译（dogfooding，rt-oracle/drift 类消失）。**MIR 仅留最小 alloc/RC primitive，Push/IndexSet/Print/string/list/json/… 改为 self-host runtime fn Call**，不烘焙进 op。当前 render_wasm.rs 手写 WAT（list_copy/itoa/print 烘焙进 op 的脚手架）是“v0 陷阱本身”，只是展示“能运行”的脚手架；**不增加手写 WAT 是纪律**。收敛：(a) 烘焙 op 换 runtime fn Call → (b) MIR op 缩至 primitive → (c) runtime Almide 化（Phase 3、v1-mir-architecture.md §4 / #575/#576）。

  PUNCH-LIST（顺序）：(1) 将 MIR op 集定义为 primitive（alloc/RC + 不可分项），Push/Print 等换 runtime fn Call；(2) **用 Almide 写最小 stdlib**（旧 `research/selfhost/`、现 `research/spike/v1-mir/` 方向）→ render_program 将 program+runtime 经 v1 编译 → 与 v0 字节一致；(3) faithful Module/Computed call 执行 → 控制流 → closure → nested heap。
  self-host 设计庞大，不要疲惫时硬塞，应在 fresh session 经验证 workflow 降险后构建。“stdlib 全可运行”= 建 self-host runtime（尚未）。

---

<a id="kgi-と-kpi-の関係運用ルール--この-1-行"></a>

## KGI 与 KPI 关系（运行规则只有这一行）

> **KGI = max（进攻类），约束为钉住全部防守类。** 破坏防守换来的进攻值不是加法，而是**减法**（侵蚀信任核心）。

因此每周**先于**看“进攻增长多少”，要看“防守是否仍全绿”。防守掉一项，该周就不是前进，而是**后退**。

---

<a id="週次記録最新を上に追記"></a>

## 每周记录（最新追加到上方）

<a id="2026-06-21--③実行-path-で実-repo-2本-full-conquestcsv--svg"></a>

### 2026-06-21 — ③ 执行 path 完全跑通 2 个真实 repo（csv + svg）
- **防守**：全绿保持。corpus-wall **每 commit ACCEPT**（4 性质：ownership 16556 / names 3850 / caps 3141 / caps-transitive 192，无 leak/double-free）。`cargo test -p almide-mir` green（加 7 个回归）。checker 规模、TB、公理纯度、主张漂移不变。静默通过仍 🔄。
  **纪律自捕获**：svg 一次字节一致提交后，**10⁴ leak loop 发现 OOM**（map.entries 结果 tuple-list flat drop）→ 立即根治（`DropListStrStr`），下一 commit 通过 gate。没有带 leak 发布（②纪律）。
- **进攻（③ 执行 path）**：**真实 repo 2 个 EXECUTE 且与 v0 字节一致**——`csv`（4/4 公开函数）+ `svg`（records renderer：rect/text/group/doc/nested children + `map.entries` 属性）。两者 10⁴ loop 无泄漏。**真实程序覆盖 1→真实 repo 2 个 full-conquest + 多个 fixture**。
  **语言覆盖**：subset → **call / 控制流 / heap-result if·match / closure（defunc inline）/ nested-heap list / records 全域 / `Map[String,String]` entries**。新机制：递归 record drop、record 参数 drop、List[Record] literal+concat、`(String,String)` tuple-list（`map_entries_str` / `DropListStrStr`）、defunc-map 内允许 self-recursion（`in_defunc_body`）、lower_bind UnOp arm（修复 `let hc = not <call>` 遗漏）。详情 [[v1-records-svg]](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/STATUS%206) / [[v1-parser-tco-lever]]。
- **性质覆盖仍 4/8**：leak-freedom **经验上 clean（10⁴ loop），但 Perceus 证明尚无**——已证明性质未增加（emit/执行层前进，不是证明）。Gap 1（byte 绑定）/ Gap 2（frees 证明）不变。
- **解读**：③“建立执行的新轴”**已到跑通 2 个真实 repo**（执行侧缩小与先行验证侧的差距）。剩余语言 gap = **Set / guards / 一般 Map（非 String 值）**。接下来 ④ MSR（支柱 A、未测量）与 Gap 1/2 证明轴，或增加对 Map/Set 依赖少的真实 repo conquest。不掉任何防守而前进。

<a id="2026-06-14--lower-被覆ほぼ天井--2-つの潜在-accept-but-unsafe-を発見封鎖"></a>

### 2026-06-14 — lower 覆盖接近天花板 + 发现并封堵 2 个潜在 accept-but-unsafe
- **防守**：全绿保持，且**静默通过（hole-hunt）前进**——验证 workflow（对抗 refute）**发现 2 个潜在 accept-but-unsafe，均封堵**：
  (1) **Try/Unwrap/Fan 提前 return wasm leak**——deferred-continue 证书平衡（no_leak），但 v0 wasm Err 路径 `return_` 跳过 Perceus 终端 rc_dec，泄漏存活 heap local。**修复 v0 codegen 核心**（emit_early_return_decs，不是 wall 而是真 bug 修复），经验确认无泄漏（100k×100KB err-loop 无 OOM 跑完）+ 无 double-free（260-file wasm corpus + wasm_gc/runtime/cross-target 全绿）。
  (2) **caps gate `mir<=ir` 抵消**——FnRef/ClosureCreate marker 过计数可能抵消 Computed/Method 遗漏的少计数，由 count_ir_calls 结构性封堵（当前不可达，by-construction 加固）。**健全性 GENUINELY 100% 保持**。
- **进攻**：**lower+verify 覆盖 4022→4083（97.3%）**。(a) 清小 gap（heap-tail Block / Map 插入 / nested tuple destructure / top-level let global / break scalar frame / loop/branch accumulator / Computed-effect-call）+ 根治上列提前 return leak 类、撤墙、−59 回收。性质覆盖**仍 4/8**（leak 修复在 emit 层而非证明）。**真实程序覆盖仍 1**（将 render 推到真实程序的 ③ 未开始）。
- **③ 范围实测订正（同日）**：验证链**已 ACCEPT call 与控制流**（`helper()` `im` / `if` `im` / `match` `ad|im` / `for` `ad`，gate 已有 two_functions/transitive_caps）。过时“render 是 subset、无 call”不适用于验证侧——checker + render pattern + V 先行。**③ 真 frontier 是“v1 忠实 EXECUTE 真实程序且与 v0 字节一致”**（当前 V 只核对 pattern，不执行；v0 是 reference=双 oracle until parity。lower 为 cert defer call，执行需新建 un-defer call 的 EXECUTION path）。详情已更新 Gap 3。
- **解读**：**数量轴（lower 覆盖）接近天花板**。剩余两个庞大新轴——**③ 建 v1 执行 path 达 v0 parity**（验证先行，建立执行的新轴）和 **④ MSR**（支柱 A、未测）。各自在 fresh session 集中构建。不掉任何防守而增长两轴，是下一战。

<a id="2026-06-13--基準点"></a>

### 2026-06-13 — 基准点
- **防守**：全绿（checker 小 / TB 已声明 / 公理纯 / CI green、跨版本 9.1.1+9.2）。主张漂移机器核对及静默通过（hole-hunt 干涸）未完成。**结构无损**——“整理”已对当前 subset 达成。
- **进攻**：4/8 性质、仅 eager、真实 1 程序、subset（无 call）、byte 绑定仍 trusted、frees 未开始、**MSR 零**。
- **解读**：要守的守住，待填内容薄。KGI 双轮（KGI-T 仅切片 / KGI-W 未测）均还在前方。下一战：不掉任何防守，增长进攻（尤其 **MSR** 与 **Gap 1 byte 绑定**）。

每周模板（仅显示，不自动输入）：
> ### YYYY-MM-DD
> - 防守：[是否全绿 / 若不变量下降立即记录 → 这是后退]
> - 进攻：[变化的进攻 KPI 及当前值]
> - 解读：[是否未降防守而进攻。下一步]

---

<a id="一言"></a>

## 一句话

守的是“**信任对象的小与纯**”（= 数百行 checker + 压至底层的 TB）。KGI 是“**每次 build 用这小信任证明全部真实生产，且机器能最准确编写**”的合取。**目前防守完美、进攻初期**——下一战是在不掉任何防守的情况下，增长进攻（MSR 与 Gap 1）。
