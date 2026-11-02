<a id="ai-coding-language-benchmark--almide-runner"></a>

# AI Coding Language Benchmark — Almide Runner

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](README.zh-CN.md)

> 固定原文の非規範的な履歴参照資料であり、原本を正本とする。自動同期しない。記載のテスト・測定・実装状態・保証は原文の主張であり、独立した再実行や現行の検証ではない。引用コードとコマンドは実行していない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/lang-bench/README.md)
> 歴史的 Sonnet 5 表記は原文主張として保持し、モデル提供状況を独立検証したものではない。後段の飽和説明は全言語20/20とする一方、日付付き表は Gleam v2 を19/20と記録する。両記述を保持し、整合する説明を捏造しない。MiniGit の one-shot 試行は session 内の oracle 反復を許し、単一生成や hidden-oracle MSR 測定ではない。モデルを呼ぶコマンドは課金され得るため実行していない。

[mame/ai-coding-lang-bench](https://github.com/mame/ai-coding-lang-bench) の minigit タスクを Almide 向けに再現し、結果をチャートデータに集計する。

<a id="layout"></a>

## 配置

| パス | 用途 | commit 済み？ |
|---|---|---|
| `data.json` | 全言語の集計統計（plot.py 入力） | はい |
| `plot.py` | 生成するもの：`docs/figures/lang-bench-*.png` | はい |
| `upstream/` | Git submodule → `mame/ai-coding-lang-bench` （SPEC、テスト script） | はい（pointer） |
| `runner.rb` | Almide に対して minigit タスクを N 回試行 | はい |
| `aggregate.rb` | Almide 統計の再計算先： `data.json`、入力：`raw/almide.jsonl` | はい |
| `raw/almide.jsonl` | 追記専用の試行記録（1 行 1 JSON） | はい |
| `.work/`, `.logs/` | 試行ごとの生成ソースと Claude log | いいえ（gitignore 対象） |

<a id="setup"></a>

## セットアップ

```bash
git submodule update --init research/benchmark/lang-bench/upstream
```

必要なもの：`claude` CLI、PATH 上の `almide`、`ruby`、`python3` + `matplotlib`（plot 用）。

<a id="run-trials"></a>

## 試行の実行

```bash
ruby research/benchmark/lang-bench/runner.rb --trials 10
```

各試行は 2 段階で実行する。

- **v1** — Claude（runner は版を固定しない `sonnet` alias を要求する）が `init`、`add`、`commit`、`log` を実装する。
- **v2** — Claude が v1 に `checkout`、`reset` を追加する。

1 試行あたり約 20–30 分を見込む。試行番号は `raw/almide.jsonl` の既存記録に基づき自動で増える。

Dry run（LLM 呼び出しなし）：

```bash
ruby research/benchmark/lang-bench/runner.rb --trials 1 --dry-run
```

<a id="update-charts"></a>

## チャート更新

```bash
ruby research/benchmark/lang-bench/aggregate.rb   # raw/ → data.json Almide entry
python3 research/benchmark/lang-bench/plot.py     # data.json → docs/figures/lang-bench-*.png
```

`plot.py` は、一致する README 画像 URL の cache busting を試みる。現在の先頭 README のように該当画像がなければ何もしない。公開された測定は `docs/benchmarks/README.md` にある。その日付付き snapshot は、ここで生成する live chart とは別物である。

<a id="data-notes"></a>

## データの注記

- `data.json` 内の他の 15 言語は upstream benchmark 由来（Opus 4.6、各 20 試行）。それらの数値はここでは再生成しない。
- `data.json` の `languages[]` にある Almide entry は、2026-07-15 に下記の同一モデル snapshot（**Sonnet 5**、20 試行）から更新した。この歴史的なモデル名は保持するが、現在の runner の版固定ではない。この benchmark から学習データへの収録有無は確立できない。
- `upstream/benchmark.rb` は無改変の upstream runner であり、言語一覧に Almide を**含まない**。このディレクトリの `runner.rb` は Almide 専用の代替で、`upstream/SPEC-v*.txt` と `upstream/test-v*.sh` だけを参照する。

<a id="same-model-snapshot-2026-07"></a>

## 同一モデル snapshot（2026-07）

Almide と現代的な同類言語（Gleam、MoonBit）、および主流の比較基準 2 言語（Rust、TypeScript）を、同じ条件で one-shot 比較した。**1 モデル（`claude-sonnet-5`）、言語ごとに 20 試行、同一の prompt と harness**で、2026-07-15 に `runner_multi.rb` を通して実行した。これにより、過去のチャートがラベルに明記するモデル不一致をなくしている。

| 言語 | Toolchain | v1 合格 | v2 合格 | 平均時間 (s) | v2 LOC | 平均費用 |
|---|---|---|---|---|---|---|
| Almide | 0.29.0 | 20/20 | **20/20** | 573 ± 112 | **233 ± 27** | $3.19 |
| Gleam | 1.15.2 | 20/20 | 19/20 | 715 ± 258 | 394 ± 27 | $2.66 |
| MoonBit | moon 0.1.20260330 | 20/20 | 20/20 | 962 ± 358 | 448 ± 54 | $5.28 |
| Rust | rustc 1.94.1 | 20/20 | 20/20 | 297 ± 89 | 351 ± 18 | $1.45 |
| TypeScript | tsx 4.23.1 | 20/20 | 20/20 | 297 ± 99 | 386 ± 138 | $1.44 |

チャート：`docs/figures/lang-bench-snapshot-2026-07.png`。集計データは `data.json` の `snapshot_2026_07` にあり、試行ごとの生記録は `raw/<lang>-sonnet5.jsonl` にある（`raw/*` ignore を越えて force-add し commit 済み）。

方法に関する注記：

- prompt は upstream の `benchmark.rb` のものをそのまま使う。Almide にはさらに作業ディレクトリに `CHEATSHEET.md` と build-command hint を与える（これは供給した context の記述であり、言語がモデルの学習データに存在しなかったことを検証済みと主張するものではない）。
- **インフラ再試行方針**：`claude` CLI がタスク着手前に停止した試行（agent turn ≤4、記述 LOC 0、実行テスト 0）は一度再実行した。元記録は理由付きで `raw/retired/` に隔離した。4/100 試行が対象（rust 5 & 11、gleam 16、moonbit 8）。本当のモデル失敗は一度も再試行していない。
- snapshot 内の唯一の本当の失敗は Gleam 試行 20 の v2。モデルの `checkout`/`reset` 実装が、実行した 26 テストのうち 12 に失敗する（suite が集計行より前に中断したため harness は 0/0 と記録したが、手動再実行で失敗を確認した。記録の `v2_note` 参照）。
- 時間は 6–9 倍の自己並列実行（全言語を同時実行）下で測定したため、snapshot 内では比較できるが、逐次 upstream 数値に対しては保守的な値である。

再現：

```bash
ruby runner_multi.rb --lang gleam --trials 20      # one language
ruby aggregate_multi.rb                            # summary table
python3 plot_snapshot.py                           # regenerate the chart
```

`runner_multi.rb` は `go`（`go version`）と `zig`（`zig version`）にも対応するため、MiniGit lane を現在の比較集合へ広げられる。どちらも commit 済みの 20 試行 snapshot はまだない。

<a id="modification-survival-lane"></a>

## Modification-survival lane

MiniGit は飽和する（どの言語も v1 と v2 で 20/20 — [#1963](https://github.com/almide/almide/issues/1963)）。仕様と採点 script の両方をモデルに渡すため、モデルは session 内で oracle に照らして反復し、壊し得るファイル間不変条件もない。測るのは簡潔さと費用であって、変更後の生存ではない。

差を見分ける lane は別タスク集合である。言語ごとに seed program と順序付きの編集列を用意し、各編集をモデルに見せない hidden oracle で採点する。prefix survival とし、ある編集の失敗はそれ以降の全 horizon の失敗となる。タスクは Dojo（[almide/almide-dojo](https://github.com/almide/almide-dojo) の `tasks/xlang/`）に置く。全 MSR task material はそこに属する。最上位 `CLAUDE.md` の repo-boundary rule 参照。Dojo harness は規則上 Almide 専用なので、言語横断 driver はこの lang-bench に置く。設計、採点の論理積、ledger 形式、実行費用は #1963 に記載している。

| パス | 用途 |
|---|---|
| `lib/bench_common.rb` | `run_cmd` / `run_claude` / `run_tests` / `count_loc` / `parse_claude_json`、両 runner で共有 |
| `runner_edits.rb` | task-dir 駆動 driver：seed + 順序付き編集、累積 tree、hidden oracle、prefix survival。 `--dry-run` が bank gate |
| `aggregate_edits.rb` | ledger：Wilson 95 % CI 付き MSR-seq、MSR-edit、最初の失敗 mode、$/sequence。出力： `INCONCLUSIVE_BANK_SATURATED`（上限到達時） |
| `tasks-example/xlang/` | ローカルで gate を動かすための 2 言語 smoke fixture |
| `test/aggregate_edits_test.rb` | `ruby -Itest test/aggregate_edits_test.rb` |

```bash
ruby runner_edits.rb --tasks tasks-example/xlang --dry-run        # gate, no model call
ruby runner_edits.rb --tasks ../../../../almide-dojo/tasks/xlang --lang rust --trials 5   # measure (spends money)
ruby aggregate_edits.rb --tasks ../../../../almide-dojo/tasks/xlang                        # ledger
```

各編集 `k` について、`survive_k = compile ∧ visible ∧ hidden ∧ scope_clean`。`scope_clean` はモデルが変更した全ファイル（編集前 snapshot に対する git diff、新規ファイルも含む）が `scope.txt` の glob に一致することを意味する。tree 契約は言語に依存しない。`bash build.sh` は `./app` に実行ファイルを残し、`visible.sh` / `hidden.sh` は tree を cwd として実行して stdin 経由で `./app` を操作し、`PASSED:`/`FAILED:` を報告する。インタプリタ言語では `build.sh` が型検査を行い、`./app` wrapper script を書き出す。

> コマンド内コメントの訳（コマンドは原文保持）：raw/ から data.json の Almide entry へ／data.json から docs/figures/lang-bench-*.png へ／1 言語／集計表／チャート再生成／gate、モデル呼び出しなし／測定（課金あり）／ledger。
