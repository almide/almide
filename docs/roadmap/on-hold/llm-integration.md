<!-- description: Built-in LLM commands for library generation, auto-fix, and code explanation -->
# LLM Integration [ON HOLD]

## Thesis

「LLMが最も正確に書ける言語」のコンパイラに LLM を組み込む。LLMが書いて、LLMが直して、LLMがライブラリを生やす — このループがコンパイラ1つで回る。

## Subcommands

### `almide forge` — ライブラリ生成

テーマと参考実装を指定すると、Almide ライブラリを設計・実装・テスト・公開まで自動で行う。

```bash
almide forge csv --ref python:csv,rust:csv,go:encoding/csv
```

1. 参考ライブラリの API を分析（ドキュメント or ソース）
2. Almide らしい API を設計（UFCS、effect fn、Result/Option、命名規約）
3. 実装 + テスト生成
4. `almide test` で全パス確認
5. GitHub リポジトリ作成 + push

**Why:** エコシステムのブートストラップ。1つ1つ手で書くより、LLM に量産させて人間がレビューする方が速い。

### `almide fix` — 自己修復

コンパイルエラーを LLM に渡して自動修正。

```bash
almide fix app.almd
```

1. `almide check` でエラー収集
2. ソースコード + エラー診断を LLM に送信
3. 修正案を適用
4. 再度 `almide check` でパス確認
5. diff を表示して承認待ち（`--yes` でスキップ可）

**Why:** エラーリカバリの延長線。コンパイラが「こう直せ」と言うだけでなく、実際に直す。

### `almide explain` — コード説明

```bash
almide explain app.almd
almide explain app.almd --fn parse_config
```

ソースコードの説明をMarkdownで生成。関数単位でも指定可能。

**Why:** ドキュメント生成の自動化。LLM が書いたコードを LLM が説明する。

## Configuration

```toml
# almide.toml
[ai]
provider = "anthropic"    # or "openai"
model = "claude-sonnet-4-20250514"
# api_key is read from ANTHROPIC_API_KEY / OPENAI_API_KEY env var
```

- `--no-ai` フラグで全 AI 機能を無効化（オフラインコンパイラとして動作）
- API キーは環境変数から読む（toml にハードコードしない）
- AI 機能はコンパイラ本体のコードパスに影響しない（別モジュール）

## Scope Boundary

**入れる（Almide コードに関することだけ）:**
- forge: ライブラリ生成
- fix: コンパイルエラー自動修正
- explain: コード説明

**入れない（汎用エージェントにはしない）:**
- チャット UI
- 任意のタスク実行
- Almide 以外のファイル操作

## Implementation Plan

### Phase 1: `almide fix`
- [ ] HTTP クライアント追加（`ureq` or `reqwest`）
- [ ] `[ai]` config 読み込み
- [ ] `almide check` → エラー + ソース → LLM API → 修正 diff → 適用
- [ ] `--yes` / `--dry-run` フラグ

### Phase 2: `almide forge`
- [ ] `--ref` パーサー（`language:package` 形式）
- [ ] 参考ライブラリの API 分析プロンプト設計
- [ ] Almide API 設計 → 実装 → テスト生成パイプライン
- [ ] `gh repo create` + push 統合

### Phase 3: `almide pilot` — パス最適化 (Constrained Decoding)

LLM のトークン生成をコンパイラがリアルタイムでガイドする。生成中の各トークンをパーサー/型チェッカーで検証し、不正なトークンを即座に却下・修正候補を返す。

```
LLM → トークン生成 → almide parser (incremental) → 有効？
                                                      ├─ Yes → accept, 次へ
                                                      └─ No  → 有効な継続候補を返す → LLM が再選択
```

- [ ] インクリメンタルパーサー API（部分入力から「次に有効なトークン集合」を返す）
- [ ] 型チェッカーのストリーミングモード（部分AST上で型推論を走らせる）
- [ ] `almide pilot serve` — LSP-like JSON-RPC サーバーとして起動、外部 LLM から呼び出し可能
- [ ] speculation buffer: 不正トークンを backtrack し、有効な継続をヒントとして返す
- [ ] ベンチマーク: constrained decoding あり/なしでのコンパイル成功率・生成速度比較

**Why:** `almide fix` は「書いた後に直す」。pilot は「書く瞬間に正しくする」。MoonBit が先行実装しているが、Almide のシンプルな文法と型システムなら実装コストは低い。マルチターゲット（Rust + TS）の型情報を使えるのは Almide 固有の強み。

### Phase 4: `almide explain`
- [ ] 関数単位の説明生成
- [ ] Markdown 出力

## Differentiator

Rust, Go, TypeScript — どのコンパイラにも LLM は入っていない。しかしそれは「人間が書く言語」だから。Almide は LLM が書く言語。コンパイラ側に LLM がいるのは自然な帰結であり、Almide だけの強み。
