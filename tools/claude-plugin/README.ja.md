# Almide — Claude Code プラグイン

[English](./README.md) | [日本語](./README.ja.md) | [简体中文](./README.zh-CN.md)

`almide` バイナリに含まれる2つのサーバーを Claude Code セッションに接続します。

- **MCP**（`almide mcp`）— `almide_check`、`almide_test`、`almide_api`、`almide_explain`、`almide_fmt_check`、`almide_survive`
- **LSP**（`almide lsp`）— `.almd` のコード支援

このプラグインはサーバーを設定するもので、バイナリをインストールするものではありません。まず `almide` を `PATH` に配置してください（リポジトリのルートで `make install`）。配置していない場合、`/plugin` の Errors タブに `Executable not found in $PATH` と表示されます。

```bash
/plugin marketplace add almide/almide
/plugin install almide@almide
```

詳しい説明：[docs/mcp.md](../../docs/mcp.md)。
マニフェストのスキーマ検証：`claude plugin validate tools/claude-plugin --strict`。
