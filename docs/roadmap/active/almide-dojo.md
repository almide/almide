<!-- description: Daily automated MSR loop — 30 tasks, Claude 100%, Llama 61%, almai integration -->
# Almide Dojo — Continuous MSR Measurement

## Motivation

Almide の存在理由は **Modification Survival Rate (MSR)** —— LLM が書いて、直して、また書いたコードが、どれくらいの確率で生き残るか。

現状、MSR の測定は `ai-coding-lang-bench` のような **単発・イベント駆動** でしか行われていない。設計変更が MSR に与える影響を日々追跡する仕組みが無いため：

- 診断メッセージを変更したときの影響が分からない
- stdlib に関数を足したときに、LLM の成功率がどう動くか見えない
- リグレッションに気づくのが遅い

本項目は **Almide Dojo** —— 毎日 LLM に Almide を書かせて、修正生存率を継続計測するループ —— を構築する。

## Design

### Repository structure

```
github.com/almide/almide-dojo
├── almide.toml         # パッケージマニフェスト (ハーネス自身が Almide パッケージ)
├── src/main.almd       # ハーネス本体 (Almide で実装 —— ドッグフード)
├── tasks/              # タスクバンク (100〜300 個)
│   ├── basic/          # FizzBuzz, 階乗, 素数判定...
│   ├── intermediate/   # パーサコンビネータ, JSON serde...
│   ├── advanced/       # minigit, インタープリタ...
│   └── */meta.toml     # 各タスクの難易度/期待行数/タイムアウト
├── runs/               # 日次ランの結果 (YYYY-MM-DD)
└── dashboards/         # 集計・可視化 (GitHub Pages)
```

ハーネスは **Almide 自身で書く**。これは意図的な選択で、理由は次の通り：

- Almide の HTTP + fs + process + json の組み合わせを **初めて非自明なプログラムで使う場所** になる
- stdlib の隙が見つかるたびに、本家にフィードバックされる (Dojo を書いていて困ったら、それは Almide の課題である)
- ハーネスのコード自体が Almide の現実世界 MSR のサンプルになる

### Task format

各タスクに次を含める：

- `prompt.md` —— LLM に渡す指示（タスクの仕様）
- `tests.almd` —— 生成コードに対して実行するテスト
- `meta.toml` —— 難易度、期待行数、タイムアウト

### Run loop

1. 全タスクについて、各モデル（Sonnet / Opus / Haiku）に生成を依頼
2. 生成された `.almd` をコンパイル
3. 失敗したら診断メッセージを渡して修正依頼（最大 N 手）
4. 成功したらテスト実行
5. すべての試行について、次を記録：
   - 一発成功率
   - N 手以内の成功率（N=1, 2, 3, 5）
   - 平均修正回数
   - **どの診断が修正に効いたか**
   - **どの診断が修正を誤らせたか**（悪性 hint）

### Metrics and dashboard

日次で次を出力：

- **一発成功率**（トレンドグラフ）
- **3 手以内成功率**
- **タスク別の成功率ヒートマップ**（停滞しているタスクの可視化）
- **診断効果ランキング**：修正に最も寄与した診断 Top 10
- **悪性 hint 検出**：LLM が hint を読んで誤った方向に直したケース
- **モデル別比較**：Sonnet vs Opus vs Haiku

### Malicious hint detection

これが Dojo の最大の価値。具体的には：

1. 失敗コード + 診断 → LLM が修正 → 再度失敗
2. 2 回目の失敗箇所が、1 回目の hint が示唆した変更と同一または隣接
3. このパターンが複数モデルで再現する

このケースを自動抽出して `almide-dojo/malicious-hints.md` に蓄積。各エントリは **次の診断改善の直接のタスク** になる。

### PR gate (opt-in)

本家 `almide/almide` リポジトリの PR で、Dojo の **縮小版（30 タスク）** を実行する workflow を追加。一発成功率が N% 以上下がったら PR を赤くする。

これにより **「LLM が書ける度合い」がコンパイラの受け入れテストになる** —— 世界初の試みになるはず。

## Implementation Phases

1. **Phase 1** ✓: `almide/almide-dojo` リポジトリ作成、基本 5 タスク、Almide で書かれたハーネスの scaffold
2. **Phase 2**: ハーネスを動く状態まで磨く（Anthropic API 呼び出し、修正ループ、単一タスク実行）
3. **Phase 3**: タスクバンク 30 まで拡張、`dojo all` コマンドと `runs/` への日次結果書き出し
4. **Phase 4**: 日次 GitHub Actions で実行、summary markdown を commit
5. **Phase 5**: ダッシュボード（GitHub Pages に静的ページ）
6. **Phase 6**: 悪性 hint 自動検出
7. **Phase 7**: タスクバンクを 100 → 300 まで拡張
8. **Phase 8**: 本家の PR gate として縮小版を統合

## Acceptance Criteria

- 30 タスク以上のタスクバンクが存在する
- 日次 CI ランが緑/赤で動作している
- 過去 30 日の MSR トレンドがダッシュボードで見える
- 悪性 hint が最低 1 件自動検出されている
- 少なくとも 1 つの診断改善が Dojo 由来で実装されている

## Risk and Cost

- **API コスト**：日次で 3 モデル × 300 タスク × N 試行 = 決して安くない。初期は 30 タスク × Sonnet のみに絞る
- **タスクバンクの品質**：偏ったタスクは偏った MSR を出す。多様性の監査が必要
- **偽陽性**：モデル側の変動（API 側の変更）を言語の回帰と誤認するリスク。ベースラインモデルのバージョンを固定する

## Related

- [Diagnostics: Here / Try / Hint Format](./diagnostics-here-try-hint.md) —— Dojo の主な改善フィードバック先
- [Stdlib Symmetry Audit](./stdlib-symmetry-audit.md) —— Dojo が "存在しない対称関数" の生成を検出する
