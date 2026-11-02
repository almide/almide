# エージェントのための Almide：MCP サーバー + Claude Code プラグイン

[English](./mcp.md) | [日本語](./mcp.ja.md) | [简体中文](./mcp.zh-CN.md)

> 最終更新：2026-09-26

エージェントがシェルを通じてコンパイラを利用すると、情報を得るために人間向けの整形済みテキストを解析しなければならない。この解析段階で精度が損なわれる。このリポジトリを評価する指標は一つ、LLM が Almide をどれだけ正確に記述・修正できるかである。`almide mcp` はこの段階を取り除き、同じ回答を、型付けされたツール呼び出しと JSON の結果として届ける。

実装はコンパイラだけである。各ツールは CLI をサブプロセスで実行し、CLI が既に生成している機械可読出力を読む。乖離し得る二つ目のコード経路はない。

## インストール

プラグインは両サーバーを設定するが、バイナリは同梱しない。先に Almide をインストールし、`PATH` 上にあることを確認する。

```bash
# 1. the compiler (also provides `almide lsp` and `almide mcp`)
make install                     # or: cargo build --release && cp target/release/almide ~/.local/bin/
almide --version

# 2. the plugin, from this repo's own marketplace
/plugin marketplace add almide/almide
/plugin install almide@almide
```

これにより、Claude Code セッションで次の機能を利用できる。

- **MCP ツール** — 下記の六つ。名前空間は `mcp__plugin_almide_compiler__<tool>`
- **LSP** — `.almd` ファイル向けの `almide lsp`（hover、completion、documentSymbol、formatting、definition、signatureHelp、codeAction、カスタム要求 `almide/survive`）

他の MCP クライアントも利用できる。サーバーは通常の stdio サーバーである。

```json
{
  "mcpServers": {
    "almide": { "command": "almide", "args": ["mcp"] }
  }
}
```

手動で確認するには次を実行する。サーバーは stdio 上で改行区切りの JSON-RPC 2.0 を使用する。

```bash
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | almide mcp
```

## ツール

| ツール | 実行内容 | 戻り値 |
|---|---|---|
| `almide_check` | `almide check --json` | `{ok, errors, warnings, diagnostics[]}` — 各診断は `{level, code, message, hint, here, try, try_replace, repair, context, file, line, col, end_col, secondary}` |
| `almide_test` | `almide test --json` | `{ok, files_passed, files_failed, files[], runner_output_unstructured}` |
| `almide_api` | `almide ide outline --json`, `almide ide stdlib-snapshot --json` | ファイル、`@stdlib/<module>`、または `@stdlib`（コアのスナップショット）の公開宣言と正確なシグネチャ |
| `almide_explain` | `almide explain <CODE>` | 診断のリファレンスページ（markdown） |
| `almide_fmt_check` | `almide fmt --check --json` | `{checked, unformatted[], unreadable[], verify_failed, ok}` |
| `almide_survive` | `almide survive <file> --with - --json` | 書き込む前に求める、編集案の survival 差分：`{schema_version, survives, reach, summary, check, tests, contracts}`。各区分は `{unchanged, newly_broken, newly_fixed, removed}` |

`try` はコピーして貼り付けられる修正断片、`try_replace` は置換対象の範囲である。修正を再考せず機械的に適用できる。`repair`（#2149、診断に修正がある場合のみ存在する）は構造化された形式：`{primary, alternatives, example}`。各編集は `{line, col, end_col, replacement, applicability}` となる。`primary` を無人で適用するのは、`applicability` が `"machine-applicable"` の場合だけにする。`line` は 1 始まり。`col` / `end_col` は 1 始まりの**文字**数（バイト数でも表示幅でもない）で、`end_col` は範囲に含まれない。バイトを数えるハーネスは UTF-8 を壊す（#2250）。

## 書き込み前に編集を評価する：`almide_survive`

modification survival を損なう失敗は、検証せずに編集を適用することである。`almide_survive` は `{file, edit}` を受け取る。`edit` は対象ファイルに対する unified diff、または置換後の全文（`kind`：`auto` / `patch` / `text`）。これを**メモリ内**で適用し、編集の前後それぞれで次を実行する。対象ファイルと、import をたどってそのファイルに到達するすべての `.almd` に対する `almide check --json`、それらのファイルの `test` ブロック、到達するすべての `// @contract:` fixture の native⇄wasm 実行である。各診断・テスト・契約は `unchanged`、`newly_broken`、`newly_fixed`、`removed` のいずれかとして返る。新たに壊れたエラー・テスト・契約がなく、テストや契約が削除されていない場合、`survives` は `true` となる（#4267 — 守っているテストを消しても、そのテストを通過したことにはならない。CLI の `--allow-removed` は意図的な削除を認めるが、このツールはそれを渡さない）。値を読むアサーションをすべて失ったテスト（`assert(true)`、空の本体）は新たに壊れたものとされ、`after: "hollowed"` となる。ファイルへの書き込みは行わない。

診断の `before` / `after` は `almide check --json` が出力したオブジェクトをそのまま保持するため、その出力に追加されたフィールドもすべて変更せずエージェントに届く。行マップを使って編集前後の診断を対応付ける（エラーより前に行を挿入しても、新たなエラーとは扱わない）。同一性の厳密な規則とスキーマは [specs/cli.md § `almide survive`](specs/cli.md) を参照。

同じレポートが LSP 要求 `almide/survive` の応答となる（`{textDocument: {uri}, text?, patch?}`。どちらもなければ、開いているバッファを編集案とする）。survive する場合だけ編集を書き込むには、シェルで `almide apply <file> --with <edit> --if-survives` を実行する。検証と書き込みを、一つの段階でアトミックに行う。

## このインターフェースが従う規則

**既定で読み取り専用。** ソースファイルを書き込むツールはない。`fmt` は `--check` 形式だけを公開する。適用にはシェルの `almide fmt <path>` を使い、編集がエージェント自身の実行記録に残るようにする。`almide_test` と `almide_survive` は副作用を持つツールであり、テストをコンパイル・実行することでビルド成果物を書き込む。その旨を説明と MCP アノテーション（`readOnlyHint: false`）に明記する。`almide_survive` は評価対象のソースを書き込まない。

**文章より構造化。** 診断は整形済みのブロックではなく、フィールドとして届く。

**スクレイピングしない。** CLI に機械可読形式がない場合は、名前が `_unstructured` で終わるフィールドで生テキストをそのまま返し、ツールはその不足を追跡する Issue を示す。整形済み診断を正規表現で読み取れば、このサーバーが取り除こうとしている脆さを再び持ち込むことになる。

## まだ構造化されていないもの

- **テストごとの失敗詳細**（#1313）。`almide test --json` はファイル単位の成否を報告する。失敗したアサーションの期待値・実際の値はテストランナー自身のテキストで、`runner_output_unstructured` として返る。
- **構文解析エラーのコード。** 構文エラーは現在 `check --json` に JSON として届く（以前はテキストのみ）が、それらの診断の `code` は空なので、`almide_explain` で参照する対象がない。
- **`almide compile --json`**（ABI レイアウトを含む完全なモジュールインターフェース）は CLI のみ。`almide_api` はエージェントが実際に尋ねるシグネチャ単位の問いを扱う。ABI の詳細はバインディングジェネレーターの領域である。
- **`almide ide doc <symbol>`**（一つのシンボルのシグネチャと doc コメント）には `--json` 形式がないため、スクレイピングするのではなく公開しない。`almide_api` はモジュール全体について、シグネチャ単位で同じ問いに答える。

## ファイル

| パス | 内容 |
|---|---|
| `src/cli/mcp.rs` | プロトコル：stdio JSON-RPC、initialize / tools/list / tools/call / ping |
| `src/cli/mcp_tools.rs` | カタログと、その裏で行う CLI 呼び出し |
| `tools/claude-plugin/.claude-plugin/plugin.json` | Claude Code プラグイン（MCP + LSP） |
| `.claude-plugin/marketplace.json` | このリポジトリをプラグインマーケットプレイスとして登録 |
| `tests/mcp_test.rs` | エンドツーエンドのゲート：stdio 経由で実バイナリを動かす |
| `src/cli/survive*.rs`, `src/source_overlay.rs` | `almide survive` / `almide apply` と、それらが評価に使うメモリ内オーバーレイ |
| `tests/survive_test.rs`, `tests/survive/` | 三つの golden survival 差分、書き込み禁止と apply のゲート |
