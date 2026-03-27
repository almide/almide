<!-- description: A/B testing infrastructure for syntax design using LLM benchmarks -->
<!-- done: 2026-03-15 -->
# Grammar Research Infrastructure

## Vision

Almide の文法設計判断を「勘」ではなく「数字」で回す。

複数の LLM に対して構文バリアントの A/B テストを自動実行し、modification survival rate への影響を定量的に測定する基盤。ツール自体を Almide で実装する（dogfooding）。

## Problem

現在の文法設計判断は議論と直感に基づいている。例:

- `fn(x) => expr` vs `(x) => expr` — どちらが LLM に壊されにくいか？
- `emit expr` vs bare expression — 明示 keyword は survival rate を上げるか？
- `web.param(req, "id")` vs `req.param("id")` — UFCS は LLM の正答率を上げるか？

これらの問いに「実験して数字で答える」仕組みがない。

## Core Concept

```
Experiment = {
  hypothesis: "短縮 lambda は modification survival rate を下げない"
  variants: [
    A: { syntax: fn(x) => expr,  corpus: [...tasks...] }
    B: { syntax: (x) => expr,    corpus: [...tasks...] }
  ]
  models: [claude-sonnet, gpt-4o, gemini-pro, ...]
  trials: 30
  metric: modification survival rate (compile + test pass)
}
```

1. 同じ意味のコードを各構文バリアントで用意
2. LLM に修正タスクを与える
3. compile → test → pass/fail を自動判定
4. バリアント間の survival rate を比較
5. 統計的有意差を検定

## Architecture

```
grammar-research/
├── src/
│   ├── mod.almd              // エントリーポイント
│   ├── experiment.almd       // Experiment 定義・実行
│   ├── variant.almd          // 構文バリアント管理
│   ├── runner.almd           // LLM 実行 (API 呼び出し)
│   ├── evaluator.almd        // compile + test 自動評価
│   ├── stats.almd            // 統計分析・有意差検定
│   └── report.almd           // 結果レポート生成
├── experiments/
│   ├── lambda-syntax/        // fn(x) => vs (x) =>
│   ├── emit-vs-bare/         // emit keyword vs type dispatch
│   ├── ufcs-external/        // module prefix vs UFCS
│   ├── builder-keyword/      // keyword あり vs なし
│   └── ...
├── corpus/                   // タスク定義
│   ├── tasks.toml            // タスクメタデータ
│   └── tasks/                // 個別タスク (.almd + spec)
└── results/                  // 実験結果 (JSON)
```

## Data Model

```almide
type Experiment = {
  name: String
  hypothesis: String
  variants: List[Variant]
  models: List[ModelConfig]
  trials_per_variant: Int
  tasks: List[Task]
}

type Variant = {
  name: String                    // "fn-lambda" / "paren-lambda"
  description: String
  corpus_dir: String              // 構文バリアント版コーパスのパス
}

type ModelConfig = {
  name: String                    // "claude-sonnet-4-6"
  provider: Provider              // Anthropic / OpenAI / Google
  temperature: Float
}

type Provider = | Anthropic | OpenAI | Google

type Task = {
  name: String
  description: String             // LLM に渡す修正指示
  base_file: String               // 修正対象のファイル
  test_file: String               // テストファイル
  max_attempts: Int               // retry 回数
}

type TrialResult = {
  experiment: String
  variant: String
  model: String
  task: String
  trial: Int
  compiled: Bool
  tests_passed: Bool
  error_type: Option[ErrorType]
  llm_output: String
  duration_ms: Int
}

type ErrorType =
  | SyntaxError { message: String }
  | TypeError { message: String }
  | TestFailure { failed: List[String] }
  | Timeout
  | ApiError { message: String }
```

## Experiment Workflow

### 1. Define Experiment

```toml
# experiments/lambda-syntax/experiment.toml

name = "lambda-syntax"
hypothesis = "Short lambda (x) => does not degrade modification survival rate vs fn(x) =>"

[[variants]]
name = "fn-lambda"
description = "Current syntax: fn(x) => expr"
corpus_dir = "experiments/lambda-syntax/fn-lambda/"

[[variants]]
name = "paren-lambda"
description = "Short syntax: (x) => expr"
corpus_dir = "experiments/lambda-syntax/paren-lambda/"

[[models]]
name = "claude-sonnet-4-6"
provider = "anthropic"
temperature = 0.0

[[models]]
name = "gpt-4o"
provider = "openai"
temperature = 0.0

trials_per_variant = 30
```

### 2. Prepare Corpus

同じロジック、異なる構文:

```almide
// experiments/lambda-syntax/fn-lambda/map_filter.almd
let result = [1, 2, 3, 4, 5]
  |> list.filter(fn(x) => x > 2)
  |> list.map(fn(x) => x * 10)

// experiments/lambda-syntax/paren-lambda/map_filter.almd
let result = [1, 2, 3, 4, 5]
  |> list.filter((x) => x > 2)
  |> list.map((x) => x * 10)
```

タスク: 「`x > 2` を `x > 3` に変更し、`x * 10` を `x * 100` に変更せよ」

### 3. Run

```bash
almide run src/mod.almd -- run experiments/lambda-syntax/
```

各 variant × model × task × trial を実行。LLM に修正指示 + コードを渡し、返ってきたコードを compile + test。

### 4. Evaluate

```almide
// evaluator.almd
effect fn evaluate(output: String, task: Task) -> TrialResult = {
  // 1. LLM 出力からコードを抽出
  let code = extract_code(output)

  // 2. 一時ファイルに書き出し
  fs.write(task.base_file, code)

  // 3. almide check (compile)
  let check = process.run("almide", ["check", task.base_file])
  guard check.exit_code == 0 else return trial_result(compiled: false, error: parse_error(check.stderr))

  // 4. almide test (test)
  let test = process.run("almide", ["test", task.test_file])
  guard test.exit_code == 0 else return trial_result(tests_passed: false, error: parse_test_failure(test.stderr))

  // 5. 成功
  trial_result(compiled: true, tests_passed: true)
}
```

### 5. Analyze

```almide
// stats.almd

// Survival rate per variant per model
fn survival_rate(results: List[TrialResult]) -> Float =
  let passed = results |> list.filter(fn(r) => r.tests_passed) |> list.len()
  int.to_float(passed) / int.to_float(list.len(results))

// Fisher's exact test for significance
fn fishers_exact(a_pass: Int, a_fail: Int, b_pass: Int, b_fail: Int) -> Float = ...

// Error type distribution
fn error_distribution(results: List[TrialResult]) -> Map[String, Int] = ...
```

### 6. Report

```
=== Lambda Syntax Experiment ===

Hypothesis: Short lambda (x) => does not degrade modification survival rate

Results (30 trials each):

                    fn(x) =>    (x) =>     p-value
claude-sonnet-4-6   87% (26/30) 90% (27/30) 0.73
gpt-4o              80% (24/30) 83% (25/30) 0.68
gemini-pro          73% (22/30) 77% (23/30) 0.61

Conclusion: No significant difference. Short lambda is safe to adopt.

Error breakdown (fn-lambda):
  SyntaxError:  3
  TypeError:    1
  TestFailure:  0

Error breakdown (paren-lambda):
  SyntaxError:  2
  TypeError:    1
  TestFailure:  0
```

## Planned Experiments

### Phase 1: 基礎実験

| 実験 | Variant A | Variant B | 仮説 |
|------|-----------|-----------|------|
| Lambda 構文 | `fn(x) => expr` | `(x) => expr` | 短縮形は survival rate を下げない |
| Builder insertion | `emit expr` keyword | bare expr (type dispatch) | keyword なしでも survival rate は同等 |
| UFCS | `web.param(req, "id")` | `req.param("id")` | UFCS は survival rate を上げる |
| Module prefix | `web.get(...)` | `get(...)` (selective import) | prefix 省略は survival rate を下げない |

### Phase 2: Template 構文実験

| 実験 | Variant A | Variant B | 仮説 |
|------|-----------|-----------|------|
| Template keyword | `template name(...)` | `fn name(...) -> HtmlDoc` | `template` keyword は survival rate を上げる |
| HTML tag 認識 | Known tags only | 任意識別子 | Known tags 制限は typo を減らす |
| Builder block style | `html { div { ... } }` | JSX 風 `<div>...</div>` | Builder block の方が壊れにくい |

### Phase 3: 言語横断実験

| 実験 | 内容 |
|------|------|
| Almide vs TypeScript | 同じタスクを Almide と TS で書かせ、iterative edit の survival rate を比較 |
| Almide vs Python | 同上 |
| Almide vs Go | 同上 |

## Self-Hosting: Almide で実装

ツール自体を Almide で書く理由:

1. **Dogfooding** — Almide の実用性を自分で証明
2. **Template 統合** — レポート生成を `template` で書ける
3. **Codec 統合** — 実験結果の JSON serialize/deserialize を `deriving Codec` で
4. **HTTP client** — LLM API 呼び出しに stdlib の `http.request` を使う
5. **Process module** — `almide check` / `almide test` の実行に `process.run` を使う

### 実装に必要な Almide 機能

| 機能 | ステータス | 必要度 |
|------|-----------|--------|
| HTTP client (`http.request`) | ✅ done | 必須 (LLM API) |
| JSON parse/stringify | ✅ done | 必須 |
| File I/O (`fs.read_text`, `fs.write`) | ✅ done | 必須 |
| Process execution (`process.run`) | ✅ done | 必須 (almide check/test) |
| `deriving Codec` | active (未実装) | Phase 2 で使う |
| Template | active (未実装) | レポート生成で使う |
| CLI args | ✅ done | 必須 |

**Phase 1 は今の Almide で実装可能。** Codec / Template は Phase 2 で統合。

## Implementation Order

### Phase 1: MVP (今の Almide で可能)

- Experiment TOML parser (json.parse ベース、または手動 parse)
- Single model × single variant の実行ループ
- almide check + almide test の自動評価
- CSV / JSON 結果出力
- 最初の実験: lambda syntax A/B

### Phase 2: Multi-model + Statistics

- 複数 LLM provider 対応 (Anthropic, OpenAI, Google)
- 並列実行 (async let)
- Fisher's exact test / chi-squared test
- テキストレポート生成

### Phase 3: Codec + Template 統合

- `deriving Codec` で実験定義・結果を型安全に
- `template` でレポート HTML 生成
- Web UI (web framework で結果閲覧)

### Phase 4: CI 統合

- 文法変更 PR に対して自動で survival rate 実験を走らせる
- regression detection: 新構文が survival rate を下げたら warning

## Relationship to Existing Research

既存の [Research: Modification Survival Rate Paper](../on-hold/research-modification-survival-rate-paper.md) は **言語間比較** に焦点。本 roadmap は **同一言語内の構文バリアント比較** に焦点。

両者は補完的:
- Paper: 「Almide は他言語より modification survival rate が高い」を示す
- Grammar Research: 「Almide 内のどの構文選択が survival rate に寄与するか」を示す

Paper の ablation study セクションは本 roadmap の実験結果で書ける。

## Success Criteria

- 文法設計の議論で「実験 X の結果、Variant A の survival rate は Y% で Variant B より Z% 高い」と言える
- 新構文追加時に A/B 実験を走らせるのが標準プロセスになる
- Almide の Mission（"the language LLMs can write most accurately"）を数値で裏付けられる
