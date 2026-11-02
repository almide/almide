# Almide — Claude Code plugin

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](./README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](./README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](./README.zh-CN.md)

Wires two servers that ship inside the `almide` binary into a Claude Code
session:

- **MCP** (`almide mcp`) — `almide_check`, `almide_test`, `almide_api`,
  `almide_explain`, `almide_fmt_check`, `almide_survive`
- **LSP** (`almide lsp`) — code intelligence for `.almd`

The plugin configures the servers; it does not install the binary. Put `almide`
on `PATH` first (`make install` from the repo root), otherwise the `/plugin`
Errors tab reports `Executable not found in $PATH`.

```bash
/plugin marketplace add almide/almide
/plugin install almide@almide
```

Full documentation: [docs/mcp.md](../../docs/mcp.md).
Manifest schema check: `claude plugin validate tools/claude-plugin --strict`.
