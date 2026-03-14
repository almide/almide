# Almide Research Laboratory

Almide の設計判断を実験データで裏付けるための研究基盤。

**原則: 設計判断は「勘」ではなく「数字」で行う。** 新しい構文提案は Grammar Lab で実験してから採否を決める。

## Grammar Lab (`grammar-lab/`)

構文バリアントの A/B テストフレームワーク。複数の LLM に対して構文の違いが modification survival rate に与える影響を定量的に測定する。ツール自体が Almide で実装されている（dogfooding）。

### 構成

```
grammar-lab/
├── src/mod.almd              Runner (Almide)
├── prompts/                  Prompt テンプレート (3 層設計)
│   ├── layer1_rules.md       共通ルール (~200 tok)
│   └── layer2_*.md           Variant 別の例示コード
├── experiments/              実験定義
│   └── lambda-syntax/        最初の実験
│       ├── experiment.json   実験設定
│       ├── variant-fn/       fn(x) => 版のソースコード
│       ├── variant-paren/    (x) => 版のソースコード
│       └── tasks/            タスク定義 + テスト
├── results/                  実験結果 (JSON)
├── outputs/                  LLM の生出力 (デバッグ用)
├── REPORT.md                 実験レポート
└── LESSONS.md                開発で学んだ知見
```

### 使い方

```bash
cd research/grammar-lab
export ALMIDE_BIN=/path/to/almide
almide build src/mod.almd -o /tmp/grammar-lab-bin
/tmp/grammar-lab-bin experiments/lambda-syntax/ --trials 5 --model claude-haiku-4-5
```

### 主な機能

- **Claude Code provider**: API key 不要。`claude -p` 経由で LLM を呼び出す
- **Transpile**: 未実装構文を現行構文に変換してコンパイル。構文を実装する前に survival rate を測定可能
- **Fisher's exact test**: Almide で実装。p 値付きで summary を出力
- **生出力保存**: `outputs/` に LLM の出力を保存。transpile バグと LLM エラーを切り分け可能

### 実験結果

| 実験 | Variant A | Variant B | Sonnet 4.6 | Haiku 4.5 | p-value | 結論 |
|------|-----------|-----------|------------|-----------|---------|------|
| Lambda syntax | `fn(x) =>` | `(x) =>` | 100% / 100% | 86% / 86% | 1.0 | **差なし。`fn` 廃止を決定** |

### 実験設計の学び

- **Haiku をベンチマークに使う** — Sonnet は天井に張り付く。弱いモデルで差が見える
- **transpile の品質を先に確認する** — transpile バグが結果を支配する。生出力を保存して切り分ける
- **実験基盤のバグを先に潰す** — 型二重定義、test block 重複などインフラ起因の fail を排除してからデータを見る
- **Layer 1 ルールは直接効く** — `if then` ルール 1 行追加で fail が消えた

### 次にやるべき実験

| 実験 | Variant A | Variant B | 目的 |
|------|-----------|-----------|------|
| Template keyword | `template name(...)` | `fn name(...) -> HtmlDoc` | `template` keyword は survival rate を上げるか |
| Builder insertion | keyword (`emit`/`embed`) | type dispatch (bare expr) | keyword なしでも同等か |
| UFCS | `web.param(req, "id")` | `req.param("id")` | method style は正答率を上げるか |
| `println` | `io.print(s ++ "\n")` | `println(s)` | 短い名前は survival rate を上げるか |

### 新しい実験を追加するには

1. `experiments/<name>/experiment.json` を作成
2. variant ディレクトリにソースコードを配置
3. `tasks/` にタスク定義 + テストファイルを配置
4. 未実装構文がある場合は `transpile_regex` を設定
5. `prompts/layer2_<variant>.md` を作成
