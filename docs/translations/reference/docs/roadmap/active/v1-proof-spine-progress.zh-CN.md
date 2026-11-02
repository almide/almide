<!-- description: v1 proof-spine 进展 — task #31（全V / leak-freedom / 传递 caps / 抽取缺口）哪些已证明、哪些仍待完成。记录 CapabilityReach.v 的传递 caps 定理（2026-06-21）。 -->
<a id="v1-proof-spine--31-progress-全v--leak-freedom--推移的caps--抽出穴"></a>

# v1 Proof Spine — #31 进展（全V / leak-freedom / 传递 caps / 抽取缺口）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-proof-spine-progress.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](v1-proof-spine-progress.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](v1-proof-spine-progress.zh-CN.md)

> 非规范性历史读者快照；固定原本不变且为权威。不自动同步，未新运行证明、runtime、测量或 CI。进度层及过时实现说明保留为历史。cert／模型级证明及限定范围实验检查不是普遍成果物安全保证或航空认证。确定性 belt 原文已声明 v0 type-state 边界退役；较强旧引语及稳定排序／输入顺序主张保留为历史断言，而非新确立保证。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-proof-spine-progress.md)

spine 已成熟：`proofs/check.sh` 用 `coqc` 构建全部定理，用 `coqchk` **独立复核**（De Bruijn 准则），审计公理（每个定理的 `Print Assumptions` = *Closed under the global context*），并通过 claim-drift gate 强制公开主张 ⊆ 已证明范围。2026-06-21 审计的 task #31 四个具名缺口状态：

<a id="-leak-freedom--already-proven"></a>

## ✅ leak-freedom — 已证明
`OwnershipChecker.check_sound : check ops = true -> no_double_free ops /\ no_leak ops`，其中 `no_leak ops := run ops = Some 0`（每个获取的引用均释放）。抽取的端到端 `check_cert_sound` 携带该性质，`OwnershipLoop.v` 将其扩展到 loop-carried slot（任意迭代次数均无 leak/double-free）。因此 cert 层面的 kernel 基础包含 leak-freedom。（自托管递归 drop **例程**——`__drop_value`、`__vdrop_obj`、生成的 `DropListStr`/`DropListListStr` loop——生成 cert 所见的单个平衡 `d`；其**内部**逐元素正确性是受信任底层，由 leak-loop 验证。）

<a id="-抽出穴-extraction-hole--closed"></a>

## ✅ 抽取缺口（extraction hole）— 已关闭
`checker.ml` **从已证明的 `check_cert` 抽取**（Extract.v），`gate.sh` 据此构建 checker。tokenizer 现位于证明**内部**（`check_cert` 解析 bytes 并检查），所以整个“cert bytes ⟶ accept/reject”pipeline 就是抽取的已证明函数，没有手写 checker 偏离。

<a id="-推移的caps--theorem-now-proven-capabilityreachv-commit-a28ccf8a"></a>

## ✅ 传递 caps — 定理现已证明（CapabilityReach.v，commit a28ccf8a）
`CapabilityBound.v` 已证明**逐函数**检查（直接使用 caps ⊆ 声明）。**传递**可达性（函数可达其 callee 的 caps，即“即使经 helper 也无 network”）此前仅由**不受信任的** Rust classifier（corpus-wall 的 `reaches_capability_or_unknown` fold）计算。**`CapabilityReach.reaches_sound`** 现证明支撑它的组合：
- model：program = 函数 node `{fallowed; fdirect; fcallees}`（callee = index）；
- `reaches` = 直接 caps ∪ ⋃ callee 的传递 caps（fuel 有界）；
- `fn_ok`（逐函数＋逐 edge）：`direct ⊆ allowed` 且每个 callee 满足 `allowed(g) ⊆ allowed(f)`；
- **定理**：`prog_ok prog -> ∀ fuel i, reaches prog fuel i ⊆ allowed(i)`。
基于 `Subset.v` 的共享规律，无新增公理；纳入 `check.sh` 的 coqc + coqchk；非空洞 demo（调用 helper 的 main 被接受且可达范围受限；helper 到达未声明 network 被拒）。

端到端接线 — 完成（commits be98af34、1485a2bc、a774500d）：fold 现位于证明内，每次 build 使用。
- `check_prog_cert`（CapabilityReach）在证明**内部**解析 call-graph witness（函数以 `;` 分隔，各为 `declared|direct|callee-indices`）；`prog_within` **计算**传递可达性，并逐函数检查 `reach ⊆ declared`。健全性为 `check_prog_cert_sound`，无新增公理。（`prog_ok`/`reaches_sound` 的逐 edge 组合律保留为单独结果；gate 使用直接 `prog_within`，允许 callee 过度声明。）
- 抽取至 OCaml（`Extract.v` → `checker.ml`），加 `caps-transitive` driver mode；`build-checker.sh` 演示有界 graph ACCEPT 及未声明 network REJECT。
- `program_cap_graph_witness`（certificate.rs）从 MIR **输出** graph；未知／跨 file／省略的 callee 路由至 sentinel UNIVERSE node，任何可达它的 caller 均**拒绝**（保守处理，如今由证明决定）。已单元测试，并经 `./checker` 端到端验证。
- `classify_corpus` 写 `caps_graph.cert`（每个完全可分析且在边界内的 file 一行）；`corpus-wall.sh` 对其运行 `check_prog_cert`。**整个 v0 corpus 的 gate 为绿：198 个 program witness ACCEPT**，并列于 ownership/names/caps。因此完全可分析 program 的可达 fold 由 kernel 已证明 checker 重新推导，而非信任 Rust。

剩余（较小）：部分可分析 file 仍用逐函数 `caps.cert`（可达性由 Rust fold 计算）。把 graph witness 扩展到它们，需要将 UNIVERSE 污染视为 gate 诚实范围内的“未验证”，而不是硬拒绝（策略细化）。

<a id="全v--cert-level-complete-40-byte-binding-now-grounded-on-both-sides"></a>

## 全V — cert 层面完成；#40 byte-binding 两侧均已取得依据
所谓 flight-grade 性质均在 cert 层面证明：ownership（double-free + leak）、names（无 dangling）、caps（逐函数＋传递组合）、stack balance、termination、COW safety、loop ownership，以及 rc_inc/rc_dec 的 **byte-binding**。

<a id="40-byte-binding--encoder--executor-both-grounded-commit-9a82b61d"></a>

## #40 byte-binding — ENCODER 与 EXECUTOR 均取得依据（commit 9a82b61d）
rc byte-binding 有两个 model 与现实的缺口；两者现均对照真实工具取得依据，`check.sh` 每次 build 复查：
- **ENCODER**（`WasmEncode.v`：指令树 → bytes，`encode_body = rc_inc_bytes`）：由 `check-wasm-bytes.sh` 支撑，wat2wasm 将 rc primitive 重新组装成**相同** bytes。
- **EXECUTOR**（`WasmExec.v`：证明 bytes memory effect 的专用 interpreter `run_g`）：现由 `check-wasm-exec.sh` 支撑。wasmtime 执行**相同** bytes，得到与证明**完全相同**的 effect：rc_inc 4→5；rc_dec rc=1 释放到0并回收到 `$freelist`（leak-freedom 回收）；rc_dec rc=0 **TRAP**（double-free sentinel）。所以对 rc op，`run_g` 忠实于真实 engine，byte **执行**绑定不是循环论证。

<a id="40-wasmcert-coq-port--the-architecture-brought-in-tree-wasmisav-commit-6f2dfda8"></a>

## #40 WasmCert-Coq 移植 — 架构纳入 tree（WasmIsa.v，commit 6f2dfda8）
此处无法实际导入外部 WasmCert-Coq library（无 opam；library 面向旧 Coq 而非 Rocq 9.1）。它带来的是架构——关系式 ISA **规范**以及被证明精化该规范的可执行 interpreter——现在我们已原生具备：
- `istep`：关系式小步语义，按 wasm-spec 风格每 opcode 一条规则（受信任 ISA **规范**），`irun` 在指令列表上顺序组合它。
- `estep`/`erun`：被证明**精化**规范的可执行 evaluator：`erun_sound`（每个结果是真实 reduction）、`erun_complete`、`istep_det`（确定性）。
- `rc_inc` 的 effect（cell → cell+1）通过精化，**经该关系**证明。
这以 rc subset 的真实语义关系，替代 WasmExec.v 中无规范可精化的专用 `run_g`。无新增公理，在 `check.sh`（coqc + coqchk）内。

结构化控制 — 完成（commit e1d2112c）：`istep`/`irun` 现覆盖 `IIf`（block `if/end`，运行本体）与 `IUnreachable`（trap 是 stuck 状态，无规则）。Coq 的结构 guard 拒绝嵌套单一／相互 fixpoint（list-inside-instr），因此可执行 `erun` 为 **fuel 有界**，经 fuel 归纳证明相对关系**健全**。**rc_dec 经该关系传递**：
- `rc_dec_isa_traps_on_zero`：double-free 从不完成（对每个 fuel，interpreter 均返回 None，sentinel 触发），安全性方向。
- `rc_dec_isa_frees_when_one`：rc=1 ⟹ 减至0并回收到 `$freelist`（leak-freedom），在规范中可达（经 `erun_sound`）。
于是 `check-wasm-exec.sh` **实验性**支撑的 double-free trap 与回收，也成为关系式 ISA **定理**。

FORALL 升级 — 完成，每个 rc 定理现为 `forall`／关系式（commits 8aaaba53、cf438a37）：
- `isa_det`：ISA 关系**确定**（config 至多一步到一个后继；组合相互归纳）。故 program reduction 的最终状态**唯一**。
- `erun_complete`：irun ⟹ ∃ fuel, erun = Some（组合归纳；`erun_S_cons` 单步展开保持内层 `erun` 折叠，配合 `erun_mono_add` fuel 单调性，将 IIf 本体／continuation 对齐到共同 fuel——解决 `cbn` 展开 `erun` 的障碍）。
- `rc_inc_isa_effect`、`rc_dec_isa_frees_when_one`：**`forall`**（“每个 reduction 均达到该 effect”）。interpreter 计算一个（`erun_sound`），`isa_det` 令其为唯一那个。
- `rc_dec_isa_traps_rel`：**`~ irun`**，没有 double-free reduction 能完成（规范自身 stuck），由 `erun_complete` 加可执行侧 `forall`-fuel trap 得出。即关系式安全定理。
所以 `erun` 是**精化**规范的已验证 interpreter（健全＋完备＋确定），三个 rc 事实（inc effect、释放／回收、double-free trap）对**每个 reduction**成立。无新增公理。

剩余重型路线（#40 最大形式）：完整正典 WasmCert-Coq ISA（完整已验证 wasm 执行语义），使绑定抽象地对**全部 wasm program**成立，而非只针对 rc subset。需要数周 library 移植，proofs 将其标为“推迟的重型路线”。**实用** byte-binding（生成 rc bytes ↔ 实际 encode ↔ 实际 execute）已闭合并有依据；WasmIsa.v 现为 rc subset 提供由规范关系支撑的语义，是完整移植将扩展的架构基础。
