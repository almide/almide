<a id="formal-credit-claims-table-575-do-333-shape"></a>

# 形式化方法验证信用（credit）主张表（#575，DO-333 形式）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](FORMAL-CREDIT.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](FORMAL-CREDIT.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](FORMAL-CREDIT.zh-CN.md)

> [正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/FORMAL-CREDIT.md)（固定提交 35297a612fb29e3f78f6587aba253aa91d51f466）的非规范性历史阅读快照。以正本为准；此副本不自动同步，也不是生成器或证明输入。未独立执行命令、证明、测量或资格鉴定评估。来源引用、生成标记与带日期数字作为历史资料保留，并非认证、资格鉴定、主管机构接受或发布的证据。

形式化方法补充规范允许用证明满足原本需通过测试满足的验证目标。要获得形式化方法验证信用（credit），每项证明产物必须表述为目标形式的主张，并明确其假设与覆盖边界；本表按证明族给出这种陈述。它不声称任何认证状态：在特定标准下是否采信某项主张，由申请方与主管机构决定。本表的作用是将各项主张表述得足够精确，以便评估。

边界列的依据：[契约边界图](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/README.md)（地图）与 [proofs/TRUSTED_BASE.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/TRUSTED_BASE.md)（实际地域——工具链固定、不可消除的信任根、篡改演练）。机械化证明采用 Rocq（`proofs/*.v`——2026-08-28 统计有 153 条 Theorem/Lemma/Corollary 声明，是历史快照）。单独的已审计假设数量由 `proofs/check.sh` 派生并检查，不是所有 theorem、lemma、corollary 声明总数。独立的 Lean “belt” crate 是设计镜像，在文末单列；它们不是实现的证明，也从不作此主张。

<a id="the-claims"></a>

## 主张

| # | 目标形式的主张 | 证明产物 | 假设（信任基） | 覆盖边界 |
|---|---|---|---|---|
| FC-1 | 已接受的所有权证书意味着证书涵盖的函数不发生 double-free、泄漏或悬空名称使用 | `OwnershipChecker.v` (17)、`OwnershipFilter.v` (3)、`OwnershipLoop.v` (4)、`Subset.v` (5)、`Termination.v` (2)、`StackBalance.v` (1) | Rocq 内核（coqchk 复查，跨版本 9.1.1/9.2）；提取胶水 `driver.ml`（受信任——双向负向测试，见 gate-verification 中 `proofs/build-checker.sh`）；证书忠实描述 MIR（生产者不受信任；检查器通过 `proofs/gate.sh` 每次构建重新验证） | 证明证书模型所述的所有权／名称性质，不是已发布字节的全部四项 `MemorySafe` 条款。不证明 lowering 的任何语义正确性——IR→MIR 行受信任，由 `mir_wellformed.rs` 和差分路径保护，而非证明 |
| FC-2 | 写时复制的别名保持值语义：写入前获取的快照观察到写入前的值 | `CowSafety.v` (3)、`CoownCompose.v` (3)、`CoownLoop.v` (5) | 相同的内核／提取信任根；运行时实现所建模的 COW 规则（由 C-033 fixture 族与差分 fuzz 桥接） | 证明的是模型的 COW 规则；实现符合模型属于 fixture＋差分证据类别，不是证明 |
| FC-3 | 所建模的 free-list 分配器从不发放存在别名或已释放的块，RC 转移保持堆不变量 | `FreeList.v` (29)、`FreeListRc.v` (27)、`RuntimeModel.v` (5)、`StructuralRuntime.v` (19)、`StructuralAlloc.v` (6)、`StructuralDecode.v` (4)、`StructuralRun.v` (42)、`LargeList.v` (42)、`LargeTree.v` (40) | 内核／提取；`StructuralRuntime.v` 证明 structural 路径生成的 `$inc`/`$dec_flat`/`$free` 指令树实现 `rt_inc`/`rt_dec` 与大小类别链表 push，并具有类别计算基石（ceil-class 覆盖、file/take 一致——分配不足导致的损坏形态不可表示）；四个运行时函数的树↔字节绑定全部由内核检查：`StructuralDecode.v` 的符号栈解码器将生成字节（每次运行由 `check-structural-bytes.sh` 落地核对）直接映射到已证明的树，包含 `$alloc`（tee 脱糖、第二个 global、`memory.size`、多字节 SLEB；grow 分支本体逐字匹配，并抽象为其声明的单步语义），Rust 侧哈希固定作为快速警戒线 | structural 路径上 inc/dec/free 的树级实现；`StructuralAlloc.v` 增加 `$alloc` 的 take/bump 路径（pop 依据 `dec_unique_files_take_class` 闭合复用循环；在 no-grow 假设下 grow 本体仍为一个抽象步骤）；inc/dec/free/alloc 的字节解码部分已闭合，`StructuralRun.v` 闭合整次运行的组合：具体树的内存效果任意序列与 ghost 分配器耦合后，保持具体内存上的 `KINV`（`structural_runs_preserve_KINV`）——13 个大小类别的 RINV，64 KiB 以上精确大小大块链表的 LINV（#3348：`LargeList.v` 建模地址有序的 split/coalesce 链表，`LargeTree.v` 证明生成的 `$lfree`/`$ltake` LOOP 树在内存链表上计算该模型），以及作为已证明状态不变量而非假定静态条件的小／大块分离；在两个具名布局前提（heap floor 至少为 16，class table 位于其下）下，将 F-class 表述为对生成代码引理的违反（`structural_no_aliased_handout`、`structural_counts_stay_honest`、`structural_large_handout_is_free`）；grow 重试仍位于 slice 2 声明的抽象边界；structural 的 `$dec_flat` 自 #4023 起携带零哨兵——实现定理保留上游 Perceus 证书的 `1 <= rc` 前提，`dec_freed_block_traps` 证明 rc 0 时的 dec 在任何 store 前停止（trap 的中止本身由引擎负责） |
| FC-4 | 已退役旧运行时所保留的 `rc_inc`/`rc_dec` 字节树解码并执行为所建模的 RC 转移 | `WasmIsa.v` (13)、`WasmExec.v` (9)、`WasmDecode.v` (6)、`WasmEncode.v` (1)、`WasmRcDec.v` (2) | 内核；原 `proofs/check-wasm-bytes.sh` 落地核对关卡随旧 renderer 一同退役（#2761） | 关于该模型运行时的定理，不是关于当前发布的运行时字节。已发布 structural-runtime 的落地核对由 FC-3 通过 `proofs/check-structural-bytes.sh` 完成；两行都不证明完整模块语义 |
| FC-5 | 函数可达能力集合受其声明的 effect 限制：经检查的调用图无法到达未声明能力 | `CapabilityBound.v` (2)、`CapabilityReach.v` (3)、`CallModes.v` (6) | 内核／提取；检查器 E006/E007 接缝在源码边界强制所建模规则（这些接缝属于 MC/DC safety set，#566） | 在所建模调用图上证明；超出模型的动态分派（计算出的调用）被壁拒绝，而非得到证明 |
| FC-6 | 名称解析在接受的子集上是全定义的，类型具体化不会在输出位置留下抽象类型 | `NameTotality.v` (2)、`TypeConcretization.v` (1) | 内核／提取；`assert_names_resolvable` 和 `ConcretizeTypes` 关卡每次构建运行对应实现检查 | 同样区分模型与实现：定理针对模型，构建内关卡承担实现侧 |
| FC-7 | 证书语言的翻译对所建模构造保持 ALS 规定的可观测语义 | `Translation.v` (2)、`ALS.v` (2) | 内核／提取；ALS 记录表的契约行承载各构造的 fixture | 刻意限制范围——仅限所建模构造；当前契约记录表是广度工具 |

提取二进制是快速路径：`proofs/gate.sh` 与语料关卡将其判定同 Rocq 内核和可移植验证器比较。这不消除表中的模型到实现假设。

逐次构建检查（并非对每个证明族提供完整语义绑定）：`proofs/gate.sh`（提取的、经内核证明的检查器重新验证每次构建的证书——工具链 stamp 不匹配为 FATAL）、`proofs/corpus-wall.sh`（检查器阶段的 ACCEPT 经内核证明；实际 REJECT 捕获了 via_if 误编译——失败方向确实在真实 bug 上触发过），以及 `proofs/structural-wall.sh`（发布的 structural 产物、证书 bundle、字节锚点与明确受信任的记录边界）。

<a id="the-lean-belts-design-mirrors-not-credit-claims"></a>

## Lean belt（设计镜像，不是形式化方法验证信用（credit）主张）

`crates/almide-perceus-belt`、`almide-race-belt`、`almide-edit-belt` 是设计规则的可执行 Lean 镜像（RC 插入规则、race determinism 论证、edit-locality 定理）。它们固定设计，Rust 实现引用它们；但没有任何机制将其机械地绑定到发布代码，因此上表没有任何主张依赖它们。

<a id="dossier-status"></a>

## Dossier 状态

本表是 #575 的持续形式交付物。逐版本组装器为 `scripts/gen-dossier.sh`，由带标签的 Trust Spine workflow 调用。具体版本附带的 dossier 与 attestation 必须作为发布产物检查；seal（`proofs/releases/*.toml`）和本文件本身并不能证明已发布。
