[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](README.zh-CN.md)

> 非规范、固定原文的历史读者副本，不是发布输入，也不授权打标签、推送、运行 workflow 或改写已发布证据。权威发布文件不变。v0.68.0 正式版说明是未发布、已被取代的草稿，不是发布证明；此处未独立重新测量原文主张或数字。不自动同步。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/releases/README.md)

<a id="release-notes"></a>

# 发布说明

每个标签对应一个 `docs/releases/<tag>.md` 文件（`-rc` 标签也有自己的文件）。`.github/workflows/release.yml` 从中发布 GitHub release 标题和正文，并拒绝构建缺少该文件或文件格式错误的标签。在推送标签**之前**编写文件，经 `develop` → `main` 合入；标签必须指向包含它的提交。

打标签前检查：

```bash
bash scripts/check-release-notes.sh vX.Y.Z
```

<a id="format"></a>

## 格式

```markdown
# vX.Y.Z — <lowercase headline: one or two concrete claims joined by ", and">

<One or two sentences: what this release is. For an rc: "**Release candidate N**
(#1484): published as a prerelease for the soak window, excluded from "latest"."
and what it adds after the previous tag.>

## <Theme — e.g. Language surface, Stdlib, Correctness, wasm, Performance, Diagnostics, Tooling>

- **<The observable change, stated plainly>.** What was wrong or new, and what
  a user now sees (#PR, issue #N).

## Gates and evidence

- <Internal gates, ledgers and logs — last and brief.>

## Dependencies

<Bumped crates, one line.>

**Full Changelog**: https://github.com/almide/almide/compare/vPREV...vX.Y.Z
```

模板说明译文：标题使用小写，以 ", and" 连接一至两个具体主张。开头用一两句话说明此发布；rc 应说明“发布候选 N（#1484），作为稳定观察期的预发布，不属于 latest”，以及相较前一标签新增的内容。按语言、标准库、正确性、wasm、性能、诊断、工具等主题分组。清楚说明可观察的更改、之前的问题或新增内容，以及用户现在看到什么，并附 PR、issue。内部门禁、台账与日志放在最后，保持简短；升级的 crate 用一行列出。

检查强制以下规则：

- 第 1 行为 `# <tag> — <headline>`（em dash，标题 10–120 字符），它成为发布标题。
- 第 2 行为空；之后全部为正文。
- 不使用自动生成的 `## What's Changed` 列表，也不使用 `* <PR title> by @user in <url>` 行。按主题解释用户可见变化，将相关 PR 合并成一个条目。
- 正文以指向此标签的 `**Full Changelog**` 比较链接结束。

示例：[v0.68.0-rc1](../../../../releases/v0.68.0-rc1.md)、[v0.68.0-rc2](../../../../releases/v0.68.0-rc2.md)。
