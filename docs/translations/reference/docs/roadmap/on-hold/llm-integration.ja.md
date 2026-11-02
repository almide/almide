<a id="llm-integration"></a>

# LLM 統合

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](llm-integration.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](llm-integration.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](llm-integration.zh-CN.md)

> [英語の正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/llm-integration.md)（固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466`）の非規範的な読者用スナップショット。元の文書が正本であり、この写しは自動同期されず、生成器・チェッカーの入力でもない。以下の日付、計画、完了状態、測定値、比較主張は当時の原文の記述を保持したもので、独立した再現や現行版についての検証ではない。コードブロックとコメントは原文のまま保持する。過去の収集用メタデータは、動作しない読者向け注記として表示する。

> 過去のメタデータ：説明 — ライブラリ生成、自動修正、コード説明のための組込み LLM コマンド。

> 原文の注意点：保留中のこの案は明示的に未実装である。将来の手順、速度・コスト・独自性の主張は提案の根拠であり、独立して立証した機能や比較ではない。「範囲の境界」の fix は提案上のラベルとして残しているが、出荷済みローカル処理の `almide fix` を再定義したり、ソース送信を認めたりするものではない。ソース送信と利用プロバイダーに関する同意、および別インターフェースという前提は維持する。

<a id="status-and-current-cli-boundary-2026-10-31"></a>

## 状態と現在の CLI の境界（2026-10-31）

これは**未実装の AI 統合案**であり、現在の CLI の説明ではない。特に、出荷済みの `almide fix` はローカルで計算した machine-applicable な診断修復を適用して再検査するもので、ソースコードを LLM に送信**しない**。[`src/cli/fix.rs`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/src/cli/fix.rs) を参照。将来の AI 支援修復コマンドには、別のインターフェースと、ソース送信／プロバイダーについての明示的な同意が必要であり、この契約を黙って変えてはならない。以下の `forge`、`pilot`、`[ai]`、`--no-ai` は提案段階のインターフェースである。関数ごとの AI `explain` 案は、出荷済みの `almide explain <diagnostic-code>` コマンドとは別である。

<a id="thesis"></a>

## 主旨

「LLM が最も正確に書ける言語」のコンパイラに LLM を埋め込む。LLM が書き、LLM が直し、LLM がライブラリを増やす — この循環を一つのコンパイラで回す。

<a id="subcommands"></a>

## サブコマンド

<a id="almide-forge--library-generation"></a>

### `almide forge` — ライブラリ生成

テーマと参考実装を指定すると、Almide ライブラリを自動で設計、実装、テスト、公開する。

```bash
almide forge csv --ref python:csv,rust:csv,go:encoding/csv
```

1. 参考ライブラリの API を分析する（文書またはソース）
2. Almide らしい API を設計する（UFCS、effect fn、Result/Option、命名規約）
3. 実装とテストを生成する
4. `almide test` ですべてのテストの合格を確認する
5. GitHub リポジトリを作成して push する

**理由：** エコシステムを立ち上げるため。ライブラリを一つずつ手書きするより、LLM に大量生成させ、人間がレビューするほうが速い。

<a id="proposed-ai-assisted-repair--not-the-shipped-almide-fix"></a>

### AI 支援修復の提案 — 出荷済みの `almide fix` ではない

コンパイルエラーを LLM に渡し、自動修正する。

```bash
almide <proposed-ai-repair-command> app.almd
```

1. `almide check` でエラーを収集する
2. ソースコードとエラー診断を LLM に送る
3. 提案された修正を適用する
4. 再び `almide check` で合格を確認する
5. diff を表示して承認を待つ（`--yes` で省略）

**理由：** エラー回復の拡張である。コンパイラが「こう直す」と言うだけでなく、実際に直す。

<a id="almide-explain--code-explanation"></a>

### `almide explain` — コード説明

```bash
almide explain app.almd
almide explain app.almd --fn parse_config
```

ソースコードの Markdown 説明を生成する。個別の関数も指定できる。

**理由：** 文書生成を自動化するため。LLM が書いたコードを LLM が説明する。

<a id="configuration"></a>

## 設定

```toml
# almide.toml
[ai]
provider = "anthropic"    # or "openai"
model = "claude-sonnet-4-20250514"
# api_key is read from ANTHROPIC_API_KEY / OPENAI_API_KEY env var
```

- `--no-ai` フラグですべての AI 機能を無効にする（オフラインコンパイラとして動作）
- API キーは環境変数から読む（toml にハードコードしない）
- AI 機能はコンパイラのコアのコード経路に影響しない（別モジュール）

<a id="scope-boundary"></a>

## 範囲の境界

**含めるもの（Almide のコードに関するものだけ）：**
- forge：ライブラリ生成
- fix：コンパイルエラーの自動修正
- explain：コード説明

**除外するもの（汎用エージェントではない）：**
- チャット UI
- 任意のタスク実行
- Almide の外のファイル操作

<a id="implementation-plan"></a>

## 実装計画

<a id="phase-1-separate-ai-assisted-repair-command"></a>

### Phase 1: 別の AI 支援修復コマンド
- [ ] HTTP クライアントを追加（`ureq` または `reqwest`）
- [ ] `[ai]` 設定を読み込む
- [ ] `almide check` → エラー + ソース → LLM API → 修正 diff → 適用
- [ ] `--yes` / `--dry-run` フラグ

<a id="phase-2-almide-forge"></a>

### Phase 2: `almide forge`
- [ ] `--ref` parser（`language:package` 形式）
- [ ] 参考ライブラリ API 分析用プロンプトの設計
- [ ] Almide API 設計 → 実装 → テスト生成のパイプライン
- [ ] `gh repo create` + push の統合

<a id="phase-3-almide-pilot--path-optimization-constrained-decoding"></a>

### Phase 3: `almide pilot` — 経路最適化（制約付きデコード）

コンパイラが LLM のトークン生成をリアルタイムで誘導する。生成中の各トークンを parser／型チェッカーが検証し、無効なトークンは即座に拒否して、修正候補を返す。

```
LLM → token generation → almide parser (incremental) → valid?
                                                          ├─ Yes → accept, next
                                                          └─ No  → return valid continuation candidates → LLM re-selects
```

- [ ] 増分 parser API（部分入力から「次に有効なトークンの集合」を返す）
- [ ] 型チェッカーのストリーミングモード（部分 AST に対して型推論する）
- [ ] `almide pilot serve` — 外部 LLM から呼べる LSP 風 JSON-RPC サーバーとして起動
- [ ] 投機バッファー：無効なトークンを巻き戻し、有効な継続を hint として返す
- [ ] ベンチマーク：制約付きデコードの有無で、コンパイル成功率と生成速度を比較する

**理由：** 提案された AI 支援修復は「書いた後で直す」。pilot は「書く瞬間に正しくする」。MoonBit には先行実装があるが、Almide の単純な文法と型システムなら実装コストは低くなる。現在のコンパイラは Rust と Wasm をターゲットにする。この提案は廃止済み TypeScript ターゲットでなく、両者で共有する検査済み IR を使わなければならない。

<a id="phase-4-almide-explain"></a>

### Phase 4: `almide explain`
- [ ] 関数ごとの説明生成
- [ ] Markdown 出力

<a id="differentiator"></a>

## 差別化要因

Rust、Go、TypeScript — どのコンパイラにも LLM は組み込まれていない。しかし、それは「人間が書く言語」だからである。Almide は LLM が書く言語だ。コンパイラ側に LLM を置くことは自然な帰結であり、Almide 独自の強みとなる。

> 保持した設定例のコメントは、provider に anthropic または openai を使い、API キーを ANTHROPIC_API_KEY / OPENAI_API_KEY 環境変数から読むことを示す。図は、LLM のトークン生成を増分 parser が検査し、有効なら受理して次へ進み、無効なら有効な継続候補を返して LLM が選び直すという提案の流れを示す。
