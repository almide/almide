<a id="tool-operational-requirements-tor--the-almide-verification-chain"></a>

# 工具运行要求（TOR）——Almide 验证链

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](TOR.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](TOR.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](TOR.zh-CN.md)

> 固定原文的非规范读者快照。以原文为准，不自动同步。保留历史状态、证明及适格性评估主张，未独立执行、审计或验证当前情况。DO-330 形式资料或适格性评估准备不等于认证；保留所述范围、假设、未适格工具及缺口。此处不执行引用的运行命令。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/TOR.md)


采用 DO-330 形式的运行约定：**要使验证链的主张适用于自身使用，操作者必须做什么，以及不得假定什么**。这是目标导向的 Stage-5 三项产物中的第三项（工具验证台账 = `proofs/gate-verification.toml`；已知问题的形式化 = 每个发布封存记录必须包含的 `known_problems` 字段）。配套的边界文档为 `proofs/TRUSTED_BASE.md`（哪些受信任、哪些已证明）和 `docs/contracts/README.md`（各门禁主张和不主张什么）。覆盖这些资料的 DO-330 形式资料包索引是 `proofs/TOOL-QUALIFICATION.md`（#574）。

每项要求都列出强制执行它的工具（`enforced-by:`），或标记为 `procedural`（依赖操作者；若已规划未来的执行工具，也会注明）。`scripts/check-tor-refs.sh` 验证每个 `enforced-by:` 路径都可解析；TOR 条目若指向已删除的工具，CI 就会失败。

<a id="1-tool-identification"></a>

## 1. 工具标识

- 工具：Almide 编译器及其携带证明的验证链（`make verify-trust` = `proofs/check.sh` + `proofs/gate.sh` + `proofs/structural-wall.sh` + `proofs/corpus-wall.sh`（原生 MIR 证书）+ `proofs/native-wall.sh` + MIR 和 almide-verify 测试；`make receipt` = `proofs/receipt.sh`）。
- 产物标识（#2760）：回执中的 C-SAFE 行指明 `proofs/structural-artifacts.sha256` 及该文件自身的摘要；每个证书覆盖的测试样例对应一行，记录 `almide build --target
  wasm` 为其交付的字节的 SHA-256。每个证书包固定其对应行；`almide-verify` 根据文件重新计算。主张针对的是这些字节，而非一次构建。
- 版本标识：一次发布就是 git 标签；其证据状态是 `proofs/releases/<tag>.toml` 中的封存记录（CI 将持续针对该标签重新测量）。
- 主张范围：回执的主张表（C-PROVEN / C-SAFE / C-REPRO / C-WCET / C-FAITHFUL）；不对表外事项作出主张。

<a id="2-operational-requirements"></a>

## 2. 运行要求

**TOR-1——证据绑定于带有标记的代码树。** 每项判定仅对生成它时的精确代码树和工具链指纹有效。PATH 中的二进制与工作区构建不匹配时，门禁拒绝运行；重新执行 `make install` 并重新验证。切勿将判定移植到另一棵代码树。
enforced-by: proofs/lib/stamp.sh

**TOR-2——在自己的机器上重新推导。** CI 在信任基础之外。要主张验证链的保证，操作者必须在本地运行 `make verify-trust &&
make receipt`；回执才是主张记录。（CI 是提供便利的镜像，并非证据。）
enforced-by: proofs/receipt.sh

**TOR-3——边界拒绝是拒绝，不是应设法绕过的错误。** 超出已验证子集的代码会收到明确的边界拒绝，且不享有任何主张：结构路径无法lowering的程序会收到 E082 拒绝；见证未覆盖的帧会以计入统计的原因作出 DECLINE，因此其程序不在证书覆盖集合内，也不携带 C-SAFE 主张。修改代码直到边界拒绝消失，意味着将代码移入主张覆盖的子集；压制、补丁绕过或忽略边界拒绝，会使该程序的所有主张失效。
enforced-by: proofs/structural-wall.sh

**TOR-4——主张的含义严格限于对应的回执行。** C-SAFE 指结构构建产生的 wasm 的内存、名称和能力健全性，范围仅限该行计数的证书覆盖的测试样例；它不表示功能正确（PID 控制器可能内存安全，却控制了错误的对象），证书覆盖集合之外的程序完全没有 C-SAFE 主张。C-SAFE 和 C-PROVEN 是回执主张，而不是 docs/contracts/contracts.toml 中的条目。它们仍受回执中关于受信任记录与投影器的假设以及辅助函数排除项的限制；它们不是对任意交付程序全部四项 `MemorySafe` 条款的证明。需求层面的验证（MC/DC、需求追踪）属于操作者的领域流程，超出本工具范围。
enforced-by: proofs/receipt.sh

**TOR-5——依赖某次发布之前，先审查其已知问题。** 每个发布封存记录都有 `known_problems` 条目（由检查器强制要求，绝不为空）。运行方必须审查并遵循其中的处置要求；在其中披露的问题不受任何主张覆盖。
enforced-by: scripts/release-seal.sh

**TOR-6——固定封存记录指定的工具链。** 封存记录记录了验证该发布时使用的 Rust 和 wasmtime 版本（Coq 的固定版本记录在 TRUSTED_BASE 中）。替换版本会使证据失效；若必须偏离，应按照 TOR-1/TOR-2 重新验证。
enforced-by: scripts/release-seal.sh

**TOR-7——按照门禁验证台账衡量门禁判定。** 从未展示过失败方向的门禁在台账中被归为 UNVERIFIED（上限只能缩减；逐步清理见 #1244）。UNVERIFIED 门禁的绿色结果所提供的证据弱于 NEGATIVE_TESTED/KERNEL_PROVEN 门禁的绿色结果；台账说明各门禁属于哪一类。
enforced-by: scripts/check-gate-verification.sh

**TOR-8——可执行规范裁决分歧。** 在参考解释器投票的范围内（三方语料库），interp 的行为就是对规范的解读；与其不一致的目标是错误的，即便两个目标彼此一致。弃权情况按类别记录且只能缩减；弃权区域有规范文本和契约，但没有可执行的第三票。
enforced-by: scripts/check-abstain-classes.sh

**TOR-9——wasm 执行环境尚未经过适格性评估。** wasmtime 固定了版本，但没有适格性评估依据（目前不存在针对它的 DO-330 路径）。Critical 配置部署必须在自身安全论证中，将 wasm 运行时视为未经过适格性评估的 COTS 组件。程序性要求——以 #865 跟踪。

**TOR-10——将产物与其证书包核对，而不是与产生它的构建核对。** 证书包（版本 2）指定其见证所描述文件的 SHA-256。在依赖交付的 `.wasm` 之前，在其旁边运行 `almide-verify bundle <bundle>`：出现不匹配——构建后哪怕只改变一个字节——都会得到 REJECT，无论见证如何描述。没有 `artifact` 记录的证书包（版本 1）证明的是见证，不是文件。
enforced-by: crates/almide-verify/src/bundle.rs

<a id="3-known-limitations-operational-view"></a>

## 3. 已知限制（运行视角）

| 限制 | 运行上的影响 |
|---|---|
| 这些门禁尚未确立领域层面的 MC/DC 适格性 | 行覆盖、条件覆盖及安全集合的判定向量是有范围限制的证据，并非全面的领域覆盖认可；应在领域流程中规划必要评估 |
| C-WCET 待完成（计数循环这一关键环节） | 不主张分配或时间上界；不要从回执推导时间预算 |
| wasm 路径上的 C-FAITHFUL 为部分成立（四个运行时 RC 例程已证明字节到语义的对应关系；其余所有生成函数体，以及程序指令到其所有权见证的关联，均未证明） | 对 RC 运行时以外的结构构建行为，应视为经过差分测试（native / wasm / interp 字节一致），而非已证明 |
| 原生路径上的 C-FAITHFUL 待完成（生产 Rust 渲染器 + Ferrocene） | 原生路径的目标代码信任经由 rustc，而非经过适格性评估的工具链 |
| 模糊测试耐久连续记录尚短 | 耐久指标（90 天里程碑）仍在累积；引用前应查看当前连续记录 |

<a id="4-traceability"></a>

## 4. 可追溯性

`scripts/check-tor-refs.sh`（CI）维护 TOR 条目与执行工具之间的对应关系。任何发布的证据链为：封存记录 → 标签 → 在该标签代码树上运行 `make verify-trust` + `make receipt` → 本 TOR 对如何运行它的要求。

> 原文范围注记：TOR-8 保留此原文的绝对裁决规则。同一固定版本的解释器指示也允许分歧暴露解释器自身的错误。原文表述存在差异；本读者副本不消除该差异，也不确立当前行为。
