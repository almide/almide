<a id="tool-qualification-data-package--the-index-574-do-330-shape"></a>

# 工具适格性评估数据包——索引（#574，DO-330 形式）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](TOOL-QUALIFICATION.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](TOOL-QUALIFICATION.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](TOOL-QUALIFICATION.zh-CN.md)

> 固定原文的非规范读者快照。以原文为准，不自动同步。保留历史状态、证明及适格性评估主张，未独立执行、审计或验证当前情况。DO-330 形式资料或适格性评估准备不等于认证；保留所述范围、假设、未适格工具及缺口。此处不执行引用的运行命令。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/TOOL-QUALIFICATION.md)


适格性评估工程师带去审计的数据包。它是索引，不是新产物：每节指向既有、受 CI 门禁约束的文档或台账。先构建这些资料，是为了让适格性评估成为组装而非重新组织。缺口在末尾逐项列名；本包不宣称认证状态（该判断属于申请方与主管机构）。

<a id="1-tool-identification-and-environment"></a>

## 1. 工具标识与环境

- 工具、版本身份、主张范围：`proofs/TOR.md` §1——发布就是 git tag，证据状态是 seal（`proofs/releases/<tag>.toml`），由 CI 永久对照 tag 重新测量。
- 工具之下的编译器：Ferrocene 通道（`scripts/check-ferrocene-lane.sh`，每周 workflow）用普通 rustc 1.95.0 测试生成的 Rust，这是记录的 upstream 兼容 pin。该通道明确仍等待获得已适格性评估的 Ferrocene 二进制；仅匹配上游版本不会使普通 rustc 获得适格性。
- 发布完整性：每个 asset 均有 Sigstore attestation（`gh attestation verify <asset> -R almide/almide`）与 checksum；依赖树每周对照 RustSec 审计（`SECURITY.md`，#1534）。

<a id="2-tool-operational-requirements"></a>

## 2. 工具运行要求

`proofs/TOR.md`——每项要求指明强制它的工具，若某行指向已删除工具，`scripts/check-tor-refs.sh` 会令 CI 失败。证据按设计可由操作员重新导出（TOR-2：CI 在信任基之外；本地运行 `make verify-trust && make receipt`）。

<a id="3-tool-verification-data"></a>

## 3. 工具验证数据

- 门禁台账：`proofs/gate-verification.toml`——带判定的门禁逐一按失败方式分类（KERNEL_PROVEN / MUTATION_TESTED / NEGATIVE_TESTED / EXERCISED；UNVERIFIED 上限为0，只减不增）。未展示失败证据的门禁不得出货。
- 结构覆盖：`proofs/coverage.sh` 后退防止机制 + 每文件安全下限（`proofs/coverage-safety-baseline.txt`）。安全集合的 MC/DC 级证据位于 `proofs/mcdc-ledger.toml`：未解决多条件决策为0，独立性配对向量由运算符交换 mutant 机械验证（`proofs/mcdc-mutation.sh`，nightly）（#566，已关闭）。
- 证明主干：`make verify-trust`（Rocq kernel + coqchk + 公理审计 + PCC 门禁 + corpus wall），通过 `proofs/receipt.sh` 生成 receipt，并带有保证树与工具链 fingerprint 如实对应的约束。

<a id="4-qualification-test-cases-derived-from-the-als"></a>

## 4. 从 ALS 导出的适格性评估测试用例

已采纳 ALS 章节及其契约链接由仓库适合性台账追踪。Almide 侧导出包括：

- 契约台账：`docs/contracts/contracts.toml`——跨目标契约（当前总数见台账，带日期汇总见 STAGE-STATUS）每项引用 ALS 节（`spec = "ALS-…"`），每个规范节至少被一项契约引用，每个 fixture 双向关联（`scripts/check-contracts.sh`）。
- 可执行语料：`spec/`（run/check/ast parity manifest，630+ fixture 的批准 hash）、wall corpus、差分 fuzz 路径、三路 oracle（native / wasm / 参考解释器，abstain 台账只缩减）。
- 独立参考求值器（als 的 `ref/`，按同样约定 pin 到 Ferrocene）——N-version 路径。

<a id="5-formal-credit"></a>

## 5. 形式化认可依据

`proofs/FORMAL-CREDIT.md`（#575）——将七个 Rocq 证明 family 表述为按目标组织的主张（FC-1..FC-7），各有假设与覆盖边界，逐行区分模型／实现；Lean 辅助验证明确不计入认可依据。

<a id="6-known-limitations"></a>

## 6. 已知限制

- 每次发布：seal 中必须有的 `known_problems` 字段。
- 长期：`docs/contracts/README.md` 中受信行（尤其 IR→MIR）、设计 wall（wall 是拒绝，不是错误——TOR-3；目标可用性原则 #1423），以及契约台账待修订列表（当前为0，只能缩减）。

<a id="7-configuration-management"></a>

## 7. 配置管理

以 git tag 为身份；seal 永久对照 tag 重新测量（CI 中的 `scripts/release-seal.sh check`）；release-blocker 门禁（#1482）在存在未关闭 I-severity issue 时拒绝 final tag；asset 和 dossier 的 Sigstore attestation；每次 lockfile 变化都运行依赖审计通道。

<a id="8-the-assembled-bundle"></a>

## 8. 组装后的 bundle

`scripts/gen-dossier.sh <tag>`（#571）生成逐发布 bundle：receipt + seal + formal credit + stage block + gate summary + 带 KNOWN PROBLEMS 的 contract summary + MC/DC 状态 + sha256 input manifest；由 trust-spine tag 运行签发 attestation 并附在发布中。

<a id="gaps-by-name-the-honest-bottom-line"></a>

## 逐项列名的缺口（诚实结论）

| 缺口 | Issue | 状态 |
|---|---|---|
| Critical profile 可分析 WCET 的 codegen | #569 | 说明 + 带门禁的参考 kernel（`docs/project/WCET-STORY.md`）；目标校准在外部 |
| 生成 Rust 的行级源码追踪 | #572 | 已关闭——`--trace-map` + 审查指南 |
| 出货 runtime（分配器、free-list、RC 操作）的 Lean/Rocq 证明覆盖 | #576 | 已关闭——StructuralRuntime 系列（`StructuralRuntime/Alloc/Decode/Run.v`）：输出字节解码为已证明树，树实现模型转移，完整运行在具体内存上保持 `RINV`；F-class 是输出代码中被违反的引理。grow retry／未分类 bump 仍在声明的抽象边界 |
| 已适格性评估／最小 wasm 执行环境 | #865 | 尚未开始 |
| Critical-profile 子集（`almide check --profile critical`） | #567 | 已关闭——每个 fn 上的有界 profile、默认拒绝全部且通过 `--allow` 授权的 capability、CI 强制子集性质 |
| Critical profile 静态内存模式 | #568 | 已关闭——参考应用满足 critical-clean，无分配 kernel 发射，并在强制固定 `--heap-cap` 预算内完成；host 要求见 `docs/project/PARTITIONED-RUNTIME.md` |
| 通过 `make verify` 的飞行参考应用 | #776 | 已关闭——G-F4 链具有端到端证据；receipt 导出全部六阶段（`proofs/receipt.sh`） |
| 首个携带 dossier 的发布（触发 #571/#1534 关闭） | #571, #1534 | 组装／attestation 机制已运行；须验证具体发布所附 dossier |

阅读此包的适格性评估工程师从 §2（操作员必须做什么）开始，验证 §3 台账可重新导出（`make verify-trust`），再抽查 §4 语料。缺口表是当前可主张范围的边界。
