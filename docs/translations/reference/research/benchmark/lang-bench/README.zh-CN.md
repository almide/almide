<a id="ai-coding-language-benchmark--almide-runner"></a>

# AI Coding Language Benchmark — Almide Runner

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](README.zh-CN.md)

> 固定原文的非规范性历史阅读快照，以原本为准。不自动同步。所述测试、测量、实现状态和保证是原文主张，不是独立重跑或当前验证。未执行引用代码或命令。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/lang-bench/README.md)
> 历史 Sonnet 5 标签按原文主张保留，未独立验证模型可用性。后段饱和陈述称所有语言20/20，但带日期表格记载 Gleam v2 为19/20；两者均保留，不杜撰调和解释。MiniGit one-shot 试验允许 session 内对照 oracle 迭代，不是单次生成或 hidden-oracle MSR 测量。调用模型的命令可能收费，未运行。

为 Almide 复现 [mame/ai-coding-lang-bench](https://github.com/mame/ai-coding-lang-bench) 的 minigit 任务，并将结果汇总进图表数据。

<a id="layout"></a>

## 布局

| 路径 | 用途 | 已提交？ |
|---|---|---|
| `data.json` | 全部语言汇总统计（plot.py 输入） | 是 |
| `plot.py` | 生成：`docs/figures/lang-bench-*.png` | 是 |
| `upstream/` | Git submodule → `mame/ai-coding-lang-bench` （SPEC、测试 script） | 是（pointer） |
| `runner.rb` | 针对 Almide 运行 N 次 minigit 试验 | 是 |
| `aggregate.rb` | 重新计算 Almide 统计，写入  `data.json`，来源 `raw/almide.jsonl` | 是 |
| `raw/almide.jsonl` | 仅追加试验记录（每行一个 JSON） | 是 |
| `.work/`, `.logs/` | 逐次试验生成的源码与 Claude log | 否（gitignore 排除） |

<a id="setup"></a>

## 设置

```bash
git submodule update --init research/benchmark/lang-bench/upstream
```

需要：`claude` CLI、PATH 中的 `almide`、`ruby`、`python3` + `matplotlib`（绘图用）。

<a id="run-trials"></a>

## 运行试验

```bash
ruby research/benchmark/lang-bench/runner.rb --trials 10
```

每次试验分两阶段：

- **v1** — Claude（runner 请求不固定版本的 `sonnet` 别名）实现 `init`、`add`、`commit`、`log`。
- **v2** — Claude 为 v1 扩展 `checkout`、`reset`。

每次试验预计约 20–30 分钟。试验编号按 `raw/almide.jsonl` 已有记录自动递增。

Dry run（不调用 LLM）：

```bash
ruby research/benchmark/lang-bench/runner.rb --trials 1 --dry-run
```

<a id="update-charts"></a>

## 更新图表

```bash
ruby research/benchmark/lang-bench/aggregate.rb   # raw/ → data.json Almide entry
python3 research/benchmark/lang-bench/plot.py     # data.json → docs/figures/lang-bench-*.png
```

`plot.py` 会尝试为匹配的 README 图片 URL 破除缓存；如果没有这些图片，例如当前首页 README，则无操作。发布的测量位于 `docs/benchmarks/README.md`；其带日期快照不同于这里生成的实时图表。

<a id="data-notes"></a>

## 数据注记

- `data.json` 中另外 15 种语言来自 upstream benchmark（Opus 4.6，各 20 次试验）。这里不重新生成这些数值。
- `data.json` 的 `languages[]` 中 Almide 条目在 2026-07-15 用下方同模型快照（**Sonnet 5**，20 次试验）刷新。保留这一历史模型标签，但它不是当前 runner 的版本固定值。本 benchmark 无法确立是否收录于训练数据。
- `upstream/benchmark.rb` 是原样 upstream runner，语言列表**不包含** Almide。本目录 `runner.rb` 是我们针对 Almide 的替代，只引用 `upstream/SPEC-v*.txt` 与 `upstream/test-v*.sh`。

<a id="same-model-snapshot-2026-07"></a>

## 同模型快照（2026-07）

对 Almide、现代同类语言（Gleam、MoonBit）及两个主流基准（Rust、TypeScript）进行同条件 one-shot 比较：**一个模型（`claude-sonnet-5`）、每种语言 20 次试验、相同 prompt 和 harness**，于 2026-07-15 经 `runner_multi.rb` 运行。这消除了历史图表标签注明的模型不一致。

| 语言 | Toolchain | v1 通过 | v2 通过 | 平均时间 (s) | v2 LOC | 平均成本 |
|---|---|---|---|---|---|---|
| Almide | 0.29.0 | 20/20 | **20/20** | 573 ± 112 | **233 ± 27** | $3.19 |
| Gleam | 1.15.2 | 20/20 | 19/20 | 715 ± 258 | 394 ± 27 | $2.66 |
| MoonBit | moon 0.1.20260330 | 20/20 | 20/20 | 962 ± 358 | 448 ± 54 | $5.28 |
| Rust | rustc 1.94.1 | 20/20 | 20/20 | 297 ± 89 | 351 ± 18 | $1.45 |
| TypeScript | tsx 4.23.1 | 20/20 | 20/20 | 297 ± 99 | 386 ± 138 | $1.44 |

图表：`docs/figures/lang-bench-snapshot-2026-07.png`。汇总数据在 `data.json` 的 `snapshot_2026_07` 下；逐次原始记录在 `raw/<lang>-sonnet5.jsonl`（已越过 `raw/*` ignore 强制添加并提交）。

方法注记：

- prompt 原样取自 upstream `benchmark.rb`。Almide 额外在工作目录收到 `CHEATSHEET.md` 和构建命令提示（这是提供的上下文记录，不是已验证该语言不存在于模型训练数据的主张）。
- **基础设施重试策略**：`claude` CLI 在尝试任务前死亡的试验（≤4 agent turn、写入 0 LOC、执行零测试）重新运行一次；原记录连同原因隔离到 `raw/retired/`。4/100 次受影响（rust 5 & 11、gleam 16、moonbit 8）。真正的模型失败从不重试。
- 快照唯一的真正失败是 Gleam 试验 20 v2：模型的 `checkout`/`reset` 实现在执行的 26 项测试中失败 12 项（suite 在汇总行之前中止，harness 因而记录 0/0；手动重跑确认失败，见记录的 `v2_note`）。
- 时间在 6–9 倍自身并行度下测量（全部语言同时运行），因此在快照内部可比较，但相较逐次 upstream 数值偏保守。

复现：

```bash
ruby runner_multi.rb --lang gleam --trials 20      # one language
ruby aggregate_multi.rb                            # summary table
python3 plot_snapshot.py                           # regenerate the chart
```

`runner_multi.rb` 也支持 `go`（`go version`）和 `zig`（`zig version`），因此 MiniGit lane 可扩展到当前比较集合；两者都尚无已提交的 20 次试验快照。

<a id="modification-survival-lane"></a>

## Modification-survival lane

MiniGit 已饱和（每种语言的 v1 和 v2 都是 20/20 — [#1963](https://github.com/almide/almide/issues/1963)）：规格与评分 script 都交给模型，模型因此在 session 内对照 oracle 迭代，也没有可破坏的跨文件不变量。它测量简洁性与成本，而非修改存活。

有区分力的 lane 是另一组任务：每种语言一个 seed program，加上有序编辑序列；每次编辑用模型看不到的 hidden oracle 评分，采用 prefix survival（某次编辑失败会令之后所有 horizon 失败）。任务在 Dojo（[almide/almide-dojo](https://github.com/almide/almide-dojo) 的 `tasks/xlang/`；全部 MSR task material 属于那里，见顶层 `CLAUDE.md` 的 repo-boundary rule）；跨语言 driver 则放在这里的 lang-bench，因为按规则 Dojo harness 只针对 Almide。设计、评分合取、ledger 格式与运行成本列在 #1963。

| 路径 | 用途 |
|---|---|
| `lib/bench_common.rb` | `run_cmd` / `run_claude` / `run_tests` / `count_loc` / `parse_claude_json`，由两个 runner 共享 |
| `runner_edits.rb` | task-dir 驱动 driver：seed + 有序编辑、累积 tree、hidden oracle、prefix survival； `--dry-run` 为 bank gate |
| `aggregate_edits.rb` | ledger：带 Wilson 95 % CI 的 MSR-seq、MSR-edit、首次失败模式、$/sequence；打印 `INCONCLUSIVE_BANK_SATURATED`（到达上限时） |
| `tasks-example/xlang/` | 用于本地执行 gate 的双语言 smoke fixture |
| `test/aggregate_edits_test.rb` | `ruby -Itest test/aggregate_edits_test.rb` |

```bash
ruby runner_edits.rb --tasks tasks-example/xlang --dry-run        # gate, no model call
ruby runner_edits.rb --tasks ../../../../almide-dojo/tasks/xlang --lang rust --trials 5   # measure (spends money)
ruby aggregate_edits.rb --tasks ../../../../almide-dojo/tasks/xlang                        # ledger
```

对每次编辑 `k`：`survive_k = compile ∧ visible ∧ hidden ∧ scope_clean`，其中 `scope_clean` 表示模型改动的每个文件（相对编辑前快照的 git diff，包含新增文件）都匹配 `scope.txt` 中的 glob。tree 契约与语言无关：`bash build.sh` 在 `./app` 留下可执行文件；`visible.sh` / `hidden.sh` 以 tree 为 cwd 运行，经 stdin 驱动 `./app` 并报告 `PASSED:`/`FAILED:`。对解释型语言，`build.sh` 进行类型检查并写出 `./app` wrapper script。

> 命令注释译文（命令保持原文）：raw/ 到 data.json 的 Almide 条目／data.json 到 docs/figures/lang-bench-*.png／一种语言／汇总表／重新生成图表／gate，不调用模型／测量（会花钱）／ledger。
