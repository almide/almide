<a id="github-linguist-への言語追加--準備台帳"></a>

# 向 GitHub Linguist 添加语言——准备台账

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](linguist-submission.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](linguist-submission.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](linguist-submission.zh-CN.md)



> 原文：[docs/project/linguist-submission.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/project/linguist-submission.md)（固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`，日语原文）。这是面向读者的非规范性快照译文，不会自动同步。保留原文的时间、适用范围与主张；翻译时未进行独立测量或实现验证。 这是原文的 2026-09-22 准备记录，不是当前资格判断。使用数量、审核标准、注册表状态及译出的维护者引语均未重新核实；当前是否提交应重新检查上游来源。采用数量估算是计划参考，不保证获准。

让 GitHub 将 `.almd` 识别为语言的计划。**截至 2026-09-22 还不能提交**。原因只有一个：第三方采用不足。文法、示例、颜色等其余事项今天就能准备好。

来源为 [linguist/CONTRIBUTING.md](https://github.com/github-linguist/linguist/blob/main/CONTRIBUTING.md) 和 [PR 模板](https://github.com/github-linguist/linguist/blob/main/.github/PULL_REQUEST_TEMPLATE.md)。数字于 2026-09-22 通过 GitHub API 实测。

<a id="1-条件と現状"></a>

## 1. 条件与现状

| 条件 | 标准 | 现状（2026-09-22） | 判断 |
|---|---|---|---|
| 扩展名使用情况 | **一年内索引的 `.almd` 至少 2,000 个文件**，排除 fork | 总计 5,512 | 表面满足 |
| 排除所有者后的情况 | 审查者用 `-user:` 排除主要所有者后评估 | `-user:almide -user:O6lvl4` 后为 904，再排除 `-user:almide-graphics` 后为 **0** | **未达标** |
| 所有者 / 仓库分布 | 随机点击结果应当分散 | 非所有者的 904 个文件中，296 个位于单一仓库 `almide-graphics/nn`。实际上只有一个组织 | **未达标** |
| 语法高亮文法 | TextMate 兼容文法位于采用**可接受许可证**的公开仓库 | `almide/vscode-almide` 的 `syntaxes/almide.tmLanguage.json`（`scopeName: source.almide`）。初次调查时没有 LICENSE，同日补记已添加 MIT（§2） | 当日已处理（§2） |
| 示例 | 实际代码。拒绝 hello world。注明许可证 | 候选很多。`almide/almide` 采用 MIT / Apache-2.0 双许可证 | 可以满足 |
| 扩展名冲突 | 不与现有语言冲突 | `languages.yml` 中没有 `.almd` | 无问题 |

可接受许可证为 apache-2.0, bsd-2-clause, bsd-3-clause, cc0-1.0, isc, mit, mpl-2.0, ncsa, permissive, unlicense, wtfpl, zlib。

**过早提交适得其反**。2026 年提交给 Linguist 的语言添加 PR 约 240 个，合并 31 个，其余因使用情况不足被关闭。被关闭的 PR 会留下记录。

<a id="通った言語は実際に何ファイルあったか2026-09-22-実測"></a>

### 获准语言实际有多少文件（2026-09-22 实测）

| 语言 | 合并 | `extension:` 的总文件数 |
|---|---|---:|
| Blueprint | 2026-06-01 | 42,624 |
| Gno | 2026-07-20 | 41,536 |
| Tape | 2026-08-06 | 24,704 |
| MeTTa | 2026-02-22 | 12,736 |
| Bend | 2026-09-20 | 9,440 |
| FPP | 2026-08-19 | 9,424 |
| Tolk | 2026-06-21 | 8,128 |
| BAML | 2026-05-13 | 6,704 |
| Quint | 2026-05-06 | 5,824 |
| **Almide** | — | **5,512（排除所有者后为 0）** |

**总量上，Almide 已与获准语言中较低的一端相当。不同的只是所有者分布。** `.qnt` 的 100 条搜索结果出现了至少 8 个所有者，即使排除维护组织仍剩 5,648 个文件。`.almd` 的 100 条结果全部属于 `almide`。

Bend 的 PR #8218 直接展示了评估方式。正文写明 `extension:bend` 为 8,640，排除维护者和排名前 2 的贡献者后为 2,664。Quint 的 PR 也同时列出了排除维护组织后的数量。

维护者的说法也很明确。某次拒绝中写道：“2000 是标准。此外，评估受欢迎程度时会排除语言所有者及最主要的贡献者。无论如何，最终都会排除你和你的组织。”

<a id="15-文法の検査結果2026-09-22-実測"></a>

## 1.5 文法检查结果（2026-09-22 实测）

文法并非进入 Linguist 的障碍。已使用 TextMate 引擎（`vscode-textmate` + `vscode-oniguruma`）实际分词测量。

| 检查 | 结果 |
|---|---|
| 结构 | `scopeName: source.almide`、`fileTypes: ["almd"]`，16 个 include 全部解析成功，无孤立定义 |
| 覆盖率（3 个文件） | 83.4% 的非空白字符有 scope |
| 覆盖率（examples + stdlib + spec，577 个文件，177 万字符） | **87.8%。没有 scope 的 token 仅为括号、逗号、冒号等符号** |
| 确认 20 种语言结构 | `effect fn`、`test`、`match` 的 `=>`、`fan`、heredoc、`${}` 插值、`!` `?` `??`、`@intrinsic`、类型声明——全部有 scope |

**发现 1 个缺陷。** 函数定义名被标为 `variable.other`，而非 `entity.name.function`。原因是顶层模式顺序：`#keyword` 位于 `#function-def` 前面，先消耗了 `fn`，导致 `#function-def` 从未触发。TextMate 采用**定义顺序中的首个匹配**，而非最长匹配。

**修复只需 1 行。** 在 `vscode-almide` 的 `generator/almide_textmate.almd` 中，将 `Include("#function-def")` 移到 `Include("#keyword")` 前面。已在本地验证：

- 生成产物的差异仅为这一行的移动（对比 `almide run generator/main.almd` 的 before/after 输出）。
- 3 个文件中的 30 处函数名变为 `entity.name.function.almide`。
- `keyword.declaration` 数量仍为 104，整体覆盖率也不变。

**另 1 项是写法不一致。** 文法将 `strict` 标为修饰符关键字，但编译器的 `KEYWORDS`（34 个词）没有 `strict`，它只出现在 `keyword_typo.rs` 的拼写提示中。没有实际危害，但作为唯一权威来源的 `grammar/tokens.toml` 记载比实现更宽。

<a id="2-今日そろえられるもの"></a>

## 2. 今天可以准备的事项

1. ~~**在 `almide/vscode-almide` 放入 LICENSE**~~——**2026-09-22 已落地**。添加 MIT，GitHub 识别也变为 `MIT`。同样添加到 `almide/tree-sitter-almide`（main，5ed0dc5）。由此满足 Linguist 导入文法的要求。
2. ~~**§1.5 的 1 行修复**~~——**2026-09-22 已落地**（vscode-almide develop，d4a1438）。已修复生成器并重新生成 `syntaxes/almide.tmLanguage.json`。用公开文件再次验证，20 种结构全部通过，函数名变为 `entity.name.function.almide`。**未完成**：develop → main 会触发 VS Code 扩展发布，因此暂缓发布。
2. **确定示例**。用实际代码代表语言结构。按实测选择的候选如下：

   | 文件 | 行数 | 内容 |
   |---|---:|---|
   | `examples/lisp.almd` | 268 | 27 处 `match`、6 处管道。最适合代表规模与结构 |
   | `examples/api-client.almd` | 79 | `import`、`effect fn`、`!` 传播 |
   | `examples/csv-to-json.almd` | 52 | 7 处管道链、`effect fn`、3 个 `test` 块 |

   避免 `stdlib/list.almd`。它主要是 `@inline_rust` 指令，没有任何管道，看起来不像使用方代码。在 PR 正文明确写“MIT / Apache-2.0 双许可证，来源为 almide/almide”。
3. **确定颜色**。建议 `#7c5cbf`。依据是 playground 和 docs 网站的强调色，`almide/docs` 的 `src/styles/custom.css` 中有 `/* Purple accent from playground: #7c5cbf / #9b7de0 */`。`languages.yml` 中没有相同值。
4. **条目草案**（`language_id` 由 `script/update-ids` 分配，因此提交时不填）：

```yaml
Almide:
  type: programming
  color: "#7c5cbf"
  extensions:
  - ".almd"
  tm_scope: source.almide
  ace_mode: text
```

<a id="3-条件を満たす唯一の道第三者の採用"></a>

## 3. 满足条件的唯一途径：第三方采用

当前索引中的 `.almd` 全部位于 `almide`、`O6lvl4`、`almide-graphics` 三者之下。`almide-graphics` 是自己所属的组织，而且单个仓库就占非所有者部分的三分之一。此时已不满足分布条件。**自己批量创建仓库来凑数是在钻评估方法的空子，而审查者会用 `-user:` 排除**，因此也毫无意义。

数量参考：每个仓库 20～40 个文件，则 **50～100 个第三方仓库**可达到 2,000。也能同时满足分布条件。现实来源包括：

- **包作者**。由他人编写可通过 `almide add` 安装的外部包。目前 `almide/` 下的包全部属于自己，评估时算作零。
- **Dojo 参与者**。任务解答仓库会包含 `.almd`。第三方放在自己的账号下就计入。
- **教程与示例**。Advent of Code、Exercism 社区轨道、语言介绍文章的示例仓库。
- **编辑器用户**。安装 VS Code 扩展的人在自己的仓库中放入 `.almd`。

衡量进度的方法（排除所有者后的实际数量）：

```bash
gh api -X GET search/code -f q='extension:almd -user:almide -user:O6lvl4 -user:almide-graphics' \
  -f per_page=1 --jq .total_count
```

审查者看 Web UI 的数字，因此提交前还应在登录状态下打开 `https://github.com/search?type=code&q=NOT+is%3Afork+path%3A*.almd` 确认。

<a id="4-出す日の手順"></a>

## 4. 提交当天的步骤

1. Fork `github-linguist/linguist` 并创建分支。
2. 将上述条目添加到 `languages.yml`。不要写 `language_id`。
3. 运行 `script/add-grammar https://github.com/almide/vscode-almide`。需要 Docker。文法有问题会被拒绝，应当场修复。
4. 将示例放入 `samples/Almide/`。
5. 用 `script/update-ids` 分配 `language_id`。
6. 通过 `bundle exec rake test`。
7. **填完全部** PR 模板。否则不会被阅读。在“添加新语言”部分写明搜索结果 URL、示例来源与许可证、文法仓库 URL、颜色及依据。

PR 正文骨架：

> Almide 是编译到 Rust 和 WebAssembly 的静态类型语言。
> 搜索结果：`https://github.com/search?type=code&q=NOT+is%3Afork+path%3A*.almd+effect+fn`
> 示例：取自 `almide/almide`，MIT / Apache-2.0 双许可证。
> 文法：`https://github.com/almide/vscode-almide`（MIT），`scopeName: source.almide`。
> 颜色 `#7c5cbf`：Almide playground 与文档网站的强调色。

<a id="5-先に取れる条件の軽いレジストリ"></a>

## 5. 可先进入、要求较轻的注册表

Linguist 可以放在最后。只为高亮，可以先提交到其他地方。**各处都有审核**，但要求的是文法完成度与持续维护，而不是文件数量。nvim-treesitter 由维护者判断是否“功能完整、经用户试用、持续维护”。Zed 也明确说明提交需审核，并非全部会被接受。

| 去处 | 所需内容 | 效果 |
|---|---|---|
| Zed extensions 注册表 | tree-sitter 文法 + 扩展定义 PR | Zed 高亮 |
| nvim-treesitter / Helix | 注册 tree-sitter 文法的 PR | Neovim / Helix 高亮 |
| Shiki | 添加 TextMate 文法 | 文档网站与静态网站高亮 |
| Pygments / Chroma | 实现 lexer | Sphinx、Hugo、许多博客平台 |
| VS Code Marketplace | 已发布 | 已经取得 |

进入这些注册表本身，就能在 Linguist PR 中为“这是一门真实存在的语言”提供旁证。
