# Almide — Claude Code plugin

Wires two servers that ship inside the `almide` binary into a Claude Code
session:

- **MCP** (`almide mcp`) — `almide_check`, `almide_test`, `almide_api`,
  `almide_explain`, `almide_fmt_check`
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
