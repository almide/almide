# 支持政策

[English](./SUPPORT.md) | [日本語](./SUPPORT.ja.md) | [简体中文](./SUPPORT.zh-CN.md)

本文明确说明采用者可以期待什么。这里记录的是政策；下文每项保证都注明落实它的机制，以免政策与执行脱节。漏洞报告单独处理：参见 [SECURITY.md](./SECURITY.md)。

## 哪些版本会获得修复

Almide **尚未达到 1.0**。目前仅支持一个版本系列：最新发布版。修复合入 `develop`，随下一个标签发布，不向旧标签回移修复，也不为旧版本提供补丁系列。已发布的标签请查看[发布历史](https://github.com/almide/almide/releases)；本政策不会把旧版本固定为“最新版”，也不承诺发布日期。

两个机制确保这一单一系列的政策得到落实：

- **发布阻断条件** — 只要存在带有 `I-unsound` / `I-miscompile` / `I-divergence` / `regression` 标签的未关闭 Issue，发布工作流就拒绝创建正式标签（#1482 的检查关卡）。这能防止在这些带标签的 Issue 尚未解决时发布正式版；并不意味着所有缺陷都已被发现或正确标记。
- **RC 渠道** — 涉及语言公开规则或编译器行为变化的版本，先打 `vX.Y.Z-rcN` 标签，并以 GitHub 预发布版的形式提供，在正式标签之前留出观察期（#1484）。

每个版本都是不可变的证据记录：CI 会持续对照该标签重新测量其 seal（`proofs/releases/<tag>.toml`），发布产物附有 Sigstore 证明（`gh attestation verify <asset> -R almide/almide`）。

**LTS**：目前没有 LTS 系列；在具备资金支持的负责主体建立该系列之前，本文将一直明确说明这一点。没有能够兑现承诺的组织，却承诺多年维护，只会流于表面。1.0 之前能够诚实提供的是更新迅速、受检查关卡约束、证据已封存的最新系列。LTS 的指定属于 [docs/roadmap/active/flight-organization.md](./docs/roadmap/active/flight-organization.md) 中追踪的组织建设阶梯。

## 这里的版本号意味着什么

版本号采用 semver 的形式，但契约不是数字本身，而是以下两个台账：

- **Dialect epoch** — [`proofs/dialect-epochs.toml`](./proofs/dialect-epochs.toml) 是仅可追加的记录，记载每一项可能破坏已有代码的变更。**只有**这类变更才会推进 epoch；当前值应查询台账，而不是从发布次数推断。文件带有 `@dialect(N)` 标记，使编译器能够区分采用旧 dialect 的文件和错误程序。`scripts/check-dialect-epochs.sh` 会将台账与编译器的 `CURRENT_DIALECT` 常量交叉核对，不允许记录与执行不一致。
- **标准库接口关卡** — `scripts/check-interface-diff.sh` 根据已提交的签名索引，将任意两个标签之间的标准库公开接口差异归类为相同／新增／破坏性。破坏性差异若未明确声明，就会阻止发布。移除接口前必须经过 `@deprecated` 过渡期（在每个使用位置给出 E052 迁移诊断）；若可能破坏已有代码，还必须记录在 dialect epoch 台账中。

具体而言：**在同一个 epoch 内，能够编译的代码会继续能够编译**。公开接口以新增方式扩展；移除前通过编译器中的 `@deprecated` 提前告知；任何破坏已有代码的变更都会建立新的 epoch，并列明破坏性变化。面向 LLM 的公开规则还受到 [docs/STABILITY.md](./docs/STABILITY.md) 的额外冻结约束。

## 巴士系数与连续性

坦率地说，Almide **只有一位人类维护者**，大部分代码由机器编写并经过审查。这意味着巴士系数为一，这里的措辞无法改变这一点。工程上的安排，是确保与项目正确性有关的知识不只存在于维护者脑中：

- **操作人员能够重新推导一切** — trust spine 的规则（`proofs/TOR.md` 中的 TOR-2）将 CI 排除在信任基础之外：`make verify-trust && make receipt` 可以在普通硬件上、无需维护者参与，重新推导完整证据链——Rocq 内核证明、每次构建的证书、跨目标测试集和封存的发布记录。
- **规范独立于实现** — ALS（[almide/als](https://github.com/almide/als)）是单独的规范性规格，拥有自己的参考求值器；编译器接受规范的检验，而不是反过来。
- **产物的连续性** — 发布版本、seal 和证明保存在公开基础设施（GitHub、Sigstore 的公开日志）中；许可证（MIT OR Apache-2.0）允许任何一方分叉并继续维护。

组织与安全联系人列在 [SECURITY.md](./SECURITY.md) 中。它们的存在不等于提供付费支持合同。本政策没有规定付费支持条款、签约安排或认证赞助。不要从工程关卡或安全报告政策推断出这些承诺；任何此类服务都需要单独列明范围与条款。组织建设阶梯的工作（#586）本身并不代表提供这些服务。

## 如何获得帮助

- **错误和功能请求** — [GitHub issues](https://github.com/almide/almide/issues)；按照 [docs/project/ISSUE-TAXONOMY.md](./docs/project/ISSUE-TAXONOMY.md) 的分类，静默产生错误答案类的报告（错误输出、跨目标行为不一致）优先分诊。
- **安全报告** — [SECURITY.md](./SECURITY.md)（私下报告，不要创建公开 Issue）。
- **文档** — 编写 Almide 请查阅 [docs/CHEATSHEET.md](./docs/CHEATSHEET.md)，语言本身请查阅 [docs/SPEC.md](./docs/SPEC.md)，跨目标承诺请查阅 [docs/contracts/](./docs/contracts/README.md)。
- **文档修正** — 面向 `develop`、仅修改说明性文字的 pull request 由 CI 的文档通道检查（[docs/project/RUST-LINT-GATES.md](./docs/project/RUST-LINT-GATES.md)），无需等待编译器测试矩阵。
