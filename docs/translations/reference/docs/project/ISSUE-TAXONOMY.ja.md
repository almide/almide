<a id="issue-label-taxonomy-1482"></a>

# Issue ラベルの分類（#1482）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](ISSUE-TAXONOMY.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](ISSUE-TAXONOMY.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](ISSUE-TAXONOMY.zh-CN.md)



> 原文：[docs/project/ISSUE-TAXONOMY.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/project/ISSUE-TAXONOMY.md)（固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466`）。読者向けの非規範的なスナップショット訳であり、自動同期されない。原文の時点・適用範囲・主張を維持したもので、翻訳時に独立した実測や実装検証は行っていない。

このファイルができる前、トラッカーには分類ラベル（`bug`、wasm 分類、`fuzz` など）はあったが、**重大度の区分がなかった**。そのため「未解決のリリース阻害要因はあるか」を検索できず、リリースのたびに記憶で答えていた。このファイルは重大度の閉じた集合、各区分への受け入れ条件、それらを利用する一つの規則を定義する。さらに下では、もう一つの検索できなかった問い、**この issue はどの crate に属するか**に答える領域軸を定義する。

<a id="the-closed-set-severity"></a>

## 閉じた集合（重大度）

| ラベル | 該当するもの | 例 |
|---|---|---|
| `I-unsound` | 保証の中核への違反：`almide check` + v1 証明書が受理したものが、メモリ安全性または検証済み所有権の契約を破る（use-after-free、double-free、誤った内容を証明する証明書）。 | 生きているハンドルを解放する drop 経路 |
| `I-miscompile` | 受理済みプログラムが、いずれかのターゲットで**誤った値**を計算する。両ターゲットが一致していて両方とも誤っている場合（interp を第三のオラクルとする区分）も含む。wall は誤コンパイルではない。正直な拒否は、この区分に該当しないことを意味する。 | `5 \|> half ?? -1` が `0` を出力する |
| `I-divergence` | 受理済みプログラムについて native / wasm / interp の**観測可能な出力**（stdout、stderr、終了コード）が一致しない。契約台帳のバイト一致の約束が破られている。 | 一方の側だけ `list.unique` が `-0.0` の後の `0.0` を残す |
| `regression` | **リリース済み**バージョンで動いていた振る舞いが HEAD で壊れている。該当する場合は `I-*` 区分を併記する。 | v0.57.0 で成功し develop で失敗するフィクスチャ |
| `P-critical` / `P-high` / `P-low` | 作業順の優先度。`I-*` 軸とは独立し、単独では**阻害する効力を持たない**。 | — |

使用規則：

- `I-*` ラベルは区分が**確認済み**（再現済み、または失敗するフィクスチャで固定済み）だと表明する。疑いの段階では検証までラベルを付けない。
- 一つの issue が複数の区分を持ってよい（`I-divergence` + `regression`）。
- wall、未実装機能、性能差、文書のずれは `I-*` 区分では**ない**。`enhancement` / `documentation` と領域ラベル（下記）を付ける。

<a id="the-blocking-rule"></a>

## リリースを阻止する規則

**`I-unsound`、`I-miscompile`、`I-divergence`、`regression` のいずれかのラベルが付いた issue が未解決の間は、正式版のリリースタグを出してはならない。**

- [`scripts/count-release-blockers.sh`](../../../../../scripts/count-release-blockers.sh) で計測する。リリース workflow は正式版作成前に `--gate` で実行するため、規則は記憶に頼らず強制される。出力される数は**異なる**未解決 issue の数である。二つの区分を持つ一つの issue は二件ではなく一件の阻害要因になる（#2400）。合計の上のラベル別内訳には重複が表れる。
- `-rc` プレリリースタグは例外である（RC はまさにツリーの soak のために存在する。`CLAUDE.md` の RC 手順を参照）。件数は引き続き出力する。
- 逃げ道は**免除ではなく降格**である。阻害要因がリリースに許容可能だと判断する場合、それは mob の決定として、理由を issue に記してラベルを削除または変更し、release seal の `known_problems` フィールドに処置を記録する。ゲート自体にスキップ用引数はない。

<a id="the-area-axis-which-crate"></a>

## 領域軸（どの crate か）

重大度は問題の影響の大きさを示すが、**作業がどこにあるか**は示していなかった。初めてトラッカーを開いた読者は、77 件の本文を読まないと「wasm emitter に何が残っているか」「frontend に何が残っているか」を尋ねられなかった。`A-*` 軸がそれに答え、各ラベルの GitHub 説明に crate 名を書くため、対応表用の別文書は不要になる。

| ラベル | crate / パス |
|---|---|
| `A-frontend` | `crates/almide-frontend`、`almide-syntax` — 解析、検査、診断、import 表 |
| `A-ir` | `crates/almide-ir`、`almide-mir`、`almide-optimize`、`almide-spine` |
| `A-codegen` | `crates/almide-codegen` — nanopass、Rust lowering、native の借用/RC 推論 |
| `A-wasm` | `crates/almide-wasm`、`almide-wasm-run`、`almide-wasm-vm` |
| `A-runtime` | `runtime/rs`、`crates/almide-rt-core` — native intrinsic |
| `A-interp` | `crates/almide-interp` — 第三のオラクル |
| `A-stdlib` | `stdlib/*.almd`、`crates/almide-types` の標準ライブラリレジストリ |
| `A-driver` | `crates/almide-driver`、`src/` — CLI：run/build/test/check/fmt/bench |
| `A-perf` | `research/benchmark/perf`、`scripts/check-perf-ratio.sh` |
| `A-ci` | `.github/workflows`、`scripts/*.sh` のゲート、台帳、ラチェット |
| `A-modules` | モジュールとパッケージのシステム — 解決、サブモジュール、MVS、dialect epoch |

使用規則：

- issue ごとに `A-*` は**一つ**。作業が本当に二つの crate にまたがる場合だけ二つにする。合うラベルがなければ、issue が所在を示していないのであり、ラベルの不足ではなく issue の不足である。
- ラベルは issue 自身の `触る場所` 行（下記）と一致しなければならない。同じ主張を二度書いているのであり、不一致ならどちらかが古い。
- `A-wasm` は従来の `wasm-codegen` をその場で改名したもので、issue は引き継いでいる。旧名を使う script や workflow はなかった。一方 `fuzz`、`fuzz-findings`、`fuzz-perf` は `.github/workflows/fuzz-nightly.yml` が利用しており、綴りを保たなければならない。
- 領域軸は重大度の集合のようには閉じて**いない**。リポジトリへの crate 追加でラベルを追加してよい。削除や範囲の変更は、引き続き `gh label` の変更と同じコミットでこのファイルに記録する。

<a id="the-issue-header-what-to-do-in-the-issue"></a>

## Issue のヘッダー（issue 内で何をするか）

すべての未解決 issue は、単独で開いた読者が何をすべきか分かるよう、元の本文より上、本文の先頭に引用ブロックを置く。

```
> **状態** — <BLOCKER / 実装 / 計測器・ゲート / 設計判断 / 追跡 / 調査>・<着手可 / 前提待ち: #NNNN / 判断待ち>
> **一行で** — one sentence
>
> **やること**
> 1. the first concrete action
>
> **完了条件** — a verifiable condition: a gate green, a measured number, a fixture, a diagnostic
> **根拠** — the measurement / file:line / PR that makes this real, or `未測定`
> **触る場所** — the 2-4 paths a fix would edit
```

`状態` は六つの値のいずれかを取り、それぞれに固有の対象がある。狙いは、トラッカーを眺める読者が、本文を読む前に作業の種類を判断できることだ。

| 状態 | 該当するもの |
|---|---|
| `BLOCKER` | `I-unsound` / `I-miscompile` / `I-divergence` / `regression` を持ち、未解決の間は正式リリースタグを拒否する。ラベルと双方向に一致する。該当ラベルのない `BLOCKER`、または `BLOCKER` のない該当ラベルは、どちらかが古いことを意味する |
| `実装` | 目標の形が分かっているコンパイラ、標準ライブラリ、ランタイムのコード変更 |
| `計測器・ゲート` | 作業そのものがゲート、ラチェット、manifest、計測ハーネスである。誤った結果を示すものの修正も含む |
| `設計判断` | コードを書けるようになる前に裁定が必要（通常は `mob` ラベルも付く） |
| `追跡` | 他の issue を項目とする親 issue、または bot が維持する台帳 |
| `調査` | 対象がまだ特定されていない。最初の手順は編集でなく計測 |

行の後半は着手のゲートであり、`着手可`、`前提待ち: #NNNN`（本当の前提だけで、*関連する*だけでは該当しない）、`判断待ち` のいずれかである。

- `根拠` は判断を支えるフィールドである。issue が計測に基づくか、もっともらしい文章に基づくかを示す。`未測定` は正当な値であり、自信のある推測より役立つ。次の読者に、最初の手順は計測だと伝えるからだ。
- `完了条件` は「終わった気がする」とは決して書かない。検証可能な終わりを示せない issue では、何があれば*判断*できるかを完了条件とし、それが作業になる。
- このブロックはラッパーであり、本文にない主張を加えない。新しい根拠は従来どおりコメントに書く。

<a id="amending-the-set"></a>

## 集合の変更

重大度の集合は意図的に閉じている。場当たり的に増える分類は意味を失う。区分の追加、受け入れ条件の変更、阻止対象区分の変更は mob の決定とし、このファイルと `scripts/count-release-blockers.sh` を同じコミットで編集して記録する。
