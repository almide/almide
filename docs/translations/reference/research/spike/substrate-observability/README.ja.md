<a id="substrate-observability--the-c-004-exception-reproduced"></a>

# 実行基盤の観測可能性 — C-004 EXCEPTION の再現

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](README.zh-CN.md)

> 非規範の原文固定・歴史的読者用スナップショット。原文を正本とし、機械入力や実行 gate として置き換えない。自動同期しない。記載されたテスト結果・件数・実装状態・証明の主張は原文の範囲に属し、ここで独立再実行したものでも現在の保証として確立したものでもない。 2026-08-07 の測定と Rung 1 の将来手順は歴史的記録のまま保持する。本写しは手順を実行せず、旧 fan 時間 API が現行だとも主張しない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/spike/substrate-observability/README.md)

[ADR-0011](../../../../../adr/0011-execution-substrate-is-a-free-variable.md) と [execution-inception.md](../../../../../roadmap/active/execution-inception.md) §1 の証拠。

```
research/spike/substrate-observability/run-repro.sh [runs]
```

<a id="what-it-shows"></a>

## 何を示すか

出力を行う `fan {}` arm は、実行基盤が観測へ届く唯一の表面である。台帳はこれをターゲット間の不一致として記録する（C-004 の EXCEPTION：「native では実時間、wasm では逐次」）。測定はこの分類の誤りを示す。これは **native ⇄ native** の不一致であり、0.29.0 で `fan.timeout` が削除された原因の性質、そして C-006 が「その性質に違反する唯一の stdlib 表面」と呼ぶものである。

2026-08-07 測定（almide 0.56.0、wasmtime 47.0.2、14-core arm64 macOS）：

| 実行基盤 | 実行数 | 異なる arm 順序の数 |
|---|---|---|
| native（スコープ付き OS スレッド） | 10 | **8–9** |
| wasm（逐次 inline） | 3 | 1 (`ABCD`) |
| 参照インタープリタ | — | 構成上 1 (`eval.rs:148`) |

3 つのオラクルのうち 2 つは、[#1000](../../../../../roadmap/active/concurrency-stance.md) が定義するリスト順の逐次観測と一致する。native が外れ値である。

<a id="notes-for-whoever-re-runs-this"></a>

## 再実行する人への注意

- **処理を定数畳み込みできないようにする必要がある。** 先行の試行では spin loop にリテラルを渡したため、LLVM がコンパイル時に評価した。各 arm が瞬時に終わり、両ターゲットのきれいな `A B C D` の背後に不一致が隠れた。このため seed は `env.args()` から取る。
- **1 回の実行では何も証明できない。** セッション最初の native 実行はしばしば arm 順に出力する。欠陥は実行を重ねた場合にしか見えない。
- **`env.sleep_ms` は並列性をより直接的に示す**（4 arm すべてが同じ開始・終了ミリ秒を報告し、合計 ≈ 1 arm の所要時間）。ただし wasm レッグでは capability 欠落で拒否されるため、比較のターゲット間部分には使えない。

<a id="when-rung-1-lands"></a>

## Rung 1 が着地したとき

arm ごとの出力 transaction（arm 別に buffer し、join で arm 順に flush）により、どの基盤も `ABCD` を出力する。その時点でスクリプトの終了コードの意味が反転する。gate に転換して `research/spike/` の外へ移し、同じ PR で C-004 の EXCEPTION 条項を削除する。
