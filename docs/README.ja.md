# Almide ドキュメント

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](./README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](./README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](./README.zh-CN.md)

まずはここから。目的別の入口を案内します。

| やりたいこと | 読む文書 |
|---|---|
| **Almide を書く** | [CHEATSHEET.md](./CHEATSHEET.md) — 構文、標準ライブラリ、慣用表現。LLM が最初に読み込むべき文書（[`/llms.txt`](../llms.txt) も参照） |
| **言語が何であるかを知る** | [SPEC.md](./SPEC.md)（規範仕様）· [GRAMMAR.md](./GRAMMAR.md)（EBNF）· [specs/](./specs/)（分野別の仕様とテストパス） |
| **コンパイラを変更する** | [ARCHITECTURE.md](./ARCHITECTURE.md) — パイプラインとモジュール構成 · [cross-target-decisions.md](./cross-target-decisions.md) — 両ターゲットが単一の情報源から答えるべき意味論上の問い |
| **信頼できるかを判断する** | [TRUST-SPINE.md](./TRUST-SPINE.md) — 証明済みのこと、計測済みのこと、そのどちらでもないこと · [benchmarks/README.md](./benchmarks/README.md) — 公開している各計測と、その日付・検証ゲート |
| **プロジェクトの現状を見る** | [status.md](./status.ja.md) — 成熟度、ターゲット、コード生成 · [how-it-works.md](./how-it-works.ja.md) — パイプラインを1ページで解説 · [written-in-almide.md](./written-in-almide.ja.md) — このリポジトリ外のプログラム |

## ディレクトリ

| パス | 内容 | 機械による読み込み |
|---|---|---|
| [`specs/`](./specs/) | 分野別の言語仕様。執筆規則は [specs/CLAUDE.md](./specs/CLAUDE.md)。`specs/als/` は契約台帳が参照する規範的な ALS の各節 | ✅ CI のパスフィルタ |
| [`stdlib/`](./stdlib/) | 標準ライブラリのモジュールごとに1ページ。各ページのシグネチャ索引は生成物（`make stdlib-docs`）。`semantics-manifest.toml` はゲートの入力 | ✅ `check-semantics-manifest.sh` |
| [`diagnostics/`](./diagnostics/) | `EXXX` コードごとに1ページ。**バイナリに組み込まれる** — `build.rs` がこのディレクトリを `include_str!` で取り込み、`almide explain EXXX` が表示する | ✅ `build.rs` |
| [`contracts/`](./contracts/) | 振る舞い契約台帳。観測可能な native ⇄ wasm の約束をすべて実行可能な証拠に結び付ける。索引と適合性レポートは生成物 | ✅ `check-contracts.sh` |
| [`adr/`](./adr/) | Architecture Decision Records — 採用・却下された設計の理由を残す、番号付きの不変な記録 |  |
| [`roadmap/`](./roadmap/) | `active/` = 進行中（約20件は Rust ソースのコメントから名前で参照）、`on-hold/`、`done/`。README はディレクトリ構造から生成 | ✅ `almide-gates` |
| [`design/`](./design/) | 設計の根拠：[DESIGN.md](./design/DESIGN.md)（曖昧さの除去）、[REJECTED_PATTERNS.md](./design/REJECTED_PATTERNS.md)、[HIDDEN_OPERATIONS.md](./design/HIDDEN_OPERATIONS.md)、[EQUIVALENCE.md](./design/EQUIVALENCE.ja.md) |  |
| [`wasm/`](./wasm/) | [WASM-OUTPUT.md](./wasm/WASM-OUTPUT.md)（wasm バックエンドの出力と約束）、ケイパビリティシステム |  |
| [`project/`](./project/) | [BENCHMARKS.md](./project/BENCHMARKS.md)、[BREAKING_CHANGE_POLICY.md](./project/BREAKING_CHANGE_POLICY.md)、[CLAUDE_TEMPLATE.md](./project/CLAUDE_TEMPLATE.md)（`almide init` が組み込む） | ✅ `include_str!` |
| [`benchmarks/`](./benchmarks/) | [README.md](./benchmarks/README.md) — 計測ページ（トップの README から分離）。生成ブロックはここのコミット済み台帳（`build-speed.txt`、`wasm-size.txt`、`wasm-runtime.txt`、`native-victory.txt`）と `proofs/` から描画 | ✅ `almide-gates bench`、`gen-readme-stats.sh`、`gen-claims.sh`、`gen-mutation-score.sh`、`check-readme-numbers.sh` |
| `assets/`、`figures/` | README 画像と生成されたベンチマーク図 |  |

## ここにあるものを移動・削除する前に

`docs/` は文章だけではありません。上の ✅ は、ビルド処理や CI ゲートがパスを指定して読むディレクトリを示します。`docs/diagnostics/` はコンパイラのバイナリにリンクされ、`contracts/`、`stdlib/semantics-manifest.toml`、`specs/als/`、`TRUST-SPINE.md`、`benchmarks/`（台帳とその描画先ページ）はゲートの入力です。`roadmap/active/` の約20ページは Rust ソースのコメントから名前で参照されています。移動する前に grep で確認してください。

MSR（modification survival rate）の管理先は、ここではなく [almide-dojo](https://github.com/almide/almide-dojo) です。このリポジトリはコンパイラの正しさを扱います。

[翻訳の進捗とファイル別の対象範囲](./project/TRANSLATION-INVENTORY.md).

診断コードの読者向け翻訳: [日本語](translations/reference/docs/diagnostics/README.ja.md) · [简体中文](translations/reference/docs/diagnostics/README.zh-CN.md).
