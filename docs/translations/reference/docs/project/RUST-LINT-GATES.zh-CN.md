<a id="rust-lint-and-formatting-gates-1462"></a>

# Rust lint 与格式化门禁（#1462）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](RUST-LINT-GATES.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](RUST-LINT-GATES.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](RUST-LINT-GATES.zh-CN.md)



> 原文：[docs/project/RUST-LINT-GATES.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/project/RUST-LINT-GATES.md)（固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`）。这是面向读者的非规范性快照译文，不会自动同步。保留原文的时间、适用范围与主张；翻译时未进行独立测量或实现验证。 CI 允许列表与运行情况是原文固定时点的描述，并不表示本译文文件符合快速通道条件。

compiler-championship 调查发现缺失的三条通道，以及各自的现状。下述排除项记录在本台账中——这里没有隐含在 Makefile 注释中的事项。

<a id="live-lanes"></a>

## 已启用的通道

| 通道 | 门禁 | 规则 |
|---|---|---|
| `.almd` 格式化，整个目录树 | `scripts/check-fmt-gate.sh`（CI `checks` job）：`spec/` + `examples/` + `tools/almide-gates/` 使用普通模式（排除故意无效的 gauntlet 单元），`stdlib/` 使用 `--no-import-edit`（属于 splice-context 源码，因此排除自动插入 import；AST 保持验证器仍保护每次重写） | 任何漂移都失败 |
| rustc 警告 | `scripts/check-rustc-warnings.sh`（CI `build` job） | 只允许缩减的基线，当前为 0，两个方向的变化都失败 |
| clippy 警告 | `scripts/check-clippy-warnings.sh`（CI `build` job） | 只允许缩减的基线（`scripts/clippy-warnings-baseline.txt`）：增长在 PR 中失败；减少在 PR 中是通知，在每晚严格的 `clippy-baseline.yml` 通道中失败；deny 级别的 lint（如 `overly_complex_bool_expr`）会让编译本身失败——首次运行抓到了 parser 命名参数路径中残留的 `\|\| true` |

<a id="narrow-prose-only-ci-lane"></a>

## 范围受限的 prose-only CI 通道

对于允许列表内的纯文字变更，CI 可以复用本仓库中、在精确的比较基准提交上成功运行的 `ci.yml` push 任务所产出的 Linux 编译器制品。制品必须有匹配的 repository/run/SHA 来源信息和经过验证的 SHA-256 摘要。缺失、过期、不可信或无法验证的制品都会回退到正常构建；这并不承诺每次文档变更都能避免编译。

分类器从可信比较基准加载。源码、workflow、生成器、编译时嵌入的诊断/模板、规范性 ALS 和未知路径不符合这条新的文字快速通道。只有每个变更路径都符合条件，merge group 才符合条件。否则执行正常的编译器构建与现有 job 选择：旧 `docs_only` 分类器对于一些文档路径仍可能跳过重型测试矩阵。因此 `prose_only` 为 false 并不保证每个 job 都会运行。源码/workflow 变更也不符合旧的 docs-only 分类。以 main 为目标的跨平台构建保持不变。初次安装无法复用此通道，因为基准中还没有分类器。

<a id="what-the-lane-admits"></a>

### 通道允许哪些内容

根目录的 `README*.md`、`CLAUDE.md`、`CHANGELOG.md`、`SUPPORT.md`、`SECURITY.md`；`TOP_DOCS` 中列出的顶层 `docs/` 页面；`docs/benchmarks/README.md`；`.claude/commands/*.md`；以及 `docs/{roadmap,specs,design,project}/**/*.md`，减去下列文件。只有在检查哪些内容读取该路径之后，才允许它进入（`include_str!`、Rust 与 Almide 测试，以及通道跳过的 job 中的门禁）。

以下不被允许，因为除了保留的文档检查之外还有其他内容读取它们：`docs/GRAMMAR.md`（语法测试会解析其 `assign` 规则）、`docs/stdlib/**`（若干 Rust 测试会解析的生成签名索引）、`docs/specs/cli.md`、`docs/project/CLAUDE_TEMPLATE.md`、`docs/diagnostics/**`、`docs/specs/als/**`、生成的 `docs/roadmap/README.md`。旧 `docs_only` 分类器也将前三者视为代码，所以修改它们会运行 Rust 测试矩阵。

即使文件在允许范围内，只要 diff 任意一侧的任何变更行是 HTML 注释或位于生成块（`…:generated:start/end`、`BEGIN/END GENERATED …`、`…: begin/end`）中，变更也会退出此通道。这些行是生成器和新鲜度门禁的输入，因此必须运行完整通道。

<a id="what-still-runs-and-what-is-skipped"></a>

### 仍运行什么，跳过什么

在此通道中，`Build (Linux)` 只产出 release 二进制文件（若存在已验证的基准制品，就使用它）。它跳过 nextest archive 和 rustc/clippy 棘轮，因为此通道没有内容读取它们。Lean、Acceptance Ring、Cross-Target 和 mutation sweep 被跳过。Emit & Format 仍运行每个读取文档的步骤：LLM-surface fences、`docs-gen --check`、标准库与 roadmap 索引、measurements 页面的生成块、契约可追溯性（读取契约声明引用的标题）、契约引用和 grammar 门禁。它只跳过读取编译器源码、`spec/` 语料、测试夹具、`docs/stdlib`（在此通道之外）、发布标签或 GitHub API 的步骤。这些步骤是 JS host 门禁、stability closure、installer attestation、release seal、interface-differ controls、pass-isolated fixtures、pass-order shuffle、ownership certifier、spec-test-file runner checks 和 doc fences。该列表由 `test_emit_and_format_lane_skips_only_the_audited_steps` 固定；添加步骤前，先审计它读取什么。分类失败或为空时运行整个 job。

具名的必需矩阵上下文继续报告结果；分支保护和 merge queue 要求不变。定时/手动验证仍使用完整通道。精确的允许列表和失败行为请参见 [`scripts/ci_prose.py`](../../../../../scripts/ci_prose.py) 及其离线回归检查 [`scripts/test_ci_prose.py`](../../../../../scripts/test_ci_prose.py)。分类器在每个 workflow 的 `Change class` job 日志中打印原因（`prose lane: "…"`）。

<a id="reading-a-run"></a>

### 阅读一次运行

每个 workflow 的分类 job 都打印一行 `prose lane: "<reason>"`，接着打印 `prose_only=true|false`。CI 将此 job 称为 `Change class`；Acceptance Ring、Cross-Target 和 mutation sweep 称其为 `prose-changes`。`false` 的原因会指出第一个让变更不符合通道条件的路径或行。使用此通道时，Emit & Format 的 `Lane` 步骤打印 `PROSE_LANE=true`，跳过的步骤在 job 视图中显示为 skipped。在 `Build (Linux)` 中，`verified exact-base compiler artifact <id>` 表示复用了基准二进制文件；`prose compiler reuse unavailable (…)` 表示重新构建。两者都是有效结果。

<a id="recorded-exclusion-cargo-fmt---all------check"></a>

## 已记录的排除项：`cargo fmt --all -- --check`

截至 2026-08-28，有意不设置此门禁：

1. 目录树测得 **6,168 个 rustfmt hunk**——采用它需要一次全量重新格式化，会与每条进行中的分支冲突，并一次性污染 `git blame`。它必须单独落地，不能搭载在门禁 PR 中。
2. 已提交的生成源码（`crates/almide-codegen/src/generated/*`）由不输出 rustfmt 形式的生成器产生；fmt 门禁会与固定这些源码的 regen-diff 门禁冲突。采用前首先需要为生成目录树设置 `rustfmt.toml` ignore 集合。
3. 由 `include!` 拆分的文件（超过 800 行模块所采用的 codopsy 规则）包含 item 片段，rustfmt 会在拆分边界进行不合适的重排；全目录树重新格式化前需要审计拆分集合。

采用计划依次为：落地 `rustfmt.toml` ignore 集合 → 在没有长期分支的时刻提交一个专门的全量重排 PR（`cargo fmt --all`）→ 在 clippy 步骤旁添加 `cargo fmt --all -- --check`。在此之前，本节如实记录该通道缺失及其原因。
