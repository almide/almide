<a id="als-requirements-standard"></a>

# ALS 要求記述標準

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](STANDARD.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](STANDARD.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](STANDARD.zh-CN.md)

> 正本：[docs/specs/als/STANDARD.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/STANDARD.md)、固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466`。非規範的な読者向けスナップショットであり、元の ALS や検査器・台帳・生成器の入力を置き換えず、自動同期もしない。要求・歴史的状態・証拠・安全性の主張は原文の範囲を保つ。この版で独立の実行・計測・認証・証明は行っていない。原文言語版は読者用コピー、他言語版は翻訳である。


> 最終更新：2026-08-21

規範的な節の書き方、識別、廃止の方法を定める。これは ALS の DO-178C Table A-1 成果物、すなわち要求を書く際に従う標準である。`scripts/check-als-style.sh` は、ここで機械が検査できるすべてを強制する。残りはレビュー上の規律である。

> **実装チェックアウトに関する注記（2026-10-31。説明であり、新しい要求方針ではない）。** この文書は、以下の 2026-08-21 時点の執筆・レビュー規律を保持している。記載された `scripts/check-als-style.sh`、`scripts/check-als-validation.sh`、`proofs/als-validation.toml` は、このチェックアウトに存在しない。記述された強制処理や、記録されたレビュアー全員が著者だという記述は、現在の検証報告ではない。利用可能な [pin ゲート](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/scripts/check-als-pin.sh) は、固定された判定者に対する契約 ID と記述／見出しの一致を検査し、[要素カバレッジのゲート](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/scripts/check-als-element-coverage.sh)は構文要素のカバレッジを検査する。どちらも実質的な独立レビューを代替せず、適格性を確立するものでもない。この注記は契約、pin、執筆規則、受入基準を変更しない。

<a id="section-identity"></a>

## 節の識別

- 節の見出しは `## ALS-<PREFIX><n>[a-z]? <title>` とする。例：`## ALS-ST3 式文とコメント`。
- **接頭辞は厳密に一つの章ファイルに属する**（章が複数の接頭辞を所有してもよいが、一つの接頭辞が複数章にまたがることはない）。この一意の対応により `spec = "ALS-…"` キーが曖昧でなくなる。2026-08-20 には章をまたぐ 12 個の重複 ID を発見して改名した。そのうち一つには三つの意味があり、参照もすでにその三つに分散していた。
- レジストリ（スタイルゲートはこの表を解析する）：

| 接頭辞 | 章 | 主題 |
|---|---|---|
| B | bounded.md | 有界プロファイル — `@bounded` 関数（ADR-0017） |
| C | collections.md | list / map / set の意味論 |
| D | data-formats.md | JSON、Value、regex、バイナリデコード |
| DT | deterministic-time.md | 時間代数、fan の予算、決定的な race |
| DL | expressions.md | 宣言（`Decl::*`） |
| E | expressions.md | 式（`ExprKind::*`） |
| I | implementation.md | 実装定義の制限 |
| M | semantics.md | パターンマッチ、束縛、ジェネリクス、ループ |
| R | runtime.md | process、fs、http、ストリーミング、表示 |
| S | strings.md | `string.*` のコードポイント意味論 |
| ST | expressions.md | 文（`Stmt::*`） |
| T | text-and-numbers.md | 数値／テキスト変換、境界、assert |

- ID は**恒久的**であり、番号の振り直しも再利用もしない。節の分割時は新しい ID を発行する。統合時は一つの ID を残し、他方を永久に廃止する（廃止した意味を別の ID で復活させてはならない。2026-08-20：expressions 章の `ALS-S4`（`Stmt::Comment`）は `ALS-ST3` に統合された。`ALS-S4` は、従来から同時に指していた strings 章の節だけを指すようになり、`Stmt::Comment` は永久に `ALS-ST3` である）。

<a id="writing-a-section"></a>

## 節の書き方

- **一つの節に一つの観測可能な振る舞い。** 観測可能なものは stdout、stderr、終了コード、または検査器の判定（受理／固定コード付き拒否）である。実装を試験できないものは含めない。
- **実際にそうであることを断定形で記す。** 計画、「すべき」、「予定」、曖昧な留保は書かない。未実装の設計は ADR またはロードマップに置き、ここには置かない。
- **節の中で証拠を名指しする**。参照する契約が裏付けを行い、節はテストパスを明記する（`テスト:` 行）。実行可能な証拠のない規範記述は存在しない。
- 各節は一つ以上の契約（`spec =` キー）から参照され、すべての契約は実在する節を参照しなければならない。双方向がゲートの対象となる（`scripts/check-contracts.sh`）。
- 意図したターゲット間の相違は、**コミットされたデータ**（名前付き契約の下に置く、ターゲットごとの値の表）であり、回避や文章上のごまかしではない。
- 章は `> Last updated: YYYY-MM-DD` で始める。

<a id="validation-record-do-178c-a-3"></a>

## 検証記録（DO-178C A-3）

スタイルゲートは機械で検査できる部分を担う。節が**正確、一貫、完全**かはレビュアーの判断であり、記録されない判断は存在しない。`proofs/als-validation.toml` はレビュー済みの各節について一行を持つ：`id`、`hash`（レビューした節の本文。`bash scripts/check-als-validation.sh --stamp ALS-<id>` が表示）、`reviewed`（日付）、`by`、`independent`（著者以外のレビュアーの場合だけ `yes`）、`verdict`（`accurate`、または `issue = "#N"` を伴う `revise`）。本文を変更すると行は古くなり、再レビューするまでゲートは赤になる。行がない節は**未検証**であり、件数上限は縮小方向にしか動かさない。新しい節は記録行とともに取り込むか、PR に根拠を記して上限を厳密に一つ増やす。ゲートは `scripts/check-als-validation.sh` である。現在、記録されたレビューはすべて著者による（QUALIFICATION.md、制限 1）。台帳はそうでないふりをせず、それを可視化する。

<a id="ratchets-the-four-direction-law"></a>

## ラチェット（四方向の法則）

このリポジトリの縮小専用台帳は、一方向だけでなく四方向すべてで失敗しなければならない：(1) 著者が明記した根拠なしに件数を増やせない、(2) 理由が成立しなくなった行は失敗する、(3) 対象が存在しなくなった行は失敗する、(4) 測定対象をゼロ件しか測らなかった測定は失敗する。壊れた測定器は合格ではない。

<a id="forbidden-vocabulary-normative-chapters"></a>

## 禁止語彙（規範章）

`TODO`、`FIXME`、「たぶん」、「そのうち」、「予定」。これらを必要とする規範文は、規範化の準備ができていない。
