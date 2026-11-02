<a id="hot-native-issues-2026-09-10"></a>

# Native の重点課題、2026-09-10

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](README.zh-CN.md)

> 固定原文の非規範的な履歴参照資料であり、原本を正本とする。自動同期しない。記載のテスト・測定・実装状態・保証は原文の主張であり、独立した再実行や現行の検証ではない。引用コードとコマンドは実行していない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/perf/hot-issues-2026-09-10/README.md)
> 過去の局所測定と検証件数を保持し、再実行していない。math 置換は明示的に出荷対象でない要因分析実験であり、表示 checksum の一致は一般的な bitwise equivalence を確立しない。

#2077–#2080 の設計と測定メモ。機械：Apple M3、macOS。コンパイラのベースは `14e667fdc` に本 PR の変更を加えたもの。研究結果は局所実験であり、普遍的な高速化の主張ではない。

<a id="decisions"></a>

## 決定

- **#2077：** `string.slice` は clamp した codepoint の意味論を維持するが、要求されていない suffix の走査を止める。prefix は引き続き走査する。新しい `string.byte_slice` は所有された `String?` を返す。負、逆順、範囲外、または UTF-8 境界でない offset は `none` を返す。有効な境界の検査は O(1)、コピーは O(window)。native は Rust の `str::get`、self-host は検査済み byte 境界と UTF-8 continuation bit を使う。結果が Option なので structural SUM-builder tier を通して受理し、layout certificate を迂回しない。
- **#2079：** 既存 BOM 除去後のソースの論理的先頭だけで `#!` を受理する。source position を保つため trivia として残し、dialect stamp と import より前に format する。OS で実行するには、依然として実行 bit、有効な interpreter path、選択した `env` 呼び出しをサポートする OS が必要。
- **#2080：** 既定 native build は、`emit` では scoped thread を示すにもかかわらず、MIR の逐次 WASM lowering を通して fan を受理していた。lowering が境界を消す前に fan block と fan.map / fan.any_map を拒否し、既存 native codegen に並行性を扱わせる。効果を持つあらゆる map callback が並列化できると主張するものでも、より広い #2044 作業を完了するものでもない。

参照ソースは `../almide-references/rust` の `0d31508599a7814a7044e9a7a871e3dc5f037753` から読んだ。`compiler/rustc_lexer/src/lib.rs`（`strip_shebang`）と `library/core/src/str`（境界検査付き slicing）である。Zig 参照は 0.15.2、commit `e4cbd752c8c05f131051f8c873cff7823177d7d3` に固定している。

<a id="native-fan-reproduction"></a>

## Native fan の再現

baseline と修正済みコンパイラで `fanblock.almd` を build し、各 binary をウォームアップして `/usr/bin/time -p` で測定する。baseline real/user は 0.52/0.51–0.52 s。修正後の warm real/user は 0.09/0.64 s。どちらも `fanblock 4389696` を出力する。修正後の最初の cold run は 0.60 s であり、warm 比較には混ぜない。これは小さな手動 probe であり、CI の安定した timing threshold ではない。routing と挙動には回帰テストがある。

<a id="2078-remains-open"></a>

## #2078 は未解決

`gramide.json` は、交互に行った 3 回の warm run、fixture/revision/compiler hash、一致する token 出力全体の hash を記録する。Go fixture は 2,973,586 bytes であり、issue の 3.95 MB fixture ではない。`gramide-compact-spans.patch` は gramide `c6933d2ef93b9c5890540b1b57910da9d90d8b73` に適用する実験であり、公開された client 変更ではない（`git apply --unidiff-zero` で適用）。各 variant を同じコンパイラで build し、`gramide check` を実行して、記録された Go ソース上の `gramide tokens` 出力を比較する。

| 変種 | real 中央値 | peak RSS 中央値 |
|---|---:|---:|
| 元の List[Int] source buffer | 0.49 s | 122.75 MiB |
| Bytes ソースバッファ | 0.49 s | 116.44 MiB |
| Bytes buffer と span 用 bytes.slice | 0.48 s | 103.09 MiB |

Bytes はすでにコンパクトな u8 要素を格納し、string 変換では一度コピーする。process 全体を 8 分の 1 にするわけではない。これら少数の sample は説得力ある実行時間の優位を示さず、RSS も sample 間で変動する。所有された token string と record の割り当てには、引き続き profiling と表現の検討が必要である。文書は zero-copy 変換と主張せず、コピーを正しく記述するようになった。

<a id="zig-arena-attribution-not-a-semantic-change"></a>

## Zig arena の要因分析であり、意味論の変更ではない

`math-attribution.json` には、大きな各サイズについて順序をシャッフルし交互に行った 7 回の run、seed 42、サイズ 10、23、24 で一致する表示 checksum がある。入力は `../listbuild/listbuild_prealloc.almd`。既定 build、`almide emit` + rustc、libm sin/cos wrapper だけを `x.sin()` / `x.cos()` に変えた実験的な生成ソース variant を比較する。Rust flags：`--edition 2021 -C opt-level=3
-C lto=yes -C codegen-units=1 -C overflow-checks=no`。

サイズ 24 の中央値は順に 0.40986、0.41457、0.21267 s。native producer を変えても影響は小さく、math 実装がこの workload の費用の大部分を説明する。これらは要因分析の run であり、Zig との新しい交互比較ではない。表示 checksum は全入力の bitwise equivalence を確立しない。`runtime/rs/src/math.rs` は cross-platform determinism のため意図的に vendored musl libm を使う。platform-math 置換を出荷してはならない。後続では arena の優位を主張する前に、決定的な sin/cos（例えば range reduction の共有）を最適化し、数値と cross-target の証拠を示す必要がある。

<a id="validation"></a>

## 検証

- `tests/hot_native_regressions_test.rs`：先頭限定 shebang、position、formatting、native concurrency routing。`tests/fan_surface_matrix_test.rs`。
- `spec/cli/shebang.almd`：check、compile、test、format、直接 OS 実行。
- `spec/wasm_cross/string_byte_slice.almd`：C-348 Unicode byte boundary、不正範囲、空範囲、既存 codepoint slicing を native と WASM で確認。
- 全体 sweep とネットワーク有効の 9 回の再試行を通じ、native test file 440 件に合格。既定 sweep とネットワーク有効の 2 回の再試行も全 440 file に合格。
- release allocation ledger と structural witness floor が合格。既存 allocation 行は不変で、新しい fixture 行が 1 つ加わる。docs generation check が合格。
- Codopsy は codegen と WASM の両方で A（90/100）を保持し、しきい値は既存の 20/30。

<a id="ci-integration-follow-up"></a>

## CI 統合の後続対応

新 fixture は明示的な Option match を使うため、incumbent MIR host harness も全ケースを compile する。期待結果と不正境界ケースは不変。native、wasm32-wasip1、browser ABI はこの fixture に対して同一の 17,620-byte incumbent module を出力し、新しい拒否 fixture はない。structural emitted / WASI size は 5,098 / 5,608 bytes。size baseline へ追加するのはこの fixture の行だけで、既存行と refusal ceiling は不変。新 API は pure として exercised surface、IDE snapshot、contract table に登録される。native concurrency の拒否は source span を持つようになり、回帰テストと spanless-wall ratchet で検査する。
