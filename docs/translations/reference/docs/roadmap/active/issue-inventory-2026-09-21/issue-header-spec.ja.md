<a id="the-common-issue-header--area-label--spec-for-rewriting-almidealmide-issues"></a>

# 共通 issue ヘッダーと area ラベル — almide/almide の issue を書き換えるための仕様

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](issue-header-spec.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](issue-header-spec.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](issue-header-spec.zh-CN.md)

> [英語の正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/issue-inventory-2026-09-21/issue-header-spec.md)（固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466`）の非規範的な読者用スナップショット。元の文書が正本であり、この写しは自動同期されず、生成器・チェッカーの入力でもない。以下の日付、計画、完了状態、測定値、比較主張は当時の原文の記述を保持したもので、独立した再現や現行版についての検証ではない。コードブロックとコメントは原文のまま保持する。過去の収集用メタデータは、動作しない読者向け注記として表示する。

> 原文メタデータ：この文書には原文の日付が付与されていない。

> 歴史的な手順書であり、この翻訳作業で実行する指示ではない。コマンド、issue 編集、復元、継続的規則の引用は読書用に再現するだけである。この写しの作成によって issue 取得・変更、ラベル適用、ビルドやベンチマークは行っていない。固定の日本語フィールド名とテンプレートをそのまま保持するが、新たな指示や機械入力にはしない。

すべての未解決 issue に、(A) 既存内容のすべてより上に本文先頭へ追加する一つのブロックと、(B) 所属 crate を示す area ラベルを付ける。元の本文は削除、言い換え、並べ替えを一切しない。このブロックは、ほかの文脈なしにその issue だけを開いた読者が、何をすればよいか、どうすれば完了と分かるかを把握できるようにするためのものである。

公開済みの実例：**#2397** — まずこれを読み（`gh issue view 2397`）、その形に合わせる。

<a id="a-the-block-verbatim-in-shape"></a>

## (A) 形をそのまま使うブロック

```markdown
> **状態** — <one of: BLOCKER / 実装 / 計測器・ゲート / 設計判断 / 追跡 / 調査>・<one of: 着手可 / 前提待ち: #NNNN / 判断待ち>
> **一行で** — <what is wrong or missing, in one sentence, no background>
>
> **やること**
> 1. <concrete first action a fresh reader can start today>
> 2. <…>
>
> **完了条件** — <a verifiable condition: a gate that goes green, a measured number, a fixture that exists, a diagnostic that fires. Never "it feels done">
> **根拠** — <the measurement, file:line, commit or PR that makes this real; or `未測定` if the issue rests on a claim nobody has checked>
> **触る場所** — <paths, 2-4 of them, the ones a fix would actually edit>

---
```

状態の意味（最も近いものを一つ選ぶ）：

| 状態 | 該当するもの |
|---|---|
| `BLOCKER` | `I-unsound` / `I-miscompile` / `I-divergence` / `regression` を持つ — 未解決の間は最終リリースタグが拒否される |
| `実装` | 目標の形が分かっている compiler/stdlib/runtime コードの変更 |
| `計測器・ゲート` | ゲート、ratchet、manifest、測定ハーネスの作成、または誤った結果を示すそれらの修正 |
| `設計判断` | コードを書く前に裁定が必要（通常は `mob` ラベル付き） |
| `追跡` | 項目がほかの issue である umbrella、または bot が管理する台帳 |
| `調査` | 対象がまだ特定されていない。最初の手順は編集でなく測定 |

<a id="rules-for-filling-it"></a>

## 記入規則

1. **導き出し、創作しない。** 各行は、その issue 自身の本文・コメントまたはツリーに基づく。issue が完了の姿を示していなければ、判断するために何が必要かを記す。それが正直な完了条件である。
2. **安価に確認できるなら状態を検証する。** issue 自身の 3 行の再現手順を動かす、指定された PR に `gh pr view` を使う、ないとされたパスを `ls` で見る、存在しないとされたシンボルを `grep` する、など。この作業でビルド、ベンチマーク、テストスイートを開始してはならない。起票後に状態が変わったなら「一行で」に書き、当時の状態を示す元の文章は残す。
3. **`根拠` には確認できるものを記す。** `crates/…/foo.rs:120`、PR 番号、issue から引用した二つの実測値、ゲートスクリプトなど。確認できるものがなければ、もっともらしい文章でなく `未測定` と書く。この欄の未検証の主張は空欄より悪い。issue 自身の数字を根拠にする場合は、誰の数字かを示す：`本文の実測(2026-09-13)`。
4. **新たな主張を加えない。** 分析を追加せず、issue にない設計を提案せず、工数や期間を見積もらない。これは wrapper であり、書き直しではない。
5. **ラベルは日本語、内容は issue が使う言語にする。** 六つのフィールド名は固定の日本語であり、読者が見渡せるようにする。
6. **`前提待ち: #NNNN`** は、その項目が入るまで issue に着手できない場合だけに使う。単に*関連*していることは前提条件ではない。
7. **近隣プロジェクト名をどこにも書かない。** 比較する言語／ツールチェーンの名前を issue に書いてはならない。表記は「a neighbouring toolchain」／「近隣実装」とする。（ユーザーの継続的な指示。）

<a id="b-the-area-label"></a>

## (B) area ラベル

issue ごとに `A-*` ラベルを正確に一つ付ける（本当に両 crate にまたがる作業の場合だけ二つ）。既存ラベルから選び、新規作成しない：

| ラベル | crate |
|---|---|
| `A-frontend` | `crates/almide-frontend`、`almide-syntax` — parse、check、diagnostics、import table |
| `A-ir` | `crates/almide-ir`、`almide-mir`、`almide-optimize`、`almide-spine` |
| `A-codegen` | `crates/almide-codegen` — nanopass、Rust lowering、native borrow/RC inference |
| `A-wasm` | `crates/almide-wasm`、`almide-wasm-run`、`almide-wasm-vm` |
| `A-runtime` | `runtime/rs`、`crates/almide-rt-core` — native intrinsic |
| `A-interp` | `crates/almide-interp` — 第三の oracle |
| `A-stdlib` | `stdlib/*.almd`、`crates/almide-types` の stdlib registry |
| `A-driver` | `crates/almide-driver`、`src/` — CLI: run/build/test/check/fmt/bench |
| `A-perf` | `research/benchmark/perf`、`scripts/check-perf-ratio.sh` |
| `A-ci` | `.github/workflows`、`scripts/*.sh` のゲート、台帳、ratchet |
| `A-modules` | モジュール・パッケージシステム — 解決、サブモジュール、MVS、dialect epoch |

- ラベルは「触る場所」と一致しなければならない。一つを選べなければ、issue のパスが不明確である。報告にそう記し、`A-*` ラベルは付けない。
- `bug` と `enhancement` のどちらも付いておらず、本文から区分が明らかな場合は追加する（誤った振る舞い = `bug`、新しい表面 = `enhancement`）。
- `I-*`、`regression`、`P-*`、`mob` は**追加、削除、変更してはならない**。重要度／スケジュール／mob の判断であり、整形作業で私たちが決めることではない（`docs/project/ISSUE-TAXONOMY.md`）。
- `gh issue edit <n> --add-label A-xxx` で適用する（`--add-label` を繰り返す）。

<a id="mechanics-this-part-is-not-optional"></a>

## 手順（この部分は必須）

- 最初に取得する：`gh issue view <n> --json body --jq .body > $SCRATCH/i<n>.md`
- 取得したファイルの先頭にブロックを追加して、新しい本文を作る（`cat hdr i<n>.md >
  i<n>.new.md`）。取得したファイル以外を基に組み立てた `--body` を渡してはならない。
- 書き込む：`gh issue edit <n> --body-file $SCRATCH/i<n>.new.md`
- 再読込し、偽り得ない方法で往復を確認する。公開した本文を再取得し、元の本文の最初の 200 文字がその中に残ることを assert する（目視でなく Python で比較）。残っていなければ取得済みコピーから復元し（`gh issue edit <n> --body-file $SCRATCH/i<n>.md`）、その issue をスキップとして報告する。
- 冪等性：取得した本文がすでに `> **状態** —` で始まるなら、その issue は対応済み。二つ目のブロックを追加しない。ラベルを確認して次へ進む。
- 本文が空、または取得に失敗した場合は、推測せずスキップして報告する。本文がほかの issue のチェックボックス一覧である追跡 issue にもブロックを追加するが、すべてのチェックボックスはそのまま残す。

<a id="report-back"></a>

## 報告

issue ごとに一行：`#NNNN <状態> <A-label> ok`。加えて、スキップした issue、検証結果が本文と矛盾した issue、area ラベルを選べなかった issue には短い注記を付ける。

> 保持したテンプレートの説明訳：状態は列挙された分類と着手可／前提待ち／判断待ちから選ぶ。「一行で」は背景なしに問題・不足を一文で記す。「やること」は初見の読者が今日始められる具体的な第一歩から列挙する。「完了条件」はゲート合格、測定値、fixture の存在、診断の発火など検証可能な条件であり、「完了した気がする」ではない。「根拠」は測定、file:line、commit、PR、または未確認の主張なら未測定。「触る場所」は修正で実際に編集する 2–4 パスである。
