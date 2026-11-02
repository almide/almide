# Almide — Claude Code 插件

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](./README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](./README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](./README.zh-CN.md)

将 `almide` 二进制程序内置的两个服务器接入 Claude Code 会话：

- **MCP**（`almide mcp`）— `almide_check`、`almide_test`、`almide_api`、`almide_explain`、`almide_fmt_check`、`almide_survive`
- **LSP**（`almide lsp`）— 为 `.almd` 提供代码智能功能

插件负责配置服务器，不负责安装二进制程序。请先将 `almide` 放入 `PATH`（在仓库根目录运行 `make install`），否则 `/plugin` 的 Errors 标签页会显示 `Executable not found in $PATH`。

```bash
/plugin marketplace add almide/almide
/plugin install almide@almide
```

完整文档：[docs/mcp.md](../../docs/mcp.md)。
清单模式检查：`claude plugin validate tools/claude-plugin --strict`。
