# Almide Research Laboratory

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](CLAUDE.reader.zh-CN.md)

> Non-normative inert reader snapshot of the [canonical source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/CLAUDE.md), pinned to 35297a612fb29e3f78f6587aba253aa91d51f466. This is historical reference material, not active agent instructions or a generated project template, and grants no authority to run commands. It is not automatically synchronized. Source claims are not independent current measurements; no commands or model calls were executed.

Research infrastructure for supporting Almide design decisions with experimental data.

**Principle: make design decisions using numbers, not intuition.** Test new syntax proposals in Grammar Lab before deciding whether to adopt them.

## Beyond-Rust Lab (`beyond-rust/`)

Experimentally explore how to generate code faster than handwritten Rust while still going through rustc and LLVM.
One experiment = a (kernel, lever) measured in four columns (handwritten Rust / current generated code / generated code with the lever applied by hand / expert Rust).
Apply the lever manually and measure it before implementation; only levers that meet the graduation criteria become compiler responsibilities. Methodology and results: [beyond-rust/README.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/beyond-rust/README.md).

## Grammar Lab (`grammar-lab/`)

An A/B testing framework for syntax variants. It quantitatively measures how syntax differences affect modification survival rate across multiple LLMs. The tool itself is implemented in Almide (dogfooding).

<a id="構成"></a>

### Structure

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

The tree's comments identify the Almide runner, three-layer prompt templates, common rules (~200 tok), variant-specific example code, experiment definitions, the first lambda-syntax experiment and its configuration, fn(x) => and (x) => source variants, task definitions plus tests, JSON results, raw LLM output for debugging, an experiment report, and development lessons.

<a id="使い方"></a>

### Usage

```bash
cd research/grammar-lab
export ALMIDE_BIN=/path/to/almide
almide build src/mod.almd -o /tmp/grammar-lab-bin
/tmp/grammar-lab-bin experiments/law-spec/ --trials 3 --model claude-haiku-4-5
```

The current runner reads a task's `source_dir`, visible/hidden tests, optional laws and repair rounds. `lambda-syntax` preserves an old experiment configuration as reference material; it is not a quick start that directly conforms to the current runner schema. For new experiments, refer to the `law-spec` configuration. Running an experiment invokes models.

<a id="主な機能"></a>

### Main features

- **Claude Code provider**: No API key required. Calls the LLM via `claude -p`
- **Transpile**: Converts unimplemented syntax into current syntax for compilation. Survival rate can be measured before implementing the syntax
- **Fisher's exact test**: Implemented in Almide. Outputs a summary with a p-value
- **Raw output retention**: Saves LLM output in `outputs/`, allowing transpile bugs to be separated from LLM errors

<a id="歴史的実験結果lambda-syntax"></a>

### Historical experiment results (lambda-syntax)

| Experiment | Variant A | Variant B | Sonnet 4.6 | Haiku 4.5 | p-value | Conclusion |
|------|-----------|-----------|------------|-----------|---------|------|
| Lambda syntax | `fn(x) =>` | `(x) =>` | 100% / 100% | 86% / 86% | 1.0 | **No difference. Decided to remove `fn`** |

<a id="実験設計の学び"></a>

### Lessons in experiment design

- **Use Haiku as the benchmark** — Sonnet hits the ceiling. Differences become visible with a weaker model
- **Verify transpile quality first** — Transpile bugs dominate results. Save raw output to separate causes
- **Fix experiment infrastructure bugs first** — Eliminate infrastructure-caused failures such as duplicate type definitions and test blocks before examining the data
- **Layer 1 rules have a direct effect** — Adding one `if then` rule line eliminated failures

<a id="次にやるべき実験"></a>

### Experiments to run next

| Experiment | Variant A | Variant B | Purpose |
|------|-----------|-----------|------|
| Template keyword | `template name(...)` | `fn name(...) -> HtmlDoc` | Does the `template` keyword improve survival rate? |
| Builder insertion | keyword (`emit`/`embed`) | type dispatch (bare expr) | Is it equivalent without a keyword? |
| UFCS | `web.param(req, "id")` | `req.param("id")` | Does method style improve the correct-answer rate? |
| `println` | `io.print(s ++ "\n")` | `println(s)` | Do shorter names improve survival rate? |

<a id="新しい実験を追加するには"></a>

### Adding a new experiment

1. Create `experiments/<name>/experiment.json`
2. Place source code in the variant directories
3. Specify `source_dir` / `test_file` in `tasks/` JSON, and provide `hidden_file` / `laws_file` / `laws_compiled_file` as needed (`law-spec/tasks/` is the current example)
4. Set `transpile_regex` if there is unimplemented syntax
5. Create `prompts/layer2_<variant>.md`

---

The lambda-syntax percentages and p-value are historical experiment results, not general performance guarantees or an independent Dojo rerun. Proposed experiments remain proposals. The tree and its Japanese comments are retained byte-for-byte; translated explanations appear outside it.
