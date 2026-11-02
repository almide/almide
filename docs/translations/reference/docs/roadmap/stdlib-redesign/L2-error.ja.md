# L2/L3: error

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-error.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](L2-error.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-error.zh-CN.md)

> 非規範の原文固定読者用スナップショット。変更しない原文を読取り側にとっての正本とする。自動同期や独立実行はしない。歴史的主張・採用案・実装状態・測定証拠の元の範囲を保持し、本翻訳が独立検証したとはしない。インラインコードは元言語の文字列も含め原文どおり保持する。 10月21日の非推奨化・撤去計画、epoch、使用数、probe結果、外部比較は原文時点の記録であり、現行API一覧ではない。chain区切りの意味変更、messageがokと空エラーを区別できない点、result.contextの却下と証拠による再検討条件を保持する。この読者用ファイルは原APIを撤去しない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-error.md)

> 元metadataの非活性注記：stdlib再設計L2/L3 — error。ADR-0004が既に固定したmap_err/match慣用句の第2の綴りであるcontext/message/chainを撤去し、モジュールを空にする。

> 最終更新: 2026-10-21
> 前提: [L0](../../../../../roadmap/stdlib-redesign/L0-conventions.md)・[L1](../../../../../roadmap/stdlib-redesign/L1-categories.md)・[L2-result](../../../../../roadmap/stdlib-redesign/L2-result.md)・ADR-0003 / ADR-0004 / ADR-0012
> 出典: Rust https://doc.rust-lang.org/std/error/trait.Error.html ・ Go https://pkg.go.dev/errors ・ Python https://docs.python.org/3/library/exceptions.html ・ TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Error/cause ・ Gleam https://gleam-stdlib.hexdocs.pm/gleam/result.html (F) ・ Elixir https://hexdocs.pm/elixir/Exception.html ・ OCaml https://ocaml.org/manual/latest/api/Result.html (F) ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin/-result/ (F) ・ Swift https://developer.apple.com/documentation/swift/error

`error` は 3 関数のモジュールです。どれも「err の文字列」を扱います。Almide のエラーは String が既定で（ADR-0002 D2、ADR-0004）、文脈を前に付けるのは `result.map_err((e) => "文脈: ${e}")` だと ADR-0004 D2 が正準形として決めています。

<a id="l2-役割の比較"></a>

## L2: 役割の比較

| 役割 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| err に文脈を前置きする | `error.context` / `result.map_err` | `map_err`（std）・`context`（anyhow、標準外） | `fmt.Errorf("..: %w", err)` | `raise X from e` | `new Error(msg, { cause })` | `map_error` (F) | — | `map_error` (F) | `recoverCatching` / 例外の `cause` | — |
| 2 つのエラーを「原因つき」でつなぐ | `error.chain` | `source()` | `%w` / `errors.Join` | `__cause__` | `cause` | — | — | — | `cause` | — |
| err の文字列を取り出す | `error.message` | `to_string()` | `err.Error()` | `str(e)` | `e.message` | — | `Exception.message` | — | `message` | `localizedDescription` |
| 原因をたどる | — | `source()` | `errors.Unwrap` / `Is` / `As` | `__cause__` | `cause` | — | — | — | `cause` | — |

<a id="完全性の規則"></a>

### 完全性の規則

- 文脈の前置きは 1 綴りだけ: `result.map_err((e) => "文脈: ${e}")`（区切りは `": "`、元のエラーは末尾）。ADR-0004 D2 と Alternatives 7 が、糖衣の `result.context` を「同じことを書く 2 つ目の綴り」として却下しています。
- エラーの中身による分岐の道具は置きません（ADR-0004 D4）。そのため「原因をたどる」役割（Go の `errors.Is` など）は意図して持ちません。

<a id="l2-の判定"></a>

### L2 の判定

- 重複が 3 つ: `error.context` は `result.map_err` の決まった形、`error.message` は `match` か `result.to_err_option(r) ?? ""`、`error.chain` は文字列の埋め込みと同じです。しかも `error.chain` の区切り（`"\ncaused by: "`）は、正準形の `": "` と違います（probe-error/p.almd で確認）。
- 結論: 3 関数を撤去し、モジュールを閉じます（R-error-01）。L1 の表も「`error` の 3 関数は撤去し、モジュールは空になる」と同じ判定です。

<a id="案"></a>

## 案

<a id="r-error-01-errorcontexterrormessageerrorchain-を撤去しerror-モジュールを閉じる---採用"></a>

### R-error-01 `error.context`・`error.message`・`error.chain` を撤去し、`error` モジュールを閉じる — ○ 採用

- 内容:
  - `error.context(r, msg)` → `@deprecated(since=15, use="result.map_err")`。置き換え: `r |> result.map_err((e) => "${msg}: ${e}")`。挙動は同じです（probe: `error.context(err("boom"), "loading")` と `map_err` 版がどちらも `err("loading: boom")`）。
  - `error.message(r)` → `@deprecated(since=15, use="result.to_err_option")`。置き換え: `result.to_err_option(r) ?? ""`。ただし ok と `err("")` を区別したいなら `match` を勧めます。いまの `error.message` は ok でも `err("")` でも `""` を返し、2 つを区別できません（probe で確認）。
  - `error.chain(outer, cause)` → 撤去。置き換え: `"${outer}: ${cause}"`。`"\ncaused by: "` の区切りがどうしても要る場合はその文字列をそのまま書きます。置き換え先が関数ではないので、`@deprecated(since=15, use="\"${outer}: ${cause}\"")` と書き方そのものを `use=` に入れ、撤去後は `removed_spellings.rs` のヒントで同じ形を示します。`almide fix` は `error.chain(a, b)` → `"${a}: ${b}"` と書き換えます（区切りが変わるので、fix は書き換えた箇所を一覧に出します）。
  - 3 件すべて `[[deprecation]]` ブロック（`removal_epoch = 16`）、撤去後は E002 にヒント。撤去後は `stdlib/error.almd`・`error_chain.almd`・`error_message.almd`、`runtime/rs/src/error.rs`、`STDLIB_MODULES` の登録を外します。
- 根拠の L0 規則: R-L0-01（同じ役割に綴り 1 つ。ほかの公開関数を 1 回呼ぶだけの転送も同じ役割に数える）、R-L0-02（Result を主語にする関数は result に置く）、R-L0-14
- (a) 9 言語で「文脈の前置き」を標準の専用関数に持つものはありません。Rust の `context` は anyhow という外部のクレートで、標準は `map_err` です。Go は `fmt.Errorf` の書式、JS は `cause` 引数で、どれも汎用の道具で書きます。
- (b) 影響なし。置き換え先はどれも純粋な計算で、両 target に既にあります。
- (c) `error.context` と `result.map_err` の 2 つがあると、文脈の区切り（`": "`）や、ok のときの挙動を書き手が両方覚える必要があります。`error.chain` は区切りまで違います。`error.message` は ok と空の err を区別しないので、`if error.message(r) == "" then ..` と書くと ok の判定を取り違えます。ADR-0004 D3-(b) の lint（`${e}` の書き忘れ、#1105）が、`map_err` 版の危険を診断で塞いでいます。
- (d) 撤去。使用数（定義と生成された doc 行を含む）: `error.context` spec 4 / tests 17 / docs 11 / stdlib 0、`error.message` spec 5 / tests 22 / docs 10 / stdlib 1、`error.chain` spec 4 / tests 2 / docs 5 / stdlib 1。CHEATSHEET と `llms.txt` には出てきません。`almide fix` は `error.context(r, m)` → `r |> result.map_err((e) => "${m}: ${e}")` と `error.message(r)` → `result.to_err_option(r) ?? ""` を書き換えます。
- ゲート: 撤去後の E002 ヒントの fixture（3 件）。R-L0-01 の同義語ゲート（#3851 F を広げたもの）は、ほかの公開関数を 1 回呼ぶだけの関数も検出するので、撤去後に同じ形の転送が戻れば失敗します。
- issue: #3909

<a id="r-error-02-errorcontext-を-resultcontext-として残す---不採用"></a>

### R-error-02 `error.context` を `result.context` として残す — × 不採用

- 内容: 置き場所だけ直して（R-L0-02）`result.context` にする案は採りません。
- 根拠の L0 規則: R-L0-01。ADR-0004 Alternatives 7 が `result.context` をこの理由で却下済みです（#1104 は却下でクローズ）。
- (c) 再検討の条件は ADR-0004 の Falsifier 3（lint があっても `${e}` の書き忘れが Dojo で多く測られた場合）です。その証拠はまだありません。
- issue: なし

<a id="l3-関数表"></a>

## L3: 関数表

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `error.chain` | `chain(outer: String, cause: String) -> String` | Go `fmt.Errorf("%w")` / JS `cause` / Python `from` | 撤去 | R-error-01 |
| `error.context` | `context(r: Result[T, String], msg: String) -> Result[T, String]` | Rust map_err（anyhow context は標準外）/ Gleam・OCaml map_error | 撤去 | R-error-01、R-error-02 |
| `error.message` | `message(r: Result[T, String]) -> String` | Go `Error()` / JS `message` / Kotlin `message`（どれも例外・エラー値の側） | 撤去 | R-error-01 |

（3 関数。追加なし）

<a id="l0-への差し戻し処理済み"></a>

## L0 への差し戻し（処理済み）

- **L1 への差し戻し**: L1 の表は「`error` の 3 関数は重複か転送なので撤去し、モジュールは空になる」に直っています。このページと一致しています。
- 転送の扱い: R-L0-01 に「ほかの公開関数を 1 回呼ぶだけの関数（転送）も同じ役割に数える」と入り、`error.context` が例に挙がっています。同義語ゲートも、ほかの公開関数を 1 回呼ぶだけのものを検出する規則を持ちます。
