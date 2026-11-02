# 生成された Rust のレビュー (#572)

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](./GENERATED-RUST-REVIEW.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](./GENERATED-RUST-REVIEW.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](./GENERATED-RUST-REVIEW.zh-CN.md)

適格性確認済みコード生成器のモデル（SCADE KCG の先例）では、生成ソースはレビュー可能な成果物です。この適格性確認モデルは、Almide や現在のビルドが適格性確認済みであるという主張ではありません。本文は Almide の `--target rust` 出力のレビューガイドです。

## 対応関係マップ

```
almide app.almd --target rust --trace-map > app.rs
```

このコマンドは、出力される各関数の上に `// almd: fn <name> @ line <N>` を挿入し、`app.trace.json` を書き出します。内容は `{fn, almd_line, rust_line}` の行で、実際に出力したテキストを走査して取得します（マップと出力が食い違うことはありません。ゲートは `tests/trace_map_test.rs`）。アンカーは関数本体のソース行を指します。セルフホストされた標準ライブラリからリンクされた関数には、その標準ライブラリのソース行が記録されます。ファイルの識別には、関数名のドット区切り接頭辞に含まれる標準ライブラリのモジュール名を使います。既定の（フラグなしの）出力は、コミット済みの出力ベースラインとバイト単位で一致します。フラグを付けなければアンカーは現れません。

## レビューで対応を確認する点

1. **関数**がトレースマップの対応単位です。最適化後も残る `.almd` 関数は通常 Rust 関数として出力されます（単相化されたジェネリクスは、具体型を接尾辞に付け、インスタンス化ごとに1つの関数として出力されます）。インライン化、デッドコード除去、ヘルパー合成によって、1対1の対応は変わり得ます。ウォーカーは最終 IR を順に出力しますが、LICM やストリーム融合などの最適化はその前に作用します。この事実だけからソースの文の順序は推定できません。観測可能な順序は、各パスの契約と出力テキストに照らしてレビューしてください。
2. **安定したパターン**：エフェクトの結果は、推論・宣言された Result のエラーチャネルを使用し、常に String とは限りません。後置 `!` は囲む文脈に従います（[文脈表](../specs/result-option-effect.md)）。`==` は該当する場合、まず Secret／規約による等価性へ lowering されます。通常の `BinOp::Eq` は `almide_eq!` として出力されます（フロントエンドの `lower/expressions_access.rs`）。文字列・リストの `+` → `AlmideConcat`、`mut` パラメーター → `&mut`、バリアント → 同じケース名の Rust enum、レコード → 同じフィールド名の struct、という対応です。チェッカーが受理したものが出力され、コード生成が型を再推論することはありません（TypeMap が正本です）。
3. **警告なし**：生成された Rust は警告なしでコンパイルできなければなりません（常設のテスト規則）。また、run-manifest コーパス全体を、記録された Ferrocene 上流互換性の固定バージョンで毎週ビルドします（`ferrocene-lane.yml`、#573）。現在このレーンがインストールするのは標準の rustc であり、適格性確認済みの Ferrocene バイナリではありません。

## 認証の枠組み

提案されている適格性確認の分担では、ここでソースを生成し、その下に適格性確認済みコンパイラを置きます。現在の標準 rustc による互換性レーンは、その適格性確認を確立するものではありません。証拠と残る不足点は [TOOL-QUALIFICATION](../../proofs/TOOL-QUALIFICATION.md)（#574）にまとめています。
