<a id="proven-vs-trusted--the-boundary-map"></a>

# 已证明与受信任 — 边界图

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](proven-vs-trusted.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](proven-vs-trusted.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](proven-vs-trusted.zh-CN.md)

> 非规范性固定原文读者快照，未改动原本为权威。不自动同步，未独立重跑证明、gate或benchmark。区分model性质证明、有限fixture检查、受信实现接缝。formal-credit/DO333形状文档不表示已认证或完成tool qualification。退役MIR pipeline不等于出货structural字节pipeline；记录纪律、未model化heap access、Rust/Lean编译及host行为仍受所述边界限制。未独立更新带日期计数或实现描述。原文称桥为三层却列四种地位，保留而不暗中重编号。命令只是复现参考，不是本翻译的执行指令。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/proven-vs-trusted.md)

审计带证明的编译器，首先问的是*哪部分已证明*。本页回答并指向各项证据。

完整台账——toolchain pin、不可约信任根、篡改演练、每bug类别regression pin——在 [`proofs/TRUSTED_BASE.md`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/TRUSTED_BASE.md)。本页是地图，彼页是实地。

同一边界按objective组织的声明——每proof family作为带假设和coverage界限的formal-credit主张（#575，DO-333形状）——见 [`proofs/FORMAL-CREDIT.md`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/FORMAL-CREDIT.md)。

<a id="the-one-sentence-version"></a>

## 一句话版本

**被接受的certificate确立其checker所model化的性质**，限定于该witness的coverage与假设，例如RC balance、name totality、capability bound。这不是整个artifact内存安全证明；记录过程和未model化access仍是信任边界。它**完全不证明lowering选对语义**：certified函数仍可打印错误字符串。

<a id="which-leg-is-this-page-about"></a>

## 本页讨论哪个路径

自v0.60.0（#1599）曾有两条wasm路径。下方图描述**MIR certificate pipeline**，即旧v1曾交付的pipeline。#2752后无route到达该路径，#2761删除renderer，所以**出货wasm字节不来自该pipeline**。现在这些行描述proofs/gate.sh对MIR的witness（#4011后 `almide verify` 改判structural bundle），不是对运行字节重检查的性质。Coq定理 `WasmEncode.v` / `WasmExec.v` / `WasmRcDec.v` 针对*model化*runtime，即退役旧runtime。

`--target wasm` 路径，即唯一wasm renderer `crates/almide-wasm` 的structural emitter，在emit时记录自身certificate（#1696，已关闭）：出货stock-WASI字节的每frame ownership witness、call mode、name totality、capability，以及每个 `xs[i]` / `b[i]` read/store的 `bounds` witness（#4005）。`proofs/structural-wall.sh` 用extracted checker、Rocq kernel、`almide-verify` 判定每certified fixture bundle，三者一致，并pin `almide build --target wasm` 写出文件SHA-256。下方行不适用于它；它信任的是记录event确实是字节内instruction（记录纪律），以及index operator之外每个heap access。按 `MemorySafe` 各条款的proven / 每build checked / trusted划分见 [docs/design/MEMORY-SAFETY.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/MEMORY-SAFETY.md#the-gap-table-2026-10-24-4011) gap表。其余证据是差分与CI gate：整个grow-only `spec/wasm_cross` corpus上同native/interpreter的字节一致；emitter semantic mutation net；每fixture alloc/size台账；拒绝ratchet（拒绝形状须继续拒绝，静默重新emit是#1688回归类别）；`proofs/check-structural-bytes.sh` 用Coq decoder model `StructuralDecode.v` 为structural runtime字节提供依据。

<a id="where-the-boundary-runs-the-mir-certificate-pipeline"></a>

## 边界所在（MIR certificate pipeline）

| Stage | 状态 | 依据 |
|---|---|---|
| lexer、parser | trusted | 差分fuzz、spec corpus |
| MIR witness checker（ownership、类型具体化、name） | **其model化性质已证明** | `proofs/OwnershipChecker.v`、`TypeConcretization.v`、`NameTotality.v`；不是整个frontend类型checker证明 |
| coown pattern | **已证明** | `proofs/CoownCompose.v`、`CoownLoop.v`、`CowSafety.v` |
| AST → IR lowering | trusted | checker的 `TypeMap` 为权威，974文件emit baseline |
| IR → MIR lowering | **trusted** | ← *F3（#777）所讨论缺口* |
| MIR ownership witness | **已证明可重检查** | `proofs/gate.sh`：不受信producer输出witness，kernel证明的checker重验 |
| `almide-verify`（binary发行运行的checker，#2152） | trusted，一致性gate而非证明 | 独立版本的五个Coq checker之Rust转写，不链接compiler crate；`proofs/gate.sh` 用全部行及带seed随机差分约束其同extracted checker判定，`proofs/corpus-wall.sh` 覆盖全corpus witness |
| MIR → wasm字节 | 退役（#2761） | 旧renderer删除；`WasmEncode.v` / `WasmExec.v` / `WasmRcDec.v` 现在针对其model runtime。出货structural runtime字节：`proofs/check-structural-bytes.sh` 对 `StructuralDecode.v` |
| wasmtime | 未qualification的tool | 构造上不属范围 |

<a id="why-the-ir--mir-row-is-the-one-that-matters"></a>

## IR → MIR为何是关键行

2026-07-03审计发现的五个破坏输出bug都在该行，而且五个都*被certificate接受*：MIR满足RC balance、name可解析，只是计算了别的东西。

它们共有如下形状，引自trusted-base台账：

> 信任区域内producer与consumer之间的REGISTRY或CONVENTION（跟踪集合、layout表、调用约定、scalar deferred-Const fallback）发生漂移。

certificate抓不到，因为漂移产生*valid* MIR。能抓到的是lowering输出上的post-pass，检查lowering应维持的不变量，与 `assert_names_resolvable`、`ConcretizeTypes` gate同型。该gate现已存在（`crates/almide-mir/src/mir_wellformed.rs`，#777项2）：逐函数op stream检查def-before-use，对每真实op断言defines/reads划分分割 `op_values`（防止三个occurrence函数漂移），违反表现为named wall而非render。它在 `lower_function_all` 内运行，所以所有做lowering的CI路径（Test WASM、Cross-Target、Trust Spine）每build都会执行。

五类regression pin见 [`proofs/TRUSTED_BASE.md`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/TRUSTED_BASE.md#the-five-2026-07-03-trusted-zone-bug-classes-and-their-regression-pins)。

gate交付后该行仍出现问题。#1537（2026-08，真实下游project报告）又一例：`desugar_guard` 嵌套重写在Unit型statement-`if` 下留下Result型arm，branch lowering的deferred-value fallthrough静默丢early return；guard `err` 未传播，函数以跳过guard的状态返回 `ok`。MIR仍valid、RC-balanced、name-total、certified。def-before-use看不到，因为没有未解析内容，是良构错误值在流动。修复又加同类post-pass named invariant（unit型arm内Result/Option tail的strict-mode wall，`crates/almide-mir/src/lower/control.rs`），完整支持由#1527 wall burndown跟踪。诚实的解读是：它每几个月产生一个新的“接受但错误”实例，每次用新invariant消除，所以地图保持**trusted**；能改变分类的是平静的一季度，不是gate数量。

<a id="the-edit-locality-kernel-seam-stage-3"></a>

## edit-locality kernel接缝（Stage 3）

λ_almd kernel（`crates/almide-edit-belt`，第三个0-sorry Lean belt）为语言core提供机器检查参考语义：`ev_agree`/`edit_frame`/`ev_det`（含确定性的L1）、`typing_modular`、`pure_silent`、`eval_sound`（可执行求值器与关系一致）。通往出货compiler的桥经过地位不同的三层：

- **kernel检查（proven）**：手写family观测。`k1_obs`…`kAll_obs` 和 `corpus_total` 是 `:= by rfl` 证明，即Lean KERNEL reduction；公理集以 `#guard_msgs in #print axioms` 固定（仅 `propext`）。`eval_sound` + `ev_det` 把每个pin的 `evalE` 输出提升为唯一 `Ev` 导出（`kAll_ev`）。生成corpus由kernel检查的是**全域性**：48程序每个都求值到 `some` 观测（`corpus_total`）。
- **求值器pin（trusted）**：`proofs/kernel-conformance/*.expected` 的**值**由COMPILED Lean求值器（`lake exe conformancegen --write`）生成，信任它意味着信任Lean compiler，即lean4自身用 `trustCompiler` 公理指明的接缝。`evalE` 未证明的完备方向也不是doc注释而是可枚举对象：`EvalCompleteness` 声明它，维持 `True` 的marker公理 `trustEvalCompleteness` 标记需要它的论证，CI axiom ratchet枚举belt每个 `axiom`。
- **trusted（已review接缝）**：`Corpus.lean` 的 `eraseE`/`eraseChain` + `render*` 对。两个约40行函数走相同surface grammar，一个生成求值器评分的λ_almd项，一个生成compiler读取的almide文本。逐constructor审核一致**就是**信任义务，一次review而非每程序review。（手写family手工加入同接缝：`spec/wasm_cross/kernel_conformance.almd` ↔ `kAll`，Rust literal ↔ Lean literal。）
- **gated**：两backend产生那些观测。`tests/kernel_conformance_test.rs` 在native及有wasm runtime时的wasm运行全corpus；`wasm_cross` harness携带family；`conformancegen --check` CI将提交corpus逐字节pin到 `Corpus.lean`。都在C-280下。

上分类采用Lean自身三层词汇（修正来自阅读lean4源码，Survey 4）：

| Lean idiom | 信任级别 | 原因 |
|---|---|---|
| `decide` / `:= by rfl` | kernel证明 | kernel自己重新归约项 |
| `#guard` | 不受信pin，**非证明** | 运行不受信elaborator求值器（lean4 `src/Init/Guard.lean`） |
| `native_decide` | **以具名公理为条件**的compiled求值器证明 | 实体化 `Lean.ofReduceBool`/`trustCompiler`，让 `#print axioms` 展示接缝 |

belt所有conformance主张都用tier 1；`.expected` 文件处于tier 3信任级（compiled求值），接缝记在这里而非公理中，因为consumer是shell gate，不是Lean证明。

所以“backend是kernel的refinement”在**生成的λ_almd可表达fragment**上强制：机器计算程序、compiled求值器期望值（非kernel检查）、机器diff backend，加一个review过的约80行接缝。不把compiler重写于prover内则仍够不到：经验证surface-core→λ_almd翻译，以及Rust实现本身每pass simulation证明。研究级残余记在 `docs/roadmap/active/edit-locality-theory.md`，此前由fragment gate作ratchet。运行首日corpus抓到两个真实compiler bug（almide#1428：checker接受程序在codegen死掉；almide#1429：v1 renderer在bare-parameter tail把effect fn签名与body分裂），说明gate确实有效。

<a id="above-l1-l4-and-l5-edit-localitymd-1a"></a>

### L1之上：L4与L5（`edit-locality.md` §1a）

每义务按能说什么拆开，因为两半地位不同，单说“L4成立”会隐藏检查了哪一半。以下定理均在 `crates/almide-edit-belt/AlmideEditBelt/Contract.lean`，0 `sorry`，无新公理。

| 义务 | 地位 | 主张 | 不主张 |
|---|---|---|---|
| L4，effect frame | **proven**（`l4_pure_replacement_silent`） | pure声明定义的替换若通过未变签名检查，在编辑后λ_almd程序任意环境产生空trace | 替换返回值或caller如何使用，caller可能随后打印不同内容 |
| L4，非空虚性 | **proven**（`l4_witness`、`l4_loud_replacement_prints`） | 四假设可同时成立；去掉purity假设结论为假（effectful替换打印 `"hi"`） | 假设最弱 |
| L4，resource frame（`scoped`，#1997） | **gated**，非证明，C-362 / C-363 / C-364 fixture | `scoped` block观测即body观测；`scoped fn` 在region内外同回答；fragment外形状在双target check时拒绝 | λ_almd任何性质：kernel无heap/region，添加region component前无可陈述内容 |
| L5，组合 | **proven**（`l5_edits_compose`、`l5_pure_silent_after_edits`） | 任意保持signature的替换序列维持well-typed，结果每pure声明定义silent | 序列保留pure code空trace以外任何观测 |
| L5，双target实现 | **trusted**，backend refinement | — | checked lowering（#1995 ExitPlan、#1996 E-OWN-LOWERING）在native/wasm保留上述性质。这是本节同一kernel/backend接缝，由 `spec/wasm_cross`、契约台账gate，非证明 |

<a id="what-each-gate-actually-claims"></a>

## 每个gate实际主张什么

| Gate | 主张 | 不主张 |
|---|---|---|
| `proofs/gate.sh` | witnessed MIR为RC-safe、name-total、capability-bounded；`almide-verify` 在全部行与1,000个带seed随机witness上给出extracted checker判定 | wasm字节匹配witness；`almide-verify` 在这些行外输入一致（无定理） |
| `proofs/structural-wall.sh` | `witness-fixtures.txt` 每fixture的出货文件SHA-256与bundle pin一致，structural build全部ownership/call-mode/name/capability/bounds/init witness均由extracted checker、kernel、`almide-verify` 接受。由 `accepts_spatial`，被接受bounds witness每次run使各记录index access在block内；由 `site_window`，被接受init witness每记录construction site使block全范围初始化；由出货文件上的 `almide-anchor-check`（#4011 (d)），每located bounds/init event确是其所指instruction——site分配大小、store的width/offset/address local、fill倒计数、check比较并abort、access load/store；每located access与check operand绑定（同index、按slot size计数的目标block count、单slot内、中间两者都无变化） | ownership event是字节内RC instruction；located judge call是copy-on-write judge且仅接收block的call原地缩小它；representation；任何bounds witness未记heap access（record field、string byte、map slot、`for` loop元素读、runtime例程） |
| `proofs/native-wall.sh` | `proofs/native-verified-floor.txt` 每program，verified native renderer渲染各函数的ownership witness来自实际render的MIR，且被extracted checker/kernel/`almide-verify` 接受；bundle pin生成Rust source SHA-256；丢release的render witness被拒 | rustc在binary保留MIR balance；floor外程序（standard-codegen fallback） |
| `scripts/witness-reinject.sh` | 各重注入bounds check缺失（store路径、read路径、过时#3345 fact）留下 `almide-verify` 拒绝的bounds witness；重注入list-literal store缺失留下其拒绝的init witness；同程序未patch emitter witness接受。`anchors`：记录call仍存的instruction修改再注入（guard变dead、slot store错位、fill cursor加倍）留下证明checker仍接受的witness，但字节被 `almide-anchor-check` 拒绝；operand bug再注入也如此（以byte计count、既未记录也未施行的kill、zero-width Bytes room） | anchor未检查的instruction变更能被捕获（representation、RC instruction） |
| `scripts/check-build-certifies.sh` | compiler旁安装 `almide-verify` 时，`almide build --target wasm` 拒绝写出自身certificate被拒的artifact：每次bounds check缺失再注入使build exit 1且不写file，未patch build写出，移走verifier后patch build也写出 | 无verifier安装的build被检查（实际不检查）；certificate未记录内容（上行） |
| `proofs/corpus-wall.sh` | `lower_function` 在corpus上全域：每函数为 `Ok` 或显式 `Unsupported` | `Ok` 函数输出正确 |
| `proofs/output-parity.sh` | baseline集合内native/structural wasm build一致，内部structural wall恰为 `proofs/output-parity-walled-baseline.txt` 行（#2793） | 集合外任何内容 |
| `scripts/check-contracts.sh` | 每个可观测cross-target承诺有可执行证据 | 承诺本身正确 |
| greenfield parity gate（`ast_parity`、`check_parity`、`run_parity`、`backend_parity`、WASI gate） | 移植parser/checker/interpreter/structural wasm逐字节复现**同commit**构建CLI在corpus打印的内容（`--emit-ast`、`check --json`、`run --target wasm`）；`scripts/check-parity-goldens.sh` 在CI由该binary再生golden，任一行diff失败，所以不能手改golden或取旧build记录（#2183） | CLI自身输出正确；oracle是被测tree而非独立参考。CLI正确性由契约台账、kernel pin、cross-target fuzz主张，路径一致只主张移植忠实 |
| cross-target fuzz | N程序未发现divergence | 不存在divergence |
| E083 exit validator（`exit_plan.rs`，#1996） | 各函数emit exit window实现checked ExitPlan：每released credit在此decrement，不decrement carry/foreign，transfer instruction与plan所指一致，frame替换edge无未处理credit；从wasm字节回读，否则以compiler-defect诊断停止build（绝不wall、绝不reroute） | plan**内容**正确（属credit harness/witness）；字节下游（wasmtime/host） |
| kernel-conformance pin（C-280） | compiled family观测在双target等于kernel检查的λ_almd trace；生成corpus匹配compiled求值器trace | 每个almide program都refine kernel——只pin fragment的像；corpus `.expected` 值是求值器pin（compiled Lean），非kernel检查 |

全部复现：`make verify-trust`。
