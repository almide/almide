原文 description 元数据：拟议的资格化证据映射和套件设计；本文不确立已获受理的监管 credit 或已资格化工具
<a id="flight-qualification--do-178cdo-330do-333-マッピング--資格化キットg-f5--g-f6"></a>

# 飞行资格化 — DO-178C/DO-330/DO-333 映射 + 资格化套件（G-F5 + G-F6）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](flight-qualification.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](flight-qualification.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](flight-qualification.zh-CN.md)

> 固定原文提交的非规范阅读快照；以原文为准，不自动同步。未独立执行、验证实现、检查证明或测量。历史命令与设计指令是资料，不是已执行的指令。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-qualification.md)

> 原文注记：全部标准目标映射及套件/客户流程是需专家评估受理的历史提案，不是法律或认证指导，也不证明取得资格化、DAL/TQL credit、可飞行产品或成本降低。原文本身区分已有技术资产、未证明 renderer/抽取接缝及当局受理；阅读较强的旧表及结尾销售/使用构想时须保留此边界。固定数量、版本是历史快照，不是当前库存。仅 Stdout 的 capability 模型不证明一般有界资源或全部 host effect；模型 RC/wasm 结果不确立应用正确性、完整翻译忠实性或 WCET。客户流程列 Ferrocene 路线，而原文明示等待合同的 stock-rustc lane，不证明已经使用该资格化路线。保留原命令名称，不主张等价，不执行。未独立验证标准解释、外部产品资格化、证明或测量。

> **目标**：[flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) 阶梯 **G-F5**（将 PCC 证书/receipt 映射到认证目标）与 **G-F6**（资格化套件产品定义，AbsInt/SCADE-KCG 模型）的具体设计。立足于 spine **现在实际产出的东西**。
> **相关**：[flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) §7.2 / [certification-grade](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certification-grade.md)（标准5机制、ALS、evidence ladder）/ [receipt-logic](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/receipt-logic.md)（主张分类）/ [certificate-format-v1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certificate-format-v1.md) / [flight-evidence-gaps](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-evidence-gaps.md)（实地审计发现 — 资格化主张当前界限）。

---

<a id="0-元の設計時点の技術資産スナップショット"></a>

## 0. 原设计时点的技术资产快照

以下固定数量和版本是原始计划值，不是当前库存。当前证明、测量及 trusted boundary 参见 `proofs/receipt.sh` 和生成的 `docs/TRUST-SPINE.md`；ALS 参见[规范规格](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/README.md)。技术资产存在与获当局受理的资格化是两回事，本文不证明后者。

- Coq **16个文件 / 45个定理**，全部 `Print Assumptions` = "Closed under the global context"（在全局上下文下闭合），`coqchk` De-Bruijn 复检，跨版本9.1.1 + 9.2（`proofs/check.sh`、`TRUSTED_BASE.md`）。
- 资格化对象 = `proofs/checker.ml`（当前1,144行 OCaml 抽取代码 — 实际行数由 `scripts/gen-claims.sh` 重新生成到 docs/TRUST-SPINE.md）← `OwnershipChecker.v`（309）+ `Subset.v`（88）+ `Extract.v`。核心 = 带符号 Δ 左 fold + `subset_check` 包含关系。
- `make verify-trust` = `check.sh` + `gate.sh` + `corpus-wall.sh` + `cargo test -p almide-mir`。`make receipt` = `receipt.sh`。CI 执行两者（`.github/workflows/trust-spine.yml`）。
- 性质覆盖 **4/8**（mem/name/cap/type ✅，leak/reuse/call-mode/byte 待做）。in-profile 4083/4195。
- receipt 主张 = C-PROVEN / C-SAFE / C-FAITHFUL / C-WALL / C-REPRO（`receipt.sh`、[receipt-logic](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/receipt-logic.md)）。

---

<a id="1-g-f51--do-178c-table-a-目標マッピング"></a>

## 1. G-F5.1 — DO-178C Table A 目标映射

以下是针对 G-F1 子集研究的**未获受理对应方案**，不是确立已取得 DAL-A credit 或标准解释正确性的表格。证书/receipt 是 **source-coding & integration + verification-of-verification 行**的验证过程证据，不是需求/功能正确性的证据。

<a id="専門レビューに付す対応候補"></a>

### 提交专家审查的对应候选

| Table | 目标（意译） | Almide 证据 | 强度 |
|---|---|---|---|
| **A-5 obj 6** | source code 准确、一致（stack/memory 管理、无无界使用） | `check_all_sound`（RC 平衡→no double-free/leak）、`balanced_cert_frees_in_memory`、`StackBalance`、`FreeList.alloc_not_live`、`CowSafety`。receipt **C-SAFE/C-PROVEN** | 强、形式化、每次构建 |
| **A-5 obj 1-2** | source 符合 LLR（部分：ALS 固定的 memory/ownership LLR） | `TypeConcretization`、`NameTotality`。ALS（`certification-grade.md` CG-1）提供 LLR | 部分（仅语言安全 LLR，不含应用功能 LLR） |
| **A-2 obj 5** | source code 开发（证书 = MIR 绑定的开发产物） | i/a/d/m/r/b witness + `Translation.v` op→wasm 表 | 辅助 |
| **A-6（集成）部分** | EOC 健壮 / target 上无 runtime error | `WasmExec`（实际字节执行 `rt_inc/dec`、double-free trap）+ sentinel。**仅 wasm target** | 部分、附 target 注释（Ferrocene pivot 移至可飞行 target） |
| **A-7 obj 3-4** | **验证的验证** — 步骤/结果正确，验证本身健全 | **最强主张**：soundness 定理 = 验证方法的验证。`coqchk` 独立复检 + 公理审计 + claim-drift gate | 强，Almide 的主场 |
| **A-7 obj 1-2** | HLR/LLR 测试覆盖 | 对应形式证据的方案（credit/替代可否未定，功能 LLR 不在内） | 仅经 DO-333 |

<a id="触れない目標正直なギャップder-に明言必須"></a>

### 不涉及的目标（诚实缺口，须向 DER 明示）

| 目标 | 原因 | 谁的工作 |
|---|---|---|
| A-3/A-4 HLR&LLR 正确性、完整性、可追溯性 | spine 证明安全性质，“计算正确控制律”另论。支柱③=零 | 客户领域工程 |
| A-7 obj 5-8 MC/DC、结构覆盖 | 覆盖是 *checker 的*性质，不是应用决策逻辑 | 客户（或子集的 DO-333 credit） |
| A-4 obj 13 WCET | `Termination.v` 为 loop-free，循环为 no-op。WCET=基石（あ），待做 | 未完成基石 |
| A-6 完整 EOC/target 测试 | wasm 已证明，但“飞机不靠 wasm 飞行” | Ferrocene pivot（G-F3） |
| A-1 PSAC/计划符合性 | 尚无认证计划 | G-F6 档案模板 |

**对外说明边界**：“Almide 提供显式模型上的检查器定理及 fixture 复验结果。可将其接受为何种认证目标的证据，需包含用途、配置、未证明部分的专家评估；此处不主张已获受理。”

---

<a id="2-g-f52--do-330-ツール資格化論証checker-を証明する"></a>

## 2. G-F5.2 — DO-330 工具资格化论证（“证明 checker”）

**提案**：整理独立检查编译器输出的配置证据，把什么应作为资格化对象交付评估。不能仅因存在检查器定理就将 compiler 降至 TQL-5，或称 checker“已通过形式证明完成资格化”。本 repo 不展示该当局受理。具体 TQL/Criteria/credit 判断视为未定。

**资格化证据包（均存在于 `proofs/`）**：
1. checker 小且可审计。核心 = 带符号 Δ 左 fold + `subset_check`。大小 ∝ #event 字符 + #subset + #表条目，**与程序/语言/编译器规模无关**（`certificate-format-v1.md` tripwire：禁止 checker 打开 callee / CFG walk / 推断）。
2. checker soundness 经机器证明（`check=accept ⟹ P`）：45个定理、`Qed`、0 `Admitted`。
3. **独立复检** `coqchk`（De Bruijn 标准、第二个小 kernel）。
4. **公理审计** `Print Assumptions` = closed，claim-drift gate 使公开账本不能漂移至 `coqchk` 验证之外。
5. **每次构建重新推导**：在客户机器上运行 `make verify-trust`。**CI 不在信任基内**（不是“相信我们的 CI”，而是“客户可重新推导”）。
6. **跨版本重新推导**（9.1.1 + 9.2）= 对 toolchain 版本的稳健性。

**DER 仍会质疑的点（提前明示）**：
- **witness ⟹ emitted-bytes 接缝依赖信任，尚未证明**。checker 验证 MIR-level witness；发出的 wasm/Rust 字节实现它们，是 §3 renderer 契约（`Translation.v` 为 presence 表匹配）。不能由此确定 checker 已资格化或 renderer 的 TQL。Rust/Ferrocene 方针不会自动证明 Almide→Rust 忠实性。`.github/workflows/ferrocene-lane.yml` 目前明示等待合同期间使用 stock rustc。
- **抽取受信任**：证明针对 Coq 项，运行的 checker 则是经 `ocamlopt` 的 OCaml（Thompson 漏洞）。以 CertiCoq+CompCert 封闭（未完成）。
- **检查实现的配置**：把独立性、共因故障、版本管理纳入评估资料，不凭一条标准泛论决定充分性。
- **无受理证据**：本仓库不证明 Almide 已获当局受理。不可仅凭此资料断言全球不存在先例或审查需要几年。
- **ALS 有效性是经验性的，never proven**：“形式语义是否捕捉意图”是不可约的底线。

---

<a id="3-g-f53--do-333-形式手法-credit-マッピング"></a>

## 3. G-F5.3 — DO-333 形式方法 credit 映射

以下为原始工作表，用于考虑健全性、与实现的对应、与需求的对应。DO-333 credit 受理、验证活动替代可否、标准条款满足均未定；每行有定理并不会使其自动成立。
| 考虑对应的 DO-178C 目标方案 | 已证明性质（文件） | J1 健全性 | J2 工具正确性 | J3 性质捕捉需求（弱点） |
|---|---|---|---|---|
| A-5 no double-free/leak | `check_all_sound`/`balanced_cert_frees_in_memory` | `accept ⟹ no_double_free ∧ no_leak`、axiom-clean | `coqchk`+`Print Assumptions`+drift gate | RC 平衡 = memory-safety LLR。✓ 子集上 |
| A-5 no dangling reference | `check_names_cert_sound` | 已证明 subset law | 同上 | name-totality = 全部使用的名称均有定义 |
| A-5 bounded resource/no undeclared effect | `check_caps_cert_sound`（`reachable⊆declared`、传递性） | 已证明 | 同上 | **诚实 scope：仅 Stdout**。其他 host effect 未建模 — 必须披露 |
| A-5 type soundness | `TypeConcretization` | 已证明 | 同上 | LLR = Unknown 类型不进入 codegen |
| A-6 EOC robustness（wasm） | `WasmExec`/`rc_dec_bytes_trap_on_zero` | 在实际字节上证明（基于 wat2wasm） | wat2wasm + wasmtime 差分 | 仅 rc 安全 op；功能 list-op 在 bootstrap runtime 中未证明 |

形式证据可在 **memory-safety/name/type/capability** 的显式模型范围内作为提交候选。能否作为认证 credit 受理另需判断。**不适用**：功能正确性（支柱③，无控制律形式规格）、WCET（`Termination.v` 中循环为 no-op，需基石（あ））、value-semantics 子集外（C-WALL 为边界：in-profile 外显式 `Unsupported`，不静默给予错误 credit）。DO-333 J3 纪律正是 receipt 逐主张的“scope(honest)”列 — receipt 是技术主张、依据、scope 的表，不自动成立 DO-333 符合性。

---

<a id="4-g-f6--資格化キット製品内容exist-vs-build"></a>

## 4. G-F6 — 资格化套件（产品内容：EXIST vs BUILD）

AbsInt/SCADE-KCG 模型：**卖套件，不是卖盒子。** 套件 = 将消除客户工具资格化成本的输入打包、版本化、签名。

| # | 套件产物 | 状态 | 所在 / 缺口 |
|---|---|---|---|
| 1 | checker 技术资产（不意味已资格化） | 技术资产存在 | `checker.ml` + `OwnershipChecker.v`/`Subset.v`/`Extract.v` + `build-checker.sh` |
| 2 | 证明 spine（16 `.v`/45个定理）+ 公理审计 + coqchk 复现 | ✅ EXISTS | `proofs/*.v`、`check.sh` |
| 3 | 验证 harness（1条命令全量重新推导） | ✅ EXISTS | `make verify-trust`、CI mirror |
| 4 | receipt（schema + sample）= 带类型主张包 | ✅ EXISTS | `make receipt`→`receipt.sh`、[receipt-logic](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/receipt-logic.md) |
| 5 | 信任基账本（toolchain pin + 不可约底线 + 已知限制） | ✅ EXISTS | `TRUSTED_BASE.md` |
| 6 | 证书格式规格（i/a/d/m/r/b + side-table + 大小不变量） | ✅ EXISTS | `certificate-format-v1.md` |
| 7 | corpus-wall 证据（实际语料上的 totality + accept⟹safe） | ✅ EXISTS | `corpus-wall.sh`、C-WALL |
| 8 | ALS 规范规格 | 规格存在，与资格化受理分开 | [ALS](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/README.md)，实现对应及 scope 见各章 |
| 9 | 追溯矩阵（ALS § ↔ C-NNN ↔ fixture） | ◑ PARTIAL | `contracts.toml` 双向 gate 存在；`spec="ALS §"` keying 为 CG-1 |
| 10 | 飞行 profile 规格（counted-loop/no-alloc/bounded，checker 强制） | 📐 DESIGNED | [flight-subset-spec](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-subset-spec.md)。基石（あ）未完成 |
| 11 | WCET-bound 证据（把循环提升到 Coq、alloc-count 上限） | ⬜ BUILD | 基石（あ）G-F2 |
| 12 | native 翻译及 toolchain 证据 | renderer 存在，完整翻译忠实性与资格化尚未确立 | `render_native.rs`、stock-rustc Ferrocene lane；与 G-F3 课题分开 |
| 13 | 档案模板（PSAC/SVP/SAS 骨架 → G-F5 目标表映射） | ⬜ BUILD | §1 映射为种子，`certification-grade.md` CG-5 |

**总结**：技术核心（1-7）**目前 EXISTS**，由 CI 强制。面向认证的 packaging（8-13）待构建 — 尤其 **13 档案模板是 §1 G-F5 映射的直接产物**。G-F6 仅凭“定义”即可从 🔴 翻至接近（无需全量构建）。

---

<a id="5-キット境界--顧客統合"></a>

## 5. 套件边界 + 客户集成

<a id="境界absintscade-kcg-アナロジー"></a>

### 边界（AbsInt/SCADE-KCG 类比）

这一产品边界是原设计的类比，本文没有确认 SCADE/AbsInt 资格化条件或 source review 豁免。不得推断 Almide 获准同等待遇。

| 拟提交评估的技术资产 | 需要另行判断、验证的范围 |
|---|---|
| checker 健全性证明及目标 witness/model 说明 | 工具资格化、适用 TQL、预期用途的受理未定 |
| mem/name/caps/type 的 scoped receipt 和 declared subset | source→artifact 对应、未证明 runtime/host、应用功能需求 |
| 验证 harness、追溯性、档案模板方案 | 实际目标配置、证据充分性以及客户/专家评估 |
| WCET 相关模型/测量方案（不主张完成） | 实机、环境前提，system-level timing/safety case |

套件是整理证据帮助评估的方案，不会无偿保证生成代码安全，不会仅凭 proof 使 checker 资格化，也不会自动把 compiler 改为 TQL-5。本文不确定当局受理及全球先例有无。

<a id="顧客の使い方"></a>

### 客户使用方式

1. **编写飞行 profile Almide** — 在飞行子集中编写控制律 kernel / 状态机 / watchdog。
2. **`make verify` 每次构建重新推导证书** — 在客户机器上（CI 在信任基外）。in-profile 外为显式 `Unsupported` wall，不是静默处理。
3. **render 到飞行 target** — MIR → 可读 Rust（`rust_pattern` 忠实）→ **Ferrocene**（ASIL-D/IEC 61508 资格化）执行 Rust→机器码，绕开 wasm 字节绑定 Gap 1。
4. **将 receipt + 套件档案纳入客户认证包** — PSAC 引用 DO-330 论证（§2）和 DO-333 正当化（§3）；receipt 成为 A-5/A-7 验证结果证据，账本成为客户工具资格化信任基披露。客户在自身流程完成 A-3/A-4、MC/DC、SSA。

**净结果**：Almide 是**资格化套件供应商**（Ferrocene/AdaCore/AbsInt 的位置），而非 avionics 集成商。价值 = **客户开始资格化工作前已有档案，从而降低资格化成本** — 把 Ferrocene FLS 做法应用于每次构建的 PCC 证书，而非一次性编译器资格化。
