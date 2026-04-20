<!-- description: almide test stderr passthrough + almide explain <code> subcommand -->
# DX: `almide test` stderr & `almide explain`

[`dx-codegen-papercuts.md`](../done/dx-codegen-papercuts.md) の A / C 系は
v0.15.0 で閉じた。残る DX 系 2 件を独立 arc として再スタート。どちらも
LLM / dojo workflow で実際に詰まった箇所。

## B-1: `almide test` がコンパイル失敗時に詳細を握り潰す

**現状**: rustc error 時に `1 previous error; N warnings emitted` だけ出す。
`almide run` は cargo の stderr をそのまま流すのに、`almide test` は要約
しかしないので失敗の中身が見えない。dojo / nn の repro 時に繰り返し踏ん
でいる。

**真因**: `src/cli/mod.rs::cargo_build_test` が cargo の stderr を
`Vec<u8>` に capture し、rustc error の場合だけ要約ロジックに流す。その
要約ロジックが `previous error; warnings emitted` の行しか拾えていない。

**提案**:
- 一次対処: `ALMIDE_TEST_VERBOSE=1` か `--verbose` flag で capture を
  skip して passthrough
- 根治: cargo の `--message-format=json` を読んで、diagnostic span を整
  形出力。rustc の構造化出力を Almide 側のソース span に逆写像できれば
  より望ましいが、手間が釣り合うか要検討

**成功条件**: rustc の `error[E0XXX]` 行と span がそのまま見え、次の一
手が決められる。CI ログが肥大化しないよう default は現状維持。

## B-2: `almide explain E003` が欲しい

**現状**: diagnostic に `E003` 等の code が付くが、code → 詳細説明の
lookup table がない。`docs/diagnostics/` に各 code 別の md は既に存在
(Phase 5 registry、`active/diagnostics-here-try-hint.md` の遺産) するが、
CLI からは辿れない。`grep` で code を探す羽目になる。

**提案**:
- `almide explain <code>` CLI subcommand を追加
- 引数なしなら `docs/diagnostics/` 全 code を一覧表示
- 該当 md をそのまま stdout に流す (`docs/diagnostics/E003.md` → 表示)
- すでに `tests/diagnostic_coverage_test.rs` が全 code 分の md 存在を強
  制しているので、追加メンテは不要

**dojo 側利益**: 自動分類精度が上がる、LLM が hint を辿りやすくなる。
現状は LLM が code を見ても「何のエラーか」文脈を取れない。

## 出元

dojo / nn loop での実挙動:
- B-1: nn の test 失敗で「再現できないバグ」になりかけたのが複数回
- B-2: dojo の auto classify で code 数値推測に依存している

## 着手順

B-2 → B-1 の順。B-2 は `docs/diagnostics/` の既存 15 個の md を読み込
んで流すだけなので 1-2 時間。B-1 は cargo JSON output の消化が要るので
もう少し見積もる。

## 参照

- [done/dx-codegen-papercuts.md](../done/dx-codegen-papercuts.md) — 前身 arc
- `src/cli/mod.rs::cargo_build_test` — B-1 の修正対象
- `docs/diagnostics/E*.md` — B-2 が参照する既存 md 群
