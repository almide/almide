<a id="the-regex-reference-table"></a>

# regex の参照表

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](README.zh-CN.md)

> [正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/regex/README.md)（固定コミット 35297a612fb29e3f78f6587aba253aa91d51f466）の非規範的な履歴参照用スナップショット。正本が優先され、この写しは自動同期されず、生成器や証明の入力ではない。コマンド、証明、測定、資格認定評価を独立に実行していない。出典参照、生成マーカー、日付付き数値は履歴資料として保持し、認証・資格認定・当局の受理・公開の証拠とはしない。

`reference-answers.tsv` は、正規表現言語の共通部分集合について、このプロジェクトの外部のエンジンが返す答えを記録する。ケースごとに `op<TAB>pattern<TAB>subject<TAB>answer` の 1 行を持つ。`scripts/check-regex-reference.sh` は Almide の両経路に同じ質問を行い、差が 1 つでもあれば失敗する。

<a id="why-a-table-and-not-a-fuzz-against-a-linked-engine"></a>

## リンクしたエンジンとの fuzz ではなく、表を使う理由

C-032 はすでにエンジンを fuzz しているが、その oracle は `runtime/rs/src/regex.rs`、すなわち*自分たち自身の* native エンジンである。これは両経路の一致を確認するが、両方が同じように、しかも誤って解釈しているルールは検出できない。#2129 はまさにその穴であり、そこで発見した 2 つの欠陥は既存のすべてのゲートから見えていなかった。

- `[]]` — 文字クラスの先頭にある `]` はリテラルの要素である（POSIX、および PCRE / Python / Rust / Go / JS が従う）。これを終端として読むと、パターンは空のクラスと余分な `]` になり、黙って一度もマッチしなくなった。エラーもマッチもなく、外から見えるものはなかった。
- 空にマッチできるパターンでの `split` — `split("^", "abc")` は `["a", "bc"]` を返し、文字を本来属するフィールドから移していた。

参照エンジンをリンクせず、コミットした表を使うのは、ゲートを密閉型にする必要があるため。ネットワークも新しい実行時依存もなく、どのマシンでも永続的に同じ答えを返す。代償は、部分集合が増えたときに表を手で更新すること。これは `crates/almide-spine/tests/golden/` の oracle manifest がすでに行っているトレードオフと同じである。

<a id="what-is-in-the-subset-and-what-is-deliberately-not"></a>

## 部分集合に含むものと、意図的に含めないもの

含む：リテラル、`.`、文字クラス（`]` が先頭の形式、クラス内の `[` を含む形式も含む）、アンカー、波括弧を含む貪欲／非貪欲の量指定子、グループと非キャプチャグループ、選択、`\d \w \s \D \W \S`、`\b`、および公開されている 6 操作。

含めないものは以下。参照エンジン同士でも一致しないため、差が出ても正しさについて何も言えない。

- 空の選択肢（`ab|`、`|ab`）。Python 3.7+ は空マッチの位置で非空マッチを再試行するが、Rust と Go はしない。Almide は Rust / Go と一致し、#2129 の際に `ripgrep`（Rust の `regex`）と照合した。
- キャプチャグループを返す `split`。Python は含めるが、Rust、Go、Almide は含めない。部分集合でグループ付きパターンが必要な箇所では、表の生成器は問い合わせ前にグループを非キャプチャへ書き換える。

<a id="regenerating"></a>

## 再生成

生成器は `docs/bugfix-research-2026-09-12.md` の #2129 節に記録されている。部分集合を増やすとは、パターンまたは対象文字列を追加し、参照エンジンに対して再実行すること。Almide がたまたま返す答えに行を合わせて編集してはならない。それではゲートが鏡になってしまう。
