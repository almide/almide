# Almide — Claude Code プラグイン

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](./README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](./README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](./README.zh-CN.md)

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
