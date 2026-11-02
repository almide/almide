<a id="native-bug-fixes-research-and-validation-2026-09-09"></a>

# Native のバグ修正：調査と検証（2026-09-09）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](bugfix-research-2026-09-09.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](bugfix-research-2026-09-09.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](bugfix-research-2026-09-09.zh-CN.md)

> 非規範的な履歴参照資料。固定原文を正本とし、自動同期しない。測定・証明 gate・現行実装の検査を独立に再実行していない。引用コマンドは実行しない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/bugfix-research-2026-09-09.md)
> 記録の回帰合格と品質スコアは歴史資料で、独立再実行していない。legacy WASM の skip を合格と数えない。timeout 対応は best effort であり隔離ではなく、Windows cleanup には別途 Windows CI が必要。

<a id="reference-checkouts"></a>

## 参照 checkout

依頼された `../almide-references` ディレクトリには浅い checkout がある。

- Gleam：`gleam-lang/gleam`、`19bf207ebb7d953ea1391f041da48c214ee1440a`。
- Rust：`rust-lang/rust`、`0d31508599a7814a7044e9a7a871e3dc5f037753`。MIR 構築と標準ライブラリの process 内部の sparse checkout。

<a id="2062-an-outlined-branch-must-preserve-its-assignment-destinations"></a>

## #2062：切り出した分岐は代入先を保存しなければならない

Rust の [`builder/expr/stmt.rs`](https://github.com/rust-lang/rust/blob/0d31508599a7814a7044e9a7a871e3dc5f037753/compiler/rustc_mir_build/src/builder/expr/stmt.rs) は、RHS を評価し、LHS を place として解決し、その place に代入することで assignment を lowering する。分岐を値引数の helper へ移すと、外側の local の記憶領域を保存できない。Gleam の [`javascript/expression.rs`](https://github.com/gleam-lang/gleam/blob/19bf207ebb7d953ea1391f041da48c214ee1440a/compiler-core/src/javascript/expression.rs) は case 式と function literal を別々に dispatch する。その不変束縛の意味論は、Almide の可変 capture をコピーできる証拠にはならない。

決定：分岐内で束縛されず module global でもない変数へ分岐が書き込む場合、branch outlining を見送る。write-only と collection-write の対象を含む既存 mutation collector を再利用する。dense expression 経路と let-bound 経路の両方がこの guard を通る。local mutation と global access は既存挙動を維持する。nested lambda の書き込みも保守的に含める。この最適化を維持するためだけに reference-parameter ABI や新しい所有権規則は導入しない。

元の retry loop は native 上で `cost=0.0 n=2` から `cost=0.5 n=2` に変わった。`almide run --target wasm` も後者を出力する。新 spec は retry-loop state、loop 外の write-only capture、list 書き込み、branch-local mutation を扱う。legacy explicit-WASM の*test* runner は rendering wall でこの fixture を skip する。この skip は WASM 回帰テスト成功として数えない。

<a id="2063-adapt-the-function-value-abi-at-the-runtime-boundary"></a>

## #2063：runtime 境界で function-value ABI を適合させる

Almide は `Rc<dyn Fn(String)>` を出力する一方、HTTP streaming 実装は `impl FnMut(String)` を受け入れていた。診断は `Rc` がその generic bound を満たさないことを示す。既存 SSE caller は stack を借用する可変 closure を必要とするため、全 callback を所有された `Rc` closure に置き換えると lifetime と mutation の挙動を変えてしまう。

決定：`almide_http_request_stream` で Almide の function-value signature を公開し、小さな closure 経由で既存 `FnMut` 実装へ転送する。native SSE caller はその実装を直接呼ぶ。loopback integration test は式・block・束縛済み callback、共有 list の変更、content-length と chunked の両 response を検査する。

<a id="2065-the-deadline-covers-pipe-eof-as-well-as-child-exit"></a>

## #2065：期限は子の終了だけでなく pipe EOF も含む

Rust の [`sys/process/unix/unix.rs`](https://github.com/rust-lang/rust/blob/0d31508599a7814a7044e9a7a871e3dc5f037753/library/std/src/sys/process/unix/unix.rs) は、設定した process group を `setpgid` で実装する。直接の子が終了後も子孫が stdout/stderr を開いたままにできるため、無条件の reader join を有界 timeout 経路に含めてはならない。

決定：private Unix process group を作り、一つの期限の下で子の終了と両 reader の完了を poll し、timeout では group を kill する。Windows は既存の `/T` 付き taskkill 方式を使う。reader は完了後だけ join し、error 経路は pipe EOF を決して待たない。意図的に group から抜けた子孫は、継承した pipe を閉じるまで detached reader を生存させ得るが、その reader によって caller を期限後まで拘束できない。process tree 終了は best effort であり、process containment 機構ではない。

Unix integration test は、生存中の子、すでに終了した子、継承された stderr、正常な非ゼロ exit を検査する。Windows の tree cleanup には Windows CI が必要。

<a id="quality-measurement-and-existing-prs"></a>

## 品質測定と既存 PR

O6lvl4/codopsy の `fabfbdccac8bee5fc71055b6859c0684fd6fce5b` から build した codopsy **2.2.0** を、変更しない `.codopsyrc.json` とともに使う。PATH に入っている binary は 2.1.0 で、異なるスコアを報告する。

commissioned CI gate が測る `crates/almide-wasm` は **91/A**。変更した optimizer crate は **96/A**、`runtime/rs/src/process.rs` は **100/A**。より広い `crates` tree は **82/B**、codegen は **89/B**、runtime tree は **83/B**。これらは既存の広範な品質不足であり、本 batch は repository 全体が A だと主張せず、その結果を作るために規則を緩めない。

実装前に既存作業を確認した。#2054 は PR #2053、#2059 は PR #2064、#2044 は PR #2055 が対応中。確認時点で #2053、#2055、#2064 は develop と競合していた。本 batch はそれらを重複実装せず、PR が存在するというだけで issue を解決済みにしない。

<a id="validation"></a>

## 検証

- `almide test`：438/438 files。WASM 425、native fallback 13。
- `almide test --target rust`：438/438 files。
- optimizer/codegen の Rust test：78 件合格。
- interpreter 回帰：合格。後続 write-only overwrite より前の loop 累積値も含む。
- HTTP streaming integration：合格（callback 3 形式、encoding 2 種）。
- Unix timeout integration：合格（orphan-pipe 3 形、通常 exit 7）。100ms を使うのは timeout ケースだけ。通常終了は余裕ある 30s の上限を使い、機械負荷が成功 assertion を不安定な timing テストに変えないようにする。
- AST corpus parity：1 件合格。diagnostics corpus parity：3 件合格。新 fixture の測定行だけ追加し、既存 oracle hash は書き換えなかった。
- file discipline、pass roster、layout discipline、libm determinism、fixture formatting、`git diff --check`：合格。

<a id="ci-coverage-follow-up"></a>

### CI coverage の後続対応

legacy v1 の `almide test` 経路は list-element mutation ケースを見送るが、structural の `almide run --target wasm` 経路は対応する。3 つの scalar/record spec ケースは v1 WASM 経路に残す。list ケースは `tests/branch_lift_heap_mutation_test.rs` で別途固定し、native と structural WASM target を実行して最後の要素が 2 であることを検査する。fallback baseline は変更しない。
