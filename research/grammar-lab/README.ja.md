# Grammar Lab

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](./README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](./README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](./README.zh-CN.md)

構文の選択が LLM の modification survival rate にどう影響するかを測る A/B テストフレームワーク。

## クイックスタート

```bash
cd research/grammar-lab

# Set paths (required if not in PATH)
export ALMIDE_BIN=/path/to/almide          # or add almide to PATH
export CLAUDE_BIN=/path/to/claude          # optional, auto-detected via `which`

# Build the runner
almide build src/mod.almd -o /tmp/grammar-lab-bin

# Run the current-schema experiment (configured models, 3 trials; invokes the model)
/tmp/grammar-lab-bin experiments/law-spec/

# Run with specific model
/tmp/grammar-lab-bin experiments/law-spec/ --model claude-haiku-4-5

# Override trial count
/tmp/grammar-lab-bin experiments/law-spec/ --trials 1
```

### パスの解決

バイナリのパスは次の順で解決する。
1. 環境変数（`ALMIDE_BIN`、`CLAUDE_BIN`）
2. `which` による検索（バイナリが PATH にある場合）
3. フォールバック（相対パス `../../target/release/almide`）

## 現行スキーマと過去の実験

`src/mod.almd::load_task` は各タスクの `source_dir` と `test_file`、および任意の `hidden_file`、`laws_file`、`laws_compiled_file` を読む。バリアントは `with_laws` を設定でき、実験では修復ラウンドを要求できる。`law-spec` はこのスキーマを備えている。以下の `lambda-syntax` は過去の実験であり、旧タスク JSON と `paren-to-fn-lambda` 実験を、そのまま現行コンパイラのクイックスタートとして使うことはできない。ランナー自体は `almide check src/mod.almd` によりモデルを呼び出さずに確認できる。

## 過去の lambda-syntax の構成

```
grammar-lab/
├── src/mod.almd                    # Runner (Almide)
├── prompts/
│   ├── layer1_rules.md             # Common rules (~200 tok)
│   ├── layer2_variant_fn.md        # Examples for fn(x) => variant
│   └── layer2_variant_paren.md     # Examples for (x) => variant
├── experiments/
│   └── lambda-syntax/
│       ├── experiment.json         # Experiment config
│       ├── variant-fn/             # Source files using fn(x) =>
│       ├── variant-paren/          # Source files using (x) =>
│       └── tasks/                  # Task definitions + test files
└── results/                        # Output JSON
```

## プロンプトの設計

研究に基づく三層構成：

| 層 | 内容 | 変わる単位 |
|-------|---------|-----------|
| Layer 1 | Almide の規則（よくある間違い） | 変わらない（共通） |
| Layer 2 | コード例（3 関数） | バリアント |
| Layer 3 | タスク固有の関数ヒント | タスク |

設計の参考：
- 「優れた例一つは五つの形容詞に勝る」（Anthropic）
- Grammar Prompting（NeurIPS 2023）：タスクごとの最小限の文法
- Few-shot の適切な範囲：2-5 例
- システムプロンプトの適切な範囲：150-300 語

## 出力

結果は JSON 配列となる。

```json
[
  {
    "experiment": "law-spec",
    "variant": "laws",
    "model": "claude-haiku-4-5",
    "task": "t01_rle",
    "trial": 1,
    "compiled_1": true,
    "gate_1": true,
    "hidden_1": true,
    "repaired": false,
    "gate_2": true,
    "hidden_2": true
  }
]
```

可視テスト／law のゲートに合格しても隠しテストに失敗した場合は、"survived but wrong" と報告する。ゲート合格だけでは正しさを意味しない。上記の例は現行の出力スキーマを示すものであり、実測した試行ではない。

## 実験の追加

1. `experiments/<name>/experiment.json` を作成する。
2. ソース `.almd` ファイルを含むバリアントのディレクトリを作成する。
3. `law-spec/tasks/` に従い、`source_dir`、`test_file`、任意の hidden/law パスを持つ `tasks/` JSON を作成する。ソースのパスは実験を基準とし、test/law のパスは `tasks/` を基準とする。
4. バリアント固有の `prompts/layer2_<variant>.md` を追加する。
5. 実行：`almide run src/mod.almd -- experiments/<name>/`
