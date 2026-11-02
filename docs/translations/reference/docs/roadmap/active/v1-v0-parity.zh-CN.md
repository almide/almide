说明：历史 incumbent-v0 对等计划，明确已测试和见证保证的限制
<a id="v1--v0-parity--the-completion-plan"></a>

# V1 → V0 对等——完成计划

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-v0-parity.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](v1-v0-parity.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](v1-v0-parity.zh-CN.md)

> 固定原文的非规范读者快照；原文仍为权威，不自动同步。未独立验证实现或执行引用指令。历史状态、计划及比较主张保留原范围。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-v0-parity.md)

> 原文注记：这是2026年7月历史完成计划，并非当前支持清单或出货批准。定位 A 的普遍正确性/字节一致主张已被原文引言明确订正；有限测试和限定 witness 不证明全部语义。B-3 记录 scalar libm exp 与7/7完成，保留的旧 D1 设计却假定 SIMD 及七个未完成函数；不推断调和旧顺序和数量。后续 closure 工作更新部分 backlog。医疗级、flight 质量是原设计目标，不是认证。未执行引用调试命令或下次会话指令，也未重测结果。


<a id="historical-completion-plan-and-evidence-correction"></a>

## 历史完成计划与证据订正

这份2026年7月计划比较当时的 MIR 路径与现已退役的 v0 路径。blocker 数量与下次会话指令属于历史，并非今天 structural emitter 的队列。后续 closure 工作见[closure architecture v2](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/closure-architecture-v2.md)。

下文 ship-gate 声称每个被接受的程序必然正确或字节相同，超出了证据范围。有限差分 corpus 与被接受的 ownership/name/capability witness 并不能证明全部程序语义。区别见[已证明与受信任](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/proven-vs-trusted.md)。保留记录中的成功和局部覆盖，不要推导普遍正确性，也不要从本计划重新打开全部旧 blocker。

> 2026-07-04 立项。使 v1（MIR trust-spine）达到 **v0 对等**的完成计划。
> 为通过 /goal 按 phase 推进，每阶段附**可测量 exit gate**和**精确恢复点**。
> 这是位于 v1-backlog.md（单项 bug 台账）之上的“完成”路线图。

<a id="v100-ship-gate出荷条件--100-北極星のサブセット"></a>

## v1.0.0 SHIP GATE（出货条件——100% 北极星的子集）

**“全部 wall 归0”是北极星，并非出货条件。** v1.0.0 如何定位，会改变门禁：

<a id="位置づけ-a-検証済みサブセットのコンパイラv100--今すぐ出荷可能"></a>

### 定位 A——“已验证子集的编译器”v1.0.0 → **现在即可出货**
v1 核心（honest wall）已成立：接受的程序必然正确，拒绝是 clean。已知 miscompile 为0，所有接受程序 native⇄wasm 字节相同 + PCC（ownership∧names∧caps）已证明，3-way oracle 全绿。wall 明确标为“尚不支持”，而非“bug”。在此承诺下可出货。

<a id="位置づけ-b-v0-の完全な置き換えv100--下の-14-が致命的ブロッカー"></a>

### 定位 B——“v0 的完全替代品”v1.0.0 → 下列 #1～4 是致命 blocker
| # | Blocker | 状态 |
|---|---|---|
| **B-1** | **动态 dispatch（Phase C）**——first-class closure + `.method()`（method/computed ~79 + heap-result match tail 74 的大部分）。决定“普通写法能否编译”的实用核心。**最大 blocker**。 | 尚未开始 |
| **B-2** | **derived Codec `.decode()`**（Camp-4 heap-Ok `?`-bind）。**进行中——精确诊断完成，有下次会话 handoff**。见下方“B-2 handoff”。 | value.field 已完成 / desugar 尚余 |
| **B-3** | **nn end-to-end（fast-exp 族）**。已发现 wasm oracle 并非 SIMD，而是 scalar libm exp（= 已 self-host 的 math.exp）。**7/7 全部打通、字节相同**（softmax_rows / gelu / swiglu_gate / rope_rotate / multi_head_attention / masked_multi_head_attention / from_q1_0_bytes）。nn matrix 栈完全 self-host，已做 byte 验证。**✅ 实质关闭**（剩余 nn unlinked 仅 fft 的 enumerate_h/zip_h 4 个辅助位置，非推理路径）。 | ✅ 7/7 |
| **B-4** | **native 目标**——v0-native matrix codegen 损坏（交接）。仅在 native 为出货目标时。 | 外部阻塞 |

**Phase E（caps 证明完全形）和 G（契约覆盖）并非 v1.0.0 必需**——是北极星（下述 DONE 标准）的一部分，而非出货门禁。推荐推进：**完成 B-3 → B-2 → B-1**（按接近完成程度及实用影响排序）。

<a id="完成の定義done-の判定基準--100-北極星"></a>

## 完成的定义（DONE 判定标准 = 100% 北极星）

v1 达到 v0 对等，指**同时**满足下列条件（均可用既有工具机械判定）：

1. **walls = 0**：`classify_corpus` 的 `walled real (lowering)` 为0（五处 structural native-FFI 仍排除）。spec 全4556函数 + org 全 corpus。
2. **output-parity 完整**：`proofs/output-parity.sh` 为 MISMATCH=0 / RUNERR=0，baseline 覆盖所有可运行 fixture（v0 可运行的所有程序字节相同）。
3. **PCC 不变**：`proofs/corpus-wall.sh` 对三属性（ownership ∧ names ∧ caps）维持 ACCEPT（每 phase 不得回归）。
4. **3-way oracle 全绿**：`wasm_runtime_interp_oracle::interp_cross_target_spec` 在所有 fixture 上 native==wasm==interp 达成共识（abstain 台账收敛到0）。
5. **org 两目标 byte-verify**：`scripts/org-trust-status.sh` 的 BYTE_VERIFIED 在全部 org 仓库的 native + wasm 均通过。
6. **native 目标恢复**：`almide run`（v0-native）通过包括 matrix 系的全部 spec。

> 当前点（2026-07-04 会话后）：spec in-profile **4374/4556（96%）**，实际 wall **469**，nn wall **0**，parity baseline 180，PCC ACCEPT（三属性），3-way 全绿。
> 已知 miscompile 为0（仅 honest wall）。
> **会话成果**：Phase A 完成（+38 in-profile，guard miscompile 从 wall 变成正确 lower）＋ Phase B 主要桶推进（Option[heap] 字段 match / let 绑定 custom-variant match / List[record-ctor variant] 字面量——合计 +14 in-profile）。实际 wall **520→469**。
> **剩余最大杠杆是 Phase C（method/computed dispatch）**：derived Codec 的 `.decode()`/`.encode()` 方法与 first-class closure，占 heap-result match tail 74、method/computed 79 sites 的大部分。roadmap 标为“大”的架构领域。

<a id="-phase-a-完了2026-07-04"></a>

## ✅ Phase A 完成（2026-07-04）

`desugar_guard`（mod_p6）：bottom-up 递归将 `guard cond else E; rest` 结构化为 `if cond then { rest } else E`。打通**函数提前 return**（`else err(…)` 进入 tail heap/scalar-result-if）与**循环 continue**（`else continue` → `else ()`，去掉 continue 节点，scalar 循环获接受）。break 暂仍 wall（需要真正提前退出循环）。由于 call-count-invariant，保持 `mir == ir`；count 端（classify_corpus）也应用 desugar。guard-else 桶 **39→0**，in-profile **4322→4360**。旧“wall”pin 更新为“desugar 触发”验证，并增加 end-to-end pin。

<a id="phase-b-進行中--heap-result-の位置網羅"></a>

## Phase B 进行中——heap-result 的位置覆盖

- ✅ **以 Option[heap] / Result[heap] 字段为 subject 的 match**（`match u.email { some(e) => "…${e}…", none => u.name }`）：跟踪 borrowed 字段的 variant handle（materialized_options/results + heap_elem_lists），执行 heap-payload borrow bind。
- ✅ **let 绑定 custom-variant heap-result match**（`let nm = match s.shape { Circle(_) =>
  "circle", … }; "${nm}…"`）：原先 `desugar_match_to_if` 只处理 literal 因而拒绝，现在通过 `wrap_match_arms`（将 continuation 压入每个 arm 并保留 Match）打通。桶 **17→2**。
- ✅ **List[record-ctor variant] 字面量**（`[Click { x, y }, KeyPress { key }, Close]`）：在 list-literal builder 元素 gate 添加 `IrExprKind::Record { name: Some(ctor) }`，通过 `try_lower_variant_ctor` 材料化。桶 **29→24**。
- 剩余子例：heap-result match tail（74，大多为 `.decode()` 方法 = Phase C）、default-field variant ctor 绑定（省略 Group 的 `items = []`）、复合插值（Map/异构 tuple 的 `${…}`——Phase D self-host）。

<a id="phase-a設計メモ原文保持-制御フローの完成言語機能の唯一の欠落"></a>

## Phase A（设计笔记，保留原文）——完成控制流（唯一缺失语言功能）

**A1. early-return / guard-else 建模**（39 sites）。
将 `guard cond else E; rest` 结构化为 `if cond then { rest } else { E }`（复用 block 尾 heap-result-if 机制，将其余 statement 折叠进 then 枝）。`return` 表达式也走同路径。
- **exit gate**：`guard-else early return` 桶 → 0。guarded `Result[String,String]` ok/err tail（当前 backlog T4-7 搁置）也自动打通。
- **恢复点**：将 `lower_stmt` 的 `IrStmtKind::Guard`（当前 honest wall）和 `lower_body_into` statement 循环，改为发现 Guard 即 then/else split。
- **权重**：中～大（core lowering 结构变更）。**尤其注意 parity 回归**。

<a id="phase-b--heap-result-の位置網羅最大バケット-130-sites"></a>

## Phase B——heap-result 位置覆盖（最大桶 ~130 sites）

v0 可在“所有位置”返回 heap-result match/if/lambda。v1 按位置增加 builder，仍有缺口。

- **B1** `heap-result match` 的 tail / let 绑定（77 + 17）。
- **B2** `heap-result if/lambda` 剩余位置（9）。
- **B3** call-arg 位置的 `List[heap] literal`（29）+ `string interpolation`（30）。
- **exit gate**：上述桶 → 0。
- **恢复点**：`tail.rs` / `control_p4.rs` 的 heap-result-arm builder，`calls_p2.rs` 的参数材料化。沿本次会话填补 record-ctor / ArrV consumer 的工作继续。
- **权重**：中（扩展既有机制，模式已确立）。

<a id="phase-c--動的ディスパッチfirst-class-クロージャ"></a>

## Phase C——动态 dispatch（first-class closure）

v0 通过 closure conversion 全支持；v1 仅支持能 inline-defunctionalize 的范围。

- **C1** `method/computed heap call`（61 + 11）——对传来传去的 closure 做 opaque 调用。
- **C2** 返回 `heap-result Lambda`（9）。
- **exit gate**：closure-passing 程序可以 lower（`unresolvable method/computed` 桶 → 0）。
- **恢复点**：扩展 `funcref_value_of` / `CallIndirect` 路径，材料化 closure env 的 value-model。衔接 docs/roadmap/done/closure-architecture-v2.md。
- **权重**：大（将 closure env 所有权纳入 PCC 的设计）。

<a id="phase-d--stdlib-self-host-の完成"></a>

## Phase D——完成 stdlib self-host

未 self-host = wall。当前211个 `.almd` /171已注册。剩余缺口：

- **D1 fast-exp 族7个**（`softmax_rows` / `gelu` / `swiglu_gate` / `multi_head_attention` ×2 / `rope_rotate` / `from_q1_0_bytes`）。**nn end-to-end 直接 blocker**。v0 对 almide-kernel SIMD fast-exp **按 lane 顺序求和**，所以 bit-exact 转写需要 exp_pd_{wasm,neon} 多项式（Horner 六项）+ 两 lane 部分和 + 奇数尾部 libm exp。**医疗级：先搭建 ULP 差分测试再动手**，不可随手做。参考实现 `crates/almide-kernel/src/silu.rs::exp_pd_neon`，先例 `stdlib/math_exp.almd`。
- **D2 effectful brick 群**（`fan.*` 并发 / `http.*` / `net.*` / `datetime.parse_iso` / `env.os` / `env.temp_dir` / `fs.stat` / `process.env`）。各自需要 WASI prim + capability 声明 + admitted-effectful 注册。从可按 fs.read_bytes（本会话）方式打通者开始。fan 非确定性需要另行决定如何纳入 capability 证明（→ Phase E）。
- **D3 剩余 pure combinator**（`list.zip`/`enumerate` 的 rich 元素版、Map/Set 未支持 repr 等）。按 ROI 顺序。
- **exit gate**：nn unlinked → 0（D1 为7、D3 为5）。effectful-27 集合可工作。
- **权重**：D1 大（精度），D2 中×n（每 brick），D3 小～中。

<a id="phase-e--capability--effect-の完成"></a>

## Phase E——完成 capability / effect

- 设计如何将 `fan.*`（并发、非确定）纳入 caps 证明模型。确定 `http`/`net`/`process` capability 类别。吸收 effectful-27-blueprint.md。
- **exit gate**：effectful stdlib wall（`needs a declared capability`）→ 0。
- **权重**：大（扩展证明机制）。

<a id="phase-f--native-ターゲット-parity"></a>

## Phase F——native 目标 parity

- **F1（交接）修复 v0-native matrix codegen 损坏**：`bridge::AlmideMatrix` enum 化与 vendored glue（旧 Vec API）不一致。处于 flat-ABI/burn 重构过渡态。**交接给负责会话**，或决定纳入本 roadmap。
- **F2 将 v1 MIR→Rust native emit 生产化**（flight-profile 关键支柱）。
- **exit gate**：`almide run` 通过包含 matrix 的全部 spec，native==wasm 字节一致。
- **权重**：F1 中（修复既有损坏），F2 大（flight 质量基础）。

<a id="phase-g--契約計測の-ratchet"></a>

## Phase G——契约、测量 ratchet

- 建立 matrix.mul（k 升序）/ from_bytes OOB（全零矩阵）契约台账（**以 F1 完成为前提**——没有 native oracle 无法 byte-verify）。
- coverage 达到目标（control_p5 defunc engine 的43%是最大未覆盖区域）。
- **exit gate**：新增 C-NNN + fixture，`check-contracts.sh` 通过。

<a id="推奨着手順依存--レバレッジ"></a>

## 推荐开工顺序（依赖 + 杠杆）

**A → B → D1 → C → D2/D3 → E → F → G**。

- **A（early-return）**是唯一语言功能缺口，也是 B/D tail 材料化的前提，故最先。
- **B**模式已确立、数量最大 → 可早期大幅减少 wall。
- **D1（fast-exp）**是 nn end-to-end 唯一 blocker，伴随精度测试准备，作为独立线路推进（不等其他 phase）。
- **C（closure）**是架构大山之一，放 B 后。
- **F1**修复 v0 损坏，是 G 前提。可依并行会话情况前后调整。

<a id="goal-の切り方提案"></a>

## /goal 如何拆分（提案）

按 phase 设定 goal 易于推进。例如：
- “完成 Phase A（guard-else early return），让 `guard-else` 桶归0。维持全部 gate 全绿。”
- “Phase B heap-result match 覆盖（tail/let/arg），让三个桶归0。”
- “带 ULP 差分测试 self-host Phase D1 七个 fast-exp，使 nn unlinked 减7。”

每个 goal 可通过上述 exit gate（`classify_corpus` 对应桶 count + 横断测试组全绿）机械确认完成。

<a id="b-2-handoff次セッション再開点--2026-07-05"></a>

## B-2 handoff（下次会话恢复点——2026-07-05）

<a id="状況"></a>

### 状况
- ✅ `value.field` 已 self-host（Object tag 检查、字节相同、已 commit）。
- ✅ **下游 lowering 正确**：`effect fn dec(v) = { let fv=value.field(v,k)!; let x=value.as_T(fv)!; …; ok(R{…}) }`（分开 bind 形态），**multi-field 也 valid wasm、字节相同**（scratch：man5/man6 的 `dec`）。
- ❌ **derive 的 `Basic.decode`（plain fn，`?`）为 invalid wasm**（`type mismatch: expected i32, found i64`）。

<a id="精密診断ここが肝"></a>

### 精确诊断（关键）
调试用 **desugared-IR dump 已实现**（已 commit，env gate）：
```
ALMIDE_DBG_DESUGAR_FN=<fn名> [ALMIDE_DBG_DESUGAR_RAW=1] almide/render_program <file>   # eprintln に desugared IR
```
原命令中的日文占位符和注释意为“函数名”和“向 eprintln 输出 desugared IR”。
`crates/almide-mir/src/lower/desugar.rs::dump_ir` / `dump_desugared_ir`，调用方为 `lower_function_all_impl`（mod.rs）。

**发现**：man6（同一文件含 `type Basic: Codec` 与手写 effect-fn `dec`）中，`dec`（valid）与 `Basic.decode`（invalid）的 **desugared IR 除 VarId 编号外完全相同**（经 `ALMIDE_DBG_DESUGAR_RAW` diff 确认）。但 **MIR 不同**（`Basic.decode` 多两个 local，v91 附近 i32/i64 互换——可由 wat local 声明确认）。

→ **相同 desugared IR 被 lower 成不同 MIR**。差别在 fn 层：
  - `dec` 为 `is_effect: true`，`Basic.decode` 为 `is_effect: false`（plain fn）。
  - derive 参数 `_v: Value` 为 `ParamBorrow::Own`（手写为 borrow）。但现有 `bind_params`（mod_p3）不看 `ParamBorrow`，将全部 heap param 当 borrow → 单凭这一点应不会产生差别。
  - **实际 lowering 路径是 `desugar_heap_branches(func.body)` → TCO → `lower_body_into`**，与 dump 的 `desugar_all` 入口不同。**接下来怀疑这里**：pre_tco 的 `desugar_heap_branches` 为两函数生成不同中间形态；dump `lower_body_into` 实际 desugar 后 body，与 `desugar_all` 版比较。

<a id="次の一手順に"></a>

### 下一步（依次）
1. **dump 实际 lowering 路径 body**：在 `lower_body_into` 入口（desugar 完成后）输出相同 `dump_ir`，diff `dec` 与 `Basic.decode`。既有 dump 走 `desugar_all`，应该能在此看到与 pre_tco 路径的差别。
2. 正确 desugar-lift 形态为 **man5/man6 `dec` 的分开 bind nested-match**（`subj: vN`（已 lift），而非 `subj: Call(value.as_int, [Try(...)])`）。derive 也生成它。上次尝试（take_or_recurse 识别 Try + 外层 unwrap 递归 + callarg-unwrap 先于 effect-unwrap）打通 single/effect，**仅 multi-field plain-fn `?` invalid wasm**（已 revert，见 git 历史 diff）。先由 #1 排除“相同 desugared 却不同 MIR”的原因，再重新应用。
3. **严守 honest-wall**：绝不 commit invalid wasm。每次尝试必须经 `render + wasmtime` 验证 valid。
