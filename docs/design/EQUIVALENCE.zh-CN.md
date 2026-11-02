# 等价性主张 — 跨目标的逐字节一致

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](./EQUIVALENCE.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](./EQUIVALENCE.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](./EQUIVALENCE.zh-CN.md)

**跨目标等价性按契约逐项检查：** 对于受支持的程序，在输入和宿主假设相匹配的条件下，比较确定性的观测结果（stdout、stderr、退出码）。宿主观测、随机性、资源限制以及相对于预言机的时间，需要满足各自声明的不变量或比较条件。通过有限语料集，并不等于证明了所有能够针对两个目标编译的程序都满足等价性。

“逐字节一致”指的是*执行输出*，不是编译产物。原生二进制文件与 `.wasm` 模块在构造上就是不同的字节序列；比较针对的是契约所覆盖的观测结果。

## 范围

这项保证是**持续维护且范围明确的**，由每次变更后重新运行的门禁维持，而非一次性的快照：

- **确定性观测结果**：在声明的跨目标测试样例和差分测试中进行字节比较，并非在不同宿主下不受限制的相等。
- **非确定性输入**：契约可以断言不变量，而非要求样本相同（例如随机值始终处于范围内）。旧的超时移除（C-006）属于历史记录；当前的 `fan.timeout` 是相对于预言机的（C-208）。
- **不可用的操作**：选定的路径可能拒绝某个程序或操作；请查阅 `proofs/target-availability.toml`。拒绝不等于成功观测到该程序在两个目标上的一致性。
- **声明的宿主或表示差异**：平台信息报告和压缩输出具有范围受限的不变量，而非普遍的字节相等。例如，zlib 契约在记录不同压缩策略的同时，比较解码与往返转换。请查阅相关契约及可执行测试样例，不要假定旧的两个函数豁免清单涵盖所有情况。

## 契约台账

这项主张不只是文字。每项可观察的承诺都是[行为契约台账](../contracts)中的具名契约，各自可追溯到级别不低于 `fixture` 的可执行证据。例外条款由 `scripts/gen-claims.sh` 从台账自动生成到[测量页面的主张区块](../benchmarks/README.md)；旁边的契约数量根据台账记录，并带有测量日期（`proofs/ledger-counts.toml`、`scripts/gen-ledger-counts.sh`，每晚由 `scripts/check-ledger-counts.sh` 重新测量）。如果该区块与任何一项输入偏离，`scripts/check-contracts.sh` 就会使 CI 失败。这让生成区块与其声明的输入保持同步，但本身并不能证明每段文字或每个证据链接都证实了所声称的行为。

台账机制：

- 新的可观察行为需要一个新的 `C-NNN` 契约，**并且**至少有一个通过 `// @contract: C-NNN` 头部声明该契约的测试样例。链接是双向的，由 CI 强制检查。
- 消除差异时，必须在同一个 PR 中将契约的 `status` 改为 `active`。`flagged-for-revision` 的数量只允许**减少**（棘轮式约束，当前上限为 0）。

## 证据层次

| 证据层次 | 检查并维持的内容 |
|---|---|
| [契约台账](../contracts) | 每项承诺都有 `C-NNN` 名称；`active` 契约必须附带级别不低于 `fixture` 的证据 |
| [跨目标测试样例门禁](../../tests/wasm_runtime_cross_target.rs) | 每个 `spec/wasm_cross/*.almd` 测试样例都在两个目标上运行，逐字节比较输出（`wasm_cross_target_spec`） |
| [差分模糊测试](../../tests/regex_fuzz_test.rs) | 随机化的程序和输入，比较原生与 wasm 的输出 |
| 生成时 Σ-probe | 在代码生成时，针对整个标量值域，将 wasm 的 Unicode 与大小写转换表和 Rust `std` 进行穷举比对 |
| [Lean 4 belt](https://github.com/almide/almide/tree/develop/crates/almide-perceus-belt) | RC 插入的正确性由 Lean 内核进行机器检查 |
| [组织级 byte-verify 扫描](https://github.com/almide/almide/blob/develop/scripts/org-byte-verify.sh) | almide 组织中每个可运行的仓库都在两个目标上执行，逐字节比较 stdout 与退出结果 |

## 自行验证

```bash
almide test                        # every fixture, both targets where declared
bash scripts/check-contracts.sh    # ledger integrity + claims-block freshness
bash scripts/org-byte-verify.sh    # org-wide two-target byte comparison
```

证书在其覆盖范围和信任假设内，确立模型化见证的性质；它们并不证明原生与 Wasm 的语义等价性。请参阅[已证明与依赖信任的部分](../contracts/proven-vs-trusted.md)以及 [TRUST-SPINE.md](../TRUST-SPINE.md)。
