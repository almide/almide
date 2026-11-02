<a id="decoder-scratch-ownership-2046"></a>

# Decoder の一時作業領域の所有権（#2046）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](wasm-decoder-reclaim-review.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](wasm-decoder-reclaim-review.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](wasm-decoder-reclaim-review.zh-CN.md)

> 非規範の原文固定・歴史的読者用スナップショット。原文を正本とし、機械入力や実行 gate として置き換えない。自動同期しない。記載されたテスト結果・件数・実装状態・証明の主張は原文の範囲に属し、ここで独立再実行したものでも現在の保証として確立したものでもない。 限定された測定ケースと維持した拒否集合は、あらゆるリークの不存在や普遍的なメモリ安全性を証明しない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/wasm-decoder-reclaim-review.md)

構造的 WASM decoder は `Try`/`Unwrap` が取り出す一時 Result wrapper をリークしていた。2 フィールドのレコード decode は 1 呼び出しごとに 128 バイトを保持し、heap watermark は 1000 回で 196544 バイトから、8000 回で 1092544 へ増えた。

参照コンパイラは `../almide-references/koka` にコミット `3ac4f001ab7277b484d661fdbada1aaf8d01ecbf` で clone した Koka である。`src/Backend/C/Parc.hs` の `parcBorrowArg` は生成された借用引数に名前を付け、消費後の drop を配置して評価順を保持する。Almide には対応する一時束縛パスが既にある。取り出し処理も現在はこのパスに参加する。wrapper は通常のローカル所有者を持ち、既存の exit plan が成功時または伝播時に解放する。payload は view のままで、保持または返却する場合は既存経路で別の credit を取得する。

末尾での取り出しは既存の carrier-transfer 経路を維持するため、一時値に名前を付けても末尾呼び出しを隠さない。move-mode effect lowering が導入する raw payload 注釈は、その ABI の取り出し経路に任せる。回帰検査は 200000 回の末尾呼び出しと可変引数の書き戻しも確認する。callback の末尾とその `ok(unwrap(call))` carrier wrapper も形を保ち、fan の prefetch と最初のエラーの転送経路を維持する。

エラー経路の測定で、さらに 2 つのリークが見つかった。借用した成功値と新規構築したエラーを並べて返す match は、関数 return 時に新規エラーへ不要な参照を与えていた。値 match の各 arm は現在、join に 1 つの所有 credit を渡す。借用 arm はそれを取得し、新規 arm は移譲する。欠落フィールドのエラー構築でも、中間の連結済み prefix が割り当てられたまま残っていた。現在は最終連結でコピーした後に、その文字列を解放する。

回帰検査は、実際に export された heap watermark を 1000 回と 8000 回で比較し、出力を確認する。平坦なレコード、入れ子レコードのリスト、フィールドの正常 decode 後の型エラー、欠落フィールドを対象とする。4 ケースとも横ばいを保つ。既存の 22 個の call-result 所有権テストと exit 検証も合格する。Codopsy は既存の複雑度閾値（20/30）のまま WASM ソースディレクトリを 91/A と報告する。

割り当て台帳の再生成では、コーパスの watermark が 79 件減少し、増加はなく、既存の構造的拒否集合も保持する。native-result 所有権と構造的 witness floor はともにベースラインを緩めず合格する。元の 100000 回呼び出し再現例は、以前はメモリを使い切ったのと同じ 4 MiB ヒープ上限で完了する。
