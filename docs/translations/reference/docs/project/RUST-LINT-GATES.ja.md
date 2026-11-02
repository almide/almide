<a id="rust-lint-and-formatting-gates-1462"></a>

# Rust lint と整形のゲート（#1462）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](RUST-LINT-GATES.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](RUST-LINT-GATES.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](RUST-LINT-GATES.zh-CN.md)



> 原文：[docs/project/RUST-LINT-GATES.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/project/RUST-LINT-GATES.md)（固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466`）。読者向けの非規範的なスナップショット訳であり、自動同期されない。原文の時点・適用範囲・主張を維持したもので、翻訳時に独立した実測や実装検証は行っていない。 CI の許可リストと実行状況は原文固定時点の記述であり、この翻訳ファイルが高速経路の対象になるとは示していない。

compiler-championship 調査で不足が見つかった三つのレーンと、それぞれの現在の状態を示す。下記の除外を記録する台帳はこの文書であり、Makefile のコメントに暗黙のまま残すものはない。

<a id="live-lanes"></a>

## 稼働中のレーン

| レーン | ゲート | 運用規律 |
|---|---|---|
| `.almd` の整形、ツリー全体 | `scripts/check-fmt-gate.sh`（CI の `checks` ジョブ）：`spec/` + `examples/` + `tools/almide-gates/` は通常モード（意図的に不正な gauntlet セルを除く）、`stdlib/` は `--no-import-edit`（splice-context ソースなので import の自動挿入を除外する。AST 保存検証器は引き続きすべての書き換えを守る） | ずれがあればすべて失敗 |
| rustc 警告 | `scripts/check-rustc-warnings.sh`（CI の `build` ジョブ） | 縮小のみのベースライン、現在 0、増減どちらも失敗 |
| clippy 警告 | `scripts/check-clippy-warnings.sh`（CI の `build` ジョブ） | 縮小のみのベースライン（`scripts/clippy-warnings-baseline.txt`）：増加は PR で失敗。減少は PR では通知し、夜間の厳格な `clippy-baseline.yml` レーンで失敗。deny レベルの lint（例：`overly_complex_bool_expr`）はコンパイル自体を失敗させる。初回実行では parser の名前付き引数経路に残っていた `\|\| true` が見つかった |

<a id="narrow-prose-only-ci-lane"></a>

## 限定された prose-only CI レーン

許可リスト内の文章のみの変更では、このリポジトリの、比較対象のベースコミットそのものに対する成功した `ci.yml` push 実行から Linux コンパイラ成果物を再利用できる。成果物には一致する repository/run/SHA の来歴と、検証済み SHA-256 ダイジェストが必要である。成果物が欠落、期限切れ、信頼できない、または検証不能なら通常ビルドに戻る。すべての文書変更がコンパイルを回避できるという約束ではない。

分類器は信頼された比較ベースから読み込む。ソース、workflow、生成器、コンパイル時に組み込む診断/テンプレート、規範的 ALS、未知のパスはこの新しい文章高速経路の対象にならない。merge group は変更されたすべてのパスが適格な場合だけ対象となる。それ以外は通常のコンパイラビルドと既存のジョブ選択が適用される。従来の `docs_only` 分類器は、一部の文書パスについて重いテストマトリクスを引き続きスキップすることがある。したがって `prose_only` が false でも、すべてのジョブが実行されるという約束ではない。ソース/workflow の変更は従来の docs-only 分類にも該当しない。main 対象のクロスプラットフォームビルドは変わらない。初回導入時はベースに分類器がまだないため、このレーンを再利用できない。

<a id="what-the-lane-admits"></a>

### レーンが受け入れるもの

ルートの `README*.md`、`CLAUDE.md`、`CHANGELOG.md`、`SUPPORT.md`、`SECURITY.md`、`TOP_DOCS` に列挙された `docs/` 直下のページ、`docs/benchmarks/README.md`、`.claude/commands/*.md`、および `docs/{roadmap,specs,design,project}/**/*.md` から下記を除いたもの。パスは、それを読むもの（`include_str!`、Rust と Almide のテスト、このレーンがスキップするジョブ内のゲート）を調べてから受け入れる。

維持される文書チェック以外のものが読むため受け入れないもの：`docs/GRAMMAR.md`（構文テストが `assign` 規則を解析する）、`docs/stdlib/**`（複数の Rust テストが解析する生成済みシグネチャ一覧）、`docs/specs/cli.md`、`docs/project/CLAUDE_TEMPLATE.md`、`docs/diagnostics/**`、`docs/specs/als/**`、生成済み `docs/roadmap/README.md`。従来の `docs_only` 分類器も最初の三つをコードとして扱うため、それらの変更では Rust テストマトリクスが実行される。

受け入れ対象ファイルの変更でも、diff のどちらか側で、変更行が HTML コメントであるか、生成ブロック（`…:generated:start/end`、`BEGIN/END GENERATED …`、`…: begin/end`）内にあれば、このレーンから外れる。これらの行は生成器や鮮度ゲートへの入力になるため、完全なレーンを実行する。

<a id="what-still-runs-and-what-is-skipped"></a>

### 何を実行し、何をスキップするか

このレーンでは `Build (Linux)` は release バイナリだけを生成する（検証済みベース成果物があれば、それを使用）。このレーン内では読まれないため、nextest archive と rustc/clippy ラチェットをスキップする。Lean、Acceptance Ring、Cross-Target、mutation sweep はスキップする。Emit & Format は文書を読むすべてのステップを引き続き実行する。LLM-surface fences、`docs-gen --check`、標準ライブラリと roadmap の一覧、measurements ページの生成ブロック、契約のトレーサビリティ（契約宣言が引用する見出しを読む）、契約の引用、grammar ゲートである。スキップするのはコンパイラソース、`spec/` コーパス、フィクスチャ、`docs/stdlib`（このレーン外）、リリースタグ、GitHub API を読むステップだけである。該当するのは JS host ゲート、stability closure、installer attestation、release seal、interface-differ controls、pass-isolated fixtures、pass-order shuffle、ownership certifier、spec-test-file runner checks、doc fences。この一覧は `test_emit_and_format_lane_skips_only_the_audited_steps` で固定する。ステップを追加する前に、何を読むかを監査する。分類が失敗または空ならジョブ全体を実行する。

名前付きの必須マトリクスコンテキストは引き続き結果を報告する。ブランチ保護と merge queue の要件は変わらない。定期/手動の検証は完全なレーンのままである。正確な許可リストと失敗時の挙動は [`scripts/ci_prose.py`](../../../../../scripts/ci_prose.py) と、そのオフライン回帰チェック [`scripts/test_ci_prose.py`](../../../../../scripts/test_ci_prose.py) を参照。分類器は各 workflow の `Change class` ジョブログに理由（`prose lane: "…"`）を出力する。

<a id="reading-a-run"></a>

### 実行結果の読み方

各 workflow の分類ジョブは `prose lane: "<reason>"` を一行出力し、続けて `prose_only=true|false` を出力する。CI はこのジョブを `Change class`、Acceptance Ring、Cross-Target、mutation sweep は `prose-changes` と呼ぶ。`false` の理由は、その変更をレーン対象外にした最初のパスまたは行を示す。レーンを使う場合、Emit & Format の `Lane` ステップは `PROSE_LANE=true` を出力し、スキップしたステップはジョブ画面に skipped と表示される。`Build (Linux)` の `verified exact-base compiler artifact <id>` はベースのバイナリを再利用したことを意味し、`prose compiler reuse unavailable (…)` は再ビルドしたことを意味する。どちらも正常な結果である。

<a id="recorded-exclusion-cargo-fmt---all------check"></a>

## 記録済みの除外：`cargo fmt --all -- --check`

2026-08-28 時点で、意図的にゲート化していない。

1. ツリーには計測上 **6,168 個の rustfmt hunk** がある。導入は一斉整形となり、進行中のすべてのブランチと競合し、一度に `git blame` を汚す。ゲートの PR に同梱せず、単独で導入しなければならない。
2. コミット済みの生成ソース（`crates/almide-codegen/src/generated/*`）は rustfmt の形を出力しない生成器から作られるため、fmt ゲートはそれらを固定する regen-diff ゲートと衝突する。導入前に、生成ツリー用の `rustfmt.toml` ignore 集合が必要である。
3. `include!` で分割したファイル（800 行超のモジュールに対する codopsy の規律）は item の断片を含み、rustfmt が分割境界で望ましくない折り返しを行う。ツリー全体の再整形前に分割集合の監査が必要である。

導入順序：`rustfmt.toml` ignore 集合を導入 → 長期ブランチがないタイミングで専用の一斉変更 PR（`cargo fmt --all`）→ clippy ステップの隣に `cargo fmt --all -- --check` を追加。それまではこの節が、レーンがないこととその理由を率直に記録する。
