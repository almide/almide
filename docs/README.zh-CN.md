# Almide 文档

[English](./README.md) | [日本語](./README.ja.md) | [简体中文](./README.zh-CN.md)

从这里开始，按你的目标选择入口。

| 我想…… | 阅读 |
|---|---|
| **编写 Almide** | [CHEATSHEET.md](./CHEATSHEET.md) — 语法、标准库、惯用写法。LLM 应首先加载的文档（另见 [`/llms.txt`](../llms.txt)） |
| **了解这门语言是什么** | [SPEC.md](./SPEC.md)（规范性文档）· [GRAMMAR.md](./GRAMMAR.md)（EBNF）· [specs/](./specs/)（分领域说明，附测试路径） |
| **修改编译器** | [ARCHITECTURE.md](./ARCHITECTURE.md) — 流水线与模块图 · [cross-target-decisions.md](./cross-target-decisions.md) — 两个目标都必须依据同一来源回答的语义问题 |
| **判断是否值得信任** | [TRUST-SPINE.md](./TRUST-SPINE.md) — 哪些已证明、哪些已测量、哪些两者都不是 · [benchmarks/README.md](./benchmarks/README.md) — 所有公开测量及其日期和验证门禁 |
| **查看项目现状** | [status.md](./status.zh-CN.md) — 成熟度、目标、代码生成 · [how-it-works.md](./how-it-works.zh-CN.md) — 一页介绍流水线 · [written-in-almide.md](./written-in-almide.zh-CN.md) — 本仓库之外的程序 |

## 目录

| 路径 | 内容 | 是否由机器读取？ |
|---|---|---|
| [`specs/`](./specs/) | 分领域的语言规范。编写规则：[specs/CLAUDE.md](./specs/CLAUDE.md)。`specs/als/` 是契约台账引用的规范性 ALS 章节 | ✅ CI 路径过滤器 |
| [`stdlib/`](./stdlib/) | 每个标准库模块一页；各页中的签名索引由工具生成（`make stdlib-docs`）。`semantics-manifest.toml` 是门禁输入 | ✅ `check-semantics-manifest.sh` |
| [`diagnostics/`](./diagnostics/) | 每个 `EXXX` 代码一页。**编译进二进制文件** — `build.rs` 用 `include_str!` 嵌入此目录，`almide explain EXXX` 将其打印出来 | ✅ `build.rs` |
| [`contracts/`](./contracts/) | 行为契约台账：每项可观察的 native ⇄ wasm 承诺都能追溯到可执行证据。索引和符合性报告由工具生成 | ✅ `check-contracts.sh` |
| [`adr/`](./adr/) | 架构决策记录 — 记录采纳或拒绝某项设计的原因，带编号且不可变 |  |
| [`roadmap/`](./roadmap/) | `active/` = 进行中（约20篇由 Rust 源码注释按名称引用）、`on-hold/`、`done/`。README 根据目录树生成 | ✅ `almide-gates` |
| [`design/`](./design/) | 设计依据：[DESIGN.md](./design/DESIGN.md)（消除歧义）、[REJECTED_PATTERNS.md](./design/REJECTED_PATTERNS.md)、[HIDDEN_OPERATIONS.md](./design/HIDDEN_OPERATIONS.md)、[EQUIVALENCE.md](./design/EQUIVALENCE.zh-CN.md) |  |
| [`wasm/`](./wasm/) | [WASM-OUTPUT.md](./wasm/WASM-OUTPUT.md)（wasm 后端的输出与承诺）、能力系统 |  |
| [`project/`](./project/) | [BENCHMARKS.md](./project/BENCHMARKS.md)、[BREAKING_CHANGE_POLICY.md](./project/BREAKING_CHANGE_POLICY.md)、[CLAUDE_TEMPLATE.md](./project/CLAUDE_TEMPLATE.md)（由 `almide init` 嵌入） | ✅ `include_str!` |
| [`benchmarks/`](./benchmarks/) | [README.md](./benchmarks/README.md) — 测量页面（从首页 README 移出）；其中的生成区块来自此处已提交的台账（`build-speed.txt`、`wasm-size.txt`、`wasm-runtime.txt`、`native-victory.txt`）以及 `proofs/` | ✅ `almide-gates bench`、`gen-readme-stats.sh`、`gen-claims.sh`、`gen-mutation-score.sh`、`check-readme-numbers.sh` |
| `assets/`、`figures/` | README 图片与生成的基准测试图表 |  |

## 移动或删除这里的内容之前

`docs/` 不只是说明文字。上表的 ✅ 标出了构建步骤或 CI 门禁按路径读取的目录：`docs/diagnostics/` 链接进编译器二进制文件；`contracts/`、`stdlib/semantics-manifest.toml`、`specs/als/`、`TRUST-SPINE.md` 和 `benchmarks/`（台账及其渲染页面）都是门禁输入。约20篇 `roadmap/active/` 文档由 Rust 源码注释按名称引用。移动前请先用 grep 检查。

MSR（modification survival rate）位于 [almide-dojo](https://github.com/almide/almide-dojo)，而非这里；本仓库关注编译器的正确性。

[翻译进度与逐文件范围](./project/TRANSLATION-INVENTORY.md).

诊断代码阅读译文: [日本語](./translations/ja/diagnostics/README.md) · [简体中文](./translations/zh-CN/diagnostics/README.md).
