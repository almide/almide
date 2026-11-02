<a id="fan-sibling-capture-ownership-2061"></a>

# Fan の兄弟 arm によるキャプチャ所有権（#2061）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](fan-capture-ownership-review.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](fan-capture-ownership-review.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](fan-capture-ownership-review.zh-CN.md)

> 非規範の原文固定・歴史的読者用スナップショット。原文を正本とし、機械入力や実行 gate として置き換えない。自動同期しない。記載されたテスト結果・件数・実装状態・証明の主張は原文の範囲に属し、ここで独立再実行したものでも現在の保証として確立したものでもない。 未対応の Option arm とリポジトリ全体 A 未達を明示的限界として保持し、成功したとは扱わない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/fan-capture-ownership-review.md)

`fan { list.sum(rows), list.len(rows) }` は型検査に合格したが、同じ Rust の `rows` 束縛を共有する 2 つの `move` クロージャを出力し、E0382 を引き起こした。どちらの呼び出しも借用するだけなのに、最初の spawn が束縛を move した。

CaptureClone は現在、fan arm を暗黙のクロージャとして扱い、Fan 式全体の外側で所有された読み取り専用キャプチャを準備する。各 arm は別々の VarId と出力名を受け取るため、各 clone はすべての spawn より前に行われる。元の値は fan 式の後も利用できる。借用された slice と string パラメータには、明示的 lambda と同じ所有値への実体化を使う。arm が直接変更するキャプチャの既存処理は維持する。この変更はスレッド間の共有可変状態を新設しない。

既存の capture-binding 実装を lambda と fan の lowering で共有する。CloneInsertion は引き続き既存 Clone ノード内の使用を数えるが、その下へ追加 clone を挿入しなくなった。呼び出し引数の所有権ロジックとキャプチャ束縛の構築は別モジュールに分かれ、閾値を変えずに各ソースを Codopsy の 800 行制限未満に保つ。

<a id="reference-research"></a>

## 参照実装の調査

コミット `0d31508599a7814a7044e9a7a871e3dc5f037753` の clone 済み Rust 参照実装には、現在 `library/std/src/thread/scoped.rs` が含まれる。`Scope::spawn` は `FnOnce + Send` 境界を持つクロージャ値を受け取る（201–206 行）。スコープ付き lifetime のサポートがあっても、2 つの move クロージャが 1 つの所有値を消費できるわけではない。クロージャ値を `spawn` に渡す前にキャプチャを準備する必要がある。既存の Gleam 参照実装の `javascript/decision.rs::assign_subject` も同様に、評価を保持するため計算済み subject を再利用前に束縛する。両リポジトリは `../almide-references` にある。

<a id="regression-coverage"></a>

## 回帰検査の範囲

`tests/fan_sibling_capture_test.rs` は、List、String、Map、レコードのキャプチャ、借用パラメータ、fan 後の使用、入れ子 fan 式を native と構造的 WASM で実行する。また出力 Rust のユーザー部分を検査し、繰り返された `.clone().clone()` を拒否する。Map callback は scalar 値を返す。fan arm から直接 Option を返すと現在は別の構造的 `bind:unmapped` 拒否に到達し、サポート対象とは主張していない。

Codopsy 2.2.0、設定変更なし：CaptureClone、capture bindings、CloneInsertion、呼び出し引数 helper は各々 100/A。codegen crate は 90/A、全 crate 合計は 83/B（Codopsy が解析しなかったファイルが 2 つ）であり、リポジトリ全体 A は未達のままである。
