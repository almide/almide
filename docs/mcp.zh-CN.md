# 面向智能体的 Almide：MCP 服务器 + Claude Code 插件

[English](./mcp.md) | [日本語](./mcp.ja.md) | [简体中文](./mcp.zh-CN.md)

> 最后更新：2026-09-26

智能体通过 shell 使用编译器时，必须解析面向人类排版的文本才能获取信息。这个解析步骤会损失准确性，而本仓库只按一个指标评分：LLM 编写和修改 Almide 的准确程度。`almide mcp` 去掉这一步，将相同的答案以带类型的工具调用和 JSON 结果交付。

编译器是唯一的实现：每个工具都在子进程中运行 CLI，并读取 CLI 已经提供的机器可读输出。不存在可能逐渐偏离的第二条代码路径。

## 安装

插件会配置两个服务器，但不附带二进制程序，因此请先安装 Almide，并确保它位于 `PATH` 中。

```bash
# 1. the compiler (also provides `almide lsp` and `almide mcp`)
make install                     # or: cargo build --release && cp target/release/almide ~/.local/bin/
almide --version

# 2. the plugin, from this repo's own marketplace
/plugin marketplace add almide/almide
/plugin install almide@almide
```

这样，Claude Code 会话便具有：

- **MCP 工具** — 以下六个工具，命名空间为 `mcp__plugin_almide_compiler__<tool>`
- **LSP** — 面向 `.almd` 文件的 `almide lsp`（hover、completion、documentSymbol、formatting、definition、signatureHelp、codeAction，以及自定义请求 `almide/survive`）

其他 MCP 客户端也可以使用；这是普通的 stdio 服务器：

```json
{
  "mcpServers": {
    "almide": { "command": "almide", "args": ["mcp"] }
  }
}
```

手动验证——服务器在 stdio 上使用按换行分隔的 JSON-RPC 2.0：

```bash
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | almide mcp
```

## 工具

| 工具 | 运行命令 | 返回内容 |
|---|---|---|
| `almide_check` | `almide check --json` | `{ok, errors, warnings, diagnostics[]}` — 每条诊断为 `{level, code, message, hint, here, try, try_replace, repair, context, file, line, col, end_col, secondary}` |
| `almide_test` | `almide test --json` | `{ok, files_passed, files_failed, files[], runner_output_unstructured}` |
| `almide_api` | `almide ide outline --json`, `almide ide stdlib-snapshot --json` | 文件、`@stdlib/<module>` 或 `@stdlib`（核心快照）的公开声明及精确签名 |
| `almide_explain` | `almide explain <CODE>` | 诊断的参考页面（markdown） |
| `almide_fmt_check` | `almide fmt --check --json` | `{checked, unformatted[], unreadable[], verify_failed, ok}` |
| `almide_survive` | `almide survive <file> --with - --json` | 在写入之前计算编辑提案的 survival 差量：`{schema_version, survives, reach, summary, check, tests, contracts}`，各部分为 `{unchanged, newly_broken, newly_fixed, removed}` |

`try` 是可复制粘贴的修复片段，`try_replace` 是它替换的范围，因此修复可以机械地应用，而不必重新推导。`repair`（#2149，仅在诊断提供修复时存在）是结构化形式：`{primary, alternatives, example}`，每项编辑为 `{line, col, end_col, replacement, applicability}`。只有当 `applicability` 为 `"machine-applicable"` 时，才可无人干预地应用 `primary`。`line` 从 1 开始；`col` / `end_col` 是从 1 开始的**字符**计数（不是字节，也不是显示宽度），不包含 `end_col` 所指位置。按字节计数的测试框架会破坏 UTF-8（#2250）。

## 在写入前评估编辑：`almide_survive`

影响 modification survival 的失败，是未经验证就应用编辑。`almide_survive` 接收 `{file, edit}`；`edit` 是针对该文件的 unified diff，或完整的新文本（`kind`：`auto` / `patch` / `text`）。它在**内存中**应用编辑，并在编辑前后分别执行：对该文件及所有经 import 能到达它的 `.almd` 文件运行 `almide check --json`，执行这些文件的 `test` 块，并对每个可达的 `// @contract:` fixture 运行 native⇄wasm。每条诊断、每项测试和契约都归入 `unchanged`、`newly_broken`、`newly_fixed`、`removed` 之一。没有新增的错误、测试或契约破坏，且没有测试或契约被移除时，`survives` 为 `true`（#4267——删除守护测试不等于通过它；CLI 的 `--allow-removed` 允许有意移除，但此工具从不传入该选项）。如果测试失去了所有读取值的断言（`assert(true)`、空的测试体），就会被判为新破坏，标记为 `after: "hollowed"`。文件永远不会被写入。

诊断的 `before` / `after` 原样携带 `almide check --json` 输出的对象，因此输出新增的每个字段都会不经修改地传给智能体。诊断通过行映射在编辑前后配对（在错误上方插入一行不会使它变成新错误）；精确的身份匹配规则和 schema 见 [specs/cli.md § `almide survive`](specs/cli.md)。

相同的报告也是 LSP 请求 `almide/survive` 的响应（`{textDocument: {uri}, text?, patch?}`；如果两者都没有，则以打开的缓冲区作为提案）。若要仅在编辑 survive 时写入，请在 shell 中运行 `almide apply <file> --with <edit> --if-survives`——在一个原子步骤中验证并写入。

## 此接口遵循的规则

**默认只读。** 没有工具会写入源文件。`fmt` 只公开 `--check` 形式；实际应用请在 shell 中运行 `almide fmt <path>`，这样编辑会出现在智能体自身的执行记录里。`almide_test` 和 `almide_survive` 有副作用：它们编译并执行测试，会写入构建产物；工具描述及 MCP 注解（`readOnlyHint: false`）都明确说明了这一点。`almide_survive` 从不写入它评估的源码。

**结构化优于文字。** 诊断以字段形式到达，而不是排版后的文本块。

**不抓取文本。** 如果 CLI 没有机器可读形式，就将原始文本原样放入名称以 `_unstructured` 结尾的字段，并说明由哪个 Issue 跟踪这一缺口。用正则表达式抓取排版后的诊断，会重新引入此服务器旨在消除的脆弱性。

## 哪些内容尚未结构化

- **单项测试的失败详情**（#1313）。`almide test --json` 按文件报告通过／失败；失败断言的预期值和实际值是测试运行器自身的文本，以 `runner_output_unstructured` 返回。
- **解析错误代码。** 语法错误现在会以 JSON 形式到达 `check --json`（过去只有文本），但这些诊断的 `code` 为空，因此 `almide_explain` 没有可查询的条目。
- **`almide compile --json`**（含 ABI 布局的完整模块接口）仅在 CLI 中提供。`almide_api` 覆盖智能体实际询问的签名层面问题；ABI 细节属于绑定生成器的领域。
- **`almide ide doc <symbol>`**（一个符号的签名与文档注释）没有 `--json` 形式，因此选择不公开，而不是抓取文本。`almide_api` 在整个模块的签名层面回答同类问题。

## 文件

| 路径 | 内容 |
|---|---|
| `src/cli/mcp.rs` | 协议：stdio JSON-RPC，initialize / tools/list / tools/call / ping |
| `src/cli/mcp_tools.rs` | 工具目录及背后的 CLI 调用 |
| `tools/claude-plugin/.claude-plugin/plugin.json` | Claude Code 插件（MCP + LSP） |
| `.claude-plugin/marketplace.json` | 将本仓库作为插件市场 |
| `tests/mcp_test.rs` | 端到端关卡：通过 stdio 驱动真实二进制程序 |
| `src/cli/survive*.rs`, `src/source_overlay.rs` | `almide survive` / `almide apply` 及其评估所用的内存覆盖层 |
| `tests/survive_test.rs`, `tests/survive/` | 三个 golden survival 差量、不写入与 apply 关卡 |
