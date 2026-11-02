<a id="explicit-fan-routing-safety-review"></a>

# 明示的な fan ルーティングの安全性レビュー

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](fan-parallel-safety-review.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](fan-parallel-safety-review.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](fan-parallel-safety-review.zh-CN.md)

> 固定原文の非規範的な履歴参照資料であり、原本を正本とする。自動同期しない。記載のテスト・測定・実装状態・保証は原文の主張であり、独立した再実行や現行の検証ではない。引用コードとコマンドは実行していない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/fan-parallel-safety-review.md)

#2055 の統合は StreamFusion と DecodeErrFrame を保持し、廃止された AutoParallel パスを除去する。パイプラインの競合を解消後、codegen の単体テストと統合テストは合格する。

<a id="plain-functions-can-contain-effects"></a>

## 通常関数にも効果が含まれ得る

次のプログラムはチェックに成功するが、元のルーティングパスは `almide_rt_fan_map_par` を出力し、要素の出力順が変わり得る。

```almide
fn observe(n: Int) -> Int = {
  println(int.to_string(n))
  n
}
effect fn main() -> Unit = {
  let values = fan.map([1, 2, 3], (n) => ok(observe(n)))!
  println(int.to_string(list.len(values)))
}
```

宣言フラグ `is_effect` だけでは純粋性を確立できない。修正後のルーティングは、自身の入力に対して StreamFusion と同じ保守的で推移的な純粋性解析を再計算する。融合が実行されたかどうかには依存しない。既存の可変キャプチャと Send-safe 型のチェックは引き続き適用される。未知の呼び出しや不透明な Rust から純粋性を確立することはできない。`tests/nanopass_test.rs` の回帰テストは、通常のヘルパーが random ランタイムに到達する場合を対象にする。

これは `docs/stream-fusion-safety-review.md` の評価順に関する議論に従う。そこには、効果を保存するため変数でない対象を一度だけ束縛する、固定版 Gleam のソース（`19bf207ebb7d953ea1391f041da48c214ee1440a`）も含まれる。参照資料は `../almide-references` に clone されている。純粋性解析は正しさを確保するための受理条件であり、コールバック引数の型から推論するものではない。

<a id="invocation-local-mutation-and-integration"></a>

## 呼び出し内に局所的な変更と統合

StreamFusion は明示的な fan ブロック直下の操作を保存する一方、コールバック内部と暗黙的な list パイプラインは引き続き融合する。これにより、後段の fan ルーティングパスが並列 list 呼び出しを利用できる状態を保つ。

Fan の証明は、解析専用の IR ビューを通して局所的な命令型ヘルパーの本体を認める。この扱いを受けるのはスカラー引数を持つ閉じた関数だけである。ヒープ引数は呼び出し元の記憶領域を別名参照し得るため、厳密な証明を維持する。呼び出し内の局所的な束縛への書き込みを、そのオペランド式で置き換え、ループ本体と条件は保持する。それらの式内の呼び出しも、引き続き推移的な効果チェックに合格しなければならない。引数への書き込みや外側の状態へのアクセスは受理されない。変換したビューは一切出力せず、全域性を確立することもない。StreamFusion は元の証明と停止性の要件を保持する。回帰テストでは、ループ内で消去される局所代入の RHS に random 呼び出しを置き、その関数が引き続き拒否されることを検証する。

この所有権境界は、固定版 Rust 参照 `0d31508599a7814a7044e9a7a871e3dc5f037753` の `library/std/src/thread/scoped.rs` と整合する。scoped spawning にも `F: Send` が必要であり、スコープのライフタイムはキャプチャした所有権を複製しない。Fan は純粋性に加えて、既存の Send と可変キャプチャのチェックを維持する。

<a id="remeasured-parallel-execution"></a>

## 並列実行の再測定

修復したコンパイラは fannkuchredux のユーザー関数に `almide_rt_list_par_map` を出力する。n=10 で、並列、`ALMIDE_FAN_SEQUENTIAL=1`、手書き Rust 参照はすべて `73196` と `Pfannkuchen(10) = 38` を出力する。
1 回のウォームアップ後、`almide build --release` とベンチマークハーネスの標準 Rust フラグを使って交互に 7 回測定した中央値は、この開発機で並列 54.16 ms、逐次 179.27 ms、Rust 172.47 ms である。native/Rust 比は 0.314、逐次/並列比は 3.310。
これらは局所的な検証結果であり、CI runner の性能ゲートの代わりでも、以前のベンチマーク版に関する主張でもない。
生の測定値：`research/benchmark/perf/results/2026-09-09-fan-repair.json`。

効果を持つ通常ヘルパーの再現例は逐次実行のままで、native と structural WASM 上で 1, 2, 3, 3 を出力する。Codopsy 2.2.0 は、しきい値を変えずにルーティングと解析のファイルを A と評価する。
