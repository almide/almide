<a id="native-read-only-ownership-review"></a>

# Native の読み取り専用所有権レビュー

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](native-read-borrow-review.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](native-read-borrow-review.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](native-read-borrow-review.zh-CN.md)

> 非規範の原文固定・歴史的読者用スナップショット。原文を正本とし、機械入力や実行 gate として置き換えない。自動同期しない。記載されたテスト結果・件数・実装状態・証明の主張は原文の範囲に属し、ここで独立再実行したものでも現在の保証として確立したものでもない。 参照コンパイラとの比較は Almide 解析の証明ではない。安全でない暫定規則は記録された改訂で検出・修正されたが、本写しは以後の全改訂を独立監査するものではない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/native-read-borrow-review.md)

<a id="string-comparisons-2066"></a>

## String 比較（#2066）

旧コンパイラは借用レコードに対して `almide_eq!(t.kind.clone(), want)` を出力し、リテラル比較オペランド用に String を割り当てる。回帰テストはこの出力で失敗する。CloneInsertion は現在、等値・順序比較の安定した String オペランドを既存の string-borrow IR ノードで包む。通常の生存性走査は引き続き両方の読み取りを数える。既存 Borrow renderer がリテラルを str として出力し、フィールドを借用するので、walker のターゲット分岐も公開言語変更も不要である。

呼び出し、添字アクセス、ブロックは値を評価する経路を維持する。特に `t.kind == change(t)` は、右辺が `t` を変更するとき元のフィールド値を比較しなければならない。その呼び出しをまたいでフィールドを借用すると lifetime が変わってしまう。回帰検査は、フィールド・パラメータ・リテラルの等値・順序比較に加え、このケースと比較後の再利用を native と構造的 WASM で試す。

<a id="list-fields-in-loops-2068"></a>

## ループ内の List フィールド（#2068）

既存のループ証明は、単独の Var に対してだけ collection clone を省略していた。Member chain も現在は同じルート束縛を識別する。本体がそのルートへ書き込まなければ、ループはリストフィールドを借用する。続いて既存の借用要素証明が、すべての使用で要素を読むだけの場合に要素コピーを避ける。本体が親を置き換え、そのフィールドに書き込み、またはその place を可変借用するなら、collection snapshot を残す。テストは両分岐とその後の親の使用を native と構造的 WASM で試す。フィールドのルートが所有され、外側ループの外での最後の使用であり、本体が所有要素を必要とする場合は、consumed-loop 注釈が代わりに `into_iter()` を選ぶ。所有権の判断はパス内に留め、walker はその注釈を読むだけである。生きた借用 collection から要素を消費するには、依然として所有コピーが必要になる。回帰検査は最後の消費ループより前に親を借用する helper を呼び、native と構造的両方の出力を確認する。

<a id="reference-milestone"></a>

## 参照実装のマイルストーン

参照コンパイラは `../almide-references/rust` にコミット `0d31508599a7814a7044e9a7a871e3dc5f037753` で clone されている。`compiler/rustc_mir_build/src/builder/expr/as_rvalue.rs` の closure-capture lowering は、副作用がなく互いに重ならない place に限り、一時値ではなく place 構築を使う。この区別が本変更の参考になる。安定した place 読み取りは借用できる一方、place を無効化し得る評価は snapshot を保持する。所有者が変更されるとき、ループはコピーを残す。これは設計比較であり、Rust の実装が Almide の解析を証明するとの主張ではない。

検証は PR と実行可能な回帰検査 `tests/native_string_comparison_borrow_test.rs`、`tests/for_in_clone_elision_test.rs` に記録される。

<a id="validation-of-the-integrated-revision"></a>

## 統合リビジョンの検証

全 439 個の Almide テストファイルが native と既定経路（427 WASM、12 native fallback）で合格する。43 個の nanopass テスト、13 個の snapshot、11 個のループ所有権テスト、1 個の文字列比較行列が合格する。元の比較 fixture と読み取り専用フィールドループ fixture は修正前に失敗していた。借用親の負例は、安全でない暫定 move 規則を修正前に捉えた。Codopsy 2.2.0 は閾値変更なしで codegen 全体を 90/A と報告する。抽出した loop renderer と projection helper により、ファイルを 800 行未満に保つ。
