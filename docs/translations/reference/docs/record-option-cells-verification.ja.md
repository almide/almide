<a id="option-record-literals-2057-verification-2026-09-09"></a>

# Option レコードリテラル：#2057 の検証（2026-09-09）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](record-option-cells-verification.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](record-option-cells-verification.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](record-option-cells-verification.zh-CN.md)

> 非規範の原文固定・歴史的読者用スナップショット。原文を正本とし、機械入力や実行 gate として置き換えない。自動同期しない。記載されたテスト結果・件数・実装状態・証明の主張は原文の範囲に属し、ここで独立再実行したものでも現在の保証として確立したものでもない。 既存系で明示的に未対応のケースと 27→28 の上限変更を保持し、それらのホストがフィールド行列全体をサポートすると推定しない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/record-option-cells-verification.md)

develop `cbf04dbc3` では、報告された直接の `none` リテラルを構造的バックエンドが既に受け入れる。元の再現例は検証付き構造的 WASM 成果物（2292 バイト）をビルドし、native と同じ `0` を出力する。emitter の変更は不要だった。`crates/almide-wasm/src/data.rs::lower_sum` は期待される Option の形状を受け取り、レコード emitter は宣言された各フィールド型を `lower` に渡す。

新しい C-255 fixture は、Result を返す関数内で、Int、Float、Bool、String、List[Int]、名前付きレコードの Option フィールドについて値のない場合・ある場合を試す。native と構造的 WASM はともに `none\nsome\n` を出力し、interpreter run コーパスも同じ出力を固定する。Codec のフィールド行列テストは再び、null 入力と欠落入力の両方について、デコードしたレコード全体を `none` フィールドを持つリテラルと比較する。

割り当て台帳と 2 種のサイズ台帳に追加したのは、新 fixture の測定行だけである。heap watermark は 69984、出力サイズは 14022、出荷 WASI サイズは 14556 バイト。既存行は変更していない。AST／診断 manifest にも新 fixture と、変更した Codec fixture の AST hash を含める。契約の証拠リンクは対称である。

<a id="incumbent-determinism-domain"></a>

## 既存系の決定性の対象領域

ホスト CI 実行 34321896641 は比較対象の全 fixture で同じバイト列を出力するが、新しい `record_option_none_cells.almd` fixture は既存の両ホストで拒否される。これにより、それらの未対応入力数は 27 から 28 へ増える。ホストとブラウザの決定性上限は、既存の構造的経路専用 fixture 方針に従い、この具体的理由で更新する。これは既存系がこのフィールド行列をサポートするとの主張ではない。実行するのは native と現在の構造的経路であり、割り当て・サイズ台帳はその構造的バイト列を対象とする。従来比較していた fixture は一つも除外していない。
